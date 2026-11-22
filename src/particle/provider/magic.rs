//! Magic and signals: effects, enchanting, portals, dust, sculk, spawners.

use super::*;

/// `Dust | DustColorTransition`.
pub(super) fn dust(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let (rgb, scale) = match spec.data {
        ParticleData::Dust { rgb, scale } => (rgb, scale),
        ParticleData::DustTransition { from, scale, .. } => (from, scale),
        _ => (0xFF0000, 1.0),
    };
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.friction = 0.96;
    p.speed_up_when_blocked = true;
    p.vel *= f64::from(0.1f32);
    p.quad_size *= 0.75 * scale;
    let base = (8.0 / (rng.next_double() * 0.8 + 0.2)) as i32;
    p.lifetime = ((base as f32 * scale) as i32).max(1);
    p.look.animated = true;
    p.look.size = SizeCurve::GrowIn;
    let base = rng.next_float() * 0.4 + 0.6;
    let mut shade = |c: f32| (rng.next_float() * 0.2 + 0.8) * c * base;
    let [r, g, b] = rgb_of(rgb);
    p.set_rgb([shade(r), shade(g), shade(b)]);
    if let ParticleData::DustTransition { from, to, .. } = spec.data {
        let [fr, fg, fb] = rgb_of(from);
        let [tr, tg, tb] = rgb_of(to);
        let from = [shade(fr), shade(fg), shade(fb)];
        let to = [shade(tr), shade(tg), shade(tb)];
        p.set_rgb(from);
        p.behavior = Behavior::DustTransition { from, to };
    }
    Some(p)
}

/// `Effect | InstantEffect | EntityEffect | Witch | Infested | RaidOmen | TrialOmen`.
pub(super) fn effect(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ctx,
        ..
    } = *c;
    let mut p = Particle::moving(
        kind,
        pos,
        DVec3::new(0.5 - rng.next_double(), aux.y, 0.5 - rng.next_double()),
        rng,
    );
    p.friction = 0.96;
    p.gravity = -0.1;
    p.speed_up_when_blocked = true;
    p.vel.y *= f64::from(0.2f32);
    if aux.x == 0.0 && aux.z == 0.0 {
        p.vel.x *= f64::from(0.1f32);
        p.vel.z *= f64::from(0.1f32);
    }
    p.quad_size *= 0.75;
    p.lifetime = (8.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.physics = false;
    p.look.animated = true;
    p.look.translucent = true;
    match spec.data {
        ParticleData::Spell { rgb, power } => {
            p.set_rgb(rgb_of(rgb));
            p.set_power(power);
        }
        ParticleData::Color(argb) => {
            p.set_rgb(rgb_of(argb));
            p.color[3] = (argb >> 24) as f32 / 255.0;
        }
        _ if kind == P::Witch => {
            let br = rng.next_float() * 0.5 + 0.35;
            p.set_rgb([br, 0.0, br]);
        }
        _ => {}
    }
    p.behavior = Behavior::Spell { alpha: p.color[3] };
    if ctx.close_to_scope(p.pos) {
        p.color[3] = 0.0;
    }
    Some(p)
}

/// `Enchant | Nautilus | VaultConnection`.
pub(super) fn enchant(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos + aux, rng);
    p.prev = pos + aux;
    p.look.uv = c.sprite(rng);
    p.vel = aux;
    p.quad_size = 0.1 * (rng.next_float() * 0.5 + 0.2);
    let br = rng.next_float() * 0.6 + 0.4;
    p.set_rgb([0.9 * br, 0.9 * br, br]);
    p.physics = false;
    p.free_move = true;
    p.lifetime = (rng.next_float() * 10.0) as i32 + 30;
    p.look.glow = Glow::EmissionQuartic;
    p.behavior = Behavior::FlyTowards { start: pos };
    if kind == P::VaultConnection {
        p.alpha_curve = LifetimeAlpha {
            start: 0.0,
            end: 0.6,
            from: 0.25,
            to: 1.0,
        };
        p.color[3] = 0.0;
        p.look.translucent = true;
        p.look.glow = Glow::Block;
        p.scale(1.5);
    }
    Some(p)
}

/// `EndRod | TotemOfUndying`.
pub(super) fn end_rod(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.friction = if kind == P::TotemOfUndying { 0.6 } else { 0.91 };
    p.gravity = if kind == P::TotemOfUndying {
        1.25
    } else {
        0.0125
    };
    p.vel = aux;
    p.quad_size *= 0.75;
    p.lifetime = 60 + rng.next_int(12);
    p.look.animated = true;
    p.look.translucent = true;
    p.look.glow = Glow::Full;
    let fade = if kind == P::EndRod {
        p.free_move = true;
        Some(rgb_of(15_916_745))
    } else {
        let rgb = if rng.next_int(4) == 0 {
            [
                0.6 + rng.next_float() * 0.2,
                0.6 + rng.next_float() * 0.3,
                rng.next_float() * 0.2,
            ]
        } else {
            [
                0.1 + rng.next_float() * 0.2,
                0.4 + rng.next_float() * 0.3,
                rng.next_float() * 0.2,
            ]
        };
        p.set_rgb(rgb);
        None
    };
    p.behavior = Behavior::Animated { fade };
    Some(p)
}

