//! Riding, the part every vehicle shares.

use glam::DVec3;
use pumpkin_data::entity::{EntityType, MobCategory};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use super::kind::EntityKind;
use crate::world::player::MoveInput;

/// How a vehicle takes what its rider does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Steering {
    /// Carries the rider where it goes.
    Carried,
    /// `AbstractBoat`: the movement keys are the paddles.
    Paddles,
    /// A saddled `AbstractHorse`.
    Horse,
    /// A saddled pig or strider with its stick.
    Stick,
    /// A happy ghast or a nautilus under a saddle.
    Swimmer { climbs: bool },
}

/// Which keys have an effect, `Entity.getRiddenInput` by vehicle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sprint: bool,
}

impl Controls {
    /// No key at all.
    pub const NONE: Self = Self {
        forward: false,
        back: false,
        left: false,
        right: false,
        jump: false,
        sprint: false,
    };

    const MOVEMENT: Self = Self {
        forward: true,
        back: true,
        left: true,
        right: true,
        ..Self::NONE
    };

    /// `input` with what has no effect taken out.
    pub fn filter(self, input: MoveInput) -> MoveInput {
        let forward = match input.forward {
            f if f > 0.0 && !self.forward => 0.0,
            f if f < 0.0 && !self.back => 0.0,
            f => f,
        };
        let strafe = match input.strafe {
            s if s > 0.0 && !self.right => 0.0,
            s if s < 0.0 && !self.left => 0.0,
            s => s,
        };
        MoveInput {
            forward,
            strafe,
            jump: input.jump && self.jump,
            sprint: input.sprint && self.sprint,
            sneak: input.sneak,
        }
    }

    /// Whether any key moves the vehicle.
    pub fn moves(self) -> bool {
        self.forward || self.back || self.left || self.right || self.jump
    }
}

impl Steering {
    /// The keys that count under this steering.
    pub fn controls(self) -> Controls {
        match self {
            Self::Carried | Self::Stick => Controls::NONE,
            Self::Paddles => Controls::MOVEMENT,
            Self::Horse => Controls {
                jump: true,
                ..Controls::MOVEMENT
            },
            Self::Swimmer { climbs } => Controls {
                jump: climbs,
                ..Controls::MOVEMENT
            },
        }
    }

    /// `Entity.canSprint` of the vehicle: only a few can be sprinted on.
    pub fn can_sprint(self) -> bool {
        false
    }

    /// Whether the rider steers: every steering but [`Self::Carried`].
    pub fn is_driven(self) -> bool {
        self != Self::Carried
    }

    /// Jumping is a charge (`PlayerRideableJumping`).
    pub fn charges_jump(self) -> bool {
        self == Self::Horse
    }

    /// The stick's right click boosts (`ItemSteerable.boost`).
    pub fn boosts_with_use(self) -> bool {
        self == Self::Stick
    }
}

/// `LocalPlayer.jumpRidingTicks` and `jumpRidingScale`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JumpCharge {
    ticks: i32,
    scale: f32,
    held: bool,
}

impl JumpCharge {
    /// How full the jump bar is, 0 to 1 (`getJumpRidingScale`).
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Once a tick, with the jump key now.
    pub fn tick(&mut self, jump: bool, jumps: bool) -> Option<i32> {
        if !jumps {
            self.scale = 0.0;
            self.held = jump;
            return None;
        }
        let mut sent = None;
        if self.ticks < 0 {
            self.ticks += 1;
            if self.ticks == 0 {
                self.scale = 0.0;
            }
        }
        if self.held && !jump {
            self.ticks = -10;
            sent = Some((self.scale * 100.0).floor() as i32);
        } else if !self.held && jump {
            self.ticks = 0;
            self.scale = 0.0;
        } else if self.held {
            self.ticks += 1;
            self.scale = if self.ticks < 10 {
                self.ticks as f32 * 0.1
            } else {
                0.8 + 2.0 / (self.ticks - 9) as f32 * 0.1
            };
        }
        self.held = jump;
        sent
    }
}

