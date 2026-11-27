//! Applying what the server sent, dispatched by group: chat, scoreboard,
//! inventory, the player and the level.

mod chat;
mod inventory;
mod player;
mod scoreboard;
mod teleport;

use super::{InWorld, LoadingReason, Received};
use melon_assets::i18n::Lang;
use melon_gui::text::{StyledText, font::Font};
use melon_net::WorldEvent;
use melon_net::play::game::GameEvent;
use melon_render::Renderer;
use melon_world::game_mode::GameMode;
use melon_world::inventory::empty;

/// `ClientPacketListener.determineLevelLoadingReason`: the portal's
/// background unless the player died; the Nether wins over the End.
fn loading_reason(died: bool, new: &str, old: &str) -> LoadingReason {
    let either = |dimension: &str| new == dimension || old == dimension;
    if died {
        LoadingReason::Other
    } else if either("minecraft:the_nether") {
        LoadingReason::NetherPortal
    } else if either("minecraft:the_end") {
        LoadingReason::EndPortal
    } else {
        LoadingReason::Other
    }
}

impl InWorld {
    /// Applies what the server sent since last frame.
    pub fn receive(&mut self, renderer: &mut Renderer, lang: &Lang, font: &Font) -> Received {
        let mut received = Received::default();
        let mut loading = None;
        let mut events = self.joined.session.poll().into_iter();
        while let Some(event) = events.next() {
            match event {
                WorldEvent::Reconfiguring => loading = Some(LoadingReason::Other),
                WorldEvent::Rejoined(info, mut joined) => {
                    joined.extend(events.by_ref());
                    received.rejoined = Some((info, joined));
                    break;
                }
                WorldEvent::ChunkLoaded(chunk) => {
                    if !self.level.is_loaded_at(chunk.x * 16, chunk.z * 16) {
                        self.meshes.remove(renderer, (chunk.x, chunk.z));
                    }
                    self.level.insert(chunk);
                }
                WorldEvent::ChunkUnloaded(x, z) => {
                    self.meshes.remove(renderer, (x, z));
                    self.level.remove(x, z);
                }
                WorldEvent::BiomesChanged(columns) => {
                    for (x, z, biomes) in columns {
                        self.level.replace_biomes(x, z, biomes);
                    }
                }
                WorldEvent::LightUpdated { x, z, update } => {
                    self.level.apply_light(x, z, &update);
                }
                WorldEvent::Ping(id) => self.joined.session.send_pong(id),
                WorldEvent::Movement(payload) => self.apply_movement_payload(payload),
                WorldEvent::ServerChannels(channels) => self.server_channels.extend(channels),
                WorldEvent::PostEffects(ids) => self.post_effects = ids,
                WorldEvent::Teleport(teleport) => self.teleport(&teleport),
                WorldEvent::Game(GameEvent::Respawn {
                    dimension_id,
                    dimension_name,
                    game_mode,
                    previous_game_mode,
                    keep_entity_data,
                    keep_attributes,
                    hashed_seed,
                }) => {
                    // `handleRespawn`, in its order.
                    let died = self.status.health <= 0.0;
                    let reason = loading_reason(died, &dimension_name, &self.level_info.dimension);
                    self.respawn_in(renderer, dimension_id, dimension_name, hashed_seed);
                    self.close_container_for_respawn();
                    self.new_local_player(keep_entity_data, keep_attributes);
                    self.set_game_mode(game_mode, Some(previous_game_mode));
                    loading = Some(reason);
                    // The death screen closes with the new player.
                    received.revived |= died;
                }
                WorldEvent::Game(GameEvent::Login {
                    entity_id,
                    dimension_id,
                    dimension_name,
                    game_mode,
                    previous_game_mode,
                    hashed_seed,
                }) => {
                    // `handleLogin`: always a new level and a new player.
                    self.joined.session.info.entity_id = entity_id;
                    self.level_info.dimension.clear();
                    self.respawn_in(renderer, dimension_id, dimension_name, hashed_seed);
                    self.new_local_player(false, false);
                    self.set_game_mode(game_mode, Some(previous_game_mode));
                    loading = Some(LoadingReason::Other);
                }
                WorldEvent::Game(event) => {
                    self.apply_game_event(event, &mut received, lang, font);
                }
            }
        }
        self.report_loading(&mut received, loading);
        received
    }

    /// `handleRespawn`'s `oldPlayer.closeContainer()`: a menu the server
    /// opened is closed, and the server hears of it.
    fn close_container_for_respawn(&mut self) {
        let Some(menu) = self.menus.open_container.take() else {
            return;
        };
        self.joined.session.send_container_close(menu.id);
        self.inventory.set_carried(empty());
        self.menus.pending_container_overlay = false;
        self.menus.pending_container_close = true;
    }

