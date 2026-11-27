//! Climbing: ladders, vines, scaffolding and ladder-backed trapdoors.

use super::PlayerState;
use crate::level::Level;
use crate::physics;
use glam::DVec3;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId};

/// `handleOnClimbable`: the most a climber moves sideways per tick.
pub(super) const CLIMB_SPEED_LIMIT: f64 = 0.15_f32 as f64;
/// The upward step of `handleRelativeFrictionAndCalculateMovement` against a climbable.
pub(super) const CLIMB_UP_SPEED: f64 = 0.2;
/// The slowest a climber falls.
pub(super) const CLIMB_FALL_LIMIT: f64 = -0.15_f32 as f64;

/// Whether a block state can be climbed.
pub(super) fn is_climbable_state(state: u16, below: u16) -> bool {
    let Some(id) = BlockStateId::new(state) else {
        return false;
    };
    let block = Block::from_state_id(id);
    if block.has_tag(&tag::Block::MINECRAFT_CLIMBABLE) {
        return true;
    }
    if !block.has_tag(&tag::Block::MINECRAFT_TRAPDOORS) {
        return false;
    }
    let Some(below_id) = BlockStateId::new(below) else {
        return false;
    };
    if Block::from_state_id(below_id) != &Block::LADDER {
        return false;
    }
    let prop = |block: &Block, id: BlockStateId, key: &str| {
        block.properties(id).and_then(|p| {
            p.to_props()
                .into_iter()
                .find(|prop| prop.key_str() == key)
                .map(|prop| prop.value_str().to_owned())
        })
    };
    prop(block, id, "open").as_deref() == Some("true")
        && prop(block, id, "facing") == prop(&Block::LADDER, below_id, "facing")
}

/// `LivingEntity.handleOnClimbable` on this tick's velocity.
pub(super) fn clamp_climb(velocity: DVec3, sneaking: bool, in_scaffolding: bool) -> DVec3 {
    let mut v = velocity;
    v.x = v.x.clamp(-CLIMB_SPEED_LIMIT, CLIMB_SPEED_LIMIT);
    v.z = v.z.clamp(-CLIMB_SPEED_LIMIT, CLIMB_SPEED_LIMIT);
    v.y = v.y.max(CLIMB_FALL_LIMIT);
    if v.y < 0.0 && sneaking && !in_scaffolding {
        v.y = 0.0;
    }
    v
}

impl PlayerState {
    /// `Player.onClimbable`.
    pub(super) fn compute_on_climbable(&self, level: &Level) -> bool {
        if self.abilities.flying || self.spectator {
            return false;
        }
        let (x, y, z) = (
            self.feet.x.floor() as i32,
            self.feet.y.floor() as i32,
            self.feet.z.floor() as i32,
        );
        is_climbable_state(
            physics::block_at(level, x, y, z),
            physics::block_at(level, x, y - 1, z),
        )
    }

    /// Whether the feet are in scaffolding (exempt from the sneak hold).
    pub(super) fn in_scaffolding(&self, level: &Level) -> bool {
        self.block_at_offset(level, 0.0) == &Block::SCAFFOLDING
    }

    /// `LivingEntity.handleOnClimbable` for the player's own velocity.
    pub(super) fn handle_on_climbable(&mut self, level: &Level, sneaking: bool) {
        self.on_climbable = self.compute_on_climbable(level);
        if !self.on_climbable {
            return;
        }
        self.fall_distance = 0.0;
        self.velocity = clamp_climb(self.velocity, sneaking, self.in_scaffolding(level));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climber_is_clamped_and_sneak_holds() {
        let v = clamp_climb(DVec3::new(0.5, -1.0, -0.5), false, false);
        assert_eq!(
            v,
            DVec3::new(CLIMB_SPEED_LIMIT, CLIMB_FALL_LIMIT, -CLIMB_SPEED_LIMIT)
        );
        assert_eq!(clamp_climb(DVec3::new(0.0, -1.0, 0.0), true, false).y, 0.0);
        assert_eq!(
            clamp_climb(DVec3::new(0.0, -1.0, 0.0), true, true).y,
            CLIMB_FALL_LIMIT
        );
    }

    #[test]
    fn ladders_and_vines_climb_stone_does_not() {
        let id = |b: &Block| b.default_state.id.as_u16();
        assert!(is_climbable_state(id(&Block::LADDER), 0));
        assert!(is_climbable_state(id(&Block::VINE), 0));
        assert!(is_climbable_state(id(&Block::SCAFFOLDING), 0));
        assert!(!is_climbable_state(id(&Block::STONE), 0));
    }
}
