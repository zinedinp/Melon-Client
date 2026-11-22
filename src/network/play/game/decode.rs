//! The player-state packets the server sends, decoded into [`GameEvent`]s.

use anyhow::Result;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;

use super::id;
use crate::network::protocol::packet::Reader;
use crate::world::border::BorderUpdate;
use crate::world::clock::ClockUpdate;
use crate::world::game_mode::GameMode;
use crate::world::inventory::merchant::{Offer, Offers};
use crate::world::level_settings::{LevelSetting, SpawnPoint};

/// `ClientboundGameEventPacket` type ids.
const GAME_EVENT_NO_RESPAWN_BLOCK: u8 = 0;
/// Tells the client to skip the death screen's delay; the screen has none.
const GAME_EVENT_IMMEDIATE_RESPAWN: u8 = 11;
const GAME_EVENT_LEVEL_CHUNKS_LOAD_START: u8 = 13;
/// Win game, demo, arrow hit, puffer sting, elder guardian, limited
const GAME_EVENT_UNSHOWN: [u8; 6] = [4, 5, 6, 9, 10, 12];
const GAME_EVENT_START_RAINING: u8 = 1;
const GAME_EVENT_STOP_RAINING: u8 = 2;
const GAME_EVENT_CHANGE_GAME_MODE: u8 = 3;
const GAME_EVENT_RAIN_LEVEL_CHANGE: u8 = 7;
const GAME_EVENT_THUNDER_LEVEL_CHANGE: u8 = 8;