    /// `handleRespawn` and `handleLogin` make a new `LocalPlayer`: what it
    /// keeps of itself between ticks starts over. Without `KEEP_ENTITY_DATA`
    /// its synced data is the defaults: full health (`LivingEntity` starts
    /// at `getMaxHealth()`, 20 before any attribute is sent), no absorption,
    /// a full breath.
    ///
    /// The new player has a new `Inventory`: empty until the server sends
    /// the menu again, hotbar slot 0. `MultiPlayerGameMode.carriedIndex`
    /// outlives the player, so the next `ensureHasSentCarriedItem` tells
    /// the server of slot 0 when it held another.
    fn new_local_player(&mut self, keep_entity_data: bool, keep_attributes: bool) {
        self.player.respawned(keep_entity_data, keep_attributes);
        self.post_effects.clear();
        self.hands.using = None;
        self.body.respawned(keep_entity_data);
        self.sent.respawned(keep_entity_data);
        self.audio.respawned();
        self.ambient.respawned();
        let held = self.inventory.selected();
        self.inventory = melon_world::inventory::Inventory::default();
        if held != 0 {
            self.joined.session.send_carried_item(0);
        }
        self.respawn_status(keep_entity_data);
    }

    /// `MultiPlayerGameMode.setLocalMode`: the game mode, its abilities and
    /// the HUD's copy.
    pub(super) fn set_game_mode(&mut self, mode: GameMode, previous: Option<Option<GameMode>>) {
        match previous {
            Some(previous) => self.game_mode.set_both(mode, previous),
            None => self.game_mode.set(mode),
        }
        self.status.game_mode = mode;
        self.player.set_game_mode(mode);
    }

    /// One `ClientPacketListener` handler per event, by group; the level's
    /// handlers live with what they change (sounds, particles, cracks).
    fn apply_game_event(
        &mut self,
        event: GameEvent,
        received: &mut Received,
        lang: &Lang,
        font: &Font,
    ) {
        match event {
            // Chat.
            GameEvent::Chat(event) => self.receive_chat(event, lang, font),
            GameEvent::Commands(tree) => self.commands = tree,
            GameEvent::NoRespawnBlock => self.no_respawn_block(lang, font),

            // Scoreboard and player list.
            GameEvent::PlayerInfo(updates) => self.update_player_info(updates),
            GameEvent::PlayersRemoved(removed) => self.remove_player_info(removed),
            GameEvent::Scores(packet) => self.apply_scores(packet),
            GameEvent::Boss(event) => self.hud.bosses.apply(event, lang),

            // Inventory and menus.
            GameEvent::ContainerContent {
                container,
                state_id,
                slots,
                carried,
            } => self.set_container_content(container, state_id, slots, carried),
            GameEvent::ContainerSlot {
                container,
                state_id,
                slot,
                item,
            } => self.set_container_slot(container, state_id, slot, item),
            GameEvent::ContainerData {
                container,
                id,
                value,
            } => self.set_container_data(container, id, value),
            GameEvent::OpenScreen {
                container,
                window_type,
                title,
            } => self.open_screen(container, window_type, &title, lang),
            GameEvent::OpenMount {
                container,
                columns,
                entity_id,
            } => self.open_mount_screen(container, columns, entity_id, lang),
            GameEvent::ContainerClose { .. } => self.close_container(),
            GameEvent::MerchantOffers { container, offers } => {
                self.set_merchant_offers(container, offers);
            }
            GameEvent::CursorItem(item) => self.set_cursor_item(item),
            GameEvent::PlayerInventorySlot { slot, item } => {
                self.set_player_inventory_slot(slot, item);
            }
            GameEvent::HeldSlot(slot) => self.set_held_slot(slot),
            GameEvent::Recipes(event) => self.apply_recipes(event),

            // The local player.
            GameEvent::Health {
                health,
                food,
                saturation,
            } => self.set_health(health, food, saturation, received),
            GameEvent::Experience {
                progress, level, ..
            } => {
                self.status.experience_progress = progress;
                self.status.experience_level = level;
            }
            GameEvent::GameMode(mode) => self.set_game_mode(mode, None),
            GameEvent::Abilities {
                flags,
                flying_speed,
                walking_speed,
            } => self
                .player
                .set_abilities(flags, flying_speed, walking_speed),
            GameEvent::PermissionLevel { entity_id, level } => {
                if entity_id == self.joined.session.info.entity_id {
                    self.permission_level = level;
                }
            }
            GameEvent::MobEffect {
                entity_id,
                effect,
                amplifier,
                duration,
                flags,
            } => self.update_mob_effect(entity_id, effect, Some((amplifier, duration, flags))),
            GameEvent::RemoveMobEffect { entity_id, effect } => {
                self.update_mob_effect(entity_id, effect, None);
            }
            GameEvent::EntityMotion {
                entity_id,
                velocity,
            } => self.set_entity_motion(entity_id, velocity),
            GameEvent::MoveVehicle { pos, yaw, pitch } => self.move_vehicle(pos, yaw, pitch),

            // The level.
            GameEvent::Entity(packet) => {
                if let melon_net::play::game::EntityPacket::Event { id, event } = &packet {
                    self.on_entity_event_particles(*id, *event);
                }
                self.receive_entity(packet);
                self.entities
                    .resolve_names(&|name| StyledText::from_component(name, lang).to_plain());
            }
            GameEvent::BlocksChanged(changes) => {
                for (pos, state) in changes {
                    self.probe_block_change(pos, state);
                    self.set_server_verified_block(pos, state);
                }
            }
            GameEvent::BlockChangedAck(sequence) => self.end_predictions_up_to(sequence),
            GameEvent::BlockEntityData { pos, head } => self.level.set_head(pos, head),
            GameEvent::BlockEvent { pos, action, param } => {
                self.block_entities.event(&self.level, pos, action, param);
            }
            GameEvent::BlockDestruction {
                breaker,
                pos,
                stage,
            } => {
                // Servers send no one their own cracks; drawn from the
                // local breaking anyway.
                if breaker != self.joined.session.info.entity_id {
                    self.cracks.set(breaker, pos, stage, self.ticks);
                }
            }
            GameEvent::Sound(sound) => self.on_server_sound(&sound),
            GameEvent::LevelEvent(event) => {
                self.level_event_particles(&event);
                self.on_level_event(event);
            }
            GameEvent::Explosion(explosion) => {
                if let Some(impulse) = explosion.knockback {
                    self.player.push(glam::DVec3::from_array(impulse));
                }
                self.explosion_particles(&explosion);
                self.on_explosion(&explosion);
            }
            GameEvent::Particles(packet) => self.on_level_particles(&packet),
            GameEvent::Animate { entity, action } => self.on_animate(entity, action),
            GameEvent::Time { game_time, clocks } => {
                self.clocks.handle_updates(game_time, &clocks);
            }
            GameEvent::RainLevel(level) => self.weather.set_rain(level),
            GameEvent::ThunderLevel(level) => self.weather.set_thunder(level),
            GameEvent::Border(update) => self.border.apply(update),
            GameEvent::Level(setting) => self.level_settings.apply(setting),

            // Handled in `receive`, or not at all.
            GameEvent::Respawn { .. } | GameEvent::Login { .. } | GameEvent::Ignored => {}
        }
    }

