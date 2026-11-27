//! The movement tick (`LocalPlayer.aiStep` into `LivingEntity.travel`):
//! the shared frame and physics. Each way of moving is a [`MoveState`].

mod fluid;
mod fly;
mod land;
mod state;

pub use state::{MoveState, MoveStates, Travel, priority};

use super::CROUCH_HEIGHT;
use super::{
    FLUID_JUMP, HEIGHT, INPUT_DAMPING, JUMP_DELAY_TICKS, MoveInput, PlayerState, SPRINT_FOOD_LEVEL,
    SPRINT_JUMP_BOOST, VERTICAL_DRAG,
};
use crate::fluid::FluidKind;
use crate::level::Level;
use crate::physics::{self, Aabb};
use glam::DVec3;

impl PlayerState {
    /// One game tick. Does nothing while the chunk under the player hasn't
    /// arrived.
    pub fn tick(&mut self, input: MoveInput, level: &Level, food: i32, sprint_window: u32) {
        self.prev_feet = self.feet;
        self.walk_dist_old = self.walk_dist;
        self.camera_eye_old = self.camera_eye;
        self.camera_eye += (self.eye_height() - self.camera_eye) * 0.5;
        if !level.is_loaded_at(self.feet.x.floor() as i32, self.feet.z.floor() as i32) {
            return;
        }
        self.update_bob();
        let before = self.bounding_box();
        self.travel(input, level, food, sprint_window);
        self.apply_block_effects(level);
        if self.spin_ticks > 0 {
            self.spin_ticks -= 1;
            self.spin_sweep = Some(Aabb {
                min: before.min.min(self.bounding_box().min),
                max: before.max.max(self.bounding_box().max),
            });
        }
        let moved = self.feet - self.prev_feet;
        let horizontal = (moved.x * moved.x + moved.z * moved.z).sqrt() as f32;
        self.walk_dist += horizontal * 0.6;
        self.walk.tick(horizontal, false);
    }

    /// `AbstractClientPlayer.updateBob`: the view bobs with the speed on
    /// the ground.
    fn update_bob(&mut self) {
        let target = if self.on_ground && !self.swimming {
            let v = self.velocity;
            ((v.x * v.x + v.z * v.z).sqrt() as f32).min(0.1)
        } else {
            0.0
        };
        self.bob_old = self.bob;
        self.bob += (target - self.bob) * 0.4;
    }

    /// Adds a movement state; see [`MoveState`].
    // TODO(mods): Rust only; client mod code (native/wasm) has no bridge to
    // register a state yet.
    pub fn register_move_state(&mut self, state: Box<dyn MoveState>) {
        self.move_states.add(state);
    }

    /// `melon:movement`'s `Allowed`: the mod states this server accepts.
    pub fn set_allowed_moves(&mut self, ids: Vec<String>) {
        self.allowed_moves = ids;
    }

    pub fn move_allowed(&self, id: &str) -> bool {
        self.allowed_moves.iter().any(|allowed| allowed == id)
    }

