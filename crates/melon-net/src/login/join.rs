//! Joining a server.

use super::client_info::write_client_information;
use super::world_registries::WorldRegistries;
use super::{ClientInfo, Identity};
use crate::play::chat::{self, command_suggestions};
use crate::play::{Play, PlayListener};
use crate::{PROTOCOL_VERSION, Session, Stream, WorldEvent};
use anyhow::{Context, Result, bail};
use melon_protocol::connection::{Connection, ReadHalf, WriteHalf};
use melon_protocol::packet::Reader;
use melon_protocol::{crypto, id, packet, varint};
use melon_world::biome::Biomes;
use melon_world::dimension::DimensionTypes;
use melon_world::game_mode::GameMode;
use pumpkin_data::packet::clientbound::{config, login as login_cb};
use pumpkin_data::packet::serverbound::{config as config_sb, login as login_sb};
use std::io::Write;
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The send side of a joined connection, shared with whatever answers the server.
pub type SharedWriter = Arc<Mutex<WriteHalf>>;

fn lock(writer: &Mutex<WriteHalf>) -> std::sync::MutexGuard<'_, WriteHalf> {
    writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What the server's Play-phase `Login` packet says about the world and the player.
pub struct JoinInfo {
    pub entity_id: i32,
    /// This player's profile id, as logged in.
    pub uuid: uuid::Uuid,
    /// Hardcore hearts, and no respawning.
    pub hardcore: bool,
    pub dimension_id: i32,
    /// `Level.dimension().identifier()`, e.g.
    pub dimension_name: String,
    pub hashed_seed: i64,
    pub game_mode: GameMode,
    /// The mode before the current one, if any.
    pub previous_game_mode: Option<GameMode>,
    pub is_flat: bool,
    /// The server checks accounts with Mojang (`onlineMode`).
    pub online_mode: bool,
    /// `minecraft:chat_type`, from the configuration phase.
    pub chat_types: chat::ChatTypes,
    /// Every registry's ids, from the configuration phase.
    pub registries: command_suggestions::Registries,
    /// The dimension types, by the index `dimension_id` is.
    pub dimension_types: DimensionTypes,
    /// The biomes, by the index chunks name them with.
    pub biomes: Biomes,
    /// The damage types' `effects`, by the id a damage event names.
    pub damage_effects: Arc<[String]>,
}

impl std::fmt::Debug for JoinInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "JoinInfo {{")?;
        writeln!(f, "  entity_id: {}", self.entity_id)?;
        writeln!(f, "  dimension_id: {}", self.dimension_id)?;
        writeln!(f, "  dimension_name: {}", self.dimension_name)?;
        writeln!(f, "  hashed_seed: {}", self.hashed_seed)?;
        writeln!(f, "  game_mode: {:?}", self.game_mode)?;
        writeln!(f, "  is_flat: {}", self.is_flat)?;
        write!(f, "}}")
    }
}

/// Connects to `addr` (`host:port`) and runs Handshake -> Login ->
pub fn connect_and_join(
    addr: &str,
    identity: &Identity,
    client: &ClientInfo,
) -> Result<(JoinInfo, Vec<WorldEvent>)> {
    let (stream, host, port) = connect_tcp(addr)?;
    let (_reader, _writer, _listener, info, events) =
        join_over(Connection::new(stream), &host, port, identity, client)?;
    Ok((info, events))
}

/// Joins the server at `addr` (`host:port`) and keeps the connection alive.
pub fn join_server(addr: &str, identity: &Identity, client: &ClientInfo) -> Result<Session> {
    let (stream, host, port) = connect_tcp(addr)?;
    let control = stream.try_clone().context("cloning the socket")?;
    let pack_close = stream.try_clone().context("cloning the socket")?;
    melon_assets::assets::packs::server::attach(
        Arc::new(|_, _| {}),
        Some(Box::new(move || {
            let _ = pack_close.shutdown(Shutdown::Both);
        })),
    );
    let (reader, writer, listener, info, events) =
        join_over(Connection::new(stream), &host, port, identity, client)?;
    Session::start(
        reader,
        writer,
        listener,
        info,
        events,
        Box::new(move || {
            let _ = control.shutdown(Shutdown::Both);
        }),
    )
}

/// How long a send may wait on a server that stopped reading before the session ends.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// `host:port` ([`parse_address`](super::resolver::parse_address)), resolved and connected.
pub fn connect_tcp(addr: &str) -> Result<(TcpStream, String, u16)> {
    let (host, port) = super::resolver::parse_address(addr).ok_or(crate::status::CannotResolve)?;
    let resolved = super::resolver::resolve(host, port)?;
    let stream = resolved
        .addrs
        .iter()
        .find_map(|target| TcpStream::connect_timeout(target, Duration::from_secs(10)).ok())
        .with_context(|| format!("connecting to {addr}"))?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    Ok((stream, resolved.host, resolved.port))
}

