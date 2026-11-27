//! Packet IDs of protocol 777, from Pumpkin's `packet.rs` (`v26_3`).

pub const HANDSHAKE_INTENTION: i32 = 0;
pub const SB_LOGIN_HELLO: i32 = 0;
pub const SB_LOGIN_ENCRYPTION_RESPONSE: i32 = 1;
pub const SB_LOGIN_ACKNOWLEDGED: i32 = 3;
pub const CB_LOGIN_DISCONNECT: i32 = 0;
pub const CB_LOGIN_ENCRYPTION_REQUEST: i32 = 1;
pub const CB_LOGIN_SUCCESS: i32 = 2;
pub const CB_LOGIN_COMPRESSION: i32 = 3;
pub const SB_CONFIG_CLIENT_INFORMATION: i32 = 0;
pub const SB_CONFIG_CUSTOM_PAYLOAD: i32 = 2;
pub const SB_CONFIG_FINISH: i32 = 3;
pub const SB_CONFIG_SELECT_KNOWN_PACKS: i32 = 7;
pub const SB_CONFIG_ACCEPT_CODE_OF_CONDUCT: i32 = 9;
pub const SB_CONFIG_KEEP_ALIVE: i32 = pumpkin_data::packet::serverbound::config::KEEP_ALIVE.0;
pub const SB_CONFIG_PONG: i32 = pumpkin_data::packet::serverbound::config::PONG.0;
pub const SB_PLAY_PONG: i32 = pumpkin_data::packet::serverbound::play::PONG.0;
pub const SB_PLAY_CUSTOM_PAYLOAD: i32 = pumpkin_data::packet::serverbound::play::CUSTOM_PAYLOAD.0;
pub const SB_CONFIG_RESOURCE_PACK: i32 = pumpkin_data::packet::serverbound::config::RESOURCE_PACK.0;
pub const CB_CONFIG_FINISH: i32 = 3;
pub const CB_CONFIG_REGISTRY_DATA: i32 = pumpkin_data::packet::clientbound::config::REGISTRY_DATA.0;
pub const CB_CONFIG_CUSTOM_PAYLOAD: i32 =
    pumpkin_data::packet::clientbound::config::CUSTOM_PAYLOAD.0;
pub const CB_CONFIG_SELECT_KNOWN_PACKS: i32 = 15;
pub const CB_CONFIG_CODE_OF_CONDUCT: i32 = 20;
pub const CB_CONFIG_KEEP_ALIVE: i32 = pumpkin_data::packet::clientbound::config::KEEP_ALIVE.0;
pub const CB_CONFIG_PING: i32 = pumpkin_data::packet::clientbound::config::PING.0;
pub const CB_PLAY_PING: i32 = pumpkin_data::packet::clientbound::play::PING.0;
pub const CB_CONFIG_RESOURCE_PACK_PUSH: i32 =
    pumpkin_data::packet::clientbound::config::RESOURCE_PACK_PUSH.0;
pub const CB_CONFIG_RESOURCE_PACK_POP: i32 =
    pumpkin_data::packet::clientbound::config::RESOURCE_PACK_POP.0;
pub const SB_PLAY_CONFIRM_TELEPORT: i32 = 0;
pub const SB_PLAY_CHUNK_BATCH_RECEIVED: i32 = 11;
pub const SB_PLAY_KEEP_ALIVE: i32 = 28;
pub const SB_PLAY_MOVE_PLAYER_POS_ROT: i32 = 31;
pub const SB_PLAY_PLAYER_LOADED: i32 = 44;
pub const SB_PLAY_RESOURCE_PACK: i32 = pumpkin_data::packet::serverbound::play::RESOURCE_PACK.0;
pub const CB_PLAY_RESOURCE_PACK_PUSH: i32 =
    pumpkin_data::packet::clientbound::play::RESOURCE_PACK_PUSH.0;
pub const CB_PLAY_RESOURCE_PACK_POP: i32 =
    pumpkin_data::packet::clientbound::play::RESOURCE_PACK_POP.0;
pub const CB_PLAY_CHUNK_BATCH_FINISHED: i32 = 11;
pub const CB_PLAY_CHUNK_BATCH_START: i32 =
    pumpkin_data::packet::clientbound::play::CHUNK_BATCH_START.0;
pub const CB_PLAY_DISCONNECT: i32 = 32;
pub const CB_PLAY_CHUNKS_BIOMES: i32 = pumpkin_data::packet::clientbound::play::CHUNKS_BIOMES.0;
pub const CB_PLAY_FORGET_LEVEL_CHUNK: i32 = 38;
pub const CB_PLAY_KEEP_ALIVE: i32 = 45;
pub const CB_PLAY_LOGIN: i32 = 50;
pub const CB_PLAY_START_CONFIGURATION: i32 =
    pumpkin_data::packet::clientbound::play::START_CONFIGURATION.0;
pub const SB_PLAY_CONFIGURATION_ACKNOWLEDGED: i32 =
    pumpkin_data::packet::serverbound::play::CONFIGURATION_ACKNOWLEDGED.0;
pub const CB_PLAY_LEVEL_CHUNK_WITH_LIGHT: i32 = 46;
pub const CB_PLAY_LIGHT_UPDATE: i32 = pumpkin_data::packet::clientbound::play::LIGHT_UPDATE.0;
pub const CB_PLAY_PLAYER_POSITION: i32 = 73;