/// The player-state packets, decoded. Chunks and teleports are in
pub enum GameEvent {
    /// `ClientboundSetHealthPacket`.
    Health {
        health: f32,
        food: i32,
        saturation: f32,
    },
    /// `ClientboundSetExperiencePacket`.
    Experience {
        progress: f32,
        level: i32,
        total: i32,
    },
    /// `ClientboundContainerSetContentPacket`: every slot of a menu.
    ContainerContent {
        container: i32,
        state_id: i32,
        slots: Vec<ItemStack>,
        carried: ItemStack,
    },
    /// `ClientboundContainerSetSlotPacket`.
    ContainerSlot {
        container: i32,
        state_id: i32,
        slot: i16,
        item: ItemStack,
    },
    /// `ClientboundContainerSetDataPacket`: one number of a menu's
    ContainerData {
        container: i32,
        id: i16,
        value: i16,
    },
    /// `ClientboundMerchantOffersPacket`: a trader's offers for the menu.
    MerchantOffers {
        container: i32,
        offers: Offers,
    },
    /// `ClientboundSetCursorItemPacket`: the stack held by the mouse.
    CursorItem(ItemStack),
    /// `ClientboundSetPlayerInventoryPacket`, by `Inventory` index (0-8
    PlayerInventorySlot {
        slot: i32,
        item: ItemStack,
    },
    /// `ClientboundSetHeldSlotPacket`.
    HeldSlot(i32),
    /// `ClientboundMoveVehiclePacket`: the server puts the vehicle the
    /// player steers back where it has it.
    MoveVehicle {
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
    },
    /// `ClientboundGameEventPacket` `NO_RESPAWN_BLOCK_AVAILABLE`: the bed
    NoRespawnBlock,
    /// A packet Melon reads and has no use for yet.
    Ignored,
    /// `ClientboundBossEventPacket`.
    Boss(super::boss::BossEvent),
    /// The world border's packets.
    Border(BorderUpdate),
    /// The recipe packets: the book, the machines' item sets, the ghost.
    Recipes(super::recipes::RecipeEvent),
    /// Difficulty, spawn, distances and tick rate.
    Level(LevelSetting),
    GameMode(GameMode),
    /// `ClientboundSetEntityMotionPacket`: an entity's new velocity, per
    EntityMotion {
        entity_id: i32,
        velocity: [f64; 3],
    },
    /// `ClientboundSoundPacket` and `ClientboundSoundEntityPacket`.
    Sound(super::sound::ServerSound),
    /// `ClientboundLevelEventPacket`.
    LevelEvent(super::sound::LevelEvent),
    /// `ClientboundExplodePacket`: the blast, its sound, and the push this
    Explosion(super::sound::Explosion),
    /// `ClientboundLevelParticlesPacket`.
    Particles(crate::particle::server::LevelParticles),
    /// `ClientboundAnimatePacket`: `action` 0 wakes up, 1 and 2 are
    Animate {
        entity: i32,
        action: u8,
    },
    /// `ClientboundBlockUpdatePacket` and `ClientboundSectionBlocksUpdatePacket`:
    BlocksChanged(Vec<([i32; 3], u16)>),
    /// `ClientboundBlockChangedAckPacket`: the server has dealt with every
    BlockChangedAck(i32),
    /// `ClientboundBlockEventPacket`: a block's animation trigger, such as a
    BlockEvent {
        pos: [i32; 3],
        action: u8,
        param: u8,
    },
    /// `ClientboundBlockEntityDataPacket`: a block entity's new tag, of
    BlockEntityData {
        pos: [i32; 3],
        head: Option<crate::world::head_profile::HeadProfile>,
    },
    /// `ClientboundBlockDestructionPacket`: how far `breaker` has broken
    BlockDestruction {
        breaker: i32,
        pos: [i32; 3],
        stage: i32,
    },
    /// `ClientboundRespawnPacket`, after death or a dimension change. The
    Respawn {
        /// The dimension type's index in the registry.
        dimension_id: i32,
        dimension_name: String,
        game_mode: GameMode,
        previous_game_mode: Option<GameMode>,
        /// `ClientboundRespawnPacket.KEEP_ENTITY_DATA`: the new player takes
        /// the old one's synced data (frozen ticks, air...).
        keep_entity_data: bool,
    },
    /// A `Login` packet in the middle of play (a server that moves its
    Login {
        entity_id: i32,
        dimension_id: i32,
        dimension_name: String,
        game_mode: GameMode,
        previous_game_mode: Option<GameMode>,
    },
    /// `ClientboundPlayerAbilitiesPacket`.
    Abilities {
        flags: u8,
        flying_speed: f32,
        walking_speed: f32,
    },
    /// `ClientboundEntityEventPacket` 24-28 for this player: its permission
    PermissionLevel {
        entity_id: i32,
        level: u8,
    },
    /// `ClientboundPlayerInfoUpdatePacket`: players added to or changed in
    PlayerInfo(Vec<PlayerInfoUpdate>),
    /// `ClientboundPlayerInfoRemovePacket`.
    PlayersRemoved(Vec<uuid::Uuid>),
    /// The scoreboard and the tab list's header and footer.
    Scores(super::scores::ScorePacket),
    Chat(crate::network::play::chat::ChatEvent),
    /// `ClientboundCommandsPacket`: the commands this player may run.
    Commands(crate::network::play::chat::commands::CommandTree),
    /// `ClientboundSetTimePacket`: game time and the 26.3 world clocks.
    Time {
        game_time: i64,
        clocks: Vec<ClockUpdate>,
    },
    /// `ClientboundGameEventPacket` rain/thunder.
    RainLevel(f32),
    ThunderLevel(f32),
    /// Spawning, moving, data, equipment and animations of any entity.
    Entity(super::entity::EntityPacket),
    /// `ClientboundUpdateMobEffectPacket`.
    MobEffect {
        entity_id: i32,
        effect: i32,
        amplifier: i32,
        /// Ticks left; `-1` never ends.
        duration: i32,
        /// Ambient (1), visible (2), icon (4), blend (8).
        flags: u8,
    },
    /// `ClientboundRemoveMobEffectPacket`.
    RemoveMobEffect {
        entity_id: i32,
        effect: i32,
    },
    /// `ClientboundMountScreenOpenPacket`: the inventory of the mount
    OpenMount {
        container: i32,
        columns: i32,
        entity_id: i32,
    },
    /// `ClientboundOpenScreenPacket`.
    OpenScreen {
        container: i32,
        window_type: i32,
        title: crate::component::Component,
    },
    /// `ClientboundContainerClosePacket`. Vanilla closes the current screen
    ContainerClose {
        container: i32,
    },
}

