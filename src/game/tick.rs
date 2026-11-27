//! The client tick.

use super::position_report::Report;
use super::{InWorld, controls, keys, minecraft_yaw};
use melon_gui::hud::item_slot::{self};
use melon_gui::overlay::inventory::InventoryOverlay;
use melon_gui::overlay::{ChatExit, OverlayKind};
use melon_net::play::game::PlayerCommand;
use melon_world::player::{self};

impl InWorld {
    /// Runs the ticks that fit into `dt` seconds (`Minecraft.tick` via its
    /// timer). While `paused`, only what vanilla ticks then: the screens and
    /// the chat, not the level, and the partial tick holds.
    pub fn advance(
        &mut self,
        dt: f32,
        blocked: bool,
        paused: bool,
        ui: &controls::WorldContext,
    ) -> u32 {
        let mut input = if blocked {
            player::MoveInput::default()
        } else {
            self.keys.movement()
        };
        // `DeltaTracker.updatePauseState`.
        match (self.paused_at, paused) {
            (None, true) => self.paused_at = Some(self.partial_tick()),
            (Some(at), false) => {
                self.paused_at = None;
                self.tick_time = at * self.level_settings.tick_seconds();
            }
            _ => {}
        }
        self.tick_time += dt;
        let mut ticks = 0;
        // `DeltaTracker.advanceGameTime` against `getTickTargetMillis`.
        while self.tick_time >= self.level_settings.tick_seconds() {
            self.tick_time -= self.level_settings.tick_seconds();
            ticks += 1;
            if ticks > 10 {
                self.tick_time = 0.0;
                break;
            }
            if paused {
                self.tick_paused(ui);
                continue;
            }
            if let Some(driver) = &mut self.bench {
                let stats = self.level.mesh_stats();
                let load = super::bench::LoadState {
                    chunks: self.level.chunk_count(),
                    meshing: stats.waiting + stats.in_flight,
                };
                let step = driver.tick(&mut self.player, load);
                input = step.input;
                self.audio.extend(step.sound);
                for command in &step.commands {
                    self.joined.session.send_command(command);
                }
                if step.find_forest {
                    let level = &self.level;
                    let spot = super::bench::forest_spot(
                        self.player.feet(),
                        |x, z| {
                            level
                                .is_loaded_at(x, z)
                                .then(|| level.motion_blocking_height(x, z))
                        },
                        |x, y, z| level.block_state(x, y, z).unwrap_or(0),
                    );
                    match spot {
                        Some(at) => {
                            let tp = format!("tp {:.1} {:.1} {:.1}", at.x, at.y, at.z);
                            self.joined.session.send_command(&tp);
                        }
                        None => {
                            melon_base::warn!("Bench: no trees nearby, the forest phase stays put")
                        }
                    }
                }
            }
            self.tick(input, ui, !blocked && self.bench.is_none());
        }
        if !paused {
            self.player.update_render(self.partial_tick(), dt);
        }
        ticks.min(10)
    }

    /// `Minecraft.tick` while paused: the open screen and the chat only.
    fn tick_paused(&mut self, ui: &controls::WorldContext) {
        if let Some(actions) = self.with_overlay(ui, |overlay, ctx| overlay.tick(ctx)) {
            self.apply_overlay_actions(actions, ui);
        }
        self.interaction.miss_time = super::interaction::SCREEN_MISS_TIME;
        self.chat.tick(ui.font);
    }

    /// `DeltaTracker.getGameTimeDeltaPartialTick(true)`: how far into the
    /// next tick this frame is, at the level's tick length.
    pub(super) fn partial_tick(&self) -> f32 {
        if let Some(at) = self.paused_at {
            return at;
        }
        (self.tick_time / self.level_settings.tick_seconds()).min(1.0)
    }

    /// `getGameTimeDeltaPartialTick(false)`: 1 while the level is frozen,
    /// for what freezes with it (block entities).
    pub(super) fn level_partial_tick(&self) -> f32 {
        if self.level_settings.runs_normally() {
            self.partial_tick()
        } else {
            1.0
        }
    }

