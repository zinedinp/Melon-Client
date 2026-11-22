//! The `tick` of each particle class, as an enum.

use glam::DVec3;

use super::base::Particle;
use super::motion::{self, emitter::Emitter};
use super::options::{ParticleSpec, PositionSource};
use super::{Kind, Spawn, TickCtx};
use crate::world::fluid::FluidKind;
use crate::world::java_random::JavaRandom;
use crate::world::physics;

/// Where a drip is in its fall (`DripParticle`'s inner classes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DripStage {
    /// `DripHangParticle`: hangs, then becomes `next`.
    Hang,
    /// `FallingParticle`: gone where it lands.
    Fall,
    /// `FallAndLandParticle`: becomes `next` where it lands.
    FallAndLand,
    /// `DripLandParticle`: lies where it fell.
    Land,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drip {
    /// `Fluids.EMPTY` is `None`: honey, nectar, tears and spores.
    pub fluid: Option<FluidKind>,
    pub stage: DripStage,
    /// The particle it turns into.
    pub next: Option<Kind>,
    /// `CoolingDripHangParticle`: lava drops darken from white-hot.
    pub cooling: bool,
}

/// `GeyserPlumeParticle`'s own fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plume {
    pub start_y: f64,
    /// `plumeHeight`: it tops out at `start_y + height - 1`.
    pub height: f32,
    pub propulsion: f32,
    /// `horizontalSprayX`, `horizontalSprayZ`.
    pub spray: [f32; 2],
    pub min_size: f32,
    pub max_size: f32,
    pub done: bool,
}

/// `FallingParticle`'s own fields (falling leaves).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    pub rot_speed: f32,
    pub spin: f32,
    /// `windBig`: the side push.
    pub wind: f32,
    pub swirl: bool,
    pub flow_away: bool,
    /// `xaFlowScale`, `zaFlowScale`.
    pub flow: [f32; 2],
    pub swirl_period: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Behavior {
    /// `Particle.tick`.
    #[default]
    Default,
    /// `CritParticle`: the default, fading from white to red.
    Crit,
    /// Only ages (`HugeExplosionParticle`, `AttackSweepParticle`).
    Still,
    /// `SuspendedTownParticle`: drifts, slowing, through blocks.
    Town,
    /// `WaterDropParticle` (rain, splash).
    WaterDrop,
    /// `BubbleParticle`: rises in water, gone out of it.
    Bubble,
    /// `BubblePopParticle`.
    BubblePop,
    /// `LavaParticle`: the default, trailing smoke.
    Lava,
    /// `CampfireSmokeParticle`: rises slowly and fades.
    Campfire,
    /// `SimpleAnimatedParticle`: fades in its second half, toward `fade`.
    Animated {
        fade: Option<[f32; 3]>,
    },
    /// `SquidInkParticle`.
    SquidInk,
    /// `PortalParticle`.
    Portal {
        start: DVec3,
    },
    ReversePortal,
    /// `FlyTowardsPositionParticle`.
    FlyTowards {
        start: DVec3,
    },
    /// `TrailParticle`.
    Trail {
        target: DVec3,
    },
    /// `VibrationSignalParticle`: yaw and pitch toward where it goes.
    Vibration {
        to: PositionSource,
        rot: f32,
        prev_rot: f32,
        pitch: f32,
        prev_pitch: f32,
    },
    /// `FallingDustParticle`: spins as it falls.
    FallingDust {
        spin: f32,
    },
    /// `DragonBreathParticle`: rises once it touched the ground.
    DragonBreath {
        landed: bool,
    },
    /// `SnowflakeParticle`.
    Snowflake,
    /// `SpellParticle`: eases back to its alpha.
    Spell {
        alpha: f32,
    },
    /// `DripParticle`.
    Drip(Drip),
    /// `DustColorTransitionParticle`: from one colour to the other.
    DustTransition {
        from: [f32; 3],
        to: [f32; 3],
    },
    /// `BubbleColumnUpParticle`: the default, gone out of water.
    InWater,
    /// `WaterCurrentDownParticle`: circles down through water.
    CurrentDown {
        angle: f32,
    },
    /// `SulfurBubbleParticle`.
    SulfurBubble {
        y_start: f64,
        y_prev: f64,
        size_start: f32,
    },
    /// `NoxiousGasParticle`: the default, fading out in its second half.
    NoxiousGas,
    /// `GeyserPlumeParticle`.
    GeyserPlume(Plume),
    /// `WakeParticle` (`fishing`): drifts, growing, through four sprites.
    Wake,
    /// `FallingParticle` (falling leaves): sways and spins down.
    Leaf(Leaf),
    /// `ShriekParticle`: unseen for `delay` ticks, then rises.
    Shriek {
        delay: i32,
    },
    /// `DustPlumeParticle`: the default, its pull and drag weakening.
    DustPlume,
    /// `FlyStraightTowardsParticle`.
    FlyStraight {
        start: DVec3,
        from: u32,
        to: u32,
    },
    /// `FireflyParticle`: wanders, glowing in and out.
    Firefly,
    /// `PlayerCloudParticle`: the default, sinking to a player under it.
    PlayerCloud,
    /// Draws nothing, spawns others.
    Emitter(Emitter),
}

