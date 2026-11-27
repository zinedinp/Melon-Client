//! Movement modes and the constants of the impulse-driven ones.

/// `LivingEntity.travel`'s branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveMode {
    /// `travelInAir` on foot.
    Walk,
    /// Walking with the sprint flag (+30% speed, jump boost).
    Sprint,
    /// `travelInAir` on a ladder, vine or scaffolding (`handleOnClimbable`).
    Climb,
    /// `travelInWater` / `travelInLava`.
    Fluid,
    /// Sprint-swimming under water (`Player.travel`'s look steering).
    Swim,
    /// Creative or spectator flight (`Player.travel`'s `abilities.flying`).
    Fly,
    /// Elytra (`travelFallFlying`): see [`glide`].
    Glide,
    /// The riptide spin (`autoSpinAttackTicks`); see [`impulse`].
    SpinAttack,
    /// A state a mod registered, by its id.
    Custom(&'static str),
}

/// Elytra constants of `LivingEntity.updateFallFlyingMovement`.
pub mod glide {
    /// Vertical lift fraction: `g * (-1 + cos(pitch)^2 * 0.75)`.
    pub const LIFT_FACTOR: f64 = 0.75;
    /// Pitch conversion of vertical speed into forward speed.
    pub const DIVE_CONVERSION: f64 = 0.1;
    /// Climb conversion when looking up.
    pub const CLIMB_CONVERSION: f64 = 0.04;
    pub const CLIMB_LIFT: f64 = 3.2;
    /// How fast velocity turns toward the look direction.
    pub const STEER: f64 = 0.1;
    /// Drag per axis (x, y, z).
    pub const DRAG: (f64, f64, f64) = (0.99_f32 as f64, 0.98_f32 as f64, 0.99_f32 as f64);
}

/// Riptide, the one impulse source so far.
pub mod impulse {
    /// Riptide: `1.5 + 0.75 * (level - 1)` blocks of push; 20 spin ticks.
    pub fn riptide_strength(level: u32) -> f32 {
        1.5 + 0.75 * level.saturating_sub(1) as f32
    }
    pub const RIPTIDE_SPIN_TICKS: u32 = 20;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riptide_strength_grows_by_three_quarters_per_level() {
        assert_eq!(impulse::riptide_strength(1), 1.5);
        assert_eq!(impulse::riptide_strength(3), 3.0);
    }
}
