//! The player's own state the server keeps authoritative.

pub mod boss;
mod decode;
mod entity;
pub mod particle;
pub mod recipes;
pub mod scores;
mod serverbound;
pub mod sound;

pub use decode::{GameEvent, PlayerInfoUpdate, decode};

/// `ClientboundRespawnPacket.KEEP_ATTRIBUTE_MODIFIERS`.
pub const KEEP_ATTRIBUTE_MODIFIERS: u8 = 1;
/// `ClientboundRespawnPacket.KEEP_ENTITY_DATA`.
pub const KEEP_ENTITY_DATA: u8 = 2;
pub use entity::{
    ATTRIBUTE_ARMOR, ATTRIBUTE_BELOW_NAME_DISTANCE, ATTRIBUTE_JUMP_STRENGTH, ATTRIBUTE_MAX_HEALTH,
    ATTRIBUTE_MOVEMENT_SPEED, ATTRIBUTE_NAME_TAG_DISTANCE, ATTRIBUTE_SCALE, ATTRIBUTE_STEP_HEIGHT,
    AddEntity, AttributeSnapshot, DataValue, EVENT_DEATH, EntityFlags, EntityPacket, EquipmentSlot,
    FLAG_ON_FIRE, LIVING_OFF_HAND, LIVING_USING_ITEM, MoveEntity, NAUSEA_ID, SetPosition, relative,
};
pub use serverbound::{
    PlayerAction, PlayerCommand, PlayerInput, write_container_click, write_creative_slot,
};

pub mod id {
    pub const CB_BLOCK_ENTITY_DATA: i32 = cb::BLOCK_ENTITY_DATA.0;
    pub const CB_BLOCK_EVENT: i32 = 7;
    pub const CB_BLOCK_UPDATE: i32 = 8;
    pub const CB_CONTAINER_CLOSE: i32 = cb::CONTAINER_CLOSE.0;
    pub const CB_CONTAINER_SET_CONTENT: i32 = 18;
    pub const CB_CONTAINER_SET_DATA: i32 = cb::CONTAINER_SET_DATA.0;
    pub const CB_MERCHANT_OFFERS: i32 = cb::MERCHANT_OFFERS.0;
    pub const CB_CONTAINER_SET_SLOT: i32 = 20;
    pub const CB_EXPLODE: i32 = 36;
    pub const CB_BOSS_EVENT: i32 = cb::BOSS_EVENT.0;
    pub const CB_GAME_EVENT: i32 = 39;
    pub const CB_RESPAWN: i32 = 84;
    pub const CB_SECTION_BLOCKS_UPDATE: i32 = 86;
    pub const CB_SET_CURSOR_ITEM: i32 = 98;
    pub const CB_SET_ENTITY_MOTION: i32 = 103;
    pub const CB_SET_EXPERIENCE: i32 = 105;
    pub const CB_SET_HEALTH: i32 = 106;
    pub const CB_SET_HELD_SLOT: i32 = 107;
    pub const CB_SET_PLAYER_INVENTORY: i32 = 110;