    /// The mod states that began since the last call, for `Start`.
    pub fn take_started_moves(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.started_moves)
    }

    /// The mode the last tick moved in; a riptide spin over anything but
    /// flight.
    pub fn move_mode(&self) -> super::MoveMode {
        if self.spin_ticks > 0 && !self.abilities.flying {
            super::MoveMode::SpinAttack
        } else {
            self.move_mode
        }
    }

    /// The pose and flags, the states' bookkeeping, then the first active
    /// state moves the player.
    fn travel(&mut self, input: MoveInput, level: &Level, food: i32, sprint_window: u32) {
        let was_jumping = self.had_jump;
        self.had_jump = input.jump;
        let heights = self.update_fluid_interaction(level);
        let can_stand = self.can_stand_up(level);
        let fits_crouched = physics::no_collision(
            level,
            &Aabb::at_feet(self.feet, self.box_width(), CROUCH_HEIGHT * self.scale())
                .deflate(1.0e-7),
        );
        self.update_water_state(level, heights.in_water(), can_stand, fits_crouched);
        self.on_climbable = self.compute_on_climbable(level);
        self.crouching = !self.abilities.flying
            && !self.spectator
            && !self.swimming
            && !self.gliding
            && self.spin_ticks == 0
            && fits_crouched
            && (input.sneak || !can_stand);
        self.update_sprint(input, food > SPRINT_FOOD_LEVEL, sprint_window);

        let env = Travel::new(self, level, input, was_jumping, heights);
        let mut states = std::mem::take(&mut self.move_states);
        states.update(self, &env);

        self.push_out_of_blocks(level);
        self.intent = env.relative(env.forward, env.strafe, 1.0);
        self.no_jump_delay = self.no_jump_delay.saturating_sub(1);
        if self.velocity.x * self.velocity.x + self.velocity.z * self.velocity.z < 9.0e-6 {
            self.velocity.x = 0.0;
            self.velocity.z = 0.0;
        }
        if self.velocity.y.abs() < 0.003 {
            self.velocity.y = 0.0;
        }
        self.jump_trigger_ticks = self.jump_trigger_ticks.saturating_sub(1);

        let mut server_move = None;
        if let Some(state) = states.select(self, &env) {
            if !state.own_vertical_input() {
                self.vertical_input(&env);
            }
            self.move_mode = state.mode(self);
            server_move = state.server_id();
            state.travel(self, &env);
        }
        if server_move.is_some() && server_move != self.server_move {
            self.started_moves.extend(server_move);
        }
        self.server_move = server_move;
        self.move_states = states;
    }

    /// `aiStep`'s keys for up and down: sink, jump or swim up, and the
    /// swim stroke.
    fn vertical_input(&mut self, env: &Travel) {
        if env.input.sneak && self.in_water {
            self.velocity.y -= FLUID_JUMP;
        }
        if env.input.jump {
            self.jump(env);
        } else {
            self.no_jump_delay = 0;
        }
        self.swim_stroke(env);
    }

    /// `Entity.baseTick`'s water and `updateSwimming`, then the pose
    /// (`updatePlayerPose`'s swimming and crawling).
    fn update_water_state(
        &mut self,
        level: &Level,
        in_water: bool,
        can_stand: bool,
        fits_crouched: bool,
    ) {
        self.in_water = in_water;
        self.eye_in_water = physics::eye_in_water(level, self.tick_eye());
        let under_water = self.eye_in_water && self.in_water;
        let feet_in_water = physics::fluid_at(
            level,
            self.feet.x.floor() as i32,
            self.feet.y.floor() as i32,
            self.feet.z.floor() as i32,
        ) == Some(FluidKind::Water);
        self.swimming = !self.abilities.flying
            && !self.spectator
            && self.sprinting
            && if self.swimming {
                self.in_water
            } else {
                under_water && feet_in_water
            };
        let crawling = !self.swimming
            && !self.abilities.flying
            && !self.spectator
            && !can_stand
            && !fits_crouched;
        self.swim_pose = self.swimming || crawling;
        self.swim_amount_old = self.swim_amount;
        self.swim_amount = crate::entity::living::Swim::step(self.swim_amount, self.swim_pose);
    }

    /// `LocalPlayer.modifyInput`: the key vector, normalised, damped, slowed
    /// by sneaking and item use, then stretched to the square's edge.
    pub(super) fn movement_input(&self, input: MoveInput) -> (f32, f32) {
        let (mut x, mut z) = (-input.strafe, input.forward);
        let len = (x * x + z * z).sqrt();
        if len < 1.0e-4 {
            return (0.0, 0.0);
        }
        x /= len;
        z /= len;
        let slow = if self.crouching {
            self.attributes.sneaking_speed()
        } else {
            1.0
        };
        let using = self.item_use.map_or(1.0, |item_use| item_use.speed);
        x *= INPUT_DAMPING * using * slow;
        z *= INPUT_DAMPING * using * slow;

        let len = (x * x + z * z).sqrt();
        let (ax, az) = ((x / len).abs(), (z / len).abs());
        let tan = if az > ax { ax / az } else { az / ax };
        let stretched = (len * (1.0 + tan * tan).sqrt()).min(1.0);
        (z / len * stretched, x / len * stretched)
    }

    /// `Entity.moveRelative`: this tick's input at `speed`, turned by the yaw.
    pub fn move_relative(&mut self, env: &Travel, speed: f32) {
        self.velocity += env.relative(env.forward, env.strafe, speed);
    }

    /// `LivingEntity.jumpFromGround`.
    pub(super) fn jump_from_ground(&mut self, env: &Travel) {
        let power = f64::from(
            self.attributes.jump_strength()
                * self.block_factor(env.level, false, crate::block_motion::jump_multiplier)
                + 0.1 * self.jump_boost as f32,
        );
        if power <= 1.0e-5 {
            return;
        }
        self.velocity.y = power.max(self.velocity.y);
        if self.sprinting {
            let (sin, cos) = env.sin_cos();
            self.velocity.x -= f64::from(sin) * SPRINT_JUMP_BOOST;
            self.velocity.z += f64::from(cos) * SPRINT_JUMP_BOOST;
        }
    }

    /// `LivingEntity.aiStep`'s jump: in water or lava deeper than
    /// `getFluidJumpThreshold` the key swims up (`jumpInLiquid`), even
    /// standing on the floor; in shallower fluid and on land it jumps.
    fn jump(&mut self, env: &Travel) {
        let heights = env.heights();
        let in_lava = heights.in_lava();
        let height = if in_lava { heights.lava } else { heights.water };
        let in_water = heights.in_water() && height > 0.0;
        let threshold = self.fluid_jump_threshold();
        let shallow = height <= threshold;
        // `jumpInLiquid(WATER)` or `jumpInLiquid(LAVA)`: the same push.
        let swims_in_water = in_water && !(self.on_ground && shallow);
        let swims_in_lava = in_lava && !(self.on_ground && heights.lava <= threshold);
        if swims_in_water || swims_in_lava {
            self.velocity.y += FLUID_JUMP;
        } else if (self.on_ground || (in_water && shallow)) && self.no_jump_delay == 0 {
            self.jump_from_ground(env);
            self.no_jump_delay = JUMP_DELAY_TICKS;
        }
    }

    /// `Entity.getFluidJumpThreshold`: how deep a fluid may be and still be
    /// jumped out of; a player too small to have its eyes above 0.4 swims.
    pub(super) fn fluid_jump_threshold(&self) -> f64 {
        if self.eye_height() < 0.4 { 0.0 } else { 0.4 }
    }

    /// `Entity.move`.
    pub fn move_colliding(&mut self, level: &Level, sneaking: bool) {
        if self.spectator {
            self.feet += self.velocity;
            self.horizontal_collision = false;
            self.minor_horizontal_collision = false;
            self.on_ground = false;
            return;
        }
        let aabb = self.bounding_box();
        let step_height = self.attributes.step_height();
        let mut delta = self.velocity;
        if self.stuck.length_squared() > 1.0e-7 {
            delta *= self.stuck;
            self.stuck = DVec3::ZERO;
            self.velocity = DVec3::ZERO;
        }
        if sneaking && !self.abilities.flying && delta.y <= 0.0 && self.is_above_ground(level, aabb)
        {
            delta = physics::back_off_from_edge(level, &self.colliders, aabb, delta, step_height);
        }
        let moved = physics::collide(
            level,
            &self.colliders,
            aabb,
            delta,
            step_height,
            self.on_ground,
        );
        let moved_sq = moved.length_squared();
        if moved_sq > 1.0e-7 || delta.length_squared() - moved_sq < 1.0e-7 {
            self.feet += moved;
        }

        let collided_x = (delta.x - moved.x).abs() >= 1.0e-5;
        let collided_z = (delta.z - moved.z).abs() >= 1.0e-5;
        self.horizontal_collision = collided_x || collided_z;
        self.minor_horizontal_collision =
            self.horizontal_collision && is_minor_deflection(self.intent, moved);
        self.on_ground = delta.y < 0.0 && moved.y != delta.y;
        self.update_supporting_block(level, moved);
        if self.on_ground {
            let last = if !self.in_water && moved.y < 0.0 {
                -moved.y
            } else {
                0.0
            };
            let fallen = self.fall_distance + last;
            if fallen > 0.0 {
                self.landed_fall = Some(fallen);
            }
        }
        if self.on_ground || self.in_water {
            self.fall_distance = 0.0;
        } else if moved.y < 0.0 {
            self.fall_distance -= moved.y;
        }
        let vertical_hit = moved.y != delta.y;
        if vertical_hit || collided_x || collided_z {
            let landed_on = (vertical_hit && self.on_ground).then(|| {
                crate::block_motion::block_underfoot(level, self.feet, self.support.block)
            });
            let air_drag = super::attributes::modified_friction(
                VERTICAL_DRAG as f32,
                self.attributes.air_drag_modifier(),
            );
            self.velocity = crate::block_motion::restituted(
                self.velocity,
                moved.y,
                [collided_x, collided_z, vertical_hit],
                landed_on,
                sneaking,
                (self.effective_gravity(), f64::from(air_drag)),
                true,
            );
        }

        let speed_factor = if self.abilities.flying {
            1.0
        } else {
            let factor = self.block_factor(level, true, crate::block_motion::speed_multiplier);
            let efficiency = self.attributes.movement_efficiency();
            f64::from(factor + (1.0 - factor) * efficiency)
        };
        self.velocity.x *= speed_factor;
        self.velocity.z *= speed_factor;
    }

    /// `Entity.checkSupportingBlock`, after a move.
    pub(super) fn update_supporting_block(&mut self, level: &Level, moved: DVec3) {
        self.support = physics::update_support(
            level,
            &self.bounding_box(),
            self.feet,
            self.on_ground,
            moved,
            self.support,
        );
    }

    /// `Player.isAboveGround`.
    fn is_above_ground(&self, level: &Level, aabb: Aabb) -> bool {
        let step = f64::from(self.attributes.step_height() as f32);
        self.on_ground
            || (self.fall_distance < step
                && !physics::can_fall_at_least(
                    level,
                    &self.colliders,
                    aabb,
                    (0.0, 0.0),
                    step - self.fall_distance,
                ))
    }

    /// Whether the standing box fits where the player crouches.
    pub(super) fn can_stand_up(&self, level: &Level) -> bool {
        physics::no_collision(
            level,
            &Aabb::at_feet(self.feet, self.box_width(), HEIGHT * self.scale()).deflate(1.0e-7),
        )
    }

    /// `Entity.getBlockPosBelowThatAffectsMyMovement`.
    pub(super) fn block_below(&self, level: &Level) -> &'static pumpkin_data::Block {
        crate::block_motion::block_below_supported(level, self.feet, self.support.block)
    }

    pub(super) fn block_at_offset(&self, level: &Level, dy: f64) -> &'static pumpkin_data::Block {
        crate::block_motion::block_at(level, self.feet + DVec3::new(0.0, dy, 0.0))
    }

    /// `Entity.getBlockSpeedFactor` (`speed`) or `getBlockJumpFactor`.
    pub(super) fn block_factor(
        &self,
        level: &Level,
        speed: bool,
        pick: fn(&pumpkin_data::Block) -> f32,
    ) -> f32 {
        crate::block_motion::factor(level, self.feet, self.support.block, speed, pick)
    }
}

