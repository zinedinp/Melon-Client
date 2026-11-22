//! The integrated server.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::num::NonZero;
use std::path::PathBuf;
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, anyhow};
use arc_swap::ArcSwap;
use pumpkin::PumpkinServer;
use pumpkin::data::VanillaData;
use pumpkin::net::java::pending::PendingConnection;
use pumpkin::net::lan_broadcast::LANBroadcast;
use pumpkin::net::{DisconnectReason, GameProfile, PacketRateLimiter};
use pumpkin::plugin::loader::PluginLoader;
use pumpkin::serve_java_connection;
use pumpkin::server::Server;
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_data::translation::{bedrock, java};
use pumpkin_util::text::TextComponent;
use pumpkin_util::threads::{self, ThreadRole};
use pumpkin_util::world_seed::Seed;
use pumpkin_util::{Difficulty, GameMode, PermissionLvl};
use pumpkin_wasm_host::WasmPluginLoader;
use tokio::io::{ReadHalf, SimplexStream, WriteHalf};
use tokio::runtime::{Builder, Handle, Runtime};
use tokio_util::io::SyncIoBridge;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::singleplayer;

/// The protocol the integrated server speaks.
pub const PROTOCOL: i32 = pumpkin_data::packet::CURRENT_MC_VERSION.protocol_version();

/// What the server reports as the player's address. Nothing dials it.
const CLIENT_ADDRESS: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

/// Room for a chunk batch in flight toward the client.
const TO_CLIENT_BYTES: usize = 1024 * 1024;
const TO_SERVER_BYTES: usize = 256 * 1024;

/// `IntegratedServer.getMaxPlayers`.
const MAX_PLAYERS: u32 = 8;

/// The account the world is played with: the server's owner.
#[derive(Clone, Debug)]
pub struct Host {
    pub uuid: uuid::Uuid,
    pub name: String,
    /// Signed in: its skin is looked up for the guests to see.
    pub online: bool,
}

/// How the world is opened to others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Publish {
    pub port: u16,
    /// Guests are verified with Mojang (Java) or Xbox (Bedrock).
    pub online_mode: bool,
    /// Bedrock clients may join too, at the `NetherNet` address (UDP/TCP 19132).
    pub bedrock: bool,
}

/// Everything the integrated server needs to open one world.
pub struct Options {
    /// The world's folder, `saves/<folder>/`.
    pub save_path: PathBuf,
    /// Instance folder: `mods/`, `plugins/`, `saves/`.
    pub game_dir: PathBuf,
    /// Game mode, difficulty, cheats, and the seed a new world is generated from.
    pub settings: singleplayer::WorldSettings,
    /// The most chunks the server streams around a player.
    pub view_distance: u8,
    /// Chunk radius the server ticks (`Options.simulationDistance`).
    pub simulation_distance: u8,
    /// Who plays it.
    pub host: Host,
}

/// A running world.
pub struct IntegratedServer {
    cancel: CancellationToken,
    thread: Option<JoinHandle<()>>,
    /// Pumpkin's server, for its tick time; gone once it stopped.
    pumpkin: std::sync::Weak<pumpkin::server::Server>,
    /// The world's folder.
    save_path: PathBuf,
    /// The server's runtime, for its listeners.
    handle: Option<Handle>,
    /// Every connection's task, which the shutdown waits for.
    tasks: TaskTracker,
    host: Host,
    /// While open to others.
    published: Option<Published>,
}

struct Published {
    publish: Publish,
    /// Ends the listeners and the LAN broadcast.
    stop: CancellationToken,
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

    /// The host's game is paused (`Minecraft.isPaused`): the server stands
    /// still too, but never while it is open to others.
    pub fn set_paused(&self, paused: bool) {
        if let Some(server) = self.pumpkin.upgrade() {
            server.host.set_paused(paused && self.published.is_none());
        }
    }

    /// How the world is open to others, while it is.
    pub fn published(&self) -> Option<Publish> {
        self.published.as_ref().map(|p| p.publish)
    }

