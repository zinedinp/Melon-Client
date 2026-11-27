//! What an entity's synced data (`SET_ENTITY_DATA`) says about how it looks.

use pumpkin_data::dye_color::DyeColor;
use pumpkin_data::tracked_data::{
    TrackedData, abstract_piglin, ageable_mob, allay, armadillo, armor_stand, avatar, axolotl, bat,
    bee, boat, bogged, camel, camel_husk, cat, chicken, copper_golem, cow, creaking, creeper,
    donkey, enderman, entity, evoker, fox, frog, ghast, glow_squid, goat, guardian, hoglin, horse,
    illusioner, living_entity, llama, mob, mooshroom, mule, panda, parrot, phantom, pig, piglin,
    pillager, polar_bear, primed_tnt, pufferfish, rabbit, salmon, sheep, shulker, skeleton, slime,
    sniffer, snow_golem, strider, trader_llama, tropical_fish, turtle, vex, villager, vindicator,
    warden, wither, wolf, zombie, zombie_villager,
};
use pumpkin_data::tracked_data::{
    abstract_arrow, abstract_minecart, arrow, cushion, end_crystal, experience_orb,
    firework_rocket, minecart_furnace, painting, sulfur_cube, thrown_trident, wither_skull,
    zombie_nautilus,
};

use super::kind::EntityKind;
use super::magic_name::Magic;
use super::player::{Arm, ShownParts};
use crate::entity::wire::DataValue;
use crate::entity::wire::{LIVING_OFF_HAND, LIVING_USING_ITEM};
use crate::player::hands::Hand;

/// `PrimedTnt.DEFAULT_FUSE_TIME`.
pub const DEFAULT_FUSE: i32 = 80;

const fn index(data: TrackedData) -> u8 {
    data.id.0
}

/// `Entity.FLAG_SHIFT_KEY_DOWN` and `FLAG_INVISIBLE`.
const SHARED_CROUCHING: u8 = 1 << 1;
const SHARED_SPRINTING: u8 = 1 << 3;
const SHARED_SWIMMING: u8 = 1 << 4;
const SHARED_INVISIBLE: u8 = 1 << 5;
/// `Entity.FLAG_GLOWING`.
const SHARED_GLOWING: u8 = 1 << 6;
/// `Mob.MOB_FLAG_LEFTHANDED` and `MOB_FLAG_AGGRESSIVE`.
const MOB_LEFT_HANDED: u8 = 1 << 1;
const MOB_AGGRESSIVE: u8 = 1 << 2;
const MOB_NO_AI: u8 = 1 << 0;
/// `Sheep`'s wool byte: colour in the low bits, sheared above.
const TAMABLE_SITTING: u8 = 1;
const TAMABLE_TAME: u8 = 4;

/// `Entity.DATA_CUSTOM_NAME`'s index.
pub const CUSTOM_NAME_INDEX: u8 = index(entity::DATA_CUSTOM_NAME);

const WOOL_COLOR: u8 = 0x0F;
const WOOL_SHEARED: u8 = 0x10;
/// `Pose.SWIMMING`'s and `Pose.CROUCHING`'s ids.
const POSE_SWIMMING: i32 = 3;
const POSE_CROUCHING: i32 = 5;

