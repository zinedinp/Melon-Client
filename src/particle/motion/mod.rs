//! Particles that go somewhere.

use glam::DVec3;

use super::base::Particle;
use super::options::PositionSource;
use super::{EntityBox, TickCtx};

pub mod emitter;

/// `PortalParticle.tick`.
pub fn portal(p: &mut Particle, start: DVec3) {
    p.begin_tick();
    if p.age_out() {
        return;
    }
    let a = p.age as f32 / p.lifetime.max(1) as f32;
    let pos = 1.0 - (-a + a * a * 2.0);
    let pos = f64::from(pos);
    p.set_pos(DVec3::new(
        start.x + p.vel.x * pos,
        start.y + p.vel.y * pos + f64::from(1.0 - a),
        start.z + p.vel.z * pos,
    ));
}

/// `ReversePortalParticle.tick`: speeds off along its velocity.
pub fn reverse_portal(p: &mut Particle) {
    p.begin_tick();
    if p.age_out() {
        return;
    }
    let speed = f64::from(p.age as f32 / p.lifetime.max(1) as f32);
    p.set_pos(p.pos() + p.vel * speed);
}

/// `FlyTowardsPositionParticle.tick`.
pub fn fly_towards(p: &mut Particle, start: DVec3) {
    p.begin_tick();
    if p.age_out() {
        return;
    }
    let pos = 1.0 - p.age as f32 / p.lifetime.max(1) as f32;
    let mut pp = 1.0 - pos;
    pp *= pp;
    pp *= pp;
    let pos = f64::from(pos);
    p.set_pos(DVec3::new(
        start.x + p.vel.x * pos,
        start.y + p.vel.y * pos - f64::from(pp * 1.2),
        start.z + p.vel.z * pos,
    ));
}

/// `TrailParticle.tick`.
pub fn trail(p: &mut Particle, target: DVec3) {
    p.begin_tick();
    if p.age_out() {
        return;
    }
    let left = (p.lifetime - p.age).max(1);
    p.set_pos(p.pos().lerp(target, 1.0 / f64::from(left)));
}

/// Where a vibration is going now.
pub fn destination(to: PositionSource, ctx: &TickCtx) -> Option<DVec3> {
    match to {
        PositionSource::Block([x, y, z]) => Some(DVec3::new(
            f64::from(x) + 0.5,
            f64::from(y) + 0.5,
            f64::from(z) + 0.5,
        )),
        PositionSource::Entity { id, y_offset } => {
            let entity: EntityBox = (ctx.entity)(id)?;
            Some(entity.pos + DVec3::new(0.0, f64::from(y_offset), 0.0))
        }
    }
}

/// The yaw and pitch from `to` back to `from` (`VibrationSignalParticle`'s rotation).
pub fn heading(from: DVec3, to: DVec3) -> (f32, f32) {
    let d = from - to;
    let rot = d.x.atan2(d.z) as f32;
    let pitch = d.y.atan2((d.x * d.x + d.z * d.z).sqrt()) as f32;
    (rot, pitch)
}

/// `VibrationSignalParticle.tick`.
pub fn vibration(p: &mut Particle, ctx: &TickCtx) {
    use super::behavior::Behavior;
    p.begin_tick();
    if p.age_out() {
        return;
    }
    let Behavior::Vibration { to, .. } = p.behavior else {
        return;
    };
    let Some(target) = destination(to, ctx) else {
        p.removed = true;
        return;
    };
    let left = (p.lifetime - p.age).max(1);
    p.set_pos(p.pos().lerp(target, 1.0 / f64::from(left)));
    let (new_rot, new_pitch) = heading(p.pos(), target);
    if let Behavior::Vibration {
        rot,
        prev_rot,
        pitch,
        prev_pitch,
        ..
    } = &mut p.behavior
    {
        *prev_rot = *rot;
        *rot = new_rot;
        *prev_pitch = *pitch;
        *pitch = new_pitch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particle::Kind;
    use crate::world::java_random::JavaRandom;

    #[test]
    fn a_trail_arrives_when_its_life_ends() {
        let mut rng = JavaRandom::new(1);
        let mut p = Particle::at(Kind::Trail, DVec3::ZERO, &mut rng);
        p.lifetime = 4;
        let target = DVec3::new(4.0, 0.0, 0.0);
        for _ in 0..3 {
            trail(&mut p, target);
        }
        assert!((p.pos().x - 4.0).abs() < 1e-9, "{:?}", p.pos());
    }

    #[test]
    fn a_portal_particle_returns_to_its_start() {
        let mut rng = JavaRandom::new(1);
        let start = DVec3::new(0.0, 64.0, 0.0);
        let mut p = Particle::at(Kind::Portal, start, &mut rng);
        p.vel = DVec3::new(1.0, 0.0, 0.0);
        p.lifetime = 10;
        for _ in 0..10 {
            portal(&mut p, start);
        }
        assert!(p.pos().x.abs() < 1e-6 && (p.pos().y - 64.0).abs() < 1e-6);
    }
}
