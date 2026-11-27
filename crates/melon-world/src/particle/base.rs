//! One particle.

use glam::{DVec3, Vec3};

use super::animation::Rect;
use super::animation::fade::{Glow, LifetimeAlpha, SizeCurve};
use super::behavior::Behavior;
use super::{Kind, TickCtx};
use crate::world::java_random::JavaRandom;
use crate::world::physics::{self, Aabb};

/// `Particle.MAXIMUM_COLLISION_VELOCITY_SQUARED`.
const MAX_COLLISION_SPEED_SQ: f64 = 100.0 * 100.0;

/// The texture a particle samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Atlas {
    /// The block atlas, which holds the particle textures too.
    #[default]
    Blocks,
    /// An item's own texture (`item` particles), by registry id.
    Item(u16),
}

/// How the quad faces the camera (`FacingCameraMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Facing {
    /// `LOOKAT_XYZ`: square to the camera.
    #[default]
    Camera,
    /// `LOOKAT_Y`: turned with the camera's yaw only.
    Upright,
}

/// How a particle is drawn, apart from where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub atlas: Atlas,
    pub uv: Rect,
    /// `Layer.translucent`: blended, without writing depth.
    pub translucent: bool,
    pub facing: Facing,
    pub size: SizeCurve,
    pub glow: Glow,
    /// The sprite follows the age through the type's set.
    pub animated: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            atlas: Atlas::Blocks,
            uv: [0.0; 4],
            translucent: false,
            facing: Facing::Camera,
            size: SizeCurve::Fixed,
            glow: Glow::World,
            animated: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    pub kind: Kind,
    /// Written through [`Self::set_pos`], which keeps `prev` in its place.
    pos: DVec3,
    /// `xo`, `yo`, `zo` less `pos`. Only drawn from, never moved by, so
    /// f32 holds it without touching the double physics.
    prev_offset: Vec3,
    pub vel: DVec3,
    /// `bbWidth`, `bbHeight`.
    pub width: f32,
    pub height: f32,
    pub age: i32,
    pub lifetime: i32,
    pub gravity: f32,
    pub friction: f32,
    pub quad_size: f32,
    /// RGBA, `0..1`.
    pub color: [f32; 4],
    pub roll: f32,
    pub prev_roll: f32,
    pub look: Look,
    pub alpha_curve: LifetimeAlpha,
    /// `hasPhysics`: collides with blocks.
    pub physics: bool,
    /// Moves without colliding nor landing (classes that override `move`).
    pub free_move: bool,
    pub on_ground: bool,
    /// Once its fall is stopped by a block it never moves again.
    pub stopped: bool,
    pub speed_up_when_blocked: bool,
    pub removed: bool,
    pub behavior: Behavior,
}

impl Particle {
    /// `Particle(level, x, y, z)`: still, 0.2 wide, a random short life.
    pub fn at(kind: Kind, pos: DVec3, rng: &mut JavaRandom) -> Self {
        let lifetime = (4.0 / (rng.next_float() * 0.9 + 0.1)) as i32;
        let quad_size = 0.1 * (rng.next_float() * 0.5 + 0.5) * 2.0;
        Self {
            kind,
            pos,
            prev_offset: Vec3::ZERO,
            vel: DVec3::ZERO,
            width: 0.2,
            height: 0.2,
            age: 0,
            lifetime,
            gravity: 0.0,
            friction: 0.98,
            quad_size,
            color: [1.0; 4],
            roll: 0.0,
            prev_roll: 0.0,
            look: Look::default(),
            alpha_curve: LifetimeAlpha::OPAQUE,
            physics: true,
            free_move: false,
            on_ground: false,
            stopped: false,
            speed_up_when_blocked: false,
            removed: false,
            behavior: Behavior::Default,
        }
    }

    /// `Particle(level, x, y, z, xa, ya, za)`.
    pub fn moving(kind: Kind, pos: DVec3, aux: DVec3, rng: &mut JavaRandom) -> Self {
        let mut p = Self::at(kind, pos, rng);
        let mut jitter = || f64::from((rng.next_float() * 2.0 - 1.0) * 0.4);
        let mut vel = DVec3::new(aux.x + jitter(), aux.y + jitter(), aux.z + jitter());
        let speed = f64::from((rng.next_float() + rng.next_float() + 1.0) * 0.15);
        let length = vel.length();
        if length > 0.0 {
            vel = vel / length * speed * f64::from(0.4f32);
        }
        vel.y += f64::from(0.1f32);
        p.vel = vel;
        p
    }

    /// `setPower`: faster or slower, the upward kick kept.
    pub fn set_power(&mut self, power: f32) {
        let power = f64::from(power);
        self.vel.x *= power;
        self.vel.y = (self.vel.y - f64::from(0.1f32)) * power + f64::from(0.1f32);
        self.vel.z *= power;
    }

    /// `SingleQuadParticle.scale`: the quad and the box.
    pub fn scale(&mut self, scale: f32) {
        self.quad_size *= scale;
        self.width = 0.2 * scale;
        self.height = 0.2 * scale;
    }

