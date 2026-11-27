//! A boat the local player steers, as the client runs it.

use glam::{DVec3, IVec3};

use crate::math::angle::{DEG_TO_RAD, cos, sin};
use crate::world::fluid::{self, FluidKind};
use crate::world::level::Level;
use crate::world::physics::{self, Aabb};

/// `AbstractBoat.getDefaultGravity`.
const GRAVITY: f64 = 0.04;

/// `AbstractBoat.Status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    InAir,
}

/// The keys that steer it (`AbstractBoat.setInput`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoatInput {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl BoatInput {
    /// `controlBoat`'s paddle state.
    pub fn paddles(self) -> (bool, bool) {
        (
            self.right && !self.left || self.up,
            self.left && !self.right || self.up,
        )
    }
}

/// Where the boat is and how big: the part of the entity the sim moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatBody {
    pub pos: DVec3,
    /// Vanilla's yaw, in degrees (0 faces south).
    pub yaw: f32,
    pub width: f64,
    pub height: f64,
}

impl BoatBody {
    fn bounds(&self) -> Aabb {
        Aabb::at_feet(self.pos, self.width, self.height)
    }
}

/// What the boat keeps from tick to tick.
#[derive(Clone, Debug)]
pub struct BoatSim {
    pub status: Status,
    old_status: Status,
    water_level: f64,
    land_friction: f32,
    last_yd: f64,
    delta_rotation: f32,
    /// `deltaMovement`, blocks a tick.
    pub velocity: DVec3,
    pub on_ground: bool,
    /// `DATA_ID_PADDLE_LEFT`/`RIGHT` as `controlBoat` sets them: what
    /// `ServerboundPaddleBoatPacket` reports.
    pub paddles: (bool, bool),
}

impl Default for BoatSim {
    fn default() -> Self {
        Self {
            status: Status::InAir,
            old_status: Status::InAir,
            water_level: 0.0,
            land_friction: 0.0,
            last_yd: 0.0,
            delta_rotation: 0.0,
            velocity: DVec3::ZERO,
            on_ground: false,
            paddles: (false, false),
        }
    }
}

/// The water in a block: its height, and whether it is a source.
fn water_at(level: &Level, x: i32, y: i32, z: i32) -> Option<(f32, bool)> {
    let state = fluid::fluid_of(physics::block_at(level, x, y, z))
        .filter(|f| f.kind == FluidKind::Water)?;
    let height = physics::fluid_height(level, IVec3::new(x, y, z), FluidKind::Water)?;
    Some((height, state.amount == 8 && !state.falling))
}

impl BoatSim {
    /// A boat taking over from where the server left it: at rest.
    pub fn new(velocity: DVec3) -> Self {
        Self {
            velocity,
            ..Self::default()
        }
    }

    /// `AbstractBoat.tick`'s authoritative half: float, steer, move.
    pub fn tick(
        &mut self,
        level: &Level,
        colliders: &[Aabb],
        body: &mut BoatBody,
        input: BoatInput,
    ) {
        self.old_status = self.status;
        self.status = self.status_of(level, body);
        self.float(level, body);
        self.steer(body, input);
        self.move_by_velocity(level, colliders, body);
    }

    /// `AbstractBoat.getStatus`.
    fn status_of(&mut self, level: &Level, body: &BoatBody) -> Status {
        let bounds = body.bounds();
        if let Some(under) = under_water(level, &bounds) {
            self.water_level = bounds.max.y;
            return under;
        }
        if self.check_in_water(level, &bounds) {
            return Status::InWater;
        }
        let friction = ground_friction(level, &bounds);
        if friction > 0.0 {
            self.land_friction = friction;
            Status::OnLand
        } else {
            Status::InAir
        }
    }

