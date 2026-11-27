//! Smoke, fire and wind: flames, smoke, ash, explosions, gusts, souls.

use super::*;

/// `CampfireCosySmoke | CampfireSignalSmoke`.
pub(super) fn campfire_smoke(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = c.sprite(rng);
    p.scale(3.0);
    p.width = 0.25;
    p.height = 0.25;
    p.lifetime = rng.next_int(50)
        + if kind == P::CampfireSignalSmoke {
            280
        } else {
            80
        };
    p.gravity = 3.0e-6;
    p.vel = DVec3::new(aux.x, aux.y + f64::from(rng.next_float() / 500.0), aux.z);
    p.look.translucent = true;
    p.color[3] = if kind == P::CampfireSignalSmoke {
        0.95
    } else {
        0.9
    };
    p.behavior = Behavior::Campfire;
    Some(p)
}

/// `Cloud | Sneeze`.
pub(super) fn cloud(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.friction = 0.96;
    p.vel = p.vel * f64::from(0.1f32) + aux;
    let col = 1.0 - rng.next_float() * 0.3;
    p.set_rgb([col; 3]);
    p.quad_size *= 1.875;
    let base = (8.0 / (rng.next_float() * 0.8 + 0.3)) as i32;
    p.lifetime = ((base as f32 * 2.5) as i32).max(1);
    p.physics = false;
    p.look.animated = true;
    p.look.translucent = true;
    p.look.size = SizeCurve::GrowIn;
    if kind == P::Sneeze {
        p.set_rgb([0.22, 1.0, 0.53]);
        p.color[3] = 0.4;
    }
    p.behavior = Behavior::PlayerCloud;
    Some(p)
}

/// `Flame | SoulFireFlame | CopperFireFlame | SmallFlame`.
pub(super) fn flame(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = rising(kind, pos, aux, c.sprite(rng), rng);
    p.free_move = true;
    p.look.size = SizeCurve::Flame;
    p.look.glow = Glow::Emission;
    if kind == P::SmallFlame {
        p.scale(0.5);
    }
    Some(p)
}

/// `DragonBreath`.
pub(super) fn dragon_breath(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.friction = 0.96;
    p.vel = aux;
    p.set_rgb([
        lerp(rng.next_float(), 0.717_647_1, 0.874_509_8),
        0.0,
        lerp(rng.next_float(), 0.823_529_4, 0.976_470_6),
    ]);
    p.quad_size *= 0.75;
    p.lifetime = (20.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.physics = false;
    p.look.animated = true;
    p.look.size = SizeCurve::GrowIn;
    p.behavior = Behavior::DragonBreath { landed: false };
    if let ParticleData::Power(power) = spec.data {
        p.set_power(power);
    }
    Some(p)
}

/// `ExplosionEmitter`.
pub(super) fn explosion_emitter(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.lifetime = 8;
    p.behavior = Behavior::Emitter(Emitter::ExplosionSeed);
    Some(p)
}

/// `Explosion | SonicBoom`.
pub(super) fn explosion(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.lifetime = 6 + rng.next_int(4);
    let col = rng.next_float() * 0.6 + 0.4;
    p.set_rgb([col; 3]);
    p.quad_size = 2.0 * (1.0 - aux.x as f32 * 0.5);
    if kind == P::SonicBoom {
        p.lifetime = 16;
        p.quad_size = 1.5;
    }
    p.look.animated = true;
    p.look.glow = Glow::Full;
    p.behavior = Behavior::Still;
    Some(p)
}

/// `LargeSmoke | Smoke | WhiteSmoke`.
pub(super) fn smoke(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let scale = if kind == P::LargeSmoke { 2.5 } else { 1.0 };
    let mut p = ash_smoke(
        kind,
        pos,
        aux,
        AshSmoke {
            dir: [0.1; 3],
            scale,
            color_random: 0.3,
            max_lifetime: 8,
            gravity: -0.1,
            physics: true,
        },
        rng,
    );
    if kind == P::WhiteSmoke {
        p.set_rgb([0.729_411_8, 0.694_117_67, 0.760_784_3]);
    }
    Some(p)
}

/// `Ash`.
pub(super) fn ash(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    Some(ash_smoke(
        kind,
        pos,
        DVec3::ZERO,
        AshSmoke {
            dir: [0.1, -0.1, 0.1],
            scale: 1.0,
            color_random: 0.5,
            max_lifetime: 20,
            gravity: 0.1,
            physics: false,
        },
        rng,
    ))
}

/// `WhiteAsh`.
pub(super) fn white_ash(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let xa = f64::from(rng.next_float()) * -1.9 * f64::from(rng.next_float()) * 0.1;
    let ya = f64::from(rng.next_float()) * -0.5 * f64::from(rng.next_float()) * 0.1 * 5.0;
    let za = f64::from(rng.next_float()) * -1.9 * f64::from(rng.next_float()) * 0.1;
    let mut p = ash_smoke(
        kind,
        pos,
        DVec3::new(xa, ya, za),
        AshSmoke {
            dir: [0.1, -0.1, 0.1],
            scale: 1.0,
            color_random: 0.0,
            max_lifetime: 20,
            gravity: 0.0125,
            physics: false,
        },
        rng,
    );
    p.set_rgb(rgb_of(12_235_202));
    Some(p)
}

/// `Poof | Spit`.
pub(super) fn poof(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.gravity = if kind == P::Spit { 0.5 } else { -0.1 };
    p.friction = 0.9;
    let mut wobble = || f64::from((rng.next_float() * 2.0 - 1.0) * 0.05);
    p.vel = DVec3::new(aux.x + wobble(), aux.y + wobble(), aux.z + wobble());
    let col = rng.next_float() * 0.3 + 0.7;
    p.set_rgb([col; 3]);
    p.quad_size = 0.1 * (rng.next_float() * rng.next_float() * 6.0 + 1.0);
    p.lifetime = (16.0 / (rng.next_float() * 0.8 + 0.2)) as i32 + 2;
    p.look.animated = true;
    Some(p)
}

/// `Soul | SculkSoul`.
pub(super) fn soul(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        kind,
        pos,
        aux,
        ctx,
        ..
    } = *c;
    let mut p = rising(kind, pos, aux, ctx.sprites.first(kind), rng);
    p.scale(1.5);
    p.look.animated = true;
    p.look.translucent = true;
    if kind == P::SculkSoul {
        p.look.glow = Glow::Block;
    }
    Some(p)
}

