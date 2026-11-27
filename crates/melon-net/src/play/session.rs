//! A joined session.

use super::level::chunk;
use super::level::chunk_batch::ChunkBatchSizeCalculator;
use super::stats::NetStats;
use super::{chat, game};
use crate::login::{self, ClientInfo, JoinInfo, write_client_information};
use melon_protocol::connection::{ReadHalf, WriteHalf};
use melon_protocol::packet::{self, Reader, write_packet};
use melon_protocol::{id, varint};
use melon_world::dimension::{DimensionTypes, Height};
use melon_world::player::hands::Hand;
use anyhow::{Context, Result};
use pumpkin_data::packet::clientbound::play as cb;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

/// Something the connection thread received that the game loop acts on.
pub enum WorldEvent {
    ChunkLoaded(melon_world::chunk::Chunk),
    /// `ClientboundForgetLevelChunkPacket`: chunk `(x, z)` left view distance.
    ChunkUnloaded(i32, i32),
    /// `ClientboundChunksBiomesPacket`.
    BiomesChanged(Vec<(i32, i32, melon_world::biome::ChunkBiomes)>),
    /// `ClientboundLightUpdatePacket`: sky/block light for one column.
    LightUpdated {
        x: i32,
        z: i32,
        update: melon_world::chunk::LightUpdate,
    },
    /// `ClientboundPlayerPositionPacket`: the server moved the player.
    Teleport(Teleport),
    /// `melon:movement` from the server: allowed states, others' starts.
    Movement(pumpkin_protocol::java::melon_movement::MovementPayload),
    /// `minecraft:register` from the server: the plugin channels it takes.
    ServerChannels(Vec<String>),
    /// `ClientboundPingPacket` in play.
    Ping(i32),
    Game(game::GameEvent),
    /// `ClientboundStartConfigurationPacket`.
    Reconfiguring,
    /// The configuration ended and a new `Login` came.
    Rejoined(Box<JoinInfo>, Vec<WorldEvent>),
}

/// `ClientboundPlayerPositionPacket`'s position, velocity and rotation.
#[derive(Clone, Copy, Debug)]
pub struct Teleport {
    /// Echoed in `ServerboundAcceptTeleportationPacket`.
    pub id: i32,
    pub pos: (f64, f64, f64),
    pub delta: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub relative: i32,
    /// Already answered while joining, before the game loop exists.
    pub acked: bool,
}

impl Teleport {
    pub const X: i32 = 1 << 0;
    pub const Y: i32 = 1 << 1;
    pub const Z: i32 = 1 << 2;
    pub const YAW: i32 = 1 << 3;
    pub const PITCH: i32 = 1 << 4;
    pub const DELTA_X: i32 = 1 << 5;
    pub const DELTA_Y: i32 = 1 << 6;
    pub const DELTA_Z: i32 = 1 << 7;
    /// The player's own motion turns with the change of look first.
    pub const ROTATE_DELTA: i32 = 1 << 8;

    /// `PositionMoveRotation.calculateAbsolute` for one value.
    pub fn resolve(&self, flag: i32, current: f64, new: f64) -> f64 {
        if self.relative & flag != 0 {
            current + new
        } else {
            new
        }
    }
}

/// A joined server.
pub struct Session {
    pub info: JoinInfo,
    /// What arrived while joining, handed out by the first [`Session::poll`].
    joining: Vec<WorldEvent>,
    pub events: Receiver<Result<WorldEvent, String>>,
    pub writer: Arc<Mutex<WriteHalf>>,
    pub end_reason: Option<String>,
    pub close: Option<Box<dyn FnOnce() + Send>>,
    /// Why a [`send`](Self::send) cut the stream off, for [`poll`](Self::poll) to end on.
    send_failed: std::sync::OnceLock<String>,
    stats: Arc<NetStats>,
}

impl Session {
    pub fn start(
        mut reader: ReadHalf,
        writer: Arc<Mutex<WriteHalf>>,
        mut listener: PlayListener,
        info: JoinInfo,
        joining: Vec<WorldEvent>,
        close: Box<dyn FnOnce() + Send>,
    ) -> Result<Self> {
        let (tx, events) = mpsc::channel();
        let stats = Arc::new(NetStats::default());
        listener.ack_teleports = false;
        thread::Builder::new()
            .name("server-connection".to_owned())
            .spawn({
                let writer = Arc::clone(&writer);
                let stats = Arc::clone(&stats);
                let uuid = info.uuid;
                let codec_registries = info.registries.codec_registries();
                move || {
                    pumpkin_protocol::codec::registry_ids::install(Some(codec_registries));
                    let reason =
                        match read_loop(&mut reader, &mut listener, &writer, &tx, &stats, uuid) {
                            Ok(reason) => reason,
                            Err(e) => format!("{e:#}"),
                        };
                    let _ = tx.send(Err(reason));
                }
            })
            .context("spawning the connection thread")?;
        Ok(Self {
            info,
            joining,
            events,
            writer,
            end_reason: None,
            close: Some(close),
            send_failed: std::sync::OnceLock::new(),
            stats,
        })
    }

    /// What the connection thread took in so far.
    pub fn stats(&self) -> &NetStats {
        &self.stats
    }

