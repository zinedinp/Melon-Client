//! The entities the server tracks for this client (vanilla `ClientLevel`'s entities).

use std::collections::HashMap;

use crate::component::Component;
use glam::{DVec3, Vec3};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item_stack::ItemStack;

use super::appearance::{self, Appearance};
use super::boat_sim;
use super::display;
use super::horse_sim;
use super::interaction_box;
use super::keyframe::AnimationState;
use super::kind::EntityKind;
use super::living::{BodyRotation, LivingRenderState, Swim, SwimTurn, rot_lerp, tick_head_turn};
use super::magic_name::Magic;
use super::mobs::{
    self, armadillo, breeze, camel, copper_golem, creaking, feline, fox, golem, horse, sniffer,
    warden,
};
use super::ride;
use super::walk::WalkAnimation;
use crate::math::wrap_degrees;
use crate::network::play::game::{
    AddEntity, EVENT_DEATH, EntityPacket, EquipmentSlot, MoveEntity, SetPosition, relative,
};
use crate::world::level::Level;
use crate::world::physics::{self, Aabb};
use crate::world::player::hands::{Hand, SwingState};

mod drive;
mod entities;
mod movement;
mod pickup;
mod pose;
mod push;
#[cfg(test)]
mod tests;
mod tick;

pub use drive::{DriveInput, DrivenVehicle};
pub use entities::Entities;

/// `LivingEntity`'s interpolation and head lerp.
const LERP_STEPS: u32 = 3;
/// A move farther than this jumps instead of gliding there.
const SNAP_DISTANCE: f64 = 64.0;
/// `LivingEntity.hurtDuration`.
pub const HURT_DURATION: i32 = 10;
/// `LivingEntity.tickDeath`: the server removes the body after 20 ticks.
pub const DEATH_DURATION: i32 = 25;
/// `Creeper.maxSwell`.
const MAX_SWELL: i32 = 30;
/// `Entity.getDefaultGravity` of a falling block and primed TNT.
const GRAVITY: f64 = 0.04;
/// `Entity.getAirDrag`.
const AIR_DRAG: f32 = 0.98;
/// `VecDeltaCodec`'s steps per block.
const DELTA_STEPS: f64 = 4096.0;

/// What an entity holds and wears.
#[derive(Clone, Default)]
pub struct Equipment {
    slots: [Option<ItemStack>; 8],
    /// Something in the saddle slot; only whether, for riding on a click.
    pub saddled: bool,
}

impl Equipment {
    fn index(slot: EquipmentSlot) -> usize {
        match slot {
            EquipmentSlot::MainHand => 0,
            EquipmentSlot::OffHand => 1,
            EquipmentSlot::Feet => 2,
            EquipmentSlot::Legs => 3,
            EquipmentSlot::Chest => 4,
            EquipmentSlot::Head => 5,
            EquipmentSlot::Body => 6,
            EquipmentSlot::Saddle => 7,
        }
    }

    pub fn set(&mut self, slot: EquipmentSlot, stack: ItemStack) {
        if slot == EquipmentSlot::Saddle {
            self.saddled = !stack.is_empty();
        }
        let i = Self::index(slot);
        self.slots[i] = (!stack.is_empty()).then_some(stack);
    }

    /// Whether nothing is held or worn.
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// The stack in `slot`, empty if none.
    pub fn get(&self, slot: EquipmentSlot) -> &ItemStack {
        self.slots[Self::index(slot)]
            .as_ref()
            .unwrap_or(ItemStack::EMPTY)
    }

    /// The worn pieces by `ArmorSlot::ALL`: head, chest, legs, feet.
    pub fn armor(&self) -> [&ItemStack; 4] {
        [
            EquipmentSlot::Head,
            EquipmentSlot::Chest,
            EquipmentSlot::Legs,
            EquipmentSlot::Feet,
        ]
        .map(|slot| self.get(slot))
    }

    pub fn hand(&self, hand: Hand) -> &ItemStack {
        self.get(match hand {
            Hand::Main => EquipmentSlot::MainHand,
            Hand::Off => EquipmentSlot::OffHand,
        })
    }
}