impl Behavior {
    /// Whether it is drawn (`NoRenderParticle` isn't).
    pub fn drawn(self) -> bool {
        match self {
            Self::Emitter(_) => false,
            Self::Shriek { delay } => delay <= 0,
            _ => true,
        }
    }
}

/// One tick of `p`, spawning into `out` what it gives off.
pub fn tick(p: &mut Particle, ctx: &TickCtx, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    match p.behavior {
        Behavior::Default | Behavior::Lava | Behavior::Animated { .. } => {
            p.base_tick(ctx);
        }
        _ => {}
    }
    match p.behavior {
        Behavior::Default => {}
        Behavior::Crit => {
            p.base_tick(ctx);
            p.color[1] *= 0.96;
            p.color[2] *= 0.9;
        }
        Behavior::Still => {
            p.begin_tick();
            p.age_out();
        }
        Behavior::Town => {
            p.begin_tick();
            if !p.count_down() {
                p.pos += p.vel;
                p.vel *= 0.99;
            }
        }
        Behavior::WaterDrop => water_drop(p, ctx, rng),
        Behavior::Bubble => {
            p.begin_tick();
            if p.count_down() {
                return;
            }
            p.vel.y += 0.002;
            p.move_by(ctx, p.vel);
            p.vel *= f64::from(0.85f32);
            let cell = p.pos.floor().as_ivec3();
            if physics::fluid_at(ctx.level, cell.x, cell.y, cell.z) != Some(FluidKind::Water) {
                p.removed = true;
            }
        }
        Behavior::BubblePop => {
            p.begin_tick();
            if p.age_out() {
                return;
            }
            p.vel.y -= f64::from(p.gravity);
            p.move_by(ctx, p.vel);
        }
        Behavior::Lava => {
            if !p.removed {
                let odds = p.age as f32 / p.lifetime.max(1) as f32;
                if rng.next_float() > odds {
                    out.push(Spawn::new(ParticleSpec::simple(Kind::Smoke), p.pos, p.vel));
                }
            }
        }
        Behavior::Campfire => {
            p.begin_tick();
            let old = p.age;
            p.age += 1;
            if old >= p.lifetime || p.color[3] <= 0.0 {
                p.removed = true;
                return;
            }
            let mut sway = || {
                let side = if rng.next_boolean() { 1.0 } else { -1.0 };
                f64::from(rng.next_float() / 5000.0) * side
            };
            p.vel.x += sway();
            p.vel.z += sway();
            p.vel.y -= f64::from(p.gravity);
            p.move_by(ctx, p.vel);
            if p.age >= p.lifetime - 60 && p.color[3] > 0.01 {
                p.color[3] -= 0.015;
            }
        }
        Behavior::Animated { fade } => {
            if p.age > p.lifetime / 2 {
                p.color[3] = 1.0 - (p.age - p.lifetime / 2) as f32 / p.lifetime.max(1) as f32;
                if let Some([r, g, b]) = fade {
                    p.color[0] += (r - p.color[0]) * 0.2;
                    p.color[1] += (g - p.color[1]) * 0.2;
                    p.color[2] += (b - p.color[2]) * 0.2;
                }
            }
        }
        Behavior::SquidInk => {
            p.base_tick(ctx);
            if p.removed {
                return;
            }
            if p.age > p.lifetime / 2 {
                p.color[3] = 1.0 - (p.age - p.lifetime / 2) as f32 / p.lifetime.max(1) as f32;
            }
            let cell = p.pos.floor().as_ivec3();
            if ctx.level.block_state(cell.x, cell.y, cell.z).unwrap_or(0) == 0 {
                p.vel.y -= f64::from(0.0074f32);
            }
        }
        Behavior::Portal { start } => motion::portal(p, start),
        Behavior::ReversePortal => motion::reverse_portal(p),
        Behavior::FlyTowards { start } => motion::fly_towards(p, start),
        Behavior::Trail { target } => motion::trail(p, target),
        Behavior::Vibration { .. } => motion::vibration(p, ctx),
        Behavior::FallingDust { spin } => {
            p.begin_tick();
            if p.age_out() {
                return;
            }
            p.roll += crate::math::angle::PI * spin * 2.0;
            if p.on_ground {
                p.prev_roll = 0.0;
                p.roll = 0.0;
            }
            p.move_by(ctx, p.vel);
            p.vel.y -= f64::from(0.003f32);
            p.vel.y = p.vel.y.max(f64::from(-0.14f32));
        }
        Behavior::DragonBreath { landed } => {
            p.begin_tick();
            if p.age_out() {
                return;
            }
            let mut landed = landed;
            if p.on_ground {
                p.vel.y = 0.0;
                landed = true;
            }
            if landed {
                p.vel.y += 0.002;
            }
            let y = p.pos.y;
            p.move_by(ctx, p.vel);
            if p.pos.y == y {
                p.vel.x *= 1.1;
                p.vel.z *= 1.1;
            }
            let friction = f64::from(p.friction);
            p.vel.x *= friction;
            p.vel.z *= friction;
            if landed {
                p.vel.y *= friction;
            }
            p.behavior = Behavior::DragonBreath { landed };
        }
        Behavior::Snowflake => {
            p.base_tick(ctx);
            p.vel.x *= f64::from(0.95f32);
            p.vel.y *= f64::from(0.9f32);
            p.vel.z *= f64::from(0.95f32);
        }
        Behavior::Spell { alpha } => {
            p.base_tick(ctx);
            if ctx.close_to_scope(p.pos) {
                p.color[3] = 0.0;
            } else {
                p.color[3] += (alpha - p.color[3]) * 0.05;
            }
        }
        Behavior::Drip(drip) => tick_drip(p, drip, ctx, out),
        Behavior::DustTransition { .. } => p.base_tick(ctx),
        Behavior::InWater => {
            p.base_tick(ctx);
            let cell = p.pos.floor().as_ivec3();
            if !p.removed
                && physics::fluid_at(ctx.level, cell.x, cell.y, cell.z) != Some(FluidKind::Water)
            {
                p.removed = true;
            }
        }
        Behavior::CurrentDown { angle } => {
            p.begin_tick();
            if p.age_out() {
                return;
            }
            let (sin, cos) = (crate::math::sin(angle), crate::math::cos(angle));
            p.vel.x = (p.vel.x + f64::from(0.6 * cos)) * 0.07;
            p.vel.z = (p.vel.z + f64::from(0.6 * sin)) * 0.07;
            p.move_by(ctx, p.vel);
            let cell = p.pos.floor().as_ivec3();
            if physics::fluid_at(ctx.level, cell.x, cell.y, cell.z) != Some(FluidKind::Water)
                || p.on_ground
            {
                p.removed = true;
            }
            p.behavior = Behavior::CurrentDown {
                angle: angle + 0.08,
            };
        }
        Behavior::SulfurBubble {
            y_start,
            y_prev,
            size_start,
        } => sulfur_bubble(p, ctx, rng, y_start, y_prev, size_start),
        Behavior::NoxiousGas => {
            p.base_tick(ctx);
            let fade_start = p.lifetime as f32 / 2.0;
            if p.age as f32 > fade_start {
                let life = p.lifetime as f32;
                p.color[3] = (life - (p.age as f32 - fade_start)) / life;
            }
        }
        Behavior::GeyserPlume(plume) => geyser_plume(p, ctx, plume),
        Behavior::Wake => {
            p.begin_tick();
            let life = 60 - p.lifetime;
            if p.count_down() {
                return;
            }
            p.vel.y -= f64::from(p.gravity);
            p.move_by(ctx, p.vel);
            p.vel *= f64::from(0.98f32);
            let size = life as f32 * 0.001;
            p.width = size;
            p.height = size;
            p.look.uv = ctx.sprites.from_age(p.kind, life % 4, 4);
        }
        Behavior::Leaf(leaf) => falling_leaf(p, ctx, leaf),
        Behavior::Shriek { delay } => {
            if delay > 0 {
                p.begin_tick();
                p.behavior = Behavior::Shriek { delay: delay - 1 };
            } else {
                p.base_tick(ctx);
            }
        }
        Behavior::DustPlume => {
            p.gravity *= 0.88;
            p.friction *= 0.92;
            p.base_tick(ctx);
        }
        Behavior::FlyStraight { start, from, to } => {
            p.begin_tick();
            if p.age_out() {
                return;
            }
            let t = p.age as f32 / p.lifetime.max(1) as f32;
            p.pos = start + p.vel * f64::from(1.0 - t);
            let [a, r, g, b] = srgb_lerp(t, from, to);
            p.color = [r, g, b, a];
        }
        Behavior::Firefly => {
            p.base_tick(ctx);
            let cell = p.pos.floor().as_ivec3();
            let state = ctx.level.block_state(cell.x, cell.y, cell.z).unwrap_or(0);
            if !super::client::block_name(state).ends_with("air") {
                p.removed = true;
                return;
            }
            let progress = (p.age as f32 / p.lifetime.max(1) as f32).clamp(0.0, 1.0);
            p.color[3] = super::animation::fade::firefly_fade(progress, 0.3, 0.5);
            if rng.next_float() > 0.95 || p.age == 1 {
                let mut speed = || f64::from(-0.05 + 0.1 * rng.next_float());
                p.vel = DVec3::new(speed(), speed(), speed());
            }
        }
        Behavior::PlayerCloud => {
            p.base_tick(ctx);
            if !p.removed
                && let Some(player) = (ctx.nearest_player)(p.pos, 2.0)
                && p.pos.y > player.pos.y
            {
                p.pos.y += (player.pos.y - p.pos.y) * 0.2;
                p.vel.y += (player.vel.y - p.vel.y) * 0.2;
            }
        }
        Behavior::Emitter(emitter) => motion::emitter::tick(p, emitter, ctx, rng, out),
    }
}

