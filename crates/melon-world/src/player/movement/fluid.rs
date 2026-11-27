//! Water and lava (`travelInWater`, `travelInLava`).

use glam::DVec3;

use super::super::{
    DOLPHINS_GRACE_DRAG, LAVA_DRAG, MoveMode, PlayerState, WATER_ACCELERATION, WATER_DRAG,
    WATER_SPRINT_DRAG,
};
use super::state::{MoveState, Travel, priority};
use crate::world::fluid::FluidKind;
use crate::world::level::Level;
use crate::world::physics;

pub(super) struct Water;

impl MoveState for Water {
    fn mode(&self, player: &PlayerState) -> MoveMode {
        if player.swimming {
            MoveMode::Swim
        } else {
            MoveMode::Fluid
        }
    }

    fn priority(&self) -> i32 {
        priority::FLUID
    }

    fn active(&self, _: &PlayerState, env: &Travel) -> bool {
        env.fluid() == Some(FluidKind::Water)
    }

    fn travel(&mut self, player: &mut PlayerState, env: &Travel) {
        player.travel_in_water(env);
    }
}

pub(super) struct Lava;

impl MoveState for Lava {
    fn mode(&self, _: &PlayerState) -> MoveMode {
        MoveMode::Fluid
    }

    fn priority(&self) -> i32 {
        priority::FLUID
    }

    fn active(&self, _: &PlayerState, env: &Travel) -> bool {
        env.fluid() == Some(FluidKind::Lava)
    }

    fn travel(&mut self, player: &mut PlayerState, env: &Travel) {
        player.travel_in_lava(env);
    }
}

impl PlayerState {
    /// `Player.travel`'s swim stroke: the look steers up and down.
    pub(super) fn swim_stroke(&mut self, env: &Travel) {
        if !self.swimming {
            return;
        }
        let look = super::super::forward_vector_for(self.yaw_degrees, self.pitch_degrees).y;
        let multiplier = if look < -0.2 { 0.085 } else { 0.06 };
        let head = self.feet + DVec3::new(0.0, 1.0 - 0.1, 0.0);
        let above = physics::fluid_at(
            env.level,
            head.x.floor() as i32,
            head.y.floor() as i32,
            head.z.floor() as i32,
        );
        if look <= 0.0 || env.input.jump || above.is_some() {
            self.velocity.y += (f64::from(look) - self.velocity.y) * multiplier;
        }
    }

    /// `LivingEntity.travelInWater`.
    fn travel_in_water(&mut self, env: &Travel) {
        let falling = self.velocity.y <= 0.0;
        let old_y = self.feet.y;
        let gravity = self.effective_gravity();
        let efficiency =
            self.attributes.water_movement_efficiency() * if self.on_ground { 1.0 } else { 0.5 };
        let mut slow_down = if self.sprinting {
            WATER_SPRINT_DRAG as f32
        } else {
            WATER_DRAG as f32
        };
        let mut acceleration = WATER_ACCELERATION;
        if efficiency > 0.0 {
            slow_down += (0.546_000_06 - slow_down) * efficiency;
            acceleration += (self.movement_speed() - acceleration) * efficiency;
        }
        if self.dolphins_grace {
            slow_down = DOLPHINS_GRACE_DRAG;
        }
        self.move_relative(env, acceleration);
        self.move_colliding(env.level, env.input.sneak);
        self.on_climbable = self.compute_on_climbable(env.level);
        if self.horizontal_collision && self.on_climbable {
            self.velocity.y = super::super::climb::CLIMB_UP_SPEED;
        }

        let drag = f64::from(slow_down);
        self.velocity.x *= drag;
        self.velocity.z *= drag;
        self.velocity.y *= f64::from(WATER_DRAG as f32);
        self.velocity.y = self.fluid_falling_adjusted(gravity, falling, self.velocity.y);
        self.jump_out_of_fluid(env.level, old_y);
    }

    /// `LivingEntity.travelInLava`: half speed; in lava no deeper than the
    /// jump threshold the vertical speed only loses a fifth.
    fn travel_in_lava(&mut self, env: &Travel) {
        let shallow = env.heights().lava <= self.fluid_jump_threshold();
        let falling = self.velocity.y <= 0.0;
        let old_y = self.feet.y;
        let gravity = self.effective_gravity();
        self.move_relative(env, WATER_ACCELERATION);
        self.move_colliding(env.level, env.input.sneak);
        self.on_climbable = self.compute_on_climbable(env.level);
        if shallow {
            self.velocity.x *= LAVA_DRAG;
            self.velocity.z *= LAVA_DRAG;
            self.velocity.y *= f64::from(WATER_DRAG as f32);
            self.velocity.y = self.fluid_falling_adjusted(gravity, falling, self.velocity.y);
        } else {
            self.velocity *= LAVA_DRAG;
        }
        if gravity != 0.0 {
            self.velocity.y -= gravity / 4.0;
        }
        self.jump_out_of_fluid(env.level, old_y);
    }

    /// `LivingEntity.getFluidFallingAdjustedMovement`'s vertical speed:
    /// gravity at a sixteenth, and a slow sink instead of hovering.
    fn fluid_falling_adjusted(&self, gravity: f64, falling: bool, dy: f64) -> f64 {
        if gravity == 0.0 || self.sprinting {
            dy
        } else if falling && (dy - 0.005).abs() >= 0.003 && (dy - gravity / 16.0).abs() < 0.003 {
            -0.003
        } else {
            dy - gravity / 16.0
        }
    }

    /// `LivingEntity.jumpOutOfFluid`: against a wall with room above it and
    /// no fluid there, hop up onto the bank.
    fn jump_out_of_fluid(&mut self, level: &Level, old_y: f64) {
        if !self.horizontal_collision {
            return;
        }
        let lift = self.velocity.y + f64::from(0.6_f32) - self.feet.y + old_y;
        let probe = self
            .bounding_box()
            .offset(DVec3::new(self.velocity.x, lift, self.velocity.z));
        if physics::no_collision(level, &probe) && physics::touches_fluid(level, &probe).is_none() {
            self.velocity.y = f64::from(0.3_f32);
        }
    }
}