    /// Everything received since the last call, in order.
    pub fn poll(&mut self) -> Vec<WorldEvent> {
        let mut out = std::mem::take(&mut self.joining);
        let joining = out.len();
        while self.end_reason.is_none() {
            match self.events.try_recv() {
                Ok(Ok(event)) => out.push(event),
                Ok(Err(reason)) => self.end_reason = Some(reason),
                Err(TryRecvError::Disconnected) => {
                    self.end_reason = Some("connection thread stopped".to_owned());
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if self.end_reason.is_none() {
            self.end_reason = self.send_failed.get().cloned();
        }
        self.stats.taken(out.len() - joining);
        out
    }

    /// Why the connection ended, once [`poll`](Self::poll) has seen it end.
    pub fn ended(&self) -> Option<&str> {
        self.end_reason.as_deref()
    }

    /// `ServerboundMovePlayerPacket.PosRot`.
    pub fn send_accept_teleport(&self, id: i32, pos: (f64, f64, f64), yaw: f32, pitch: f32) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "accept_teleportation",
                format!(
                    "id={id} pos=({:.5},{:.5},{:.5}) yaw={yaw:.3} pitch={pitch:.3} handler=game-thread after apply",
                    pos.0, pos.1, pos.2
                ),
            );
        }
        self.send(id::SB_PLAY_CONFIRM_TELEPORT, |buf| {
            write_accept_teleport(buf, id, pos, yaw, pitch)
        });
    }

    /// `ServerboundCustomPayloadPacket` on `channel`.
    pub fn send_custom_payload(&self, channel: &str, data: &[u8]) {
        self.send(id::SB_PLAY_CUSTOM_PAYLOAD, |buf| {
            packet::write_string(buf, channel)?;
            buf.write_all(data)?;
            Ok(())
        });
    }

    /// `minecraft:register`: the plugin channels Melon listens on.
    pub fn send_register(&self, channels: &[&str]) {
        self.send_custom_payload("minecraft:register", channels.join("\0").as_bytes());
    }