/// `ARGB.srgbLerp`: each channel straight between the two, as `[a, r, g, b]`.
fn srgb_lerp(t: f32, from: u32, to: u32) -> [f32; 4] {
    std::array::from_fn(|i| {
        let shift = 24 - 8 * i as u32;
        let a = ((from >> shift) & 0xFF) as f32;
        let b = ((to >> shift) & 0xFF) as f32;
        (a + t * (b - a)).floor() / 255.0
    })
}

/// Whether `cell` holds a water source (`isSourceOfType(WATER)`).
pub fn water_source(ctx: &TickCtx, cell: glam::IVec3) -> bool {
    let state = physics::block_at(ctx.level, cell.x, cell.y, cell.z);
    crate::world::fluid::fluid_of(state)
        .is_some_and(|f| f.kind == FluidKind::Water && f.amount == 8 && !f.falling)
}

/// `SulfurBubbleParticle.tick`.
fn sulfur_bubble(
    p: &mut Particle,
    ctx: &TickCtx,
    rng: &mut JavaRandom,
    y_start: f64,
    y_prev: f64,
    size_start: f32,
) {
    const RISE: f64 = 3.0;
    p.base_tick(ctx);
    if !p.removed && !water_source(ctx, p.pos.floor().as_ivec3()) {
        p.removed = true;
    }
    if !p.removed && (p.pos.y >= y_start + RISE || p.pos.y <= y_prev) {
        p.removed = true;
    }
    let mut wiggle = || {
        let side = if rng.next_boolean() { 1.0 } else { -1.0 };
        f64::from(rng.next_float() * 0.003 * side) * 0.5
    };
    p.vel.x += wiggle();
    p.vel.z += wiggle();
    p.move_by(ctx, DVec3::new(p.vel.x, 0.0, p.vel.z));
    let progress = ((p.pos.y - y_start) / RISE) as f32;
    p.quad_size = size_start + progress * (0.15 - size_start);
    p.behavior = Behavior::SulfurBubble {
        y_start,
        y_prev: p.pos.y,
        size_start,
    };
}

