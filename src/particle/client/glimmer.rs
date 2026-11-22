//! `Block.animateTick` of the blocks that glimmer, drip and shed.

use glam::{DVec3, IVec3};

use super::super::options::{ParticleData, ParticleSpec};
use super::super::{Kind, Spawn, World};
use super::{block_name, lit, prop};
use crate::world::block::direction::Direction;
use crate::world::block::state_flags;
use crate::world::block_tint::TintKind;
use crate::world::java_random::JavaRandom;

/// The block's animation, if it has one here.
pub fn animate(world: &World, state: u16, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let base = pos.as_dvec3();
    let name = block_name(state);
    match name {
        "end_rod" => end_rod(state, base, rng, out),
        "nether_portal" => nether_portal(world, base, pos, rng, out),
        "ender_chest" => {
            for _ in 0..3 {
                let flip_x = f64::from(rng.next_int(2) * 2 - 1);
                let flip_z = f64::from(rng.next_int(2) * 2 - 1);
                let at = base
                    + DVec3::new(
                        0.5 + 0.25 * flip_x,
                        f64::from(rng.next_float()),
                        0.5 + 0.25 * flip_z,
                    );
                let aux = DVec3::new(
                    f64::from(rng.next_float()) * flip_x,
                    f64::from((rng.next_float() - 0.5) * 0.125),
                    f64::from(rng.next_float()) * flip_z,
                );
                out.push(Spawn::simple(Kind::Portal, at, aux));
            }
        }
        "end_portal" => {
            let at = base + DVec3::new(rng.next_double(), 0.8, rng.next_double());
            out.push(Spawn::simple(Kind::Smoke, at, DVec3::ZERO));
        }
        "crying_obsidian" => {
            if rng.next_int(5) == 0 {
                let dir = random_direction(rng);
                if dir != Direction::Up && !sturdy_toward(world, pos, dir) {
                    let [sx, sy, sz] = dir.step();
                    let mut off = |step: i32| {
                        if step == 0 {
                            rng.next_double()
                        } else {
                            0.5 + f64::from(step) * 0.6
                        }
                    };
                    let at = base + DVec3::new(off(sx), off(sy), off(sz));
                    out.push(Spawn::simple(Kind::DrippingObsidianTear, at, DVec3::ZERO));
                }
            }
        }
        "wet_sponge" => wet_sponge(world, pos, rng, out),
        "spore_blossom" => spore_blossom(world, pos, rng, out),
        "mycelium" => {
            if rng.next_int(10) == 0 {
                let at = base + DVec3::new(rng.next_double(), 1.1, rng.next_double());
                out.push(Spawn::simple(Kind::Mycelium, at, DVec3::ZERO));
            }
        }
        "wither_rose" => {
            let (x, z) = (base.x + 0.5, base.z + 0.5);
            for _ in 0..3 {
                if rng.next_boolean() {
                    let at = DVec3::new(
                        x + rng.next_double() / 5.0,
                        base.y + (0.5 - rng.next_double()),
                        z + rng.next_double() / 5.0,
                    );
                    out.push(Spawn::simple(Kind::Smoke, at, DVec3::ZERO));
                }
            }
        }
        "redstone_ore" | "deepslate_redstone_ore" if lit(state) => {
            redstone_ore(world, pos, rng, out);
        }
        "respawn_anchor" if prop(state, "charges").is_some_and(|c| c != "0") => {
            let _sound = rng.next_int(100);
            let at = base
                + DVec3::new(
                    0.5 + (0.5 - rng.next_double()),
                    1.0,
                    0.5 + (0.5 - rng.next_double()),
                );
            let aux = DVec3::new(0.0, f64::from(rng.next_float() * 0.04), 0.0);
            out.push(Spawn::simple(Kind::ReversePortal, at, aux));
        }
        "enchanting_table" => enchanting_table(world, pos, rng, out),
        "brewing_stand" => {
            let at = base
                + DVec3::new(
                    0.4 + f64::from(rng.next_float() * 0.2),
                    0.7 + f64::from(rng.next_float() * 0.3),
                    0.4 + f64::from(rng.next_float() * 0.2),
                );
            out.push(Spawn::simple(Kind::Smoke, at, DVec3::ZERO));
        }
        "repeater" if prop(state, "powered").as_deref() == Some("true") => {
            repeater(state, base, rng, out);
        }
        "redstone_wire" => redstone_wire(state, base, rng, out),
        "pointed_dripstone" => dripstone(world, state, pos, rng, out),
        "beehive" | "bee_nest" if prop(state, "honey_level").is_some_and(|l| l == "5") => {
            for _ in 0..rng.next_int(1) + 1 {
                honey_drip(world, pos, rng, out);
            }
        }
        "bubble_column" => {
            if prop(state, "drag").as_deref() == Some("true") {
                let at = base + DVec3::new(0.5, 0.8, 0.0);
                out.push(Spawn::simple(Kind::CurrentDown, at, DVec3::ZERO).always_show(true));
                sound_roll(rng, 200, 2);
            } else {
                let up = DVec3::new(0.0, 0.04, 0.0);
                let middle = base + DVec3::new(0.5, 0.0, 0.5);
                out.push(Spawn::simple(Kind::BubbleColumnUp, middle, up).always_show(true));
                let mut r = || f64::from(rng.next_float());
                let at = base + DVec3::new(r(), r(), r());
                out.push(Spawn::simple(Kind::BubbleColumnUp, at, up).always_show(true));
                sound_roll(rng, 200, 2);
            }
        }
        "end_gateway" => end_gateway(world, base, pos, rng, out),
        "firefly_bush" => {
            let _sound = rng.next_int(30);
            if world.raw_brightness(pos) <= 13 && rng.next_double() <= 0.7 {
                let at = base
                    + DVec3::new(
                        rng.next_double() * 10.0 - 5.0,
                        rng.next_double() * 5.0,
                        rng.next_double() * 10.0 - 5.0,
                    );
                out.push(Spawn::simple(Kind::Firefly, at, DVec3::ZERO));
            }
        }
        "dried_ghast" => {
            let middle = base + DVec3::splat(0.5);
            let _sound = rng.next_int(40);
            if prop(state, "waterlogged").as_deref() != Some("true") {
                if rng.next_int(6) == 0 {
                    let aux = DVec3::new(0.0, 0.02, 0.0);
                    out.push(Spawn::simple(Kind::WhiteSmoke, middle, aux));
                }
            } else if rng.next_int(6) == 0 {
                let x = f64::from((rng.next_float() * 2.0 - 1.0) / 3.0);
                let z = f64::from((rng.next_float() * 2.0 - 1.0) / 3.0);
                let aux = DVec3::new(0.0, f64::from(rng.next_float()), 0.0);
                let at = middle + DVec3::new(x, 0.4, z);
                out.push(Spawn::simple(Kind::HappyVillager, at, aux));
            }
        }
        "lightning_rod" => {
            if world.thundering
                && i64::from(rng.next_int(200)) <= world.game_time.rem_euclid(200)
                && pos.y == world.level.motion_blocking_height(pos.x, pos.z) - 1
            {
                let axis = prop(state, "facing")
                    .as_deref()
                    .and_then(Direction::from_name)
                    .map_or(1, Direction::axis);
                let count = 1 + rng.next_int(2);
                let kind = Kind::ElectricSpark;
                super::faces::along_axis(base, axis, 0.125, kind, count, rng, out);
            }
        }
        _ if name.ends_with("_leaves") => leaves(world, name, pos, rng, out),
        _ if falls(name) => falling_dust(world, state, pos, rng, out),
        _ => {}
    }
}