    /// `ServerboundPongPacket`: the same number the ping carried.
    pub fn send_pong(&self, ping_id: i32) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "pong",
                format!("id={ping_id} handler=game-thread in packet order"),
            );
        }
        self.send(id::SB_PLAY_PONG, |buf| {
            buf.write_all(&ping_id.to_be_bytes())?;
            Ok(())
        });
    }

    pub fn send_position(
        &self,
        pos: (f64, f64, f64),
        yaw: f32,
        pitch: f32,
        on_ground: bool,
        horizontal_collision: bool,
    ) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "move_player_pos_rot",
                format!(
                    "pos=({:.5},{:.5},{:.5}) yaw={yaw:.3} pitch={pitch:.3} ground={on_ground} wall={horizontal_collision}",
                    pos.0, pos.1, pos.2
                ),
            );
        }
        self.send(id::SB_PLAY_MOVE_PLAYER_POS_ROT, |buf| {
            for v in [pos.0, pos.1, pos.2] {
                buf.write_all(&v.to_be_bytes())?;
            }
            packet::write_f32_be(buf, yaw)?;
            packet::write_f32_be(buf, pitch)?;
            buf.write_all(&[u8::from(on_ground) | (u8::from(horizontal_collision) << 1)])?;
            Ok(())
        });
    }

    /// `ServerboundMovePlayerPacket.Pos`.
    pub fn send_position_only(&self, pos: (f64, f64, f64), on_ground: bool, wall: bool) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "move_player_pos",
                format!(
                    "pos=({:.5},{:.5},{:.5}) ground={on_ground} wall={wall}",
                    pos.0, pos.1, pos.2
                ),
            );
        }
        self.send(game::id::SB_MOVE_PLAYER_POS, |buf| {
            for v in [pos.0, pos.1, pos.2] {
                buf.write_all(&v.to_be_bytes())?;
            }
            buf.write_all(&[u8::from(on_ground) | (u8::from(wall) << 1)])?;
            Ok(())
        });
    }

    /// `ServerboundMovePlayerPacket.StatusOnly`.
    pub fn send_status_only(&self, on_ground: bool, wall: bool) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "move_player_status_only",
                format!("ground={on_ground} wall={wall}"),
            );
        }
        self.send(game::id::SB_MOVE_PLAYER_STATUS_ONLY, |buf| {
            buf.write_all(&[u8::from(on_ground) | (u8::from(wall) << 1)])?;
            Ok(())
        });
    }

    /// `ServerboundSetCarriedItemPacket`: the selected hotbar slot (0-8).
    pub fn send_carried_item(&self, slot: u8) {
        self.send(game::id::SB_SET_CARRIED_ITEM, |buf| {
            buf.write_all(&i16::from(slot).to_be_bytes())?;
            Ok(())
        });
    }

    /// `ServerboundContainerClickPacket`, see [`game::write_container_click`].
    pub fn send_container_click(
        &self,
        container: i32,
        state_id: i32,
        click: &melon_world::inventory::Click,
        changed: &[(i16, &pumpkin_data::item_stack::ItemStack)],
        carried: &pumpkin_data::item_stack::ItemStack,
    ) {
        self.send(game::id::SB_CONTAINER_CLICK, |buf| {
            game::write_container_click(buf, container, state_id, click, changed, carried)
        });
    }

    /// `ServerboundContainerClosePacket`.
    pub fn send_container_close(&self, container: i32) {
        self.send(game::id::SB_CONTAINER_CLOSE, |buf| {
            Ok(varint::write_var_int(buf, container)?)
        });
    }

    /// `ServerboundContainerButtonClickPacket`.
    pub fn send_container_button(&self, container: i32, button: i32) {
        self.send(game::id::SB_CONTAINER_BUTTON_CLICK, |buf| {
            varint::write_var_int(buf, container)?;
            Ok(varint::write_var_int(buf, button)?)
        });
    }

    /// `ServerboundPlaceRecipePacket`.
    pub fn send_place_recipe(&self, container: i32, recipe: i32, use_max: bool) {
        self.send(crate::play::game::id::SB_PLACE_RECIPE, |buf| {
            varint::write_var_int(buf, container)?;
            varint::write_var_int(buf, recipe)?;
            packet::write_bool(buf, use_max)
        });
    }

    /// `ServerboundRecipeBookChangeSettingsPacket`.
    pub fn send_recipe_book_settings(&self, book: i32, open: bool, filtering: bool) {
        self.send(
            crate::play::game::id::SB_RECIPE_BOOK_CHANGE_SETTINGS,
            |buf| {
                varint::write_var_int(buf, book)?;
                packet::write_bool(buf, open)?;
                packet::write_bool(buf, filtering)
            },
        );
    }

    /// `ServerboundRecipeBookSeenRecipePacket`: the new-recipe mark was seen.
    pub fn send_recipe_seen(&self, recipe: i32) {
        self.send(crate::play::game::id::SB_RECIPE_BOOK_SEEN_RECIPE, |buf| {
            Ok(varint::write_var_int(buf, recipe)?)
        });
    }

    /// `ServerboundRenameItemPacket`: the anvil's name field.
    pub fn send_rename_item(&self, name: &str) {
        self.send(game::id::SB_RENAME_ITEM, |buf| {
            packet::write_string(buf, name)
        });
    }

    /// `ServerboundSelectTradePacket`: the villager's offer at `index`.
    pub fn send_select_trade(&self, index: i32) {
        self.send(game::id::SB_SELECT_TRADE, |buf| {
            Ok(varint::write_var_int(buf, index)?)
        });
    }

    /// `ServerboundSetBeaconPacket`: the chosen effects, by `MobEffect`
    pub fn send_set_beacon(&self, primary: Option<i32>, secondary: Option<i32>) {
        self.send(game::id::SB_SET_BEACON, |buf| {
            for effect in [primary, secondary] {
                packet::write_bool(buf, effect.is_some())?;
                if let Some(id) = effect {
                    varint::write_var_int(buf, id)?;
                }
            }
            Ok(())
        });
    }

    /// `ServerboundClientCommandPacket.Action.PERFORM_RESPAWN`.
    pub fn send_respawn(&self) {
        self.send(game::id::SB_CLIENT_COMMAND, |buf| {
            Ok(varint::write_var_int(buf, 0)?)
        });
    }

    /// `ServerboundPlayerActionPacket` `DROP_ALL_ITEMS`/`DROP_ITEM`: throws the held stack or one item.
    pub fn send_drop(&self, whole_stack: bool) {
        self.send_player_action(if whole_stack {
            game::PlayerAction::DropAllItems
        } else {
            game::PlayerAction::DropItem
        });
    }

    /// `ServerboundPlayerActionPacket` for an action without a block.
    pub fn send_player_action(&self, action: game::PlayerAction) {
        self.send_player_action_at(action, [0, 0, 0], 0, 0);
    }

    /// `ServerboundPlayerActionPacket` targeting a block.
    pub fn send_player_action_at(
        &self,
        action: game::PlayerAction,
        pos: [i32; 3],
        face: u8,
        sequence: i32,
    ) {
        self.send(game::id::SB_PLAYER_ACTION, |buf| {
            varint::write_var_int(buf, action as i32)?;
            buf.write_all(&packet::pack_block_pos(pos).to_be_bytes())?;
            buf.write_all(&[face])?;
            Ok(varint::write_var_int(buf, sequence)?)
        });
    }

    /// `ServerboundAttackPacket`: hits the entity `entity_id`.
    pub fn send_attack(&self, entity_id: i32) {
        self.send(game::id::SB_ATTACK, |buf| {
            varint::write_var_int(buf, entity_id)?;
            Ok(())
        });
    }

    /// `ServerboundInteractPacket`: a right click on the entity `entity_id`
    pub fn send_interact(&self, entity_id: i32, hand: Hand, location: [f64; 3], sneaking: bool) {
        self.send(game::id::SB_INTERACT, |buf| {
            varint::write_var_int(buf, entity_id)?;
            varint::write_var_int(buf, hand.id())?;
            packet::write_lp_vec3(buf, location)?;
            packet::write_bool(buf, sneaking)
        });
    }

    /// `ServerboundSpectatorActionPacket`.
    pub fn send_spectate(&self, entity: Option<i32>) {
        let value = entity.map_or(0, |id| id.wrapping_add(1));
        self.send(game::id::SB_SPECTATOR_ACTION, |buf| {
            varint::write_var_int(buf, value)?;
            Ok(())
        });
    }

    /// `ServerboundPunchPacket`.
    pub fn send_punch(&self) {
        self.send(game::id::SB_PUNCH, |_| Ok(()));
    }

    /// `ServerboundUseItemPacket`.
    pub fn send_use_item(&self, hand: Hand, sequence: i32, yaw: f32, pitch: f32) {
        self.send(game::id::SB_USE_ITEM, |buf| {
            varint::write_var_int(buf, hand.id())?;
            varint::write_var_int(buf, sequence)?;
            packet::write_f32_be(buf, yaw)?;
            packet::write_f32_be(buf, pitch)
        });
    }

    /// `ServerboundUseItemOnPacket`: right-click a block.
    pub fn send_use_item_on(
        &self,
        hand: Hand,
        pos: [i32; 3],
        face: u8,
        cursor: [f32; 3],
        inside: bool,
        sequence: i32,
    ) {
        self.send(game::id::SB_USE_ITEM_ON, |buf| {
            write_use_item_on(buf, hand, pos, face, cursor, inside, sequence)
        });
    }

    /// `melon:swap`: break the block at `pos` and place `hand`'s block in
    /// its spot as one change; the body is a use-item-on.
    pub fn send_swap_block(
        &self,
        hand: Hand,
        pos: [i32; 3],
        face: u8,
        cursor: [f32; 3],
        inside: bool,
        sequence: i32,
    ) {
        self.send(id::SB_PLAY_CUSTOM_PAYLOAD, |buf| {
            packet::write_string(buf, SWAP_CHANNEL)?;
            write_use_item_on(buf, hand, pos, face, cursor, inside, sequence)
        });
    }

    /// `ServerboundPickItemFromBlockPacket`.
    pub fn send_pick_item_from_block(&self, pos: [i32; 3], include_data: bool) {
        self.send(game::id::SB_PICK_ITEM_FROM_BLOCK, |buf| {
            buf.write_all(&packet::pack_block_pos(pos).to_be_bytes())?;
            packet::write_bool(buf, include_data)
        });
    }

    /// `ServerboundPlayerCommandPacket`, for this player.
    pub fn send_player_command(&self, command: game::PlayerCommand) {
        self.send_player_command_with(command, 0);
    }

    /// The same with its data: how hard a ridden horse jumps (0-100).
    pub fn send_player_command_with(&self, command: game::PlayerCommand, data: i32) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "player_command",
                format!("command={} data={data}", command as i32),
            );
        }
        let entity_id = self.info.entity_id;
        self.send(game::id::SB_PLAYER_COMMAND, |buf| {
            varint::write_var_int(buf, entity_id)?;
            varint::write_var_int(buf, command as i32)?;
            Ok(varint::write_var_int(buf, data)?)
        });
    }

    /// `ServerboundMovePlayerPacket.Rot`.
    pub fn send_rotation(&self, yaw: f32, pitch: f32, on_ground: bool, horizontal: bool) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "move_player_rot",
                format!("yaw={yaw:.3} pitch={pitch:.3} ground={on_ground} wall={horizontal}"),
            );
        }
        self.send(game::id::SB_MOVE_PLAYER_ROT, |buf| {
            packet::write_f32_be(buf, yaw)?;
            packet::write_f32_be(buf, pitch)?;
            buf.write_all(&[u8::from(on_ground) | (u8::from(horizontal) << 1)])?;
            Ok(())
        });
    }

    /// `ServerboundMoveVehiclePacket`: where the vehicle the player steers is.
    pub fn send_move_vehicle(&self, pos: [f64; 3], yaw: f32, pitch: f32, on_ground: bool) {
        self.send(game::id::SB_MOVE_VEHICLE, |buf| {
            for v in pos {
                buf.write_all(&v.to_be_bytes())?;
            }
            packet::write_f32_be(buf, yaw)?;
            packet::write_f32_be(buf, pitch)?;
            packet::write_bool(buf, on_ground)
        });
    }

    /// `ServerboundPaddleBoatPacket`: which paddles are in the water.
    pub fn send_paddle_boat(&self, left: bool, right: bool) {
        self.send(game::id::SB_PADDLE_BOAT, |buf| {
            packet::write_bool(buf, left)?;
            packet::write_bool(buf, right)
        });
    }

    /// `ServerboundPlayerInputPacket`.
    pub fn send_player_input(&self, input: game::PlayerInput) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(true, "player_input", format!("bits={:#04x}", input.bits()));
        }
        self.send(game::id::SB_PLAYER_INPUT, |buf| {
            buf.write_all(&[input.bits()])?;
            Ok(())
        });
    }

    /// `ServerboundClientInformationPacket` in play: the options changed.
    pub fn send_client_tick_end(&self) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(true, "client_tick_end", "handler=end of client tick");
        }
        self.send(game::id::SB_CLIENT_TICK_END, |_| Ok(()));
    }

    pub fn send_client_information(&self, info: &ClientInfo) {
        self.send(game::id::SB_CLIENT_INFORMATION, |buf| {
            write_client_information(buf, info)
        });
    }

    /// `ServerboundChatPacket`: a message, unsigned.
    pub fn send_chat(&self, message: &str, offset: i32) {
        self.send(chat::id::SB_CHAT, |buf| {
            chat::write_chat(buf, message, offset)
        });
    }

    /// `ServerboundChatCommandPacket`: `command` without its slash.
    pub fn send_command(&self, command: &str) {
        self.send(chat::id::SB_CHAT_COMMAND, |buf| {
            chat::write_command(buf, command)
        });
    }

    /// `ServerboundChatAckPacket`.
    pub fn send_chat_ack(&self, offset: i32) {
        self.send(chat::id::SB_CHAT_ACK, |buf| chat::write_ack(buf, offset));
    }

    /// `ServerboundCommandSuggestionPacket`.
    pub fn send_command_suggestion(&self, transaction: i32, text: &str) {
        self.send(chat::id::SB_COMMAND_SUGGESTION, |buf| {
            chat::write_suggestion_request(buf, transaction, text)
        });
    }

    /// `ServerboundChangeGameModePacket`: the game mode switcher's choice.
    pub fn send_change_game_mode(&self, mode: melon_world::game_mode::GameMode) {
        self.send(game::id::SB_CHANGE_GAME_MODE, |buf| {
            buf.write_all(&[mode.id()])?;
            Ok(())
        });
    }

    /// `ServerboundChangeDifficultyPacket`: the difficulty's id (0 peaceful .. 3 hard).
    pub fn send_change_difficulty(&self, difficulty: u8) {
        self.send(game::id::SB_CHANGE_DIFFICULTY, |buf| {
            varint::write_var_int(buf, i32::from(difficulty))?;
            Ok(())
        });
    }

    /// `ServerboundLockDifficultyPacket`.
    pub fn send_lock_difficulty(&self, locked: bool) {
        self.send(game::id::SB_LOCK_DIFFICULTY, |buf| {
            buf.write_all(&[u8::from(locked)])?;
            Ok(())
        });
    }

    /// `ServerboundPlayerAbilitiesPacket`: started or stopped flying.
    pub fn send_player_abilities(&self, flags: u8) {
        self.send(game::id::SB_PLAYER_ABILITIES, |buf| {
            buf.write_all(&[flags])?;
            Ok(())
        });
    }

    /// `ServerboundSetCreativeModeSlotPacket`.
    pub fn send_creative_slot(&self, slot: i16, stack: &pumpkin_data::item_stack::ItemStack) {
        self.send(game::id::SB_SET_CREATIVE_MODE_SLOT, |buf| {
            game::write_creative_slot(buf, slot, stack)
        });
    }

    /// `ServerboundPlayerLoadedPacket`.
    pub fn send_player_loaded(&self) {
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                true,
                "player_loaded",
                "handler=game-thread after LevelLoadTracker; server starts accepting moves",
            );
        }
        self.send(id::SB_PLAY_PLAYER_LOADED, |_| Ok(()));
    }

    /// `ServerboundTeleportToEntityPacket`: a spectator jumps to a player.
    pub fn send_teleport_to_entity(&self, target: uuid::Uuid) {
        self.send(game::id::SB_TELEPORT_TO_ENTITY, |buf| {
            packet::write_uuid(buf, &target)
        });
    }

    /// A new world on the same connection ([`WorldEvent::Rejoined`]).
    pub fn rejoined(&mut self, info: JoinInfo, events: Vec<WorldEvent>) {
        self.info = info;
        self.joining = events;
    }

    /// Writes one packet; a write that fails or times out ends the session.
    pub fn send(&self, id: i32, body: impl FnOnce(&mut Vec<u8>) -> Result<()>) {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !writer.play {
            return;
        }
        melon_core::probe::trace_sent(id);
        let _ = writer.write_packet(id, body);
        if let Some(kind) = writer.failed() {
            let failure = timed_out(std::io::Error::from(kind).into());
            let _ = self.send_failed.set(format!("{failure:#}"));
            return;
        }
        self.stats.transmitted();
    }

    /// Disconnects. The server sees the connection close and saves the player.
    pub fn close(&mut self) {
        if let Some(close) = self.close.take() {
            close();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

/// An answer the client owes the server: packet ID and body.
pub struct Reply {
    pub id: i32,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn send(self, stream: &mut impl Write, compression: Option<i32>) -> Result<()> {
        write_packet(stream, compression, self.id, |buf| {
            buf.extend_from_slice(&self.body);
            Ok(())
        })
    }
}

/// What a Play packet means for the game.
pub enum Play {
    Event(WorldEvent),
    Disconnected(String),
    Nothing,
}

/// `ClientPacketListener`'s part Melon implements, shared by the join and the session.
pub struct PlayListener {
    batches: ChunkBatchSizeCalculator,
    /// The dimension types the server registered.
    dimensions: DimensionTypes,
    /// How tall the level is whose chunks arrive.
    height: Height,
    /// Join has no game loop yet, so spawn teleports are acked here.
    ack_teleports: bool,
    /// `MELON_PACKET_DEBUG`: reports the packets nothing resolves.
    debug: super::debug::PacketDebug,
}

impl Default for PlayListener {
    fn default() -> Self {
        Self::new(DimensionTypes::default())
    }
}

impl PlayListener {
    pub fn new(dimensions: DimensionTypes) -> Self {
        Self {
            batches: ChunkBatchSizeCalculator::default(),
            height: Height::OVERWORLD,
            dimensions,
            ack_teleports: true,
            debug: super::debug::PacketDebug::from_env(),
        }
    }

    /// The level the login put the player in: dimension type `id`.
    pub fn enter(&mut self, id: i32) {
        self.height = self.dimensions.get(id).height;
    }

    /// Replaces the packet debugger (a replay sets its own).
    pub fn set_debug(&mut self, debug: super::debug::PacketDebug) {
        self.debug = debug;
    }

    pub fn handle(&mut self, pid: i32, body: &[u8]) -> Result<(Option<Reply>, Play)> {
        self.debug.received(pid, body);
        handle_play_packet(pid, body, self)
    }
}

fn handle_play_packet(
    pid: i32,
    body: &[u8],
    listener: &mut PlayListener,
) -> Result<(Option<Reply>, Play)> {
    let batches = &mut listener.batches;
    let mut r = Reader::new(body);
    Ok(match pid {
        id::CB_PLAY_KEEP_ALIVE => (
            Some(Reply {
                id: id::SB_PLAY_KEEP_ALIVE,
                body: body.to_vec(),
            }),
            Play::Nothing,
        ),
        id::CB_PLAY_PING => {
            let ping_id = i32::from_be_bytes(body.try_into().unwrap_or([0; 4]));
            if listener.ack_teleports {
                if melon_core::probe::enabled() {
                    melon_core::probe::trace_io(
                        false,
                        "ping",
                        format!("id={ping_id} handler=net-thread pong (join, no game loop)"),
                    );
                }
                (
                    Some(Reply {
                        id: id::SB_PLAY_PONG,
                        body: body.to_vec(),
                    }),
                    Play::Nothing,
                )
            } else {
                if melon_core::probe::enabled() {
                    melon_core::probe::trace_io(
                        false,
                        "ping",
                        format!("id={ping_id} handler=queued for game-thread pong"),
                    );
                }
                (None, Play::Event(WorldEvent::Ping(ping_id)))
            }
        }
        id::CB_PLAY_CHUNK_BATCH_START => {
            batches.on_batch_start();
            (None, Play::Nothing)
        }
        id::CB_PLAY_CHUNK_BATCH_FINISHED => {
            batches.on_batch_finished(r.var_int()?);
            let mut reply = Vec::new();
            packet::write_f32_be(&mut reply, batches.desired_chunks_per_tick())?;
            (
                Some(Reply {
                    id: id::SB_PLAY_CHUNK_BATCH_RECEIVED,
                    body: reply,
                }),
                Play::Nothing,
            )
        }
        id::CB_PLAY_LEVEL_CHUNK_WITH_LIGHT => (
            None,
            Play::Event(WorldEvent::ChunkLoaded(chunk::decode(
                body,
                listener.height,
            )?)),
        ),
        id::CB_PLAY_LIGHT_UPDATE => {
            let x = r.var_int()?;
            let z = r.var_int()?;
            let update = chunk::decode_light(&mut r)?;
            (None, Play::Event(WorldEvent::LightUpdated { x, z, update }))
        }
        id::CB_PLAY_CHUNKS_BIOMES => {
            let columns = chunk::decode_biomes(body, listener.height)?;
            (None, Play::Event(WorldEvent::BiomesChanged(columns)))
        }
        id::CB_PLAY_FORGET_LEVEL_CHUNK => {
            let key = r.i64_be()?;
            let event = WorldEvent::ChunkUnloaded(key as i32, (key >> 32) as i32);
            (None, Play::Event(event))
        }
        id::CB_PLAY_PLAYER_POSITION => {
            let mut teleport = read_player_position(&mut r)?;
            let reply = if listener.ack_teleports {
                teleport.acked = true;
                if melon_core::probe::enabled() {
                    melon_core::probe::trace_io(
                        false,
                        "player_position",
                        format!(
                            "id={} pos=({:.5},{:.5},{:.5}) delta={:?} yaw={} pitch={} {} handler=net-thread accept (join)",
                            teleport.id,
                            teleport.pos.0,
                            teleport.pos.1,
                            teleport.pos.2,
                            teleport.delta,
                            teleport.yaw,
                            teleport.pitch,
                            melon_core::probe::relative_flags(teleport.relative),
                        ),
                    );
                }
                Some(confirm_teleport(
                    teleport.id,
                    teleport.pos,
                    teleport.yaw,
                    teleport.pitch,
                )?)
            } else {
                if melon_core::probe::enabled() {
                    melon_core::probe::trace_io(
                        false,
                        "player_position",
                        format!(
                            "id={} pos=({:.5},{:.5},{:.5}) delta={:?} yaw={} pitch={} {} handler=queued for game-thread apply+accept",
                            teleport.id,
                            teleport.pos.0,
                            teleport.pos.1,
                            teleport.pos.2,
                            teleport.delta,
                            teleport.yaw,
                            teleport.pitch,
                            melon_core::probe::relative_flags(teleport.relative),
                        ),
                    );
                }
                None
            };
            (reply, Play::Event(WorldEvent::Teleport(teleport)))
        }
        game::id::CB_RESPAWN => {
            let spawn = login::read_spawn_info(&mut r)?;
            let data_to_keep = r.u8()?;
            listener.enter(spawn.dimension_id);
            let event = game::GameEvent::Respawn {
                dimension_id: spawn.dimension_id,
                dimension_name: spawn.dimension_name,
                game_mode: spawn.game_mode,
                previous_game_mode: spawn.previous_game_mode,
                keep_entity_data: data_to_keep & game::KEEP_ENTITY_DATA != 0,
                keep_attributes: data_to_keep & game::KEEP_ATTRIBUTE_MODIFIERS != 0,
                hashed_seed: spawn.hashed_seed,
            };
            (None, Play::Event(WorldEvent::Game(event)))
        }
        id::CB_PLAY_LOGIN => {
            let join = login::read_login(&mut r)?;
            listener.enter(join.dimension_id);
            let event = game::GameEvent::Login {
                entity_id: join.entity_id,
                dimension_id: join.dimension_id,
                dimension_name: join.dimension_name,
                game_mode: join.game_mode,
                previous_game_mode: join.previous_game_mode,
                hashed_seed: join.hashed_seed,
            };
            (None, Play::Event(WorldEvent::Game(event)))
        }
        id::CB_PLAY_DISCONNECT => (
            None,
            Play::Disconnected(Reader::new(body).component_text().unwrap_or_default()),
        ),
        id::CB_PLAY_RESOURCE_PACK_PUSH => {
            login::read_pack_push(body)?;
            (None, Play::Nothing)
        }
        id::CB_PLAY_RESOURCE_PACK_POP => {
            login::read_pack_pop(body)?;
            (None, Play::Nothing)
        }
        pid if pid == cb::SHOW_DIALOG.0 => {
            crate::dialog::connection::read_show(body, true)?;
            (None, Play::Nothing)
        }
        pid if pid == cb::CLEAR_DIALOG.0 => {
            crate::dialog::connection::clear();
            (None, Play::Nothing)
        }
        pid if pid == cb::SERVER_LINKS.0 => {
            crate::dialog::connection::read_links(body)?;
            (None, Play::Nothing)
        }
        pid if pid == cb::UPDATE_TAGS.0 => {
            crate::dialog::connection::read_tags(body)?;
            (None, Play::Nothing)
        }
        pid if pid == cb::CUSTOM_PAYLOAD.0 => (None, read_custom_payload(&mut r)?),
        pid if pid == cb::COOKIE_REQUEST.0 => (Some(cookie_response(&mut r)?), Play::Nothing),
        pid if IGNORED.contains(&pid) => (None, Play::Nothing),
        _ => {
            let decoded = game::decode(pid, body);
            match &decoded {
                Ok(None) => listener.debug.unresolved(pid, body, None),
                Err(e) => listener.debug.unresolved(pid, body, Some(format!("{e:#}"))),
                Ok(Some(_)) => {}
            }
            (
                None,
                decoded?.map_or(Play::Nothing, |e| Play::Event(WorldEvent::Game(e))),
            )
        }
    })
}

/// The session's packet loop.
pub fn read_loop(
    reader: &mut ReadHalf,
    listener: &mut PlayListener,
    writer: &login::SharedWriter,
    events: &mpsc::Sender<Result<WorldEvent, String>>,
    stats: &NetStats,
    uuid: uuid::Uuid,
) -> Result<String> {
    loop {
        let (pid, body) = reader.read_packet().map_err(timed_out)?;
        if pid == id::CB_PLAY_START_CONFIGURATION {
            let rejoined = reconfigure(reader, listener, writer, events, uuid)?;
            if events.send(Ok(rejoined)).is_err() {
                return Ok("left the world".to_owned());
            }
            continue;
        }
        let started = std::time::Instant::now();
        let (reply, play) = listener.handle(pid, &body)?;
        stats.packet(pid, body.len(), started.elapsed().as_nanos() as u64);
        stats.set_nanos_per_chunk(listener.batches.nanos_per_chunk());
        if let Some(reply) = reply {
            let mut writer = writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let compression = writer.compression();
            reply.send(&mut *writer, compression).map_err(timed_out)?;
        }
        let event = match play {
            Play::Event(event) => event,
            Play::Disconnected(reason) => {
                return Ok(format!("disconnected by the server: {reason}"));
            }
            Play::Nothing => continue,
        };
        stats.sent(matches!(event, WorldEvent::ChunkLoaded(_)));
        if events.send(Ok(event)).is_err() {
            return Ok("left the world".to_owned());
        }
    }
}

/// `ClientPacketListener.handleConfigurationStart`.
fn reconfigure(
    reader: &mut ReadHalf,
    listener: &mut PlayListener,
    writer: &login::SharedWriter,
    events: &mpsc::Sender<Result<WorldEvent, String>>,
    uuid: uuid::Uuid,
) -> Result<WorldEvent> {
    {
        let mut writer = writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        writer.write_packet(id::SB_PLAY_CONFIGURATION_ACKNOWLEDGED, |_| Ok(()))?;
        writer.play = false;
    }
    let _ = events.send(Ok(WorldEvent::Reconfiguring));
    let (chat_types, registries, world) =
        login::configure_loop(reader, writer).map_err(timed_out)?;
    let debug = std::mem::take(&mut listener.debug);
    *listener = PlayListener::new(world.dimension_types.clone());
    listener.debug = debug;
    let (mut info, joined) = login::play(reader, writer, listener).map_err(timed_out)?;
    listener.ack_teleports = false;
    info.chat_types = chat_types;
    info.registries = registries;
    info.dimension_types = world.dimension_types;
    info.damage_effects = world.damage_effects;
    info.biomes = world.biomes;
    info.uuid = uuid;
    Ok(WorldEvent::Rejoined(Box::new(info), joined))
}

/// What a read or write that waited longer than the socket's timeout turns into.
fn timed_out(e: anyhow::Error) -> anyhow::Error {
    let timeout = e.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
    });
    if timeout {
        anyhow::anyhow!(TIMEOUT_REASON)
    } else {
        e
    }
}