    pub(super) fn tick(
        &mut self,
        input: player::MoveInput,
        ui: &controls::WorldContext,
        interact: bool,
    ) {
        let lang = ui.lang;
        // `Minecraft.tick`: the level's tick rate first.
        self.level_settings.tick_rate_tick();
        let runs = self.level_settings.runs_normally();
        self.player
            .set_item_use(self.hands.using.map(|using| using.item_use));
        self.status_effects.tick();
        // `ClientLevel.tick`: border, time and cracks stand while frozen.
        if runs {
            self.border.tick();
        }
        self.player
            .set_speed_effects(self.status_effects.speed_factor());
        {
            use melon_world::player::status_effects::{
                DOLPHINS_GRACE_ID, JUMP_BOOST_ID, SLOW_FALLING_ID,
            };
            let effects = &self.status_effects;
            self.player.set_physics_effects(
                effects.get(SLOW_FALLING_ID).is_some(),
                effects
                    .get(JUMP_BOOST_ID)
                    .map_or(0, |e| e.amplifier as u32 + 1),
                effects.get(DOLPHINS_GRACE_ID).is_some(),
            );
        }
        let client_loaded = self.loading.is_none();
        let steering = self.steering();
        if client_loaded && steering.is_none() {
            let own = self.player.collision_box();
            let near = own.deflate(-3.0);
            let colliders = self.player.colliders_buffer();
            self.entities
                .colliders_near_into(&near, own.min.y, colliders);
            if !self.game_mode.current.is_spectator() {
                colliders.extend(self.border.collision_boxes(&own));
            }
            self.player.set_glider_worn(self.glider_worn());
            self.player.tick(
                input,
                &self.level,
                self.status.food,
                u32::from(ui.config.input.sprint_window),
            );
            self.report_started_moves();
            self.after_airborne_tick();
            self.tick_footsteps();
        }

        self.apply_pending_container_overlay(ui);
        if let Some(actions) = self.with_overlay(ui, |overlay, ctx| overlay.tick(ctx)) {
            self.apply_overlay_actions(actions, ui);
        }
        if self.interaction.right_click_delay > 0 {
            self.interaction.right_click_delay -= 1;
        }
        if interact && self.overlay.is_none() {
            // `Minecraft.tick`: the keys first, then the rest of the miss.
            self.handle_keybinds(ui);
            if self.interaction.miss_time > 0 {
                self.interaction.miss_time -= 1;
            }
        } else {
            self.interaction.miss_time = super::interaction::SCREEN_MISS_TIME;
            self.abort_destroy();
            self.release_using_item();
        }
        let off = &self.inventory.slots()[melon_world::inventory::OFFHAND_SLOT];
        self.hands.tick(self.inventory.selected_stack(), off);
        if self
            .hands
            .using
            .is_some_and(|using| using.overrun >= melon_world::player::hands::OVERRUN_TICKS)
        {
            self.release_using_item();
        }
        self.play_consume_sound();
        self.tick_ambient_sounds(ui);
        self.ticks += 1;
        self.update_own_crack();
        if runs {
            self.cracks.tick(self.ticks);
            // `Minecraft.tick`: `animateTick` and the particle engine.
            self.tick_particles(ui);
            self.clocks.tick();
        }
        self.tick_water_vision();
        self.tick_effects();
        self.tick_fov(ui.config.fov_effect_scale as f32);
        self.tick_loading();
        if self.player.take_abilities_changed() {
            self.joined
                .session
                .send_player_abilities(self.player.abilities().packet_flags());
        }
        let feet = self.player.feet().as_vec3();
        if runs {
            // `Level.tickBlockEntities` runs its tickers only then.
            self.block_entities.tick(&self.level, feet);
        }
        self.entities
            .set_piglins_zombify(self.level_info.dimension_type.piglins_zombify());
        self.entities.tick(runs);
        self.tick_pickups();
        self.push_player();
        self.entities.simulate(&self.level);
        self.tick_ride(input);
        self.tick_vitals();
        self.body.hurt = (self.body.hurt - 1).max(0);
        if self.body.death > 0 && self.status.health > 0.0 {
            self.status_effects.clear();
            self.mining_effects = Default::default();
            self.effects.set_nausea_ticks(0);
        }
        if self.body.death == 0 && self.status.health <= 0.0 {
            self.play_own_voice("entity.player.death");
        }
        self.body.death = if self.status.health <= 0.0 {
            (self.body.death + 1).min(melon_world::entity::tracker::DEATH_DURATION)
        } else {
            0
        };
        self.entities.update_water(&self.level);
        self.entities.update_anger(self.clocks.game_time());
        if self.loading.is_none() {
            self.send_input(input);
            self.send_position();
        }
        let selected = self.inventory.selected_stack();
        self.hud.tick(selected, &self.status, || {
            item_slot::styled_hover_name(lang, selected)
        });
        self.chat.tick(ui.font);
        if let Some(offset) = self.chat.take_ack() {
            self.joined.session.send_chat_ack(offset);
        }
        self.follow_game_mode(ui);
        self.joined.session.send_client_tick_end();
    }

