//! Water and lava: bubbles, drips, rain, ink, geysers, the bobber's wake.

use super::smoke::ash_smoke;
use super::*;

/// `BubbleColumnUp`.
pub(super) fn bubble_column_up(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let uv = c.sprite(rng);
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    p.gravity = -0.125;
    p.friction = 0.85;
    p.width = 0.02;
    p.height = 0.02;
    p.quad_size *= rng.next_float() * 0.6 + 0.2;
    let mut drift =
        |a: f64| a * f64::from(0.2f32) + f64::from((rng.next_float() * 2.0 - 1.0) * 0.02);
    p.vel = DVec3::new(drift(aux.x), drift(aux.y), drift(aux.z));
    p.lifetime = (40.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.behavior = Behavior::InWater;
    Some(p)
}

/// `CurrentDown`.
pub(super) fn current_down(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let uv = c.sprite(rng);
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    p.lifetime = (rng.next_float() * 60.0) as i32 + 30;
    p.physics = false;
    p.vel = DVec3::new(0.0, -0.05, 0.0);
    p.width = 0.02;
    p.height = 0.02;
    p.quad_size *= rng.next_float() * 0.6 + 0.2;
    p.gravity = 0.002;
    p.behavior = Behavior::CurrentDown { angle: 0.0 };
    Some(p)
}

/// `Bubble`.
pub(super) fn bubble(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = c.sprite(rng);
    p.width = 0.02;
    p.height = 0.02;
    p.quad_size *= rng.next_float() * 0.6 + 0.2;
    let mut drift = |a: f64| a * 0.2 + f64::from((rng.next_float() * 2.0 - 1.0) * 0.02);
    p.vel = DVec3::new(drift(aux.x), drift(aux.y), drift(aux.z));
    p.lifetime = (8.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.behavior = Behavior::Bubble;
    Some(p)
}

/// `BubblePop`.
pub(super) fn bubble_pop(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.lifetime = 4;
    p.gravity = 0.008;
    p.vel = aux;
    p.look.animated = true;
    p.behavior = Behavior::BubblePop;
    Some(p)
}

/// `DrippingLava | FallingLava | LandingLava | DrippingWater | FallingWater | DrippingHoney | FallingHoney | LandingHoney | FallingNectar | FallingSporeBlossom | DrippingObsidianTear | FallingObsidianTear | LandingObsidianTear | DrippingDripstoneLava | FallingDripstoneLava | DrippingDripstoneWater | FallingDripstoneWater`.
pub(super) fn drip(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    Some(drip_particle(kind, pos, c.sprite(rng), rng))
}

/// `Lava`.
pub(super) fn lava(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, .. } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = c.sprite(rng);
    p.gravity = 0.75;
    p.friction = 0.999;
    p.vel *= f64::from(0.8f32);
    p.vel.y = f64::from(rng.next_float() * 0.4 + 0.05);
    p.quad_size *= rng.next_float() * 2.0 + 0.2;
    p.lifetime = (16.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.look.size = SizeCurve::Lava;
    p.look.glow = Glow::Block;
    p.behavior = Behavior::Lava;
    Some(p)
}

/// `Rain | Splash`.
pub(super) fn rain(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = c.sprite(rng);
    p.vel.x *= f64::from(0.3f32);
    p.vel.y = f64::from(rng.next_float() * 0.2 + 0.1);
    p.vel.z *= f64::from(0.3f32);
    p.width = 0.01;
    p.height = 0.01;
    p.gravity = 0.06;
    p.lifetime = (8.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.behavior = Behavior::WaterDrop;
    if kind == P::Splash {
        p.gravity = 0.04;
        if aux.y == 0.0 && (aux.x != 0.0 || aux.z != 0.0) {
            p.vel = DVec3::new(aux.x, 0.1, aux.z);
        }
    }
    Some(p)
}

/// `SquidInk | GlowSquidInk`.
pub(super) fn squid_ink(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let mut p = Particle::at(kind, pos, rng);
    p.friction = 0.92;
    p.quad_size = 0.5;
    p.set_rgb(if kind == P::GlowSquidInk {
        [0.2, 0.8, 0.6]
    } else {
        [0.0; 3]
    });
    p.lifetime = (p.quad_size * 12.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.physics = false;
    p.vel = aux;
    p.look.animated = true;
    p.look.translucent = true;
    p.look.glow = Glow::Full;
    p.behavior = Behavior::SquidInk;
    Some(p)
}

/// `SulfurBubbles`.
pub(super) fn sulfur_bubbles(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx { kind, pos, aux, .. } = *c;
    let uv = c.sprite(rng);
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    p.gravity = -0.04;
    p.friction = 0.85;
    p.width = 0.02;
    p.height = 0.02;
    let mut push =
        |a: f64| a * f64::from(0.2f32) + f64::from((rng.next_float() * 2.0 - 1.0) * 0.02);
    p.vel.x = push(aux.x);
    p.vel.z = push(aux.y);
    let size_start = 0.02 + 0.02 * rng.next_float();
    p.quad_size = size_start;
    p.lifetime = i32::MAX;
    p.behavior = Behavior::SulfurBubble {
        y_start: pos.y,
        y_prev: pos.y,
        size_start,
    };
    Some(p)
}

/// `Geyser`.
pub(super) fn geyser(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let ParticleData::Geyser { water_blocks } = spec.data else {
        return None;
    };
    let mut p = Particle::at(kind, pos, rng);
    p.lifetime = 20;
    p.behavior = Behavior::Emitter(Emitter::Geyser {
        aux: aux.as_vec3().to_array(),
        water_blocks,
    });
    Some(p)
}

/// `GeyserBase | GeyserPoof`.
pub(super) fn geyser_base(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let ParticleData::GeyserBase {
        water_blocks,
        impulse,
    } = spec.data
    else {
        return None;
    };
    let mut off = |spread: f32| f64::from((rng.next_float() - 0.5) * spread);
    let at = pos + DVec3::new(off(0.5), off(0.5) + f64::from(0.2f32), off(0.5));
    let burst = impulse + 0.25 * water_blocks as f32;
    let size = 3.0 + 0.125 * water_blocks as f32;
    let mut p = ash_smoke(kind, at, [burst; 3], aux, size, 0.0, 0, 0.0, true, rng);
    p.friction = 0.725;
    p.set_rgb([1.0; 3]);
    p.vel.y = p.vel.y.abs();
    p.lifetime = (25.0 * (0.8 + 0.2 * rng.next_float())) as i32;
    Some(p)
}

/// `GeyserPlume`.
pub(super) fn geyser_plume(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ..
    } = *c;
    let ParticleData::Geyser { water_blocks } = spec.data else {
        return None;
    };
    let x = f64::from((rng.next_float() - 0.5) * 0.2);
    let y = f64::from(rng.next_float());
    let z = f64::from((rng.next_float() - 0.5) * 0.2);
    let mut p = Particle::moving(kind, pos + DVec3::new(x, y, z), aux, rng);
    let height = 5 * water_blocks.max(1);
    p.speed_up_when_blocked = true;
    p.lifetime = height * 5;
    p.vel.y = 0.0;
    let spray = [
        (rng.next_float() - 0.5) * 0.2,
        (rng.next_float() - 0.5) * 0.2,
    ];
    p.friction = 1.0;
    let propulsion = if water_blocks == 1 { 1.5 } else { 1.0 } * height as f32 * 1.45;
    p.gravity = -propulsion;
    let randomized = p.quad_size * 0.75;
    let min_size = randomized * (2.0 + height as f32 / 8.0);
    let max_size = randomized * (3.0 + height as f32 / 8.0);
    p.quad_size = min_size;
    p.look.animated = true;
    p.behavior = Behavior::GeyserPlume(Plume {
        start_y: p.pos.y,
        height: height as f32,
        propulsion,
        spray,
        min_size,
        max_size,
        done: false,
    });
    Some(p)
}

/// `Fishing`.
pub(super) fn fishing(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        kind,
        pos,
        aux,
        ctx,
        ..
    } = *c;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.vel.x *= f64::from(0.3f32);
    p.vel.y = f64::from(rng.next_float() * 0.2 + 0.1);
    p.vel.z *= f64::from(0.3f32);
    p.width = 0.01;
    p.height = 0.01;
    p.lifetime = (8.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.gravity = 0.0;
    p.vel = aux;
    p.look.uv = ctx.sprites.from_age(kind, p.age, p.lifetime);
    p.behavior = Behavior::Wake;
    Some(p)
}

/// `DripParticle`'s providers: colour, stage and what each turns into.
fn drip_particle(kind: Kind, pos: DVec3, uv: [f32; 4], rng: &mut JavaRandom) -> Particle {
    use Kind as P;
    const WATER: [f32; 3] = [0.2, 0.3, 1.0];
    const LAVA: [f32; 3] = [1.0, 0.285_714_3, 0.083_333_336];
    const TEAR: [f32; 3] = [0.511_718_75, 0.031_25, 0.890_625];
    let mut p = Particle::at(kind, pos, rng);
    p.look.uv = uv;
    p.width = 0.01;
    p.height = 0.01;
    p.gravity = 0.06;
    let water = Some(FluidKind::Water);
    let lava = Some(FluidKind::Lava);
    let fall_and_land = |rng: &mut JavaRandom| (64.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    let land = |rng: &mut JavaRandom, of: f32| (of / (rng.next_float() * 0.8 + 0.2)) as i32;
    let (fluid, stage, next, cooling, rgb) = match kind {
        P::DrippingWater => (water, DripStage::Hang, Some(P::FallingWater), false, WATER),
        P::FallingWater => (water, DripStage::FallAndLand, Some(P::Splash), false, WATER),
        P::DrippingLava => (lava, DripStage::Hang, Some(P::FallingLava), true, [1.0; 3]),
        P::FallingLava => (
            lava,
            DripStage::FallAndLand,
            Some(P::LandingLava),
            false,
            LAVA,
        ),
        P::LandingLava => (lava, DripStage::Land, None, false, LAVA),
        P::DrippingDripstoneWater => (
            water,
            DripStage::Hang,
            Some(P::FallingDripstoneWater),
            false,
            WATER,
        ),
        P::FallingDripstoneWater => (water, DripStage::FallAndLand, Some(P::Splash), false, WATER),
        P::DrippingDripstoneLava => (
            lava,
            DripStage::Hang,
            Some(P::FallingDripstoneLava),
            true,
            [1.0; 3],
        ),
        P::FallingDripstoneLava => (
            lava,
            DripStage::FallAndLand,
            Some(P::LandingLava),
            false,
            LAVA,
        ),
        P::DrippingHoney => (
            None,
            DripStage::Hang,
            Some(P::FallingHoney),
            false,
            [0.622, 0.508, 0.082],
        ),
        P::FallingHoney => (
            None,
            DripStage::FallAndLand,
            Some(P::LandingHoney),
            false,
            [0.582, 0.448, 0.082],
        ),
        P::LandingHoney => (None, DripStage::Land, None, false, [0.522, 0.408, 0.082]),
        P::FallingNectar => (None, DripStage::Fall, None, false, [0.92, 0.782, 0.72]),
        P::FallingSporeBlossom => (None, DripStage::Fall, None, false, [0.32, 0.5, 0.22]),
        P::DrippingObsidianTear => (
            None,
            DripStage::Hang,
            Some(P::FallingObsidianTear),
            false,
            TEAR,
        ),
        P::FallingObsidianTear => (
            None,
            DripStage::FallAndLand,
            Some(P::LandingObsidianTear),
            false,
            TEAR,
        ),
        _ => (None, DripStage::Land, None, false, TEAR),
    };
    p.set_rgb(rgb);
    match stage {
        DripStage::Hang => {
            p.gravity *= 0.02;
            p.lifetime = 40;
        }
        DripStage::FallAndLand => p.lifetime = fall_and_land(rng),
        DripStage::Land => p.lifetime = land(rng, 16.0),
        DripStage::Fall => {}
    }
    match kind {
        P::FallingHoney => p.gravity = 0.01,
        P::DrippingHoney | P::DrippingObsidianTear => {
            p.gravity *= 0.01;
            p.lifetime = 100;
        }
        P::LandingHoney => p.lifetime = land(rng, 128.0),
        P::FallingNectar => {
            p.lifetime = land(rng, 16.0);
            p.gravity = 0.007;
        }
        P::FallingSporeBlossom => {
            p.lifetime = (64.0 / lerp(rng.next_float(), 0.1, 0.9)) as i32;
            p.gravity = 0.005;
        }
        P::FallingObsidianTear => p.gravity = 0.01,
        P::LandingObsidianTear => p.lifetime = land(rng, 28.0),
        _ => {}
    }
    if matches!(
        kind,
        P::DrippingObsidianTear | P::FallingObsidianTear | P::LandingObsidianTear
    ) {
        p.look.glow = Glow::Block;
    }
    p.behavior = Behavior::Drip(Drip {
        fluid,
        stage,
        next,
        cooling,
    });
    p
}