/// One player's entry of `ClientboundPlayerInfoUpdatePacket`; each field is
#[derive(Clone, Debug, Default)]
pub struct PlayerInfoUpdate {
    pub uuid: uuid::Uuid,
    pub name: Option<String>,
    pub game_mode: Option<GameMode>,
    pub listed: Option<bool>,
    /// Milliseconds the server measured; negative while unknown.
    pub latency: Option<i32>,
    pub display_name: Option<Option<crate::component::Component>>,
    /// `tabListOrder`: higher comes first.
    pub list_order: Option<i32>,
    /// The face in the list shows the skin's hat layer.
    pub show_hat: Option<bool>,
    /// The profile's `textures` property (Base64), which names its skin.
    pub textures: Option<Box<str>>,
}

/// `ClientboundPlayerInfoUpdatePacket.Action`, in wire order.
mod player_info {
    pub const ADD_PLAYER: u8 = 1 << 0;
    pub const INITIALIZE_CHAT: u8 = 1 << 1;
    pub const UPDATE_GAME_MODE: u8 = 1 << 2;
    pub const UPDATE_LISTED: u8 = 1 << 3;
    pub const UPDATE_LATENCY: u8 = 1 << 4;
    pub const UPDATE_DISPLAY_NAME: u8 = 1 << 5;
    pub const UPDATE_LIST_ORDER: u8 = 1 << 6;
    pub const UPDATE_HAT: u8 = 1 << 7;
}

fn read_player_info(r: &mut Reader) -> Result<Vec<PlayerInfoUpdate>> {
    use player_info::*;
    let actions = r.u8()?;
    let count = r.var_int()?.clamp(0, 4096);
    let mut players = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let mut update = PlayerInfoUpdate {
            uuid: r.uuid()?,
            ..Default::default()
        };
        if actions & ADD_PLAYER != 0 {
            update.name = Some(r.string()?);
            for _ in 0..r.var_int()?.clamp(0, 16) {
                let name = r.string()?;
                let value = r.string()?;
                if r.bool()? {
                    let _signature = r.string()?;
                }
                if name == "textures" {
                    update.textures = Some(value.into());
                }
            }
        }
        if actions & INITIALIZE_CHAT != 0 && r.bool()? {
            let _session = r.uuid()?;
            let _expires = r.i64_be()?;
            let key = r.var_int()?.max(0) as usize;
            r.skip(key)?;
            let signature = r.var_int()?.max(0) as usize;
            r.skip(signature)?;
        }
        if actions & UPDATE_GAME_MODE != 0 {
            update.game_mode = Some(GameMode::from_id(r.var_int()?));
        }
        if actions & UPDATE_LISTED != 0 {
            update.listed = Some(r.bool()?);
        }
        if actions & UPDATE_LATENCY != 0 {
            update.latency = Some(r.var_int()?);
        }
        if actions & UPDATE_DISPLAY_NAME != 0 {
            update.display_name = Some(if r.bool()? {
                Some(r.component()?)
            } else {
                None
            });
        }
        if actions & UPDATE_LIST_ORDER != 0 {
            update.list_order = Some(r.var_int()?);
        }
        if actions & UPDATE_HAT != 0 {
            update.show_hat = Some(r.bool()?);
        }
        players.push(update);
    }
    Ok(players)
}