/// What the client can tell of a vehicle's saddle and gear, and of what the rider holds.
#[derive(Clone, Copy)]
pub struct Gear<'a> {
    /// Something in the saddle slot.
    pub saddled: bool,
    /// Something in the body slot (a happy ghast's harness).
    pub harnessed: bool,
    /// The rider's hands.
    pub main_hand: &'a ItemStack,
    pub off_hand: &'a ItemStack,
    /// The rider is the local player: only players steer anything.
    pub rider_is_player: bool,
}

impl Gear<'_> {
    /// `LivingEntity.isHolding`: in either hand.
    fn holds(&self, item: &Item) -> bool {
        [self.main_hand, self.off_hand]
            .iter()
            .any(|stack| !stack.is_empty() && stack.item.id == item.id)
    }
}

/// How `kind` takes its first passenger, by `getControllingPassenger`.
pub fn steering(kind: EntityKind, ty: &EntityType, gear: Gear) -> Steering {
    if ty.boat.is_some() {
        return if gear.rider_is_player {
            Steering::Paddles
        } else {
            Steering::Carried
        };
    }
    if !gear.rider_is_player {
        return Steering::Carried;
    }
    match kind {
        k if k.is_equine() && gear.saddled => Steering::Horse,
        EntityKind::Pig if gear.saddled && gear.holds(&Item::CARROT_ON_A_STICK) => Steering::Stick,
        EntityKind::HappyGhast if gear.harnessed => Steering::Swimmer { climbs: true },
        EntityKind::Nautilus if gear.saddled => Steering::Swimmer { climbs: false },
        _ => Steering::Carried,
    }
}

/// The vehicle, as far as its seats are concerned.
#[derive(Clone, Copy, Debug)]
pub struct Vehicle<'a> {
    pub ty: &'a EntityType,
    pub baby: bool,
    /// Its yaw, vanilla degrees (0 faces south).
    pub yaw: f32,
    /// `getScale() * getAgeScale()`.
    pub scale: f32,
    /// A horse's `standAnimO`: how far it has reared, 0 to 1.
    pub rearing: f32,
}

/// What a rider adds: how it sits on a vehicle.
#[derive(Clone, Copy, Debug)]
pub struct Rider<'a> {
    pub ty: &'a EntityType,
    pub kind: EntityKind,
}

/// `Vec3.yRot` by `-yaw`.
pub fn turned(point: DVec3, yaw: f32) -> DVec3 {
    let angle = -f64::from(yaw).to_radians();
    let (sin, cos) = angle.sin_cos();
    DVec3::new(
        point.x * cos + point.z * sin,
        point.y,
        point.z * cos - point.x * sin,
    )
}

/// `Entity.getPassengerAttachmentPoint`: where passenger `index` of `count`
pub fn seat(vehicle: Vehicle, index: usize, count: usize, rider: Rider) -> DVec3 {
    let scale = f64::from(vehicle.scale);
    if let Some(boat) = &vehicle.ty.boat {
        let mut offset = f64::from(boat.single_passenger_x_offset);
        if count > 1 {
            offset = if index == 0 { 0.2 } else { -0.6 };
            if is_animal(rider.ty) {
                offset += 0.2;
            }
        }
        return turned(DVec3::new(0.0, boat.ride_height, offset), vehicle.yaw);
    }
    let attachments = match (&vehicle.ty.baby_attachments, vehicle.baby) {
        (Some(baby), true) => baby,
        _ => &vehicle.ty.attachments,
    };
    let points = attachments.passenger;
    let point = points
        .get(index.min(points.len().saturating_sub(1)))
        .copied()
        .unwrap_or([0.0; 3]);
    let mut at = turned(DVec3::from_array(point), vehicle.yaw);
    if vehicle.ty.resource_name.ends_with("minecart")
        && matches!(
            rider.kind,
            EntityKind::Villager | EntityKind::WanderingTrader
        )
    {
        at = DVec3::ZERO;
    }
    if vehicle.rearing > 0.0 {
        at += turned(
            DVec3::new(
                0.0,
                0.15 * f64::from(vehicle.rearing) * scale,
                -0.7 * f64::from(vehicle.rearing) * scale,
            ),
            vehicle.yaw,
        );
    }
    at
}

