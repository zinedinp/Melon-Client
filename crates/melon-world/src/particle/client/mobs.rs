//! Other entities' particles, made on the client each tick.

use glam::DVec3;

use super::super::options::{ParticleData, ParticleSpec};
use super::super::{EntityBox, Kind, Spawn, World};
use crate::world::java_random::JavaRandom;

/// An entity as its tick sees it.
#[derive(Clone, Copy, Debug)]
pub struct Ticking {
    pub body: EntityBox,
    /// Shared flags 1 and 3.
    pub crouching: bool,
    pub sprinting: bool,
}

/// One tick of `entity`'s particles.
pub fn tick(world: &World, entity: &Ticking, rng: &mut JavaRandom, out: &mut Vec<Spawn>) {
    let body = &entity.body;
    let cell = body.pos.floor().as_ivec3();
    let fluid = crate::world::physics::fluid_at(world.level, cell.x, cell.y, cell.z);
    let in_water = fluid == Some(crate::world::fluid::FluidKind::Water);
    match body.name {
        "tnt" => out.push(Spawn::simple(
            Kind::Smoke,
            body.pos + DVec3::Y * 0.5,
            DVec3::ZERO,
        )),
        "enderman" | "endermite" => {
            let drop = if body.name == "enderman" { 0.25 } else { 0.0 };
            for _ in 0..2 {
                let at = random_in(body, rng) - DVec3::Y * drop;
                let aux = DVec3::new(
                    (rng.next_double() - 0.5) * 2.0,
                    -rng.next_double(),
                    (rng.next_double() - 0.5) * 2.0,
                );
                out.push(Spawn::simple(Kind::Portal, at, aux));
            }
        }
        "blaze" => {
            if rng.next_int(24) == 0 {
                rng.next_float();
                rng.next_float();
            }
            for _ in 0..2 {
                out.push(Spawn::simple(
                    Kind::LargeSmoke,
                    random_in(body, rng),
                    DVec3::ZERO,
                ));
            }
        }
        "fireball" | "small_fireball" | "wither_skull" | "dragon_fireball" => {
            if in_water {
                let at = body.pos - body.vel * 0.25;
                for _ in 0..4 {
                    out.push(Spawn::simple(Kind::Bubble, at, body.vel));
                }
            }
            let trail = if body.name == "dragon_fireball" {
                ParticleSpec {
                    kind: Kind::DragonBreath,
                    data: ParticleData::Power(1.0),
                }
            } else {
                ParticleSpec::simple(Kind::Smoke)
            };
            out.push(Spawn::new(trail, body.pos + DVec3::Y * 0.5, DVec3::ZERO));
        }
        "eye_of_ender" => {
            let origin = body.pos + body.vel - body.vel * 0.25;
            if in_water {
                for _ in 0..4 {
                    out.push(Spawn::simple(Kind::Bubble, origin, body.vel));
                }
            } else {
                let at = DVec3::new(
                    origin.x + rng.next_double() * 0.6 - 0.3,
                    origin.y - 0.5,
                    origin.z + rng.next_double() * 0.6 - 0.3,
                );
                out.push(Spawn::simple(Kind::Portal, at, body.vel));
            }
        }
        "firework_rocket" => {
            let aux = DVec3::new(
                rng.next_gaussian() * 0.05,
                -body.vel.y * 0.5,
                rng.next_gaussian() * 0.05,
            );
            out.push(Spawn::simple(Kind::Firework, body.pos, aux));
        }
        _ => {}
    }
    let in_lava = fluid == Some(crate::world::fluid::FluidKind::Lava);
    if entity.sprinting && !entity.crouching && !in_water && !in_lava {
        let on = [
            body.pos.x.floor() as i32,
            (body.pos.y - 0.2).floor() as i32,
            body.pos.z.floor() as i32,
        ];
        let state = world.level.block_state(on[0], on[1], on[2]).unwrap_or(0);
        super::entity::sprint_dust(body.pos, body.width, body.vel, (on, state), rng, out);
    }
}

/// `getRandomX(0.5)`, `getRandomY()`, `getRandomZ(0.5)`.
fn random_in(body: &EntityBox, rng: &mut JavaRandom) -> DVec3 {
    let w = f64::from(body.width) * 0.5;
    let x = body.pos.x + (2.0 * rng.next_double() - 1.0) * w;
    let y = body.pos.y + f64::from(body.height) * rng.next_double();
    let z = body.pos.z + (2.0 * rng.next_double() - 1.0) * w;
    DVec3::new(x, y, z)
}