/// Decodes `pid` if it is one of the packets above.
pub fn decode(pid: i32, body: &[u8]) -> Result<Option<GameEvent>> {
    let mut r = Reader::new(body);
    if let Some(packet) = super::entity::decode(pid, &mut r)? {
        return Ok(Some(GameEvent::Entity(packet)));
    }
    if let Some(event) = super::recipes::decode(pid, &mut r)? {
        return Ok(Some(GameEvent::Recipes(event)));
    }
    Ok(Some(match pid {
        id::CB_SET_HEALTH => GameEvent::Health {
            health: r.f32_be()?,
            food: r.var_int()?,
            saturation: r.f32_be()?,
        },
        id::CB_SET_EXPERIENCE => GameEvent::Experience {
            progress: r.f32_be()?,
            level: r.var_int()?,
            total: r.var_int()?,
        },
        id::CB_CONTAINER_SET_CONTENT => {
            let container = r.var_int()?;
            let state_id = r.var_int()?;
            let count = r.var_int()?.clamp(0, 4096) as usize;
            let mut slots = Vec::with_capacity(count);
            for _ in 0..count {
                slots.push(r.item_stack()?);
            }
            GameEvent::ContainerContent {
                container,
                state_id,
                slots,
                carried: r.item_stack()?,
            }
        }
        id::CB_OPEN_MOUNT_SCREEN => GameEvent::OpenMount {
            container: i32::from(r.u8()?),
            columns: r.var_int()?,
            entity_id: r.i32_be()?,
        },
        id::CB_OPEN_SCREEN => GameEvent::OpenScreen {
            container: r.var_int()?,
            window_type: r.var_int()?,
            title: r.component()?,
        },
        id::CB_CONTAINER_CLOSE => GameEvent::ContainerClose {
            container: r.var_int()?,
        },
        id::CB_CONTAINER_SET_SLOT => GameEvent::ContainerSlot {
            container: r.var_int()?,
            state_id: r.var_int()?,
            slot: r.i16_be()?,
            item: r.item_stack()?,
        },
        id::CB_CONTAINER_SET_DATA => GameEvent::ContainerData {
            container: r.var_int()?,
            id: r.i16_be()?,
            value: r.i16_be()?,
        },
        id::CB_MERCHANT_OFFERS => GameEvent::MerchantOffers {
            container: r.var_int()?,
            offers: r.merchant_offers()?,
        },
        id::CB_SET_CURSOR_ITEM => GameEvent::CursorItem(r.item_stack()?),
        id::CB_SET_PLAYER_INVENTORY => GameEvent::PlayerInventorySlot {
            slot: r.var_int()?,
            item: r.item_stack()?,
        },
        id::CB_SET_HELD_SLOT => GameEvent::HeldSlot(r.var_int()?),
        id::CB_MOVE_VEHICLE => GameEvent::MoveVehicle {
            pos: [r.f64_be()?, r.f64_be()?, r.f64_be()?],
            yaw: r.f32_be()?,
            pitch: r.f32_be()?,
        },
        id::CB_INITIALIZE_BORDER => GameEvent::Border(BorderUpdate::Initialize {
            center: (r.f64_be()?, r.f64_be()?),
            from: r.f64_be()?,
            to: r.f64_be()?,
            ticks: r.var_long()?,
            absolute_max: r.var_int()?,
            warning_blocks: r.var_int()?,
            warning_time: r.var_int()?,
        }),
        id::CB_SET_BORDER_CENTER => {
            GameEvent::Border(BorderUpdate::Center(r.f64_be()?, r.f64_be()?))
        }
        id::CB_SET_BORDER_LERP_SIZE => GameEvent::Border(BorderUpdate::Lerp {
            from: r.f64_be()?,
            to: r.f64_be()?,
            ticks: r.var_long()?,
        }),
        id::CB_SET_BORDER_SIZE => GameEvent::Border(BorderUpdate::Size(r.f64_be()?)),
        id::CB_SET_BORDER_WARNING_DELAY => {
            GameEvent::Border(BorderUpdate::WarningTime(r.var_int()?))
        }
        id::CB_SET_BORDER_WARNING_DISTANCE => {
            GameEvent::Border(BorderUpdate::WarningBlocks(r.var_int()?))
        }
        id::CB_CHANGE_DIFFICULTY => GameEvent::Level(LevelSetting::Difficulty {
            difficulty: r.u8()?,
            locked: r.bool()?,
        }),
        id::CB_SET_DEFAULT_SPAWN_POSITION => {
            GameEvent::Level(LevelSetting::DefaultSpawn(SpawnPoint {
                dimension: r.string()?,
                pos: r.block_pos()?,
                yaw: r.f32_be()?,
                pitch: r.f32_be()?,
            }))
        }
        id::CB_SET_CHUNK_CACHE_CENTER => GameEvent::Level(LevelSetting::ChunkCenter {
            x: r.var_int()?,
            z: r.var_int()?,
        }),
        id::CB_SET_CHUNK_CACHE_RADIUS => GameEvent::Level(LevelSetting::ChunkRadius(r.var_int()?)),
        id::CB_SET_SIMULATION_DISTANCE => {
            GameEvent::Level(LevelSetting::SimulationDistance(r.var_int()?))
        }
        id::CB_TICKING_STATE => GameEvent::Level(LevelSetting::TickingState {
            rate: r.f32_be()?,
            frozen: r.bool()?,
        }),
        id::CB_TICKING_STEP => GameEvent::Level(LevelSetting::TickingStep(r.var_int()?)),
        id::CB_BOSS_EVENT => GameEvent::Boss(super::boss::decode(&mut r)?),
        id::CB_GAME_EVENT => {
            let event = r.u8()?;
            let value = r.f32_be()?;
            match event {
                GAME_EVENT_NO_RESPAWN_BLOCK => GameEvent::NoRespawnBlock,
                GAME_EVENT_IMMEDIATE_RESPAWN | GAME_EVENT_LEVEL_CHUNKS_LOAD_START => {
                    GameEvent::Ignored
                }
                e if GAME_EVENT_UNSHOWN.contains(&e) => GameEvent::Ignored,
                GAME_EVENT_START_RAINING => GameEvent::RainLevel(0.0),
                GAME_EVENT_STOP_RAINING => GameEvent::RainLevel(1.0),
                GAME_EVENT_CHANGE_GAME_MODE => GameEvent::GameMode(GameMode::from_id(value as i32)),
                GAME_EVENT_RAIN_LEVEL_CHANGE => GameEvent::RainLevel(value),
                GAME_EVENT_THUNDER_LEVEL_CHANGE => GameEvent::ThunderLevel(value),
                _ => return Ok(None),
            }
        }
        id::CB_UPDATE_MOB_EFFECT => {
            let (entity_id, effect, amplifier, duration, flags) =
                super::entity::read_update_effect(&mut r)?;
            GameEvent::MobEffect {
                entity_id,
                effect,
                amplifier,
                duration,
                flags,
            }
        }
        id::CB_REMOVE_MOB_EFFECT => {
            let (entity_id, effect) = super::entity::read_remove_effect(&mut r)?;
            GameEvent::RemoveMobEffect { entity_id, effect }
        }
        id::CB_SET_ENTITY_MOTION => {
            let entity_id = r.var_int()?;
            GameEvent::EntityMotion {
                entity_id,
                velocity: r.lp_vec3()?,
            }
        }
        id::CB_EXPLODE => GameEvent::Explosion(super::sound::read_explosion(&mut r)?),
        id::CB_LEVEL_PARTICLES => {
            GameEvent::Particles(super::particle::read_level_particles(&mut r)?)
        }
        id::CB_ANIMATE => GameEvent::Animate {
            entity: r.var_int()?,
            action: r.u8()?,
        },
        id::CB_SOUND => GameEvent::Sound(super::sound::read_sound(&mut r)?),
        id::CB_SOUND_ENTITY => GameEvent::Sound(super::sound::read_entity_sound(&mut r)?),
        id::CB_LEVEL_EVENT => GameEvent::LevelEvent(super::sound::read_level_event(&mut r)?),
        id::CB_BLOCK_CHANGED_ACK => GameEvent::BlockChangedAck(r.var_int()?),
        id::CB_BLOCK_UPDATE => {
            let pos = r.block_pos()?;
            GameEvent::BlocksChanged(vec![(pos, r.var_int()? as u16)])
        }
        id::CB_SECTION_BLOCKS_UPDATE => {
            let section = r.i64_be()?;
            let origin = [
                (section >> 42) as i32 * 16,
                (section << 44 >> 44) as i32 * 16,
                (section << 22 >> 42) as i32 * 16,
            ];
            let count = r.var_int()?.max(0) as usize;
            let mut changes = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                let entry = r.var_long()?;
                let local = entry & 0xFFF;
                let pos = [
                    origin[0] + (local >> 8 & 15) as i32,
                    origin[1] + (local & 15) as i32,
                    origin[2] + (local >> 4 & 15) as i32,
                ];
                changes.push((pos, (entry >> 12) as u16));
            }
            GameEvent::BlocksChanged(changes)
        }
        id::CB_BLOCK_DESTRUCTION => GameEvent::BlockDestruction {
            breaker: r.var_int()?,
            pos: r.block_pos()?,
            stage: i32::from(r.u8()? as i8),
        },
        id::CB_BLOCK_ENTITY_DATA => {
            use pumpkin_protocol::ser::NetworkReadSliceExt;
            let pos = r.block_pos()?;
            r.var_int()?;
            let tag = r
                .remaining()
                .get_nbt(&pumpkin_util::version::JavaMinecraftVersion::V_26_3)
                .map_err(|e| anyhow::anyhow!("block entity NBT: {e}"))?;
            GameEvent::BlockEntityData {
                pos,
                head: tag
                    .as_ref()
                    .and_then(crate::network::play::level::chunk::head_of_tag),
            }
        }
        id::CB_BLOCK_EVENT => GameEvent::BlockEvent {
            pos: r.block_pos()?,
            action: r.u8()?,
            param: r.u8()?,
        },
        id::CB_PLAYER_ABILITIES => GameEvent::Abilities {
            flags: r.u8()?,
            flying_speed: r.f32_be()?,
            walking_speed: r.f32_be()?,
        },
        id::CB_ENTITY_EVENT => {
            let entity_id = r.i32_be()?;
            let event = r.u8()?;
            if !(24..=28).contains(&event) {
                return Ok(Some(GameEvent::Entity(
                    super::entity::EntityPacket::Event {
                        id: entity_id,
                        event,
                    },
                )));
            }
            GameEvent::PermissionLevel {
                entity_id,
                level: event - 24,
            }
        }
        id::CB_PLAYER_INFO_UPDATE => match read_player_info(&mut r) {
            Ok(players) => GameEvent::PlayerInfo(players),
            Err(e) => {
                eprintln!("Spielerliste unlesbar: {e:#}");
                return Ok(None);
            }
        },
        crate::network::play::chat::commands::CB_COMMANDS => {
            match crate::network::play::chat::commands::CommandTree::decode(body) {
                Ok(tree) => GameEvent::Commands(tree),
                Err(e) => {
                    eprintln!("Befehlsbaum unlesbar: {e:#}");
                    return Ok(None);
                }
            }
        }
        id::CB_PLAYER_INFO_REMOVE => {
            let count = r.var_int()?.clamp(0, 4096);
            let mut removed = Vec::with_capacity(count as usize);
            for _ in 0..count {
                removed.push(r.uuid()?);
            }
            GameEvent::PlayersRemoved(removed)
        }
        id::CB_SET_TIME => {
            let game_time = r.i64_be()?;
            let count = r.var_int()?.clamp(0, 256) as usize;
            let mut clocks = Vec::with_capacity(count);
            for _ in 0..count {
                clocks.push(ClockUpdate {
                    id: r.var_int()?,
                    total_ticks: r.var_long()?,
                    partial_tick: r.f32_be()?,
                    rate: r.f32_be()?,
                });
            }
            GameEvent::Time { game_time, clocks }
        }
        _ => {
            match super::scores::decode(pid, body) {
                Ok(Some(packet)) => return Ok(Some(GameEvent::Scores(packet))),
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Scoreboard-Paket {pid} unlesbar: {e:#}");
                    crate::probe::note("decode.error", format!("scoreboard packet {pid}"), || {
                        format!("{e:#}")
                    });
                    return Ok(None);
                }
            }
            return Ok(match crate::network::play::chat::decode(pid, body) {
                Ok(event) => event.map(GameEvent::Chat),
                Err(e) => {
                    eprintln!("Chat-Paket {pid} unlesbar: {e:#}");
                    crate::probe::note("decode.error", format!("chat packet {pid}"), || {
                        format!("{e:#}")
                    });
                    None
                }
            });
        }
    }))
}

