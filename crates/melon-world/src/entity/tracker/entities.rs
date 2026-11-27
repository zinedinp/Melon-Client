//! [`Entities`]: every tracked entity by id, and the packets that change them.

use super::*;
use std::collections::HashSet;

/// Every entity the server tracks for this client but the player itself.
#[derive(Default)]
pub struct Entities {
    /// Entity ids are small server counters: no need for SipHash.
    pub(super) by_id: HashMap<i32, ClientEntity, crate::math::ChunkHash>,
    /// Some entity was renamed since [`Self::resolve_names`].
    names_pending: bool,
    /// The dimension turns piglins into zombified ones.
    piglins_zombify: bool,
    /// The vehicle the local player rides, and the local player's id.
    pub(super) own_vehicle: Option<i32>,
    pub(super) own_id: i32,
    /// The vehicle the local player steers, and its simulation.
    pub(super) driven: Option<super::drive::Driven>,
    /// Passenger lists whose vehicle has not spawned yet.
    pending_passengers: HashMap<i32, Vec<i32>, crate::math::ChunkHash>,
    /// Items flying into hands, and the sounds of them being taken.
    pub(super) pickups: Vec<super::pickup::Pickup>,
    pub(super) pickup_sounds: Vec<super::pickup::PickupSound>,
    /// Sounds entities made themselves (`Level.playLocalSound`).
    own_sounds: Vec<OwnSound>,
    /// `TickRateManager.isEntityFrozen` as of the last tick: the entities a
    /// frozen level holds still.
    frozen: HashSet<i32>,
}

/// A sound an entity played itself, where it stood.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnSound {
    pub event: &'static str,
    pub at: DVec3,
}

/// A rider's id, its new seat and the body turn it follows.
type RiderMove = (i32, DVec3, Option<(f32, f32)>);

impl Entities {
    pub fn set_piglins_zombify(&mut self, zombify: bool) {
        self.piglins_zombify = zombify;
    }

    /// Takes in a packet; `own_id`'s are the local player's, not ours.
    pub fn apply(&mut self, packet: EntityPacket, own_id: i32) {
        self.own_id = own_id;
        match packet {
            EntityPacket::Add(add) => {
                if add.id == own_id {
                    return;
                }
                if let Some(entity) = ClientEntity::spawn(&add) {
                    self.by_id.insert(add.id, entity);
                    self.bind_new_entity(add.id);
                }
            }
            EntityPacket::Remove(ids) => {
                for id in ids {
                    self.remove(id);
                }
            }
            EntityPacket::Passengers { id, passengers } => self.set_passengers(id, passengers),
            EntityPacket::Take {
                item,
                taker,
                amount,
            } => self.take_item(item, taker, amount),
            EntityPacket::Move(m) if self.steers_vehicle(m.id) => {
                if let Some(entity) = self.by_id.get_mut(&m.id) {
                    entity.track_codec_move(&m);
                }
            }
            EntityPacket::SetPosition(p) if self.steers_vehicle(p.id) => {
                if let Some(entity) = self.by_id.get_mut(&p.id) {
                    entity.track_codec_position(&p);
                }
            }
            packet => {
                if let EntityPacket::Data { values, .. } = &packet {
                    self.names_pending |= values
                        .iter()
                        .any(|(i, v)| *i == appearance::CUSTOM_NAME_INDEX && v.name().is_some());
                }
                if let Some(entity) = packet_id(&packet).and_then(|id| self.by_id.get_mut(&id)) {
                    entity.apply(packet);
                }
            }
        }
    }

    /// Every named entity's name is looked at again by the next [`Self::resolve_names`].
    pub fn rename_all(&mut self) {
        for entity in self.by_id.values_mut() {
            entity.name_pending = entity.custom_name.is_some();
        }
        self.names_pending = true;
    }

    /// [`ClientEntity::resolve_name`] for every entity renamed since the last call.
    pub fn resolve_names(&mut self, text_of: &impl Fn(&Component) -> String) {
        if !std::mem::take(&mut self.names_pending) {
            return;
        }
        for entity in self.by_id.values_mut() {
            entity.resolve_name(text_of);
        }
    }

