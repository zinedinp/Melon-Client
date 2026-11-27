//! The local player: health, effects, motion, the driven vehicle and its
//! own entity data.

use glam::DVec3;

use crate::game::{InWorld, Received};
use melon_net::play::game::{DataValue, EntityFlags, EntityPacket, FLAG_ON_FIRE, NAUSEA_ID};

impl InWorld {
    /// `handleSetHealth`; a death or a revival is reported.
    pub(super) fn set_health(
        &mut self,
        health: f32,
        food: i32,
        saturation: f32,
        received: &mut Received,
    ) {
        let was_dead = self.status.health <= 0.0;
        self.status.health = health;
        self.status.food = food;
        self.status.saturation = saturation;
        received.died |= health <= 0.0 && !was_dead;
        received.revived |= health > 0.0 && was_dead;
    }

    /// `handleUpdateMobEffect` (`Some`) and `handleRemoveMobEffect` (`None`),
    /// for the own player only: other entities show theirs through entity data.
    pub(super) fn update_mob_effect(
        &mut self,
        entity_id: i32,
        effect: i32,
        added: Option<(i32, i32, u8)>,
    ) {
        if entity_id != self.joined.session.info.entity_id {
            return;
        }
        match added {
            Some((amplifier, duration, flags)) => {
                if effect == NAUSEA_ID {
                    self.effects.set_nausea_ticks(duration);
                }
                self.mining_effects.set(effect, Some(amplifier));
                self.status_effects.set(effect, amplifier, duration, flags);
            }
            None => {
                if effect == NAUSEA_ID {
                    self.effects.set_nausea_ticks(0);
                }
                self.mining_effects.set(effect, None);
                self.status_effects.remove(effect);
            }
        }
    }

    /// `handleSetEntityMotion`: `lerpMotion`, the own player's included.
    pub(super) fn set_entity_motion(&mut self, entity_id: i32, velocity: [f64; 3]) {
        let own = self.joined.session.info.entity_id;
        if entity_id != own {
            self.entities.apply(
                EntityPacket::Motion {
                    id: entity_id,
                    velocity,
                },
                own,
            );
            return;
        }
        if melon_core::probe::enabled() {
            melon_core::probe::trace_io(
                false,
                "set_entity_motion",
                format!(
                    "own velocity=({:.5},{:.5},{:.5}) handler=game-thread lerpMotion/setDeltaMovement",
                    velocity[0], velocity[1], velocity[2]
                ),
            );
        }
        self.player.set_velocity(DVec3::from_array(velocity));
    }

    /// `handleMoveVehicle`: a vehicle driven here snaps to the server's
    /// place and answers at once with where it is now.
    pub(super) fn move_vehicle(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        if self
            .entities
            .snap_driven(DVec3::from_array(pos), yaw, pitch)
        {
            self.send_move_vehicle();
        }
    }

    /// The local player's own entity data: on fire, gliding, using an item.
    pub(in crate::game) fn own_entity_data(&mut self, values: &[(u8, DataValue)]) {
        let flags = EntityFlags::of(values);
        if let Some(shared) = flags.shared {
            self.effects.set_on_fire(shared & (1 << FLAG_ON_FIRE) != 0);
            self.player.set_gliding(shared & (1 << 7) != 0);
        }
        if let Some(living) = flags.living {
            self.sync_using_item(living);
            if living & (1 << 2) == 0 {
                self.player.end_spin_attack();
            }
        }
    }
}
