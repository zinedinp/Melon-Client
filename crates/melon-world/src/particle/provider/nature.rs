//! Nature: spores and the underwater haze, snow, leaves, fireflies.

use super::*;

/// `Underwater | CrimsonSpore | WarpedSpore | SporeBlossomAir`.
pub(super) fn underwater(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let at = pos - DVec3::Y * 0.125;
    let mut p = match kind {
        P::Underwater => Particle::at(kind, at, rng),
        P::CrimsonSpore => {
            let vel = DVec3::new(
                rng.next_gaussian() * 1.0e-6,
                rng.next_gaussian() * 1.0e-4,
                rng.next_gaussian() * 1.0e-6,
            );
            Particle::moving(kind, at, vel, rng)
        }
        P::WarpedSpore => {
            let ya = f64::from(rng.next_float()) * -1.9 * f64::from(rng.next_float()) * 0.1;
            Particle::moving(kind, at, DVec3::new(0.0, ya, 0.0), rng)
        }
        _ => Particle::moving(kind, at, DVec3::new(0.0, f64::from(-0.8f32), 0.0), rng),
    };
    p.look.uv = c.sprite(rng);
    p.width = 0.01;
    p.height = 0.01;
    let grow = if kind == P::Underwater { 0.2 } else { 0.6 };
    p.quad_size *= rng.next_float() * 0.6 + grow;
    p.lifetime = (16.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.physics = false;
    p.friction = 1.0;
    p.gravity = 0.0;
    match kind {
        P::Underwater => p.set_rgb([0.4, 0.4, 0.7]),
        P::CrimsonSpore => p.set_rgb([0.9, 0.4, 0.5]),
        P::WarpedSpore => {
            p.set_rgb([0.1, 0.1, 0.3]);
            p.width = 0.001;
            p.height = 0.001;
        }
        _ => {
            p.lifetime = 500 + rng.next_int(501);
            p.gravity = 0.01;
            p.set_rgb([0.32, 0.5, 0.22]);
        }
    }
    Some(p)
}

/// `Snowflake`.
pub(super) fn snowflake(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.gravity = 0.225;
    p.friction = 1.0;
    let mut wobble = || f64::from((rng.next_float() * 2.0 - 1.0) * 0.05);
    p.vel = DVec3::new(aux.x + wobble(), aux.y + wobble(), aux.z + wobble());
    p.quad_size = 0.1 * (rng.next_float() * rng.next_float() + 1.0);
    p.lifetime = (16.0 / (rng.next_float() * 0.8 + 0.2)) as i32 + 2;
    p.look.animated = true;
    p.set_rgb([0.923, 0.964, 0.999]);
    p.behavior = Behavior::Snowflake;
    Some(p)
}

/// `CherryLeaves | PaleOakLeaves | RedPoplarLeaves | OrangePoplarLeaves | YellowPoplarLeaves | TintedLeaves`.
pub(super) fn leaves(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec, kind, pos, ..
    } = *c;
    let (fall, side, swirl, flow_away, scale, start): (f32, f32, bool, bool, f32, f32) =
        if kind == P::CherryLeaves {
            (0.25, 2.0, false, true, 1.0, 0.0)
        } else {
            (0.07, 10.0, true, false, 2.0, 0.021)
        };
    let uv = c.sprite(rng);
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    let rot_speed = (if rng.next_boolean() { -30.0f32 } else { 30.0 }).to_radians();
    let spin = (if rng.next_boolean() { -5.0f32 } else { 5.0 }).to_radians();
    p.lifetime = 300;
    p.gravity = fall * 1.2 * 0.0025;
    let size = scale * if rng.next_boolean() { 0.05 } else { 0.075 };
    p.quad_size = size;
    p.width = size;
    p.height = size;
    p.friction = 1.0;
    p.vel.y = -f64::from(start);
    let r = rng.next_float();
    let flow_turn = f64::from(r * 60.0).to_radians();
    p.behavior = Behavior::Leaf(Leaf {
        rot_speed,
        spin,
        wind: side,
        swirl,
        flow_away,
        flow: [
            (flow_turn.cos() * f64::from(side)) as f32,
            (flow_turn.sin() * f64::from(side)) as f32,
        ],
        swirl_period: (1000.0 + r * 3000.0).to_radians(),
    });
    if let ParticleData::Color(argb) = spec.data {
        p.set_rgb(rgb_of(argb));
    }
    Some(p)
}

/// `Firefly`.
pub(super) fn firefly(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let xa = 0.5 - rng.next_double();
    let ya = if rng.next_boolean() { aux.y } else { -aux.y };
    let za = 0.5 - rng.next_double();
    let uv = c.sprite(rng);
    let mut p = Particle::moving(kind, pos, DVec3::new(xa, ya, za), rng);
    p.look.uv = uv;
    p.speed_up_when_blocked = true;
    p.friction = 0.96;
    p.quad_size *= 0.75;
    p.vel *= f64::from(0.8f32);
    p.lifetime = 200 + rng.next_int(101);
    p.scale(1.5);
    p.color[3] = 0.0;
    p.look.translucent = true;
    p.look.glow = Glow::Firefly;
    p.behavior = Behavior::Firefly;
    Some(p)
}