/// Joins over an already open `stream`, such as the pipe to the integrated server.
pub fn join_stream(
    stream: impl Stream + 'static,
    identity: &Identity,
    client: &ClientInfo,
    close: Box<dyn FnOnce() + Send>,
) -> Result<Session> {
    let (reader, writer, listener, info, events) = join_over(
        Connection::new(stream),
        "localhost",
        25565,
        identity,
        client,
    )?;
    Session::start(reader, writer, listener, info, events, close)
}

pub fn join_over(
    mut conn: Connection,
    host: &str,
    port: u16,
    identity: &Identity,
    client: &ClientInfo,
) -> Result<(
    ReadHalf,
    SharedWriter,
    PlayListener,
    JoinInfo,
    Vec<WorldEvent>,
)> {
    handshake(&mut conn, host, port)?;
    login(&mut conn, identity)?;
    let (mut reader, writer) = conn.into_split().context("splitting the connection")?;
    let writer: SharedWriter = Arc::new(Mutex::new(writer));
    melon_assets::assets::packs::server::set_responder(pack_responder(&writer));
    crate::dialog::connection::start(custom_action_sender(&writer));
    let (chat_types, registries, world) = configure(&mut reader, &writer, client)?;
    let mut listener = PlayListener::new(world.dimension_types.clone());
    let (mut info, events) = play(&mut reader, &writer, &mut listener)?;
    info.chat_types = chat_types;
    info.registries = registries;
    info.dimension_types = world.dimension_types;
    info.damage_effects = world.damage_effects;
    info.biomes = world.biomes;
    info.uuid = identity.uuid();
    Ok((reader, writer, listener, info, events))
}

/// Sends a resource pack's status (`ServerboundResourcePackPacket`) to the server.
fn pack_responder(writer: &SharedWriter) -> melon_assets::assets::packs::server::Responder {
    let writer = Arc::clone(writer);
    Arc::new(move |pack, status| {
        let mut writer = lock(&writer);
        let pid = if writer.play {
            id::SB_PLAY_RESOURCE_PACK
        } else {
            id::SB_CONFIG_RESOURCE_PACK
        };
        let _ = writer.write_packet(pid, |buf| {
            packet::write_uuid(buf, &pack)?;
            varint::write_var_int(buf, status as i32)?;
            Ok(())
        });
    })
}

/// Sends `ServerboundCustomClickActionPacket` in the phase the connection
/// is in, for a dialog's or a chat link's custom action.
fn custom_action_sender(writer: &SharedWriter) -> crate::dialog::connection::Sender {
    let writer = Arc::clone(writer);
    Arc::new(move |action, payload| {
        let mut writer = lock(&writer);
        let pid = if writer.play {
            pumpkin_data::packet::serverbound::play::CUSTOM_CLICK_ACTION.0
        } else {
            pumpkin_data::packet::serverbound::config::CUSTOM_CLICK_ACTION.0
        };
        let sent = writer.write_packet(pid, |buf| {
            packet::write_string(buf, action)?;
            crate::dialog::action::write_payload(buf, payload)
                .ok_or_else(|| anyhow::anyhow!("payload too large"))
        });
        if let Err(e) = sent {
            melon_base::warn!("Custom action {action} not sent: {e:#}");
        }
    })
}

/// `ClientboundResourcePackPushPacket`, handed to the pack manager.
pub fn read_pack_push(body: &[u8]) -> Result<()> {
    let mut r = Reader::new(body);
    let pack = r.uuid()?;
    let url = r.string()?;
    let hash = r.string_max(40)?;
    let required = r.bool()?;
    let prompt = if r.bool()? {
        Some(r.component()?)
    } else {
        None
    };
    melon_assets::assets::packs::server::push(pack, url, hash, required, prompt);
    Ok(())
}

/// `ClientboundResourcePackPopPacket`: one pack, or all.
pub fn read_pack_pop(body: &[u8]) -> Result<()> {
    let mut r = Reader::new(body);
    let pack = if r.bool()? { Some(r.uuid()?) } else { None };
    melon_assets::assets::packs::server::pop(pack);
    Ok(())
}