    /// `ClientLevel.tickEntities`. While the level does not run normally
    /// only players and what carries a player tick.
    pub fn tick(&mut self, runs_normally: bool) {
        self.frozen = if runs_normally {
            HashSet::new()
        } else {
            self.frozen_ids()
        };
        let zombify = self.piglins_zombify;
        let frozen = &self.frozen;
        for entity in self.by_id.values_mut() {
            if frozen.contains(&entity.id) {
                continue;
            }
            if matches!(entity.kind, EntityKind::Piglin | EntityKind::PiglinBrute) {
                entity.appearance.zombifying = entity.appearance.piglin_converting(zombify);
            }
            entity.tick();
            for event in entity.sounds.drain(..) {
                self.own_sounds.push(OwnSound {
                    event,
                    at: entity.pos,
                });
            }
        }
        self.position_riders();
        if runs_normally {
            self.check_crystals();
        }
    }

    /// The sounds entities made themselves since the last call.
    pub fn take_own_sounds(&mut self) -> Vec<OwnSound> {
        std::mem::take(&mut self.own_sounds)
    }

    /// Whether the level holds `id` still: it is drawn where it is, as
    /// vanilla draws it with a partial tick of 1.
    pub fn is_frozen(&self, id: i32) -> bool {
        self.frozen.contains(&id)
    }

    /// `TickRateManager.isEntityFrozen` for each entity: by the root of
    /// what it rides (passengers tick with their vehicle), frozen unless
    /// that is a player or carries one, the local player among them.
    fn frozen_ids(&self) -> HashSet<i32> {
        let carries_player = |root: i32| {
            let mut stack = vec![root];
            let mut seen = 0;
            while let Some(id) = stack.pop() {
                seen += 1;
                if seen > 64 {
                    break;
                }
                let Some(entity) = self.by_id.get(&id) else {
                    continue;
                };
                for &rider in &entity.passengers {
                    let is_player = rider == self.own_id
                        || self
                            .by_id
                            .get(&rider)
                            .is_some_and(|e| e.kind == EntityKind::Player);
                    if is_player {
                        return true;
                    }
                    stack.push(rider);
                }
            }
            false
        };
        let root_of = |mut id: i32| {
            for _ in 0..16 {
                match self.by_id.get(&id).and_then(|e| e.vehicle) {
                    Some(vehicle) if self.by_id.contains_key(&vehicle) => id = vehicle,
                    _ => break,
                }
            }
            id
        };
        self.by_id
            .keys()
            .copied()
            .filter(|&id| {
                let root = root_of(id);
                let is_player = self
                    .by_id
                    .get(&root)
                    .is_some_and(|e| e.kind == EntityKind::Player);
                !is_player && !carries_player(root)
            })
            .collect()
    }

    /// `EnderDragon.checkCrystals` as the client runs it: the crystal it
    /// heals from is forgotten once gone, and one tick in ten (vanilla's
    /// `random.nextInt(10) == 0`) it looks again for the nearest within 32
    /// blocks of its box.
    fn check_crystals(&mut self) {
        let crystals: Vec<(i32, DVec3)> = self
            .by_id
            .values()
            .filter(|e| e.kind == EntityKind::EndCrystal)
            .map(|e| (e.id, e.pos))
            .collect();
        for dragon in self.by_id.values_mut() {
            if dragon.kind != EntityKind::EnderDragon || dragon.appearance.dying() {
                continue;
            }
            if let Some(id) = dragon.nearest_crystal
                && !crystals.iter().any(|&(c, _)| c == id)
            {
                dragon.nearest_crystal = None;
            }
            // A stand-in for the dragon's random: well spread over ticks
            // and dragons.
            let roll = (u64::from(dragon.age) ^ (dragon.id as u64).rotate_left(32))
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                >> 32;
            if !roll.is_multiple_of(10) {
                continue;
            }
            let (min, max) = dragon.bounds(0.0);
            let (min, max) = (min - DVec3::splat(32.0), max + DVec3::splat(32.0));
            // An end crystal's box is two blocks wide and high.
            let near = |pos: DVec3| {
                pos.x + 1.0 > min.x
                    && pos.x - 1.0 < max.x
                    && pos.y + 2.0 > min.y
                    && pos.y < max.y
                    && pos.z + 1.0 > min.z
                    && pos.z - 1.0 < max.z
            };
            dragon.nearest_crystal = crystals
                .iter()
                .filter(|&&(_, pos)| near(pos))
                .min_by(|a, b| {
                    a.1.distance_squared(dragon.pos)
                        .total_cmp(&b.1.distance_squared(dragon.pos))
                })
                .map(|&(id, _)| id);
        }
    }