impl Reader<'_> {
    /// `ItemStack.OPTIONAL_STREAM_CODEC`, through Pumpkin's decoder: its data
    pub fn merchant_offers(&mut self) -> Result<Offers> {
        let count = self.var_int()?.clamp(0, 256) as usize;
        let mut offers = Vec::with_capacity(count);
        for _ in 0..count {
            let base_cost_a = self.item_cost()?;
            let result = self.item_stack()?;
            let cost_b = if self.bool()? {
                self.item_cost()?
            } else {
                crate::world::inventory::empty()
            };
            let out_of_stock = self.bool()?;
            let uses = self.i32_be()?;
            let max_uses = self.i32_be()?;
            let xp = self.i32_be()?;
            let special_price_diff = self.i32_be()?;
            let price_multiplier = self.f32_be()?;
            let demand = self.i32_be()?;
            offers.push(Offer {
                base_cost_a,
                cost_b,
                result,
                out_of_stock,
                uses,
                max_uses,
                xp,
                special_price_diff,
                price_multiplier,
                demand,
            });
        }
        Ok(Offers {
            offers,
            level: self.var_int()?,
            xp: self.var_int()?,
            show_progress: self.bool()?,
            can_restock: self.bool()?,
        })
    }

    /// `ItemCost.STREAM_CODEC`: the item, how many, and the components it
    fn item_cost(&mut self) -> Result<ItemStack> {
        let id = self.var_int()?;
        let count = self.var_int()?.clamp(1, 99);
        anyhow::ensure!(self.var_int()? == 0, "item cost with components");
        let item = u16::try_from(id)
            .ok()
            .and_then(Item::from_id)
            .ok_or_else(|| anyhow::anyhow!("unknown item {id}"))?;
        Ok(ItemStack::new(count as u8, item))
    }

    pub fn item_stack(&mut self) -> Result<ItemStack> {
        ItemStackSerializer::read(self.remaining())
            .map(ItemStackSerializer::to_stack)
            .map_err(|e| anyhow::anyhow!("item stack: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_protocol::codec::lp_vector_3d::LpVector3d;
    use pumpkin_util::math::vector3::Vector3;

    /// Pumpkin's writer, read back: a zero velocity is one byte, a real one
    #[test]
    fn packed_velocities_round_trip() {
        for velocity in [[0.0, 0.0, 0.0], [0.4, 0.36, -0.1], [-7.5, 2.0, 12.25]] {
            let mut bytes = Vec::new();
            LpVector3d(Vector3::new(velocity[0], velocity[1], velocity[2]))
                .write(&mut bytes)
                .unwrap();
            let mut r = Reader::new(&bytes);
            let read = r.lp_vec3().unwrap();
            assert!(r.remaining().is_empty(), "{velocity:?} left bytes over");
            for (a, b) in read.iter().zip(velocity) {
                assert!(
                    (a - b).abs() < 1.0e-3 * b.abs().max(1.0),
                    "{read:?} vs {velocity:?}"
                );
            }
        }
    }

    /// HugoSMP auction items put Latin-1 umlauts in protocol strings.
    #[test]
    fn an_item_stack_with_latin1_in_a_protocol_string_still_decodes() {
        let mut body = vec![1, 1, 1, 0, 17, 0, 0, 1];
        let mut text = vec![b'x'; 65];
        text.push(0xE4);
        text.extend_from_slice(b"ufer");
        body.push(text.len() as u8);
        body.extend_from_slice(&text);
        body.push(0);
        let mut r = Reader::new(&body);
        let stack = r.item_stack().expect("vanilla replaces invalid UTF-8");
        assert!(!stack.is_empty());
    }
}

#[cfg(test)]
mod block_packet_tests {
    use super::*;

    #[test]
    fn decodes_the_borders_and_the_levels_settings() {
        let mut body = Vec::new();
        for value in [1.0_f64, 2.0, 100.0, 200.0] {
            body.extend(value.to_be_bytes());
        }
        body.extend([40, 0x90, 0xdc, 0xdf, 0x0e, 5, 15]);
        let Some(GameEvent::Border(BorderUpdate::Initialize {
            center,
            to,
            ticks,
            warning_blocks,
            warning_time,
            ..
        })) = decode(id::CB_INITIALIZE_BORDER, &body).unwrap()
        else {
            panic!("not a border");
        };
        assert_eq!((center, to, ticks), ((1.0, 2.0), 200.0, 40));
        assert_eq!((warning_blocks, warning_time), (5, 15));

        let mut state = 20.0_f32.to_be_bytes().to_vec();
        state.push(1);
        assert!(matches!(
            decode(id::CB_TICKING_STATE, &state).unwrap(),
            Some(GameEvent::Level(LevelSetting::TickingState {
                frozen: true,
                ..
            }))
        ));
        assert!(matches!(
            decode(id::CB_CHANGE_DIFFICULTY, &[3, 0]).unwrap(),
            Some(GameEvent::Level(LevelSetting::Difficulty {
                difficulty: 3,
                locked: false
            }))
        ));
    }

    #[test]
    fn decodes_container_close() {
        let Some(GameEvent::ContainerClose { container }) =
            decode(id::CB_CONTAINER_CLOSE, &[1]).unwrap()
        else {
            panic!("not a container close");
        };
        assert_eq!(container, 1);
        assert_eq!(id::CB_CONTAINER_CLOSE, 17);
    }

    #[test]
    fn decodes_pumpkins_block_positions_and_section_updates() {
        use pumpkin_util::math::position::BlockPos;
        use pumpkin_util::math::vector3::Vector3;
        let pos = BlockPos(Vector3::new(-1234, -60, 98765));
        assert_eq!(
            crate::network::protocol::reader::unpack_block_pos(pos.as_long()),
            [-1234, -60, 98765]
        );

        let section: i64 =
            ((-2i64 & 0x3FFFFF) << 42) | ((3i64 & 0x3FFFFF) << 20) | (-4i64 & 0xFFFFF);
        let mut body = section.to_be_bytes().to_vec();
        body.push(1);
        let entry: i64 = (42 << 12) | (2 << 8) | (7 << 4) | 5;
        let mut v = entry as u64;
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                body.push(byte);
                break;
            }
            body.push(byte | 0x80);
        }
        let Some(GameEvent::BlocksChanged(changes)) =
            decode(id::CB_SECTION_BLOCKS_UPDATE, &body).unwrap()
        else {
            panic!("not a block change");
        };
        assert_eq!(changes, vec![([-32 + 2, -64 + 5, 48 + 7], 42)]);
    }

    #[test]
    fn decodes_set_time_clock_updates() {
        let mut body = 1234i64.to_be_bytes().to_vec();
        body.push(1);
        body.push(0);
        let mut ticks = 6000u64;
        loop {
            let byte = (ticks & 0x7F) as u8;
            ticks >>= 7;
            if ticks == 0 {
                body.push(byte);
                break;
            }
            body.push(byte | 0x80);
        }
        body.extend_from_slice(&0.0f32.to_be_bytes());
        body.extend_from_slice(&1.0f32.to_be_bytes());
        let Some(GameEvent::Time { game_time, clocks }) = decode(id::CB_SET_TIME, &body).unwrap()
        else {
            panic!("not a time packet");
        };
        assert_eq!(game_time, 1234);
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0].id, 0);
        assert_eq!(clocks[0].total_ticks, 6000);
        assert_eq!(clocks[0].rate, 1.0);
    }

    #[test]
    fn start_and_stop_raining_match_vanilla_client() {
        let start = decode(id::CB_GAME_EVENT, &[1, 0, 0, 0, 0]).unwrap();
        let stop = decode(id::CB_GAME_EVENT, &[2, 0, 0, 0, 0]).unwrap();
        assert!(matches!(start, Some(GameEvent::RainLevel(v)) if v == 0.0));
        assert!(matches!(stop, Some(GameEvent::RainLevel(v)) if v == 1.0));
        let mut rain = vec![7];
        rain.extend_from_slice(&0.5f32.to_be_bytes());
        assert!(matches!(
            decode(id::CB_GAME_EVENT, &rain).unwrap(),
            Some(GameEvent::RainLevel(v)) if v == 0.5
        ));
    }

    #[test]
    fn shared_flags_packet_reads_the_local_fire_bit() {
        use super::super::entity::{EntityFlags, EntityPacket};
        let mut body = vec![1];
        body.extend_from_slice(&[0, 0, 1, 0xFF]);
        let Some(GameEvent::Entity(EntityPacket::Data { id, values })) =
            decode(id::CB_SET_ENTITY_DATA, &body).unwrap()
        else {
            panic!("not entity data");
        };
        assert_eq!(id, 1);
        assert_eq!(EntityFlags::of(&values).shared.unwrap() & 1, 1);
    }
}
