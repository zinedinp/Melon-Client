//! A saddled horse the local player steers, as the client runs it.

use crate::block_motion::block_below;
use glam::DVec3;

use crate::fluid::FluidKind;
use crate::level::Level;
use melon_core::math::angle::DEG_TO_RAD;
use crate::physics::{self, Aabb};

/// `Attributes.GRAVITY`'s default.
const GRAVITY: f64 = 0.08;
/// `LivingEntity.getFrictionInfluencedSpeed`'s ground factor.
const GROUND_ACCELERATION: f32 = 0.216_000_02;
/// `Attributes.AIR_DRAG` / vertical drag.
const HORIZONTAL_DRAG: f32 = 0.91;
const VERTICAL_DRAG: f64 = 0.98;
/// `LocalPlayer.modifyInput`'s 0.98 on both axes.
const INPUT_DAMPING: f32 = 0.98;
/// The keys after `getRiddenInput`, and the rider's look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorseInput {
    /// `zza`, already scaled (back is a quarter).
    pub forward: f32,
    /// `xxa * 0.5`, left-positive as vanilla.
    pub strafe: f32,
    /// Vanilla degrees (0 faces south).
    pub yaw: f32,
    /// The rider's pitch, halved (`getRiddenRotation`).
    pub pitch: f32,
}

impl HorseInput {
    /// `LocalPlayer.modifyInput` then `AbstractHorse.getRiddenInput`.
    pub fn from_keys(forward: f32, strafe: f32, yaw: f32, pitch: f32) -> Self {
        let (forward, left) = damped_keys(forward, -strafe);
        Self {
            forward: if forward <= 0.0 {
                forward * 0.25
            } else {
                forward
            },
            strafe: left * 0.5,
            yaw,
            pitch,
        }
    }
}

/// What the horse keeps from tick to tick.
#[derive(Clone, Copy, Debug)]
pub struct HorseSim {
    pub velocity: DVec3,
    pub on_ground: bool,
    /// `playerJumpPendingScale`, 0 when none is waiting.
    jump_pending: f32,
}

impl HorseSim {
    pub fn new(velocity: DVec3) -> Self {
        Self {
            velocity,
            on_ground: false,
            jump_pending: 0.0,
        }
    }

    /// `PlayerRideableJumping.onPlayerJump`.
    pub fn on_player_jump(&mut self, power: i32) {
        let power = power.max(0);
        self.jump_pending = if power >= 90 {
            1.0
        } else {
            0.4 + 0.4 * power as f32 / 90.0
        };
    }

    /// `travelRidden` plus `tickRidden`.
    pub fn tick(
        &mut self,
        level: &Level,
        colliders: &[Aabb],
        body: &mut HorseBody,
        input: HorseInput,
        rearing: bool,
    ) {
        body.yaw = input.yaw;
        body.pitch = input.pitch;
        let mut ridden = DVec3::new(f64::from(input.strafe), 0.0, f64::from(input.forward));
        if self.on_ground && self.jump_pending == 0.0 && rearing {
            ridden = DVec3::ZERO;
        }
        if self.on_ground && self.jump_pending > 0.0 {
            self.execute_jump(body.yaw, ridden.z, body.jump_strength);
        }
        self.travel(level, colliders, body, ridden);
    }

    /// `AbstractHorse.executeRidersJump`.
    fn execute_jump(&mut self, yaw: f32, forward: f64, jump_strength: f32) {
        let amount = self.jump_pending;
        self.jump_pending = 0.0;
        self.velocity.y = f64::from(jump_strength * amount);
        if forward > 0.0 {
            let (sin, cos) = ((yaw * DEG_TO_RAD).sin(), (yaw * DEG_TO_RAD).cos());
            self.velocity.x += f64::from(-0.4 * sin * amount);
            self.velocity.z += f64::from(0.4 * cos * amount);
        }
        self.on_ground = false;
    }

    /// `LivingEntity.travel` for a player-ridden horse: water or air.
    fn travel(&mut self, level: &Level, colliders: &[Aabb], body: &mut HorseBody, input: DVec3) {
        let bounds = body.bounds();
        if physics::touches_fluid(level, &bounds.deflate(0.001)) == Some(FluidKind::Water) {
            self.travel_in_water(level, colliders, body, input);
        } else {
            self.travel_in_air(level, colliders, body, input);
        }
    }

    fn travel_in_air(
        &mut self,
        level: &Level,
        colliders: &[Aabb],
        body: &mut HorseBody,
        input: DVec3,
    ) {
        let friction = if self.on_ground {
            block_below(level, body.pos).slipperiness
        } else {
            1.0
        };
        let acceleration = if self.on_ground {
            if friction > 0.6 {
                body.speed * (GROUND_ACCELERATION / (friction * friction * friction))
            } else {
                body.speed
            }
        } else {
            body.speed * 0.1
        };
        self.move_relative(input, acceleration, body.yaw);
        self.move_colliding(level, colliders, body);
        let drag = f64::from(if self.on_ground {
            friction * HORIZONTAL_DRAG
        } else {
            HORIZONTAL_DRAG
        });
        self.velocity.x *= drag;
        self.velocity.z *= drag;
        self.velocity.y = (self.velocity.y - GRAVITY) * VERTICAL_DRAG;
    }

    fn travel_in_water(
        &mut self,
        level: &Level,
        colliders: &[Aabb],
        body: &mut HorseBody,
        input: DVec3,
    ) {
        self.move_relative(input, 0.02, body.yaw);
        self.move_colliding(level, colliders, body);
        self.velocity.x *= 0.8;
        self.velocity.z *= 0.8;
        self.velocity.y = self.velocity.y * 0.8 - GRAVITY / 16.0;
    }