/// The end reason of a connection that timed out.
pub const TIMEOUT_REASON: &str = "disconnect.timeout";

/// `ClientboundPlayerPositionPacket`.
pub fn read_player_position(r: &mut Reader) -> Result<Teleport> {
    let id = r.var_int()?;
    let [x, y, z] = r.vec3_f64()?;
    let delta = r.vec3_f64()?;
    let yaw = r.f32_be()?;
    let pitch = r.f32_be()?;
    let relative = r.i32_be()?;
    Ok(Teleport {
        id,
        pos: (x, y, z),
        delta,
        yaw,
        pitch,
        relative,
        acked: false,
    })
}

/// 26.3 `ServerboundAcceptTeleportationPacket`.
pub fn confirm_teleport(
    teleport_id: i32,
    pos: (f64, f64, f64),
    yaw: f32,
    pitch: f32,
) -> Result<Reply> {
    let mut buf = Vec::new();
    write_accept_teleport(&mut buf, teleport_id, pos, yaw, pitch)?;
    Ok(Reply {
        id: id::SB_PLAY_CONFIRM_TELEPORT,
        body: buf,
    })
}

fn write_accept_teleport(
    buf: &mut Vec<u8>,
    teleport_id: i32,
    pos: (f64, f64, f64),
    yaw: f32,
    pitch: f32,
) -> Result<()> {
    varint::write_var_int(buf, teleport_id)?;
    for v in [pos.0, pos.1, pos.2] {
        buf.write_all(&v.to_be_bytes())?;
    }
    packet::write_f32_be(buf, yaw)?;
    packet::write_f32_be(buf, pitch)?;
    Ok(())
}

