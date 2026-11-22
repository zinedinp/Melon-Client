//! The integrated server: Pumpkin running inside Melon's own process, joined

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::num::NonZero;
use std::path::PathBuf;
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, anyhow};
use pumpkin::PumpkinServer;
use pumpkin::data::VanillaData;
use pumpkin::net::PacketRateLimiter;
use pumpkin::net::java::pending::PendingConnection;
use pumpkin::plugin::loader::PluginLoader;
use pumpkin::serve_java_connection;
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_util::world_seed::Seed;
use pumpkin_util::{Difficulty, GameMode, PermissionLvl};
use pumpkin_wasm_host::WasmPluginLoader;
use tokio::io::{ReadHalf, SimplexStream, WriteHalf};
use tokio::runtime::{Builder, Handle, Runtime};
use tokio_util::io::SyncIoBridge;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::singleplayer;

/// The protocol the integrated server speaks. Must be the one
pub const PROTOCOL: i32 = pumpkin_data::packet::CURRENT_MC_VERSION.protocol_version();

/// What the server reports as the player's address. Nothing dials it.
const CLIENT_ADDRESS: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

/// Room for a chunk batch in flight toward the client, and far more than the
const TO_CLIENT_BYTES: usize = 1024 * 1024;
const TO_SERVER_BYTES: usize = 256 * 1024;

/// Everything the integrated server needs to open one world.
pub struct Options {
    /// The world's folder, `saves/<folder>/`. Pumpkin keeps `level.dat` and
    pub save_path: PathBuf,
    /// Instance folder: `mods/`, `plugins/`, `saves/`.
    pub game_dir: PathBuf,
    /// Game mode, difficulty, cheats, and the seed a new world is generated
    pub settings: singleplayer::WorldSettings,
    /// The most chunks the server streams around a player; within it, the
    pub view_distance: u8,
    /// Chunk radius the server ticks (`Options.simulationDistance`). Read
    pub simulation_distance: u8,
}

/// A running world. Dropping it saves the world and stops the server,
pub struct IntegratedServer {
    cancel: CancellationToken,
    thread: Option<JoinHandle<()>>,
    /// Pumpkin's server, for its tick time; gone once it stopped.
    pumpkin: std::sync::Weak<pumpkin::server::Server>,
    /// The world's folder.
    save_path: PathBuf,
}

impl IntegratedServer {
    /// The world's folder, `saves/<folder>/`.
    pub fn save_path(&self) -> &std::path::Path {
        &self.save_path
    }

    /// Pumpkin's average time per tick over the last 100 ticks, in ms.
    pub fn mspt(&self) -> Option<f64> {
        self.pumpkin.upgrade().map(|server| server.get_mspt())
    }

    /// Pumpkin's world of `dimension` (e.g. `minecraft:overworld`), for the
    /// F3 entries vanilla reads off its integrated server.
    pub fn world(&self, dimension: &str) -> Option<Arc<pumpkin::world::World>> {
        let server = self.pumpkin.upgrade()?;
        let worlds = server.worlds.load();
        worlds
            .iter()
            .find(|w| w.dimension.minecraft_name == dimension)
            .cloned()
    }

    /// Whether the server has stopped, by itself (`/stop`) or after a close.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for IntegratedServer {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            crate::warn!("The integrated server crashed");
        }
    }
}

/// The client's end of the pipe.
pub struct ClientPipe {
    rx: SyncIoBridge<ReadHalf<SimplexStream>>,
    tx: SyncIoBridge<WriteHalf<SimplexStream>>,
}

impl crate::network::Stream for ClientPipe {
    fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        Ok((Box::new(self.rx), Box::new(self.tx)))
    }
}

impl Read for ClientPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.rx.read(buf)
    }
}

impl Write for ClientPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tx.flush()
    }
}

