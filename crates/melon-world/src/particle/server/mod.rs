//! Particles the server asks for: `ClientboundLevelParticlesPacket`

use glam::DVec3;

use super::Spawn;
use super::options::ParticleSpec;
use crate::java_random::JavaRandom;

pub mod entity_event;
pub mod explosion;
pub mod level_event;

/// `ClientboundLevelParticlesPacket.RandomizationType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Randomization {
    /// Spread and speed both normal-distributed around the centre.
    #[default]
    Default,
    /// Spread one-sided (`0..dist`), the speed as given.
    Alternative,
    /// One-sided spread, the speed scaled by `0..1` per axis.
    AlternativeWithSpeed,
}

impl Randomization {
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Alternative,
            2 => Self::AlternativeWithSpeed,
            _ => Self::Default,
        }
    }
}

/// `ClientboundLevelParticlesPacket`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelParticles {
    pub spec: ParticleSpec,
    pub override_limiter: bool,
    pub always_show: bool,
    pub pos: [f64; 3],
    pub dist: [f32; 3],
    pub max_speed: [f32; 3],
    pub count: i32,
    pub randomization: Randomization,
}

impl LevelParticles {
    /// `handleParticleEvent`: one particle moving along `dist * max_speed`
    pub fn spawns(&self, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
        let at = DVec3::from_array(self.pos);
        let dist = self.dist.map(f64::from);
        let speed = self.max_speed.map(f64::from);
        let spawn = |pos: DVec3, aux: DVec3| {
            Spawn::new(self.spec, pos, aux)
                .forced(self.override_limiter)
                .always_show(self.always_show)
        };
        if self.count == 0 {
            let aux = DVec3::new(speed[0] * dist[0], speed[1] * dist[1], speed[2] * dist[2]);
            out.push(spawn(at, aux));
            return;
        }
        let count = self.count.clamp(0, super::MAX_PER_GROUP as i32);
        for _ in 0..count {
            let (offset, aux) = match self.randomization {
                Randomization::Default => {
                    let mut g = || rng.next_gaussian();
                    let offset = DVec3::new(g() * dist[0], g() * dist[1], g() * dist[2]);
                    let aux = DVec3::new(g() * speed[0], g() * speed[1], g() * speed[2]);
                    (offset, aux)
                }
                Randomization::Alternative | Randomization::AlternativeWithSpeed => {
                    let mut d = || rng.next_double();
                    let offset = DVec3::new(d() * dist[0], d() * dist[1], d() * dist[2]);
                    let mut aux = DVec3::from_array(speed);
                    if self.randomization == Randomization::AlternativeWithSpeed {
                        aux *= DVec3::new(d(), d(), d());
                    }
                    (offset, aux)
                }
            };
            out.push(spawn(at + offset, aux));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particle::Kind;

    fn packet(count: i32, randomization: Randomization) -> LevelParticles {
        LevelParticles {
            spec: ParticleSpec::simple(Kind::Flame),
            override_limiter: true,
            always_show: false,
            pos: [10.0, 64.0, -5.0],
            dist: [1.0, 2.0, 0.0],
            max_speed: [0.5, 0.5, 0.5],
            count,
            randomization,
        }
    }

    #[test]
    fn a_count_of_zero_is_one_particle_flying_along_the_spread() {
        let mut out = Vec::new();
        packet(0, Randomization::Default).spawns(&mut JavaRandom::new(1), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pos, DVec3::new(10.0, 64.0, -5.0));
        assert_eq!(out[0].aux, DVec3::new(0.5, 1.0, 0.0));
        assert!(out[0].override_limiter);
    }

    #[test]
    fn the_alternative_spread_is_one_sided() {
        let mut out = Vec::new();
        packet(50, Randomization::Alternative).spawns(&mut JavaRandom::new(1), &mut out);
        assert_eq!(out.len(), 50);
        for s in &out {
            assert!((10.0..=11.0).contains(&s.pos.x) && (64.0..=66.0).contains(&s.pos.y));
            assert_eq!(s.pos.z, -5.0);
            assert_eq!(s.aux, DVec3::splat(0.5));
        }
        let mut out = Vec::new();
        packet(50, Randomization::AlternativeWithSpeed).spawns(&mut JavaRandom::new(1), &mut out);
        assert!(out.iter().all(|s| s.aux.max_element() <= 0.5));
        let mut out = Vec::new();
        packet(50, Randomization::Default).spawns(&mut JavaRandom::new(1), &mut out);
        assert!(out.iter().any(|s| s.pos.x < 10.0));
    }
}