    pub fn set_rgb(&mut self, [r, g, b]: [f32; 3]) {
        self.color[0] = r;
        self.color[1] = g;
        self.color[2] = b;
    }

    /// The box around it: centred on `pos` across, standing on it.
    pub fn bounds(&self) -> Aabb {
        Aabb::at_feet(self.pos, f64::from(self.width), f64::from(self.height))
    }

    /// `Particle.move`.
    pub fn move_by(&mut self, ctx: &TickCtx, delta: DVec3) {
        if self.free_move {
            self.set_pos(self.pos + delta);
            return;
        }
        if self.stopped {
            return;
        }
        let mut moved = delta;
        if self.physics && delta != DVec3::ZERO && delta.length_squared() < MAX_COLLISION_SPEED_SQ {
            moved = physics::collide(ctx.level, &[], self.bounds(), delta, 0.0, false);
        }
        self.set_pos(self.pos + moved);
        if delta.y.abs() >= 1.0e-5 && moved.y.abs() < 1.0e-5 {
            self.stopped = true;
        }
        self.on_ground = delta.y != moved.y && delta.y < 0.0;
        if delta.x != moved.x {
            self.vel.x = 0.0;
        }
        if delta.z != moved.z {
            self.vel.z = 0.0;
        }
    }

    /// `Particle.tick`: ages, falls by its gravity, moves and slows down.
    pub fn base_tick(&mut self, ctx: &TickCtx) {
        self.reset_prev();
        if self.age >= self.lifetime {
            self.age += 1;
            self.removed = true;
            return;
        }
        self.age += 1;
        self.vel.y -= 0.04 * f64::from(self.gravity);
        let y = self.pos.y;
        self.move_by(ctx, self.vel);
        if self.speed_up_when_blocked && self.pos.y == y {
            self.vel.x *= 1.1;
            self.vel.z *= 1.1;
        }
        self.vel *= f64::from(self.friction);
        if self.on_ground {
            self.vel.x *= f64::from(0.7f32);
            self.vel.z *= f64::from(0.7f32);
        }
    }

    /// The start of a tick for classes with their own: the place before.
    pub fn begin_tick(&mut self) {
        self.reset_prev();
        self.prev_roll = self.roll;
    }

    /// `age++ >= lifetime`: whether it is gone now.
    pub fn age_out(&mut self) -> bool {
        let old = self.age;
        self.age += 1;
        if old >= self.lifetime {
            self.removed = true;
        }
        self.removed
    }

    /// The `lifetime-- <= 0` countdown some classes use instead of age.
    pub fn count_down(&mut self) -> bool {
        let old = self.lifetime;
        self.lifetime -= 1;
        if old <= 0 {
            self.removed = true;
        }
        self.removed
    }

    /// The quad's size at `partial` (`getQuadSize`).
    pub fn size(&self, partial: f32) -> f32 {
        self.quad_size * self.look.size.factor(self.age, self.lifetime, partial)
    }

    /// The alpha drawn at `partial`.
    pub fn alpha(&self, partial: f32) -> f32 {
        if self.alpha_curve.is_opaque() {
            self.color[3]
        } else {
            self.alpha_curve.at(self.age, self.lifetime, partial)
        }
    }

    /// Where it is drawn at `partial`.
    pub fn lerp_pos(&self, partial: f32) -> DVec3 {
        self.prev().lerp(self.pos, f64::from(partial))
    }

    pub fn pos(&self) -> DVec3 {
        self.pos
    }

    /// Moves it to `pos`; where it was the tick before stays.
    pub fn set_pos(&mut self, pos: DVec3) {
        let prev = self.prev();
        self.pos = pos;
        self.set_prev(prev);
    }

    /// Where it was when the tick began (`xo`, `yo`, `zo`).
    pub fn prev(&self) -> DVec3 {
        self.pos + self.prev_offset.as_dvec3()
    }

    pub fn set_prev(&mut self, prev: DVec3) {
        self.prev_offset = (prev - self.pos).as_vec3();
    }

    /// `xo = x` and so on: the tick before is where it is now.
    pub fn reset_prev(&mut self) {
        self.prev_offset = Vec3::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_keeps_where_it_was_and_far_away_it_interpolates_exactly() {
        let far = DVec3::new(29_999_000.25, 70.5, -29_999_000.75);
        let mut p = Particle::at(Kind::Flame, far, &mut JavaRandom::new(1));
        p.set_pos(far + DVec3::new(0.5, -0.25, 0.125));
        assert_eq!(p.prev(), far);
        p.set_pos(p.pos() + DVec3::new(0.25, 0.0, 0.0));
        assert_eq!(p.prev(), far);
        assert_eq!(p.lerp_pos(0.0), far);
        assert_eq!(p.lerp_pos(1.0), p.pos());
        p.reset_prev();
        assert_eq!(p.prev(), p.pos());
    }
}
