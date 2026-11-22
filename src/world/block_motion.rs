//! What blocks do to the motion of what moves over them, shared by the player and the entities.

use crate::world::level::Level;
use crate::world::physics;
use glam::DVec3;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId};

/// How far under the feet the block that affects movement is.
pub const BELOW: f64 = 0.500_001;

/// The block at the cell holding `at`; air outside loaded chunks.
pub fn block_at(level: &Level, at: DVec3) -> &'static Block {
    let cell = at.floor();
    let id = physics::block_at(level, cell.x as i32, cell.y as i32, cell.z as i32);
    Block::from_state_id(BlockStateId::new_or_air(id))
}

/// The block under `feet` that decides friction and the factors.
pub fn block_below(level: &Level, feet: DVec3) -> &'static Block {
    block_at(level, feet - DVec3::new(0.0, BELOW, 0.0))
}

/// `Entity.getOnPos(offset)`: with a supporting block, its column at
/// `feet.y - offset` (the block itself for an offset of `1e-5` or less, or
/// when it is a fence, wall or gate); without one, the cell under the feet.
pub fn on_pos(
    level: &Level,
    feet: DVec3,
    supporting: Option<(i32, i32, i32)>,
    offset: f64,
) -> (i32, i32, i32) {
    let Some((x, y, z)) = supporting else {
        let cell = (feet - DVec3::new(0.0, offset, 0.0)).floor();
        return (cell.x as i32, cell.y as i32, cell.z as i32);
    };
    if offset <= f64::from(1.0e-5_f32) {
        return (x, y, z);
    }
    let id = physics::block_at(level, x, y, z);
    let support = Block::from_state_id(BlockStateId::new_or_air(id));
    if (offset <= 0.5 && support.has_tag(&tag::Block::MINECRAFT_FENCES))
        || support.has_tag(&tag::Block::MINECRAFT_WALLS)
        || support.has_tag(&tag::Block::MINECRAFT_FENCE_GATES)
    {
        return (x, y, z);
    }
    (x, (feet.y - offset).floor() as i32, z)
}

/// The block at [`on_pos`].
pub fn block_on(
    level: &Level,
    feet: DVec3,
    supporting: Option<(i32, i32, i32)>,
    offset: f64,
) -> &'static Block {
    let (x, y, z) = on_pos(level, feet, supporting, offset);
    Block::from_state_id(BlockStateId::new_or_air(physics::block_at(level, x, y, z)))
}

/// `Entity.getBlockPosBelowThatAffectsMyMovement` (`getOnPos(0.500001F)`).
pub fn block_below_supported(
    level: &Level,
    feet: DVec3,
    supporting: Option<(i32, i32, i32)>,
) -> &'static Block {
    block_on(level, feet, supporting, BELOW)
}

/// `Entity.getOnPosLegacy` (`getOnPos(0.2F)`).
pub fn block_underfoot(
    level: &Level,
    feet: DVec3,
    supporting: Option<(i32, i32, i32)>,
) -> &'static Block {
    block_on(level, feet, supporting, f64::from(0.2_f32))
}

/// `Properties.bounceRestitution` of the blocks that have one.
pub fn bounciness(block: &Block) -> f32 {
    let name = block.name;
    if name == "slime_block" {
        1.0
    } else if name.ends_with("_bed") {
        0.75
    } else {
        0.0
    }
}

/// What a move that hit something leaves of the velocity.
#[allow(clippy::too_many_arguments)]
pub fn restituted(
    current: DVec3,
    moved_y: f64,
    [x_hit, z_hit, vertical_hit]: [bool; 3],
    landed_on: Option<&Block>,
    suppress: bool,
    gravity: f64,
    air_drag: f64,
    living: bool,
) -> DVec3 {
    let mut after = current;
    let mut restitution = 0.0;
    if x_hit {
        after.x = -current.x * restitution;
    }
    if z_hit {
        after.z = -current.z * restitution;
    }
    if vertical_hit {
        if let Some(block) = landed_on {
            restitution = if -current.y > gravity
                && !suppress
                && !block.has_tag(&tag::Block::MINECRAFT_SUPPRESSES_BOUNCE)
            {
                let scale = if living { 1.0 } else { 0.8 };
                restitution.max(f64::from(bounciness(block)) * scale)
            } else {
                0.0
            };
        }
        let (compensation, drag) = if restitution > 0.0 {
            let portion = moved_y / current.y;
            (portion * gravity, 1.0 + (air_drag - 1.0) * portion)
        } else {
            (0.0, 1.0)
        };
        after.y = (compensation - current.y) * drag * restitution;
    }
    after
}

/// `Entity.getBlockSpeedFactor` / `getBlockJumpFactor`, `pick` choosing which.
pub fn factor(
    level: &Level,
    feet: DVec3,
    supporting: Option<(i32, i32, i32)>,
    speed: bool,
    pick: fn(&Block) -> f32,
) -> f32 {
    let here_block = block_at(level, feet);
    let here = pick(here_block);
    if speed && (here_block == &Block::WATER || here_block == &Block::BUBBLE_COLUMN) {
        return here;
    }
    if here == 1.0 {
        pick(block_below_supported(level, feet, supporting))
    } else {
        here
    }
}

pub fn speed_multiplier(block: &Block) -> f32 {
    block.velocity_multiplier
}

pub fn jump_multiplier(block: &Block) -> f32 {
    block.jump_velocity_multiplier
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fast_landing_on_slime_bounces_and_sneaking_or_stone_does_not() {
        let fall = DVec3::new(0.0, -0.5, 0.0);
        let hit = [false, false, true];
        let up = |block: &Block, suppress| {
            restituted(fall, -0.4, hit, Some(block), suppress, 0.08, 0.98, true).y
        };
        assert!(up(&Block::SLIME_BLOCK, false) > 0.3);
        assert_eq!(up(&Block::SLIME_BLOCK, true), 0.0);
        assert_eq!(up(&Block::STONE, false), 0.0);
        assert_eq!(up(&Block::HONEY_BLOCK, false), 0.0);
    }

    #[test]
    fn soul_sand_and_honey_slow_and_water_does_not() {
        assert!(speed_multiplier(&Block::SOUL_SAND) < 1.0);
        assert!(speed_multiplier(&Block::HONEY_BLOCK) < 1.0);
        assert!(jump_multiplier(&Block::HONEY_BLOCK) < 1.0);
        assert_eq!(speed_multiplier(&Block::STONE), 1.0);
    }
}