#[derive(Clone, Copy, Debug)]
pub struct Appearance {
    pub shared_flags: u8,
    /// `Entity.DATA_NO_GRAVITY`: a server holds an item in the air with it.
    pub no_gravity: bool,
    /// `Entity.DATA_SILENT`.
    pub silent: bool,
    pub mob_flags: u8,
    /// `DATA_LIVING_ENTITY_FLAGS`: using an item, in which hand.
    pub living_flags: u8,
    pub pose: i32,
    pub health: f32,
    pub baby: bool,
    /// The pig, cow or chicken variant's registry id.
    pub variant: i32,
    pub wool: u8,
    /// `Creeper.DATA_SWELL_DIR`: -1 idle, 1 swelling.
    pub swell_dir: i32,
    /// `AbstractCubeMob.getSize`: 1, 2 or 4 (up to 127).
    pub size: i32,
    /// `Piglin.DATA_IS_DANCING`.
    pub dancing: bool,
    /// `Piglin.DATA_IS_CHARGING_CROSSBOW`.
    pub charging_crossbow: bool,
    /// A raider cheering (`isCelebrating`).
    pub celebrating: bool,
    /// `MushroomCow.Variant.BROWN` (id 1); red (0) otherwise.
    pub brown_mushrooms: bool,
    /// `TamableAnimal.DATA_FLAGS_ID`.
    pub tamable_flags: u8,
    /// `DATA_COLLAR_COLOR`: a dye id (red by default).
    pub collar: u8,
    /// `Wolf.DATA_VARIANT_ID`, a `WolfVariant` (pale by default).
    pub wolf_variant: i32,
    /// `Cat.DATA_VARIANT_ID`, a `CatVariant` (tabby by default).
    pub cat_variant: i32,
    /// `Wolf.DATA_ANGER_END_TIME`: the game time its anger ends, -1 for none.
    pub anger_end: i64,
    /// Whether that is still ahead (set each tick from the game time).
    pub angry: bool,
    /// `Wolf.isInterested`: begging, the head tilted.
    pub interested: bool,
    /// `Cat.IS_LYING` and `RELAX_STATE_ONE`.
    pub lying: bool,
    pub relaxed: bool,
    /// `AbstractHorse.DATA_ID_FLAGS`: eating 16, rearing 32, mouth open 64.
    pub horse_flags: u8,
    /// `Horse.DATA_ID_TYPE_VARIANT`: colour, and markings in the next byte.
    pub horse_variant: i32,
    /// `AbstractChestedHorse.DATA_ID_CHEST`.
    pub has_chest: bool,
    /// `SpellcasterIllager.DATA_SPELL_CASTING_ID` is above 0.
    pub casting_spell: bool,
    /// `VillagerData`: type, profession and level (registry ids).
    pub villager: (i32, i32, i32),
    /// `AbstractVillager.getUnhappyCounter() > 0`.
    pub unhappy: bool,
    /// A zombie villager curing (`isConverting`), a zombie drowning into a drowned, ...
    pub converting: bool,
    /// `DATA_IMMUNE_TO_ZOMBIFICATION` of a piglin.
    pub immune_to_zombification: bool,
    /// A piglin that is turning into a zombified one (`isConverting`).
    pub zombifying: bool,
    /// `DATA_TICKS_FROZEN`.
    pub frozen_ticks: i32,
    pub powered: bool,
    pub ignited: bool,
    /// An enderman holds a block (`DATA_CARRY_STATE` is a state).
    pub block_state: u16,
    /// `PrimedTnt.DATA_FUSE_ID`: ticks until it blows.
    pub fuse: i32,
    /// `VehicleEntity`'s `DATA_ID_HURT`, `HURTDIR` and `DAMAGE`.
    pub vehicle_hurt: i32,
    pub vehicle_hurt_dir: i32,
    pub vehicle_damage: f32,
    /// A boat's paddles in the water, left and right.
    pub paddles: (bool, bool),
    pub creepy: bool,
    /// `Ghast.isCharging`: it shows its fiery face.
    pub charging: bool,
    /// `SnowGolem.hasPumpkin`.
    pub pumpkin: bool,
    /// `GlowSquid.getDarkTicksRemaining`: it dims after a hit.
    pub dark_ticks: i32,
    /// `Salmon.Variant`'s id: small, medium (the default), large.
    pub salmon_variant: i32,
    /// `Pufferfish.getPuffState`: 0 deflated, 1 half, 2 puffed.
    pub puff_state: i32,
    /// `TropicalFish`'s packed pattern and colours.
    pub tropical_variant: i32,
    /// `Axolotl.Variant`'s id, and whether it plays dead.
    pub axolotl_variant: i32,
    pub playing_dead: bool,
    /// `Turtle.hasEgg` and `isLayingEgg`.
    pub has_egg: bool,
    pub laying_egg: bool,
    /// `Guardian.isMoving`.
    pub guardian_moving: bool,
    /// What the custom name makes renderers do (`Magic::of`).
    pub magic: Magic,
    /// `Rabbit.Variant`'s id (brown is 0, not the registry's temperate).
    pub rabbit_variant: i32,
    /// A bat hangs (`Bat.isResting`).
    pub resting: bool,
    /// A player's skin layers and main arm.
    pub shown: ShownParts,
    pub player_main_arm: Arm,
    /// `Fox.DATA_FLAGS_ID`.
    pub fox_flags: u8,
    /// `Fox.DATA_TYPE_ID`: 0 red, 1 snow.
    pub fox_variant: i32,
    /// `Armadillo.ARMADILLO_STATE`.
    pub armadillo_state: i32,
    /// `PolarBear.DATA_STANDING_ID`.
    pub polar_standing: bool,
    /// `Camel.LAST_POSE_CHANGE_TICK`: sitting when negative.
    pub camel_pose_tick: i64,
    /// `Camel.DASH`.
    pub camel_dashing: bool,
    /// `Goat.DATA_HAS_LEFT_HORN` / `DATA_HAS_RIGHT_HORN`.
    pub goat_left_horn: bool,
    pub goat_right_horn: bool,
    /// `Bee.DATA_FLAGS_ID`: rolling 2, stung 4, nectar 8.
    pub bee_flags: u8,
    /// `Llama.DATA_VARIANT_ID`.
    pub llama_variant: i32,
    /// `Panda.MAIN_GENE_ID`.
    pub panda_gene: u8,
    /// `Panda.DATA_ID_FLAGS`.
    pub panda_flags: u8,
    /// `Parrot.DATA_VARIANT_ID`.
    pub parrot_variant: i32,
    /// `Sniffer.DATA_STATE`.
    pub sniffer_state: i32,
    /// `Shulker.DATA_PEEK_ID`.
    pub shulker_peek: u8,
    /// `Shulker.DATA_COLOR_ID`, 16 for the default purple.
    pub shulker_color: u8,
    /// `Shulker.DATA_ATTACH_FACE_ID`.
    pub attach_face: i32,
    /// `Strider.DATA_SUFFOCATING`.
    pub strider_cold: bool,
    /// `Creaking.IS_ACTIVE`.
    pub creaking_active: bool,
    /// `CopperGolem.COPPER_GOLEM_STATE`.
    pub copper_state: i32,
    /// `CopperGolem.DATA_WEATHER_STATE`.
    pub copper_weather: i32,
    /// `Warden.CLIENT_ANGER_LEVEL`.
    pub warden_anger: i32,
    /// `WitherBoss.DATA_ID_INV`.
    pub wither_invul: i32,
    /// `ArmorStand.DATA_CLIENT_FLAGS`.
    pub stand_flags: u8,
    /// `Entity.DATA_CUSTOM_NAME_VISIBLE`: the name hangs over it.
    pub name_visible: bool,
    /// The limb angles in degrees (x, y, z), by [`StandPose`]'s order.
    pub stand_pose: [[f32; 3]; 6],
    /// What projectiles, orbs and decorations sync about how they look.
    pub thing: ThingLook,
}

/// Where each limb sits in [`Appearance::stand_pose`].
#[derive(Clone, Copy, Debug)]
pub enum StandPose {
    Head,
    Body,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

/// `ArmorStand.DEFAULT_*_POSE`, in [`StandPose`]'s order.
pub const DEFAULT_STAND_POSE: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
    [-10.0, 0.0, -10.0],
    [-15.0, 0.0, 10.0],
    [-1.0, 0.0, -1.0],
    [1.0, 0.0, 1.0],
];

pub const STAND_SMALL: u8 = 1;
pub const STAND_ARMS: u8 = 4;
pub const STAND_NO_BASE_PLATE: u8 = 8;
pub const STAND_MARKER: u8 = 16;