/// `Entity.getInputVector`.
pub(super) fn rotate_input(forward: f32, strafe: f32, speed: f32, sin: f32, cos: f32) -> DVec3 {
    let (mut x, mut z) = (f64::from(strafe), f64::from(forward));
    let len_sq = x * x + z * z;
    if len_sq < 1.0e-7 {
        return DVec3::ZERO;
    }
    if len_sq > 1.0 {
        let len = len_sq.sqrt();
        x /= len;
        z /= len;
    }
    let speed = f64::from(speed);
    let (x, z) = (x * speed, z * speed);
    let (sin, cos) = (f64::from(sin), f64::from(cos));
    DVec3::new(x * cos - z * sin, 0.0, z * cos + x * sin)
}

/// `LocalPlayer.isHorizontalCollisionMinor`.
pub(super) fn is_minor_deflection(intent: DVec3, moved: DVec3) -> bool {
    let intent_sq = intent.x * intent.x + intent.z * intent.z;
    let moved_sq = moved.x * moved.x + moved.z * moved.z;
    if intent_sq < 1.0e-5 || moved_sq < 1.0e-5 {
        return false;
    }
    let dot = intent.x * moved.x + intent.z * moved.z;
    (dot / (intent_sq * moved_sq).sqrt()).acos() < 0.139_626_34
}
