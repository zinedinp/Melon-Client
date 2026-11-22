//! What the blocks and fluids around the player do to its motion after a move.

use super::PlayerState;
use crate::world::fluid::FluidKind;
use crate::world::level::Level;
use crate::world::physics::{self, Aabb};
use glam::DVec3;
use pumpkin_data::{Block, BlockStateId};

/// `Entity.applyCurrentTo`'s scales.
const WATER_CURRENT: f64 = 0.014;
const LAVA_CURRENT: f64 = 0.002_333_333_333_333_333_5;
/// The least a current moves something that is almost still.
const MIN_CURRENT: f64 = 0.004_500_000_000_000_000_5;

/// `WebBlock`, `SweetBerryBushBlock` and `PowderSnowBlock`'s slowdowns.
const COBWEB: DVec3 = DVec3::new(0.25, 0.05_f32 as f64, 0.25);
const BERRY_BUSH: DVec3 = DVec3::new(0.8_f32 as f64, 0.75, 0.8_f32 as f64);
const POWDER_SNOW: DVec3 = DVec3::new(0.9_f32 as f64, 1.5, 0.9_f32 as f64);

/// `CurrentAccumulator.applyTo` for the player.
pub(super) fn current_impulse(
    flow: DVec3,
    count: u32,
    scale: f64,
    velocity: DVec3,
) -> Option<DVec3> {
    if count == 0 || flow.length_squared() < f64::from(1.0e-5_f32) {
        return None;
    }
    let mut impulse = flow / f64::from(count) * scale;
    if velocity.x.abs() < 0.003 && velocity.z.abs() < 0.003 && impulse.length() < MIN_CURRENT {
        impulse = impulse.normalize_or_zero() * MIN_CURRENT;
    }
    Some(impulse)
}

/// `EntityFluidInteraction.getFluidHeight`: how far water and lava reach
/// above the feet, `0` outside them. Only cells whose fluid reaches the
/// feet count, so a thin flowing layer under a jump is no water.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct FluidHeights {
    pub water: f64,
    pub lava: f64,
}

impl FluidHeights {
    /// `Entity.isInWater`.
    pub fn in_water(self) -> bool {
        self.water > 0.0
    }

    /// `Entity.isInLava`.
    pub fn in_lava(self) -> bool {
        self.lava > 0.0
    }

    /// The fluid `LivingEntity.travelInFluid` moves through: water wins.
    pub fn travel_fluid(self) -> Option<FluidKind> {
        if self.in_water() {
            Some(FluidKind::Water)
        } else if self.in_lava() {
            Some(FluidKind::Lava)
        } else {
            None
        }
    }
}

fn block_of(level: &Level, cell: (i32, i32, i32)) -> &'static Block {
    let id = physics::block_at(level, cell.0, cell.1, cell.2);
    Block::from_state_id(BlockStateId::new_or_air(id))
}

/// `HoneyBlock.isSlidingDown` and `doSlideMovement` on `velocity`.
pub(super) fn honey_slide(
    velocity: DVec3,
    feet: DVec3,
    width: f64,
    cell: (i32, i32, i32),
    on_ground: bool,
) -> Option<DVec3> {
    let old_dy = |d: f64| d / f64::from(0.98_f32) + 0.08;
    let new_dy = |d: f64| (d - 0.08) * f64::from(0.98_f32);
    if on_ground || feet.y > f64::from(cell.1) + 0.9375 - 1.0e-7 || old_dy(velocity.y) >= -0.08 {
        return None;
    }
    let reach = 0.4375 + width / 2.0;
    let dx = (f64::from(cell.0) + 0.5 - feet.x).abs() + 1.0e-7;
    let dz = (f64::from(cell.2) + 0.5 - feet.z).abs() + 1.0e-7;
    if dx <= reach && dz <= reach {
        return None;
    }
    let mut v = velocity;
    if old_dy(v.y) < -0.13 {
        let f = -0.05 / old_dy(v.y);
        v.x *= f;
        v.z *= f;
    }
    v.y = new_dy(-0.05);
    Some(v)
}