impl Default for Appearance {
    fn default() -> Self {
        Self {
            shared_flags: 0,
            no_gravity: false,
            silent: false,
            mob_flags: 0,
            living_flags: 0,
            pose: 0,
            health: f32::MAX,
            baby: false,
            variant: 1,
            wool: 0,
            swell_dir: -1,
            size: 1,
            dancing: false,
            charging_crossbow: false,
            celebrating: false,
            brown_mushrooms: false,
            tamable_flags: 0,
            collar: 14,
            wolf_variant: 3,
            cat_variant: 9,
            anger_end: -1,
            angry: false,
            interested: false,
            lying: false,
            relaxed: false,
            horse_flags: 0,
            horse_variant: 0,
            has_chest: false,
            casting_spell: false,
            villager: (2, 0, 1),
            unhappy: false,
            converting: false,
            immune_to_zombification: false,
            zombifying: false,
            frozen_ticks: 0,
            powered: false,
            ignited: false,
            block_state: 0,
            fuse: DEFAULT_FUSE,
            vehicle_hurt: 0,
            vehicle_hurt_dir: 1,
            vehicle_damage: 0.0,
            paddles: (false, false),
            creepy: false,
            charging: false,
            pumpkin: true,
            dark_ticks: 0,
            salmon_variant: 1,
            puff_state: 0,
            axolotl_variant: 0,
            playing_dead: false,
            has_egg: false,
            laying_egg: false,
            guardian_moving: false,
            tropical_variant: super::mobs::fish::DEFAULT_TROPICAL,
            resting: false,
            rabbit_variant: 0,
            magic: Magic::None,
            shown: ShownParts::default(),
            player_main_arm: Arm::Right,
            fox_flags: 0,
            fox_variant: 0,
            armadillo_state: 0,
            polar_standing: false,
            camel_pose_tick: 0,
            camel_dashing: false,
            goat_left_horn: true,
            goat_right_horn: true,
            bee_flags: 0,
            llama_variant: 0,
            panda_gene: 0,
            panda_flags: 0,
            parrot_variant: 0,
            sniffer_state: 0,
            shulker_peek: 0,
            shulker_color: 16,
            attach_face: 1,
            strider_cold: false,
            creaking_active: false,
            copper_state: 0,
            copper_weather: 0,
            warden_anger: 0,
            wither_invul: 0,
            stand_flags: 0,
            name_visible: false,
            stand_pose: DEFAULT_STAND_POSE,
            thing: ThingLook::default(),
        }
    }
}

/// `Entity.getTicksRequiredToFreeze`.
const FROZEN_TICKS_TO_FREEZE: i32 = 140;

impl Appearance {
    /// Takes in one synced value of an entity of `kind`.
    pub fn apply(&mut self, kind: EntityKind, index_: u8, value: &DataValue) {
        let i = index_;
        let byte = || value.byte();
        let int = || value.int();
        match i {
            _ if kind == EntityKind::PrimedTnt && i >= index(primed_tnt::DATA_FUSE_ID) => {
                self.apply_kind(kind, i, value)
            }
            _ if i == index(entity::DATA_SHARED_FLAGS_ID) => {
                if let Some(b) = byte() {
                    self.shared_flags = b;
                }
            }
            _ if i == index(entity::DATA_SILENT) => {
                self.silent = value.bool().unwrap_or(self.silent);
            }
            _ if i == index(entity::DATA_NO_GRAVITY) => {
                if let Some(v) = value.bool() {
                    self.no_gravity = v;
                }
            }
            _ if i == index(entity::DATA_CUSTOM_NAME_VISIBLE) => {
                if let Some(v) = value.bool() {
                    self.name_visible = v;
                }
            }
            _ if i == index(entity::DATA_TICKS_FROZEN) => {
                if let Some(t) = int() {
                    self.frozen_ticks = t;
                }
            }
            _ if i == index(entity::DATA_POSE) => {
                if let Some(p) = int() {
                    self.pose = p;
                }
            }
            _ if i == index(living_entity::DATA_LIVING_ENTITY_FLAGS) => {
                if let Some(b) = byte() {
                    self.living_flags = b;
                }
            }
            _ if i == index(living_entity::DATA_HEALTH_ID) => {
                if let Some(h) = value.float() {
                    self.health = h;
                }
            }
            _ => self.apply_kind(kind, i, value),
        }
    }

