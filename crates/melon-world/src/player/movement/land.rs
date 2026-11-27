//! On foot (`travelInAir`): walking, sprinting, climbing and falling. The
//! fallback state, always active.

use super::super::{
    AIR_ACCELERATION, GROUND_ACCELERATION, HORIZONTAL_DRAG, MoveMode, PlayerState,
    SPRINT_AIR_ACCELERATION, VERTICAL_DRAG, attributes,
};
use super::state::{MoveState, Travel, priority};
use crate::level::Level;

pub(super) struct Land;

impl MoveState for Land {
    fn mode(&self, player: &PlayerState) -> MoveMode {
        if player.on_climbable {
            MoveMode::Climb
        } else if player.sprinting {
            MoveMode::Sprint
        } else {
            MoveMode::Walk
        }
    }

    fn priority(&self) -> i32 {
        priority::LAND
    }

    fn active(&self, _: &PlayerState, _: &Travel) -> bool {
        true
    }

    fn travel(&mut self, player: &mut PlayerState, env: &Travel) {
        // A glider caught on a ladder (`travelFallFlying` gives way) lets go.
        let gliding = player.gliding;
        player.travel_on_land(env);
        if gliding {
            player.gliding = false;
        }
    }
}

impl PlayerState {
    /// `LivingEntity.travelInAir`, for custom states that walk.
    pub fn travel_on_land(&mut self, env: &Travel) {
        let level = env.level;
        let sneak = env.input.sneak;
        let was_on_ground = self.on_ground;
        let friction = self.surface_friction(level);
        let acceleration = self.friction_speed(friction, false);
        self.move_relative(env, acceleration);
        self.handle_on_climbable(level, sneak);
        self.move_colliding(level, sneak);
        self.on_climbable = self.compute_on_climbable(level);
        if self.on_climbable && (self.horizontal_collision || env.input.jump) {
            self.velocity.y = super::super::climb::CLIMB_UP_SPEED;
        }

        let air_drag = self.attributes.air_drag_modifier();
        let vertical = f64::from(attributes::modified_friction(
            VERTICAL_DRAG as f32,
            air_drag,
        ));
        self.velocity.y = (self.velocity.y - self.effective_gravity()) * vertical;
        let horizontal = attributes::modified_friction(HORIZONTAL_DRAG, air_drag);
        let drag = f64::from(if was_on_ground {
            friction * horizontal
        } else {
            horizontal
        });
        self.velocity.x *= drag;
        self.velocity.z *= drag;
    }

    /// `blockFriction` of `travelInAir`: the ground's slipperiness through
    /// the friction attribute, `1` in the air.
    pub(super) fn surface_friction(&self, level: &Level) -> f32 {
        if self.on_ground {
            attributes::modified_friction(
                self.block_below(level).slipperiness,
                self.attributes.friction_modifier(),
            )
        } else {
            1.0
        }
    }

    /// `getFrictionInfluencedSpeed`: on the ground the speed, slippery
    /// ground less; in the air the flying speed.
    pub(super) fn friction_speed(&self, friction: f32, flying: bool) -> f32 {
        if self.on_ground {
            let speed = self.movement_speed();
            if friction > 0.6 {
                speed * (GROUND_ACCELERATION / (friction * friction * friction))
            } else {
                speed
            }
        } else if flying {
            if self.sprinting {
                self.abilities.flying_speed * 2.0
            } else {
                self.abilities.flying_speed
            }
        } else if self.sprinting {
            SPRINT_AIR_ACCELERATION
        } else {
            AIR_ACCELERATION
        }
    }
}
