//! `ClientLevel.tickWeatherEffects`'s particles.

use glam::{DVec3, IVec3};

use super::super::{Kind, Spawn, World};
use super::{block_name, lit};
use crate::settings::options::ParticleStatus;
use crate::world::fluid::{self, FluidKind};
use crate::world::java_random::JavaRandom;

/// The rain's splashes this tick.
#[allow(clippy::too_many_arguments)]
pub fn rain_splashes(
    world: &World,
    camera: IVec3,
    radius: i32,
    rain: f32,
    status: ParticleStatus,
    rains_at: &dyn Fn(IVec3) -> bool,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    if rain <= 0.0 || radius <= 0 || status == ParticleStatus::Minimal {
        return;
    }
    let diameter = 2 * radius + 1;
    let thin = if status == ParticleStatus::Decreased {
        2
    } else {
        1
    };
    let drops = (0.225 * (diameter * diameter) as f32 * rain * rain) as i32 / thin;
    let min_y = world.level.height().0;
    for _ in 0..drops {
        let x = camera.x + rng.next_int(diameter) - radius;
        let z = camera.z + rng.next_int(diameter) - radius;
        let top = world.level.motion_blocking_height(x, z);
        if top <= min_y || (top - camera.y).abs() > 10 {
            continue;
        }
        let at = IVec3::new(x, top, z);
        if !rains_at(at) {
            continue;
        }
        let ground = at - IVec3::Y;
        let bx = rng.next_double();
        let bz = rng.next_double();
        let state = world
            .level
            .block_state(ground.x, ground.y, ground.z)
            .unwrap_or(0);
        let block_top =
            crate::world::physics::block_shapes(world.level, (ground.x, ground.y, ground.z))
                .filter(|b| {
                    let (px, pz) = (f64::from(ground.x) + bx, f64::from(ground.z) + bz);
                    b.min.x <= px && px <= b.max.x && b.min.z <= pz && pz <= b.max.z
                })
                .map(|b| b.max.y - f64::from(ground.y))
                .fold(0.0, f64::max);
        let here = fluid::fluid_of(state);
        let fluid_top = here.map_or(0.0, |f| f64::from(f.own_height()));
        let lava = here.is_some_and(|f| f.kind == FluidKind::Lava);
        let name = block_name(state);
        let hot = lava || name == "magma_block" || (name.ends_with("campfire") && lit(state));
        let kind = if hot { Kind::Smoke } else { Kind::Rain };
        let pos = ground.as_dvec3() + DVec3::new(bx, block_top.max(fluid_top), bz);
        out.push(Spawn::simple(kind, pos, DVec3::ZERO));
    }
}