    fn apply_kind(&mut self, kind: EntityKind, i: u8, value: &DataValue) {
        use EntityKind as K;
        if kind.is_avatar() {
            if i == index(avatar::DATA_PLAYER_MODE_CUSTOMISATION)
                && let Some(mask) = value.byte()
            {
                self.shown = ShownParts::from_mask(mask);
            } else if i == index(avatar::DATA_PLAYER_MAIN_HAND)
                && let Some(arm) = value.int()
            {
                self.player_main_arm = if arm == 0 { Arm::Left } else { Arm::Right };
            }
            return;
        }
        if kind.is_mob() && i == index(mob::DATA_MOB_FLAGS_ID) {
            if let Some(b) = value.byte() {
                self.mob_flags = b;
            }
            return;
        }
        let baby_index = match kind {
            K::Zombie | K::Husk | K::Drowned | K::ZombifiedPiglin | K::ZombieVillager => {
                Some(index(zombie::DATA_BABY_ID))
            }
            K::Piglin => Some(index(piglin::DATA_BABY_ID)),
            k if k.can_be_baby() => Some(index(ageable_mob::DATA_BABY_ID)),
            _ => None,
        };
        if Some(i) == baby_index {
            if let Some(b) = value.bool() {
                self.baby = b;
            }
            return;
        }
        if kind == K::ArmorStand {
            let pose = [
                (armor_stand::DATA_HEAD_POSE, StandPose::Head),
                (armor_stand::DATA_BODY_POSE, StandPose::Body),
                (armor_stand::DATA_LEFT_ARM_POSE, StandPose::LeftArm),
                (armor_stand::DATA_RIGHT_ARM_POSE, StandPose::RightArm),
                (armor_stand::DATA_LEFT_LEG_POSE, StandPose::LeftLeg),
                (armor_stand::DATA_RIGHT_LEG_POSE, StandPose::RightLeg),
            ];
            if i == index(armor_stand::DATA_CLIENT_FLAGS) {
                self.stand_flags = value.byte().unwrap_or(self.stand_flags);
            } else if let Some((_, limb)) = pose.iter().find(|(d, _)| i == index(*d))
                && let Some(r) = value.rotations()
            {
                self.stand_pose[*limb as usize] = r;
            }
            return;
        }
        let variant_index = match kind {
            K::Pig => Some(index(pig::DATA_VARIANT_ID)),
            K::Cow => Some(index(cow::DATA_VARIANT_ID)),
            K::Chicken => Some(index(chicken::DATA_VARIANT_ID)),
            K::Frog => Some(index(frog::DATA_VARIANT_ID)),
            _ => None,
        };
        if Some(i) == variant_index {
            if let Some(v) = value.int() {
                self.variant = v;
            }
            return;
        }
        match kind {
            K::Villager if i == index(villager::DATA_VILLAGER_DATA) => {
                self.villager = value.villager().unwrap_or(self.villager);
            }
            K::ZombieVillager if i == index(zombie_villager::DATA_VILLAGER_DATA) => {
                self.villager = value.villager().unwrap_or(self.villager);
            }
            K::Villager | K::WanderingTrader if i == index(villager::DATA_UNHAPPY_COUNTER) => {
                self.unhappy = value.int().is_some_and(|c| c > 0);
            }
            K::ZombieVillager if i == index(zombie_villager::DATA_CONVERTING_ID) => {
                self.converting = value.bool().unwrap_or(self.converting);
            }
            K::Zombie | K::Husk | K::Drowned if i == index(zombie::DATA_DROWNED_CONVERSION_ID) => {
                self.converting = value.bool().unwrap_or(self.converting);
            }
            K::Skeleton if i == index(skeleton::DATA_STRAY_CONVERSION_ID) => {
                self.converting = value.bool().unwrap_or(self.converting);
            }
            K::Piglin | K::PiglinBrute
                if i == index(abstract_piglin::DATA_IMMUNE_TO_ZOMBIFICATION) =>
            {
                self.immune_to_zombification = value.bool().unwrap_or(false);
            }
            K::Wolf if i == index(wolf::DATA_FLAGS_ID) => {
                self.tamable_flags = value.byte().unwrap_or(self.tamable_flags);
            }
            K::Cat if i == index(cat::DATA_FLAGS_ID) => {
                self.tamable_flags = value.byte().unwrap_or(self.tamable_flags);
            }
            K::Wolf if i == index(wolf::DATA_COLLAR_COLOR) => {
                self.collar = value.int().map_or(self.collar, |c| c.clamp(0, 15) as u8);
            }
            K::Cat if i == index(cat::DATA_COLLAR_COLOR) => {
                self.collar = value.int().map_or(self.collar, |c| c.clamp(0, 15) as u8);
            }
            K::Wolf if i == index(wolf::DATA_VARIANT_ID) => {
                self.wolf_variant = value.int().unwrap_or(self.wolf_variant);
            }
            K::Cat if i == index(cat::DATA_VARIANT_ID) => {
                self.cat_variant = value.int().unwrap_or(self.cat_variant);
            }
            K::Wolf if i == index(wolf::DATA_ANGER_END_TIME) => {
                self.anger_end = value.long().unwrap_or(self.anger_end);
            }
            K::Wolf if i == index(wolf::DATA_INTERESTED_ID) => {
                self.interested = value.bool().unwrap_or(self.interested);
            }
            K::Cat if i == index(cat::IS_LYING) => {
                self.lying = value.bool().unwrap_or(self.lying);
            }
            K::Cat if i == index(cat::RELAX_STATE_ONE) => {
                self.relaxed = value.bool().unwrap_or(self.relaxed);
            }
            k if k.is_equine() && i == index(horse::DATA_ID_FLAGS) => {
                self.horse_flags = value.byte().unwrap_or(self.horse_flags);
            }
            K::Horse if i == index(horse::DATA_ID_TYPE_VARIANT) => {
                self.horse_variant = value.int().unwrap_or(self.horse_variant);
            }
            K::Donkey if i == index(donkey::DATA_ID_CHEST) => {
                self.has_chest = value.bool().unwrap_or(self.has_chest);
            }
            K::Mule if i == index(mule::DATA_ID_CHEST) => {
                self.has_chest = value.bool().unwrap_or(self.has_chest);
            }
            K::Mooshroom if i == index(mooshroom::DATA_TYPE) => {
                self.brown_mushrooms = value.int() == Some(1);
            }
            K::Pillager if i == index(pillager::IS_CELEBRATING) => {
                self.celebrating = value.bool().unwrap_or(self.celebrating);
            }
            K::Vindicator if i == index(vindicator::IS_CELEBRATING) => {
                self.celebrating = value.bool().unwrap_or(self.celebrating);
            }
            K::Evoker if i == index(evoker::IS_CELEBRATING) => {
                self.celebrating = value.bool().unwrap_or(self.celebrating);
            }
            K::Evoker if i == index(evoker::DATA_SPELL_CASTING_ID) => {
                self.casting_spell = value.byte().is_some_and(|b| b > 0);
            }
            K::Illusioner if i == index(illusioner::IS_CELEBRATING) => {
                self.celebrating = value.bool().unwrap_or(self.celebrating);
            }
            K::Illusioner if i == index(illusioner::DATA_SPELL_CASTING_ID) => {
                self.casting_spell = value.byte().is_some_and(|b| b > 0);
            }
            K::Fox if i == index(fox::DATA_FLAGS_ID) => {
                if let Some(b) = value.byte() {
                    self.fox_flags = b;
                    self.interested = b & super::mobs::fox::FLAG_INTERESTED != 0;
                }
            }
            K::Fox if i == index(fox::DATA_TYPE_ID) => {
                self.fox_variant = value.int().unwrap_or(self.fox_variant);
            }
            K::Armadillo if i == index(armadillo::ARMADILLO_STATE) => {
                self.armadillo_state = value.int().unwrap_or(self.armadillo_state);
            }
            K::PolarBear if i == index(polar_bear::DATA_STANDING_ID) => {
                self.polar_standing = value.bool().unwrap_or(self.polar_standing);
            }
            K::Camel if i == index(camel::LAST_POSE_CHANGE_TICK) => {
                self.camel_pose_tick = value.long().unwrap_or(self.camel_pose_tick);
            }
            K::Camel if i == index(camel::DASH) => {
                self.camel_dashing = value.bool().unwrap_or(self.camel_dashing);
            }
            K::Bee if i == index(bee::DATA_FLAGS_ID) => {
                self.bee_flags = value.byte().unwrap_or(self.bee_flags);
            }
            K::Bee if i == index(bee::DATA_ANGER_END_TIME) => {
                self.anger_end = value.long().unwrap_or(self.anger_end);
            }
            K::Goat if i == index(goat::DATA_HAS_LEFT_HORN) => {
                self.goat_left_horn = value.bool().unwrap_or(self.goat_left_horn);
            }
            K::Goat if i == index(goat::DATA_HAS_RIGHT_HORN) => {
                self.goat_right_horn = value.bool().unwrap_or(self.goat_right_horn);
            }
            K::Llama if i == index(llama::DATA_ID_CHEST) => {
                self.has_chest = value.bool().unwrap_or(self.has_chest);
            }
            K::Llama if i == index(llama::DATA_VARIANT_ID) => {
                self.llama_variant = value.int().unwrap_or(self.llama_variant);
            }
            K::TraderLlama if i == index(trader_llama::DATA_ID_CHEST) => {
                self.has_chest = value.bool().unwrap_or(self.has_chest);
            }
            K::TraderLlama if i == index(trader_llama::DATA_VARIANT_ID) => {
                self.llama_variant = value.int().unwrap_or(self.llama_variant);
            }
            K::Panda if i == index(panda::MAIN_GENE_ID) => {
                self.panda_gene = value.byte().unwrap_or(self.panda_gene);
            }
            K::Panda if i == index(panda::DATA_ID_FLAGS) => {
                self.panda_flags = value.byte().unwrap_or(self.panda_flags);
            }
            K::Parrot if i == index(parrot::DATA_VARIANT_ID) => {
                self.parrot_variant = value.int().unwrap_or(self.parrot_variant);
            }
            K::Parrot if i == index(parrot::DATA_FLAGS_ID) => {
                self.tamable_flags = value.byte().unwrap_or(self.tamable_flags);
            }
            K::Hoglin if i == index(hoglin::DATA_IMMUNE_TO_ZOMBIFICATION) => {
                self.immune_to_zombification = value.bool().unwrap_or(false);
            }
            K::Phantom if i == index(phantom::ID_SIZE) => {
                self.size = value.int().unwrap_or(self.size);
            }
            K::Vex if i == index(vex::DATA_FLAGS_ID) => {
                self.charging = value.byte().is_some_and(|b| b & 1 != 0);
            }
            K::Allay if i == index(allay::DATA_DANCING) => {
                self.dancing = value.bool().unwrap_or(self.dancing);
            }
            K::Strider if i == index(strider::DATA_SUFFOCATING) => {
                self.strider_cold = value.bool().unwrap_or(self.strider_cold);
            }
            K::Shulker if i == index(shulker::DATA_PEEK_ID) => {
                self.shulker_peek = value.byte().unwrap_or(self.shulker_peek);
            }
            K::Shulker if i == index(shulker::DATA_COLOR_ID) => {
                self.shulker_color = value.byte().unwrap_or(self.shulker_color);
            }
            K::Shulker if i == index(shulker::DATA_ATTACH_FACE_ID) => {
                self.attach_face = value.int().unwrap_or(self.attach_face);
            }
            K::Sniffer if i == index(sniffer::DATA_STATE) => {
                self.sniffer_state = value.int().unwrap_or(self.sniffer_state);
            }
            K::Creaking if i == index(creaking::IS_ACTIVE) => {
                self.creaking_active = value.bool().unwrap_or(self.creaking_active);
            }
            K::CopperGolem if i == index(copper_golem::COPPER_GOLEM_STATE) => {
                self.copper_state = value.int().unwrap_or(self.copper_state);
            }
            K::CopperGolem if i == index(copper_golem::DATA_WEATHER_STATE) => {
                self.copper_weather = value.int().unwrap_or(self.copper_weather);
            }
            K::Warden if i == index(warden::CLIENT_ANGER_LEVEL) => {
                self.warden_anger = value.int().unwrap_or(self.warden_anger);
            }
            K::Wither if i == index(wither::DATA_ID_INV) => {
                self.wither_invul = value.int().unwrap_or(self.wither_invul);
            }
            K::CamelHusk if i == index(camel_husk::LAST_POSE_CHANGE_TICK) => {
                self.camel_pose_tick = value.long().unwrap_or(self.camel_pose_tick);
            }
            K::CamelHusk if i == index(camel_husk::DASH) => {
                self.camel_dashing = value.bool().unwrap_or(self.camel_dashing);
            }
            K::Bogged if i == index(bogged::DATA_SHEARED) => {
                if value.bool() == Some(true) {
                    self.wool |= WOOL_SHEARED;
                } else if value.bool() == Some(false) {
                    self.wool &= !WOOL_SHEARED;
                }
            }
            K::Pillager if i == index(pillager::IS_CHARGING_CROSSBOW) => {
                self.charging_crossbow = value.bool().unwrap_or(self.charging_crossbow);
            }
            K::Piglin if i == index(piglin::DATA_IS_CHARGING_CROSSBOW) => {
                self.charging_crossbow = value.bool().unwrap_or(self.charging_crossbow);
            }
            K::Piglin if i == index(piglin::DATA_IS_DANCING) => {
                self.dancing = value.bool().unwrap_or(self.dancing);
            }
            K::SulfurCube if i == index(sulfur_cube::ID_SIZE) => {
                if let Some(s) = value.int() {
                    self.size = s.clamp(1, 127);
                }
            }
            K::SulfurCube if i == index(sulfur_cube::MAX_FUSE) => {
                self.thing.max_fuse = value.int().unwrap_or(self.thing.max_fuse);
            }
            K::WitherSkull if i == index(wither_skull::DATA_DANGEROUS) => {
                self.thing.dangerous = value.bool().unwrap_or(self.thing.dangerous);
            }
            K::Arrow | K::SpectralArrow | K::Trident if i == index(abstract_arrow::IN_GROUND) => {
                self.thing.in_ground = value.bool().unwrap_or(self.thing.in_ground);
            }
            K::Arrow if i == index(arrow::ID_EFFECT_COLOR) => {
                self.thing.arrow_color = value.int().unwrap_or(self.thing.arrow_color);
            }
            K::Trident if i == index(thrown_trident::ID_FOIL) => {
                self.thing.foil = value.bool().unwrap_or(self.thing.foil);
            }
            K::EndCrystal if i == index(end_crystal::DATA_BEAM_TARGET) => {
                if let Some(target) = value.block_pos() {
                    self.thing.beam_target = target;
                }
            }
            K::EndCrystal if i == index(end_crystal::DATA_SHOW_BOTTOM) => {
                self.thing.show_bottom = value.bool().unwrap_or(self.thing.show_bottom);
            }
            K::ExperienceOrb if i == index(experience_orb::DATA_VALUE) => {
                self.thing.xp_value = value.int().unwrap_or(self.thing.xp_value);
            }
            K::FireworkRocket if i == index(firework_rocket::DATA_SHOT_AT_ANGLE) => {
                self.thing.shot_at_angle = value.bool().unwrap_or(self.thing.shot_at_angle);
            }
            K::Cushion if i == index(cushion::DATA_COLOR) => {
                self.thing.color = value
                    .int()
                    .map_or(self.thing.color, |c| c.clamp(0, 15) as u8);
            }
            K::ZombieNautilus if i == index(zombie_nautilus::DATA_VARIANT_ID) => {
                self.thing.variant = value.int().unwrap_or(self.thing.variant);
            }
            K::Painting if i == index(painting::DATA_DIRECTION) => {
                self.thing.direction = value.int().unwrap_or(self.thing.direction);
            }
            K::Painting if i == index(painting::DATA_PAINTING_VARIANT_ID) => {
                self.thing.variant = value.int().unwrap_or(self.thing.variant);
            }
            K::Slime | K::MagmaCube if i == index(slime::ID_SIZE) => {
                if let Some(s) = value.int() {
                    self.size = s.clamp(1, 127);
                }
            }
            K::Sheep if i == index(sheep::DATA_WOOL_ID) => {
                if let Some(b) = value.byte() {
                    self.wool = b;
                }
            }
            K::Creeper if i == index(creeper::DATA_SWELL_DIR) => {
                if let Some(d) = value.int() {
                    self.swell_dir = d;
                }
            }
            K::Rabbit if i == index(rabbit::DATA_TYPE_ID) => {
                self.rabbit_variant = value.int().unwrap_or(self.rabbit_variant);
            }
            K::Bat if i == index(bat::DATA_ID_FLAGS) => {
                self.resting = value
                    .byte()
                    .is_some_and(|flags| flags & super::mobs::bat::FLAG_RESTING != 0);
            }
            K::Enderman if i == index(enderman::DATA_CARRY_STATE) => {
                self.block_state = value.int().and_then(|s| u16::try_from(s).ok()).unwrap_or(0);
            }
            K::Boat | K::Minecart if i == index(boat::DATA_ID_HURT) => {
                self.vehicle_hurt = value.int().unwrap_or(0);
            }
            K::Boat | K::Minecart if i == index(boat::DATA_ID_HURTDIR) => {
                self.vehicle_hurt_dir = value.int().unwrap_or(1);
            }
            K::Boat | K::Minecart if i == index(boat::DATA_ID_DAMAGE) => {
                self.vehicle_damage = value.float().unwrap_or(0.0);
            }
            K::Minecart if i == index(abstract_minecart::DATA_ID_CUSTOM_DISPLAY_BLOCK) => {
                // A block state id, 0 for none (the type's own then).
                self.thing.display_block = value.int().unwrap_or(0);
            }
            K::Minecart if i == index(abstract_minecart::DATA_ID_DISPLAY_OFFSET) => {
                self.thing.display_offset = value.int();
            }
            K::Minecart if i == index(minecart_furnace::DATA_ID_FUEL) => {
                self.thing.fuel = value.bool().unwrap_or(self.thing.fuel);
            }
            K::Boat if i == index(boat::DATA_ID_PADDLE_LEFT) => {
                self.paddles.0 = value.bool().unwrap_or(false);
            }
            K::Boat if i == index(boat::DATA_ID_PADDLE_RIGHT) => {
                self.paddles.1 = value.bool().unwrap_or(false);
            }
            K::PrimedTnt if i == index(primed_tnt::DATA_FUSE_ID) => {
                self.fuse = value.int().unwrap_or(self.fuse);
            }
            K::PrimedTnt if i == index(primed_tnt::DATA_BLOCK_STATE_ID) => {
                self.block_state = value.int().and_then(|s| u16::try_from(s).ok()).unwrap_or(0);
            }
            K::Enderman if i == index(enderman::DATA_CREEPY) => {
                self.creepy = value.bool().unwrap_or(self.creepy);
            }
            K::Ghast if i == index(ghast::DATA_IS_CHARGING) => {
                self.charging = value.bool().unwrap_or(self.charging);
            }
            K::Axolotl if i == index(axolotl::DATA_VARIANT) => {
                self.axolotl_variant = value.int().unwrap_or(self.axolotl_variant);
            }
            K::Axolotl if i == index(axolotl::DATA_PLAYING_DEAD) => {
                self.playing_dead = value.bool().unwrap_or(self.playing_dead);
            }
            K::Turtle if i == index(turtle::HAS_EGG) => {
                self.has_egg = value.bool().unwrap_or(self.has_egg);
            }
            K::Turtle if i == index(turtle::LAYING_EGG) => {
                self.laying_egg = value.bool().unwrap_or(self.laying_egg);
            }
            K::Guardian | K::ElderGuardian if i == index(guardian::DATA_ID_MOVING) => {
                self.guardian_moving = value.bool().unwrap_or(self.guardian_moving);
            }
            K::Salmon if i == index(salmon::DATA_TYPE) => {
                self.salmon_variant = value.int().unwrap_or(self.salmon_variant);
            }
            K::Pufferfish if i == index(pufferfish::PUFF_STATE) => {
                self.puff_state = value.int().unwrap_or(self.puff_state);
            }
            K::TropicalFish if i == index(tropical_fish::DATA_ID_TYPE_VARIANT) => {
                self.tropical_variant = value.int().unwrap_or(self.tropical_variant);
            }
            K::GlowSquid if i == index(glow_squid::DATA_DARK_TICKS_REMAINING) => {
                self.dark_ticks = value.int().unwrap_or(self.dark_ticks);
            }
            K::SnowGolem if i == index(snow_golem::DATA_PUMPKIN_ID) => {
                self.pumpkin = value.byte().is_none_or(|flags| flags & 16 != 0);
            }
            K::Creeper if i == index(creeper::DATA_IS_POWERED) => {
                self.powered = value.bool().unwrap_or(self.powered);
            }
            K::Creeper if i == index(creeper::DATA_IS_IGNITED) => {
                self.ignited = value.bool().unwrap_or(self.ignited);
            }
            _ => {}
        }
    }