/// `GeyserPlumeParticle.tick`.
fn geyser_plume(p: &mut Particle, ctx: &TickCtx, mut plume: Plume) {
    let y_before = p.pos.y;
    p.base_tick(ctx);
    let max_y = plume.start_y + f64::from(plume.height) - 1.0;
    if !plume.done && (p.vel.y < 0.0 || p.pos.y > max_y || p.pos.y == y_before) {
        p.lifetime = p.lifetime.min(p.age + 5);
        p.friction = 0.0;
        plume.done = true;
    }
    let linear = ((p.pos.y - plume.start_y) / (max_y - plume.start_y)).clamp(0.0, 1.0);
    p.gravity = plume.propulsion * linear.powi(3) as f32 * 0.12;
    p.vel.x = linear * f64::from(plume.spray[0]);
    p.vel.z = linear * f64::from(plume.spray[1]);
    p.quad_size = plume.min_size + (linear as f32) * (plume.max_size - plume.min_size);
    p.behavior = Behavior::GeyserPlume(plume);
}

/// `FallingParticle.tick`: the leaf's swirl and drift, spinning faster.
fn falling_leaf(p: &mut Particle, ctx: &TickCtx, mut leaf: Leaf) {
    const LIFE: f32 = 300.0;
    const ACCELERATION: f64 = 0.0025;
    p.begin_tick();
    if p.count_down() {
        return;
    }
    let alive = LIFE - p.lifetime as f32;
    let age = (alive / LIFE).min(1.0);
    let mut push = [0.0f64; 2];
    if leaf.flow_away {
        let curve = f64::from(age).powf(1.25);
        push[0] += f64::from(leaf.flow[0]) * curve;
        push[1] += f64::from(leaf.flow[1]) * curve;
    }
    if leaf.swirl {
        let turn = f64::from(age) * f64::from(leaf.swirl_period);
        push[0] += f64::from(age) * turn.cos() * f64::from(leaf.wind);
        push[1] += f64::from(age) * turn.sin() * f64::from(leaf.wind);
    }
    p.vel.x += push[0] * ACCELERATION;
    p.vel.z += push[1] * ACCELERATION;
    p.vel.y -= f64::from(p.gravity);
    leaf.rot_speed += leaf.spin / 20.0;
    p.roll += leaf.rot_speed / 20.0;
    p.move_by(ctx, p.vel);
    if p.on_ground || (p.lifetime < 299 && (p.vel.x == 0.0 || p.vel.z == 0.0)) {
        p.removed = true;
    }
    if !p.removed {
        p.vel *= f64::from(p.friction);
    }
    p.behavior = Behavior::Leaf(leaf);
}