/// Opens the world and returns once the server is up and waiting for the
pub fn start(options: Options) -> Result<(IntegratedServer, ClientPipe)> {
    crate::instances::ensure_dirs(&options.game_dir);
    pumpkin::data::set_data_dir(crate::home::dir().join("cache").join("server"));
    if let Err(error) = prepare_new_world(&options) {
        crate::warn!("World data packs not prepared: {error}");
    }
    let (client_rx, server_tx) = tokio::io::simplex(TO_CLIENT_BYTES);
    let (server_rx, client_tx) = tokio::io::simplex(TO_SERVER_BYTES);

    let save_path = options.save_path.clone();
    let cancel = CancellationToken::new();
    let (started, ready) = sync_channel::<Started>(1);
    let thread = thread::Builder::new()
        .name("integrated-server".to_owned())
        .spawn({
            let cancel = cancel.clone();
            move || {
                let runtime = match runtime() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = started.send(Err(error));
                        return;
                    }
                };
                runtime.block_on(serve(
                    options,
                    Box::new(server_rx),
                    Box::new(server_tx),
                    &cancel,
                    &started,
                ));
            }
        })
        .context("spawning the integrated server thread")?;
    let mut server = IntegratedServer {
        cancel,
        thread: Some(thread),
        pumpkin: std::sync::Weak::new(),
        save_path,
    };

    let (handle, pumpkin) = ready
        .recv()
        .map_err(|_| anyhow!("the integrated server stopped while starting"))?
        .map_err(|error| anyhow!(error))?;
    server.pumpkin = pumpkin;
    let pipe = ClientPipe {
        rx: SyncIoBridge::new_with_handle(client_rx, handle.clone()),
        tx: SyncIoBridge::new_with_handle(client_tx, handle),
    };
    Ok((server, pipe))
}

/// Pumpkin's worker threads: half the machine. The client needs the rest for
fn runtime() -> Result<Runtime, String> {
    let available = thread::available_parallelism().map_or(2, NonZero::get);
    Builder::new_multi_thread()
        .worker_threads((available / 2).max(1))
        .thread_name("server-worker")
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start the server runtime: {error}"))
}

/// What the server thread reports once it is up.
type Started = Result<(Handle, std::sync::Weak<pumpkin::server::Server>), String>;

async fn serve(
    options: Options,
    read: pumpkin::net::java::JavaReadHalf,
    write: pumpkin::net::java::JavaWriteHalf,
    cancel: &CancellationToken,
    started: &std::sync::mpsc::SyncSender<Started>,
) {
    let opened = PumpkinServer::new(
        basic_config(&options),
        advanced_config(&options),
        TelemetryConfig {
            enabled: false,
            ..TelemetryConfig::default()
        },
        vanilla_data(),
        plugin_loaders(),
        false,
    )
    .await;
    let pumpkin = match opened {
        Ok(pumpkin) => pumpkin,
        Err(error) => {
            let _ = started.send(Err(format!("failed to open the world: {error}")));
            return;
        }
    };
    pumpkin.init_plugins().await;
    let server = Arc::clone(&pumpkin.server);

    let tasks = TaskTracker::new();
    let pending = PendingConnection::from_transport(
        read,
        write,
        CLIENT_ADDRESS,
        0,
        PacketRateLimiter::from_config(&server.advanced_config.networking.java.packet_limiter),
        Arc::downgrade(&server),
    );
    tasks.spawn(serve_java_connection(Arc::clone(&server), pending));

    let _ = started.send(Ok((Handle::current(), Arc::downgrade(&server))));

    tokio::select! {
        () = cancel.cancelled() => {},
        () = server.stop_token.cancelled() => {},
    }
    server.stop();
    pumpkin.shutdown(&tasks).await;
}

/// A new world with data packs or experiments gets its `level.dat` before
fn prepare_new_world(options: &Options) -> std::io::Result<()> {
    let packs = &options.settings.data_packs;
    if packs.is_empty() || options.save_path.join("level.dat").exists() {
        return Ok(());
    }
    singleplayer::data_packs::prepare_level(&options.save_path, world_seed(options), packs)
}

/// The seed a new world is generated with.
fn world_seed(options: &Options) -> i64 {
    singleplayer::parse_seed(&options.settings.seed).unwrap_or_else(rand::random)
}

fn is_hardcore(options: &Options) -> bool {
    options.settings.game_mode == singleplayer::WorldCreationMode::Hardcore
}

fn basic_config(options: &Options) -> BasicConfiguration {
    let hardcore = is_hardcore(options);
    BasicConfiguration {
        seed: Seed(
            singleplayer::parse_seed(&options.settings.seed).unwrap_or_else(rand::random) as u64,
        ),
        default_difficulty: if hardcore {
            Difficulty::Hard
        } else {
            match options.settings.difficulty {
                singleplayer::Difficulty::Peaceful => Difficulty::Peaceful,
                singleplayer::Difficulty::Easy => Difficulty::Easy,
                singleplayer::Difficulty::Normal => Difficulty::Normal,
                singleplayer::Difficulty::Hard => Difficulty::Hard,
            }
        },
        hardcore,
        default_gamemode: match options.settings.game_mode {
            singleplayer::WorldCreationMode::Creative => GameMode::Creative,
            singleplayer::WorldCreationMode::Adventure => GameMode::Adventure,
            singleplayer::WorldCreationMode::Survival
            | singleplayer::WorldCreationMode::Hardcore => GameMode::Survival,
        },
        default_level_name: options.save_path.to_string_lossy().into_owned(),
        spawn_protection: 0,
        white_list: false,
        use_favicon: false,
        allow_chat_reports: false,
        accepts_transfers: false,
        ..BasicConfiguration::default()
    }
}