/// `ClientboundCustomPayloadPacket`: only `melon:movement` means anything;
/// a broken one is dropped like an unknown channel.
fn read_custom_payload(r: &mut Reader) -> Result<Play> {
    use pumpkin_protocol::java::melon_movement::{CHANNEL, MovementPayload};
    Ok(match r.string()?.as_str() {
        CHANNEL => MovementPayload::decode(r.remaining()).map_or(Play::Nothing, |payload| {
            Play::Event(WorldEvent::Movement(payload))
        }),
        "minecraft:register" => Play::Event(WorldEvent::ServerChannels(
            String::from_utf8_lossy(r.remaining())
                .split('\0')
                .filter(|channel| !channel.is_empty())
                .map(str::to_owned)
                .collect(),
        )),
        _ => Play::Nothing,
    })
}

/// `ClientCommonPacketListenerImpl.handleRequestCookie`, with no payload: Melon stores no cookies.
fn cookie_response(r: &mut Reader) -> Result<Reply> {
    let key = r.string()?;
    let mut body = Vec::new();
    packet::write_string(&mut body, &key)?;
    packet::write_bool(&mut body, false)?;
    Ok(Reply {
        id: pumpkin_data::packet::serverbound::play::COOKIE_RESPONSE.0,
        body,
    })
}