    /// An enderman holds a block.
    pub fn carrying(&self) -> bool {
        self.block_state != 0
    }

    pub fn stand_small(&self) -> bool {
        self.stand_flags & STAND_SMALL != 0
    }

    pub fn stand_marker(&self) -> bool {
        self.stand_flags & STAND_MARKER != 0
    }

    pub fn crouching(&self) -> bool {
        self.pose == POSE_CROUCHING || self.shared_flags & SHARED_CROUCHING != 0
    }

    /// `Entity.isVisuallySwimming`.
    pub fn visually_swimming(&self) -> bool {
        self.pose == POSE_SWIMMING || self.shared_flags & SHARED_SWIMMING != 0
    }

    /// `LivingEntity.isUsingItem`, and the hand if so.
    pub fn using_hand(&self) -> Option<Hand> {
        let off_hand = self.living_flags & LIVING_OFF_HAND != 0;
        (self.living_flags & LIVING_USING_ITEM != 0).then_some(if off_hand {
            Hand::Off
        } else {
            Hand::Main
        })
    }

    /// `AbstractPiglin.isConverting`, given whether the dimension zombifies.
    pub fn piglin_converting(&self, dimension_zombifies: bool) -> bool {
        dimension_zombifies && !self.immune_to_zombification && self.mob_flags & MOB_NO_AI == 0
    }