    /// The probe's note of a block changing near the player.
    fn probe_block_change(&self, pos: [i32; 3], state: u16) {
        if !melon_core::probe::enabled() {
            return;
        }
        let f = self.player.feet();
        let near = (f.x - f64::from(pos[0]) - 0.5).abs() < 4.0
            && (f.y - f64::from(pos[1])).abs() < 4.0
            && (f.z - f64::from(pos[2]) - 0.5).abs() < 4.0;
        let old = self.level.block_state(pos[0], pos[1], pos[2]);
        if !near || old == Some(state) {
            return;
        }
        let (t, n) = (self.clocks.game_time(), self.ticks);
        melon_core::probe::note("block.near", format!("{pos:?}/{t}/{n}"), || {
            format!(
                "old={old:?} new={state} feet=({:.2},{:.2},{:.2}) tick={t}",
                f.x, f.y, f.z
            )
        });
    }
}

#[cfg(test)]
mod respawn_tests {
    use super::*;

    #[test]
    fn the_loading_background_follows_determine_level_loading_reason() {
        let (over, nether, end) = (
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end",
        );
        assert_eq!(
            loading_reason(false, nether, over),
            LoadingReason::NetherPortal
        );
        assert_eq!(
            loading_reason(false, over, nether),
            LoadingReason::NetherPortal
        );
        assert_eq!(loading_reason(false, end, over), LoadingReason::EndPortal);
        assert_eq!(loading_reason(false, over, end), LoadingReason::EndPortal);
        // The Nether is asked first.
        assert_eq!(
            loading_reason(false, end, nether),
            LoadingReason::NetherPortal
        );
        // Dying, or a dimension without a portal, shows the plain one.
        assert_eq!(loading_reason(true, nether, over), LoadingReason::Other);
        assert_eq!(loading_reason(false, over, over), LoadingReason::Other);
        assert_eq!(
            loading_reason(false, "plugin:arena", over),
            LoadingReason::Other
        );
    }
}
