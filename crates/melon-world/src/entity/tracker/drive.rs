//! The vehicle the local player steers. Vanilla calls it local-instance
//! authoritative: the client moves it, reports it with
//! `ServerboundMoveVehiclePacket`, and the server can only snap it back
//! (`ClientboundMoveVehiclePacket`). The server's own moves of it are
//! ignored meanwhile.

use super::*;

/// The steered vehicle and the client's simulation of it.
pub(super) struct Driven {
    pub(super) id: i32,
    sim: VehicleSim,
}

/// One simulation per kind of vehicle the client can drive.
enum VehicleSim {
    Boat(boat_sim::BoatSim),
    Horse(horse_sim::HorseSim),
}

/// What the rider does to the vehicle this tick.
#[derive(Clone, Copy, Debug)]
pub enum DriveInput {
    /// `AbstractBoat.setInput`.
    Boat(boat_sim::BoatInput),
    /// `AbstractHorse.getRiddenInput`.
    Horse(horse_sim::HorseInput),
}

impl DriveInput {
    /// Whether `entity` is driven by this kind of input.
    fn fits(&self, entity: &ClientEntity) -> bool {
        match self {
            Self::Boat(_) => entity.kind == EntityKind::Boat,
            Self::Horse(_) => entity.kind.is_equine(),
        }
    }
}

/// What the server hears about the steered vehicle each tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrivenVehicle {
    pub pos: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    /// A boat's paddles (`ServerboundPaddleBoatPacket`); `None` for others.
    pub paddles: Option<(bool, bool)>,
}

impl Entities {
    /// One tick of the steered vehicle: the boat's `tick` or the horse's
    /// `travelRidden`. `None`, a vehicle the input does not fit, or one
    /// the player no longer steers hands it back to the server.
    pub fn drive(&mut self, level: &Level, input: Option<DriveInput>) {
        let steered = self.own_vehicle.filter(|id| {
            self.by_id.get(id).is_some_and(|e| {
                e.passengers.first() == Some(&self.own_id) && input.is_some_and(|i| i.fits(e))
            })
        });
        let (Some(id), Some(input)) = (steered, input) else {
            return self.release();
        };
        if self.driven.as_ref().is_some_and(|d| d.id != id) {
            self.release();
        }
        let Some(entity) = self.by_id.get(&id) else {
            return;
        };
        let (width, height) = entity.size();
        let (width, height) = (f64::from(width), f64::from(height));
        let near = Aabb::at_feet(entity.pos, width, height).deflate(-3.0);
        let colliders = self.colliders_near(&near, entity.pos.y);
        let Some(entity) = self.by_id.get_mut(&id) else {
            return;
        };
        let driven = self.driven.get_or_insert_with(|| Driven {
            id,
            sim: match input {
                DriveInput::Boat(_) => VehicleSim::Boat(boat_sim::BoatSim::new(entity.velocity)),
                DriveInput::Horse(_) => {
                    VehicleSim::Horse(horse_sim::HorseSim::new(entity.velocity))
                }
            },
        });
        match (&mut driven.sim, input) {
            (VehicleSim::Boat(sim), DriveInput::Boat(input)) => {
                drive_boat(sim, entity, level, &colliders, width, height, input);
            }
            (VehicleSim::Horse(sim), DriveInput::Horse(input)) => {
                drive_horse(sim, entity, level, &colliders, width, height, input);
            }
            // `fits` keeps the kind of a vehicle fixed while it is driven.
            _ => return self.release(),
        }
        entity.base = entity.pos;
        entity.lerp = None;
        self.position_riders();
    }

    /// The server takes the vehicle back: its moves count again, starting
    /// from where the client left it.
    fn release(&mut self) {
        if let Some(driven) = self.driven.take()
            && let Some(entity) = self.by_id.get_mut(&driven.id)
        {
            entity.base = entity.pos;
            entity.lerp = None;
        }
    }

    pub(super) fn steers_vehicle(&self, id: i32) -> bool {
        self.driven.as_ref().is_some_and(|d| d.id == id)
    }

    /// `PlayerRideableJumping.onPlayerJump` on the steered horse.
    pub fn horse_jump(&mut self, power: i32) {
        if let Some(Driven {
            sim: VehicleSim::Horse(sim),
            ..
        }) = &mut self.driven
        {
            sim.on_player_jump(power);
        }
    }

    /// The steered vehicle as `ServerboundMoveVehiclePacket.fromEntity`
    /// and the boat's paddles see it.
    pub fn driven_vehicle(&self) -> Option<DrivenVehicle> {
        let driven = self.driven.as_ref()?;
        let entity = self.by_id.get(&driven.id)?;
        let paddles = match &driven.sim {
            VehicleSim::Boat(sim) => Some(sim.paddles),
            VehicleSim::Horse(_) => None,
        };
        Some(DrivenVehicle {
            pos: entity.pos,
            yaw: entity.yaw,
            pitch: entity.pitch,
            on_ground: entity.on_ground,
            paddles,
        })
    }

    /// `ClientPacketListener.handleMoveVehicle`: the server refused a move
    /// and puts the steered vehicle back (`absSnapTo`; its speed stays).
    /// Returns whether the player steers one, which then answers with
    /// where it is now.
    pub fn snap_driven(&mut self, pos: DVec3, yaw: f32, pitch: f32) -> bool {
        let Some(id) = self.driven.as_ref().map(|d| d.id) else {
            return false;
        };
        let Some(entity) = self.by_id.get_mut(&id) else {
            return false;
        };
        if pos.distance(entity.pos) > 1.0e-5 {
            entity.pos = pos;
            entity.base = pos;
            entity.lerp = None;
            entity.yaw = yaw;
            entity.pitch = pitch;
            self.position_riders();
        }
        true
    }
}

fn drive_boat(
    sim: &mut boat_sim::BoatSim,
    entity: &mut ClientEntity,
    level: &Level,
    colliders: &[Aabb],
    width: f64,
    height: f64,
    input: boat_sim::BoatInput,
) {
    let mut body = boat_sim::BoatBody {
        pos: entity.pos,
        yaw: entity.yaw,
        width,
        height,
    };
    sim.tick(level, colliders, &mut body, input);
    entity.pos = body.pos;
    entity.yaw = body.yaw;
    entity.on_ground = sim.on_ground;
}

fn drive_horse(
    sim: &mut horse_sim::HorseSim,
    entity: &mut ClientEntity,
    level: &Level,
    colliders: &[Aabb],
    width: f64,
    height: f64,
    input: horse_sim::HorseInput,
) {
    let mut body = horse_sim::HorseBody {
        pos: entity.pos,
        yaw: entity.yaw,
        pitch: entity.pitch,
        width,
        height,
        speed: entity.movement_speed,
        jump_strength: entity.jump_strength,
        step_height: entity.step_height,
    };
    let rearing = entity.appearance.horse_rearing();
    sim.tick(level, colliders, &mut body, input, rearing);
    let moved = body.pos - entity.pos;
    let distance = (moved.x * moved.x + moved.z * moved.z).sqrt() as f32;
    entity.walk.tick(distance, entity.appearance.baby);
    entity.pos = body.pos;
    entity.yaw = body.yaw;
    entity.pitch = body.pitch;
    entity.body = body.yaw;
    entity.head = body.yaw;
    entity.on_ground = sim.on_ground;
    entity.velocity = sim.velocity;
}