/// Where the interpolation is headed (`InterpolationHandler`).
#[derive(Clone, Copy, Debug)]
struct Lerp {
    pos: DVec3,
    yaw: f32,
    pitch: f32,
    steps: u32,
}

/// One tracked entity. Angles are vanilla's degrees (yaw 0 faces south).
#[derive(Clone)]
pub struct ClientEntity {
    pub id: i32,
    pub uuid: uuid::Uuid,
    pub kind: EntityKind,
    pub ty: &'static EntityType,
    /// `VecDeltaCodec.base`.
    base: DVec3,
    pos: DVec3,
    prev_pos: DVec3,
    /// `Entity.deltaMovement`, per tick.
    velocity: DVec3,
    yaw: f32,
    prev_yaw: f32,
    pitch: f32,
    prev_pitch: f32,
    head: f32,
    prev_head: f32,
    body: f32,
    prev_body: f32,
    lerp: Option<Lerp>,
    head_lerp: Option<(f32, u32)>,
    body_rotation: BodyRotation,
    pub walk: WalkAnimation,
    pub swing: SwingState,
    hurt_time: i32,
    death_time: i32,
    age: u32,
    /// A mod movement state it began (`melon:movement`'s `Started`) and the
    /// ticks since, for the mod's look; gone after [`CUSTOM_MOVE_TICKS`].
    // TODO(mods): stored only; no hook yet lets a mod pose or draw it.
    custom_move: Option<(String, u32)>,
    /// `getTicksUsingItem`: ticks since it began using an item.
    using_ticks: u32,
    /// A cat's lying amounts, this tick's and last tick's.
    feline: (feline::Amounts, feline::Amounts),
    /// `Wolf.interestedAngle`, this tick's and last tick's.
    interest: (f32, f32),
    /// A horse's eating, rearing and mouth amounts, likewise.
    equine: (horse::Amounts, horse::Amounts),
    /// A rabbit's hop and idle timers.
    rabbit: mobs::rabbit::RabbitClock,
    /// A camel's idle timeout.
    camel: camel::CamelClock,
    /// Fox crouch amount, this tick and last.
    fox_crouch: (f32, f32),
    /// Polar bear stand animation, 0..6, this tick and last.
    polar_stand: (f32, f32),
    /// Ticks spent in the armadillo's current synced state.
    armadillo_ticks: u32,
    armadillo_seen: i32,
    /// Ticks since `camel_pose_tick` last changed.
    camel_pose_age: u32,
    camel_seen_tick: i64,
    /// The keyframe animation clocks (`AnimationState`s), by slot.
    pub anim: [AnimationState; mobs::ANIMATION_SLOTS],
    /// An iron golem's arm swing and flower offer, in ticks left.
    golem: (i32, i32),
    /// A warden's tendrils and heartbeat.
    pub warden: mobs::warden::WardenClock,
    /// Sounds the entity makes itself this tick (a warden's heartbeat).
    pub(super) sounds: Vec<&'static str>,
    swell: i32,
    prev_swell: i32,
    /// `Chicken.flap`, `flapSpeed` and `flapping`, with last tick's.
    flap: f32,
    prev_flap: f32,
    flap_speed: f32,
    prev_flap_speed: f32,
    flapping: f32,
    /// `swimAmount` now and a tick ago, and whether the swimming box is in water.
    swim_amount: f32,
    prev_swim_amount: f32,
    pub in_water: bool,
    /// Moved sideways this tick (`horizontalDistanceSqr > 1.0E-7`).
    pub moving: bool,
    /// `AbstractCubeMob.squish`, its last tick's and `targetSquish`.
    squish: f32,
    prev_squish: f32,
    target_squish: f32,
    was_on_ground: bool,
    /// A boat's paddle clocks.
    paddles: mobs::vehicle::Paddles,
    /// `Squid.aiStep`'s swimming.
    squid: mobs::squid::Swim,
    /// `Guardian.aiStep`'s tail and spikes.
    guardian: mobs::guardian::Swim,
    /// `Axolotl`'s four eased factors.
    axolotl: mobs::axolotl::Animators,
    pub on_ground: bool,
    /// `Entity.mainSupportingBlockPos`, for the block factor.
    support: crate::world::physics::Support,
    pub appearance: Appearance,
    /// `Entity.getCustomName` as plain text, for the name tag.
    pub custom_name: Option<Box<Component>>,
    /// The name changed and its effect (`Magic`) is not worked out yet.
    name_pending: bool,
    pub equipment: Equipment,
    /// `ItemEntity.DATA_ITEM`: the dropped stack.
    pub item: ItemStack,
    /// A display entity's data and easing, for those.
    pub display: Option<Box<display::DisplayState>>,
    /// An `Interaction`'s synced box and response flag.
    pub interaction: Option<interaction_box::InteractionBox>,
    /// An item frame's facing (`Direction.get3DDataValue`, from the spawn data).
    pub hang: i32,
    pub frame_turn: i32,
    /// `ItemEntity.bobOffs`.
    pub bob_offs: f32,
    /// `Leashable.getLeashHolder`: the entity holding its lead, by id.
    pub leash_holder: Option<i32>,
    /// A fishing bobber's player (`Projectile.getOwner`, the spawn data).
    pub owner: Option<i32>,
    /// `isPickable`, from the type once rather than at every pick.
    pickable: bool,
    /// What it rides (`getVehicle`), and who rides it (`getPassengers`), in order.
    pub vehicle: Option<i32>,
    pub passengers: Vec<i32>,
    /// `Attributes.MAX_HEALTH`: the type's, until the server says more.
    pub max_health: f32,
    /// `Attributes.MOVEMENT_SPEED`, `JUMP_STRENGTH` and `STEP_HEIGHT`.
    movement_speed: f32,
    jump_strength: f32,
    step_height: f32,
    /// `Attributes.SCALE` (`LivingEntity.getScale`): the box and the model
    /// grow with it.
    scale_attribute: f32,
    /// `Attributes.NAME_TAG_DISTANCE` and `BELOW_NAME_DISTANCE`, in blocks.
    pub name_tag_distance: f32,
    pub below_name_distance: f32,
    /// The body turn of the living vehicle it rides, last tick's and this tick's.
    vehicle_body: Option<(f32, f32)>,
    /// `EvokerFangs.lifeTicks` once the bite began (`clientSideAttackStarted`).
    pub fangs_life: Option<i32>,
    /// `AbstractArrow.shakeTime`: ticks left of the wobble when it sticks.
    pub arrow_shake: i32,
    /// `SulfurCube.fuse` or `MinecartTNT.fuse`: ticks until it blows, -1
    /// unlit.
    pub fuse_ticks: i32,
    /// A mannequin's profile and description; `None` for anything else.
    pub mannequin: Option<Box<Mannequin>>,
    /// `EnderDragon.nearestCrystal`: the end crystal healing it, by id.
    pub nearest_crystal: Option<i32>,
}