/// `Glow | WaxOn | WaxOff | ElectricSpark | Scrape`.
pub(super) fn glow(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let start = if kind == P::Glow {
        DVec3::new(0.5 - rng.next_double(), aux.y, 0.5 - rng.next_double())
    } else {
        DVec3::ZERO
    };
    let mut p = Particle::moving(kind, pos, start, rng);
    p.friction = 0.96;
    p.speed_up_when_blocked = true;
    p.quad_size *= 0.75;
    p.physics = false;
    p.look.animated = true;
    p.look.glow = Glow::Emission;
    match kind {
        P::Glow => {
            p.set_rgb(if rng.next_boolean() {
                [0.6, 1.0, 0.8]
            } else {
                [0.08, 0.4, 0.4]
            });
            p.vel.y *= f64::from(0.2f32);
            if aux.x == 0.0 && aux.z == 0.0 {
                p.vel.x *= f64::from(0.1f32);
                p.vel.z *= f64::from(0.1f32);
            }
            p.lifetime = (8.0 / (rng.next_double() * 0.8 + 0.2)) as i32;
        }
        P::ElectricSpark => {
            p.set_rgb([1.0, 0.9, 1.0]);
            p.vel = aux * 0.25;
            p.lifetime = rng.next_int(2) + 2;
        }
        P::Scrape => {
            p.set_rgb(if rng.next_boolean() {
                [0.29, 0.58, 0.51]
            } else {
                [0.43, 0.77, 0.62]
            });
            p.vel = aux * 0.01;
            p.lifetime = rng.next_int(30) + 10;
        }
        _ => {
            p.set_rgb(if kind == P::WaxOn {
                [0.91, 0.55, 0.08]
            } else {
                [1.0, 0.9, 1.0]
            });
            p.vel = DVec3::new(aux.x * 0.01 / 2.0, aux.y * 0.01, aux.z * 0.01 / 2.0);
            p.lifetime = rng.next_int(30) + 10;
        }
    }
    Some(p)
}

/// `Note`.
pub(super) fn note(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = c.sprite(rng);
    p.friction = 0.66;
    p.speed_up_when_blocked = true;
    p.vel *= f64::from(0.01f32);
    p.vel.y += 0.2;
    let note = aux.x as f32;
    let channel = |offset: f32| {
        ((note + offset) * crate::math::angle::TAU)
            .sin()
            .mul_add(0.65, 0.35)
            .max(0.0)
    };
    p.set_rgb([channel(0.0), channel(1.0 / 3.0), channel(2.0 / 3.0)]);
    p.quad_size *= 1.5;
    p.lifetime = 6;
    p.look.size = SizeCurve::GrowIn;
    Some(p)
}

/// `Portal | ReversePortal`.
pub(super) fn portal(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = c.sprite(rng);
    p.vel = aux;
    p.quad_size = 0.1 * (rng.next_float() * 0.2 + 0.5);
    let br = rng.next_float() * 0.6 + 0.4;
    p.set_rgb([br * 0.9, br * 0.3, br]);
    p.lifetime = (rng.next_float() * 10.0) as i32 + 40;
    p.free_move = true;
    p.look.size = SizeCurve::Portal;
    p.look.glow = Glow::EmissionQuartic;
    p.behavior = Behavior::Portal { start: pos };
    if kind == P::ReversePortal {
        p.quad_size *= 1.5;
        p.lifetime = (rng.next_float() * 2.0) as i32 + 60;
        p.look.size = SizeCurve::ReversePortal;
        p.behavior = Behavior::ReversePortal;
    }
    Some(p)
}

/// `Trail`.
pub(super) fn trail(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let ParticleData::Trail { target, rgb, ticks } = spec.data else {
        return None;
    };
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.look.uv = c.sprite(rng);
    let mut vary = |c: f32| (c * (0.875 + rng.next_float() * 0.25)).min(1.0);
    let [r, g, b] = rgb_of(rgb);
    p.set_rgb([vary(r), vary(g), vary(b)]);
    p.quad_size = 0.26;
    p.lifetime = ticks;
    p.look.glow = Glow::Full;
    p.behavior = Behavior::Trail {
        target: DVec3::from_array(target),
    };
    Some(p)
}