    /// `checkInWater`.
    fn check_in_water(&mut self, level: &Level, bounds: &Aabb) -> bool {
        let (x0, x1) = (bounds.min.x.floor() as i32, bounds.max.x.ceil() as i32);
        let (y0, y1) = (
            bounds.min.y.floor() as i32,
            (bounds.min.y + 0.001).ceil() as i32,
        );
        let (z0, z1) = (bounds.min.z.floor() as i32, bounds.max.z.ceil() as i32);
        let mut in_water = false;
        self.water_level = -f64::MAX;
        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    if let Some((height, _)) = water_at(level, x, y, z) {
                        let top = f64::from(y as f32 + height);
                        self.water_level = self.water_level.max(top);
                        in_water |= bounds.min.y < top;
                    }
                }
            }
        }
        in_water
    }

    /// `getWaterLevelAbove`: the surface above the boat's top.
    fn water_level_above(&self, level: &Level, bounds: &Aabb) -> f64 {
        let (x0, x1) = (bounds.min.x.floor() as i32, bounds.max.x.ceil() as i32);
        let y0 = bounds.max.y.floor() as i32;
        let y1 = (bounds.max.y - self.last_yd).ceil() as i32;
        let (z0, z1) = (bounds.min.z.floor() as i32, bounds.max.z.ceil() as i32);
        for y in y0..y1 {
            let mut height = 0.0_f32;
            let mut full = false;
            'column: for x in x0..x1 {
                for z in z0..z1 {
                    if let Some((h, _)) = water_at(level, x, y, z) {
                        height = height.max(h);
                    }
                    if height >= 1.0 {
                        full = true;
                        break 'column;
                    }
                }
            }
            if !full {
                return f64::from(y as f32 + height);
            }
        }
        f64::from(y1) + 1.0
    }

    /// `floatBoat`: gravity, buoyancy and the drag of whatever it is in.
    fn float(&mut self, level: &Level, body: &mut BoatBody) {
        let mut v_speed = -GRAVITY;
        let mut buoyancy = 0.0;
        let inv_friction: f32;
        let landed_in_water = self.old_status == Status::InAir
            && self.status != Status::InAir
            && self.status != Status::OnLand;
        if landed_in_water {
            self.water_level = body.pos.y;
            let target = self.water_level_above(level, &body.bounds()) - body.height + 0.101;
            let moved = body
                .bounds()
                .offset(DVec3::new(0.0, target - body.pos.y, 0.0));
            if physics::no_collision(level, &moved) {
                body.pos.y = target;
                self.velocity.y = 0.0;
                self.last_yd = 0.0;
            }
            self.status = Status::InWater;
            return;
        }
        match self.status {
            Status::InWater => {
                buoyancy = (self.water_level - body.pos.y) / body.height;
                inv_friction = 0.9;
            }
            Status::UnderFlowingWater => {
                v_speed = -7.0e-4;
                inv_friction = 0.9;
            }
            Status::UnderWater => {
                buoyancy = f64::from(0.01_f32);
                inv_friction = 0.45;
            }
            Status::InAir => inv_friction = 0.9,
            Status::OnLand => {
                inv_friction = self.land_friction;
                self.land_friction /= 2.0;
            }
        }
        let drag = f64::from(inv_friction);
        self.velocity = DVec3::new(
            self.velocity.x * drag,
            self.velocity.y + v_speed,
            self.velocity.z * drag,
        );
        self.delta_rotation *= inv_friction;
        if buoyancy > 0.0 {
            self.velocity.y = (self.velocity.y + buoyancy * (GRAVITY / 0.65)) * 0.75;
        }
    }

    /// `controlBoat`: the turn, and the push along where it faces.
    fn steer(&mut self, body: &mut BoatBody, input: BoatInput) {
        let mut acceleration = 0.0_f32;
        if input.left {
            self.delta_rotation -= 1.0;
        }
        if input.right {
            self.delta_rotation += 1.0;
        }
        if input.right != input.left && !input.up && !input.down {
            acceleration += 0.005;
        }
        body.yaw += self.delta_rotation;
        if input.up {
            acceleration += 0.04;
        }
        if input.down {
            acceleration -= 0.005;
        }
        self.velocity += DVec3::new(
            f64::from(sin(-body.yaw * DEG_TO_RAD) * acceleration),
            0.0,
            f64::from(cos(body.yaw * DEG_TO_RAD) * acceleration),
        );
        self.paddles = input.paddles();
    }

    /// `Entity.move(MoverType.SELF, deltaMovement)`.
    fn move_by_velocity(&mut self, level: &Level, colliders: &[Aabb], body: &mut BoatBody) {
        let delta = self.velocity;
        let moved = physics::collide(level, colliders, body.bounds(), delta, 0.0, self.on_ground);
        body.pos += moved;
        let blocked = |a: f64, b: f64| (a - b).abs() >= 1.0e-5;
        let x_collision = blocked(delta.x, moved.x);
        let z_collision = blocked(delta.z, moved.z);
        let vertical = delta.y != moved.y;
        self.on_ground = vertical && delta.y < 0.0;
        self.last_yd = self.velocity.y;
        if x_collision {
            self.velocity.x = 0.0;
        }
        if z_collision {
            self.velocity.z = 0.0;
        }
        if vertical {
            self.velocity.y = 0.0;
        }
    }
}

/// `isUnderwater`.
fn under_water(level: &Level, bounds: &Aabb) -> Option<Status> {
    let top = bounds.max.y + 0.001;
    let (x0, x1) = (bounds.min.x.floor() as i32, bounds.max.x.ceil() as i32);
    let (y0, y1) = (bounds.max.y.floor() as i32, top.ceil() as i32);
    let (z0, z1) = (bounds.min.z.floor() as i32, bounds.max.z.ceil() as i32);
    let mut under = false;
    for x in x0..x1 {
        for y in y0..y1 {
            for z in z0..z1 {
                if let Some((height, source)) = water_at(level, x, y, z)
                    && top < f64::from(y as f32 + height)
                {
                    if !source {
                        return Some(Status::UnderFlowingWater);
                    }
                    under = true;
                }
            }
        }
    }
    under.then_some(Status::UnderWater)
}

