//! Creative and spectator flight (`Player.travel`'s `abilities.flying`).

use super::super::{
    FLY_TOGGLE_WINDOW_TICKS, FLYING_VERTICAL_DRAG, HORIZONTAL_DRAG, MoveMode, PlayerState,
};
use super::state::{MoveState, Travel, priority};

pub(super) struct Fly;

impl MoveState for Fly {
    fn mode(&self, _: &PlayerState) -> MoveMode {
        MoveMode::Fly
    }

    fn priority(&self) -> i32 {
        priority::FLY
    }

    fn update(&mut self, player: &mut PlayerState, env: &Travel) {
        player.update_flight(env);
    }

    fn active(&self, player: &PlayerState, _: &Travel) -> bool {
        player.abilities.flying
    }

    fn own_vertical_input(&self) -> bool {
        true
    }

    fn travel(&mut self, player: &mut PlayerState, env: &Travel) {
        player.travel_flying(env);
        if player.on_ground && !player.spectator {
            player.abilities.flying = false;
            player.abilities_changed = true;
        }
    }
}

impl PlayerState {
    /// `LocalPlayer.aiStep`'s flight block: spectators always fly, creative
    /// players toggle it with a double jump.
    fn update_flight(&mut self, env: &Travel) {
        let input = env.input;
        if self.abilities.may_fly {
            if self.spectator {
                if !self.abilities.flying {
                    self.abilities.flying = true;
                    self.abilities_changed = true;
                }
            } else if !env.was_jumping && input.jump {
                if self.jump_trigger_ticks == 0 {
                    self.jump_trigger_ticks = FLY_TOGGLE_WINDOW_TICKS;
                } else {
                    self.abilities.flying = !self.abilities.flying;
                    self.abilities_changed = true;
                    self.jump_trigger_ticks = 0;
                    if self.abilities.flying && self.on_ground {
                        self.jump_from_ground(env);
                    }
                }
            }
        } else if self.abilities.flying {
            self.abilities.flying = false;
        }
        if self.abilities.flying {
            let up = i8::from(input.jump) - i8::from(input.sneak);
            self.velocity.y += f64::from(up) * f64::from(self.abilities.flying_speed) * 3.0;
        }
    }

    /// `Player.travel` while flying: `travelInAir` with the flying speed and
    /// the vertical speed damped instead of pulled down.
    fn travel_flying(&mut self, env: &Travel) {
        let original_y = self.velocity.y;
        let was_on_ground = self.on_ground;
        let friction = self.surface_friction(env.level);
        let acceleration = self.friction_speed(friction, true);
        self.move_relative(env, acceleration);
        self.move_colliding(env.level, env.input.sneak);
        let drag = f64::from(if was_on_ground {
            friction * HORIZONTAL_DRAG
        } else {
            HORIZONTAL_DRAG
        });
        self.velocity.x *= drag;
        self.velocity.z *= drag;
        self.velocity.y = original_y * FLYING_VERTICAL_DRAG;
    }
}
