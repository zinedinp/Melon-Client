//! A block's crumbs.

use glam::DVec3;

use super::super::{Kind, ParticleEngine, ParticleSpec, World, provider};
use crate::block::direction::Direction;
use crate::physics::Aabb;

/// `getShape`'s boxes of `state` at `pos`, block-relative.
fn shape_boxes(world: &World, pos: [i32; 3]) -> Vec<Aabb> {
    let origin = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
    let boxes: Vec<Aabb> = crate::pick::outline_shapes(world.level, (pos[0], pos[1], pos[2]))
        .map(|b| Aabb {
            min: b.min - origin,
            max: b.max - origin,
        })
        .collect();
    if boxes.is_empty() {
        vec![Aabb {
            min: DVec3::ZERO,
            max: DVec3::ONE,
        }]
    } else {
        boxes
    }
}

/// `addDestroyBlockEffect`.
pub fn destroy(engine: &mut ParticleEngine, world: &World, pos: [i32; 3], state: u16) {
    if state == 0 {
        return;
    }
    let boxes = if world.level.block_state(pos[0], pos[1], pos[2]) == Some(state) {
        shape_boxes(world, pos)
    } else {
        vec![Aabb {
            min: DVec3::ZERO,
            max: DVec3::ONE,
        }]
    };
    let origin = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
    let spec = ParticleSpec::block(Kind::Block, state);
    engine.with_ctx(world, |ctx, engine| {
        for b in boxes {
            let size = (b.max - b.min).min(DVec3::ONE);
            let count = |w: f64| ((w / 0.25).ceil() as i32).max(2);
            let (cx, cy, cz) = (count(size.x), count(size.y), count(size.z));
            for xx in 0..cx {
                for yy in 0..cy {
                    for zz in 0..cz {
                        let rel = DVec3::new(
                            (f64::from(xx) + 0.5) / f64::from(cx),
                            (f64::from(yy) + 0.5) / f64::from(cy),
                            (f64::from(zz) + 0.5) / f64::from(cz),
                        );
                        let at = origin + rel * size + b.min;
                        if let Some(p) =
                            provider::create(&spec, at, rel - DVec3::splat(0.5), ctx, engine.rng())
                        {
                            engine.add(p);
                        }
                    }
                }
            }
        }
    });
}

/// `addBreakingParticles`: one crumb just off the face being mined.
pub fn crack(engine: &mut ParticleEngine, world: &World, pos: [i32; 3], face: Direction) {
    let Some(state) = world
        .level
        .block_state(pos[0], pos[1], pos[2])
        .filter(|&s| s != 0)
    else {
        return;
    };
    let boxes = shape_boxes(world, pos);
    let mut bounds = boxes[0];
    for b in &boxes[1..] {
        bounds.min = bounds.min.min(b.min);
        bounds.max = bounds.max.max(b.max);
    }
    let origin = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
    let spec = ParticleSpec::block(Kind::Block, state);
    engine.with_ctx(world, |ctx, engine| {
        let rng = engine.rng();
        let r = f64::from(0.1f32);
        let mut along = |axis: usize| {
            rng.next_double() * (bounds.max[axis] - bounds.min[axis] - 2.0 * r)
                + r
                + bounds.min[axis]
        };
        let mut at = DVec3::new(along(0), along(1), along(2));
        match face {
            Direction::Down => at.y = bounds.min.y - r,
            Direction::Up => at.y = bounds.max.y + r,
            Direction::North => at.z = bounds.min.z - r,
            Direction::South => at.z = bounds.max.z + r,
            Direction::West => at.x = bounds.min.x - r,
            Direction::East => at.x = bounds.max.x + r,
        }
        if let Some(mut p) = provider::create(&spec, origin + at, DVec3::ZERO, ctx, engine.rng()) {
            p.set_power(0.2);
            p.scale(0.6);
            engine.add(p);
        }
    });
}
