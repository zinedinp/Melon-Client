//! `FluidState.animateTick` and the drips under fluids.

use glam::{DVec3, IVec3};

use super::super::{Kind, Spawn, World};
use crate::world::fluid::{self, FluidKind};
use crate::world::java_random::JavaRandom;

/// The fluid in `state`'s animation.
pub fn animate(world: &World, state: u16, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let Some(here) = fluid::fluid_of(state) else {
        return;
    };
    let base = pos.as_dvec3();
    match here.kind {
        FluidKind::Water => {
            let source_or_falling = here.amount == 8 || here.falling;
            if source_or_falling && rng.next_int(10) == 0 {
                let at = base + DVec3::new(rng.next_double(), rng.next_double(), rng.next_double());
                out.push(Spawn::simple(Kind::Underwater, at, DVec3::ZERO));
            }
        }
        FluidKind::Lava => {
            let above = world
                .level
                .block_state(pos.x, pos.y + 1, pos.z)
                .unwrap_or(0);
            if above == 0 && rng.next_int(100) == 0 {
                let at = base + DVec3::new(rng.next_double(), 1.0, rng.next_double());
                out.push(Spawn::simple(Kind::Lava, at, DVec3::ZERO));
            }
        }
    }
    if rng.next_int(10) == 0 {
        let below = world
            .level
            .block_state(pos.x, pos.y - 1, pos.z)
            .unwrap_or(0);
        let sturdy = crate::world::block::state_flags::is_solid(state);
        drip_below(world, pos - IVec3::Y, below, here.kind, sturdy, rng, out);
    }
}

/// `trySpawnDripParticles`.
fn drip_below(
    world: &World,
    pos: IVec3,
    state: u16,
    kind: FluidKind,
    top_solid: bool,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    if fluid::fluid_of(state).is_some() {
        return;
    }
    let (bottom, top) = shape_span(world, pos);
    let drip = match kind {
        FluidKind::Water => Kind::DrippingWater,
        FluidKind::Lava => Kind::DrippingLava,
    };
    let base = pos.as_dvec3();
    let y = if top < 1.0 {
        if !top_solid {
            return;
        }
        1.0 - 0.05
    } else if impermeable(state) {
        return;
    } else if bottom > 0.0 {
        bottom - 0.05
    } else {
        let under = pos - IVec3::Y;
        let under_state = world
            .level
            .block_state(under.x, under.y, under.z)
            .unwrap_or(0);
        if shape_span(world, under).1 >= 1.0 || fluid::fluid_of(under_state).is_some() {
            return;
        }
        -0.05
    };
    let at = base + DVec3::new(rng.next_double(), y, rng.next_double());
    out.push(Spawn::simple(drip, at, DVec3::ZERO));
}

/// The lowest and highest point of a block's collision shape, `(0, 0)`
fn shape_span(world: &World, pos: IVec3) -> (f64, f64) {
    let y = f64::from(pos.y);
    let (lo, hi) = crate::world::physics::block_shapes(world.level, (pos.x, pos.y, pos.z))
        .fold((f64::MAX, 0.0f64), |(lo, hi), b| {
            (lo.min(b.min.y - y), hi.max(b.max.y - y))
        });
    if lo == f64::MAX { (0.0, 0.0) } else { (lo, hi) }
}

/// `BlockTags.IMPERMEABLE`: glass doesn't let a drop through.
fn impermeable(state: u16) -> bool {
    let name = super::block_name(state);
    name == "glass"
        || name.ends_with("stained_glass")
        || name == "tinted_glass"
        || name == "barrier"
}
