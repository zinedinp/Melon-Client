//! `Block.animateTick` of the blocks that smoke and burn.

use glam::{DVec3, IVec3};

use super::super::{Kind, ParticleSpec, Spawn, World};
use super::{block_name, lit, prop};
use crate::world::block::direction::Direction;
use crate::world::java_random::JavaRandom;

/// The block's animation, if it has one here.
pub fn animate(world: &World, state: u16, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let base = pos.as_dvec3();
    let name = block_name(state);
    let smoke = |at: DVec3| Spawn::simple(Kind::Smoke, at, DVec3::ZERO);
    match name {
        "torch" | "soul_torch" | "copper_torch" => {
            let at = base + DVec3::new(0.5, 0.7, 0.5);
            out.push(smoke(at));
            out.push(Spawn::simple(flame_of(name), at, DVec3::ZERO));
        }
        "wall_torch" | "soul_wall_torch" | "copper_wall_torch" => {
            let [ox, _, oz] = facing(state).opposite().step().map(f64::from);
            let at = base + DVec3::new(0.5 + 0.27 * ox, 0.7 + 0.22, 0.5 + 0.27 * oz);
            out.push(smoke(at));
            out.push(Spawn::simple(flame_of(name), at, DVec3::ZERO));
        }
        "redstone_torch" if lit(state) => {
            let mut jitter = || (rng.next_double() - 0.5) * 0.2;
            let at = base + DVec3::new(0.5 + jitter(), 0.7 + jitter(), 0.5 + jitter());
            out.push(Spawn::new(ParticleSpec::REDSTONE, at, DVec3::ZERO));
        }
        "redstone_wall_torch" if lit(state) => {
            let [ox, _, oz] = facing(state).opposite().step().map(f64::from);
            let mut jitter = || (rng.next_double() - 0.5) * 0.2;
            let at = base
                + DVec3::new(
                    0.5 + jitter() + 0.27 * ox,
                    0.7 + jitter() + 0.22,
                    0.5 + jitter() + 0.27 * oz,
                );
            out.push(Spawn::new(ParticleSpec::REDSTONE, at, DVec3::ZERO));
        }
        "campfire" if lit(state) => {
            let _crackle = rng.next_int(10);
            if rng.next_int(5) == 0 {
                let aux = DVec3::new(
                    f64::from(rng.next_float() / 2.0),
                    5.0e-5,
                    f64::from(rng.next_float() / 2.0),
                );
                out.push(Spawn::simple(Kind::Lava, base + DVec3::splat(0.5), aux));
            }
        }
        "fire" => fire(world, pos, rng, out),
        "furnace" | "blast_furnace" if lit(state) => {
            let dir = facing(state);
            let ss = rng.next_double() * 0.6 - 0.3;
            let [sx, _, sz] = dir.step().map(f64::from);
            let dx = if dir.axis() == 0 { sx * 0.52 } else { ss };
            let high = if name == "furnace" { 6.0 } else { 9.0 };
            let dy = rng.next_double() * high / 16.0;
            let dz = if dir.axis() == 2 { sz * 0.52 } else { ss };
            let at = base + DVec3::new(0.5 + dx, dy, 0.5 + dz);
            out.push(smoke(at));
            if name == "furnace" {
                out.push(Spawn::simple(Kind::Flame, at, DVec3::ZERO));
            }
        }
        "smoker" if lit(state) => out.push(smoke(base + DVec3::new(0.5, 1.1, 0.5))),
        _ if (name.ends_with("candle") || name.ends_with("candle_cake")) && lit(state) => {
            for offset in candle_offsets(state, name) {
                candle_flame(base + offset, rng, out);
            }
        }
        _ => {}
    }
}

/// The flame a torch burns with.
fn flame_of(name: &str) -> Kind {
    if name.starts_with("soul") {
        Kind::SoulFireFlame
    } else if name.starts_with("copper") {
        Kind::CopperFireFlame
    } else {
        Kind::Flame
    }
}

/// A block's `facing`, north if it has none.
fn facing(state: u16) -> Direction {
    prop(state, "facing")
        .as_deref()
        .and_then(Direction::from_name)
        .unwrap_or(Direction::North)
}

/// `AbstractCandleBlock.addParticlesAndSound`.
fn candle_flame(at: DVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    if rng.next_float() < 0.3 {
        out.push(Spawn::simple(Kind::Smoke, at, DVec3::ZERO));
    }
    out.push(Spawn::simple(Kind::SmallFlame, at, DVec3::ZERO));
}