/// `Direction.getRandom`: one of the six in vanilla's order.
fn random_direction(rng: &mut JavaRandom) -> Direction {
    Direction::ALL[rng.next_int(6) as usize]
}

/// Whether the neighbour toward `dir` closes that side (`canOcclude` and a full face).
fn sturdy_toward(world: &World, pos: IVec3, dir: Direction) -> bool {
    let [dx, dy, dz] = dir.step();
    let at = pos + IVec3::new(dx, dy, dz);
    let next = world.level.block_state(at.x, at.y, at.z).unwrap_or(0);
    state_flags::is_solid(next)
}

/// `EndRodBlock.animateTick`.
fn end_rod(state: u16, base: DVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let dir = prop(state, "facing")
        .as_deref()
        .and_then(Direction::from_name)
        .unwrap_or(Direction::Up);
    let mut near = || 0.55 - f64::from(rng.next_float() * 0.1);
    let at = base + DVec3::new(near(), near(), near());
    let r = f64::from(0.4 - (rng.next_float() + rng.next_float()) * 0.4);
    if rng.next_int(5) == 0 {
        let [sx, sy, sz] = dir.step().map(f64::from);
        let mut drift = || rng.next_gaussian() * 0.005;
        let aux = DVec3::new(drift(), drift(), drift());
        out.push(Spawn::simple(
            Kind::EndRod,
            at + DVec3::new(sx, sy, sz) * r,
            aux,
        ));
    }
}