/// `Animal`s take a boat's seat a little further forward.
fn is_animal(ty: &EntityType) -> bool {
    *ty.category == MobCategory::CREATURE
}

/// Where a rider's feet go.
pub fn rider_feet(vehicle_pos: DVec3, seat: DVec3, rider: &EntityType) -> DVec3 {
    let [x, y, z] = rider.attachments.vehicle;
    vehicle_pos + seat - DVec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gear<'a>(saddled: bool, hand: &'a ItemStack) -> Gear<'a> {
        Gear {
            saddled,
            harnessed: false,
            main_hand: hand,
            off_hand: ItemStack::EMPTY,
            rider_is_player: true,
        }
    }

    #[test]
    fn a_horse_is_steered_only_under_a_saddle() {
        let ty = &EntityType::HORSE;
        assert_eq!(
            steering(EntityKind::Horse, ty, gear(true, ItemStack::EMPTY)),
            Steering::Horse
        );
        assert_eq!(
            steering(EntityKind::Horse, ty, gear(false, ItemStack::EMPTY)),
            Steering::Carried
        );
    }

    #[test]
    fn a_pig_wants_a_saddle_and_the_stick() {
        let stick = ItemStack::new(1, &Item::CARROT_ON_A_STICK);
        let ty = &EntityType::PIG;
        assert_eq!(
            steering(EntityKind::Pig, ty, gear(true, &stick)),
            Steering::Stick
        );
        assert_eq!(
            steering(EntityKind::Pig, ty, gear(true, ItemStack::EMPTY)),
            Steering::Carried
        );
        assert_eq!(
            steering(EntityKind::Pig, ty, gear(false, &stick)),
            Steering::Carried
        );
    }

    #[test]
    fn a_boat_takes_a_player_in_front_and_nobody_else() {
        let ty = &EntityType::OAK_BOAT;
        assert_eq!(
            steering(EntityKind::Other, ty, gear(false, ItemStack::EMPTY)),
            Steering::Paddles
        );
        let mut mob = gear(false, ItemStack::EMPTY);
        mob.rider_is_player = false;
        assert_eq!(steering(EntityKind::Other, ty, mob), Steering::Carried);
    }

    #[test]
    fn the_keys_that_count_depend_on_the_vehicle() {
        let walk = MoveInput {
            forward: 1.0,
            strafe: -1.0,
            jump: true,
            sprint: true,
            sneak: true,
        };
        let carried = Steering::Carried.controls().filter(walk);
        assert_eq!((carried.forward, carried.strafe), (0.0, 0.0));
        assert!(!carried.jump && !carried.sprint && carried.sneak);
        let boat = Steering::Paddles.controls().filter(walk);
        assert_eq!((boat.forward, boat.strafe), (1.0, -1.0));
        assert!(!boat.jump && !boat.sprint);
        assert!(Steering::Horse.controls().filter(walk).jump);
        assert!(!Steering::Stick.controls().moves());
        assert!(!Steering::Swimmer { climbs: false }.controls().jump);
        assert!(Steering::Swimmer { climbs: true }.controls().jump);
    }

    #[test]
    fn a_held_jump_charges_and_letting_go_sends_it() {
        let mut jump = JumpCharge::default();
        assert_eq!(jump.tick(true, true), None);
        for _ in 0..9 {
            assert_eq!(jump.tick(true, true), None);
        }
        assert!((jump.scale() - 0.9).abs() < 1e-6, "{}", jump.scale());
        assert_eq!(jump.tick(false, true), Some(90));
        let mut long = JumpCharge::default();
        for _ in 0..200 {
            long.tick(true, true);
        }
        assert!(long.scale() < 1.0 && long.scale() > 0.8);
    }

    #[test]
    fn a_tap_sends_nothing_much_and_a_vehicle_that_cannot_jump_sends_nothing() {
        let mut jump = JumpCharge::default();
        jump.tick(true, true);
        assert_eq!(jump.tick(false, true), Some(0));
        let mut boat = JumpCharge::default();
        boat.tick(true, false);
        assert_eq!(boat.tick(false, false), None);
        assert_eq!(boat.scale(), 0.0);
    }

    #[test]
    fn a_missing_key_is_not_taken_from_the_other_direction() {
        let controls = Controls {
            forward: true,
            ..Controls::NONE
        };
        let back = MoveInput {
            forward: -1.0,
            ..Default::default()
        };
        assert_eq!(controls.filter(back).forward, 0.0);
        let ahead = MoveInput {
            forward: 1.0,
            ..Default::default()
        };
        assert_eq!(controls.filter(ahead).forward, 1.0);
    }

    fn vehicle(ty: &EntityType, yaw: f32) -> Vehicle<'_> {
        Vehicle {
            ty,
            baby: false,
            yaw,
            scale: 1.0,
            rearing: 0.0,
        }
    }

    const PLAYER: Rider<'static> = Rider {
        ty: &EntityType::PLAYER,
        kind: EntityKind::Player,
    };

    #[test]
    fn a_rider_sits_where_the_vehicles_data_says() {
        for yaw in [0.0, 90.0, 217.0] {
            let at = seat(vehicle(&EntityType::HORSE, yaw), 0, 1, PLAYER);
            assert!((at.y - 1.44375).abs() < 1e-6 && at.x.abs() < 1e-9 && at.z.abs() < 1e-9);
        }
        let feet = rider_feet(
            DVec3::new(10.0, 64.0, 5.0),
            DVec3::new(0.0, 1.44375, 0.0),
            &EntityType::PLAYER,
        );
        assert!((feet.y - (64.0 + 1.44375 - 0.6)).abs() < 1e-6);
        assert_eq!((feet.x, feet.z), (10.0, 5.0));
    }

    #[test]
    fn two_in_a_boat_sit_front_and_back_and_turn_with_it() {
        let ty = &EntityType::OAK_BOAT;
        let front = seat(vehicle(ty, 0.0), 0, 2, PLAYER);
        let back = seat(vehicle(ty, 0.0), 1, 2, PLAYER);
        assert!((front.z - 0.2).abs() < 1e-9 && (back.z + 0.6).abs() < 1e-9);
        assert!((front.y - 0.1875).abs() < 1e-9);
        let west = seat(vehicle(ty, 90.0), 0, 2, PLAYER);
        assert!((west.x + 0.2).abs() < 1e-9 && west.z.abs() < 1e-9);
        let alone = seat(vehicle(ty, 0.0), 0, 1, PLAYER);
        assert_eq!(
            alone.z,
            f64::from(ty.boat.unwrap().single_passenger_x_offset)
        );
    }

    #[test]
    fn a_rearing_horse_leans_its_rider_back() {
        let mut v = vehicle(&EntityType::HORSE, 0.0);
        let level = seat(v, 0, 1, PLAYER);
        v.rearing = 1.0;
        let up = seat(v, 0, 1, PLAYER);
        assert!((up.y - level.y - 0.15).abs() < 1e-9);
        assert!((up.z - level.z + 0.7).abs() < 1e-9);
    }

    #[test]
    fn a_baby_has_its_own_seat() {
        let mut v = vehicle(&EntityType::PIG, 0.0);
        let adult = seat(v, 0, 1, PLAYER).y;
        v.baby = true;
        assert!(seat(v, 0, 1, PLAYER).y < adult);
    }

    #[test]
    fn a_villager_sits_low_in_a_minecart() {
        let villager = Rider {
            ty: &EntityType::VILLAGER,
            kind: EntityKind::Villager,
        };
        let cart = vehicle(&EntityType::MINECART, 0.0);
        assert_eq!(seat(cart, 0, 1, villager), DVec3::ZERO);
        assert!(seat(cart, 0, 1, PLAYER).y > 0.0);
    }
}
