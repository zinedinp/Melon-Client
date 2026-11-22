//! Particles the client makes up by itself, with no packet asking:

use glam::{DVec3, IVec3};
use pumpkin_data::{Block, BlockStateId};

use super::{Kind, ParticleEngine, Spawn, World};
use crate::world::java_random::JavaRandom;

pub mod ambient;
pub mod blocks;
pub mod entity;
pub mod faces;
pub mod fluids;
pub mod glimmer;
pub mod mobs;
pub mod terrain;
pub mod weather;

/// `animateTick`: this many tries a tick, in each of two radii.
const ANIMATE_TRIES: u32 = 667;

/// A block's registry name, `air` for none.
pub fn block_name(state: u16) -> &'static str {
    BlockStateId::new(state).map_or("air", |id| Block::from_state_id(id).name)
}

/// A block state's property `key`, as its value's name.
pub fn prop(state: u16, key: &str) -> Option<String> {
    let id = BlockStateId::new(state)?;
    let block = Block::from_state_id(id);
    block.properties(id).and_then(|p| {
        p.to_props()
            .into_iter()
            .find(|prop| prop.key_str() == key)
            .map(|prop| prop.value_str().to_string())
    })
}

/// Whether the state is `lit=true`.
pub fn lit(state: u16) -> bool {
    prop(state, "lit").as_deref() == Some("true")
}

/// The biome's ambient particles at a block, with each one's chance a tick.
pub type AmbientAt<'a> = &'a dyn Fn(IVec3) -> &'a [(Kind, f32)];

/// `ClientLevel.animateTick`: random blocks within 16 and 32 of `center`
pub fn animate_tick(engine: &mut ParticleEngine, world: &World, center: IVec3, ambient: AmbientAt) {
    let mut out = Vec::new();
    let rng = engine.rng();
    for _ in 0..ANIMATE_TRIES {
        for radius in [16, 32] {
            let pos = center
                + IVec3::new(
                    rng.next_int(radius) - rng.next_int(radius),
                    rng.next_int(radius) - rng.next_int(radius),
                    rng.next_int(radius) - rng.next_int(radius),
                );
            animate_block(world, pos, ambient, rng, &mut out);
        }
    }
    engine.request_all(out);
}

/// `doAnimateTick` of one block.
fn animate_block(
    world: &World,
    pos: IVec3,
    ambient: AmbientAt,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let Some(state) = world.level.block_state(pos.x, pos.y, pos.z) else {
        return;
    };
    if state != 0 {
        blocks::animate(world, state, pos, rng, out);
        glimmer::animate(world, state, pos, rng, out);
        fluids::animate(world, state, pos, rng, out);
    }
    if world
        .marker
        .is_some_and(|marker| block_name(state) == marker)
    {
        let spec = super::ParticleSpec::block(Kind::BlockMarker, state);
        out.push(Spawn::new(
            spec,
            pos.as_dvec3() + DVec3::splat(0.5),
            DVec3::ZERO,
        ));
    }
    let ambient = ambient(pos);
    if !ambient.is_empty() && !full_block(world, pos) {
        for &(kind, chance) in ambient {
            if rng.next_float() <= chance {
                let at = DVec3::new(
                    f64::from(pos.x) + rng.next_double(),
                    f64::from(pos.y) + rng.next_double(),
                    f64::from(pos.z) + rng.next_double(),
                );
                out.push(Spawn::simple(kind, at, DVec3::ZERO));
            }
        }
    }
}

/// `isCollisionShapeFullBlock`.
pub(crate) fn full_block(world: &World, pos: IVec3) -> bool {
    let mut boxes = crate::world::physics::block_shapes(world.level, (pos.x, pos.y, pos.z));
    let full = |b: crate::world::physics::Aabb| {
        let min = b.min - pos.as_dvec3();
        let max = b.max - pos.as_dvec3();
        min.max_element() <= 1.0e-7 && max.min_element() >= 1.0 - 1.0e-7
    };
    match (boxes.next(), boxes.next()) {
        (Some(only), None) => full(only),
        _ => false,
    }
}