/// `NetherPortalBlock.animateTick`.
fn nether_portal(
    world: &World,
    base: DVec3,
    pos: IVec3,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    if rng.next_int(100) == 0 {
        let _pitch = rng.next_float();
    }
    let portal_at =
        |x: i32| world.level.block_state(x, pos.y, pos.z).map(block_name) == Some("nether_portal");
    let along_x = portal_at(pos.x - 1) || portal_at(pos.x + 1);
    for _ in 0..4 {
        let mut at = base + DVec3::new(rng.next_double(), rng.next_double(), rng.next_double());
        let mut aux = DVec3::new(
            (f64::from(rng.next_float()) - 0.5) * 0.5,
            (f64::from(rng.next_float()) - 0.5) * 0.5,
            (f64::from(rng.next_float()) - 0.5) * 0.5,
        );
        let flip = f64::from(rng.next_int(2) * 2 - 1);
        if along_x {
            at.z = base.z + 0.5 + 0.25 * flip;
            aux.z = f64::from(rng.next_float() * 2.0) * flip;
        } else {
            at.x = base.x + 0.5 + 0.25 * flip;
            aux.x = f64::from(rng.next_float() * 2.0) * flip;
        }
        out.push(Spawn::simple(Kind::Portal, at, aux));
    }
}

/// `WetSpongeBlock.animateTick`: a drop from a side that's open.
fn wet_sponge(world: &World, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let dir = random_direction(rng);
    if dir == Direction::Up || sturdy_toward(world, pos, dir) {
        return;
    }
    let mut at = pos.as_dvec3();
    if dir == Direction::Down {
        at.y -= 0.05;
        at.x += rng.next_double();
        at.z += rng.next_double();
    } else {
        at.y += rng.next_double() * 0.8;
        if dir.axis() == 0 {
            at.z += rng.next_double();
            at.x += if dir == Direction::East { 1.1 } else { 0.05 };
        } else {
            at.x += rng.next_double();
            at.z += if dir == Direction::South { 1.1 } else { 0.05 };
        }
    }
    out.push(Spawn::simple(Kind::DrippingWater, at, DVec3::ZERO));
}

/// `SporeBlossomBlock.animateTick`.
fn spore_blossom(world: &World, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let falling = pos.as_dvec3() + DVec3::new(rng.next_double(), 0.7, rng.next_double());
    out.push(Spawn::simple(
        Kind::FallingSporeBlossom,
        falling,
        DVec3::ZERO,
    ));
    for _ in 0..14 {
        let x = pos.x + rng.next_int(21) - 10;
        let y = pos.y - rng.next_int(10);
        let z = pos.z + rng.next_int(21) - 10;
        let at = IVec3::new(x, y, z);
        if !super::full_block(world, at) {
            let spot =
                at.as_dvec3() + DVec3::new(rng.next_double(), rng.next_double(), rng.next_double());
            out.push(Spawn::simple(Kind::SporeBlossomAir, spot, DVec3::ZERO));
        }
    }
}

/// `RedStoneOreBlock.spawnParticles`: dust on every side that shows.
fn redstone_ore(world: &World, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    const OFFSET: f64 = 0.5625;
    for dir in Direction::ALL {
        if sturdy_toward(world, pos, dir) {
            continue;
        }
        let [sx, sy, sz] = dir.step();
        let axis = dir.axis();
        let mut off = |on_axis: bool, step: i32| {
            if on_axis {
                0.5 + OFFSET * f64::from(step)
            } else {
                f64::from(rng.next_float())
            }
        };
        let dx = off(axis == 0, sx);
        let dy = off(axis == 1, sy);
        let dz = off(axis == 2, sz);
        let at = pos.as_dvec3() + DVec3::new(dx, dy, dz);
        out.push(Spawn::new(ParticleSpec::REDSTONE, at, DVec3::ZERO));
    }
}