/// The server's `melon:swap` (`pumpkin_protocol::java::melon_swap`).
pub const SWAP_CHANNEL: &str = "melon:swap";

/// The body of `ServerboundUseItemOnPacket`.
fn write_use_item_on(
    buf: &mut Vec<u8>,
    hand: Hand,
    pos: [i32; 3],
    face: u8,
    cursor: [f32; 3],
    inside: bool,
    sequence: i32,
) -> Result<()> {
    varint::write_var_int(buf, hand.id())?;
    buf.write_all(&packet::pack_block_pos(pos).to_be_bytes())?;
    varint::write_var_int(buf, i32::from(face))?;
    packet::write_f32_be(buf, cursor[0])?;
    packet::write_f32_be(buf, cursor[1])?;
    packet::write_f32_be(buf, cursor[2])?;
    packet::write_bool(buf, inside)?;
    packet::write_bool(buf, false)?;
    Ok(varint::write_var_int(buf, sequence)?)
}

/// Packets Melon knows and has no use for.
const IGNORED: [i32; 5] = [
    cb::BUNDLE_DELIMITER.0,
    cb::STOP_SOUND.0,
    cb::CUSTOM_CHAT_COMPLETIONS.0,
    cb::OPEN_SIGN_EDITOR.0,
    cb::SERVER_DATA.0,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_that_waited_too_long_is_a_timeout() {
        let wait = std::io::Error::from(std::io::ErrorKind::WouldBlock);
        let e = timed_out(anyhow::Error::new(wait).context("packet length"));
        assert_eq!(e.to_string(), TIMEOUT_REASON);
        let other = timed_out(anyhow::anyhow!("implausible packet length 9"));
        assert_ne!(other.to_string(), TIMEOUT_REASON);
    }

    #[test]
    fn a_ping_is_answered_with_the_same_number() {
        let mut listener = PlayListener::default();
        let (reply, play) = listener
            .handle(id::CB_PLAY_PING, &7_i32.to_be_bytes())
            .unwrap();
        let reply = reply.expect("a pong");
        assert_eq!(reply.id, id::SB_PLAY_PONG);
        assert_eq!(reply.body, 7_i32.to_be_bytes());
        assert!(matches!(play, Play::Nothing));
    }

    #[test]
    fn a_play_cookie_request_is_answered_without_a_cookie() {
        let mut listener = PlayListener {
            ack_teleports: false,
            ..PlayListener::default()
        };
        let mut body = Vec::new();
        packet::write_string(&mut body, "minecraft:token").unwrap();
        let (reply, play) = listener.handle(cb::COOKIE_REQUEST.0, &body).unwrap();
        let reply = reply.expect("a cookie response");
        assert_eq!(
            reply.id,
            pumpkin_data::packet::serverbound::play::COOKIE_RESPONSE.0
        );
        body.push(0);
        assert_eq!(reply.body, body);
        assert!(matches!(play, Play::Nothing));
    }

    #[test]
    fn a_play_ping_waits_for_the_game_loop() {
        let mut listener = PlayListener {
            ack_teleports: false,
            ..PlayListener::default()
        };
        let (reply, play) = listener
            .handle(id::CB_PLAY_PING, &7_i32.to_be_bytes())
            .unwrap();
        assert!(reply.is_none());
        assert!(matches!(play, Play::Event(WorldEvent::Ping(7))));
    }

    #[test]
    fn a_play_teleport_is_confirmed_after_it_is_applied() {
        let mut listener = PlayListener {
            ack_teleports: false,
            ..PlayListener::default()
        };
        let mut body = Vec::new();
        varint::write_var_int(&mut body, 3).unwrap();
        for v in [8.5_f64, 64.0, -12.25] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        for v in [0.0_f64, 0.0, 0.0] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&0.0_f32.to_be_bytes());
        body.extend_from_slice(&0.0_f32.to_be_bytes());
        body.extend_from_slice(&0_i32.to_be_bytes());
        let (reply, play) = listener.handle(id::CB_PLAY_PLAYER_POSITION, &body).unwrap();
        assert!(reply.is_none());
        let Play::Event(WorldEvent::Teleport(teleport)) = play else {
            panic!("queued teleport");
        };
        assert_eq!(teleport.id, 3);
        assert!(!teleport.acked);
    }

    #[test]
    fn accept_teleportation_echoes_the_id_and_resolved_feet() {
        let reply = confirm_teleport(3, (10.0, 64.0, -3.5), 90.0, 15.0).unwrap();
        assert_eq!(reply.id, id::SB_PLAY_CONFIRM_TELEPORT);
        let mut r = Reader::new(&reply.body);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!(r.vec3_f64().unwrap(), [10.0, 64.0, -3.5]);
        assert_eq!(r.f32_be().unwrap(), 90.0);
        assert_eq!(r.f32_be().unwrap(), 15.0);
        assert!(r.remaining().is_empty());
    }

    #[test]
    fn player_position_starts_with_the_teleport_id() {
        let mut body = Vec::new();
        varint::write_var_int(&mut body, 3).unwrap();
        for v in [8.5_f64, 64.0, -12.25] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        for v in [0.0_f64, 0.0, 0.0] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&0.0_f32.to_be_bytes());
        body.extend_from_slice(&0.0_f32.to_be_bytes());
        body.extend_from_slice(&0_i32.to_be_bytes());
        let teleport = read_player_position(&mut Reader::new(&body)).unwrap();
        assert_eq!(teleport.id, 3);
        assert_eq!(teleport.pos, (8.5, 64.0, -12.25));
        assert!(!teleport.acked);
    }
}
