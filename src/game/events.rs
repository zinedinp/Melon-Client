//! Applying what the server sent: chunks, teleports, chat, game mode and

use super::{InWorld, LoadingReason, Received, melon_yaw, minecraft_yaw};
use crate::gui::text::{StyledText, font::Font};
use crate::i18n::Lang;
use crate::network::play::chat::ChatEvent;
use crate::network::play::game::recipes::{FLAG_HIGHLIGHT, RecipeEvent};
use crate::network::play::game::scores::ScorePacket;
use crate::network::play::game::{DataValue, EntityFlags, FLAG_ON_FIRE, GameEvent, NAUSEA_ID};
use crate::network::{Teleport, WorldEvent};
use crate::renderer::Renderer;
use crate::world::entity::kind::EntityKind;
use crate::world::game_mode::GameMode;
use crate::world::inventory::{ContainerKind, MountLook, OpenContainer, empty};
use glam::DVec3;

/// `Attributes.MAX_HEALTH`'s default: a new player's health.
const DEFAULT_MAX_HEALTH: f32 = 20.0;

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
    /// Applies what the server sent since last frame: chunks, teleports and
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

    pub(super) fn receive_chat(&mut self, event: ChatEvent, lang: &Lang, font: &Font) {
        if let ChatEvent::Suggestions(suggestions) = event {
            if let Some(overlay) = &mut self.overlay {
                overlay.receive_suggestions(suggestions, lang);
            }
            return;
        }
        match event {
            ChatEvent::Title(c) => {
                self.hud.set_title(StyledText::from_component(&c, lang));
                return;
            }
            ChatEvent::Subtitle(c) => {
                self.hud.set_subtitle(StyledText::from_component(&c, lang));
                return;
            }
            ChatEvent::TitleTimes {
                fade_in,
                stay,
                fade_out,
            } => {
                self.hud.set_title_times(fade_in, stay, fade_out);
                return;
            }
            ChatEvent::ClearTitles { reset } => {
                self.hud.clear_titles(reset);
                return;
            }
            _ => {}
        }
        if let Some(message) = self.chat.receive(event, lang, font) {
            self.hud.set_overlay_message(message);
        }
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
    fn new_local_player(&mut self, keep_entity_data: bool, keep_attributes: bool) {
        self.player.respawned(keep_entity_data, keep_attributes);
        self.hands.using = None;
        self.body.respawned(keep_entity_data);
        self.sent.respawned(keep_entity_data);
        self.audio.respawned();
        self.ambient.respawned();
        if !keep_entity_data {
            self.status.health = DEFAULT_MAX_HEALTH;
            self.status.absorption = 0.0;
            self.status.air = crate::gui::hud::bars::air::MAX_AIR;
        }
    }

    /// `ClientPacketListener.handleMovePlayer`, with its relative flags for
    pub(super) fn teleport(&mut self, t: &Teleport) {
        self.predictions.on_teleport();
        self.positioned_in_new_level();
        if self.entities.own_vehicle().is_none() {
            let feet = self.player.feet();
            let old_yaw = minecraft_yaw(self.player.yaw());
            let old_pitch = self.player.pitch();
            let new_yaw = if t.relative & Teleport::YAW != 0 {
                old_yaw + t.yaw
            } else {
                t.yaw
            };
            let new_pitch = if t.relative & Teleport::PITCH != 0 {
                old_pitch + t.pitch
            } else {
                t.pitch
            }
            .clamp(-90.0, 90.0);
            let mut velocity = self.player.velocity();
            if t.relative & Teleport::ROTATE_DELTA != 0 {
                velocity = rotate_motion(velocity, old_yaw - new_yaw, old_pitch - new_pitch);
            }
            let (x, y, z) = t.pos;
            if crate::probe::enabled() {
                let (from, rel) = (feet, t.relative);
                crate::probe::note(
                    "teleport",
                    format!(
                        "to=({x:.2},{y:.2},{z:.2}) {}",
                        crate::probe::relative_flags(rel)
                    ),
                    || {
                        format!(
                            "id={} from={from:?} delta={:?} dist={:.4} on_ground={} handler=game-thread apply then accept_teleportation",
                            t.id,
                            t.delta,
                            from.distance(DVec3::new(x, y, z)),
                            self.player.on_ground()
                        )
                    },
                );
                let now = self.clocks.game_time();
                let last = super::tick::LAST_HOLD.swap(now, std::sync::atomic::Ordering::Relaxed);
                if rel & 0x7 == 0 && (now - last).abs() > 100 {
                    super::tick::HOLD_TRACE_UNTIL
                        .store(now + 30, std::sync::atomic::Ordering::Relaxed);
                    let level = &self.level;
                    crate::probe::note("hold", format!("{now}"), || {
                        let (bx, by, bz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
                        let mut cells = String::new();
                        for dy in (-1..=3).rev() {
                            for dz in -1..=1 {
                                for dx in -1..=1 {
                                    let at = (bx + dx, by + dy, bz + dz);
                                    let id =
                                        crate::world::physics::block_at(level, at.0, at.1, at.2);
                                    if id == 0 {
                                        continue;
                                    }
                                    let boxes: Vec<String> =
                                        crate::world::physics::block_shapes(level, at)
                                            .map(|b| format!("y{:.3}..{:.3}", b.min.y, b.max.y))
                                            .collect();
                                    cells.push_str(&format!("\n  {at:?} state={id} {boxes:?}"));
                                }
                            }
                        }
                        format!(
                            "packets of the last 5 s: {}\nwe sent (last 5 s): {}\nour last moves:{}\nmove.log:{}\nblocks around the pin ({x:.2},{y:.2},{z:.2}), y-1..head+1:{cells}",
                            crate::probe::recent_packets(5),
                            crate::probe::recent_sent(5),
                            crate::probe::recent_moves(),
                            crate::probe::recent_io()
                        )
                    });
                }
            }
            self.player.teleport(
                DVec3::new(
                    t.resolve(Teleport::X, feet.x, x),
                    t.resolve(Teleport::Y, feet.y, y),
                    t.resolve(Teleport::Z, feet.z, z),
                ),
                DVec3::new(
                    t.resolve(Teleport::DELTA_X, velocity.x, t.delta[0]),
                    t.resolve(Teleport::DELTA_Y, velocity.y, t.delta[1]),
                    t.resolve(Teleport::DELTA_Z, velocity.z, t.delta[2]),
                ),
                DVec3::new(
                    t.resolve(Teleport::X, self.player.prev_feet().x, x),
                    t.resolve(Teleport::Y, self.player.prev_feet().y, y),
                    t.resolve(Teleport::Z, self.player.prev_feet().z, z),
                ),
            );
            self.player.settle_after_teleport(&self.level);
            self.player.set_rotation(melon_yaw(new_yaw), new_pitch);
        }
        if !t.acked {
            let feet = self.player.feet();
            self.joined.session.send_accept_teleport(
                t.id,
                (feet.x, feet.y, feet.z),
                minecraft_yaw(self.player.yaw()),
                self.player.pitch(),
            );
        }
        self.sent.input_dirty = true;
        self.abort_destroy();
    }

    /// `MultiPlayerGameMode.setLocalMode`: the HUD, the abilities and the
    pub(super) fn set_game_mode(&mut self, mode: GameMode, previous: Option<Option<GameMode>>) {
        match previous {
            Some(previous) => self.game_mode.set_both(mode, previous),
            None => self.game_mode.set(mode),
        }
        self.status.game_mode = mode;
        self.player.set_game_mode(mode);
    }

    pub(super) fn apply_game_event(
        &mut self,
        event: GameEvent,
        received: &mut Received,
        lang: &Lang,
        font: &Font,
    ) {
        match event {
            GameEvent::Health {
                health,
                food,
                saturation,
            } => {
                let was_dead = self.status.health <= 0.0;
                self.status.health = health;
                self.status.food = food;
                self.status.saturation = saturation;
                received.died |= health <= 0.0 && !was_dead;
                received.revived |= health > 0.0 && was_dead;
            }
            GameEvent::Experience {
                progress, level, ..
            } => {
                self.status.experience_progress = progress;
                self.status.experience_level = level;
            }
            GameEvent::ContainerContent {
                container: 0,
                state_id,
                slots,
                carried,
            } => {
                let keep_cursor = self.game_mode.current.has_infinite_materials();
                let cursor = self.inventory.carried().clone();
                self.inventory.set_all(state_id, slots, carried);
                if keep_cursor {
                    self.inventory.set_carried(cursor);
                }
            }
            GameEvent::ContainerContent {
                container,
                state_id,
                slots,
                carried,
            } => {
                if self
                    .menus
                    .open_container
                    .as_ref()
                    .is_some_and(|menu| menu.id == container)
                {
                    let menu = self.menus.open_container.as_mut().unwrap();
                    menu.set_all(state_id, slots);
                    menu.copy_player_into(&mut self.inventory);
                    self.inventory.set_carried(carried);
                }
            }
            GameEvent::OpenScreen {
                container,
                window_type,
                title,
            } => {
                if self.menus.open_container.is_some() {
                    self.inventory.set_carried(empty());
                }
                let kind = ContainerKind::from_window_type(window_type);
                let title = StyledText::from_component(&title, lang);
                let mut menu = OpenContainer::new(container, kind, title);
                menu.copy_player_from(&self.inventory);
                self.menus.open_container = Some(menu);
                self.menus.pending_container_close = false;
                self.menus.pending_container_overlay = true;
            }
            GameEvent::ContainerClose { .. } => {
                if self.menus.open_container.is_some() {
                    self.inventory.set_carried(empty());
                }
                self.menus.open_container = None;
                self.menus.pending_container_overlay = false;
                self.menus.pending_container_close = true;
            }
            GameEvent::OpenMount {
                container,
                columns,
                entity_id,
            } => {
                if self.menus.open_container.is_some() {
                    self.inventory.set_carried(empty());
                }
                let title =
                    self.entities
                        .get(entity_id)
                        .map(|mount| match &mount.custom_name {
                            Some(name) => StyledText::from_component(name, lang),
                            None => StyledText::plain(lang.get_dynamic(&format!(
                                "entity.minecraft.{}",
                                mount.ty.resource_name
                            ))),
                        })
                        .unwrap_or_default();
                let mount = self.entities.get(entity_id).map(|mount| mount.kind);
                let (saddle, armor) = mount.map_or((false, false), |kind| kind.mount_slots());
                let kind = ContainerKind::Mount {
                    columns: columns.clamp(0, 9) as u8,
                    look: MountLook {
                        entity: entity_id,
                        saddle,
                        armor,
                        nautilus: mount == Some(EntityKind::Nautilus),
                    },
                };
                let mut menu = OpenContainer::new(container, kind, title);
                menu.copy_player_from(&self.inventory);
                self.menus.open_container = Some(menu);
                self.menus.pending_container_close = false;
                self.menus.pending_container_overlay = true;
            }
            GameEvent::ContainerSlot {
                container,
                state_id,
                slot,
                item,
            } => match container {
                0 => self.inventory.set_slot(state_id, slot, item),
                -1 if slot == -1 => self.inventory.set_carried(item),
                id => {
                    if self
                        .menus
                        .open_container
                        .as_ref()
                        .is_some_and(|menu| menu.id == id)
                    {
                        let menu = self.menus.open_container.as_mut().unwrap();
                        menu.set_slot(state_id, slot, item);
                        menu.copy_player_into(&mut self.inventory);
                    }
                }
            },
            GameEvent::ContainerData {
                container,
                id,
                value,
            } => {
                if let Some(menu) = self
                    .menus
                    .open_container
                    .as_mut()
                    .filter(|menu| menu.id == container)
                    && let Ok(id) = usize::try_from(id)
                {
                    menu.set_data(id, value);
                }
            }
            GameEvent::MerchantOffers { container, offers } => {
                if let Some(menu) = self
                    .menus
                    .open_container
                    .as_mut()
                    .filter(|menu| menu.id == container)
                {
                    menu.trades = Some(offers);
                }
            }
            GameEvent::CursorItem(item) => self.inventory.set_carried(item),
            GameEvent::PlayerInventorySlot { slot, item } => {
                self.inventory.set_inventory_slot(slot, item);
            }
            GameEvent::HeldSlot(slot) => {
                if (0..9).contains(&slot) {
                    self.inventory.select(slot as u8);
                }
            }
            GameEvent::MoveVehicle { pos, yaw, pitch } => {
                // `handleMoveVehicle` answers at once with where it is now.
                if self
                    .entities
                    .snap_driven(glam::DVec3::from_array(pos), yaw, pitch)
                {
                    self.send_move_vehicle();
                }
            }
            GameEvent::GameMode(mode) => {
                self.set_game_mode(mode, None);
            }
            GameEvent::Abilities {
                flags,
                flying_speed,
                walking_speed,
            } => {
                self.player
                    .set_abilities(flags, flying_speed, walking_speed);
            }
            GameEvent::PermissionLevel { entity_id, level } => {
                if entity_id == self.joined.session.info.entity_id {
                    self.permission_level = level;
                }
            }
            GameEvent::PlayerInfo(updates) => {
                for update in updates {
                    let entry = self.players.entry(update.uuid).or_default();
                    if let Some(name) = update.name {
                        entry.name = name;
                    }
                    if let Some(mode) = update.game_mode {
                        entry.game_mode = mode;
                    }
                    if let Some(listed) = update.listed {
                        entry.listed = listed;
                    }
                    if let Some(latency) = update.latency {
                        entry.latency = latency;
                    }
                    if let Some(name) = update.display_name {
                        entry.display_name = name;
                    }
                    if let Some(order) = update.list_order {
                        entry.list_order = order;
                    }
                    if let Some(hat) = update.show_hat {
                        entry.show_hat = hat;
                    }
                    if update.textures.is_some() {
                        entry.textures = update.textures;
                    }
                }
            }
            GameEvent::Scores(packet) => match packet {
                ScorePacket::TabList { header, footer } => {
                    self.tab_list.set_header_footer(header, footer);
                }
                ScorePacket::Scoreboard(update) => self.scoreboard.apply(update),
            },
            GameEvent::PlayersRemoved(removed) => {
                for uuid in removed {
                    self.players.remove(&uuid);
                }
            }
            GameEvent::EntityMotion {
                entity_id,
                velocity,
            } => {
                let own = self.joined.session.info.entity_id;
                if crate::probe::enabled() && entity_id == own {
                    crate::probe::trace_io(
                        false,
                        "set_entity_motion",
                        format!(
                            "own velocity=({:.5},{:.5},{:.5}) handler=game-thread lerpMotion/setDeltaMovement",
                            velocity[0], velocity[1], velocity[2]
                        ),
                    );
                }
                if entity_id == own {
                    self.player.set_velocity(DVec3::from_array(velocity));
                } else {
                    self.entities.apply(
                        crate::network::play::game::EntityPacket::Motion {
                            id: entity_id,
                            velocity,
                        },
                        own,
                    );
                }
            }
            GameEvent::Sound(sound) => self.on_server_sound(&sound),
            GameEvent::LevelEvent(event) => {
                self.level_event_particles(&event);
                self.on_level_event(event);
            }
            GameEvent::Explosion(explosion) => {
                if let Some(impulse) = explosion.knockback {
                    self.player.push(DVec3::from_array(impulse));
                }
                self.explosion_particles(&explosion);
                self.on_explosion(&explosion);
            }
            GameEvent::Particles(packet) => self.on_level_particles(&packet),
            GameEvent::Animate { entity, action } => self.on_animate(entity, action),
            GameEvent::BlocksChanged(changes) => {
                for (pos, state) in changes {
                    if crate::probe::enabled() {
                        let f = self.player.feet();
                        if (f.x - f64::from(pos[0]) - 0.5).abs() < 4.0
                            && (f.y - f64::from(pos[1])).abs() < 4.0
                            && (f.z - f64::from(pos[2]) - 0.5).abs() < 4.0
                        {
                            let old = self.level.block_state(pos[0], pos[1], pos[2]);
                            if old != Some(state) {
                                let (t, n) = (self.clocks.game_time(), self.ticks);
                                crate::probe::note(
                                    "block.near",
                                    format!("{pos:?}/{t}/{n}"),
                                    || {
                                        format!(
                                            "old={old:?} new={state} feet=({:.2},{:.2},{:.2}) tick={t}",
                                            f.x, f.y, f.z
                                        )
                                    },
                                );
                            }
                        }
                    }
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
                if breaker != self.joined.session.info.entity_id {
                    self.cracks.set(breaker, pos, stage, self.ticks);
                }
            }
            GameEvent::Respawn { .. } | GameEvent::Login { .. } => {}
            GameEvent::Commands(tree) => self.commands = tree,
            GameEvent::Chat(event) => self.receive_chat(event, lang, font),
            GameEvent::Time { game_time, clocks } => {
                self.clocks.handle_updates(game_time, &clocks);
            }
            GameEvent::NoRespawnBlock => {
                let text = lang.get_dynamic("block.minecraft.spawn.not_valid");
                self.hud.set_overlay_message(StyledText::plain(text));
            }
            GameEvent::Ignored => {}
            GameEvent::Boss(event) => self.hud.bosses.apply(event, lang),
            GameEvent::Border(update) => self.border.apply(update),
            GameEvent::Recipes(event) => self.apply_recipes(event),
            GameEvent::Level(setting) => self.level_settings.apply(setting),
            GameEvent::RainLevel(level) => self.weather.set_rain(level),
            GameEvent::ThunderLevel(level) => self.weather.set_thunder(level),
            GameEvent::Entity(packet) => {
                if let crate::network::play::game::EntityPacket::Event { id, event } = &packet {
                    self.on_entity_event_particles(*id, *event);
                }
                self.receive_entity(packet);
                self.entities
                    .resolve_names(&|name| StyledText::from_component(name, lang).to_plain());
            }
            GameEvent::MobEffect {
                entity_id,
                effect,
                amplifier,
                duration,
                flags,
            } => {
                if entity_id == self.joined.session.info.entity_id {
                    if effect == NAUSEA_ID {
                        self.effects.set_nausea_ticks(duration);
                    }
                    self.mining_effects.set(effect, Some(amplifier));
                    self.status_effects.set(effect, amplifier, duration, flags);
                }
            }
            GameEvent::RemoveMobEffect { entity_id, effect } => {
                if entity_id == self.joined.session.info.entity_id {
                    if effect == NAUSEA_ID {
                        self.effects.set_nausea_ticks(0);
                    }
                    self.mining_effects.set(effect, None);
                    self.status_effects.remove(effect);
                }
            }
        }
    }

    /// The local player's own entity data: on fire, using an item.
    pub(super) fn own_entity_data(&mut self, values: &[(u8, DataValue)]) {
        let flags = EntityFlags::of(values);
        if let Some(shared) = flags.shared {
            self.effects.set_on_fire(shared & (1 << FLAG_ON_FIRE) != 0);
            self.player.set_gliding(shared & (1 << 7) != 0);
        }
        if let Some(living) = flags.living {
            self.sync_using_item(living);
            if living & (1 << 2) == 0 {
                self.player.end_spin_attack();
            }
        }
    }
}

impl InWorld {
    /// `ClientPacketListener`'s handlers for the recipe packets.
    fn apply_recipes(&mut self, event: RecipeEvent) {
        match event {
            RecipeEvent::Update(sets) => self.recipe_sets = sets,
            RecipeEvent::BookAdd { entries, replace } => {
                if replace {
                    self.recipes.clear();
                }
                for (entry, flags) in entries {
                    if flags & FLAG_HIGHLIGHT != 0 {
                        self.recipes.add_highlight(entry.id);
                    }
                    self.recipes.add(entry);
                }
                self.recipes.rebuild();
            }
            RecipeEvent::BookRemove(ids) => {
                for id in ids {
                    self.recipes.remove(id);
                }
                self.recipes.rebuild();
            }
            RecipeEvent::BookSettings(settings) => self.recipes.set_settings(settings),
            RecipeEvent::Ghost { container, display } => {
                self.menus.ghost_recipe = Some((container, display));
            }
        }
    }
}

/// `Vec3.xRot(pitch).yRot(yaw)`, the angles in degrees: a motion turned
fn rotate_motion(v: DVec3, yaw: f32, pitch: f32) -> DVec3 {
    let (sin, cos) = f64::from(pitch).to_radians().sin_cos();
    let v = DVec3::new(v.x, v.y * cos + v.z * sin, v.z * cos - v.y * sin);
    let (sin, cos) = f64::from(yaw).to_radians().sin_cos();
    DVec3::new(v.x * cos + v.z * sin, v.y, v.z * cos - v.x * sin)
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

#[cfg(test)]
mod rotate_tests {
    use super::*;

    #[test]
    fn a_motion_turns_with_the_look() {
        let v = rotate_motion(DVec3::new(0.0, 0.0, 1.0), 90.0, 0.0);
        assert!((v.x - 1.0).abs() < 1e-12 && v.z.abs() < 1e-12);
        assert_eq!(rotate_motion(DVec3::Y, 0.0, 0.0), DVec3::Y);
    }
}
