//! Blocks: their breaking, markers, falling dust and item crumbs.

use super::*;

/// `Block | BlockCrumble | DustPillar`.
pub(super) fn block(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ctx,
        ..
    } = *c;
    let ParticleData::Block(state) = spec.data else {
        return None;
    };
    let mut p = terrain(kind, pos, aux, state, ctx, rng)?;
    if kind == P::BlockCrumble {
        p.vel = DVec3::ZERO;
        p.lifetime = rng.next_int(10) + 1;
    } else if kind == P::DustPillar {
        p.vel = DVec3::new(
            rng.next_gaussian() / 30.0,
            aux.y + rng.next_gaussian() / 2.0,
            rng.next_gaussian() / 30.0,
        );
        p.lifetime = rng.next_int(20) + 20;
    }
    Some(p)
}

/// `BlockMarker`.
pub(super) fn block_marker(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        ctx,
        ..
    } = *c;
    let ParticleData::Block(state) = spec.data else {
        return None;
    };
    let look = ctx.blocks?.particle_look(state)?;
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.look.uv = look.uv;
    p.look.translucent = look.translucent;
    p.gravity = 0.0;
    p.lifetime = 80;
    p.physics = false;
    p.quad_size = 0.5;
    p.look.size = SizeCurve::Marker;
    Some(p)
}

/// `FallingDust`.
pub(super) fn falling_dust(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        ctx,
        ..
    } = *c;
    let ParticleData::Block(state) = spec.data else {
        return None;
    };
    let at = pos.floor().as_ivec3();
    let [r, g, b] = dust_color(ctx, state, [at.x, at.y, at.z]);
    let mut p = Particle::at(kind, pos, rng);
    p.set_rgb([r, g, b]);
    p.quad_size *= 0.674_999_95;
    let base = (32.0 / (rng.next_float() * 0.8 + 0.2)) as i32;
    p.lifetime = ((base as f32 * 0.9) as i32).max(1);
    p.look.animated = true;
    p.look.size = SizeCurve::GrowIn;
    let spin = (rng.next_float() - 0.5) * 0.1;
    p.roll = rng.next_float() * crate::math::angle::TAU;
    p.prev_roll = p.roll;
    p.behavior = Behavior::FallingDust { spin };
    Some(p)
}

/// `Item | ItemSlime | ItemCobweb | ItemSnowball | SulfurCubeGoo`.
pub(super) fn item(c: &Cx, rng: &mut JavaRandom) -> Option<Particle> {
    let Cx {
        spec,
        kind,
        pos,
        aux,
        ctx,
        ..
    } = *c;
    let item = match (kind, spec.data) {
        (P::Item, ParticleData::Item(item)) => Some(item),
        (P::ItemSlime, _) => Some(pumpkin_data::item::Item::SLIME_BALL.id),
        (P::ItemCobweb, _) => Some(pumpkin_data::item::Item::COBWEB.id),
        (P::ItemSnowball, _) => Some(pumpkin_data::item::Item::SNOWBALL.id),
        _ => None,
    };
    let mut p = Particle::moving(kind, pos, DVec3::ZERO, rng);
    p.gravity = 1.0;
    p.quad_size /= 2.0;
    if kind == P::Item {
        p.vel = p.vel * f64::from(0.1f32) + aux;
    }
    let (atlas, whole) = match item {
        Some(item) => (Atlas::Item(item), [0.0, 0.0, 1.0, 1.0]),
        None => (Atlas::Blocks, ctx.sprites.first(kind)),
    };
    p.look.atlas = atlas;
    p.look.uv = quarter(whole, rng);
    Some(p)
}

/// `TerrainParticle`.
fn terrain(
    kind: Kind,
    pos: DVec3,
    aux: DVec3,
    state: u16,
    ctx: &TickCtx,
    rng: &mut JavaRandom,
) -> Option<Particle> {
    if state == 0 || is_moving_piston(state) {
        return None;
    }
    let models = ctx.blocks?;
    let look = models.particle_look(state)?;
    let mut p = Particle::moving(kind, pos, aux, rng);
    p.gravity = 1.0;
    let mut rgb = look.rgb.map(|c| c * 0.6);
    if look.tint != crate::block_tint::TintKind::None {
        let at = pos.floor().as_ivec3();
        let c = ctx.level.tint_at(look.tint, [at.x, at.y, at.z]);
        for (out, c) in rgb.iter_mut().zip(c) {
            *out *= f32::from(c) / 255.0;
        }
    }
    p.set_rgb(rgb);
    p.quad_size /= 2.0;
    p.look.uv = quarter(look.uv, rng);
    p.look.translucent = look.translucent;
    Some(p)
}

/// A random quarter of `rect`, as `TerrainParticle` picks its UV.
fn quarter(rect: [f32; 4], rng: &mut JavaRandom) -> [f32; 4] {
    let uo = rng.next_float() * 3.0;
    let vo = rng.next_float() * 3.0;
    let u = |f: f32| rect[0] + (rect[2] - rect[0]) * f;
    let v = |f: f32| rect[1] + (rect[3] - rect[1]) * f;
    [
        u((uo + 1.0) / 4.0),
        v(vo / 4.0),
        u(uo / 4.0),
        v((vo + 1.0) / 4.0),
    ]
}

fn is_moving_piston(state: u16) -> bool {
    crate::particle::client::block_name(state) == "moving_piston"
}

/// `FallingDustParticle`'s colour.
fn dust_color(ctx: &TickCtx, state: u16, at: [i32; 3]) -> [f32; 3] {
    let name = crate::particle::client::block_name(state);
    let dust = match name {
        "sand" | "suspicious_sand" => Some(14_406_560),
        "red_sand" => Some(11_098_145),
        "gravel" | "suspicious_gravel" => Some(8_356_727),
        "dragon_egg" => Some(0),
        _ => None,
    };
    if let Some(rgb) = dust {
        return rgb_of(rgb);
    }
    if let Some(look) = ctx.blocks.and_then(|m| m.particle_look(state))
        && look.tint_source
    {
        if look.tint == crate::block_tint::TintKind::None {
            return look.rgb;
        }
        let [r, g, b] = ctx.level.tint_at(look.tint, at);
        return [r, g, b].map(|c| f32::from(c) / 255.0);
    }
    let map = pumpkin_data::BlockStateId::new(state)
        .map_or(0, |id| pumpkin_data::Block::from_state_id(id).map_color);
    rgb_of(crate::map_color::rgb(map))
}