    /// What the entities around shove the player by (`pushEntities` of `LivingEntity`).
    fn push_player(&mut self) {
        if self.player.riding() || !self.player.is_pushable() || self.status.health <= 0.0 {
            return;
        }
        let own_uuid = self.joined.session.info.uuid;
        let scoreboard = &self.scoreboard;
        let players = &self.players;
        let team_of = |name: &str| {
            scoreboard
                .team_of(name)
                .map(|(team, t)| (team.to_owned(), t.look.collision))
        };
        let own_team = players.get(&own_uuid).and_then(|p| team_of(&p.name));
        let impulse =
            self.entities
                .push_on_player(&self.player.collision_box(), |uuid, is_player| {
                    let name = if is_player {
                        match players.get(&uuid) {
                            Some(p) => p.name.clone(),
                            None => return true,
                        }
                    } else {
                        uuid.to_string()
                    };
                    let theirs = team_of(&name);
                    melon_world::scoreboard::team_allows_push(
                        own_team.as_ref().map(|(n, r)| (n.as_str(), *r)),
                        theirs.as_ref().map(|(n, r)| (n.as_str(), *r)),
                    )
                });
        if impulse != glam::DVec3::ZERO {
            self.player.push(impulse);
        }
    }

    /// `InventoryScreen.containerTick` and its creative twin.
    pub(super) fn follow_game_mode(&mut self, ui: &controls::WorldContext) {
        let creative = self.game_mode.current.has_infinite_materials();
        let spectator = self.game_mode.current.is_spectator();
        match self.overlay_kind() {
            Some(OverlayKind::Inventory) if creative => {
                let screen = self.creative_overlay(ui);
                self.open_overlay(Box::new(screen), ui);
            }
            Some(OverlayKind::Creative) if spectator => self.close_overlay(ui),
            Some(OverlayKind::Creative) if !creative => {
                self.with_overlay(ui, |overlay, ctx| {
                    overlay.closed(ChatExit::Interrupted, ctx);
                });
                let (width, height) = ui.size;
                let screen = InventoryOverlay::new(ui.lang, width, height);
                self.open_overlay(Box::new(screen), ui);
            }
            Some(OverlayKind::Inventory) if spectator => self.close_overlay(ui),
            _ => {}
        }
    }

    /// `LocalPlayer.sendIsSprintingIfNeeded` and the input packet.
    pub(super) fn send_input(&mut self, input: player::MoveInput) {
        let keys = keys::player_input(input);
        if keys != self.sent.input || self.sent.input_dirty {
            self.sent.input = keys;
            self.joined.session.send_player_input(keys);
        }
        self.sent.input_dirty = false;
        let sprinting = self.player.sprinting();
        if sprinting != self.sent.sprinting {
            self.sent.sprinting = sprinting;
            self.joined.session.send_player_command(if sprinting {
                PlayerCommand::StartSprinting
            } else {
                PlayerCommand::StopSprinting
            });
        }
    }

    /// `LocalPlayer.sendPosition`: every tick, so the server follows the
    /// `melon:movement`'s `Start` for each mod state that began, ahead of
    /// the move it makes, so the server's checks already grant it.
    fn report_started_moves(&mut self) {
        use pumpkin_protocol::java::melon_movement::{CHANNEL, MovementPayload};
        for id in self.player.take_started_moves() {
            let start = MovementPayload::Start { id: id.to_owned() };
            self.joined
                .session
                .send_custom_payload(CHANNEL, &start.encode());
        }
    }