/// `Mannequin`'s synced look.
#[derive(Clone, Debug)]
pub struct Mannequin {
    /// `DATA_PROFILE`.
    pub profile: crate::world::head_profile::AvatarProfile,
    /// `DATA_DESCRIPTION`: the line under its name, the "Mannequin" label
    /// until the server says otherwise; `None` once hidden.
    pub description: Option<Component>,
}

impl Default for Mannequin {
    fn default() -> Self {
        Self {
            profile: Default::default(),
            description: Some(Component::translatable(
                "entity.minecraft.mannequin.label",
                Vec::new(),
            )),
        }
    }
}

/// How long a started mod movement state stays on an entity.
pub const CUSTOM_MOVE_TICKS: u32 = 40;

impl ClientEntity {
    /// The mod movement state it began and the ticks since.
    pub fn custom_move(&self) -> Option<(&str, u32)> {
        self.custom_move
            .as_ref()
            .map(|(id, age)| (id.as_str(), *age))
    }

    /// `recreateFromPacket`; `None` for a type the registry doesn't know.
    pub fn spawn(add: &AddEntity) -> Option<Self> {
        let ty = EntityType::from_raw(u16::try_from(add.kind).ok()?)?;
        if crate::probe::enabled() {
            let kind = EntityKind::of(ty);
            crate::probe::count("entity.spawned", ty.resource_name);
            if kind == EntityKind::Other {
                crate::probe::note("entity.not_drawn", ty.resource_name, || {
                    format!("first at {:?}, data {}", add.pos, add.data)
                });
            }
        }
        let pos = DVec3::from_array(add.pos);
        Some(Self {
            id: add.id,
            uuid: add.uuid,
            kind: EntityKind::of(ty),
            ty,
            base: pos,
            pos,
            prev_pos: pos,
            velocity: DVec3::from_array(add.velocity),
            yaw: add.yaw,
            prev_yaw: add.yaw,
            pitch: add.pitch,
            prev_pitch: add.pitch,
            head: add.head_yaw,
            prev_head: add.head_yaw,
            body: add.head_yaw,
            prev_body: add.head_yaw,
            lerp: None,
            head_lerp: None,
            body_rotation: BodyRotation::default(),
            walk: WalkAnimation::default(),
            swing: SwingState::default(),
            hurt_time: 0,
            death_time: 0,
            age: 0,
            custom_move: None,
            using_ticks: 0,
            feline: Default::default(),
            interest: (0.0, 0.0),
            equine: Default::default(),
            rabbit: mobs::rabbit::RabbitClock::new(add.id),
            camel: camel::CamelClock::new(add.id),
            fox_crouch: (0.0, 0.0),
            polar_stand: (0.0, 0.0),
            armadillo_ticks: 0,
            armadillo_seen: i32::MIN,
            camel_pose_age: 0,
            camel_seen_tick: 0,
            anim: Default::default(),
            golem: (0, 0),
            warden: Default::default(),
            sounds: Vec::new(),
            swell: 0,
            prev_swell: 0,
            flap: 0.0,
            prev_flap: 0.0,
            flap_speed: 0.0,
            prev_flap_speed: 0.0,
            swim_amount: 0.0,
            prev_swim_amount: 0.0,
            in_water: false,
            moving: false,
            squish: 0.0,
            prev_squish: 0.0,
            target_squish: 0.0,
            was_on_ground: false,
            paddles: Default::default(),
            squid: mobs::squid::Swim::new(add.id),
            guardian: mobs::guardian::Swim::new(add.id),
            axolotl: Default::default(),
            flapping: 1.0,
            on_ground: false,
            support: Default::default(),
            custom_name: None,
            name_pending: false,
            appearance: Appearance {
                block_state: match EntityKind::of(ty) {
                    EntityKind::FallingBlock => u16::try_from(add.data).unwrap_or(0),
                    EntityKind::PrimedTnt => pumpkin_data::Block::TNT.default_state.id.as_u16(),
                    _ => 0,
                },
                ..Appearance::default()
            },
            equipment: Equipment::default(),
            item: ItemStack::EMPTY.clone(),
            display: None,
            interaction: None,
            hang: add.data,
            frame_turn: 0,
            bob_offs: bob_offset(add.uuid),
            leash_holder: None,
            owner: (EntityKind::of(ty) == EntityKind::FishingBobber).then_some(add.data),
            pickable: pickable(ty),
            vehicle: None,
            passengers: Vec::new(),
            max_health: type_attr(ty, crate::network::play::game::ATTRIBUTE_MAX_HEALTH, 20.0),
            movement_speed: type_attr(
                ty,
                crate::network::play::game::ATTRIBUTE_MOVEMENT_SPEED,
                0.7,
            ),
            jump_strength: type_attr(
                ty,
                crate::network::play::game::ATTRIBUTE_JUMP_STRENGTH,
                0.42,
            ),
            step_height: type_attr(ty, crate::network::play::game::ATTRIBUTE_STEP_HEIGHT, 0.6),
            scale_attribute: type_attr(ty, crate::network::play::game::ATTRIBUTE_SCALE, 1.0),
            name_tag_distance: type_attr(
                ty,
                crate::network::play::game::ATTRIBUTE_NAME_TAG_DISTANCE,
                64.0,
            ),
            below_name_distance: type_attr(
                ty,
                crate::network::play::game::ATTRIBUTE_BELOW_NAME_DISTANCE,
                10.0,
            ),
            vehicle_body: None,
            fangs_life: None,
            arrow_shake: 0,
            fuse_ticks: -1,
            mannequin: (EntityKind::of(ty) == EntityKind::Mannequin).then(Default::default),
            nearest_crystal: None,
        })
    }

