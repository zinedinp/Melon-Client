//! `ClientExplosionTracker`.

use glam::DVec3;

use super::super::Spawn;
use super::super::options::ParticleSpec;
use crate::settings::options::ParticleStatus;
use crate::world::java_random::JavaRandom;
use crate::world::level::Level;

/// `MAX_PARTICLES_PER_TICK`.
const MAX_PER_TICK: i32 = 512;

/// One entry of the packet's `blockParticles` (`ExplosionParticleInfo`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Debris {
    pub spec: ParticleSpec,
    /// How far out from the centre, against where the block was.
    pub scaling: f32,
    pub speed: f32,
    pub weight: i32,
}

#[derive(Clone, Debug, PartialEq)]
struct Blast {
    center: DVec3,
    radius: f32,
    block_count: i32,
    debris: Vec<Debris>,
}

#[derive(Clone, Debug, Default)]
pub struct ExplosionTracker {
    blasts: Vec<Blast>,
}

impl ExplosionTracker {
    /// `track`: an explosion with debris to throw.
    pub fn track(&mut self, center: DVec3, radius: f32, block_count: i32, debris: Vec<Debris>) {
        if !debris.is_empty() && debris.iter().any(|d| d.weight > 0) {
            self.blasts.push(Blast {
                center,
                radius,
                block_count,
                debris,
            });
        }
    }

    /// `tick`: up to 512 particles, the blasts picked by their block count.
    pub fn tick(
        &mut self,
        status: ParticleStatus,
        level: &Level,
        rng: &mut JavaRandom,
        out: &mut Vec<Spawn>,
    ) {
        if status != ParticleStatus::All || self.blasts.is_empty() {
            self.blasts.clear();
            return;
        }
        let total = self
            .blasts
            .iter()
            .fold(0i32, |sum, b| sum.saturating_add(b.block_count.max(0)));
        for _ in 0..total.min(MAX_PER_TICK) {
            let mut pick = rng.next_int(total.max(1));
            let Some(blast) = self.blasts.iter().find(|b| {
                pick -= b.block_count.max(0);
                pick < 0
            }) else {
                continue;
            };
            throw(blast, level, rng, out);
        }
        self.blasts.clear();
    }
}

/// `addParticle`.
fn throw(blast: &Blast, level: &Level, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let mut unit = || f64::from(rng.next_float() * 2.0 - 1.0);
    let dir = DVec3::new(unit(), unit(), unit()).normalize_or_zero();
    let radius = rng.next_float().cbrt() * blast.radius;
    let local = dir * f64::from(radius);
    let at = (blast.center + local).floor().as_ivec3();
    if level.block_state(at.x, at.y, at.z).unwrap_or(0) != 0 {
        return;
    }
    let speed =
        0.5 / (radius / blast.radius.max(1.0e-6) + 0.1) * rng.next_float() * rng.next_float() + 0.3;
    let total: i32 = blast.debris.iter().map(|d| d.weight.max(0)).sum();
    let mut pick = rng.next_int(total.max(1));
    let Some(info) = blast.debris.iter().find(|d| {
        pick -= d.weight.max(0);
        pick < 0
    }) else {
        return;
    };
    let pos = blast.center + local * f64::from(info.scaling);
    let vel = dir * f64::from(speed * info.speed);
    out.push(Spawn::new(info.spec, pos, vel));
}
