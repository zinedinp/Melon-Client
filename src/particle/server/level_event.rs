//! The particles of `ClientboundLevelEventPacket` (`LevelEventHandler.levelEvent`).

use glam::DVec3;

use super::super::client::{faces, terrain};
use super::super::{Kind, ParticleEngine, ParticleSpec, Spawn, World};
use crate::world::block::direction::Direction;
use crate::world::java_random::JavaRandom;

/// `LevelEvent` ids that spawn particles.
pub mod id {
    pub const LAVA_FIZZ: i32 = 1501;
    pub const REDSTONE_TORCH_BURNOUT: i32 = 1502;
    pub const END_PORTAL_FRAME_FILL: i32 = 1503;
    pub const DRIPSTONE_DRIP: i32 = 1504;
    pub const BONE_MEAL_USE: i32 = 1505;
    pub const DISPENSER_SMOKE: i32 = 2000;
    pub const DESTROY_BLOCK: i32 = 2001;
    pub const SPLASH_POTION: i32 = 2002;
    pub const EYE_OF_ENDER_DEATH: i32 = 2003;
    pub const MOB_SPAWNER: i32 = 2004;
    pub const DRAGON_FIREBALL_SPLASH: i32 = 2006;
    pub const INSTANT_POTION: i32 = 2007;
    pub const EXPLOSION: i32 = 2008;
    pub const EVAPORATE: i32 = 2009;
    pub const WHITE_SMOKE: i32 = 2010;
    pub const BEE_GROWTH: i32 = 2011;
    pub const TURTLE_EGG_PLACEMENT: i32 = 2012;
    pub const SMASH_ATTACK: i32 = 2013;
    pub const DESTROY_BLOCK_SILENT: i32 = 2014;
    pub const DRAGON_EGG_TELEPORT: i32 = 2015;
    pub const SHULKER_TELEPORT: i32 = 2016;
    pub const CONSUME_TELEPORT: i32 = 2017;
    pub const ENDERMAN_TELEPORT: i32 = 2018;
    pub const BREAKING_BLOCK: i32 = 2019;
    pub const BREAKING_BLOCK_SOUND: i32 = 2020;
    pub const END_GATEWAY_SPAWN: i32 = 3000;
    pub const ELECTRIC_SPARK: i32 = 3002;
    pub const WAX_ON: i32 = 3003;
    pub const WAX_OFF: i32 = 3004;
    pub const SCRAPE: i32 = 3005;
    pub const SHRIEK: i32 = 3007;
    pub const BRUSH_COMPLETE: i32 = 3008;
    pub const SCULK_CHARGE: i32 = 3006;
    pub const EGG_CRACK: i32 = 3009;
    pub const TRIAL_SPAWNER_SPAWN: i32 = 3011;
    pub const TRIAL_SPAWNER_SPAWN_MOB_AT: i32 = 3012;
    pub const TRIAL_SPAWNER_DETECT_PLAYER: i32 = 3013;
    pub const TRIAL_SPAWNER_EJECT_ITEM: i32 = 3014;
    pub const VAULT_ACTIVATE: i32 = 3015;
    pub const VAULT_DEACTIVATE: i32 = 3016;
    pub const VAULT_EJECT_ITEM: i32 = 3017;
    pub const COBWEB_WEAVE: i32 = 3018;
    pub const TRIAL_SPAWNER_DETECT_PLAYER_OMINOUS: i32 = 3019;
    pub const TRIAL_SPAWNER_BECOME_OMINOUS: i32 = 3020;
    pub const TRIAL_SPAWNER_SPAWN_ITEM: i32 = 3021;
}