    fn apply(&mut self, packet: EntityPacket) {
        match packet {
            EntityPacket::Move(m) => self.relative_move(&m),
            EntityPacket::SetPosition(p) => self.set_position(&p),
            EntityPacket::Motion { velocity, .. } => self.velocity = DVec3::from_array(velocity),
            EntityPacket::HeadRotation { yaw, .. } => self.head_lerp = Some((yaw, LERP_STEPS)),
            EntityPacket::Data { values, .. } => {
                for (index, value) in &values {
                    // `ItemEntity.DATA_ITEM`, and the thrown items', rockets' and
                    // spawners' stack (index 8 of each).
                    let holds_item = match self.kind {
                        EntityKind::Item => {
                            *index == pumpkin_data::tracked_data::item::DATA_ITEM.id.0
                        }
                        EntityKind::ThrownItem
                        | EntityKind::FireworkRocket
                        | EntityKind::OminousItemSpawner => *index == 8,
                        _ => false,
                    };
                    if holds_item && let crate::network::play::game::DataValue::Item(stack) = value
                    {
                        self.item = (**stack).clone();
                    }
                    if let Some(mannequin) = &mut self.mannequin {
                        use pumpkin_data::tracked_data::mannequin as m;
                        match value {
                            crate::network::play::game::DataValue::Profile(profile)
                                if *index == m::DATA_PROFILE.id.0 =>
                            {
                                mannequin.profile = (**profile).clone();
                            }
                            crate::network::play::game::DataValue::Name(text)
                                if *index == m::DATA_DESCRIPTION.id.0 =>
                            {
                                mannequin.description = text.as_deref().cloned();
                            }
                            _ => {}
                        }
                    }
                    if self.kind.is_item_frame() {
                        match (index, value) {
                            (9, crate::network::play::game::DataValue::Item(stack)) => {
                                self.item = (**stack).clone();
                            }
                            (10, v) => self.frame_turn = v.int().unwrap_or(0),
                            _ => {}
                        }
                    }
                    if self.kind == EntityKind::Interaction
                        && self
                            .interaction
                            .get_or_insert_with(Default::default)
                            .apply(*index, value)
                    {
                        continue;
                    }
                    if self.kind.is_display() && *index >= 8 {
                        self.display
                            .get_or_insert_with(Default::default)
                            .apply(self.kind, *index, value);
                        continue;
                    }
                    if self.kind == EntityKind::Other && crate::probe::enabled() {
                        crate::probe::note(
                            "entity.data",
                            format!("{} #{index}", self.ty.resource_name),
                            || format!("{value:?}"),
                        );
                    }
                    let thing = self.appearance.thing;
                    self.appearance.apply(self.kind, *index, value);
                    let now = self.appearance.thing;
                    // `AbstractArrow.onSyncedDataUpdated`: it shakes as it sticks.
                    if now.in_ground && !thing.in_ground && self.age > 0 && self.arrow_shake <= 0 {
                        self.arrow_shake = mobs::projectile::ARROW_SHAKE_TICKS;
                    }
                    // `SulfurCube.onSyncedDataUpdated(MAX_FUSE)`.
                    if now.max_fuse != thing.max_fuse {
                        self.fuse_ticks = now.max_fuse;
                    }
                    if *index == appearance::CUSTOM_NAME_INDEX
                        && let Some(name) = value.name()
                    {
                        self.custom_name = name.cloned().map(Box::new);
                        self.name_pending = true;
                    }
                }
            }
            EntityPacket::Equipment { slots, .. } => {
                for (slot, stack) in slots {
                    self.equipment.set(slot, stack);
                }
            }
            EntityPacket::Hurt { .. } | EntityPacket::Damage { .. } => {
                self.hurt_time = HURT_DURATION
            }
            EntityPacket::Swing { hand, swing, .. } => {
                self.swing.start_if_able(hand, swing);
            }
            EntityPacket::Event { event, .. } => {
                if event == EVENT_DEATH && self.kind != EntityKind::Player {
                    self.appearance.health = 0.0;
                }
                if self.kind.is_squid() && event == mobs::squid::EVENT_STROKE {
                    self.squid.stroke();
                }
                if self.kind == EntityKind::Minecart
                    && self.ty.resource_name == "tnt_minecart"
                    && event == mobs::vehicle::EVENT_PRIME_TNT
                {
                    self.fuse_ticks = crate::world::entity::appearance::DEFAULT_FUSE;
                }
                if self.kind == EntityKind::EvokerFangs
                    && event == mobs::projectile::EVENT_FANGS_ATTACK
                {
                    self.fangs_life = Some(mobs::projectile::FANGS_LIFE_TICKS);
                }
                if self.kind == EntityKind::Rabbit && event == mobs::rabbit::EVENT_JUMP {
                    self.rabbit.jump();
                }
                if self.kind == EntityKind::Warden {
                    self.warden.event(event, &mut self.anim, self.age as i32);
                }
                if self.kind == EntityKind::IronGolem {
                    match event {
                        golem::EVENT_ATTACK => self.golem.0 = golem::ATTACK_TICKS,
                        golem::EVENT_OFFER_FLOWER => self.golem.1 = golem::OFFER_FLOWER_TICKS,
                        golem::EVENT_STOP_OFFER_FLOWER => self.golem.1 = 0,
                        _ => {}
                    }
                }
            }
            EntityPacket::Leash { holder, .. } => self.leash_holder = holder,
            EntityPacket::Attributes { values, .. } => {
                for (attribute, value) in values {
                    let value = value as f32;
                    if attribute == crate::network::play::game::ATTRIBUTE_MAX_HEALTH {
                        self.max_health = value;
                    } else if attribute == crate::network::play::game::ATTRIBUTE_MOVEMENT_SPEED {
                        self.movement_speed = value;
                    } else if attribute == crate::network::play::game::ATTRIBUTE_JUMP_STRENGTH {
                        self.jump_strength = value;
                    } else if attribute == crate::network::play::game::ATTRIBUTE_STEP_HEIGHT {
                        self.step_height = value;
                    } else if attribute == crate::network::play::game::ATTRIBUTE_SCALE {
                        self.scale_attribute = value;
                    } else if attribute == crate::network::play::game::ATTRIBUTE_NAME_TAG_DISTANCE {
                        self.name_tag_distance = value;
                    } else if attribute == crate::network::play::game::ATTRIBUTE_BELOW_NAME_DISTANCE
                    {
                        self.below_name_distance = value;
                    }
                }
            }
            EntityPacket::Add(_)
            | EntityPacket::Remove(_)
            | EntityPacket::Take { .. }
            | EntityPacket::Passengers { .. } => {}
        }
    }
}