    /// The physics of the entities that run their own.
    pub fn simulate(&mut self, level: &Level) {
        let frozen = &self.frozen;
        self.by_id.retain(|id, entity| {
            frozen.contains(id) || !(entity.simulates() && entity.simulate(level))
        });
    }

    /// Which swimmers are in water.
    pub fn update_water(&mut self, level: &crate::level::Level) {
        for entity in self.by_id.values_mut() {
            if entity.kind == EntityKind::Boat {
                let eye = entity.pos + DVec3::new(0.0, f64::from(entity.ty.eye_height), 0.0);
                entity.in_water = crate::physics::fluid_at(
                    level,
                    eye.x.floor() as i32,
                    eye.y.floor() as i32,
                    eye.z.floor() as i32,
                ) == Some(crate::fluid::FluidKind::Water);
            } else if entity.kind.is_aquatic()
                || (matches!(
                    entity.kind,
                    EntityKind::Player | EntityKind::Mannequin | EntityKind::Drowned
                ) && (entity.swim_amount > 0.0 || entity.appearance.visually_swimming()))
            {
                entity.in_water = crate::physics::touches_fluid(level, &entity.water_box())
                    == Some(crate::fluid::FluidKind::Water);
            } else {
                entity.in_water = false;
            }
        }
    }

    /// Which wolves are angry at `game_time` (`NeutralMob.isAngry`).
    pub fn update_anger(&mut self, game_time: i64) {
        for entity in self.by_id.values_mut() {
            if entity.kind == EntityKind::Wolf {
                entity.appearance.angry = entity.appearance.anger_end > game_time;
            }
        }
    }

    /// Forgets everything: a new level, or leaving.
    pub fn clear(&mut self) {
        self.by_id.clear();
        self.frozen.clear();
        self.own_vehicle = None;
        self.driven = None;
        self.pending_passengers.clear();
    }

    /// A newly spawned entity.
    fn bind_new_entity(&mut self, id: i32) {
        if let Some(passengers) = self.pending_passengers.remove(&id) {
            self.set_passengers(id, passengers);
        }
        self.attach_if_listed(id);
    }

    fn attach_if_listed(&mut self, rider: i32) {
        let Some(vehicle) = self
            .by_id
            .iter()
            .find_map(|(id, e)| e.passengers.contains(&rider).then_some(*id))
        else {
            return;
        };
        if rider == self.own_id {
            self.own_vehicle = Some(vehicle);
        } else if let Some(entity) = self.by_id.get_mut(&rider) {
            entity.vehicle = Some(vehicle);
            entity.lerp = None;
        }
    }

    /// `removeEntity`: gone, and nobody rides it or sits on it any more.
    pub(super) fn remove(&mut self, id: i32) {
        self.pending_passengers.remove(&id);
        if self.steers_vehicle(id) {
            self.driven = None;
        }
        let Some(gone) = self.by_id.remove(&id) else {
            return;
        };
        for rider in gone.passengers {
            if rider == self.own_id {
                self.own_vehicle = None;
            } else if let Some(entity) = self.by_id.get_mut(&rider) {
                entity.vehicle = None;
                entity.vehicle_body = None;
            }
        }
        if let Some(vehicle) = gone.vehicle.and_then(|v| self.by_id.get_mut(&v)) {
            vehicle.passengers.retain(|p| *p != id);
        }
    }