pub fn handshake(conn: &mut Connection, host: &str, port: u16) -> Result<()> {
    conn.write_packet(id::HANDSHAKE_INTENTION, |buf| {
        varint::write_var_int(buf, PROTOCOL_VERSION)?;
        packet::write_string(buf, host)?;
        buf.write_all(&port.to_be_bytes())?;
        Ok(varint::write_var_int(buf, 2)?)
    })
}

pub fn login(conn: &mut Connection, identity: &Identity) -> Result<()> {
    let uuid = identity.uuid();
    conn.write_packet(id::SB_LOGIN_HELLO, |buf| {
        packet::write_string(buf, identity.username())?;
        packet::write_uuid(buf, &uuid)
    })?;

    loop {
        let (pid, body) = conn.read_packet()?;
        let mut r = Reader::new(&body);
        match pid {
            id::CB_LOGIN_DISCONNECT => {
                let reason = r.string().unwrap_or_default();
                bail!("server rejected login: {reason}");
            }
            id::CB_LOGIN_ENCRYPTION_REQUEST => {
                handle_encryption_request(conn, &mut r, identity)?;
            }
            id::CB_LOGIN_COMPRESSION => {
                conn.enable_compression(r.var_int()?);
            }
            id::CB_LOGIN_SUCCESS => break,
            // `handleCustomQuery`: no mod understands it.
            pid if pid == login_cb::CUSTOM_QUERY.0 => {
                let transaction = r.var_int()?;
                conn.write_packet(login_sb::CUSTOM_QUERY_ANSWER.0, |buf| {
                    varint::write_var_int(buf, transaction)?;
                    buf.write_all(&[0])?;
                    Ok(())
                })?;
            }
            // `handleRequestCookie`: Melon keeps no cookies.
            pid if pid == login_cb::COOKIE_REQUEST.0 => {
                let key = r.string()?;
                conn.write_packet(login_sb::COOKIE_RESPONSE.0, |buf| {
                    packet::write_string(buf, &key)?;
                    buf.write_all(&[0])?;
                    Ok(())
                })?;
            }
            other => bail!("unexpected packet {other} while logging in"),
        }
    }

    conn.write_packet(id::SB_LOGIN_ACKNOWLEDGED, |_| Ok(()))
}

/// `ClientHandshakePacketListenerImpl.handleHello`.
pub fn handle_encryption_request(
    conn: &mut Connection,
    r: &mut Reader<'_>,
    identity: &Identity,
) -> Result<()> {
    let server_id = r.string()?;
    let key_len = r.var_int()? as usize;
    let public_key_der = r.bytes(key_len)?;
    let token_len = r.var_int()? as usize;
    let verify_token = r.bytes(token_len)?;
    let should_authenticate = r.bool()?;

    let public_key = crypto::parse_public_key(&public_key_der)?;
    let shared_secret = crypto::random_bytes::<16>();
    let encrypted_secret = crypto::rsa_encrypt(&public_key, &shared_secret)?;
    let encrypted_token = crypto::rsa_encrypt(&public_key, &verify_token)?;

    if should_authenticate {
        let Identity::Online {
            uuid,
            access_token,
            session,
            ..
        } = identity
        else {
            bail!(
                "the server requires a signed-in account (online_mode = true), but an offline profile is selected - pick a Microsoft account or one of another sign-in service in the launcher"
            );
        };
        let hash = crypto::server_hash(&server_id, &shared_secret, &public_key_der);
        melon_base::auth::minecraft::join_server(
            session,
            access_token,
            &uuid.simple().to_string(),
            &hash,
        )?;
    }

    conn.write_packet(id::SB_LOGIN_ENCRYPTION_RESPONSE, |buf| {
        varint::write_var_int(buf, encrypted_secret.len() as i32)?;
        buf.extend_from_slice(&encrypted_secret);
        varint::write_var_int(buf, encrypted_token.len() as i32)?;
        buf.extend_from_slice(&encrypted_token);
        Ok(())
    })?;

    conn.enable_encryption(&shared_secret);
    Ok(())
}

/// The configuration phase; returns the chat types the server registered.
pub fn configure(
    reader: &mut ReadHalf,
    writer: &SharedWriter,
    client: &ClientInfo,
) -> Result<(
    chat::ChatTypes,
    command_suggestions::Registries,
    WorldRegistries,
)> {
    lock(writer).write_packet(id::SB_CONFIG_CUSTOM_PAYLOAD, |buf| {
        packet::write_string(buf, "minecraft:brand")?;
        packet::write_string(buf, "melon")
    })?;
    lock(writer).write_packet(id::SB_CONFIG_CLIENT_INFORMATION, |buf| {
        write_client_information(buf, client)
    })?;
    configure_loop(reader, writer)
}