/// The particles of level event `event` at `pos`.
pub fn spawn(engine: &mut ParticleEngine, world: &World, event: i32, pos: [i32; 3], data: i32) {
    let base = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
    let mut out = Vec::new();
    {
        let rng = engine.rng();
        let simple = |kind, at, aux| Spawn::simple(kind, at, aux);
        match event {
            id::LAVA_FIZZ => {
                for _ in 0..8 {
                    let at = base + DVec3::new(rng.next_double(), 1.2, rng.next_double());
                    out.push(simple(Kind::LargeSmoke, at, DVec3::ZERO));
                }
            }
            id::REDSTONE_TORCH_BURNOUT => {
                for _ in 0..5 {
                    let mut r = || rng.next_double() * 0.6 + 0.2;
                    let at = base + DVec3::new(r(), r(), r());
                    out.push(simple(Kind::Smoke, at, DVec3::ZERO));
                }
            }
            id::END_PORTAL_FRAME_FILL => {
                for _ in 0..16 {
                    let mut r = || (5.0 + rng.next_double() * 6.0) / 16.0;
                    let at = base + DVec3::new(r(), 0.8125, r());
                    out.push(simple(Kind::Smoke, at, DVec3::ZERO));
                }
            }
            id::DRIPSTONE_DRIP => {
                let state = world.level.block_state(pos[0], pos[1], pos[2]).unwrap_or(0);
                let at = glam::IVec3::from_array(pos);
                if let Some(drip) = super::super::client::glimmer::dripstone_drip(world, state, at)
                {
                    out.push(drip);
                }
            }
            id::BONE_MEAL_USE => growth(world, pos, data, rng, &mut out),
            id::DISPENSER_SMOKE => shoot(base, data, Kind::Smoke, rng, &mut out),
            id::WHITE_SMOKE => shoot(base, data, Kind::WhiteSmoke, rng, &mut out),
            id::SPLASH_POTION | id::INSTANT_POTION => {
                let kind = if event == id::SPLASH_POTION {
                    Kind::Effect
                } else {
                    Kind::InstantEffect
                };
                potion_splash(base, data, kind, rng, &mut out);
            }
            id::EYE_OF_ENDER_DEATH => {
                let at = base + DVec3::new(0.5, 0.0, 0.5);
                let eye = ParticleSpec::item(pumpkin_data::item::Item::ENDER_EYE.id);
                for _ in 0..8 {
                    let aux = DVec3::new(
                        rng.next_gaussian() * 0.15,
                        rng.next_double() * 0.2,
                        rng.next_gaussian() * 0.15,
                    );
                    out.push(Spawn::new(eye, at, aux));
                }
                let mut angle = 0.0f64;
                while angle < crate::math::angle::double::TAU {
                    let ring = at + DVec3::new(angle.cos() * 5.0, -0.4, angle.sin() * 5.0);
                    for speed in [-5.0, -7.0] {
                        let aux = DVec3::new(angle.cos() * speed, 0.0, angle.sin() * speed);
                        out.push(simple(Kind::Portal, ring, aux));
                    }
                    angle += crate::math::angle::double::PI / 20.0;
                }
            }
            id::MOB_SPAWNER => {
                for _ in 0..20 {
                    let mut r = || (rng.next_double() - 0.5) * 2.0;
                    let at = base + DVec3::splat(0.5) + DVec3::new(r(), r(), r());
                    out.push(simple(Kind::Smoke, at, DVec3::ZERO));
                    out.push(simple(Kind::Flame, at, DVec3::ZERO));
                }
            }
            id::DRAGON_FIREBALL_SPLASH => {
                for _ in 0..200 {
                    let dist = rng.next_float() * 4.0;
                    let angle = rng.next_float() * crate::math::angle::TAU;
                    let aux = DVec3::new(
                        f64::from(angle.cos() * dist),
                        0.01 + rng.next_double() * 0.5,
                        f64::from(angle.sin() * dist),
                    );
                    let at = base + DVec3::new(aux.x * 0.1, 0.3, aux.z * 0.1);
                    let spec = ParticleSpec {
                        kind: Kind::DragonBreath,
                        data: super::super::ParticleData::Power(dist),
                    };
                    out.push(Spawn::new(spec, at, aux));
                }
            }
            id::EXPLOSION => {
                out.push(simple(
                    Kind::Explosion,
                    base + DVec3::splat(0.5),
                    DVec3::ZERO,
                ));
            }
            id::EVAPORATE => {
                for _ in 0..8 {
                    let at = base + DVec3::new(rng.next_double(), 1.2, rng.next_double());
                    out.push(simple(Kind::Cloud, at, DVec3::ZERO));
                }
            }
            id::BEE_GROWTH | id::TURTLE_EGG_PLACEMENT => {
                in_block(world, pos, data, Kind::HappyVillager, rng, &mut out);
            }
            id::SMASH_ATTACK => smash_attack(world, base, pos, data, rng, &mut out),
            id::DRAGON_EGG_TELEPORT | id::SHULKER_TELEPORT | id::CONSUME_TELEPORT => {
                let radius = match event {
                    id::DRAGON_EGG_TELEPORT => [16, 8, 16],
                    id::SHULKER_TELEPORT => [8, 8, 8],
                    _ => [127, 127, 127],
                };
                let to = unpack_difference(pos, data, radius);
                teleport_trail(base, to, [1.0, 0.0], rng, &mut out);
            }
            id::ENDERMAN_TELEPORT => {
                let to = unpack_difference(pos, data, [127, 127, 127]);
                teleport_trail(base, to, [0.6, 2.9], rng, &mut out);
            }
            id::END_GATEWAY_SPAWN => {
                let at = base + DVec3::splat(0.5);
                out.push(simple(Kind::ExplosionEmitter, at, DVec3::ZERO).always_show(true));
            }
            id::ELECTRIC_SPARK => {
                if (0..3).contains(&data) {
                    let count = 10 + rng.next_int(10);
                    let kind = Kind::ElectricSpark;
                    faces::along_axis(base, data as usize, 0.125, kind, count, rng, &mut out);
                } else {
                    faces::on_faces(base, Kind::ElectricSpark, (3, 5), 0.55, rng, &mut out);
                }
            }
            id::SCULK_CHARGE => sculk_charge(world, base, pos, data, rng, &mut out),
            id::WAX_ON => faces::on_faces(base, Kind::WaxOn, (3, 5), 0.55, rng, &mut out),
            id::WAX_OFF => faces::on_faces(base, Kind::WaxOff, (3, 5), 0.55, rng, &mut out),
            id::SCRAPE => faces::on_faces(base, Kind::Scrape, (3, 5), 0.55, rng, &mut out),
            id::EGG_CRACK => faces::on_faces(base, Kind::EggCrack, (3, 6), 0.55, rng, &mut out),
            id::SHRIEK => {
                for i in 0..10 {
                    let spec = ParticleSpec {
                        kind: Kind::Shriek,
                        data: super::super::ParticleData::Shriek(i * 5),
                    };
                    out.push(Spawn::new(
                        spec,
                        base + DVec3::new(0.5, 0.5, 0.5),
                        DVec3::ZERO,
                    ));
                }
            }
            id::COBWEB_WEAVE => {
                for _ in 0..10 {
                    let aux = DVec3::new(
                        rng.next_gaussian() * 0.02,
                        rng.next_gaussian() * 0.02,
                        rng.next_gaussian() * 0.02,
                    );
                    let mut r = || rng.next_double();
                    let at = base + DVec3::new(r(), r(), r());
                    out.push(simple(Kind::Poof, at, aux));
                }
            }
            id::TRIAL_SPAWNER_SPAWN
            | id::TRIAL_SPAWNER_SPAWN_MOB_AT
            | id::TRIAL_SPAWNER_SPAWN_ITEM => {
                let flame = if data == 1 {
                    Kind::SoulFireFlame
                } else {
                    Kind::Flame
                };
                for _ in 0..20 {
                    let mut r = || (rng.next_double() - 0.5) * 2.0;
                    let at = base + DVec3::splat(0.5) + DVec3::new(r(), r(), r());
                    out.push(simple(Kind::Smoke, at, DVec3::ZERO));
                    out.push(simple(flame, at, DVec3::ZERO));
                }
            }
            id::TRIAL_SPAWNER_DETECT_PLAYER | id::TRIAL_SPAWNER_DETECT_PLAYER_OMINOUS => {
                let kind = if event == id::TRIAL_SPAWNER_DETECT_PLAYER {
                    Kind::TrialSpawnerDetection
                } else {
                    Kind::TrialSpawnerDetectionOminous
                };
                detect_player(base, data, kind, rng, &mut out);
            }
            id::TRIAL_SPAWNER_BECOME_OMINOUS => {
                detect_player(base, 0, Kind::TrialSpawnerDetectionOminous, rng, &mut out);
                for _ in 0..20 {
                    let mut r = || (rng.next_double() - 0.5) * 2.0;
                    let at = base + DVec3::splat(0.5) + DVec3::new(r(), r(), r());
                    let mut g = || rng.next_gaussian() * 0.02;
                    let aux = DVec3::new(g(), g(), g());
                    out.push(simple(Kind::TrialOmen, at, aux));
                    out.push(simple(Kind::SoulFireFlame, at, aux));
                }
            }
            id::TRIAL_SPAWNER_EJECT_ITEM | id::VAULT_EJECT_ITEM => {
                for _ in 0..20 {
                    let mut r = || 0.4 + rng.next_double() * 0.2;
                    let at = base + DVec3::new(r(), r(), r());
                    let mut g = || rng.next_gaussian() * 0.02;
                    let aux = DVec3::new(g(), g(), g());
                    let flame = DVec3::new(aux.x, aux.y, aux.z * 0.25);
                    out.push(simple(Kind::SmallFlame, at, flame));
                    out.push(simple(Kind::Smoke, at, aux));
                }
            }
            id::VAULT_ACTIVATE | id::VAULT_DEACTIVATE => {
                let flame = if data == 0 {
                    Kind::SmallFlame
                } else {
                    Kind::SoulFireFlame
                };
                for _ in 0..20 {
                    if event == id::VAULT_ACTIVATE {
                        let at = base
                            + DVec3::new(
                                faces::between(rng, 0.1, 0.9),
                                faces::between(rng, 0.25, 0.75),
                                faces::between(rng, 0.1, 0.9),
                            );
                        out.push(simple(Kind::Smoke, at, DVec3::ZERO));
                        out.push(simple(flame, at, DVec3::ZERO));
                    } else {
                        let mut r = || faces::between(rng, 0.4, 0.6);
                        let at = base + DVec3::new(r(), r(), r());
                        let mut g = || rng.next_gaussian() * 0.02;
                        let aux = DVec3::new(g(), g(), g());
                        out.push(simple(flame, at, aux));
                    }
                }
            }
            _ => {}
        }
    }
    engine.request_all(out);
    match event {
        id::DESTROY_BLOCK | id::DESTROY_BLOCK_SILENT => {
            let state = u16::try_from(data).unwrap_or(0);
            terrain::destroy(engine, world, pos, state);
        }
        id::BRUSH_COMPLETE => {
            let state = world.level.block_state(pos[0], pos[1], pos[2]).unwrap_or(0);
            terrain::destroy(engine, world, pos, state);
        }
        id::BREAKING_BLOCK | id::BREAKING_BLOCK_SOUND => {
            let face = Direction::ALL[data.clamp(0, 5) as usize];
            terrain::crack(engine, world, pos, face);
        }
        _ => {}
    }
}

