//! Elytra gliding (`travelFallFlying`) and the riptide spin.

use super::mode::glide;
use super::movement::{MoveState, Travel, priority};
use super::{MoveMode, PlayerState};
use crate::math::angle;
use crate::world::level::Level;
use crate::world::physics::Aabb;
use glam::DVec3;

/// `LivingEntity.updateFallFlyingMovement`.
pub(super) fn fall_flying_step(movement: DVec3, look: DVec3, lean: f32, gravity: f64) -> DVec3 {
    let look_horizontal = (look.x * look.x + look.z * look.z).sqrt();
    let move_horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    let lift = f64::from(lean).cos().powi(2);
    let mut v = movement;
    v.y += gravity * (-1.0 + lift * glide::LIFT_FACTOR);
    if v.y < 0.0 && look_horizontal > 0.0 {
        let convert = v.y * -glide::DIVE_CONVERSION * lift;
        v += DVec3::new(
            look.x * convert / look_horizontal,
            convert,
            look.z * convert / look_horizontal,
        );
    }
    if lean < 0.0 && look_horizontal > 0.0 {
        let convert = move_horizontal * -f64::from(angle::sin(lean)) * glide::CLIMB_CONVERSION;
        v += DVec3::new(
            -look.x * convert / look_horizontal,
            convert * glide::CLIMB_LIFT,
            -look.z * convert / look_horizontal,
        );
    }
    if look_horizontal > 0.0 {
        v.x += (look.x / look_horizontal * move_horizontal - v.x) * glide::STEER;
        v.z += (look.z / look_horizontal * move_horizontal - v.z) * glide::STEER;
    }
    DVec3::new(
        v.x * glide::DRAG.0,
        v.y * glide::DRAG.1,
        v.z * glide::DRAG.2,
    )
}

/// Elytra flight; starts on the jump key's rising edge in the air.
pub(super) struct Glide;

impl MoveState for Glide {
    fn mode(&self, _: &PlayerState) -> MoveMode {
        MoveMode::Glide
    }

    fn priority(&self) -> i32 {
        priority::GLIDE
    }

    fn update(&mut self, player: &mut PlayerState, env: &Travel) {
        player.end_glide_if_grounded();
        if env.input.jump && !env.was_jumping && !player.on_climbable {
            player.try_start_fall_flying(env.fluid().is_some());
        }
    }

    /// On a ladder the land state takes over and ends the glide.
    fn active(&self, player: &PlayerState, env: &Travel) -> bool {
        player.gliding && !player.on_climbable && env.fluid().is_none()
    }

    fn travel(&mut self, player: &mut PlayerState, env: &Travel) {
        player.travel_fall_flying(env.level, env.input.sneak);
    }
}

impl PlayerState {
    /// Whether a glider is worn in its own slot (`LivingEntity.canGlideUsing`).
    pub fn set_glider_worn(&mut self, worn: bool) {
        self.glider_worn = worn;
    }

    /// `Entity.isFallFlying`.
    pub fn gliding(&self) -> bool {
        self.gliding
    }

    /// The server's word on the shared flag.
    pub fn set_gliding(&mut self, gliding: bool) {
        self.gliding = gliding;
    }

    /// Whether the start of a glide is still to be sent to the server.
    pub fn take_glide_start(&mut self) -> bool {
        std::mem::take(&mut self.glide_start_pending)
    }

    /// `LivingEntity.canGlide`: in the air, not flying, with a glider on.
    fn can_glide(&self) -> bool {
        !self.on_ground && !self.abilities.flying && self.glider_worn
    }

    /// `Player.tryToStartFallFlying`, on the jump key's rising edge.
    fn try_start_fall_flying(&mut self, in_liquid: bool) -> bool {
        if self.gliding || !self.can_glide() || in_liquid {
            return false;
        }
        self.gliding = true;
        self.glide_start_pending = true;
        true
    }

    /// `LivingEntity.travelFallFlying`.
    fn travel_fall_flying(&mut self, level: &Level, sneaking: bool) {
        let look = super::forward_vector_for(self.yaw_degrees, self.pitch_degrees);
        let lean = self.pitch_degrees * angle::DEG_TO_RAD;
        self.velocity = fall_flying_step(
            self.velocity,
            DVec3::new(look.x.into(), look.y.into(), look.z.into()),
            lean,
            self.effective_gravity(),
        );
        self.move_colliding(level, sneaking);
    }

    /// The air of a gliding player ends when they land or lose the glider.
    fn end_glide_if_grounded(&mut self) {
        if self.gliding && !self.can_glide() {
            self.gliding = false;
        }
    }