impl PlayerState {
    /// `Entity.updateFluidInteraction`: how deep the player stands in water
    /// and lava, and their push (`isPushedByFluid`: not while flying).
    pub(super) fn update_fluid_interaction(&mut self, level: &Level) -> FluidHeights {
        let region = self.bounding_box().deflate(0.001);
        let feet_y = self.bounding_box().min.y;
        let mut heights = FluidHeights::default();
        for (kind, scale) in [
            (FluidKind::Water, WATER_CURRENT),
            (FluidKind::Lava, LAVA_CURRENT),
        ] {
            let current = physics::fluid_current(level, &region, feet_y, kind);
            match kind {
                FluidKind::Water => heights.water = current.height,
                FluidKind::Lava => heights.lava = current.height,
            }
            if !self.abilities.flying
                && current.height > 0.0
                && let Some(impulse) =
                    current_impulse(current.flow, current.count, scale, self.velocity)
            {
                self.velocity += impulse;
            }
        }
        heights
    }

    /// The blocks the player stands in after a move.
    pub(super) fn apply_block_effects(&mut self, level: &Level) {
        if self.spectator {
            return;
        }
        let bounds: Aabb = self.bounding_box().deflate(1.0e-5);
        let feet_cell = (
            self.feet.x.floor() as i32,
            self.feet.y.floor() as i32,
            self.feet.z.floor() as i32,
        );
        let mut stuck = None;
        for cell in physics::cells(&bounds, -1.0e-9, 0) {
            let block = block_of(level, cell);
            if block == &Block::COBWEB {
                stuck = Some(COBWEB);
            } else if block == &Block::SWEET_BERRY_BUSH {
                stuck = Some(BERRY_BUSH);
            } else if block == &Block::POWDER_SNOW && cell == feet_cell {
                stuck = Some(POWDER_SNOW);
            } else if block == &Block::HONEY_BLOCK
                && let Some(v) = honey_slide(
                    self.velocity,
                    self.feet,
                    self.box_width(),
                    cell,
                    self.on_ground,
                )
            {
                self.velocity = v;
                self.fall_distance = 0.0;
            }
        }
        if let Some(multiplier) = stuck
            && !self.abilities.flying
        {
            self.stuck = multiplier;
            self.fall_distance = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_current_pushes_with_its_scale_and_a_still_player_gets_the_minimum() {
        let flow = DVec3::new(1.0, 0.0, 0.0) * 2.0;
        let moving = current_impulse(flow, 2, 0.014, DVec3::new(0.1, 0.0, 0.0)).unwrap();
        assert!((moving.x - 0.014).abs() < 1e-12);
        let still = current_impulse(flow, 2, 0.002, DVec3::ZERO).unwrap();
        assert!((still.x - MIN_CURRENT).abs() < 1e-12);
        assert!(current_impulse(DVec3::ZERO, 1, 0.014, DVec3::ZERO).is_none());
        assert!(current_impulse(flow, 0, 0.014, DVec3::ZERO).is_none());
    }

    #[test]
    fn honey_slows_a_falling_player_at_its_side_but_not_one_standing_on_it() {
        let falling = DVec3::new(0.1, -0.5, 0.0);
        let cell = (0, 64, 0);
        let out = honey_slide(falling, DVec3::new(0.5 + 0.8, 64.5, 0.5), 0.6, cell, false);
        let v = out.expect("slides");
        assert!((v.y - (-0.05 - 0.08) * 0.98).abs() < 1e-6);
        assert!(v.x < 0.1);
        assert!(honey_slide(falling, DVec3::new(0.5, 64.5, 0.5), 0.6, cell, false).is_none());
        assert!(honey_slide(falling, DVec3::new(1.3, 64.5, 0.5), 0.6, cell, true).is_none());
    }
}