/// `shootParticles`: ten out of the face `data` names (a dispenser's).
fn shoot(base: DVec3, data: i32, kind: Kind, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let dir = Direction::ALL[(data % 6).unsigned_abs() as usize];
    let [nx, ny, nz] = dir.step().map(f64::from);
    for _ in 0..10 {
        let pow = rng.next_double() * 0.2 + 0.01;
        let at = base
            + DVec3::new(
                nx * 0.6 + 0.5 + nx * 0.01 + (rng.next_double() - 0.5) * nz * 0.5,
                ny * 0.6 + 0.5 + ny * 0.01 + (rng.next_double() - 0.5) * ny * 0.5,
                nz * 0.6 + 0.5 + nz * 0.01 + (rng.next_double() - 0.5) * nx * 0.5,
            );
        let aux = DVec3::new(
            nx * pow + rng.next_gaussian() * 0.01,
            ny * pow + rng.next_gaussian() * 0.01,
            nz * pow + rng.next_gaussian() * 0.01,
        );
        out.push(Spawn::simple(kind, at, aux));
    }
}

/// `potionSplashParticles`: the bottle's shards and a ring of its colour.
fn potion_splash(base: DVec3, rgb: i32, kind: Kind, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let at = base + DVec3::new(0.5, 0.0, 0.5);
    let shards = ParticleSpec::item(pumpkin_data::item::Item::SPLASH_POTION.id);
    for _ in 0..8 {
        let aux = DVec3::new(
            rng.next_gaussian() * 0.15,
            rng.next_double() * 0.2,
            rng.next_gaussian() * 0.15,
        );
        out.push(Spawn::new(shards, at, aux));
    }
    let [r, g, b] = super::super::provider::rgb_of(rgb as u32);
    for _ in 0..100 {
        let dist = rng.next_double() * 4.0;
        let angle = rng.next_double() * crate::math::angle::double::TAU;
        let aux = DVec3::new(
            angle.cos() * dist,
            0.01 + rng.next_double() * 0.5,
            angle.sin() * dist,
        );
        let shade = 0.75 + rng.next_float() * 0.25;
        let color = |c: f32| ((c * shade).clamp(0.0, 1.0) * 255.0) as u32;
        let spec = ParticleSpec {
            kind,
            data: super::super::ParticleData::Spell {
                rgb: color(r) << 16 | color(g) << 8 | color(b),
                power: dist as f32,
            },
        };
        let pos = at + DVec3::new(aux.x * 0.1, 0.3, aux.z * 0.1);
        out.push(Spawn::new(spec, pos, aux));
    }
}