    /// `startAutoSpinAttack` for the riptide throw, from the release.
    pub fn start_riptide(&mut self, level: &Level, enchantment: u32) {
        let strength = super::mode::impulse::riptide_strength(enchantment);
        let look = super::forward_vector_for(self.yaw_degrees, self.pitch_degrees);
        let len = look.length();
        let push = look * (strength / len);
        self.velocity += DVec3::new(push.x.into(), push.y.into(), push.z.into());
        self.spin_ticks = super::mode::impulse::RIPTIDE_SPIN_TICKS;
        if self.on_ground {
            let up = DVec3::new(0.0, f64::from(1.199_999_9_f32), 0.0);
            let moved = crate::world::physics::collide(
                level,
                &self.colliders,
                self.bounding_box(),
                up,
                self.attributes.step_height(),
                true,
            );
            self.feet += moved;
            self.on_ground = false;
        }
    }

    /// `LivingEntity.isAutoSpinAttack`.
    pub fn spin_attacking(&self) -> bool {
        self.spin_ticks > 0
    }

    /// The server's word on living flag 4 going off (the spin it ended).
    pub fn end_spin_attack(&mut self) {
        self.spin_ticks = 0;
    }

    /// The box the last tick swept while spinning, to be tested for what it hit.
    pub fn take_spin_sweep(&mut self) -> Option<Aabb> {
        self.spin_sweep.take()
    }

    /// `checkAutoSpinAttack` once the game knows whether a living entity was hit.
    pub fn resolve_spin_attack(&mut self, hit_living: bool) {
        if hit_living {
            self.spin_ticks = 0;
            self.velocity *= -0.2;
        } else if self.horizontal_collision {
            self.spin_ticks = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(yaw: f32, pitch: f32) -> DVec3 {
        let v = crate::world::player::forward_vector_for(yaw, pitch);
        DVec3::new(v.x.into(), v.y.into(), v.z.into())
    }

    #[test]
    fn a_level_glide_sinks_slowly_and_keeps_its_heading() {
        let l = look(0.0, 0.0);
        let v = fall_flying_step(DVec3::new(0.0, 0.0, -1.0), l, 0.0, 0.08);
        let expected = (-0.02 + 0.02 * 0.1) * f64::from(0.98_f32);
        assert!((v.y - expected).abs() < 1e-9, "{v}");
        assert!(v.z < -0.9);
    }

    #[test]
    fn a_dive_trades_height_for_speed() {
        let pitch = 60.0_f32;
        let l = look(0.0, pitch);
        let start = DVec3::new(0.0, -0.5, -0.2);
        let v = fall_flying_step(start, l, pitch * angle::DEG_TO_RAD, 0.08);
        assert!(v.z.abs() > start.z.abs(), "{v}");
    }

    #[test]
    fn a_climb_turns_speed_into_height() {
        let pitch = -40.0_f32;
        let l = look(0.0, pitch);
        let start = DVec3::new(0.0, 0.0, -1.5);
        let v = fall_flying_step(start, l, pitch * angle::DEG_TO_RAD, 0.08);
        assert!(v.y > 0.0, "{v}");
        assert!(v.z.abs() < start.z.abs() * 0.99, "{v}");
    }

    #[test]
    fn straight_down_has_no_heading_to_steer() {
        let l = look(0.0, 90.0);
        let v = fall_flying_step(DVec3::ZERO, l, 90.0 * angle::DEG_TO_RAD, 0.08);
        assert!(v.x.abs() < 1e-9 && v.z.abs() < 1e-9, "{v}");
        assert!(v.y < 0.0);
    }

    #[test]
    fn a_glide_needs_air_and_a_glider() {
        let mut p = PlayerState::new(DVec3::new(0.0, 80.0, 0.0));
        assert!(!p.try_start_fall_flying(false), "no glider");
        p.set_glider_worn(true);
        assert!(!p.try_start_fall_flying(true), "in a liquid");
        assert!(p.try_start_fall_flying(false));
        assert!(p.gliding() && p.take_glide_start() && !p.take_glide_start());
        p.on_ground = true;
        p.end_glide_if_grounded();
        assert!(!p.gliding());
    }

    #[test]
    fn a_hit_bounces_the_spin_back_and_a_wall_ends_it() {
        let mut p = PlayerState::new(DVec3::ZERO);
        p.spin_ticks = 10;
        p.velocity = DVec3::new(1.0, 0.0, 0.0);
        p.resolve_spin_attack(true);
        assert!(!p.spin_attacking());
        assert!((p.velocity.x + 0.2).abs() < 1e-9);
        p.spin_ticks = 10;
        p.horizontal_collision = true;
        p.resolve_spin_attack(false);
        assert!(!p.spin_attacking());
    }
}