    /// `handleSetEntityPassengersPacket`: `passengers` now ride `vehicle`;
    fn set_passengers(&mut self, vehicle: i32, passengers: Vec<i32>) {
        let Some(mount) = self.by_id.get_mut(&vehicle) else {
            self.pending_passengers.insert(vehicle, passengers);
            return;
        };
        let before = std::mem::replace(&mut mount.passengers, passengers.clone());
        for gone in before.into_iter().filter(|p| !passengers.contains(p)) {
            if gone == self.own_id {
                self.own_vehicle = None;
            } else if let Some(entity) = self.by_id.get_mut(&gone) {
                entity.vehicle = None;
                entity.vehicle_body = None;
            }
        }
        for rider in passengers {
            if rider == self.own_id {
                self.own_vehicle = Some(vehicle);
            } else if let Some(entity) = self.by_id.get_mut(&rider) {
                entity.vehicle = Some(vehicle);
                entity.lerp = None;
            }
        }
    }

    /// The vehicle the local player rides.
    pub fn colliders_near(&self, near: &Aabb, feet: f64) -> Vec<Aabb> {
        let mut out = Vec::new();
        self.colliders_near_into(near, feet, &mut out);
        out
    }

    /// [`Self::colliders_near`] into a buffer that is emptied first.
    pub fn colliders_near_into(&self, near: &Aabb, feet: f64, out: &mut Vec<Aabb>) {
        let own = self.own_vehicle;
        out.clear();
        out.extend(
            self.by_id
                .values()
                .filter(|e| Some(e.id) != own && (own.is_none() || e.vehicle != own))
                .filter_map(|e| e.collider(feet))
                .filter(|b| b.intersects(near)),
        );
    }

    /// The local player's entity id.
    pub fn own_id(&self) -> i32 {
        self.own_id
    }

    /// Whether a living entity other than the player (or what they ride) is in `area`.
    pub fn any_living_in(&self, area: &Aabb) -> bool {
        let own = self.own_vehicle;
        self.by_id.values().any(|e| {
            e.id != self.own_id
                && Some(e.id) != own
                && e.is_living()
                && e.appearance.health > 0.0
                && {
                    let (w, h) = e.size();
                    Aabb::at_feet(e.pos, f64::from(w), f64::from(h)).intersects(area)
                }
        })
    }

    #[cfg(test)]
    pub(super) fn set_own_vehicle_for_test(&mut self, vehicle: Option<i32>) {
        self.own_vehicle = vehicle;
    }

    pub fn own_vehicle(&self) -> Option<i32> {
        self.own_vehicle
    }

    /// Where `rider` sits on `vehicle`, as a [`ride::Vehicle`] and its seat.
    fn seat_of(
        &self,
        vehicle: &ClientEntity,
        rider: i32,
        rider_ty: &EntityType,
        kind: EntityKind,
    ) -> DVec3 {
        let seat = Self::seat_point(vehicle, rider, rider_ty, kind);
        ride::rider_feet(vehicle.pos, seat, rider_ty)
    }

    /// `Entity.getPassengerAttachmentPoint`: the seat of `rider` on
    /// `vehicle`, relative to the vehicle's position.
    fn seat_point(
        vehicle: &ClientEntity,
        rider: i32,
        rider_ty: &EntityType,
        kind: EntityKind,
    ) -> DVec3 {
        let index = vehicle
            .passengers
            .iter()
            .position(|p| *p == rider)
            .unwrap_or(0);
        ride::seat(
            vehicle.as_vehicle(),
            index,
            vehicle.passengers.len(),
            ride::Rider { ty: rider_ty, kind },
        )
    }

    /// `Entity.getPassengerRidingPosition`: where the entity `rider` sits
    /// this tick (its vehicle's position plus the seat), and the vehicle.
    pub fn riding_position(&self, rider: i32) -> Option<(DVec3, &ClientEntity)> {
        let entity = self.by_id.get(&rider)?;
        let vehicle = self.by_id.get(&entity.vehicle?)?;
        let seat = Self::seat_point(vehicle, rider, entity.ty, entity.kind);
        Some((vehicle.pos + seat, vehicle))
    }

    /// Where the local player's feet go while it rides.
    pub fn own_seat(&self) -> Option<DVec3> {
        let vehicle = self.by_id.get(&self.own_vehicle?)?;
        Some(self.seat_of(
            vehicle,
            self.own_id,
            &EntityType::PLAYER,
            EntityKind::Player,
        ))
    }