/// `getGroundFriction`.
fn ground_friction(level: &Level, bounds: &Aabb) -> f32 {
    let underside = Aabb {
        min: bounds.min,
        max: DVec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
    };
    let probe = Aabb {
        min: DVec3::new(bounds.min.x, bounds.min.y - 0.001, bounds.min.z),
        max: underside.max,
    };
    let (x0, x1) = (
        probe.min.x.floor() as i32 - 1,
        probe.max.x.ceil() as i32 + 1,
    );
    let (y0, y1) = (
        probe.min.y.floor() as i32 - 1,
        probe.max.y.ceil() as i32 + 1,
    );
    let (z0, z1) = (
        probe.min.z.floor() as i32 - 1,
        probe.max.z.ceil() as i32 + 1,
    );
    let mut friction = 0.0;
    let mut count = 0;
    for x in x0..x1 {
        for z in z0..z1 {
            let edges = i32::from(x == x0 || x == x1 - 1) + i32::from(z == z0 || z == z1 - 1);
            if edges == 2 {
                continue;
            }
            for y in y0..y1 {
                if edges > 0 && (y == y0 || y == y1 - 1) {
                    continue;
                }
                let state = physics::block_at(level, x, y, z);
                let block = pumpkin_data::Block::from_state_id(
                    pumpkin_data::BlockStateId::new(state)
                        .unwrap_or_else(|| pumpkin_data::BlockStateId::new(0).expect("air")),
                );
                if block.id == pumpkin_data::Block::LILY_PAD.id {
                    continue;
                }
                if physics::block_shapes(level, (x, y, z)).any(|shape| shape.intersects(&probe)) {
                    friction += block.slipperiness;
                    count += 1;
                }
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        friction / count as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{Chunk, Section};
    use pumpkin_data::Block;

    /// A lake: water from `y = 61` to `63` over a stone floor at 60.
    fn lake() -> Level {
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let stone = Block::STONE.default_state.id.as_u16();
        let water = Block::WATER.default_state.id.as_u16();
        for x in 0..16 {
            for z in 0..16 {
                level.set_block([x, 60, z], stone, false);
                for y in 61..=63 {
                    level.set_block([x, y, z], water, false);
                }
            }
        }
        level
    }

    fn body(y: f64) -> BoatBody {
        BoatBody {
            pos: DVec3::new(8.0, y, 8.0),
            yaw: 0.0,
            width: 1.375,
            height: 0.5625,
        }
    }

    fn run(level: &Level, body: &mut BoatBody, input: BoatInput, ticks: usize) -> BoatSim {
        let mut sim = BoatSim::default();
        for _ in 0..ticks {
            sim.tick(level, &[], body, input);
        }
        sim
    }

    #[test]
    fn a_boat_dropped_on_the_water_floats_at_its_surface() {
        let level = lake();
        let mut boat = body(66.0);
        let sim = run(&level, &mut boat, BoatInput::default(), 200);
        assert!(matches!(sim.status, Status::InWater | Status::UnderWater));
        assert!(boat.pos.y > 62.0 && boat.pos.y < 65.0, "{boat:?}");
        assert!(sim.velocity.y.abs() < 0.05, "{sim:?}");
    }

    #[test]
    fn paddling_forward_moves_it_along_where_it_faces() {
        let level = lake();
        let mut boat = body(62.5);
        let ahead = BoatInput {
            up: true,
            ..BoatInput::default()
        };
        run(&level, &mut boat, ahead, 40);
        assert!(boat.pos.z > 8.0 + 3.0, "{boat:?}");
        assert!((boat.pos.x - 8.0).abs() < 0.01, "{boat:?}");
    }

    #[test]
    fn turning_right_turns_the_yaw_up_and_drifts_the_boat() {
        let level = lake();
        let mut boat = body(62.5);
        let right = BoatInput {
            right: true,
            ..BoatInput::default()
        };
        run(&level, &mut boat, right, 20);
        assert!(boat.yaw > 10.0, "{boat:?}");
    }

    #[test]
    fn a_wall_stops_the_boat_and_its_speed() {
        let mut level = lake();
        let stone = Block::STONE.default_state.id.as_u16();
        for y in 61..=66 {
            for x in 0..16 {
                level.set_block([x, y, 11], stone, false);
            }
        }
        let mut boat = body(62.5);
        let ahead = BoatInput {
            up: true,
            ..BoatInput::default()
        };
        let sim = run(&level, &mut boat, ahead, 80);
        assert!(boat.pos.z + 1.375 / 2.0 <= 11.0 + 1.0e-6, "{boat:?}");
        assert!(sim.velocity.z.abs() < 0.05, "{sim:?}");
    }

    #[test]
    fn paddles_move_for_a_turn_or_for_ahead() {
        let key = |left, right, up| BoatInput {
            left,
            right,
            up,
            down: false,
        };
        assert_eq!(key(false, false, false).paddles(), (false, false));
        assert_eq!(key(false, true, false).paddles(), (true, false));
        assert_eq!(key(true, false, false).paddles(), (false, true));
        assert_eq!(key(false, false, true).paddles(), (true, true));
    }
}