/// The configuration phase's packets up to `FinishConfiguration`.
pub fn configure_loop(
    reader: &mut ReadHalf,
    writer: &SharedWriter,
) -> Result<(
    chat::ChatTypes,
    command_suggestions::Registries,
    WorldRegistries,
)> {
    let mut chat_types = chat::ChatTypes::default();
    let mut registries = command_suggestions::Registries::default();
    let mut world = WorldRegistries::default();
    loop {
        let (pid, body) = reader.read_packet()?;
        match pid {
            id::CB_CONFIG_KEEP_ALIVE => {
                lock(writer).write_packet(id::SB_CONFIG_KEEP_ALIVE, |buf| {
                    buf.extend_from_slice(&body);
                    Ok(())
                })?;
            }
            id::CB_CONFIG_PING => {
                lock(writer).write_packet(id::SB_CONFIG_PONG, |buf| {
                    buf.extend_from_slice(&body);
                    Ok(())
                })?;
            }
            id::CB_CONFIG_CUSTOM_PAYLOAD => registries.read_payload(&body)?,
            id::CB_CONFIG_RESOURCE_PACK_PUSH => read_pack_push(&body)?,
            id::CB_CONFIG_RESOURCE_PACK_POP => read_pack_pop(&body)?,
            id::CB_CONFIG_REGISTRY_DATA => {
                crate::dialog::connection::read_registry(&body)?;
                if let Some(types) = chat::ChatTypes::from_registry_packet(&body)? {
                    chat_types = types;
                }
                registries.read_packet(&body)?;
                world.read_packet(&body)?;
            }
            id::CB_CONFIG_SELECT_KNOWN_PACKS => {
                lock(writer).write_packet(id::SB_CONFIG_SELECT_KNOWN_PACKS, |buf| {
                    Ok(varint::write_var_int(buf, 0)?)
                })?;
            }
            pid if pid == config::SHOW_DIALOG.0 => {
                crate::dialog::connection::read_show(&body, false)?;
            }
            pid if pid == config::CLEAR_DIALOG.0 => crate::dialog::connection::clear(),
            pid if pid == config::SERVER_LINKS.0 => crate::dialog::connection::read_links(&body)?,
            pid if pid == config::UPDATE_TAGS.0 => crate::dialog::connection::read_tags(&body)?,
            // `ClientboundDisconnectPacket`: the server's reason, not the
            // closed socket that follows it.
            pid if pid == config::DISCONNECT.0 => {
                bail!(
                    "{}",
                    Reader::new(&body).component_text().unwrap_or_default()
                );
            }
            // `handleRequestCookie`: Melon keeps no cookies.
            pid if pid == config::COOKIE_REQUEST.0 => {
                let key = Reader::new(&body).string()?;
                lock(writer).write_packet(config_sb::COOKIE_RESPONSE.0, |buf| {
                    packet::write_string(buf, &key)?;
                    buf.write_all(&[0])?;
                    Ok(())
                })?;
            }
            id::CB_CONFIG_CODE_OF_CONDUCT => {
                lock(writer).write_packet(id::SB_CONFIG_ACCEPT_CODE_OF_CONDUCT, |_| Ok(()))?;
            }
            id::CB_CONFIG_FINISH => {
                let mut writer = lock(writer);
                writer.write_packet(id::SB_CONFIG_FINISH, |_| Ok(()))?;
                writer.play = true;
                // This thread reads the play packets up to the join.
                registries.install_for_codecs();
                return Ok((chat_types, registries, world));
            }
            _ => {}
        }
    }
}

/// How long the join waits for the spawn and a chunk: `LevelLoadTracker.CLIENT_WAIT_TIMEOUT_MS`.
const JOIN_WAIT: Duration = Duration::from_secs(30);

/// Reads Play packets until the `Login` packet and the first position sync, or until
/// [`JOIN_WAIT`] after the `Login` (vanilla lets the player in anyway then).
pub fn play(
    reader: &mut ReadHalf,
    writer: &SharedWriter,
    listener: &mut PlayListener,
) -> Result<(JoinInfo, Vec<WorldEvent>)> {
    let mut join: Option<JoinInfo> = None;
    let mut events = Vec::new();
    let mut got_spawn = false;
    let mut got_chunk = false;
    let mut deadline = Instant::now() + JOIN_WAIT;

    while Instant::now() < deadline {
        let (pid, body) = reader.read_packet()?;
        if pid == id::CB_PLAY_LOGIN {
            let login = read_login(&mut Reader::new(&body))?;
            listener.enter(login.dimension_id);
            join = Some(login);
            deadline = Instant::now() + JOIN_WAIT;
            continue;
        }
        let (reply, play) = listener.handle(pid, &body)?;
        if let Some(reply) = reply {
            let mut writer = lock(writer);
            let compression = writer.compression();
            reply.send(&mut *writer, compression)?;
        }
        if join.is_none() {
            continue;
        }
        match play {
            Play::Event(event) => {
                got_spawn |= matches!(event, WorldEvent::Teleport(_));
                got_chunk |= matches!(event, WorldEvent::ChunkLoaded(_));
                events.push(event);
            }
            Play::Nothing => {}
            Play::Disconnected(reason) => {
                bail!("the server disconnected us while joining: {reason}")
            }
        }

        if got_spawn && got_chunk {
            lock(writer).write_packet(id::SB_PLAY_PLAYER_LOADED, |_| Ok(()))?;
            break;
        }
    }

    let join =
        join.context("server never sent the Play-phase Login/chunk/position packets in time")?;
    Ok((join, events))
}

