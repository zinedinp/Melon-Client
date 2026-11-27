//! `ParticleUtils`' spawners around a block's faces.

use glam::DVec3;

use super::super::{Kind, ParticleSpec, Spawn};
use crate::world::block::direction::Direction;
use crate::world::java_random::JavaRandom;

/// `spawnParticlesOnBlockFaces`.
pub fn on_faces(
    base: DVec3,
    kind: Kind,
    (min, max): (i32, i32),
    step: f64,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    for face in Direction::ALL {
        let count = min + rng.next_int(max - min + 1);
        for _ in 0..count {
            let speed = DVec3::new(
                between(rng, -0.5, 0.5),
                between(rng, -0.5, 0.5),
                between(rng, -0.5, 0.5),
            );
            on_face(base, face, kind, speed, step, rng, out);
        }
    }
}

/// `spawnParticleOnFace`.
pub fn on_face(
    base: DVec3,
    face: Direction,
    kind: Kind,
    speed: DVec3,
    step: f64,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    on_face_of(
        base,
        face,
        ParticleSpec::simple(kind),
        speed,
        step,
        rng,
        out,
    );
}

/// [`on_face`] for a particle with data.
pub fn on_face_of(
    base: DVec3,
    face: Direction,
    spec: ParticleSpec,
    speed: DVec3,
    step: f64,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let center = base + DVec3::splat(0.5);
    let [sx, sy, sz] = face.step();
    let mut along = |s: i32| {
        if s == 0 {
            between(rng, -0.5, 0.5)
        } else {
            f64::from(s) * step
        }
    };
    let at = center + DVec3::new(along(sx), along(sy), along(sz));
    let pick = |s: i32, v: f64| if s == 0 { v } else { 0.0 };
    let aux = DVec3::new(pick(sx, speed.x), pick(sy, speed.y), pick(sz, speed.z));
    out.push(Spawn::new(spec, at, aux));
}

/// `spawnParticlesAlongAxis`.
pub fn along_axis(
    base: DVec3,
    axis: usize,
    radius: f64,
    kind: Kind,
    count: i32,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let center = base + DVec3::splat(0.5);
    for _ in 0..count {
        let mut spread = |on: bool| between(rng, -1.0, 1.0) * if on { 0.5 } else { radius };
        let at = center + DVec3::new(spread(axis == 0), spread(axis == 1), spread(axis == 2));
        let mut speed = |on: bool| if on { between(rng, -1.0, 1.0) } else { 0.0 };
        let aux = DVec3::new(speed(axis == 0), speed(axis == 1), speed(axis == 2));
        out.push(Spawn::simple(kind, at, aux));
    }
}

/// `Mth.nextDouble(random, min, max)`.
pub fn between(rng: &mut JavaRandom, min: f64, max: f64) -> f64 {
    min + rng.next_double() * (max - min)
}