/// `EnchantingTableBlock.animateTick`.
fn enchanting_table(world: &World, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let block_at = |at: IVec3| block_name(world.level.block_state(at.x, at.y, at.z).unwrap_or(0));
    for z in -2..=2 {
        for y in 0..=1 {
            for x in -2..=2 {
                if x != -2 && x != 2 && z != -2 && z != 2 {
                    continue;
                }
                if rng.next_int(16) != 0 {
                    continue;
                }
                let offset = IVec3::new(x, y, z);
                let shelf = block_at(pos + offset) == "bookshelf";
                let between = block_at(pos + IVec3::new(x / 2, y, z / 2)).ends_with("air");
                if shelf && between {
                    let aux = DVec3::new(
                        f64::from(x as f32 + rng.next_float() - 0.5),
                        f64::from(y as f32 - rng.next_float() - 1.0),
                        f64::from(z as f32 + rng.next_float() - 0.5),
                    );
                    let at = pos.as_dvec3() + DVec3::new(0.5, 2.0, 0.5);
                    out.push(Spawn::simple(Kind::Enchant, at, aux));
                }
            }
        }
    }
}

/// `RepeaterBlock.animateTick`: dust over one of its two torches.
fn repeater(state: u16, base: DVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let dir = prop(state, "facing")
        .as_deref()
        .and_then(Direction::from_name)
        .unwrap_or(Direction::North);
    let mut jitter = || (rng.next_double() - 0.5) * 0.2;
    let at = base + DVec3::new(0.5 + jitter(), 0.4 + jitter(), 0.5 + jitter());
    let mut offset = -5.0f32;
    if rng.next_boolean() {
        let delay: f32 = prop(state, "delay")
            .and_then(|d| d.parse().ok())
            .unwrap_or(1.0);
        offset = delay * 2.0 - 1.0;
    }
    offset /= 16.0;
    let [sx, _, sz] = dir.step().map(|s| f64::from(offset) * f64::from(s));
    out.push(Spawn::new(
        ParticleSpec::REDSTONE,
        at + DVec3::new(sx, 0.0, sz),
        DVec3::ZERO,
    ));
}

/// The rolls of an ambient sound Melon's block sounds don't play.
fn sound_roll(rng: &mut JavaRandom, one_in: i32, extra: u32) {
    if rng.next_int(one_in) == 0 {
        for _ in 0..extra {
            rng.next_float();
        }
    }
}

/// `LeavesBlock.animateTick`: rain drips through, then a falling leaf.
fn leaves(world: &World, name: &str, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    if (world.raining_at)(pos + IVec3::Y) && rng.next_int(15) == 1 {
        let below = pos - IVec3::Y;
        let below_state = world
            .level
            .block_state(below.x, below.y, below.z)
            .unwrap_or(0);
        if !state_flags::is_solid(below_state) {
            out.push(Spawn::simple(
                Kind::DrippingWater,
                spawn_below(pos, rng),
                DVec3::ZERO,
            ));
        }
    }
    falling_leaves(world, name, pos, rng, out);
}

/// `ParticleUtils.spawnParticleBelow`: somewhere just under the block.
fn spawn_below(pos: IVec3, rng: &mut JavaRandom) -> DVec3 {
    DVec3::new(
        f64::from(pos.x) + rng.next_double(),
        f64::from(pos.y) - 0.05,
        f64::from(pos.z) + rng.next_double(),
    )
}