/// `Vibration`.
pub(super) fn vibration(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        ctx,
        ..
    } = *c;
    let ParticleData::Vibration { to, ticks } = spec.data else {
        return None;
    };
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = c.sprite(rng);
    p.quad_size = 0.3;
    p.lifetime = ticks;
    p.look.translucent = true;
    p.look.glow = Glow::Block;
    let (rot, pitch) =
        motion::destination(to, ctx).map_or((0.0, 0.0), |target| motion::heading(pos, target));
    p.behavior = Behavior::Vibration {
        to,
        rot,
        prev_rot: rot,
        pitch,
        prev_pitch: pitch,
    };
    Some(p)
}

/// `PauseMobGrowth | ResetMobGrowth`.
pub(super) fn mob_growth(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.look.uv = c.sprite(rng);
    p.vel = aux;
    p.gravity = 0.0;
    p.vel.y += if kind == P::ResetMobGrowth {
        0.03
    } else {
        -0.03
    };
    p.quad_size *= rng.next_float() * 0.6 + 0.5;
    p.lifetime = 8;
    Some(p)
}

/// `Firework`.
pub(super) fn firework(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.friction = 0.91;
    p.gravity = 0.1;
    p.vel = aux;
    p.quad_size *= 0.75;
    p.lifetime = 48 + rng.next_int(12);
    p.color[3] = 0.99;
    p.look.animated = true;
    p.look.translucent = true;
    p.look.glow = Glow::Full;
    p.behavior = Behavior::Animated { fade: None };
    Some(p)
}

/// `Flash`.
pub(super) fn flash(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec, kind, pos, ..
    } = *c;
    let uv = c.sprite(rng);
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    p.lifetime = 4;
    p.quad_size = 1.0;
    if let ParticleData::Color(argb) = spec.data {
        p.set_rgb(rgb_of(argb));
    }
    p.alpha_curve = LifetimeAlpha {
        start: 0.725,
        end: 0.225,
        from: 0.0,
        to: 1.0,
    };
    p.look.translucent = true;
    p.look.size = SizeCurve::Flash;
    Some(p)
}

/// `SculkCharge | SculkChargePop`.
pub(super) fn sculk_charge(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.friction = 0.96;
    p.scale(if kind == P::SculkCharge { 1.5 } else { 1.0 });
    p.physics = false;
    p.vel = aux;
    if let ParticleData::SculkCharge(roll) = spec.data {
        p.roll = roll;
        p.prev_roll = roll;
    }
    p.lifetime = if kind == P::SculkCharge {
        rng.next_int(12) + 8
    } else {
        rng.next_int(4) + 6
    };
    p.look.animated = true;
    p.look.translucent = true;
    p.look.glow = Glow::Block;
    Some(p)
}

/// `Shriek`.
pub(super) fn shriek(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec, kind, pos, ..
    } = *c;
    let ParticleData::Shriek(delay) = spec.data else {
        return None;
    };
    let uv = c.sprite(rng);
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = uv;
    p.quad_size = 0.85;
    p.lifetime = 30;
    p.gravity = 0.0;
    p.vel = DVec3::new(0.0, 0.1, 0.0);
    p.alpha_curve = LifetimeAlpha {
        start: 1.0,
        end: 0.0,
        from: 0.0,
        to: 1.0,
    };
    p.look.translucent = true;
    p.look.glow = Glow::Block;
    p.look.size = SizeCurve::Shriek;
    p.behavior = Behavior::Shriek { delay };
    Some(p)
}

/// `TrialSpawnerDetection | TrialSpawnerDetectionOminous`.
pub(super) fn trial_spawner_detection(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    const SCALE: f32 = 1.5;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.friction = 0.96;
    p.gravity = -0.1;
    p.speed_up_when_blocked = true;
    p.vel = DVec3::new(0.0, p.vel.y * 0.9, 0.0) + aux;
    p.quad_size *= 0.75 * SCALE;
    let between = rng.next_float() * 0.5 + 0.5;
    p.lifetime = ((8.0 / between * SCALE) as i32).max(1);
    p.look.animated = true;
    p.look.glow = Glow::Block;
    p.look.facing = crate::particle::base::Facing::Upright;
    p.look.size = SizeCurve::GrowIn;
    Some(p)
}

/// `OminousSpawning`.
pub(super) fn ominous_spawning(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let uv = c.sprite(rng);
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    p.vel = aux;
    p.pos = pos + aux;
    p.prev = p.pos;
    p.quad_size = 0.1 * (rng.next_float() * 0.5 + 0.2);
    p.physics = false;
    p.free_move = true;
    p.lifetime = (rng.next_float() * 5.0) as i32 + 25;
    p.look.glow = Glow::Block;
    p.behavior = Behavior::FlyStraight {
        start: pos,
        from: 0xFF45_AEFE,
        to: 0xFFFF_FFFF,
    };
    p.scale(rng.next_float() * 2.0 + 3.0);
    Some(p)
}