/// `WaterDropParticle.tick`.
fn water_drop(p: &mut Particle, ctx: &TickCtx, rng: &mut JavaRandom) {
    p.begin_tick();
    if p.count_down() {
        return;
    }
    p.vel.y -= f64::from(p.gravity);
    p.move_by(ctx, p.vel);
    p.vel *= f64::from(0.98f32);
    if p.on_ground {
        if rng.next_float() < 0.5 {
            p.removed = true;
        }
        p.vel.x *= f64::from(0.7f32);
        p.vel.z *= f64::from(0.7f32);
    }
    let cell = p.pos.floor().as_ivec3();
    let inside = surface_at(ctx, cell, p.pos);
    if inside > 0.0 && p.pos.y < f64::from(cell.y) + inside {
        p.removed = true;
    }
}

/// The top of the block's collision shape or fluid in `cell` under `pos`.
pub fn surface_at(ctx: &TickCtx, cell: glam::IVec3, pos: DVec3) -> f64 {
    let shape = physics::block_shapes(ctx.level, (cell.x, cell.y, cell.z))
        .filter(|b| b.min.x <= pos.x && pos.x <= b.max.x && b.min.z <= pos.z && pos.z <= b.max.z)
        .map(|b| b.max.y - f64::from(cell.y))
        .fold(0.0, f64::max);
    let fluid = [FluidKind::Water, FluidKind::Lava]
        .into_iter()
        .filter_map(|kind| physics::fluid_height(ctx.level, cell, kind))
        .fold(0.0f32, f32::max);
    shape.max(f64::from(fluid))
}