/// Field order and presence are the 26.3 branch of `CLogin::write_packet_data`.
pub fn read_login(r: &mut Reader) -> Result<JoinInfo> {
    let entity_id = r.i32_be()?;
    let hardcore = r.bool()?;
    let dimension_count = r.var_int()?;
    for _ in 0..dimension_count {
        let _level_name = r.string()?;
    }
    let _max_players = r.var_int()?;
    let _view_distance = r.var_int()?;
    let _simulated_distance = r.var_int()?;
    let _reduced_debug_info = r.bool()?;
    let _enabled_respawn_screen = r.bool()?;
    let _limited_crafting = r.bool()?;
    let spawn = read_spawn_info(r)?;
    let online_mode = r.bool()?;
    let _enforce_secure_chat = r.bool()?;

    Ok(JoinInfo {
        entity_id,
        uuid: uuid::Uuid::nil(),
        hardcore,
        dimension_id: spawn.dimension_id,
        dimension_name: spawn.dimension_name,
        hashed_seed: spawn.hashed_seed,
        game_mode: spawn.game_mode,
        previous_game_mode: spawn.previous_game_mode,
        is_flat: spawn.is_flat,
        online_mode,
        chat_types: chat::ChatTypes::default(),
        registries: Default::default(),
        dimension_types: DimensionTypes::default(),
        biomes: Biomes::default(),
        damage_effects: Default::default(),
    })
}

/// `CommonPlayerSpawnInfo`, shared by the login and respawn packets.
pub struct SpawnInfo {
    pub dimension_id: i32,
    pub dimension_name: String,
    pub hashed_seed: i64,
    pub game_mode: GameMode,
    pub previous_game_mode: Option<GameMode>,
    pub is_flat: bool,
}

pub fn read_spawn_info(r: &mut Reader) -> Result<SpawnInfo> {
    let dimension_id = r.var_int()?;
    let dimension_name = r.string()?;
    let hashed_seed = r.i64_be()?;
    let game_mode = GameMode::from_id(r.var_int()?);
    let previous_game_mode = GameMode::from_optional_id(r.var_int()? - 1);
    let _debug = r.bool()?;
    let is_flat = r.bool()?;
    if r.bool()? {
        let _death_dimension = r.string()?;
        let _death_pos = r.i64_be()?;
    }
    let _portal_cooldown = r.var_int()?;
    let _sea_level = r.var_int()?;
    Ok(SpawnInfo {
        dimension_id,
        dimension_name,
        hashed_seed,
        game_mode,
        previous_game_mode,
        is_flat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MELON_TEST_SERVER=host:port cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a running server in MELON_TEST_SERVER"]
    fn joins_a_real_server_and_streams_chunks() {
        let addr = std::env::var("MELON_TEST_SERVER").expect("MELON_TEST_SERVER");
        let identity = Identity::Offline {
            username: "MelonTest".to_owned(),
        };
        let mut session = join_server(&addr, &identity, &Default::default()).expect("join");
        let mut chunks = 0;
        let mut spawn = None;
        let until = std::time::Instant::now() + Duration::from_secs(60);
        while chunks < 50 && std::time::Instant::now() < until {
            for event in session.poll() {
                match event {
                    WorldEvent::ChunkLoaded(_) => chunks += 1,
                    WorldEvent::Teleport(t) => spawn = Some(t),
                    _ => {}
                }
            }
            assert!(session.ended().is_none(), "kicked: {:?}", session.ended());
            let t = spawn.expect("the join waits for a spawn position");
            session.send_position(t.pos, t.yaw, t.pitch, true, false);
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("{chunks} chunks");
        assert!(chunks >= 50, "only {chunks} chunks arrived");
    }
}