    /// The local player's vehicle and what it makes of the player's held items.
    pub fn own_steering(&self, main: &ItemStack, off: &ItemStack) -> Option<ride::Steering> {
        let vehicle = self.by_id.get(&self.own_vehicle?)?;
        if vehicle.passengers.first() != Some(&self.own_id) {
            return Some(ride::Steering::Carried);
        }
        let gear = ride::Gear {
            saddled: vehicle.equipment.saddled,
            harnessed: !vehicle.equipment.get(EquipmentSlot::Body).is_empty(),
            main_hand: main,
            off_hand: off,
            rider_is_player: true,
        };
        Some(ride::steering(vehicle.kind, vehicle.ty, gear))
    }

    /// The mount's hearts.
    pub fn own_mount_health(&self) -> Option<(f32, f32)> {
        let vehicle = self.by_id.get(&self.own_vehicle?)?;
        vehicle
            .is_living()
            .then_some((vehicle.appearance.health, vehicle.max_health))
    }

    /// The body turn of the local player's vehicle, last tick's and this tick's.
    pub fn own_vehicle_body(&self) -> Option<(f32, f32)> {
        self.by_id.get(&self.own_vehicle?)?.body_turn()
    }

    /// The kind of the local player's vehicle.
    pub fn own_vehicle_kind(&self) -> Option<EntityKind> {
        Some(self.by_id.get(&self.own_vehicle?)?.kind)
    }

    /// `Entity.rideTick`'s second half, for every tracked rider.
    pub(super) fn position_riders(&mut self) {
        for _ in 0..3 {
            let moves: Vec<RiderMove> = self
                .by_id
                .values()
                .filter_map(|rider| {
                    let vehicle = self.by_id.get(&rider.vehicle?)?;
                    let seat = self.seat_of(vehicle, rider.id, rider.ty, rider.kind);
                    let body = vehicle.body_turn();
                    Some((rider.id, seat, body))
                })
                .collect();
            if moves.is_empty() {
                return;
            }
            for (id, feet, body) in moves {
                if let Some(rider) = self.by_id.get_mut(&id) {
                    rider.pos = feet;
                    rider.vehicle_body = body;
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn get(&self, id: i32) -> Option<&ClientEntity> {
        self.by_id.get(&id)
    }

    /// `melon:movement`'s `Started`: entity `id` began the mod state `state`.
    pub fn start_custom_move(&mut self, id: i32, state: String) {
        if let Some(entity) = self.by_id.get_mut(&id) {
            entity.custom_move = Some((state, 0));
        }
    }

    /// Whether a bobber of `owner`'s is out (`Player.fishing != null`).
    pub fn is_fishing(&self, owner: i32) -> bool {
        self.by_id
            .values()
            .any(|e| e.kind == EntityKind::FishingBobber && e.owner == Some(owner))
    }

    pub fn iter(&self) -> impl Iterator<Item = &ClientEntity> {
        self.by_id.values()
    }

    /// Whether `holder` holds anyone's lead.
    pub fn holds_leash(&self, holder: i32) -> bool {
        self.by_id.values().any(|e| e.leash_holder == Some(holder))
    }
}

fn packet_id(packet: &EntityPacket) -> Option<i32> {
    Some(match packet {
        EntityPacket::Move(m) => m.id,
        EntityPacket::SetPosition(p) => p.id,
        EntityPacket::Motion { id, .. } => *id,
        EntityPacket::HeadRotation { id, .. }
        | EntityPacket::Data { id, .. }
        | EntityPacket::Equipment { id, .. }
        | EntityPacket::Hurt { id, .. }
        | EntityPacket::Damage { id, .. }
        | EntityPacket::Swing { id, .. }
        | EntityPacket::Event { id, .. }
        | EntityPacket::Attributes { id, .. }
        | EntityPacket::Leash { id, .. } => *id,
        EntityPacket::Add(_)
        | EntityPacket::Remove(_)
        | EntityPacket::Take { .. }
        | EntityPacket::Passengers { .. } => {
            return None;
        }
    })
}