/// `CandleBlock.PARTICLE_OFFSETS` by `candles`; a cake's one candle.
fn candle_offsets(state: u16, name: &str) -> Vec<DVec3> {
    let px = |x: f64, y: f64, z: f64| DVec3::new(x, y, z) * 0.0625;
    if name.ends_with("candle_cake") {
        return vec![DVec3::new(0.5, 1.0, 0.5)];
    }
    let candles = prop(state, "candles")
        .and_then(|n| n.parse::<u8>().ok())
        .unwrap_or(1);
    match candles {
        1 => vec![px(8.0, 8.0, 8.0)],
        2 => vec![px(6.0, 7.0, 8.0), px(10.0, 8.0, 7.0)],
        3 => vec![px(8.0, 5.0, 10.0), px(6.0, 7.0, 8.0), px(9.0, 8.0, 7.0)],
        _ => vec![
            px(7.0, 5.0, 9.0),
            px(10.0, 7.0, 9.0),
            px(6.0, 7.0, 6.0),
            px(9.0, 8.0, 6.0),
        ],
    }
}

/// `FireBlock.animateTick`.
fn fire(world: &World, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let state_at = |p: IVec3| world.level.block_state(p.x, p.y, p.z).unwrap_or(0);
    let below = state_at(pos - IVec3::Y);
    let base = pos.as_dvec3();
    let smoke = |at: DVec3| Spawn::simple(Kind::LargeSmoke, at, DVec3::ZERO);
    if burns(below) || crate::world::block::state_flags::is_solid(below) {
        for _ in 0..3 {
            let at = base
                + DVec3::new(
                    rng.next_double(),
                    rng.next_double() * 0.5 + 0.5,
                    rng.next_double(),
                );
            out.push(smoke(at));
        }
        return;
    }
    let edge = |rng: &mut JavaRandom| rng.next_double() * f64::from(0.1f32);
    let sides: [(IVec3, SidePoint); 5] = [
        (IVec3::NEG_X, |r, e| {
            DVec3::new(e, r.next_double(), r.next_double())
        }),
        (IVec3::X, |r, e| {
            DVec3::new(1.0 - e, r.next_double(), r.next_double())
        }),
        (IVec3::NEG_Z, |r, e| {
            DVec3::new(r.next_double(), r.next_double(), e)
        }),
        (IVec3::Z, |r, e| {
            DVec3::new(r.next_double(), r.next_double(), 1.0 - e)
        }),
        (IVec3::Y, |r, e| {
            DVec3::new(r.next_double(), 1.0 - e, r.next_double())
        }),
    ];
    for (side, at) in sides {
        if burns(state_at(pos + side)) {
            for _ in 0..2 {
                let e = edge(rng);
                out.push(smoke(base + at(rng, e)));
            }
        }
    }
}

/// A point by a burning side: `(random, inset)` to a block-relative spot.
type SidePoint = fn(&mut JavaRandom, f64) -> DVec3;

/// `FireBlock.canBurn`.
fn burns(state: u16) -> bool {
    let Some(id) = pumpkin_data::BlockStateId::new(state) else {
        return false;
    };
    let ignites = pumpkin_data::Block::from_state_id(id)
        .flammable
        .as_ref()
        .is_some_and(|f| f.spread_chance > 0);
    ignites && prop(state, "waterlogged").as_deref() != Some("true")
}

/// `CampfireBlockEntity.particleTick`.
pub fn campfires(world: &World, rng: &mut JavaRandom, near: IVec3, out: &mut Vec<Spawn>) {
    for pos in world.level.block_entities() {
        let pos = IVec3::from_array(pos);
        if (pos - near).abs().max_element() > 64 {
            continue;
        }
        let state = world.level.block_state(pos.x, pos.y, pos.z).unwrap_or(0);
        let name = block_name(state);
        if !matches!(name, "campfire" | "soul_campfire") || !lit(state) {
            continue;
        }
        if rng.next_float() < 0.11 {
            let signal = prop(state, "signal_fire").as_deref() == Some("true");
            let mut i = 0;
            while i < rng.next_int(2) + 2 {
                campfire_smoke(pos.as_dvec3(), signal, rng, out);
                i += 1;
            }
        }
    }
}

/// `CampfireBlock.makeParticles` without the cooking smoke.
fn campfire_smoke(base: DVec3, signal: bool, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let kind = if signal {
        Kind::CampfireSignalSmoke
    } else {
        Kind::CampfireCosySmoke
    };
    let side = |rng: &mut JavaRandom| {
        let d = rng.next_double() / 3.0;
        if rng.next_boolean() { d } else { -d }
    };
    let x = 0.5 + side(rng);
    let y = rng.next_double() + rng.next_double();
    let z = 0.5 + side(rng);
    out.push(
        Spawn::simple(kind, base + DVec3::new(x, y, z), DVec3::new(0.0, 0.07, 0.0))
            .forced(true)
            .always_show(true),
    );
}