/// `VecDeltaCodec.decode`: an axis with no delta keeps the base exactly.
fn decode_delta(base: DVec3, [x, y, z]: [i16; 3]) -> DVec3 {
    let axis = |b: f64, d: i16| {
        if d == 0 {
            b
        } else {
            ((b * DELTA_STEPS).round() + f64::from(d)) / DELTA_STEPS
        }
    };
    DVec3::new(axis(base.x, x), axis(base.y, y), axis(base.z, z))
}

fn type_attr(ty: &EntityType, id: i32, fallback: f32) -> f32 {
    ty.attributes
        .iter()
        .find(|(a, _)| a.id as i32 == id)
        .map_or(fallback, |(_, v)| *v as f32)
}

/// `Entity.isPickable`.
fn pickable(ty: &EntityType) -> bool {
    !ty.attributes.is_empty()
        || ty.boat.is_some()
        || ty.resource_name.ends_with("minecart")
        || ty.resource_name == "end_crystal"
        || ty.id == EntityType::LEASH_KNOT.id
        || ty.id == EntityType::INTERACTION.id
}

/// `ItemEntity.bobOffs`, which vanilla draws at random for each client:
fn bob_offset(uuid: uuid::Uuid) -> f32 {
    let (hi, lo) = uuid.as_u64_pair();
    let bits = (hi ^ lo.rotate_left(17)) >> 40;
    bits as f32 / (1u64 << 24) as f32 * std::f32::consts::TAU
}