    /// `Entity.moveRelative` with vanilla's yaw (0 faces south).
    fn move_relative(&mut self, input: DVec3, speed: f32, yaw: f32) {
        let mut x = input.x;
        let mut z = input.z;
        let len_sq = x * x + z * z;
        if len_sq < 1.0e-7 {
            return;
        }
        if len_sq > 1.0 {
            let len = len_sq.sqrt();
            x /= len;
            z /= len;
        }
        let speed = f64::from(speed);
        let (x, z) = (x * speed, z * speed);
        let (sin, cos) = {
            let a = f64::from(yaw) * f64::from(DEG_TO_RAD);
            (a.sin(), a.cos())
        };
        self.velocity += DVec3::new(x * cos - z * sin, 0.0, z * cos + x * sin);
    }

    fn move_colliding(&mut self, level: &Level, colliders: &[Aabb], body: &mut HorseBody) {
        let aabb = body.bounds();
        let delta = self.velocity;
        let moved = physics::collide(
            level,
            colliders,
            aabb,
            delta,
            f64::from(body.step_height),
            self.on_ground,
        );
        body.pos += moved;
        if (delta.x - moved.x).abs() >= 1.0e-5 {
            self.velocity.x = 0.0;
        }
        if (delta.z - moved.z).abs() >= 1.0e-5 {
            self.velocity.z = 0.0;
        }
        self.on_ground = delta.y < 0.0 && moved.y != delta.y;
        if moved.y != delta.y {
            self.velocity.y = 0.0;
        }
        let factor = f64::from(block_below(level, body.pos).velocity_multiplier);
        self.velocity.x *= factor;
        self.velocity.z *= factor;
    }
}

/// Where the horse is and how it walks.
#[derive(Clone, Copy, Debug)]
pub struct HorseBody {
    pub pos: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub width: f64,
    pub height: f64,
    /// `Attributes.MOVEMENT_SPEED`.
    pub speed: f32,
    /// `Attributes.JUMP_STRENGTH`.
    pub jump_strength: f32,
    /// `Attributes.STEP_HEIGHT`.
    pub step_height: f32,
}

impl HorseBody {
    fn bounds(&self) -> Aabb {
        Aabb::at_feet(self.pos, self.width, self.height)
    }
}

/// `LocalPlayer.modifyInput` without sneak or item-use.
fn damped_keys(forward: f32, left: f32) -> (f32, f32) {
    let (mut x, mut z) = (left, forward);
    let len = (x * x + z * z).sqrt();
    if len < 1.0e-4 {
        return (0.0, 0.0);
    }
    x /= len;
    z /= len;
    x *= INPUT_DAMPING;
    z *= INPUT_DAMPING;
    let len = (x * x + z * z).sqrt();
    let (ax, az) = ((x / len).abs(), (z / len).abs());
    let tan = if az > ax { ax / az } else { az / ax };
    let stretched = (len * (1.0 + tan * tan).sqrt()).min(1.0);
    (z / len * stretched, x / len * stretched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, Section};
    use crate::level::Level;

    fn ground() -> Level {
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        for x in 0..16 {
            for z in 0..16 {
                level.set_block([x, 63, z], stone, false);
            }
        }
        level
    }

    fn body(pos: DVec3) -> HorseBody {
        HorseBody {
            pos,
            yaw: 0.0,
            pitch: 0.0,
            width: 1.396_484_4,
            height: 1.6,
            speed: 0.225,
            jump_strength: 0.7,
            step_height: 1.0,
        }
    }

    #[test]
    fn a_held_forward_walks_south_and_a_jump_lifts() {
        let level = ground();
        let mut sim = HorseSim::new(DVec3::ZERO);
        let mut horse = body(DVec3::new(8.0, 64.0, 8.0));
        let ahead = HorseInput::from_keys(1.0, 0.0, 0.0, 0.0);
        for _ in 0..20 {
            sim.tick(&level, &[], &mut horse, ahead, false);
        }
        assert!(horse.pos.z > 8.0 + 1.5, "{}", horse.pos.z);
        assert!((horse.pos.y - 64.0).abs() < 0.01, "{}", horse.pos.y);
        sim.on_player_jump(100);
        sim.tick(&level, &[], &mut horse, ahead, false);
        assert!(sim.velocity.y > 0.5, "{}", sim.velocity.y);
    }

    #[test]
    fn back_is_a_quarter_and_a_rearing_horse_stays() {
        let ahead = HorseInput::from_keys(1.0, 0.0, 0.0, 0.0);
        let back = HorseInput::from_keys(-1.0, 0.0, 0.0, 0.0);
        assert!(ahead.forward > 0.9);
        assert!((back.forward + ahead.forward * 0.25).abs() < 1e-5);
        let level = ground();
        let mut sim = HorseSim::new(DVec3::ZERO);
        let mut horse = body(DVec3::new(8.0, 64.0, 8.0));
        let still = HorseInput::from_keys(0.0, 0.0, 0.0, 0.0);
        for _ in 0..8 {
            sim.tick(&level, &[], &mut horse, still, false);
        }
        let settled = horse.pos.z;
        for _ in 0..10 {
            sim.tick(&level, &[], &mut horse, ahead, true);
        }
        assert!(
            (horse.pos.z - settled).abs() < 0.02,
            "{} from {settled}",
            horse.pos.z
        );
    }
}