    /// `IntegratedServer.publishServer`: listens on `publish.port` for Java
    /// clients (and on the `NetherNet` address for Bedrock ones), and says so
    /// on the LAN. The game no longer pauses after this.
    pub fn publish(&mut self, publish: Publish) -> Result<()> {
        if self.published.is_some() {
            return Err(anyhow!("already published"));
        }
        let server = self
            .pumpkin
            .upgrade()
            .ok_or_else(|| anyhow!("the integrated server stopped"))?;
        let handle = self
            .handle
            .clone()
            .ok_or_else(|| anyhow!("the integrated server is not running"))?;
        server.host.set_online_mode(publish.online_mode);
        let stop = self.cancel.child_token();
        let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, publish.port));
        let listener = handle
            .block_on(tokio::net::TcpListener::bind(address))
            .with_context(|| format!("listening on port {}", publish.port))?;
        if publish.bedrock {
            handle
                .block_on(pumpkin::listen_bedrock(
                    Arc::clone(&server),
                    stop.clone(),
                    Arc::new(self.tasks.clone()),
                ))
                .context("listening for Bedrock")?;
        }
        handle.spawn(accept_java(
            Arc::clone(&server),
            listener,
            stop.clone(),
            self.tasks.clone(),
        ));
        let motd = server.advanced_config.networking.java.motd.clone();
        let broadcast = LANBroadcast::new(&server.advanced_config.networking.lan_broadcast, &motd);
        let broadcast_stop = stop.clone();
        let broadcast_server = Arc::clone(&server);
        handle.spawn(async move {
            tokio::select! {
                () = broadcast_stop.cancelled() => {}
                () = broadcast.start(broadcast_server, address) => {}
            }
        });
        server.host.set_published(true);
        server.enforce_game_mode();
        crate::info!("Published LAN server on port {}", publish.port);
        self.published = Some(Published { publish, stop });
        Ok(())
    }

    /// `IntegratedServer.unpublishServer`: the listeners close and the guests
    /// are sent off. Whether it was published.
    pub fn unpublish(&mut self) -> bool {
        let Some(published) = self.published.take() else {
            return false;
        };
        crate::info!(
            "Unpublishing integrated server (was on port {})",
            published.publish.port
        );
        published.stop.cancel();
        if let Some(server) = self.pumpkin.upgrade() {
            server.host.set_published(false);
            // The macro needs Pumpkin's own crate paths; the keys are its constants.
            #[expect(deprecated)]
            let reason = TextComponent::translate_cross(
                java::MULTIPLAYER_DISCONNECT_SERVER_SHUTDOWN,
                bedrock::DISCONNECTIONSCREEN_DISCONNECTED,
                [],
            );
            for player in server.get_all_players() {
                if !server.host.is_owner(&player.gameprofile.name) {
                    player.kick(DisconnectReason::Shutdown, &reason);
                }
            }
        }
        true
    }

    /// The world's "Allow Commands".
    pub fn allow_commands(&self) -> bool {
        self.pumpkin
            .upgrade()
            .is_some_and(|server| server.host.allow_commands())
    }

    /// `setWorldAllowCommands`.
    pub fn set_allow_commands(&self, allow: bool) {
        if let Some(server) = self.pumpkin.upgrade() {
            server.host.set_allow_commands(allow);
            server.enforce_game_mode();
            server.refresh_permissions();
        }
    }

    /// `getGuestCommandAccess`.
    pub fn guest_command_access(&self) -> bool {
        self.pumpkin
            .upgrade()
            .is_some_and(|server| server.host.guest_command_access())
    }

    /// `setGuestCommandAccess`.
    pub fn set_guest_command_access(&self, access: bool) {
        if let Some(server) = self.pumpkin.upgrade() {
            server.host.set_guest_command_access(access);
            server.refresh_permissions();
            server.enforce_game_mode();
        }
    }

    /// `forceGameMode`.
    pub fn force_game_mode(&self) -> bool {
        self.pumpkin
            .upgrade()
            .is_none_or(|server| server.host.force_game_mode())
    }

    /// `setForceGameMode`.
    pub fn set_force_game_mode(&self, force: bool) {
        if let Some(server) = self.pumpkin.upgrade() {
            server.host.set_force_game_mode(force);
            server.enforce_game_mode();
        }
    }

    /// The world's game mode, which new players start in.
    pub fn default_game_mode(&self) -> Option<GameMode> {
        self.pumpkin
            .upgrade()
            .map(|server| server.default_game_mode())
    }

    /// `setWorldGameType`.
    pub fn set_default_game_mode(&self, mode: GameMode) {
        if let Some(server) = self.pumpkin.upgrade() {
            server.set_default_game_mode(mode);
        }
    }

    /// `getPersonalGameMode`: the host's own.
    pub fn personal_game_mode(&self) -> Option<GameMode> {
        let server = self.pumpkin.upgrade()?;
        let player = server.get_player_by_uuid(self.host.uuid)?;
        Some(player.gamemode.load())
    }

    /// `setPersonalGameType`.
    pub fn set_personal_game_mode(&self, mode: GameMode) {
        if let Some(server) = self.pumpkin.upgrade()
            && let Some(player) = server.get_player_by_uuid(self.host.uuid)
        {
            let _ = player.set_gamemode(mode);
        }
    }
}