/// `TrialSpawner.addDetectPlayerParticles`: more for more players.
fn detect_player(
    base: DVec3,
    players: i32,
    kind: Kind,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    // Java's `int` arithmetic: a huge negative count wraps instead of panicking.
    for _ in 0..players.min(10).wrapping_mul(5).wrapping_add(30) {
        let x = f64::from(2.0 * rng.next_float() - 1.0) * 0.65;
        let z = f64::from(2.0 * rng.next_float() - 1.0) * 0.65;
        let y = 0.1 + f64::from(rng.next_float() * 0.8);
        out.push(Spawn::simple(
            kind,
            base + DVec3::new(0.5 + x, y, 0.5 + z),
            DVec3::ZERO,
        ));
    }
}

/// `BoneMealItem.addGrowthParticles`.
fn growth(world: &World, pos: [i32; 3], count: i32, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let state = world.level.block_state(pos[0], pos[1], pos[2]).unwrap_or(0);
    let name = super::super::client::block_name(state);
    let spreads = matches!(
        name,
        "grass_block"
            | "netherrack"
            | "crimson_nylium"
            | "warped_nylium"
            | "moss_block"
            | "pale_moss_block"
    );
    if spreads || name == "water" {
        let at = if spreads {
            [pos[0], pos[1] + 1, pos[2]]
        } else {
            pos
        };
        spread(world, at, count.wrapping_mul(3), 3.0, 1.0, rng, out);
    } else {
        let at = if matches!(name, "mangrove_leaves" | "rooted_dirt") {
            [pos[0], pos[1] - 1, pos[2]]
        } else {
            pos
        };
        in_block(world, at, count, Kind::HappyVillager, rng, out);
    }
}