    pub const SB_CLIENT_COMMAND: i32 = 12;
    pub const SB_CLIENT_TICK_END: i32 = 13;
    pub const SB_CLIENT_INFORMATION: i32 = 14;
    pub const SB_CONTAINER_CLICK: i32 = 18;
    pub const SB_CONTAINER_CLOSE: i32 = 19;
    pub const SB_CONTAINER_BUTTON_CLICK: i32 = sb::CONTAINER_BUTTON_CLICK.0;
    pub const SB_RENAME_ITEM: i32 = sb::RENAME_ITEM.0;
    pub const SB_PLACE_RECIPE: i32 = sb::PLACE_RECIPE.0;
    pub const SB_RECIPE_BOOK_CHANGE_SETTINGS: i32 = sb::RECIPE_BOOK_CHANGE_SETTINGS.0;
    pub const SB_RECIPE_BOOK_SEEN_RECIPE: i32 = sb::RECIPE_BOOK_SEEN_RECIPE.0;
    pub const SB_SELECT_TRADE: i32 = sb::SELECT_TRADE.0;
    pub const SB_SET_BEACON: i32 = sb::SET_BEACON.0;
    pub const SB_PLAYER_ACTION: i32 = 41;
    pub const SB_PLAYER_COMMAND: i32 = 42;
    pub const SB_PLAYER_INPUT: i32 = 43;
    pub const SB_MOVE_PLAYER_POS: i32 = 30;
    pub const SB_MOVE_PLAYER_STATUS_ONLY: i32 = 33;
    pub const SB_MOVE_PLAYER_ROT: i32 = sb::MOVE_PLAYER_ROT.0;
    pub const CB_MOVE_VEHICLE: i32 = cb::MOVE_VEHICLE.0;
    pub const SB_MOVE_VEHICLE: i32 = sb::MOVE_VEHICLE.0;
    pub const SB_PADDLE_BOAT: i32 = sb::PADDLE_BOAT.0;
    pub const SB_SET_CARRIED_ITEM: i32 = 54;
    pub const SB_USE_ITEM_ON: i32 = sb::USE_ITEM_ON.0;
    pub const SB_PICK_ITEM_FROM_BLOCK: i32 = sb::PICK_ITEM_FROM_BLOCK.0;
    pub const SB_PUNCH: i32 = sb::PUNCH.0;
    pub const SB_ATTACK: i32 = sb::ATTACK.0;
    pub const SB_INTERACT: i32 = sb::INTERACT.0;
    pub const SB_SPECTATOR_ACTION: i32 = sb::SPECTATOR_ACTION.0;
    pub const SB_USE_ITEM: i32 = sb::USE_ITEM.0;
    pub const CB_OPEN_SCREEN: i32 = cb::OPEN_SCREEN.0;
    pub const CB_OPEN_MOUNT_SCREEN: i32 = cb::MOUNT_SCREEN_OPEN.0;
    pub const CB_BLOCK_CHANGED_ACK: i32 = cb::BLOCK_CHANGED_ACK.0;
    pub const CB_INITIALIZE_BORDER: i32 = cb::INITIALIZE_BORDER.0;
    pub const CB_SET_BORDER_CENTER: i32 = cb::SET_BORDER_CENTER.0;
    pub const CB_SET_BORDER_LERP_SIZE: i32 = cb::SET_BORDER_LERP_SIZE.0;
    pub const CB_SET_BORDER_SIZE: i32 = cb::SET_BORDER_SIZE.0;
    pub const CB_SET_BORDER_WARNING_DELAY: i32 = cb::SET_BORDER_WARNING_DELAY.0;
    pub const CB_SET_BORDER_WARNING_DISTANCE: i32 = cb::SET_BORDER_WARNING_DISTANCE.0;
    pub const CB_CHANGE_DIFFICULTY: i32 = cb::CHANGE_DIFFICULTY.0;
    pub const CB_SET_DEFAULT_SPAWN_POSITION: i32 = cb::SET_DEFAULT_SPAWN_POSITION.0;
    pub const CB_SET_CHUNK_CACHE_CENTER: i32 = cb::SET_CHUNK_CACHE_CENTER.0;
    pub const CB_SET_CHUNK_CACHE_RADIUS: i32 = cb::SET_CHUNK_CACHE_RADIUS.0;
    pub const CB_SET_SIMULATION_DISTANCE: i32 = cb::SET_SIMULATION_DISTANCE.0;
    pub const CB_TICKING_STATE: i32 = cb::TICKING_STATE.0;
    pub const CB_TICKING_STEP: i32 = cb::TICKING_STEP.0;

    use pumpkin_data::packet::{clientbound::play as cb, serverbound::play as sb};
    pub const CB_ENTITY_EVENT: i32 = cb::ENTITY_EVENT.0;
    pub const CB_BLOCK_DESTRUCTION: i32 = cb::BLOCK_DESTRUCTION.0;
    pub const CB_LEVEL_EVENT: i32 = cb::LEVEL_EVENT.0;
    pub const CB_LEVEL_PARTICLES: i32 = cb::LEVEL_PARTICLES.0;
    pub const CB_ANIMATE: i32 = cb::ANIMATE.0;
    pub const CB_SOUND: i32 = cb::SOUND.0;
    pub const CB_SOUND_ENTITY: i32 = cb::SOUND_ENTITY.0;
    pub const CB_PLAYER_ABILITIES: i32 = cb::PLAYER_ABILITIES.0;
    pub const CB_PLAYER_INFO_REMOVE: i32 = cb::PLAYER_INFO_REMOVE.0;
    pub const CB_PLAYER_INFO_UPDATE: i32 = cb::PLAYER_INFO_UPDATE.0;
    pub const CB_SET_TIME: i32 = cb::SET_TIME.0;
    pub const CB_SET_ENTITY_DATA: i32 = cb::SET_ENTITY_DATA.0;
    pub const CB_UPDATE_MOB_EFFECT: i32 = cb::UPDATE_MOB_EFFECT.0;
    pub const CB_REMOVE_MOB_EFFECT: i32 = cb::REMOVE_MOB_EFFECT.0;
    pub const SB_CHANGE_GAME_MODE: i32 = sb::CHANGE_GAME_MODE.0;
    pub const SB_CHANGE_DIFFICULTY: i32 = sb::CHANGE_DIFFICULTY.0;
    pub const SB_LOCK_DIFFICULTY: i32 = sb::LOCK_DIFFICULTY.0;
    pub const SB_PLAYER_ABILITIES: i32 = sb::PLAYER_ABILITIES.0;
    pub const SB_SET_CREATIVE_MODE_SLOT: i32 = sb::SET_CREATIVE_MODE_SLOT.0;
    pub const SB_TELEPORT_TO_ENTITY: i32 = sb::TELEPORT_TO_ENTITY.0;
}
