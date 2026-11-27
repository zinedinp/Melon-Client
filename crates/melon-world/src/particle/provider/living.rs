//! Creatures and combat: hearts, villagers' moods, crits and sweeps.

use super::*;

/// `Heart`.
pub(super) fn heart(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    Some(heart_particle(kind, pos, c.sprite(rng), rng))
}

/// `AngryVillager`.
pub(super) fn angry_villager(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    Some(heart_particle(
        kind,
        pos + DVec3::Y * 0.5,
        c.sprite(rng),
        rng,
    ))
}

/// `Composter | Dolphin | EggCrack | HappyVillager | Mycelium`.
pub(super) fn composter(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.look.uv = c.sprite(rng);
    let br = rng.next_float() * 0.1 + 0.2;
    p.set_rgb([br; 3]);
    p.width = 0.02;
    p.height = 0.02;
    p.quad_size *= rng.next_float() * 0.6 + 0.5;
    p.vel *= f64::from(0.02f32);
    p.lifetime = (20.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.behavior = Behavior::Town;
    match kind {
        P::Composter => {
            p.set_rgb([1.0; 3]);
            p.lifetime = 3 + rng.next_int(5);
        }
        P::Dolphin => {
            p.set_rgb([0.3, 0.5, 1.0]);
            p.color[3] = 1.0 - rng.next_float() * 0.7;
            p.lifetime /= 2;
        }
        P::EggCrack | P::HappyVillager => p.set_rgb([1.0; 3]),
        _ => {}
    }
    Some(p)
}

/// `Crit | DamageIndicator | EnchantedHit`.
pub(super) fn crit(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        kind,
        pos,
        aux,
        ctx,
        ..
    } = *c;
    let aux = if kind == P::DamageIndicator {
        aux + DVec3::Y
    } else {
        aux
    };
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = c.sprite(rng);
    p.friction = 0.7;
    p.gravity = 0.5;
    p.vel = p.vel * f64::from(0.1f32) + aux * 0.4;
    let col = rng.next_float() * 0.3 + 0.6;
    p.set_rgb([col; 3]);
    p.quad_size *= 0.75;
    p.lifetime = ((6.0 / (rng.next_float() * 0.8 + 0.6)) as i32).max(1);
    p.physics = false;
    p.look.size = SizeCurve::GrowIn;
    p.behavior = Behavior::Crit;
    p.base_tick(ctx);
    p.color[1] *= 0.96;
    p.color[2] *= 0.9;
    if kind == P::DamageIndicator {
        p.lifetime = 20;
    } else if kind == P::EnchantedHit {
        p.color[0] *= 0.3;
        p.color[1] *= 0.8;
    }
    Some(p)
}

/// `SweepAttack`.
pub(super) fn sweep_attack(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.lifetime = 4;
    let col = rng.next_float() * 0.6 + 0.4;
    p.set_rgb([col; 3]);
    p.quad_size = 1.0 - aux.x as f32 * 0.5;
    p.look.animated = true;
    p.look.glow = Glow::Full;
    p.behavior = Behavior::Still;
    Some(p)
}

/// `HeartParticle`: rises a little and slows.
fn heart_particle(kind: Kind, pos: DVec3, uv: [f32; 4], rng: &mut JavaRandom) -> Particle {
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = uv;
    p.speed_up_when_blocked = true;
    p.friction = 0.86;
    p.vel *= f64::from(0.01f32);
    p.vel.y += 0.1;
    p.quad_size *= 1.5;
    p.lifetime = 16;
    p.physics = false;
    p.look.size = SizeCurve::GrowIn;
    p
}
