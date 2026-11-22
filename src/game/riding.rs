//! Riding pipe: collect how the vehicle steers, drive it, seat the rider,
//! then tell the server.

use super::{InWorld, melon_yaw, minecraft_yaw};
use crate::network::play::game::PlayerCommand;
use crate::world::entity::boat_sim::BoatInput;
use crate::world::entity::horse_sim::HorseInput;
use crate::world::entity::kind::EntityKind;
use crate::world::entity::ride::{JumpCharge, Steering};
use crate::world::entity::tracker::DriveInput;
use crate::world::inventory::OFFHAND_SLOT;
use crate::world::player::MoveInput;

/// What riding keeps between ticks.
#[derive(Default)]
pub struct RideState {
    pub jump: JumpCharge,
}

impl InWorld {
    /// How the vehicle takes the player's keys; `None` on foot.
    pub(super) fn steering(&self) -> Option<Steering> {
        let main = self.inventory.selected_stack();
        let off = &self.inventory.slots()[OFFHAND_SLOT];
        self.entities.own_steering(main, off)
    }

    /// Whether the inventory key opens the mount's inventory instead of the player's.
    pub(super) fn rides_with_inventory(&self) -> bool {
        self.entities
            .own_vehicle_kind()
            .is_some_and(|kind| kind.is_equine() || kind == EntityKind::Nautilus)
    }

    /// `Keybinds` while riding.
    pub(super) fn open_mount_inventory(&self) {
        self.joined
            .session
            .send_player_command(PlayerCommand::OpenInventory);
    }

    /// One riding tick, after the entities have moved: steer the vehicle,
    /// seat the rider on it, then the jump charge and the paddles.
    pub(super) fn tick_ride(&mut self, input: MoveInput) {
        let steering = self.steering();
        self.drive_vehicle(input, steering);
        let riding = self.seat_on_vehicle(steering);
        self.send_ride_packets(input, riding);
    }

    /// The vehicle's own motion from the rider's keys: a boat's paddles or
    /// a horse's walk. Any other steering hands the vehicle back to the
    /// server.
    fn drive_vehicle(&mut self, input: MoveInput, steering: Option<Steering>) {
        let drive = steering.and_then(|steering| {
            let keys = steering.controls().filter(input);
            match steering {
                Steering::Paddles => Some(DriveInput::Boat(BoatInput {
                    left: keys.strafe < 0.0,
                    right: keys.strafe > 0.0,
                    up: keys.forward > 0.0,
                    down: keys.forward < 0.0,
                })),
                Steering::Horse => Some(DriveInput::Horse(HorseInput::from_keys(
                    keys.forward,
                    keys.strafe,
                    minecraft_yaw(self.player.yaw()),
                    self.player.pitch() * 0.5,
                ))),
                _ => None,
            }
        });
        self.entities.drive(&self.level, drive);
    }

    /// The player stays on its seat (`Entity.positionRider`). `None` when
    /// it rides nothing.
    fn seat_on_vehicle(&mut self, steering: Option<Steering>) -> Option<Steering> {
        let (steering, seat) = match (steering, self.entities.own_seat()) {
            (Some(steering), Some(seat)) => (steering, seat),
            _ => {
                if self.player.riding() {
                    self.player.dismount();
                }
                self.ride.jump.tick(false, false);
                self.status.jump_bar = None;
                return None;
            }
        };
        let body = self
            .entities
            .own_vehicle_body()
            .map(|(before, now)| (melon_yaw(before), melon_yaw(now)));
        self.player.ride_tick(seat, body);
        Some(steering)
    }

    /// Jump charge and paddle packets, then the HUD's jump bar.
    fn send_ride_packets(&mut self, input: MoveInput, steering: Option<Steering>) {
        let Some(steering) = steering else {
            return;
        };
        let keys = steering.controls().filter(input);
        if let Some(strength) = self.ride.jump.tick(keys.jump, steering.charges_jump()) {
            self.entities.horse_jump(strength);
            self.joined
                .session
                .send_player_command_with(PlayerCommand::StartRidingJump, strength);
        }
        // `AbstractBoat.tick` sends the paddles every tick it steers the
        // boat, ahead of the move; a server reads them as the rowing.
        if let Some((left, right)) = self.entities.driven_vehicle().and_then(|v| v.paddles) {
            self.joined.session.send_paddle_boat(left, right);
        }
        self.status.jump_bar = self.jump_bar();
    }

    /// The mount, if it is a horse-like one, for the HUD's jump bar.
    pub(super) fn jump_bar(&self) -> Option<f32> {
        let kind: EntityKind = self.entities.own_vehicle_kind()?;
        (kind.is_equine() && self.steering() == Some(Steering::Horse))
            .then(|| self.ride.jump.scale())
    }
}
