//! Particles that draw nothing (`NoRenderParticle`) and spawn others for a while.

use glam::DVec3;

use super::super::base::Particle;
use super::super::options::{ParticleData, ParticleSpec};
use super::super::{Kind, Spawn, TickCtx};
use crate::java_random::JavaRandom;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Emitter {
    /// `HugeExplosionSeedParticle`.
    ExplosionSeed,
    /// `TrackingEmitter`.
    Tracking { entity: i32, spec: ParticleSpec },
    /// `GustSeedParticle`: three gusts within `scale`, every `delay + 1`
    GustSeed { scale: f32, delay: i32 },
    /// `NoxiousGasCloudParticle`.
    NoxiousGasCloud,
    /// `GeyserEruptionParticle`: the plume, its base and its puffs.
    Geyser { aux: [f32; 3], water_blocks: i32 },
}

/// One tick of an emitter.
pub fn tick(
    p: &mut Particle,
    emitter: Emitter,
    ctx: &TickCtx,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    p.reset_prev();
    match emitter {
        Emitter::ExplosionSeed => {
            for _ in 0..6 {
                let mut spread = || (rng.next_double() - rng.next_double()) * 4.0;
                let at = p.pos() + DVec3::new(spread(), spread(), spread());
                out.push(Spawn::new(
                    ParticleSpec::simple(Kind::Explosion),
                    at,
                    DVec3::new(f64::from(p.age as f32 / p.lifetime as f32), 0.0, 0.0),
                ));
            }
            p.age += 1;
            if p.age == p.lifetime {
                p.removed = true;
            }
        }
        Emitter::Tracking { entity, spec } => {
            let Some(body) = (ctx.entity)(entity) else {
                p.removed = true;
                return;
            };
            for _ in 0..16 {
                let xa = f64::from(rng.next_float() * 2.0 - 1.0);
                let ya = f64::from(rng.next_float() * 2.0 - 1.0);
                let za = f64::from(rng.next_float() * 2.0 - 1.0);
                if xa * xa + ya * ya + za * za > 1.0 {
                    continue;
                }
                let width = f64::from(body.width);
                let height = f64::from(body.height);
                let at = DVec3::new(
                    body.pos.x + width * (xa / 4.0),
                    body.pos.y + height * (0.5 + ya / 4.0),
                    body.pos.z + width * (za / 4.0),
                );
                out.push(Spawn::new(spec, at, DVec3::new(xa, ya + 0.2, za)));
            }
            p.age += 1;
            if p.age >= p.lifetime {
                p.removed = true;
            }
        }
        Emitter::GustSeed { scale, delay } => {
            if p.age % (delay + 1) == 0 {
                let scale = f64::from(scale);
                for _ in 0..3 {
                    let mut spread = || (rng.next_double() - rng.next_double()) * scale;
                    let at = p.pos() + DVec3::new(spread(), spread(), spread());
                    let progress = f64::from(p.age as f32 / p.lifetime as f32);
                    out.push(Spawn::simple(
                        Kind::Gust,
                        at,
                        DVec3::new(progress, 0.0, 0.0),
                    ));
                }
            }
            let old = p.age;
            p.age += 1;
            if old == p.lifetime {
                p.removed = true;
            }
        }
        Emitter::NoxiousGasCloud => {
            p.base_tick(ctx);
            if p.age % 2 == 0 {
                let source = p.pos().floor().as_ivec3();
                let dir = DVec3::new(
                    f64::from(rng.next_float() - 0.5),
                    0.0,
                    f64::from(rng.next_float() - 0.5),
                )
                .normalize_or_zero();
                let distance = f64::from(rng.next_float() * 3.0);
                let at = source.as_dvec3() + DVec3::splat(0.5) + dir * distance - DVec3::Y * 0.25;
                if gas_reaches(ctx, source, at) {
                    out.push(Spawn::simple(Kind::NoxiousGas, at, DVec3::ZERO).always_show(true));
                }
            }
        }
        Emitter::Geyser { aux, water_blocks } => {
            p.base_tick(ctx);
            let aux = DVec3::from_array(aux.map(f64::from));
            let base = |kind: Kind, impulse: f32| ParticleSpec {
                kind,
                data: ParticleData::GeyserBase {
                    water_blocks,
                    impulse,
                },
            };
            if p.age % 2 == 0 {
                for _ in 0..2 {
                    out.push(Spawn::new(base(Kind::GeyserBase, 1.5), p.pos(), aux));
                }
            }
            let plume = ParticleSpec {
                kind: Kind::GeyserPlume,
                data: ParticleData::Geyser { water_blocks },
            };
            for _ in 0..water_blocks.max(0) + 2 {
                out.push(Spawn::new(plume, p.pos(), aux));
            }
            if p.age % 10 == 0 {
                for _ in 0..20 {
                    out.push(Spawn::new(base(Kind::GeyserPoof, 2.0), p.pos(), aux));
                }
            }
        }
    }
}

/// `PotentSulfurBlockEntity.canBeReachedByNoxiousGas`.
fn gas_reaches(ctx: &TickCtx, source: glam::IVec3, at: DVec3) -> bool {
    let cell = at.floor().as_ivec3();
    let state = crate::physics::block_at(ctx.level, cell.x, cell.y, cell.z);
    let name = super::super::client::block_name(state);
    let passable = name.ends_with("air")
        || name == "water"
        || crate::physics::block_shapes(ctx.level, (cell.x, cell.y, cell.z))
            .next()
            .is_none();
    let centre = source.as_dvec3() + DVec3::splat(0.5);
    let below_source = centre - DVec3::Y;
    let below = at - DVec3::Y;
    passable
        && at.distance_squared(centre) <= 9.0
        && super::super::behavior::water_source(ctx, below.floor().as_ivec3())
        && crate::pick::clip_blocks(ctx.level, below_source, below).is_none()
}