/// `ParticleUtils.spawnParticles` without floating ones: over ground only.
fn spread(
    world: &World,
    pos: [i32; 3],
    count: i32,
    width: f64,
    height: f64,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    for _ in 0..count.clamp(0, 1024) {
        let mut g = || rng.next_gaussian() * 0.02;
        let aux = DVec3::new(g(), g(), g());
        let start = 0.5 - width;
        let at = DVec3::new(
            f64::from(pos[0]) + start + rng.next_double() * width * 2.0,
            f64::from(pos[1]) + rng.next_double() * height,
            f64::from(pos[2]) + start + rng.next_double() * width * 2.0,
        );
        let below = at.floor().as_ivec3() - glam::IVec3::Y;
        let state = world
            .level
            .block_state(below.x, below.y, below.z)
            .unwrap_or(0);
        if !super::super::client::block_name(state).ends_with("air") {
            out.push(Spawn::simple(Kind::HappyVillager, at, aux));
        }
    }
}

/// Level event 3006.
fn sculk_charge(
    world: &World,
    base: DVec3,
    pos: [i32; 3],
    data: i32,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    use crate::math::angle::PI;
    let count = data >> 6;
    if count > 0 {
        if rng.next_float() < 0.3 + count as f32 * 0.1 {
            rng.next_float();
            rng.next_float();
        }
        let faces_packed = data & 63;
        let speed = |rng: &mut JavaRandom| {
            let mut v = || faces::between(rng, -0.005, 0.005);
            DVec3::new(v(), v(), v())
        };
        for dir in Direction::ALL {
            let (roll, step) = if faces_packed == 0 {
                let roll = if dir == Direction::Down { PI } else { 0.0 };
                (roll, if dir.axis() == 1 { 0.65 } else { 0.57 })
            } else if faces_packed & (1 << dir.index()) != 0 {
                (if dir == Direction::Up { PI } else { 0.0 }, 0.35)
            } else {
                continue;
            };
            let spec = ParticleSpec {
                kind: Kind::SculkCharge,
                data: super::super::ParticleData::SculkCharge(roll),
            };
            for _ in 0..rng.next_int(count + 1) {
                let v = speed(rng);
                faces::on_face_of(base, dir, spec, v, step, rng, out);
            }
        }
    } else {
        let solid = super::super::client::full_block(world, glam::IVec3::from_array(pos));
        let (amount, spread) = if solid { (40, 0.45) } else { (20, 0.25) };
        for _ in 0..amount {
            let mut v = || f64::from(2.0 * rng.next_float() - 1.0);
            let vel = DVec3::new(v(), v(), v());
            let at = base + DVec3::splat(0.5) + vel * spread;
            out.push(Spawn::simple(
                Kind::SculkChargePop,
                at,
                vel * f64::from(0.07f32),
            ));
        }
    }
}