/// `DripParticle.tick` with its stage's `preMoveUpdate` and `postMoveUpdate`.
fn tick_drip(p: &mut Particle, drip: Drip, ctx: &TickCtx, out: &mut Vec<Spawn>) {
    p.begin_tick();
    if drip.cooling {
        p.color[0] = 1.0;
        p.color[1] = 16.0 / (40 - p.lifetime + 16) as f32;
        p.color[2] = 4.0 / (40 - p.lifetime + 8) as f32;
    }
    if p.count_down() {
        if drip.stage == DripStage::Hang
            && let Some(next) = drip.next
        {
            out.push(Spawn::new(ParticleSpec::simple(next), p.pos, p.vel));
        }
        return;
    }
    p.vel.y -= f64::from(p.gravity);
    p.move_by(ctx, p.vel);
    match drip.stage {
        DripStage::Hang => p.vel *= 0.02,
        DripStage::Fall if p.on_ground => p.removed = true,
        DripStage::FallAndLand if p.on_ground => {
            p.removed = true;
            if let Some(next) = drip.next {
                out.push(Spawn::new(ParticleSpec::simple(next), p.pos, DVec3::ZERO));
            }
        }
        _ => {}
    }
    if p.removed {
        return;
    }
    p.vel *= f64::from(0.98f32);
    if let Some(fluid) = drip.fluid {
        let cell = p.pos.floor().as_ivec3();
        if let Some(height) = physics::fluid_height(ctx.level, cell, fluid)
            && p.pos.y < f64::from(cell.y) + f64::from(height)
        {
            p.removed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particle::animation::SpriteSets;
    use crate::particle::options::ParticleData;
    use crate::particle::provider;
    use crate::world::level::Level;

    fn with_ctx(f: impl FnOnce(&TickCtx)) {
        let level = Level::default();
        let sprites = SpriteSets::default();
        let entity = |_| None;
        let ctx = TickCtx {
            level: &level,
            sprites: &sprites,
            blocks: None,
            entity: &entity,
            nearest_player: &|_, _| None,
            scoping_eye: None,
        };
        f(&ctx);
    }

    fn spec(kind: Kind, data: ParticleData) -> ParticleSpec {
        ParticleSpec { kind, data }
    }

    #[test]
    fn every_type_is_made_and_ticks_without_panicking() {
        with_ctx(|ctx| {
            let mut rng = JavaRandom::new(7);
            let mut out = Vec::new();
            for kind in crate::particle::kind::all() {
                let data = match kind {
                    Kind::Geyser | Kind::GeyserPlume => ParticleData::Geyser { water_blocks: 2 },
                    Kind::GeyserBase | Kind::GeyserPoof => ParticleData::GeyserBase {
                        water_blocks: 2,
                        impulse: 1.5,
                    },
                    Kind::Shriek => ParticleData::Shriek(0),
                    Kind::SculkCharge => ParticleData::SculkCharge(1.0),
                    Kind::Flash | Kind::TintedLeaves => ParticleData::Color(0xFF20_8040),
                    _ => ParticleData::None,
                };
                let at = DVec3::new(0.5, 64.0, 0.5);
                let Some(mut p) = provider::create(
                    &spec(kind, data),
                    at,
                    DVec3::new(0.1, 0.2, 0.1),
                    ctx,
                    &mut rng,
                ) else {
                    continue;
                };
                for _ in 0..40 {
                    tick(&mut p, ctx, &mut rng, &mut out);
                }
            }
        });
    }

    #[test]
    fn a_geyser_sends_up_its_plume_base_and_puffs() {
        with_ctx(|ctx| {
            let mut rng = JavaRandom::new(1);
            let geyser = spec(Kind::Geyser, ParticleData::Geyser { water_blocks: 3 });
            let mut p = provider::create(&geyser, DVec3::ZERO, DVec3::ZERO, ctx, &mut rng).unwrap();
            assert!(!p.behavior.drawn());
            let mut out = Vec::new();
            tick(&mut p, ctx, &mut rng, &mut out);
            assert_eq!(out.len(), 5);
            assert!(out.iter().all(|s| s.spec.kind == Kind::GeyserPlume));
            out.clear();
            for _ in 0..9 {
                tick(&mut p, ctx, &mut rng, &mut out);
            }
            let count = |kind| out.iter().filter(|s| s.spec.kind == kind).count();
            assert_eq!(count(Kind::GeyserBase), 10);
            assert_eq!(count(Kind::GeyserPlume), 45);
            assert_eq!(count(Kind::GeyserPoof), 20);
        });
    }

    #[test]
    fn a_shriek_waits_unseen_then_rises() {
        with_ctx(|ctx| {
            let mut rng = JavaRandom::new(1);
            let shriek = spec(Kind::Shriek, ParticleData::Shriek(2));
            let mut p = provider::create(&shriek, DVec3::ZERO, DVec3::ZERO, ctx, &mut rng).unwrap();
            let mut out = Vec::new();
            assert!(!p.behavior.drawn());
            tick(&mut p, ctx, &mut rng, &mut out);
            tick(&mut p, ctx, &mut rng, &mut out);
            assert!(p.behavior.drawn());
            assert_eq!(p.pos, DVec3::ZERO);
            tick(&mut p, ctx, &mut rng, &mut out);
            assert!(p.pos.y > 0.0);
        });
    }

    #[test]
    fn a_falling_leaf_sinks_and_spins() {
        with_ctx(|ctx| {
            let mut rng = JavaRandom::new(4);
            let leaf = ParticleSpec::simple(Kind::PaleOakLeaves);
            let at = DVec3::new(0.0, 100.0, 0.0);
            let mut p = provider::create(&leaf, at, DVec3::ZERO, ctx, &mut rng).unwrap();
            let mut out = Vec::new();
            for _ in 0..20 {
                tick(&mut p, ctx, &mut rng, &mut out);
            }
            assert!(!p.removed);
            assert!(p.pos.y < at.y);
            assert_ne!(p.roll, 0.0);
            assert!(p.pos.x != 0.0 || p.pos.z != 0.0);
        });
    }
}