/// `NoxiousGas`.
pub(super) fn noxious_gas(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = ash_smoke(
        kind,
        pos,
        aux,
        AshSmoke {
            dir: [0.1; 3],
            scale: 3.0,
            color_random: 0.3,
            max_lifetime: 5,
            gravity: -0.02,
            physics: true,
        },
        rng,
    );
    p.set_rgb([1.0; 3]);
    p.lifetime = (6.0 / (rng.next_float() * 0.5 + 0.5) as f64 * 3.0) as i32;
    p.look.translucent = true;
    p.behavior = Behavior::NoxiousGas;
    Some(p)
}

/// `NoxiousGasCloud`.
pub(super) fn noxious_gas_cloud(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.lifetime = 20;
    p.behavior = Behavior::Emitter(Emitter::NoxiousGasCloud);
    Some(p)
}

/// `Gust | SmallGust`.
pub(super) fn gust(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.lifetime = 12 + rng.next_int(4);
    p.quad_size = 1.0;
    p.width = 1.0;
    p.height = 1.0;
    p.look.animated = true;
    p.look.glow = Glow::Full;
    p.behavior = Behavior::Still;
    if kind == P::SmallGust {
        p.scale(0.15);
    }
    Some(p)
}

/// `GustEmitterLarge | GustEmitterSmall`.
pub(super) fn gust_emitter(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    let (scale, lifetime, delay) = if kind == P::GustEmitterLarge {
        (3.0, 7, 0)
    } else {
        (1.0, 3, 2)
    };
    p.lifetime = lifetime;
    p.behavior = Behavior::Emitter(Emitter::GustSeed { scale, delay });
    Some(p)
}

/// `DustPlume`.
pub(super) fn dust_plume(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let aux = aux + DVec3::Y * f64::from(0.15f32);
    let mut p = ash_smoke(
        kind,
        pos,
        aux,
        AshSmoke {
            dir: [0.7, 0.6, 0.7],
            scale: 1.0,
            color_random: 0.5,
            max_lifetime: 7,
            gravity: 0.5,
            physics: false,
        },
        rng,
    );
    let shift = rng.next_float() * 0.2;
    p.set_rgb(rgb_of(12_235_202).map(|c| c - shift));
    p.behavior = Behavior::DustPlume;
    Some(p)
}

/// `RisingParticle`: barely thrown, `aux` its drift, a short life.
fn rising(kind: Kind, pos: DVec3, aux: DVec3, uv: [f32; 4], rng: &mut JavaRandom) -> Particle {
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.look.uv = uv;
    p.friction = 0.96;
    p.vel = p.vel * f64::from(0.01f32) + aux;
    let mut jitter = || f64::from((rng.next_float() - rng.next_float()) * 0.05);
    p.set_pos(p.pos() + DVec3::new(jitter(), jitter(), jitter()));
    p.reset_prev();
    p.lifetime = (8.0 / (rng.next_double() * 0.8 + 0.2)) as i32 + 4;
    p
}

/// `BaseAshSmokeParticle`'s settings past the position and the speed.
#[derive(Clone, Copy)]
pub(super) struct AshSmoke {
    /// Scales the random start speed per axis.
    pub dir: [f32; 3],
    pub scale: f32,
    pub color_random: f32,
    pub max_lifetime: i32,
    pub gravity: f32,
    pub physics: bool,
}

/// `BaseAshSmokeParticle`: smoke and ash, darkened at random, growing in.
pub(super) fn ash_smoke(
    kind: Kind,
    pos: DVec3,
    aux: DVec3,
    look: AshSmoke,
    rng: &mut JavaRandom,
) -> Particle {
    let AshSmoke {
        dir,
        scale,
        color_random,
        max_lifetime,
        gravity,
        physics,
    } = look;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.friction = 0.96;
    p.gravity = gravity;
    p.speed_up_when_blocked = true;
    p.vel *= DVec3::new(f64::from(dir[0]), f64::from(dir[1]), f64::from(dir[2]));
    p.vel += aux;
    let col = rng.next_float() * color_random;
    p.set_rgb([col; 3]);
    p.quad_size *= 0.75 * scale;
    p.lifetime = ((f64::from(max_lifetime) / (rng.next_double() * 0.8 + 0.2) * f64::from(scale))
        as i32)
        .max(1);
    p.physics = physics;
    p.look.animated = true;
    p.look.size = SizeCurve::GrowIn;
    p
}