/// `ParticleUtils.spawnParticleInBlock`: `count` in the block's height.
pub fn in_block(
    world: &World,
    pos: [i32; 3],
    count: i32,
    kind: Kind,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let state = world.level.block_state(pos[0], pos[1], pos[2]).unwrap_or(0);
    let height = if state == 0 {
        1.0
    } else {
        crate::world::pick::outline_shapes(world.level, (pos[0], pos[1], pos[2]))
            .map(|b| b.max.y - f64::from(pos[1]))
            .fold(0.0, f64::max)
    };
    let base = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
    for _ in 0..count.clamp(0, 1024) {
        let aux = DVec3::new(
            rng.next_gaussian() * 0.02,
            rng.next_gaussian() * 0.02,
            rng.next_gaussian() * 0.02,
        );
        let at = base
            + DVec3::new(
                rng.next_double(),
                rng.next_double() * height,
                rng.next_double(),
            );
        out.push(Spawn::simple(kind, at, aux));
    }
}

/// `ParticleUtils.spawnSmashAttackParticles`.
fn smash_attack(
    world: &World,
    base: DVec3,
    pos: [i32; 3],
    count: i32,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let state = world.level.block_state(pos[0], pos[1], pos[2]).unwrap_or(0);
    let spec = ParticleSpec::block(Kind::DustPillar, state);
    let center = base + DVec3::new(0.5, 1.0, 0.5);
    let mut i = 0;
    while (i as f32) < count as f32 / 3.0 {
        let at = center + DVec3::new(rng.next_gaussian() / 2.0, 0.0, rng.next_gaussian() / 2.0);
        let mut g = || rng.next_gaussian() * f64::from(0.2f32);
        out.push(Spawn::new(spec, at, DVec3::new(g(), g(), g())));
        i += 1;
    }
    let mut i = 0;
    while (i as f32) < count as f32 / 1.5 {
        let angle = f64::from(i);
        let at = center
            + DVec3::new(
                3.5 * angle.cos() + rng.next_gaussian() / 2.0,
                0.0,
                3.5 * angle.sin() + rng.next_gaussian() / 2.0,
            );
        let mut g = || rng.next_gaussian() * f64::from(0.05f32);
        out.push(Spawn::new(spec, at, DVec3::new(g(), g(), g())));
        i += 1;
    }
}

/// `BlockUtil.unpackDifferenceInPosition`.
pub fn unpack_difference(pos: [i32; 3], packed: i32, radius: [i32; 3]) -> [i32; 3] {
    [
        pos[0] + (packed >> 16 & 0xFF) - radius[0],
        pos[1] + (packed >> 8 & 0xFF) - radius[1],
        pos[2] + (packed & 0xFF) - radius[2],
    ]
}

/// 128 portal particles strewn from `to` back to `base`.
fn teleport_trail(
    base: DVec3,
    to: [i32; 3],
    [width, height]: [f64; 2],
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let to = DVec3::new(f64::from(to[0]), f64::from(to[1]), f64::from(to[2]));
    let block = height == 0.0;
    for _ in 0..128 {
        let t = rng.next_double();
        let mut v = || f64::from((rng.next_float() - 0.5) * 0.2);
        let aux = DVec3::new(v(), v(), v());
        let along = to.lerp(base, t);
        let at = if block {
            along
                + DVec3::new(
                    rng.next_double() - 0.5 + 0.5,
                    rng.next_double() - 0.5,
                    rng.next_double() - 0.5 + 0.5,
                )
        } else {
            along
                + DVec3::new(
                    (rng.next_double() - 0.5) * width * 2.0,
                    rng.next_double() * height,
                    (rng.next_double() - 0.5) * width * 2.0,
                )
        };
        out.push(Spawn::simple(Kind::Portal, at, aux));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_difference_unpacks_around_the_radius() {
        let packed = (16 + 3) << 16 | (8 - 2) << 8 | 16;
        assert_eq!(
            unpack_difference([10, 64, 10], packed, [16, 8, 16]),
            [13, 62, 10]
        );
    }
}