/// `FallingParticlesLeavesBlock.animateTick`.
fn falling_leaves(
    world: &World,
    name: &str,
    pos: IVec3,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    enum Leaf {
        Own(Kind),
        Tinted(u32),
        Foliage,
    }
    let (chance, leaf) = match name {
        "cherry_leaves" => (0.1, Leaf::Own(Kind::CherryLeaves)),
        "pale_oak_leaves" => (0.02, Leaf::Own(Kind::PaleOakLeaves)),
        "red_poplar_leaves" => (0.01, Leaf::Own(Kind::RedPoplarLeaves)),
        "orange_poplar_leaves" => (0.01, Leaf::Own(Kind::OrangePoplarLeaves)),
        "yellow_poplar_leaves" => (0.01, Leaf::Own(Kind::YellowPoplarLeaves)),
        "azalea_leaves" | "flowering_azalea_leaves" => (0.01, Leaf::Tinted(0xFF70_922D)),
        "birch_leaves" => (0.01, Leaf::Tinted(0xFF80_A755)),
        "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves"
        | "mangrove_leaves" => (0.01, Leaf::Foliage),
        _ => return,
    };
    if rng.next_float() >= chance || super::full_block(world, pos - IVec3::Y) {
        return;
    }
    let tinted = |argb: u32| ParticleSpec {
        kind: Kind::TintedLeaves,
        data: ParticleData::Color(argb),
    };
    let spec = match leaf {
        Leaf::Own(kind) => ParticleSpec::simple(kind),
        Leaf::Tinted(argb) => tinted(argb),
        Leaf::Foliage => {
            let [r, g, b] = world.level.tint_at(TintKind::Foliage, pos.to_array());
            tinted(0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        }
    };
    let at = DVec3::new(
        f64::from(pos.x) + rng.next_double(),
        f64::from(pos.y) - 0.05,
        f64::from(pos.z) + rng.next_double(),
    );
    out.push(Spawn::new(spec, at, DVec3::ZERO));
}

/// `RedstoneWireBlock.COLORS`: the wire's colour at each power.
fn wire_color(power: u8) -> u32 {
    let power = f32::from(power) / 15.0;
    let red = power * 0.6 + if power > 0.0 { 0.4 } else { 0.3 };
    let green = (power * power * 0.7 - 0.5).clamp(0.0, 1.0);
    let blue = (power * power * 0.6 - 0.7).clamp(0.0, 1.0);
    let byte = |c: f32| (c * 255.0).floor() as u32;
    byte(red) << 16 | byte(green) << 8 | byte(blue)
}

/// `RedstoneWireBlock.animateTick`: dust along each powered line.
fn redstone_wire(state: u16, base: DVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let power: u8 = prop(state, "power")
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    if power == 0 {
        return;
    }
    let spec = ParticleSpec {
        kind: Kind::Dust,
        data: ParticleData::Dust {
            rgb: wire_color(power),
            scale: 1.0,
        },
    };
    let mut line = |side: Direction, along: Direction, from: f32, to: f32, rng: &mut JavaRandom| {
        let span = to - from;
        if rng.next_float() >= 0.2 * span {
            return;
        }
        let on_line = from + span * rng.next_float();
        let [sx, sy, sz] = side.step().map(|s| 0.4375 * s as f32);
        let [ax, ay, az] = along.step().map(|a| on_line * a as f32);
        let at = base
            + DVec3::new(
                0.5 + f64::from(sx + ax),
                0.5 + f64::from(sy + ay),
                0.5 + f64::from(sz + az),
            );
        out.push(Spawn::new(spec, at, DVec3::ZERO));
    };
    for (dir, key) in [
        (Direction::North, "north"),
        (Direction::East, "east"),
        (Direction::South, "south"),
        (Direction::West, "west"),
    ] {
        match prop(state, key).as_deref() {
            Some("up") => {
                line(dir, Direction::Up, -0.5, 0.5, rng);
                line(Direction::Down, dir, 0.0, 0.5, rng);
            }
            Some("side") => line(Direction::Down, dir, 0.0, 0.5, rng),
            _ => line(Direction::Down, dir, 0.0, 0.3, rng),
        }
    }
}

/// `PointedDripstoneBlock.animateTick`.
fn dripstone(world: &World, state: u16, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let tip = prop(state, "thickness").as_deref() == Some("tip");
    let wet = prop(state, "waterlogged").as_deref() == Some("true");
    if !tip || wet || !hangs(state) {
        return;
    }
    let roll = rng.next_float();
    if roll > 0.12 {
        return;
    }
    let Some(fluid) = fluid_above_stalactite(world, pos) else {
        return;
    };
    if fluid.is_some() || roll < 0.02 {
        out.push(drip_of(state, pos, fluid));
    }
}

/// `PointedDripstoneBlock.spawnDripParticle` (level event 1504).
pub fn dripstone_drip(world: &World, state: u16, pos: IVec3) -> Option<Spawn> {
    if block_name(state) != "pointed_dripstone" || !hangs(state) {
        return None;
    }
    let fluid = fluid_above_stalactite(world, pos)?;
    Some(drip_of(state, pos, fluid))
}

/// `isStalactite`: pointed dripstone pointing down.
fn hangs(state: u16) -> bool {
    prop(state, "vertical_direction").as_deref() == Some("down")
}

/// `getFluidAboveStalactite`.
fn fluid_above_stalactite(
    world: &World,
    pos: IVec3,
) -> Option<Option<crate::world::fluid::FluidKind>> {
    use crate::world::fluid::FluidKind;
    let at = |y: i32| world.level.block_state(pos.x, y, pos.z).unwrap_or(0);
    let mut root = None;
    for step in 1..11 {
        let y = pos.y + step;
        let above = at(y);
        if block_name(above) != "pointed_dripstone" {
            root = Some(y);
            break;
        }
        if !hangs(above) {
            return None;
        }
    }
    let root = root?;
    Some(if block_name(at(root + 1)) == "mud" {
        Some(FluidKind::Water)
    } else {
        crate::world::physics::fluid_at(world.level, pos.x, root + 1, pos.z)
    })
}

/// The drip from a tip: lava's, else water's.
fn drip_of(state: u16, pos: IVec3, fluid: Option<crate::world::fluid::FluidKind>) -> Spawn {
    let kind = if fluid == Some(crate::world::fluid::FluidKind::Lava) {
        Kind::DrippingDripstoneLava
    } else {
        Kind::DrippingDripstoneWater
    };
    let [ox, _, oz] = crate::world::block::offset::offset(state, pos.x, pos.z).map(f64::from);
    let drip = pos.as_dvec3() + DVec3::new(0.5 + ox, 0.3125 - 0.0625, 0.5 + oz);
    Spawn::simple(kind, drip, DVec3::ZERO)
}

/// `BeehiveBlock.trySpawnDripParticles`.
fn honey_drip(world: &World, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    if rng.next_float() < 0.3 {
        return;
    }
    let below = pos - IVec3::Y;
    let below_state = world
        .level
        .block_state(below.x, below.y, below.z)
        .unwrap_or(0);
    let below_wet = crate::world::fluid::fluid_of(below_state).is_some();
    if super::full_block(world, below) || below_wet {
        return;
    }
    let x = f64::from(pos.x) + rng.next_double();
    let z = f64::from(pos.z) + rng.next_double();
    let at = DVec3::new(x, f64::from(pos.y) - 0.05, z);
    out.push(Spawn::simple(Kind::DrippingHoney, at, DVec3::ZERO));
}

/// The blocks that fall and dust below (`FallingBlock`).
fn falls(name: &str) -> bool {
    matches!(
        name,
        "sand"
            | "red_sand"
            | "gravel"
            | "suspicious_sand"
            | "suspicious_gravel"
            | "anvil"
            | "chipped_anvil"
            | "damaged_anvil"
            | "dragon_egg"
            | "scaffolding"
    ) || name.ends_with("_concrete_powder")
}

/// `FallingBlock.animateTick`.
fn falling_dust(world: &World, state: u16, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    if rng.next_int(16) != 0 {
        return;
    }
    let below = pos - IVec3::Y;
    let below_state = world
        .level
        .block_state(below.x, below.y, below.z)
        .unwrap_or(0);
    let free = below_state == 0
        || pumpkin_data::BlockStateId::new(below_state).is_some_and(|id| {
            let s = id.to_state();
            s.is_air() || s.replaceable()
        })
        || matches!(block_name(below_state), "fire" | "soul_fire")
        || crate::world::fluid::fluid_of(below_state).is_some();
    if free {
        let spec = ParticleSpec {
            kind: Kind::FallingDust,
            data: ParticleData::Block(state),
        };
        out.push(Spawn::new(spec, spawn_below(pos, rng), DVec3::ZERO));
    }
}

/// `EndGatewayBlock.animateTick`: a mote for each face that shows.
fn end_gateway(world: &World, base: DVec3, pos: IVec3, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let faces = Direction::ALL
        .into_iter()
        .filter(|&d| {
            let [dx, dy, dz] = d.step();
            let next = world
                .level
                .block_state(pos.x + dx, pos.y + dy, pos.z + dz)
                .unwrap_or(0);
            block_name(next) != "end_gateway" && !state_flags::is_solid(next)
        })
        .count();
    for _ in 0..faces {
        let mut at = base + DVec3::new(rng.next_double(), rng.next_double(), rng.next_double());
        let mut aux = DVec3::new(
            (rng.next_double() - 0.5) * 0.5,
            (rng.next_double() - 0.5) * 0.5,
            (rng.next_double() - 0.5) * 0.5,
        );
        let flip = f64::from(rng.next_int(2) * 2 - 1);
        if rng.next_boolean() {
            at.z = base.z + 0.5 + 0.25 * flip;
            aux.z = f64::from(rng.next_float() * 2.0) * flip;
        } else {
            at.x = base.x + 0.5 + 0.25 * flip;
            aux.x = f64::from(rng.next_float() * 2.0) * flip;
        }
        out.push(Spawn::simple(Kind::Portal, at, aux));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redstone_dust_reddens_with_power() {
        assert_eq!(wire_color(0), 0x4C_0000);
        assert_eq!(wire_color(15), 0xFF_3200);
    }
}