    /// `LivingEntityRenderer.isShaking`.
    pub fn shaking(&self) -> bool {
        self.converting || self.zombifying || self.frozen_ticks >= FROZEN_TICKS_TO_FREEZE
    }

    /// `AbstractHorse.isEating`, `isStanding` and the open mouth.
    pub fn horse_eating(&self) -> bool {
        self.horse_flags & 16 != 0
    }

    pub fn horse_rearing(&self) -> bool {
        self.horse_flags & 32 != 0
    }

    pub fn horse_mouth_open(&self) -> bool {
        self.horse_flags & 64 != 0
    }

    pub fn sprinting(&self) -> bool {
        self.shared_flags & SHARED_SPRINTING != 0
    }

    /// `TamableAnimal.isInSittingPose`.
    pub fn sitting(&self) -> bool {
        self.tamable_flags & TAMABLE_SITTING != 0
    }

    /// `TamableAnimal.isTame`.
    pub fn tame(&self) -> bool {
        self.tamable_flags & TAMABLE_TAME != 0
    }

    pub fn collar_color(&self) -> DyeColor {
        DyeColor::by_id(self.collar).unwrap_or(DyeColor::Red)
    }

    pub fn invisible(&self) -> bool {
        self.shared_flags & SHARED_INVISIBLE != 0
    }