/// How long the host's skin waits for its player to be on the server.
const SKIN_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// `getSingleplayerProfile`'s properties: the host logs in over its pipe
/// without them, so its signed `textures` are fetched on a thread and put on
/// its player once it is there, for guests to see its skin.
fn give_host_its_skin(server: std::sync::Weak<Server>, uuid: uuid::Uuid) {
    let spawned = thread::Builder::new()
        .name("host-skin".to_owned())
        .spawn(move || {
            let properties = match crate::auth::skin::fetch_signed_properties(&uuid.to_string()) {
                Ok(properties) => properties,
                Err(error) => {
                    crate::warn!("The host's skin not fetched: {error:#}");
                    return;
                }
            };
            let properties: Vec<pumpkin_protocol::Property> = properties
                .into_iter()
                .map(|p| pumpkin_protocol::Property {
                    name: p.name.into(),
                    value: p.value.into(),
                    signature: p.signature.map(Into::into),
                })
                .collect();
            let started = std::time::Instant::now();
            while started.elapsed() < SKIN_WAIT {
                let Some(server) = server.upgrade() else {
                    return;
                };
                if let Some(player) = server.get_player_by_uuid(uuid) {
                    player.gameprofile.properties.store(Arc::new(properties));
                    return;
                }
                drop(server);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        });
    if let Err(error) = spawned {
        crate::warn!("No thread for the host's skin: {error}");
    }
}

/// Takes in the Java clients of a published world until `stop`.
async fn accept_java(
    server: Arc<Server>,
    listener: tokio::net::TcpListener,
    stop: CancellationToken,
    tasks: TaskTracker,
) {
    // The host's pipe is connection 0.
    let mut next_id = 1;
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, address)) => {
                    if let Err(error) = stream.set_nodelay(true) {
                        crate::warn!("LAN connection without TCP_NODELAY: {error}");
                    }
                    let limiter = PacketRateLimiter::from_config(
                        &server.advanced_config.networking.java.packet_limiter,
                    );
                    let pending = PendingConnection::new(
                        stream,
                        address,
                        next_id,
                        limiter,
                        Arc::downgrade(&server),
                    );
                    next_id += 1;
                    tasks.spawn(serve_java_connection(Arc::clone(&server), pending));
                }
                Err(error) => {
                    crate::warn!("LAN connection not accepted: {error}");
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            },
        }
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

/// Opens the world and returns once the server is up and waiting for the client.
pub fn start(options: Options) -> Result<(IntegratedServer, ClientPipe)> {
    crate::instances::ensure_dirs(&options.game_dir);
    pumpkin::data::set_data_dir(crate::home::dir().join("cache").join("server"));
    rank_server_threads();
    if let Err(error) = prepare_new_world(&options) {
        crate::warn!("World data packs not prepared: {error}");
    }
    if let Err(error) = migrate_offline_host(&options.save_path, &options.host) {
        crate::warn!("The host's player data not moved to its account: {error}");
    }
    let host_online = options.host.online;
    let (client_rx, server_tx) = tokio::io::simplex(TO_CLIENT_BYTES);
    let (server_rx, client_tx) = tokio::io::simplex(TO_SERVER_BYTES);

    let save_path = options.save_path.clone();
    let host = options.host.clone();
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
        handle: None,
        tasks: TaskTracker::new(),
        host,
        published: None,
    };

    let (handle, pumpkin, tasks) = ready
        .recv()
        .map_err(|_| anyhow!("the integrated server stopped while starting"))?
        .map_err(|error| anyhow!(error))?;
    server.pumpkin = pumpkin;
    server.handle = Some(handle.clone());
    server.tasks = tasks;
    if host_online {
        give_host_its_skin(server.pumpkin.clone(), server.host.uuid);
    }
    let pipe = ClientPipe {
        rx: SyncIoBridge::new_with_handle(client_rx, handle.clone()),
        tx: SyncIoBridge::new_with_handle(client_tx, handle),
    };
    Ok((server, pipe))
}

/// Pumpkin's worker threads: half the machine.
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
type Started = Result<(Handle, std::sync::Weak<Server>, TaskTracker), String>;

/// The server's threads under the client's: the tick as the client's own
/// work, generation and IO below it (see `platform::lower_thread_priority`).
/// The global Rayon pool (Pumpkin's region IO) is built here for that, once.
fn rank_server_threads() {
    threads::set_start_hook(|role| match role {
        ThreadRole::Tick => {}
        ThreadRole::Background => crate::platform::lower_thread_priority(),
    });
    let _ = rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("Rayon-Worker-{i}"))
        .start_handler(|_| crate::platform::lower_thread_priority())
        .build_global();
}