    /// `melon:movement` from the server.
    pub(super) fn apply_movement_payload(
        &mut self,
        payload: pumpkin_protocol::java::melon_movement::MovementPayload,
    ) {
        use pumpkin_protocol::java::melon_movement::MovementPayload;
        match payload {
            MovementPayload::Allowed(ids) => self.player.set_allowed_moves(ids),
            MovementPayload::Started { entity_id, id } => {
                self.entities.start_custom_move(entity_id, id);
            }
            MovementPayload::Start { .. } => {}
        }
    }

    pub(super) fn send_position(&mut self) {
        if melon_core::probe::enabled()
            && self.clocks.game_time()
                <= HOLD_TRACE_UNTIL.load(std::sync::atomic::Ordering::Relaxed)
        {
            let now = self.clocks.game_time();
            let (feet, v) = (self.player.feet(), self.player.velocity());
            let (g, w) = (self.player.on_ground(), self.player.horizontal_collision());
            let loaded = self
                .level
                .is_loaded_at(feet.x.floor() as i32, feet.z.floor() as i32);
            let (loading, vehicle, riding) = (
                self.loading.is_some(),
                self.entities.own_vehicle().is_some(),
                self.player.riding(),
            );
            let ticks = self.ticks;
            melon_core::probe::note("hold.trace", format!("{now}/{ticks}"), || {
                format!(
                    "feet=({:.4},{:.4},{:.4}) velocity=({:.4},{:.4},{:.4}) on_ground={g} wall={w} chunk_loaded={loaded} loading={loading} vehicle={vehicle} riding={riding}",
                    feet.x, feet.y, feet.z, v.x, v.y, v.z
                )
            });
        }
        if melon_core::probe::enabled() {
            let (f, v) = (self.player.feet(), self.player.velocity());
            melon_core::probe::trace_move(format!(
                "{} feet=({:.3},{:.3},{:.3}) v=({:.3},{:.3},{:.3}) ground={} sprint={} sneak={}",
                self.ticks,
                f.x,
                f.y,
                f.z,
                v.x,
                v.y,
                v.z,
                self.player.on_ground(),
                self.player.sprinting(),
                self.keys.sneak,
            ));
        }
        if self.player.riding() {
            self.joined.session.send_rotation(
                minecraft_yaw(self.player.yaw()),
                self.player.pitch(),
                false,
                false,
            );
            self.send_move_vehicle();
            return;
        }
        let feet = self.player.feet();
        let yaw = minecraft_yaw(self.player.yaw());
        let pitch = self.player.pitch();
        let on_ground = self.player.on_ground();
        let wall = self.player.horizontal_collision();
        let session = &self.joined.session;
        match self.sent.position.tick(feet, yaw, pitch, on_ground, wall) {
            Some(Report::PosRot) => {
                session.send_position((feet.x, feet.y, feet.z), yaw, pitch, on_ground, wall);
            }
            Some(Report::Pos) => {
                session.send_position_only((feet.x, feet.y, feet.z), on_ground, wall)
            }
            Some(Report::Rot) => session.send_rotation(yaw, pitch, on_ground, wall),
            Some(Report::StatusOnly) => session.send_status_only(on_ground, wall),
            None => {}
        }
    }

    /// `ServerboundMoveVehiclePacket.fromEntity` for the vehicle the player
    /// steers; nothing when the server moves it.
    pub(super) fn send_move_vehicle(&self) {
        if let Some(vehicle) = self.entities.driven_vehicle() {
            self.joined.session.send_move_vehicle(
                vehicle.pos.to_array(),
                vehicle.yaw,
                vehicle.pitch,
                vehicle.on_ground,
            );
        }
    }
}

/// Game time of the last hold, and until when each tick of it is traced.
pub(super) static LAST_HOLD: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(i64::MIN);
pub(super) static HOLD_TRACE_UNTIL: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(i64::MIN);