fn advanced_config(options: &Options) -> AdvancedConfiguration {
    let mut config = AdvancedConfiguration::default();
    let networking = &mut config.networking;

    networking.java.enabled = false;
    networking.bedrock.enabled = false;
    networking.management.enabled = false;
    networking.query.enabled = false;
    networking.lan_broadcast.enabled = false;
    networking.java.online_mode = false;
    networking.java.encryption = false;
    networking.java.compression.enabled = false;
    networking.java.max_players = 1;
    let distance = |chunks: u8| NonZero::new(chunks.max(2)).unwrap_or(NonZero::<u8>::MIN);
    networking.java.view_distance = distance(options.view_distance);
    networking.java.simulation_distance = distance(options.simulation_distance);

    config.commands.use_console = false;
    config.commands.use_tty = false;
    config.commands.default_op_level = if options.settings.allow_cheats && !is_hardcore(options) {
        PermissionLvl::Four
    } else {
        PermissionLvl::Zero
    };
    config.plugins.enabled = true;
    config.plugins.hot_reload = false;
    let mods = options.game_dir.join("mods");
    let plugins = options.game_dir.join("plugins");
    config.mods.enabled = true;
    config.mods.dir = mods.to_string_lossy().into_owned();
    config.mods.compile_source = true;
    config.plugins.dir = plugins.to_string_lossy().into_owned();
    config
}

/// The standalone server's loaders: WASM plugins only.
fn plugin_loaders() -> Vec<Arc<dyn PluginLoader>> {
    let verify = AdvancedConfiguration::default().plugins.verify_signatures;
    vec![Arc::new(WasmPluginLoader::new(verify))]
}

/// Pumpkin's ban, whitelist and operator lists, kept in memory. Its own
fn vanilla_data() -> VanillaData {
    VanillaData {
        banned_ip_list: RwLock::default(),
        banned_player_list: RwLock::default(),
        operator_config: RwLock::default(),
        user_cache: RwLock::default(),
        whitelist_config: RwLock::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(game_mode: singleplayer::WorldCreationMode) -> Options {
        Options {
            save_path: PathBuf::from("saves/world"),
            game_dir: PathBuf::from("."),
            settings: singleplayer::WorldSettings {
                name: "World".to_owned(),
                game_mode,
                seed: "42".to_owned(),
                difficulty: singleplayer::Difficulty::Easy,
                allow_cheats: true,
                ..Default::default()
            },
            view_distance: 4,
            simulation_distance: 4,
        }
    }

    #[test]
    fn speaks_the_clients_protocol() {
        assert_eq!(PROTOCOL, crate::network::PROTOCOL_VERSION);
    }

    #[test]
    fn world_settings_reach_the_pumpkin_config() {
        let options = options(singleplayer::WorldCreationMode::Creative);
        let basic = basic_config(&options);
        assert_eq!(basic.default_gamemode, GameMode::Creative);
        assert_eq!(basic.seed.0, 42);
        assert_eq!(basic.default_difficulty, Difficulty::Easy);
        assert_eq!(basic.default_level_name, "saves/world");
        assert_eq!(basic.spawn_protection, 0);
        assert!(!basic.hardcore);

        let advanced = advanced_config(&options);
        assert!(!advanced.networking.java.enabled);
        assert!(!advanced.networking.java.compression.enabled);
        assert_eq!(advanced.commands.default_op_level, PermissionLvl::Four);
        assert!(advanced.mods.enabled);
        assert!(advanced.mods.dir.ends_with("mods"));
        assert!(advanced.plugins.dir.ends_with("plugins"));
    }

    #[test]
    fn hardcore_is_hard_survival_without_cheats() {
        let options = options(singleplayer::WorldCreationMode::Hardcore);
        let basic = basic_config(&options);
        assert!(basic.hardcore);
        assert_eq!(basic.default_gamemode, GameMode::Survival);
        assert_eq!(basic.default_difficulty, Difficulty::Hard);
        assert_eq!(
            advanced_config(&options).commands.default_op_level,
            PermissionLvl::Zero
        );
    }
}