/// Pumpkin used to log the host in offline: its data is under the
/// offline UUID of its name. Moved to the account's, once, where the
/// account has none yet.
fn migrate_offline_host(save_path: &std::path::Path, host: &Host) -> io::Result<()> {
    let Ok(offline) = pumpkin::net::offline_uuid(&host.name) else {
        return Ok(());
    };
    if offline == host.uuid {
        return Ok(());
    }
    let players = save_path.join("players");
    let Ok(folders) = std::fs::read_dir(&players) else {
        return Ok(());
    };
    let (from, to) = (offline.to_string(), host.uuid.to_string());
    for folder in folders.flatten() {
        let Ok(files) = std::fs::read_dir(folder.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name();
            let Some(rest) = name.to_str().and_then(|n| n.strip_prefix(&from)) else {
                continue;
            };
            let target = folder.path().join(format!("{to}{rest}"));
            if !target.exists() {
                std::fs::rename(file.path(), &target)?;
                crate::info!("Moved {} to the account's UUID", file.path().display());
            }
        }
    }
    Ok(())
}

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
    server.host.embed(
        &options.host.name,
        options.settings.allow_cheats && !is_hardcore(&options),
    );

    let tasks = TaskTracker::new();
    let pending = PendingConnection::local(
        read,
        write,
        CLIENT_ADDRESS,
        PacketRateLimiter::from_config(&server.advanced_config.networking.java.packet_limiter),
        Arc::downgrade(&server),
        GameProfile {
            id: options.host.uuid,
            name: options.host.name.clone(),
            properties: ArcSwap::default(),
            profile_actions: None,
        },
    );
    tasks.spawn(serve_java_connection(Arc::clone(&server), pending));

    let _ = started.send(Ok((
        Handle::current(),
        Arc::downgrade(&server),
        tasks.clone(),
    )));

    tokio::select! {
        () = cancel.cancelled() => {},
        () = server.stop_token.cancelled() => {},
    }
    server.stop();
    pumpkin.shutdown(&tasks).await;
}

/// A new world with data packs or experiments gets its `level.dat` before the server starts.
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
    // What a LAN guest gets: the host's own pipe skips all three.
    networking.java.online_mode = true;
    networking.java.encryption = true;
    networking.java.compression.enabled = true;
    networking.java.max_players = MAX_PLAYERS;
    // `IntegratedServer.initServer`'s MOTD, for the server list and the LAN broadcast.
    networking.java.motd = motd(options);
    networking.bedrock.online_mode = true;
    let distance = |chunks: u8| NonZero::new(chunks.max(2)).unwrap_or(NonZero::<u8>::MIN);
    networking.java.view_distance = distance(options.view_distance);
    networking.java.simulation_distance = distance(options.simulation_distance);

    config.commands.use_console = false;
    config.commands.use_tty = false;
    // The host's level comes from `Host::permission_level`, the guests' too.
    config.commands.default_op_level = PermissionLvl::Zero;
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

/// `"<host> - <level name>"`.
fn motd(options: &Options) -> String {
    format!("{} - {}", options.host.name, options.settings.name)
}

/// The standalone server's loaders: WASM plugins only.
fn plugin_loaders() -> Vec<Arc<dyn PluginLoader>> {
    let verify = AdvancedConfiguration::default().plugins.verify_signatures;
    vec![Arc::new(WasmPluginLoader::new(verify))]
}

/// Pumpkin's ban, whitelist and operator lists, kept in memory.
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
            host: Host {
                uuid: uuid::Uuid::from_u128(7),
                name: "Steve".to_owned(),
                online: false,
            },
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
        // Nothing listens until the world is published.
        assert!(!advanced.networking.java.enabled);
        assert!(!advanced.networking.bedrock.enabled);
        assert!(advanced.networking.java.online_mode);
        assert_eq!(advanced.networking.java.max_players, MAX_PLAYERS);
        assert_eq!(advanced.networking.java.motd, "Steve - World");
        // The host's and the guests' levels come from the host's rules.
        assert_eq!(advanced.commands.default_op_level, PermissionLvl::Zero);
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
    }

    #[test]
    fn the_hosts_offline_data_moves_to_its_account() {
        let dir = std::env::temp_dir().join(format!("melon-migrate-{}", std::process::id()));
        let host = Host {
            uuid: uuid::Uuid::from_u128(0x1234),
            name: "Steve".to_owned(),
            online: true,
        };
        let offline = pumpkin::net::offline_uuid("Steve").unwrap();
        let data = dir.join("players").join("data");
        let advancements = dir.join("players").join("advancements");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&advancements).unwrap();
        std::fs::write(data.join(format!("{offline}.dat")), b"old").unwrap();
        std::fs::write(advancements.join(format!("{offline}.json")), b"{}").unwrap();
        // The account's own data stays.
        std::fs::write(advancements.join(format!("{}.json", host.uuid)), b"new").unwrap();
        migrate_offline_host(&dir, &host).unwrap();
        assert_eq!(
            std::fs::read(data.join(format!("{}.dat", host.uuid))).unwrap(),
            b"old"
        );
        assert!(!data.join(format!("{offline}.dat")).exists());
        assert_eq!(
            std::fs::read(advancements.join(format!("{}.json", host.uuid))).unwrap(),
            b"new"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
