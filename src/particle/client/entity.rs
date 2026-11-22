//! An entity's own particles, made on the client.

use glam::DVec3;

use super::super::{Kind, ParticleSpec, Spawn};
use crate::world::java_random::JavaRandom;

/// `spawnSprintParticle`: one crumb of the block underfoot, kicked back.
pub fn sprint_dust(
    feet: DVec3,
    width: f32,
    vel: DVec3,
    on: ([i32; 3], u16),
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let (pos, state) = on;
    if state == 0 || super::block_name(state) == "barrier" {
        return;
    }
    let w = f64::from(width);
    let mut x = feet.x + (rng.next_double() - 0.5) * w;
    let mut z = feet.z + (rng.next_double() - 0.5) * w;
    let block = feet.floor().as_ivec3();
    if block.x != pos[0] {
        x = x.clamp(f64::from(pos[0]), f64::from(pos[0]) + 1.0);
    }
    if block.z != pos[2] {
        z = z.clamp(f64::from(pos[2]), f64::from(pos[2]) + 1.0);
    }
    out.push(Spawn::new(
        ParticleSpec::block(Kind::Block, state),
        DVec3::new(x, feet.y + 0.1, z),
        DVec3::new(vel.x * -4.0, 1.5, vel.z * -4.0),
    ));
}

/// `doWaterSplashEffect`'s particles.
pub fn water_splash(
    feet: DVec3,
    width: f32,
    vel: DVec3,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let w = f64::from(width);
    let surface = feet.y.floor() + 1.0;
    let count = (1.0 + width * 20.0) as i32;
    for _ in 0..count {
        let xo = (rng.next_double() * 2.0 - 1.0) * w;
        let zo = (rng.next_double() * 2.0 - 1.0) * w;
        let aux = DVec3::new(vel.x, vel.y - rng.next_double() * f64::from(0.2f32), vel.z);
        out.push(Spawn::simple(
            Kind::Bubble,
            DVec3::new(feet.x + xo, surface, feet.z + zo),
            aux,
        ));
    }
    for _ in 0..count {
        let xo = (rng.next_double() * 2.0 - 1.0) * w;
        let zo = (rng.next_double() * 2.0 - 1.0) * w;
        out.push(Spawn::simple(
            Kind::Splash,
            DVec3::new(feet.x + xo, surface, feet.z + zo),
            vel,
        ));
    }
}

/// `spawnItemParticles`.
pub fn item_crumbs(
    item: u16,
    eye: DVec3,
    yaw: f32,
    pitch: f32,
    count: u32,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    let spec = ParticleSpec::item(item);
    let turn = |v: DVec3| {
        let v = x_rot(v, -pitch.to_radians());
        y_rot(v, -yaw.to_radians())
    };
    for _ in 0..count {
        let d = DVec3::new(
            (f64::from(rng.next_float()) - 0.5) * 0.1,
            f64::from(rng.next_float()) * 0.1 + 0.1,
            0.0,
        );
        let d = turn(d);
        let y1 = -f64::from(rng.next_float()) * 0.6 - 0.3;
        let p = DVec3::new((f64::from(rng.next_float()) - 0.5) * 0.3, y1, 0.6);
        let p = turn(p) + eye;
        out.push(Spawn::new(spec, p, DVec3::new(d.x, d.y + 0.05, d.z)));
    }
}

/// `Vec3.xRot`.
pub(crate) fn x_rot(v: DVec3, angle: f32) -> DVec3 {
    let (s, c) = (f64::from(angle.sin()), f64::from(angle.cos()));
    DVec3::new(v.x, v.y * c + v.z * s, v.z * c - v.y * s)
}

/// `Vec3.yRot`.
pub(crate) fn y_rot(v: DVec3, angle: f32) -> DVec3 {
    let (s, c) = (f64::from(angle.sin()), f64::from(angle.cos()));
    DVec3::new(v.x * c + v.z * s, v.y, v.z * c - v.x * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crumbs_fall_in_front_of_the_face() {
        let mut out = Vec::new();
        let eye = DVec3::new(0.0, 65.62, 0.0);
        item_crumbs(1, eye, 0.0, 0.0, 5, &mut JavaRandom::new(4), &mut out);
        assert_eq!(out.len(), 5);
        assert!(out.iter().all(|s| s.pos.z > 0.5 && s.pos.y < eye.y));
    }
}