    /// `Entity.isCurrentlyGlowing` on the client: the glowing flag.
    pub fn glowing(&self) -> bool {
        self.shared_flags & SHARED_GLOWING != 0
    }

    /// `Mob.isAggressive`: zombies raise their arms higher, skeletons draw.
    pub fn aggressive(&self) -> bool {
        self.mob_flags & MOB_AGGRESSIVE != 0
    }

    /// `getMainArm`: a mob's from its flags, a player's from its data.
    pub fn main_arm(&self, kind: EntityKind) -> Arm {
        if kind.is_avatar() {
            self.player_main_arm
        } else if self.mob_flags & MOB_LEFT_HANDED != 0 {
            Arm::Left
        } else {
            Arm::Right
        }
    }

    /// `LivingEntity.isDeadOrDying`.
    pub fn dying(&self) -> bool {
        self.health <= 0.0
    }

    pub fn wool_color(&self) -> DyeColor {
        DyeColor::by_id(self.wool & WOOL_COLOR).unwrap_or(DyeColor::White)
    }

    pub fn sheared(&self) -> bool {
        self.wool & WOOL_SHEARED != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wool_baby_and_variant_by_kind() {
        let mut a = Appearance::default();
        a.apply(EntityKind::Sheep, 18, &DataValue::Byte(0x15));
        assert!(a.sheared());
        assert_eq!(a.wool_color(), DyeColor::Lime);
        a.apply(EntityKind::Sheep, 16, &DataValue::Bool(true));
        assert!(a.baby);
        let mut pig = Appearance::default();
        pig.apply(EntityKind::Pig, 19, &DataValue::Int(0));
        assert_eq!(pig.variant, 0);
        let mut creeper = Appearance::default();
        creeper.apply(EntityKind::Creeper, 16, &DataValue::Int(1));
        assert_eq!((creeper.swell_dir, creeper.variant), (1, 1));
    }

    /// Each kind's baby flag and variant sit at the index of its own class in the data.
    #[test]
    fn every_baby_capable_kind_reads_its_own_classes_baby_flag() {
        use EntityKind as K;
        use pumpkin_data::tracked_data as t;
        let table = [
            (K::Pig, index(t::pig::DATA_BABY_ID)),
            (K::Rabbit, index(t::rabbit::DATA_BABY_ID)),
            (K::Cow, index(t::cow::DATA_BABY_ID)),
            (K::Mooshroom, index(t::mooshroom::DATA_BABY_ID)),
            (K::Wolf, index(t::wolf::DATA_BABY_ID)),
            (K::Horse, index(t::horse::DATA_BABY_ID)),
            (K::Donkey, index(t::donkey::DATA_BABY_ID)),
            (K::Mule, index(t::mule::DATA_BABY_ID)),
            (K::SkeletonHorse, index(t::skeleton_horse::DATA_BABY_ID)),
            (K::ZombieHorse, index(t::zombie_horse::DATA_BABY_ID)),
            (K::Cat, index(t::cat::DATA_BABY_ID)),
            (K::Ocelot, index(t::ocelot::DATA_BABY_ID)),
            (K::Sheep, index(t::sheep::DATA_BABY_ID)),
            (K::Chicken, index(t::chicken::DATA_BABY_ID)),
            (K::Zombie, index(t::zombie::DATA_BABY_ID)),
            (K::Husk, index(t::husk::DATA_BABY_ID)),
            (K::Drowned, index(t::drowned::DATA_BABY_ID)),
            (K::Piglin, index(t::piglin::DATA_BABY_ID)),
            (K::ZombifiedPiglin, index(t::zombified_piglin::DATA_BABY_ID)),
            (K::Villager, index(t::villager::DATA_BABY_ID)),
            (K::ZombieVillager, index(t::zombie_villager::DATA_BABY_ID)),
            (K::HappyGhast, index(t::happy_ghast::DATA_BABY_ID)),
            (K::Squid, index(t::squid::DATA_BABY_ID)),
            (K::Dolphin, index(t::dolphin::DATA_BABY_ID)),
            (K::Turtle, index(t::turtle::DATA_BABY_ID)),
            (K::Nautilus, index(t::nautilus::DATA_BABY_ID)),
            (K::Axolotl, index(t::axolotl::DATA_BABY_ID)),
            (K::GlowSquid, index(t::glow_squid::DATA_BABY_ID)),
            (K::Fox, index(t::fox::DATA_BABY_ID)),
            (K::Camel, index(t::camel::DATA_BABY_ID)),
            (K::Armadillo, index(t::armadillo::DATA_BABY_ID)),
            (K::PolarBear, index(t::polar_bear::DATA_BABY_ID)),
            (K::Bee, index(t::bee::DATA_BABY_ID)),
            (K::Goat, index(t::goat::DATA_BABY_ID)),
            (K::Llama, index(t::llama::DATA_BABY_ID)),
            (K::TraderLlama, index(t::trader_llama::DATA_BABY_ID)),
            (K::Panda, index(t::panda::DATA_BABY_ID)),
            (K::Hoglin, index(t::hoglin::DATA_BABY_ID)),
            (K::Zoglin, index(t::zoglin::DATA_BABY_ID)),
            (K::Strider, index(t::strider::DATA_BABY_ID)),
            (K::Sniffer, index(t::sniffer::DATA_BABY_ID)),
            (K::CamelHusk, index(t::camel_husk::DATA_BABY_ID)),
        ];
        for (kind, at) in table {
            assert!(kind.can_be_baby(), "{kind:?}");
            let mut a = Appearance::default();
            a.apply(kind, at, &DataValue::Bool(true));
            assert!(a.baby, "{kind:?} at {at}");
        }
    }

    #[test]
    fn health_at_zero_is_dying() {
        let mut a = Appearance::default();
        assert!(!a.dying());
        a.apply(EntityKind::Zombie, 9, &DataValue::Float(0.0));
        assert!(a.dying());
    }
}

/// The synced data of the things that are not mobs: projectiles, orbs,
/// crystals, paintings (and the sulfur cube's fuse, the zombie nautilus's
/// variant, which share the fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThingLook {
    /// `WitherSkull.DATA_DANGEROUS`: the blue skull.
    pub dangerous: bool,
    /// `AbstractArrow.IN_GROUND`.
    pub in_ground: bool,
    /// `Arrow.ID_EFFECT_COLOR`: above 0 for a tipped arrow.
    pub arrow_color: i32,
    /// `ThrownTrident.ID_FOIL`.
    pub foil: bool,
    /// `EndCrystal.DATA_BEAM_TARGET` and `DATA_SHOW_BOTTOM`.
    pub beam_target: Option<[i32; 3]>,
    pub show_bottom: bool,
    /// `ExperienceOrb.DATA_VALUE`.
    pub xp_value: i32,
    /// `FireworkRocketEntity.DATA_SHOT_AT_ANGLE`.
    pub shot_at_angle: bool,
    /// `Cushion.DATA_COLOR`, a dye id.
    pub color: u8,
    /// `SulfurCube.MAX_FUSE`: the fuse it was lit with, -1 unlit.
    pub max_fuse: i32,
    /// A zombie nautilus's or painting's variant (registry id).
    pub variant: i32,
    /// `Painting.DATA_DIRECTION` (`Direction.get3DDataValue`).
    pub direction: i32,
    /// `AbstractMinecart.DATA_ID_CUSTOM_DISPLAY_BLOCK` (0 for the type's
    /// own) and `DATA_ID_DISPLAY_OFFSET` (`None` for the type's default).
    pub display_block: i32,
    pub display_offset: Option<i32>,
    /// `MinecartFurnace.DATA_ID_FUEL`: its furnace is lit.
    pub fuel: bool,
}

impl Default for ThingLook {
    fn default() -> Self {
        Self {
            dangerous: false,
            in_ground: false,
            arrow_color: -1,
            foil: false,
            beam_target: None,
            show_bottom: true,
            xp_value: 0,
            shot_at_angle: false,
            color: 0,
            max_fuse: -1,
            variant: 0,
            direction: 2,
            display_block: 0,
            display_offset: None,
            fuel: false,
        }
    }
}
