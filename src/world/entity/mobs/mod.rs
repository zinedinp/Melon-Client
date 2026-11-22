//! The mobs Melon draws.

pub mod allay;
pub mod armadillo;
pub mod armor_stand;
pub mod axolotl;
pub mod bat;
pub mod bee;
pub mod blaze;
pub mod breeze;
pub mod camel;
pub mod chicken;
pub mod copper_golem;
pub mod cow;
pub mod creaking;
pub mod creeper;
pub mod custom_head;
pub mod dolphin;
pub mod dragon;
pub mod enderman;
pub mod feline;
pub mod fish;
pub mod fox;
pub mod frog;
pub mod ghast;
pub mod goat;
pub mod golem;
pub mod guardian;
pub mod hoglin;
pub mod horse;
pub mod illager;
pub mod leash_knot;
pub mod llama;
pub mod mouth_item;
pub mod nautilus;
pub mod panda;
pub mod parrot;
pub mod phantom;
pub mod pig;
pub mod piglin;
pub mod polar_bear;
pub mod projectile;
pub mod rabbit;
pub mod ravager;
pub mod segmented;
pub mod sheep;
pub mod shulker;
pub mod skeleton;
pub mod slime;
pub mod sniffer;
pub mod snow_golem;
pub mod spider;
pub mod squid;
pub mod strider;
pub mod sulfur_cube;
pub mod tnt;
pub mod turtle;
pub mod vehicle;
pub mod vex;
pub mod villager;
pub mod warden;
pub mod wither;
pub mod wolf;
pub mod zombie;

mod equipment;
mod layers;
mod placement;
mod pose;

pub use equipment::*;
pub use layers::*;
pub use placement::*;
pub use pose::*;

use std::borrow::Cow;

use glam::Vec3;
use pumpkin_data::data_component_impl::{ChargedProjectilesImpl, EquippableImpl, TrimImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::Taggable;

use super::humanoid::{
    self, ArmPose, ArmorSlot, AttackStyle, HumanoidParts, HumanoidRenderState, SwingPose,
};
use super::keyframe::{AnimationDefinition, AnimationState, KeyframeAnimation, KeyframeSet};
use super::kind::EntityKind;
use super::living::{LivingRenderState, Placement};
use super::model::{LayerDefinition, Model, ModelPose, PartId};
use super::player::Arm;
use super::quadruped::{self, QuadrupedParts};
use super::tracker::{ClientEntity, Equipment};
use crate::network::play::game::EquipmentSlot;

/// Every baked mob model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelId {
    Pig,
    PigCold,
    PigBaby,
    Cow,
    CowCold,
    CowWarm,
    CowBaby,
    Sheep,
    SheepBaby,
    SheepFur,
    Chicken,
    ChickenCold,
    ChickenBaby,
    Creeper,
    Zombie,
    ZombieBaby,
    Drowned,
    DrownedOuter,
    DrownedBabyOuter,
    Piglin,
    PiglinBaby,
    Villager,
    VillagerNoHat,
    VillagerBaby,
    VillagerBabyNoHat,
    Witch,
    Illager,
    Wolf,
    WolfBaby,
    Feline,
    FelineBaby,
    FelineCollar,
    FelineBabyCollar,
    Horse,
    HorseBaby,
    Donkey,
    DonkeyBaby,
    ZombieVillager,
    ZombieVillagerNoHat,
    ZombieVillagerBaby,
    ZombieVillagerBabyNoHat,
    Skeleton,
    /// `SkeletonModel.createSingleModelDualBodyLayer`.
    Parched,
    /// `STRAY_OUTER_LAYER`: the clothes, a humanoid grown by a quarter pixel.
    StrayClothes,
    Spider,
    Bat,
    Rabbit,
    RabbitBaby,
    Endermite,
    Silverfish,
    IronGolem,
    Enderman,
    Blaze,
    SnowGolem,
    Ghast,
    HappyGhast,
    HappyGhastBaby,
    HappyGhastHarness,
    HappyGhastBabyHarness,
    Squid,
    SquidBaby,
    Cod,
    Salmon,
    SalmonSmall,
    SalmonLarge,
    PufferBig,
    PufferMid,
    PufferSmall,
    TropicalSmall,
    TropicalSmallPattern,
    TropicalLarge,
    TropicalLargePattern,
    Dolphin,
    DolphinBaby,
    Turtle,
    TurtleBaby,
    Guardian,
    ElderGuardian,
    Tadpole,
    Nautilus,
    NautilusBaby,
    Axolotl,
    AxolotlBaby,
    Fox,
    FoxBaby,
    Frog,
    Camel,
    CamelBaby,
    Armadillo,
    ArmadilloBaby,
    PolarBear,
    PolarBearBaby,
    Bogged,
    BoggedClothes,
    SlimeInner,
    SlimeOuter,
    MagmaCube,
    LeashKnot,
    /// `ArmorStandModel`, full and small.
    ArmorStand,
    ArmorStandSmall,
    Boat,
    ChestBoat,
    Raft,
    ChestRaft,
    /// `BoatModel.createWaterPatch`.
    BoatWaterPatch,
    Minecart,
    Bee,
    BeeBaby,
    Goat,
    GoatBaby,
    Llama,
    LlamaBaby,
    LlamaDecor,
    LlamaBabyDecor,
    Panda,
    PandaBaby,
    Parrot,
    Hoglin,
    HoglinBaby,
    Phantom,
    Vex,
    Allay,
    Strider,
    StriderBaby,
    Ravager,
    Wither,
    WitherArmor,
    Dragon,
    Shulker,
    Sniffer,
    SnifferBaby,
    Warden,
    Breeze,
    BreezeWind,
    Creaking,
    CopperGolem,
    HorseSaddle,
    HorseArmor,
    WolfArmor,
    PigSaddle,
    CamelSaddle,
    NautilusSaddle,
    Armor(ArmorStyle, ArmorSlot),
    BabyArmor(ArmorStyle, ArmorSlot),
    SulfurCubeOuter,
    SulfurCubeInner,
    SulfurCubeSmallOuter,
    SulfurCubeSmallInner,
    /// `ZombieNautilusCoralModel`: the warm one's corals on the shell.
    ZombieNautilusCoral,
    WitherSkull,
    ShulkerBullet,
    /// The bullet again, half as big again, for its translucent glow.
    ShulkerBulletGlow,
    LlamaSpit,
    WindCharge,
    EvokerFangs,
    EndCrystal,
    Trident,
    Arrow,
    Cushion,
    /// `CREEPER_ARMOR`.
    CreeperArmor,
    NautilusArmor,
    /// The warden's emissive layers.
    WardenTendrils,
    WardenHeart,
    WardenBioluminescent,
    WardenPulsatingSpots,
}

/// Whose armor models a wearer has: the models differ in their boxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArmorStyle {
    Humanoid,
    /// `ZombieVillagerModel.createArmorLayerSet`: the head of a villager.
    ZombieVillager,
    /// `PiglinModel.createArmorMeshSet` (outer grown by 1.02).
    Piglin,
    /// `ArmorStandArmorModel.createArmorLayerSet`, full and small.
    ArmorStand,
}

impl ModelId {
    pub const ALL: [Self; 191] = {
        use ArmorSlot::{Chest, Feet, Head, Legs};
        use ArmorStyle::{Humanoid, Piglin, ZombieVillager};
        [
            Self::Pig,
            Self::PigCold,
            Self::PigBaby,
            Self::Cow,
            Self::CowCold,
            Self::CowWarm,
            Self::CowBaby,
            Self::Sheep,
            Self::SheepBaby,
            Self::SheepFur,
            Self::Chicken,
            Self::ChickenCold,
            Self::ChickenBaby,
            Self::Creeper,
            Self::Zombie,
            Self::ZombieBaby,
            Self::Drowned,
            Self::DrownedOuter,
            Self::DrownedBabyOuter,
            Self::Piglin,
            Self::PiglinBaby,
            Self::Villager,
            Self::VillagerNoHat,
            Self::VillagerBaby,
            Self::VillagerBabyNoHat,
            Self::Witch,
            Self::Illager,
            Self::Wolf,
            Self::WolfBaby,
            Self::Feline,
            Self::FelineBaby,
            Self::FelineCollar,
            Self::FelineBabyCollar,
            Self::Horse,
            Self::HorseBaby,
            Self::Donkey,
            Self::DonkeyBaby,
            Self::ZombieVillager,
            Self::ZombieVillagerNoHat,
            Self::ZombieVillagerBaby,
            Self::ZombieVillagerBabyNoHat,
            Self::Skeleton,
            Self::StrayClothes,
            Self::Spider,
            Self::Bat,
            Self::Rabbit,
            Self::RabbitBaby,
            Self::Endermite,
            Self::Silverfish,
            Self::IronGolem,
            Self::Enderman,
            Self::Blaze,
            Self::SnowGolem,
            Self::Ghast,
            Self::HappyGhast,
            Self::HappyGhastBaby,
            Self::HappyGhastHarness,
            Self::HappyGhastBabyHarness,
            Self::Squid,
            Self::SquidBaby,
            Self::Cod,
            Self::Salmon,
            Self::SalmonSmall,
            Self::SalmonLarge,
            Self::PufferBig,
            Self::PufferMid,
            Self::PufferSmall,
            Self::TropicalSmall,
            Self::TropicalSmallPattern,
            Self::TropicalLarge,
            Self::TropicalLargePattern,
            Self::Dolphin,
            Self::DolphinBaby,
            Self::Turtle,
            Self::TurtleBaby,
            Self::Guardian,
            Self::ElderGuardian,
            Self::Tadpole,
            Self::Nautilus,
            Self::NautilusBaby,
            Self::Axolotl,
            Self::AxolotlBaby,
            Self::Fox,
            Self::FoxBaby,
            Self::Frog,
            Self::Camel,
            Self::CamelBaby,
            Self::Armadillo,
            Self::ArmadilloBaby,
            Self::PolarBear,
            Self::PolarBearBaby,
            Self::Bogged,
            Self::BoggedClothes,
            Self::SlimeInner,
            Self::SlimeOuter,
            Self::MagmaCube,
            Self::LeashKnot,
            Self::Boat,
            Self::ChestBoat,
            Self::Raft,
            Self::ChestRaft,
            Self::BoatWaterPatch,
            Self::Minecart,
            Self::Bee,
            Self::BeeBaby,
            Self::Goat,
            Self::GoatBaby,
            Self::Llama,
            Self::LlamaBaby,
            Self::LlamaDecor,
            Self::LlamaBabyDecor,
            Self::Panda,
            Self::PandaBaby,
            Self::Parrot,
            Self::Hoglin,
            Self::HoglinBaby,
            Self::Phantom,
            Self::Vex,
            Self::Allay,
            Self::Strider,
            Self::StriderBaby,
            Self::Ravager,
            Self::Wither,
            Self::WitherArmor,
            Self::Dragon,
            Self::Shulker,
            Self::Sniffer,
            Self::SnifferBaby,
            Self::Warden,
            Self::Breeze,
            Self::BreezeWind,
            Self::Creaking,
            Self::CopperGolem,
            Self::HorseSaddle,
            Self::HorseArmor,
            Self::WolfArmor,
            Self::PigSaddle,
            Self::CamelSaddle,
            Self::NautilusSaddle,
            Self::Armor(Humanoid, Head),
            Self::Armor(Humanoid, Chest),
            Self::Armor(Humanoid, Legs),
            Self::Armor(Humanoid, Feet),
            Self::Armor(ZombieVillager, Head),
            Self::Armor(ZombieVillager, Chest),
            Self::Armor(ZombieVillager, Legs),
            Self::Armor(ZombieVillager, Feet),
            Self::Armor(Piglin, Head),
            Self::Armor(Piglin, Chest),
            Self::Armor(Piglin, Legs),
            Self::Armor(Piglin, Feet),
            Self::BabyArmor(Humanoid, Head),
            Self::BabyArmor(Humanoid, Chest),
            Self::BabyArmor(Humanoid, Legs),
            Self::BabyArmor(Humanoid, Feet),
            Self::BabyArmor(Piglin, Head),
            Self::BabyArmor(Piglin, Chest),
            Self::BabyArmor(Piglin, Legs),
            Self::BabyArmor(Piglin, Feet),
            Self::ArmorStand,
            Self::ArmorStandSmall,
            Self::Armor(ArmorStyle::ArmorStand, Head),
            Self::Armor(ArmorStyle::ArmorStand, Chest),
            Self::Armor(ArmorStyle::ArmorStand, Legs),
            Self::Armor(ArmorStyle::ArmorStand, Feet),
            Self::BabyArmor(ArmorStyle::ArmorStand, Head),
            Self::BabyArmor(ArmorStyle::ArmorStand, Chest),
            Self::BabyArmor(ArmorStyle::ArmorStand, Legs),
            Self::BabyArmor(ArmorStyle::ArmorStand, Feet),
            Self::SulfurCubeOuter,
            Self::SulfurCubeInner,
            Self::SulfurCubeSmallOuter,
            Self::SulfurCubeSmallInner,
            Self::ZombieNautilusCoral,
            Self::WitherSkull,
            Self::ShulkerBullet,
            Self::LlamaSpit,
            Self::WindCharge,
            Self::EvokerFangs,
            Self::EndCrystal,
            Self::Trident,
            Self::Arrow,
            Self::Cushion,
            Self::ShulkerBulletGlow,
            Self::CreeperArmor,
            Self::NautilusArmor,
            Self::WardenTendrils,
            Self::WardenHeart,
            Self::WardenBioluminescent,
            Self::WardenPulsatingSpots,
            Self::Parched,
        ]
    };

    /// Both sides of its faces drawn: vanilla's `entityCutout` doesn't cull,
    /// which only shows where the texture has holes to see the inside
    /// through (the skeletons' ribs, the skeleton horse).
    pub const fn two_sided(self) -> bool {
        matches!(
            self,
            Self::Skeleton
                | Self::Parched
                | Self::StrayClothes
                | Self::Bogged
                | Self::BoggedClothes
                | Self::Horse
                | Self::HorseBaby
        )
    }

    /// Its position in [`Self::ALL`].
    pub const fn index(self) -> usize {
        match self {
            Self::Pig => 0,
            Self::PigCold => 1,
            Self::PigBaby => 2,
            Self::Cow => 3,
            Self::CowCold => 4,
            Self::CowWarm => 5,
            Self::CowBaby => 6,
            Self::Sheep => 7,
            Self::SheepBaby => 8,
            Self::SheepFur => 9,
            Self::Chicken => 10,
            Self::ChickenCold => 11,
            Self::ChickenBaby => 12,
            Self::Creeper => 13,
            Self::Zombie => 14,
            Self::ZombieBaby => 15,
            Self::Drowned => 16,
            Self::DrownedOuter => 17,
            Self::DrownedBabyOuter => 18,
            Self::Piglin => 19,
            Self::PiglinBaby => 20,
            Self::Villager => 21,
            Self::VillagerNoHat => 22,
            Self::VillagerBaby => 23,
            Self::VillagerBabyNoHat => 24,
            Self::Witch => 25,
            Self::Illager => 26,
            Self::Wolf => 27,
            Self::WolfBaby => 28,
            Self::Feline => 29,
            Self::FelineBaby => 30,
            Self::FelineCollar => 31,
            Self::FelineBabyCollar => 32,
            Self::Horse => 33,
            Self::HorseBaby => 34,
            Self::Donkey => 35,
            Self::DonkeyBaby => 36,
            Self::ZombieVillager => 37,
            Self::ZombieVillagerNoHat => 38,
            Self::ZombieVillagerBaby => 39,
            Self::ZombieVillagerBabyNoHat => 40,
            Self::Skeleton => 41,
            Self::StrayClothes => 42,
            Self::Spider => 43,
            Self::Bat => 44,
            Self::Rabbit => 45,
            Self::RabbitBaby => 46,
            Self::Endermite => 47,
            Self::Silverfish => 48,
            Self::IronGolem => 49,
            Self::Enderman => 50,
            Self::Blaze => 51,
            Self::SnowGolem => 52,
            Self::Ghast => 53,
            Self::HappyGhast => 54,
            Self::HappyGhastBaby => 55,
            Self::HappyGhastHarness => 56,
            Self::HappyGhastBabyHarness => 57,
            Self::Squid => 58,
            Self::SquidBaby => 59,
            Self::Cod => 60,
            Self::Salmon => 61,
            Self::SalmonSmall => 62,
            Self::SalmonLarge => 63,
            Self::PufferBig => 64,
            Self::PufferMid => 65,
            Self::PufferSmall => 66,
            Self::TropicalSmall => 67,
            Self::TropicalSmallPattern => 68,
            Self::TropicalLarge => 69,
            Self::TropicalLargePattern => 70,
            Self::Dolphin => 71,
            Self::DolphinBaby => 72,
            Self::Turtle => 73,
            Self::TurtleBaby => 74,
            Self::Guardian => 75,
            Self::ElderGuardian => 76,
            Self::Tadpole => 77,
            Self::Nautilus => 78,
            Self::NautilusBaby => 79,
            Self::Axolotl => 80,
            Self::AxolotlBaby => 81,
            Self::Fox => 82,
            Self::FoxBaby => 83,
            Self::Frog => 84,
            Self::Camel => 85,
            Self::CamelBaby => 86,
            Self::Armadillo => 87,
            Self::ArmadilloBaby => 88,
            Self::PolarBear => 89,
            Self::PolarBearBaby => 90,
            Self::Bogged => 91,
            Self::BoggedClothes => 92,
            Self::SlimeInner => 93,
            Self::SlimeOuter => 94,
            Self::MagmaCube => 95,
            Self::LeashKnot => 96,
            Self::Boat => 97,
            Self::ChestBoat => 98,
            Self::Raft => 99,
            Self::ChestRaft => 100,
            Self::BoatWaterPatch => 101,
            Self::Minecart => 102,
            Self::Bee => 103,
            Self::BeeBaby => 104,
            Self::Goat => 105,
            Self::GoatBaby => 106,
            Self::Llama => 107,
            Self::LlamaBaby => 108,
            Self::LlamaDecor => 109,
            Self::LlamaBabyDecor => 110,
            Self::Panda => 111,
            Self::PandaBaby => 112,
            Self::Parrot => 113,
            Self::Hoglin => 114,
            Self::HoglinBaby => 115,
            Self::Phantom => 116,
            Self::Vex => 117,
            Self::Allay => 118,
            Self::Strider => 119,
            Self::StriderBaby => 120,
            Self::Ravager => 121,
            Self::Wither => 122,
            Self::WitherArmor => 123,
            Self::Dragon => 124,
            Self::Shulker => 125,
            Self::Sniffer => 126,
            Self::SnifferBaby => 127,
            Self::Warden => 128,
            Self::Breeze => 129,
            Self::BreezeWind => 130,
            Self::Creaking => 131,
            Self::CopperGolem => 132,
            Self::HorseSaddle => 133,
            Self::HorseArmor => 134,
            Self::WolfArmor => 135,
            Self::PigSaddle => 136,
            Self::CamelSaddle => 137,
            Self::NautilusSaddle => 138,
            Self::ArmorStand => 159,
            Self::ArmorStandSmall => 160,
            Self::Armor(ArmorStyle::ArmorStand, slot) => 161 + slot as usize,
            Self::BabyArmor(ArmorStyle::ArmorStand, slot) => 165 + slot as usize,
            Self::Armor(style, slot) => 139 + armor_index(style, slot),
            Self::BabyArmor(style, slot) => 151 + baby_armor_index(style, slot),
            Self::SulfurCubeOuter => 169,
            Self::SulfurCubeInner => 170,
            Self::SulfurCubeSmallOuter => 171,
            Self::SulfurCubeSmallInner => 172,
            Self::ZombieNautilusCoral => 173,
            Self::WitherSkull => 174,
            Self::ShulkerBullet => 175,
            Self::LlamaSpit => 176,
            Self::WindCharge => 177,
            Self::EvokerFangs => 178,
            Self::EndCrystal => 179,
            Self::Trident => 180,
            Self::Arrow => 181,
            Self::Cushion => 182,
            Self::ShulkerBulletGlow => 183,
            Self::CreeperArmor => 184,
            Self::NautilusArmor => 185,
            Self::WardenTendrils => 186,
            Self::WardenHeart => 187,
            Self::WardenBioluminescent => 188,
            Self::WardenPulsatingSpots => 189,
            Self::Parched => 190,
        }
    }

    /// The keyframe animations of this model and the state slot driving each.
    pub fn animations(self) -> &'static [(&'static AnimationDefinition, usize)] {
        match self {
            Self::Bat => &bat::ANIMATIONS,
            Self::Rabbit => &rabbit::ANIMATIONS,
            Self::RabbitBaby => &rabbit::BABY_ANIMATIONS,
            Self::AxolotlBaby => &axolotl::BABY_ANIMATIONS,
            Self::Frog => &frog::ANIMATIONS,
            Self::Camel | Self::CamelSaddle => &camel::ANIMATIONS,
            Self::CamelBaby => &camel::BABY_ANIMATIONS,
            Self::Armadillo => &armadillo::ANIMATIONS,
            Self::ArmadilloBaby => &armadillo::BABY_ANIMATIONS,
            Self::Sniffer | Self::SnifferBaby => &sniffer::ANIMATIONS,
            Self::Warden
            | Self::WardenTendrils
            | Self::WardenHeart
            | Self::WardenBioluminescent
            | Self::WardenPulsatingSpots => &warden::ANIMATIONS,
            Self::Breeze | Self::BreezeWind => &breeze::ANIMATIONS,
            Self::Creaking => &creaking::ANIMATIONS,
            Self::CopperGolem => &copper_golem::ANIMATIONS,
            _ => &[],
        }
    }

    /// The model's `LayerDefinition`.
    pub fn layer(self) -> LayerDefinition {
        match self {
            Self::Pig => pig::adult(),
            Self::PigCold => pig::cold(),
            Self::PigBaby => pig::baby(),
            Self::Cow => cow::adult(),
            Self::CowCold => cow::cold(),
            Self::CowWarm => cow::warm(),
            Self::CowBaby => cow::baby(),
            Self::Sheep => sheep::adult(),
            Self::SheepBaby => sheep::baby(),
            Self::SheepFur => sheep::fur(),
            Self::Chicken => chicken::adult(),
            Self::ChickenCold => chicken::cold(),
            Self::ChickenBaby => chicken::baby(),
            Self::Creeper => creeper::layer(),
            Self::Zombie => zombie::adult(),
            Self::ZombieBaby => zombie::baby(),
            Self::Drowned => zombie::drowned(0.0),
            Self::DrownedOuter => zombie::drowned(0.25),
            Self::DrownedBabyOuter => zombie::baby_grown(0.25),
            Self::Piglin => piglin::adult(),
            Self::PiglinBaby => piglin::baby(),
            Self::Villager => villager::adult(),
            Self::VillagerNoHat => villager::adult_no_hat(),
            Self::VillagerBaby => villager::baby(),
            Self::VillagerBabyNoHat => villager::baby_no_hat(),
            Self::Witch => villager::witch(),
            Self::Illager => illager::layer(),
            Self::Wolf => wolf::adult(),
            Self::WolfBaby => wolf::baby(),
            Self::Feline => feline::adult(0.0),
            Self::FelineBaby => feline::baby(),
            Self::FelineCollar => feline::adult(0.01),
            Self::FelineBabyCollar => feline::baby_grown(0.05),
            Self::Horse => horse::horse(),
            Self::HorseBaby => horse::baby_horse(),
            Self::Donkey => horse::donkey(),
            Self::DonkeyBaby => horse::baby_donkey(),
            Self::ZombieVillager => villager::zombie_villager(),
            Self::ZombieVillagerNoHat => villager::zombie_villager_no_hat(),
            Self::ZombieVillagerBaby => villager::baby_zombie_villager(),
            Self::ZombieVillagerBabyNoHat => villager::baby_zombie_villager_no_hat(),
            Self::Skeleton => skeleton::layer(),
            Self::Parched => skeleton::parched(),
            Self::StrayClothes => skeleton::clothes(),
            Self::Spider => spider::layer(),
            Self::Bat => bat::layer(),
            Self::Rabbit => rabbit::adult(),
            Self::RabbitBaby => rabbit::baby(),
            Self::Endermite => segmented::endermite(),
            Self::Silverfish => segmented::silverfish(),
            Self::IronGolem => golem::layer(),
            Self::Enderman => enderman::layer(),
            Self::Blaze => blaze::layer(),
            Self::SnowGolem => snow_golem::layer(),
            Self::Ghast => ghast::layer(),
            Self::HappyGhast => ghast::happy(false),
            Self::HappyGhastBaby => ghast::happy(true),
            Self::HappyGhastHarness => ghast::harness(false),
            Self::HappyGhastBabyHarness => ghast::harness(true),
            Self::Squid => squid::layer(false),
            Self::SquidBaby => squid::layer(true),
            Self::Cod => fish::cod(),
            Self::Salmon => fish::salmon(1.0),
            Self::SalmonSmall => fish::salmon(0.5),
            Self::SalmonLarge => fish::salmon(1.5),
            Self::PufferBig => fish::pufferfish_big(),
            Self::PufferMid => fish::pufferfish_mid(),
            Self::PufferSmall => fish::pufferfish_small(),
            Self::TropicalSmall => fish::tropical_small(0.0),
            Self::TropicalSmallPattern => fish::tropical_small(fish::PATTERN_GROW),
            Self::TropicalLarge => fish::tropical_large(0.0),
            Self::TropicalLargePattern => fish::tropical_large(fish::PATTERN_GROW),
            Self::Dolphin => dolphin::adult(),
            Self::DolphinBaby => dolphin::baby(),
            Self::Turtle => turtle::adult(),
            Self::TurtleBaby => turtle::baby(),
            Self::Guardian => guardian::layer(false),
            Self::ElderGuardian => guardian::layer(true),
            Self::Tadpole => fish::tadpole(),
            Self::Nautilus => nautilus::adult(),
            Self::NautilusBaby => nautilus::baby(),
            Self::Axolotl => axolotl::adult(),
            Self::AxolotlBaby => axolotl::baby(),
            Self::Fox => fox::adult(),
            Self::FoxBaby => fox::baby(),
            Self::Frog => frog::layer(),
            Self::Camel => camel::adult(),
            Self::CamelBaby => camel::baby(),
            Self::Armadillo => armadillo::adult(),
            Self::ArmadilloBaby => armadillo::baby(),
            Self::PolarBear => polar_bear::adult(),
            Self::PolarBearBaby => polar_bear::baby(),
            Self::Bogged => skeleton::bogged(),
            Self::BoggedClothes => skeleton::bogged_clothes(),
            Self::SlimeInner => slime::inner(),
            Self::SlimeOuter => slime::outer(),
            Self::MagmaCube => slime::magma_cube(),
            Self::LeashKnot => leash_knot::layer(),
            Self::ArmorStand => armor_stand::body(),
            Self::ArmorStandSmall => armor_stand::small_body(),
            Self::Boat => vehicle::boat(false),
            Self::ChestBoat => vehicle::boat(true),
            Self::Raft => vehicle::raft(false),
            Self::ChestRaft => vehicle::raft(true),
            Self::BoatWaterPatch => vehicle::water_patch(),
            Self::Minecart => vehicle::minecart(),
            Self::Bee => bee::adult(),
            Self::BeeBaby => bee::baby(),
            Self::Goat => goat::adult(),
            Self::GoatBaby => goat::baby(),
            Self::Llama => llama::adult(),
            Self::LlamaBaby => llama::baby(),
            Self::LlamaDecor => llama::decor(),
            Self::LlamaBabyDecor => llama::baby_decor(),
            Self::Panda => panda::adult(),
            Self::PandaBaby => panda::baby(),
            Self::Parrot => parrot::layer(),
            Self::Hoglin => hoglin::adult(),
            Self::HoglinBaby => hoglin::baby(),
            Self::Phantom => phantom::layer(),
            Self::Vex => vex::layer(),
            Self::Allay => allay::layer(),
            Self::Strider => strider::adult(),
            Self::StriderBaby => strider::baby(),
            Self::Ravager => ravager::layer(),
            Self::Wither => wither::layer(),
            Self::WitherArmor => wither::armor_layer(),
            Self::Dragon => dragon::layer(),
            Self::Shulker => shulker::layer(),
            Self::Sniffer => sniffer::adult(),
            Self::SnifferBaby => sniffer::baby(),
            Self::Warden => warden::layer(),
            Self::Breeze => breeze::body(),
            Self::BreezeWind => breeze::wind(),
            Self::Creaking => creaking::layer(),
            Self::CopperGolem => copper_golem::layer(),
            Self::HorseSaddle => horse::saddle(),
            Self::HorseArmor => horse::armor(),
            Self::WolfArmor => wolf::armor(),
            Self::PigSaddle => pig::saddle(),
            Self::CamelSaddle => camel::saddle(),
            Self::NautilusSaddle => nautilus::saddle(),
            Self::Armor(ArmorStyle::Humanoid, slot) => {
                humanoid::create_armor_layers()[slot as usize].clone()
            }
            Self::Armor(ArmorStyle::ZombieVillager, slot) => {
                villager::zombie_villager_armor()[slot as usize].clone()
            }
            Self::Armor(ArmorStyle::ArmorStand, slot) => {
                armor_stand::armor()[slot as usize].clone()
            }
            Self::BabyArmor(ArmorStyle::ArmorStand, slot) => {
                armor_stand::small_armor()[slot as usize].clone()
            }
            Self::Armor(ArmorStyle::Piglin, slot) => piglin::armor()[slot as usize].clone(),
            Self::BabyArmor(ArmorStyle::Piglin, slot) => {
                piglin::baby_armor()[slot as usize].clone()
            }
            Self::BabyArmor(_, slot) => humanoid::create_baby_armor_layers()[slot as usize].clone(),
            Self::SulfurCubeOuter => sulfur_cube::outer(),
            Self::SulfurCubeInner => sulfur_cube::inner(),
            Self::SulfurCubeSmallOuter => sulfur_cube::outer_small(),
            Self::SulfurCubeSmallInner => sulfur_cube::inner_small(),
            Self::ZombieNautilusCoral => nautilus::zombie_coral(),
            Self::WitherSkull => projectile::wither_skull(),
            Self::ShulkerBullet => projectile::shulker_bullet(),
            Self::LlamaSpit => projectile::llama_spit(),
            Self::WindCharge => projectile::wind_charge(),
            Self::EvokerFangs => projectile::evoker_fangs(),
            Self::EndCrystal => projectile::end_crystal(),
            Self::Trident => projectile::trident(),
            Self::Arrow => projectile::arrow(),
            Self::Cushion => projectile::cushion(),
            Self::ShulkerBulletGlow => projectile::shulker_bullet_glow(),
            Self::CreeperArmor => creeper::armor(),
            Self::NautilusArmor => nautilus::armor(),
            Self::WardenTendrils => warden::tendrils(),
            Self::WardenHeart => warden::heart(),
            Self::WardenBioluminescent => warden::bioluminescent(),
            Self::WardenPulsatingSpots => warden::pulsating_spots(),
        }
    }
}

const fn armor_index(style: ArmorStyle, slot: ArmorSlot) -> usize {
    let base = match style {
        ArmorStyle::Humanoid => 0,
        ArmorStyle::ZombieVillager => 4,
        ArmorStyle::Piglin => 8,
        ArmorStyle::ArmorStand => 0,
    };
    base + slot as usize
}

const fn baby_armor_index(style: ArmorStyle, slot: ArmorSlot) -> usize {
    let base = match style {
        ArmorStyle::Humanoid | ArmorStyle::ZombieVillager => 0,
        ArmorStyle::Piglin => 4,
        ArmorStyle::ArmorStand => 0,
    };
    base + slot as usize
}

/// How a model is posed: which parts move, found once when it is baked.
#[derive(Clone, Copy, Debug)]
pub enum Rig {
    Quadruped(QuadrupedParts),
    Chicken(chicken::ChickenParts),
    Creeper(creeper::CreeperParts),
    Spider(spider::SpiderParts),
    Segmented(segmented::SegmentedParts),
    Bat(bat::BatParts),
    Rabbit(rabbit::RabbitParts),
    Golem(golem::GolemParts),
    Enderman(HumanoidParts),
    Blaze(blaze::BlazeParts),
    SnowGolem(snow_golem::SnowGolemParts),
    Ghast(ghast::GhastParts),
    HappyGhast(ghast::HappyGhastParts),
    HappyGhastHarness(ghast::HarnessParts),
    Squid(squid::SquidParts),
    Fish(fish::FishRig),
    Dolphin(dolphin::DolphinParts),
    Turtle(turtle::TurtleParts),
    Guardian(guardian::GuardianParts),
    Nautilus(nautilus::NautilusParts),
    Axolotl(axolotl::AxolotlParts),
    Fox(fox::FoxParts),
    Frog(frog::FrogParts),
    Camel(camel::CamelParts),
    Armadillo(armadillo::ArmadilloParts),
    PolarBear(polar_bear::PolarBearParts),
    Bogged(skeleton::BoggedParts),
    MagmaCube(slime::MagmaParts),
    Humanoid(HumanoidParts),
    Piglin(piglin::PiglinParts),
    Villager(villager::VillagerParts),
    Illager(illager::IllagerParts),
    Wolf(wolf::WolfParts),
    Feline(feline::FelineParts),
    Equine(horse::EquineParts),
    Boat(vehicle::BoatParts),
    Bee(bee::BeeParts),
    Goat(goat::GoatParts),
    Llama(llama::LlamaParts),
    Panda(panda::PandaParts),
    Parrot(parrot::ParrotParts),
    Hoglin(hoglin::HoglinParts),
    Phantom(phantom::PhantomParts),
    Vex(vex::VexParts),
    Allay(allay::AllayParts),
    Strider(strider::StriderParts),
    Ravager(ravager::RavagerParts),
    Wither(wither::WitherParts),
    Dragon(dragon::DragonParts),
    Shulker(shulker::ShulkerParts),
    Sniffer(sniffer::SnifferParts),
    Warden(warden::WardenParts),
    Creaking(creaking::CreakingParts),
    CopperGolem(copper_golem::CopperGolemParts),
    ArmorStand(armor_stand::ArmorStandParts),
    /// A projectile's, crystal's or fangs' few moving parts.
    Thing(projectile::ThingParts),
    /// Not posed: drawn as built.
    Static,
}

impl Rig {
    pub fn of(id: ModelId, model: &Model) -> Self {
        use ModelId as M;
        match id {
            M::Pig
            | M::PigCold
            | M::PigBaby
            | M::PigSaddle
            | M::Cow
            | M::CowCold
            | M::CowWarm
            | M::CowBaby => Self::Quadruped(QuadrupedParts::find(model)),
            M::Sheep | M::SheepBaby | M::SheepFur => Self::Quadruped(QuadrupedParts::find(model)),
            M::Chicken | M::ChickenCold | M::ChickenBaby => {
                Self::Chicken(chicken::ChickenParts::find(model))
            }
            M::Creeper | M::CreeperArmor => Self::Creeper(creeper::CreeperParts::find(model)),
            M::Spider => Self::Spider(spider::SpiderParts::find(model)),
            M::Bat => Self::Bat(bat::BatParts::find(model)),
            M::Rabbit | M::RabbitBaby => Self::Rabbit(rabbit::RabbitParts::find(model)),
            M::Endermite => Self::Segmented(segmented::SegmentedParts::find(
                model,
                segmented::Kind::Endermite,
            )),
            M::Silverfish => Self::Segmented(segmented::SegmentedParts::find(
                model,
                segmented::Kind::Silverfish,
            )),
            M::IronGolem => Self::Golem(golem::GolemParts::find(model)),
            M::Enderman => Self::Enderman(HumanoidParts::find(model)),
            M::Blaze => Self::Blaze(blaze::BlazeParts::find(model)),
            M::SnowGolem => Self::SnowGolem(snow_golem::SnowGolemParts::find(model)),
            M::Ghast => Self::Ghast(ghast::GhastParts::find(model)),
            M::HappyGhast | M::HappyGhastBaby => {
                Self::HappyGhast(ghast::HappyGhastParts::find(model))
            }
            M::Squid | M::SquidBaby => Self::Squid(squid::SquidParts::find(model)),
            M::Cod
            | M::TropicalSmall
            | M::TropicalSmallPattern
            | M::TropicalLarge
            | M::TropicalLargePattern => Self::Fish(fish::FishRig::find_tail(model)),
            M::Salmon | M::SalmonSmall | M::SalmonLarge => Self::Fish(fish::FishRig::salmon(model)),
            M::PufferBig | M::PufferMid | M::PufferSmall => {
                Self::Fish(fish::FishRig::puffer(model))
            }
            M::Dolphin | M::DolphinBaby => Self::Dolphin(dolphin::DolphinParts::find(model)),
            M::Turtle | M::TurtleBaby => Self::Turtle(turtle::TurtleParts::find(model)),
            M::Guardian | M::ElderGuardian => Self::Guardian(guardian::GuardianParts::find(model)),
            M::Tadpole => Self::Fish(fish::FishRig::tadpole(model)),
            M::Nautilus
            | M::NautilusBaby
            | M::NautilusSaddle
            | M::NautilusArmor
            | M::ZombieNautilusCoral => Self::Nautilus(nautilus::NautilusParts::find(model)),
            M::Axolotl => Self::Axolotl(axolotl::AxolotlParts::find(model)),
            M::Fox => Self::Fox(fox::FoxParts::find(model, false)),
            M::FoxBaby => Self::Fox(fox::FoxParts::find(model, true)),
            M::Frog => Self::Frog(frog::FrogParts::find(model)),
            M::Camel | M::CamelBaby | M::CamelSaddle => Self::Camel(camel::CamelParts::find(model)),
            M::Armadillo | M::ArmadilloBaby => {
                Self::Armadillo(armadillo::ArmadilloParts::find(model))
            }
            M::PolarBear | M::PolarBearBaby => {
                Self::PolarBear(polar_bear::PolarBearParts::find(model))
            }
            M::Bogged => Self::Bogged(skeleton::BoggedParts::find(model)),
            M::BoggedClothes => Self::Humanoid(HumanoidParts::find(model)),
            M::AxolotlBaby => Self::Static,
            M::HappyGhastHarness | M::HappyGhastBabyHarness => {
                Self::HappyGhastHarness(ghast::HarnessParts::find(model))
            }
            M::Bee => Self::Bee(bee::BeeParts::find(model)),
            M::BeeBaby => Self::Bee(bee::BeeParts::find(model)),
            M::Goat | M::GoatBaby => Self::Goat(goat::GoatParts::find(model)),
            M::Llama | M::LlamaBaby | M::LlamaDecor | M::LlamaBabyDecor => {
                Self::Llama(llama::LlamaParts::find(model))
            }
            M::Panda => Self::Panda(panda::PandaParts::find(model)),
            M::PandaBaby => Self::Panda(panda::PandaParts::find(model)),
            M::Parrot => Self::Parrot(parrot::ParrotParts::find(model)),
            M::Hoglin | M::HoglinBaby => Self::Hoglin(hoglin::HoglinParts::find(model)),
            M::Phantom => Self::Phantom(phantom::PhantomParts::find(model)),
            M::Vex => Self::Vex(vex::VexParts::find(model)),
            M::Allay => Self::Allay(allay::AllayParts::find(model)),
            M::Strider | M::StriderBaby => Self::Strider(strider::StriderParts::find(model)),
            M::Ravager => Self::Ravager(ravager::RavagerParts::find(model)),
            M::Wither | M::WitherArmor => Self::Wither(wither::WitherParts::find(model)),
            M::Dragon => Self::Dragon(dragon::DragonParts::find(model)),
            M::Shulker => Self::Shulker(shulker::ShulkerParts::find(model)),
            M::Sniffer | M::SnifferBaby => Self::Sniffer(sniffer::SnifferParts::find(model)),
            M::Warden
            | M::WardenTendrils
            | M::WardenHeart
            | M::WardenBioluminescent
            | M::WardenPulsatingSpots => Self::Warden(warden::WardenParts::find(model)),
            M::Creaking => Self::Creaking(creaking::CreakingParts::find(model)),
            M::CopperGolem => Self::CopperGolem(copper_golem::CopperGolemParts::find(model)),
            M::ArmorStand | M::ArmorStandSmall => {
                Self::ArmorStand(armor_stand::ArmorStandParts::find(model))
            }
            M::LeashKnot
            | M::SlimeInner
            | M::SlimeOuter
            | M::Minecart
            | M::BoatWaterPatch
            | M::Breeze
            | M::BreezeWind => Self::Static,
            M::Boat | M::ChestBoat | M::Raft | M::ChestRaft => {
                Self::Boat(vehicle::BoatParts::find(model))
            }
            M::MagmaCube => Self::MagmaCube(slime::MagmaParts::find(model)),
            M::WitherSkull => Self::Thing(projectile::ThingParts::look(model, "head")),
            M::ShulkerBullet | M::ShulkerBulletGlow => {
                Self::Thing(projectile::ThingParts::look(model, "main"))
            }
            M::WindCharge => Self::Thing(projectile::ThingParts::wind_charge(model)),
            M::EvokerFangs => Self::Thing(projectile::ThingParts::fangs(model)),
            M::EndCrystal => Self::Thing(projectile::ThingParts::crystal(model)),
            M::Arrow => Self::Thing(projectile::ThingParts::Arrow),
            M::SulfurCubeOuter
            | M::SulfurCubeInner
            | M::SulfurCubeSmallOuter
            | M::SulfurCubeSmallInner
            | M::LlamaSpit
            | M::Trident
            | M::Cushion => Self::Static,
            M::Villager | M::VillagerNoHat | M::VillagerBaby | M::VillagerBabyNoHat | M::Witch => {
                Self::Villager(villager::VillagerParts::find(model))
            }
            M::Illager => Self::Illager(illager::IllagerParts::find(model)),
            M::Wolf | M::WolfArmor => Self::Wolf(wolf::WolfParts::find(model, false)),
            M::WolfBaby => Self::Wolf(wolf::WolfParts::find(model, true)),
            M::Feline | M::FelineCollar => Self::Feline(feline::FelineParts::find(model, false)),
            M::Horse | M::Donkey | M::HorseSaddle | M::HorseArmor => {
                Self::Equine(horse::EquineParts::find(model, horse::Build::Adult))
            }
            M::HorseBaby => Self::Equine(horse::EquineParts::find(model, horse::Build::Foal)),
            M::DonkeyBaby => {
                Self::Equine(horse::EquineParts::find(model, horse::Build::DonkeyFoal))
            }
            M::FelineBaby | M::FelineBabyCollar => {
                Self::Feline(feline::FelineParts::find(model, true))
            }
            M::ZombieVillager
            | M::ZombieVillagerNoHat
            | M::ZombieVillagerBaby
            | M::ZombieVillagerBabyNoHat => Self::Humanoid(HumanoidParts::find(model)),
            M::Piglin => Self::Piglin(piglin::PiglinParts::find(model, false)),
            M::PiglinBaby => Self::Piglin(piglin::PiglinParts::find(model, true)),
            M::Zombie
            | M::ZombieBaby
            | M::Drowned
            | M::DrownedOuter
            | M::DrownedBabyOuter
            | M::Skeleton
            | M::Parched
            | M::StrayClothes
            | M::Armor(..)
            | M::BabyArmor(..) => Self::Humanoid(HumanoidParts::find(model)),
        }
    }
}

/// A baked model and its rig.
#[derive(Clone, Debug)]
pub struct MobModel {
    pub model: Model,
    pub rig: Rig,
    /// The keyframe animations the model plays, each with the slot of the state that runs it.
    pub animations: KeyframeSet,
    /// A walk-driven animation and the clock slot that gates it.
    pub walk: Option<(KeyframeAnimation, usize)>,
    /// A second walk cycle (a frog's swim).
    pub extra_walk: Option<KeyframeAnimation>,
}

/// How many animation clocks an entity has: the most any mob runs at once.
pub const ANIMATION_SLOTS: usize = 6;

/// What a pose reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoseInput {
    pub living: LivingRenderState,
    pub humanoid: HumanoidRenderState,
    /// A skeleton's chop when aggressive without a bow.
    pub skeleton_melee: bool,
    /// `ChickenRenderState.flap` and `flapSpeed`.
    pub flap: (f32, f32),
    /// `SlimeRenderState.squish`.
    pub squish: f32,
    pub piglin: piglin::PiglinPose,
    pub villager: villager::VillagerPose,
    pub illager: illager::IllagerPose,
    pub wolf: wolf::WolfPose,
    pub feline: feline::FelinePose,
    pub equine: horse::EquinePose,
    pub golem: golem::GolemPose,
    pub happy_ghast: ghast::HappyGhastPose,
    /// `SquidRenderState.tentacleAngle`.
    pub squid_angle: f32,
    /// Whether a fish or dolphin is in the water, and moving sideways.
    pub in_water: bool,
    pub moving: bool,
    pub turtle: turtle::TurtlePose,
    pub guardian: guardian::GuardianPose,
    /// An adult axolotl's blend factors.
    pub axolotl: axolotl::Factors,
    /// The keyframe animation clocks, by slot (`AnimationState`s of the entity).
    pub anim: [AnimationState; ANIMATION_SLOTS],
    pub bat_resting: bool,
    pub enderman: enderman::EndermanPose,
    /// A boat's paddles.
    pub rowing: vehicle::Rowing,
    pub fox: fox::FoxPose,
    pub polar_stand: f32,
    pub armadillo_hiding: bool,
    pub camel_jump: f32,
    pub frog_croak: bool,
    pub sheared: bool,
    pub goat: goat::GoatPose,
    pub llama_chest: bool,
    pub panda: panda::PandaPose,
    pub parrot: parrot::ParrotPose,
    pub parrot_flap: f32,
    pub hoglin: hoglin::HoglinPose,
    pub bee: bee::BeePose,
    pub strider: strider::StriderPose,
    pub ravager: ravager::RavagerPose,
    pub shulker: shulker::ShulkerPose,
    pub wither: wither::WitherPose,
    pub vex: vex::VexPose,
    pub allay: allay::AllayPose,
    pub dragon: dragon::DragonPose,
    pub phantom_flap: f32,
    pub sniffer_searching: bool,
    pub warden_tendril: f32,
    pub creaking_move: bool,
    pub copper_holding: bool,
    pub armor_stand: armor_stand::StandPoseInput,
    pub thing: projectile::ThingPose,
}

impl MobModel {
    pub fn new(id: ModelId) -> Self {
        let model = Model::bake(&id.layer());
        let animations = KeyframeSet::bake(&model, id.animations())
            .unwrap_or_else(|missing| panic!("{id:?}: {missing}"));
        let walk = match id {
            ModelId::Nautilus
            | ModelId::NautilusBaby
            | ModelId::NautilusSaddle
            | ModelId::NautilusArmor => KeyframeAnimation::bake(&model, nautilus::WALK_ANIMATION)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::AxolotlBaby => KeyframeAnimation::bake(&model, axolotl::WALK_FLOOR)
                .ok()
                .map(|a| (a, axolotl::WALK)),
            ModelId::FoxBaby => KeyframeAnimation::bake(&model, fox::WALK_ANIMATION)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::Frog => KeyframeAnimation::bake(&model, frog::WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::Camel | ModelId::CamelSaddle => KeyframeAnimation::bake(&model, camel::WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::CamelBaby => KeyframeAnimation::bake(&model, camel::BABY_WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::Armadillo => KeyframeAnimation::bake(&model, armadillo::WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::ArmadilloBaby => KeyframeAnimation::bake(&model, armadillo::BABY_WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::Sniffer | ModelId::SnifferBaby => {
                KeyframeAnimation::bake(&model, sniffer::WALK)
                    .ok()
                    .map(|a| (a, usize::MAX))
            }
            ModelId::Creaking => KeyframeAnimation::bake(&model, creaking::WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            ModelId::CopperGolem => KeyframeAnimation::bake(&model, copper_golem::WALK)
                .ok()
                .map(|a| (a, usize::MAX)),
            _ => None,
        };
        let extra_walk = match id {
            ModelId::Frog => KeyframeAnimation::bake(&model, frog::SWIM).ok(),
            ModelId::Sniffer | ModelId::SnifferBaby => {
                KeyframeAnimation::bake(&model, sniffer::SEARCH).ok()
            }
            ModelId::CopperGolem => {
                KeyframeAnimation::bake(&model, copper_golem::WALK_WITH_ITEM).ok()
            }
            _ => None,
        };
        Self {
            rig: Rig::of(id, &model),
            model,
            animations,
            walk,
            extra_walk,
        }
    }

    /// `setupAnim`: a fresh pose for this frame.
    pub fn pose(&self, input: &PoseInput) -> ModelPose {
        let mut pose = self.model.initial_pose();
        match &self.rig {
            Rig::Boat(parts) => vehicle::setup_anim(parts, &mut pose, input.rowing),
            Rig::Quadruped(parts) => quadruped::setup_anim(parts, &mut pose, &input.living),
            Rig::Chicken(parts) => chicken::setup_anim(parts, &mut pose, &input.living, input.flap),
            Rig::Creeper(parts) => creeper::setup_anim(parts, &mut pose, &input.living),
            Rig::Spider(parts) => spider::setup_anim(parts, &mut pose, &input.living),
            Rig::Segmented(parts) => segmented::setup_anim(parts, &mut pose, &input.living),
            Rig::Rabbit(parts) => {
                rabbit::setup_anim(parts, &mut pose, &input.living, &input.anim[rabbit::IDLE])
            }
            Rig::Bat(parts) => bat::setup_anim(parts, &mut pose, &input.living, input.bat_resting),
            Rig::Enderman(parts) => {
                humanoid::setup_anim(parts, &mut pose, &input.humanoid);
                enderman::setup_anim(parts, &mut pose, input.enderman);
            }
            Rig::Golem(parts) => golem::setup_anim(parts, &mut pose, &input.living, input.golem),
            Rig::Blaze(parts) => blaze::setup_anim(parts, &mut pose, &input.living),
            Rig::SnowGolem(parts) => snow_golem::setup_anim(parts, &mut pose, &input.living),
            Rig::Ghast(parts) => ghast::setup_anim(parts, &mut pose, &input.living),
            Rig::HappyGhast(parts) => {
                ghast::happy_setup_anim(parts, &mut pose, &input.living, input.happy_ghast)
            }
            Rig::Squid(parts) => squid::setup_anim(parts, &mut pose, input.squid_angle),
            Rig::Fish(rig) => {
                fish::setup_anim(rig, &mut pose, input.living.age_in_ticks, input.in_water)
            }
            Rig::Dolphin(parts) => {
                dolphin::setup_anim(parts, &mut pose, &input.living, input.moving)
            }
            Rig::Turtle(parts) => turtle::setup_anim(parts, &mut pose, &input.living, input.turtle),
            Rig::Guardian(parts) => {
                guardian::setup_anim(parts, &mut pose, &input.living, input.guardian)
            }
            Rig::Nautilus(parts) => {
                if let Some((swim, _)) = &self.walk {
                    nautilus::setup_anim(parts, swim, &mut pose, &input.living);
                }
            }
            Rig::Axolotl(parts) => {
                axolotl::setup_anim(parts, &mut pose, &input.living, input.axolotl)
            }
            Rig::Fox(parts) => {
                fox::setup_anim(
                    parts,
                    &mut pose,
                    &input.living,
                    input.fox,
                    self.walk.as_ref().map(|(a, _)| a),
                );
            }
            Rig::Frog(parts) => {
                if let (Some((walk, _)), Some(swim)) = (&self.walk, &self.extra_walk) {
                    frog::setup_anim(
                        parts,
                        &mut pose,
                        &input.living,
                        input.in_water,
                        input.frog_croak,
                        walk,
                        swim,
                    );
                }
            }
            Rig::Camel(parts) => {
                if let Some((walk, _)) = &self.walk {
                    camel::setup_anim(parts, walk, &mut pose, &input.living, input.camel_jump);
                }
            }
            Rig::Armadillo(parts) => {
                if let Some((walk, _)) = &self.walk {
                    armadillo::setup_anim(
                        parts,
                        walk,
                        &mut pose,
                        &input.living,
                        input.armadillo_hiding,
                    );
                }
            }
            Rig::PolarBear(parts) => {
                polar_bear::setup_anim(parts, &mut pose, &input.living, input.polar_stand);
            }
            Rig::Bee(parts) => bee::setup_anim(parts, &mut pose, &input.living, input.bee),
            Rig::Goat(parts) => goat::setup_anim(parts, &mut pose, &input.living, input.goat),
            Rig::Llama(parts) => {
                llama::setup_anim(parts, &mut pose, &input.living, input.llama_chest)
            }
            Rig::Panda(parts) => panda::setup_anim(parts, &mut pose, &input.living, input.panda),
            Rig::Parrot(parts) => parrot::setup_anim(
                parts,
                &mut pose,
                &input.living,
                input.parrot,
                input.parrot_flap,
            ),
            Rig::Hoglin(parts) => hoglin::setup_anim(parts, &mut pose, &input.living, input.hoglin),
            Rig::Phantom(parts) => phantom::setup_anim(parts, &mut pose, input.phantom_flap),
            Rig::Vex(parts) => vex::setup_anim(parts, &mut pose, &input.living, input.vex),
            Rig::Allay(parts) => allay::setup_anim(parts, &mut pose, &input.living, input.allay),
            Rig::Strider(parts) => {
                strider::setup_anim(parts, &mut pose, &input.living, input.strider)
            }
            Rig::Ravager(parts) => {
                ravager::setup_anim(parts, &mut pose, &input.living, input.ravager)
            }
            Rig::Wither(parts) => wither::setup_anim(parts, &mut pose, &input.living, input.wither),
            Rig::Dragon(parts) => dragon::setup_anim(parts, &mut pose, input.dragon),
            Rig::Shulker(parts) => {
                shulker::setup_anim(parts, &mut pose, &input.living, input.shulker)
            }
            Rig::Sniffer(parts) => {
                if let (Some((walk, _)), Some(search)) = (&self.walk, &self.extra_walk) {
                    sniffer::setup_anim(
                        parts,
                        &mut pose,
                        &input.living,
                        input.sniffer_searching,
                        walk,
                        search,
                        None,
                    );
                }
            }
            Rig::Warden(parts) => {
                warden::setup_anim(parts, &mut pose, &input.living, input.warden_tendril)
            }
            Rig::Creaking(parts) => {
                if let Some((walk, _)) = &self.walk {
                    creaking::setup_anim(
                        parts,
                        walk,
                        &mut pose,
                        &input.living,
                        input.creaking_move,
                    );
                }
            }
            Rig::CopperGolem(parts) => {
                if let (Some((walk, _)), Some(walk_item)) = (&self.walk, &self.extra_walk) {
                    copper_golem::setup_anim(
                        parts,
                        walk,
                        walk_item,
                        &mut pose,
                        &input.living,
                        input.copper_holding,
                    );
                }
            }
            Rig::Bogged(parts) => {
                humanoid::setup_anim(&parts.humanoid, &mut pose, &input.humanoid);
                if input.skeleton_melee {
                    let swing = input.humanoid.swing.map_or(0.0, |s| s.animation);
                    skeleton::melee(&parts.humanoid, &mut pose, swing, input.living.age_in_ticks);
                }
                parts.hide_mushrooms(&mut pose, input.sheared);
            }
            Rig::HappyGhastHarness(parts) => {
                ghast::harness_setup_anim(parts, &mut pose, input.happy_ghast)
            }
            Rig::Static => {
                if let Some((walk, axolotl::WALK)) = &self.walk
                    && input.anim[axolotl::WALK].is_started()
                {
                    walk.apply_walk(
                        &mut pose,
                        input.living.walk_position,
                        input.living.walk_speed,
                        15.0,
                        30.0,
                    );
                }
            }
            Rig::MagmaCube(parts) => slime::setup_anim(parts, &mut pose, input.squish),
            Rig::Villager(parts) => {
                villager::setup_anim(parts, &mut pose, &input.living, input.villager)
            }
            Rig::Illager(parts) => {
                illager::setup_anim(parts, &mut pose, &input.living, input.illager)
            }
            Rig::Wolf(parts) => wolf::setup_anim(parts, &mut pose, &input.living, input.wolf),
            Rig::Equine(parts) => horse::setup_anim(parts, &mut pose, &input.living, input.equine),
            Rig::Feline(parts) => feline::setup_anim(parts, &mut pose, &input.living, input.feline),
            Rig::Piglin(parts) => {
                humanoid::setup_anim(&parts.humanoid, &mut pose, &input.humanoid);
                piglin::setup_anim(parts, &mut pose, &input.living, input.piglin);
            }
            Rig::ArmorStand(parts) => armor_stand::setup_anim(parts, &mut pose, &input.armor_stand),
            Rig::Thing(parts) => {
                projectile::setup_anim(&self.model, *parts, &mut pose, input.thing)
            }
            Rig::Humanoid(parts) => {
                humanoid::setup_anim(parts, &mut pose, &input.humanoid);
                if input.skeleton_melee {
                    let swing = input.humanoid.swing.map_or(0.0, |s| s.animation);
                    skeleton::melee(parts, &mut pose, swing, input.living.age_in_ticks);
                }
            }
        }
        self.animations
            .apply(&mut pose, &input.anim, input.living.age_in_ticks);
        pose
    }

    /// The humanoid parts, for armor and items in hand.
    pub fn humanoid(&self) -> Option<&HumanoidParts> {
        match &self.rig {
            Rig::Humanoid(parts) => Some(parts),
            Rig::ArmorStand(parts) => Some(&parts.humanoid),
            Rig::Piglin(parts) => Some(&parts.humanoid),
            Rig::Illager(parts) => Some(&parts.humanoid),
            Rig::Bogged(parts) => Some(&parts.humanoid),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::living;
    use crate::world::entity::magic_name::Magic;
    use pumpkin_data::entity::EntityType;
    use pumpkin_data::item_stack::ItemStack;

    /// The lowest and highest point of `model` posed at rest, in blocks.
    fn extent(id: ModelId) -> (f32, f32) {
        let mob = MobModel::new(id);
        let pose = mob.pose(&PoseInput::default());
        let root = living::root_transform(&LivingRenderState::default(), Placement::default());
        let matrices = mob.model.part_matrices(&pose, root);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for (i, m) in matrices.iter().enumerate() {
            let Some(m) = m else { continue };
            for v in mob
                .model
                .part_vertices(crate::world::entity::model::PartId(i))
            {
                let y = m.transform_point3(Vec3::from_array(v.pos)).y;
                lo = lo.min(y);
                hi = hi.max(y);
            }
        }
        (lo, hi)
    }

    fn spawned(ty: &EntityType) -> ClientEntity {
        ClientEntity::spawn(&crate::network::play::game::AddEntity {
            id: 1,
            uuid: uuid::Uuid::nil(),
            kind: i32::from(ty.id),
            pos: [0.0; 3],
            velocity: [0.0; 3],
            pitch: 0.0,
            yaw: 0.0,
            head_yaw: 0.0,
            data: 0,
        })
        .unwrap()
    }

    fn body(entity: &ClientEntity) -> Vec<LayerSpec> {
        let mut out = Vec::new();
        layers(entity, 0.0, &|_| villager::Hat::None, &mut out);
        out
    }

    #[test]
    fn shadows_follow_each_renderers_radius() {
        let radius = |ty: &EntityType, baby: bool| {
            let mut entity = spawned(ty);
            entity.appearance.baby = baby;
            shadow_radius(&entity)
        };
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(radius(&EntityType::COW, true), 0.35));
        // `VillagerRenderer` and `StriderRenderer` halve a baby's again.
        assert!(close(radius(&EntityType::VILLAGER, true), 0.125));
        assert!(close(radius(&EntityType::STRIDER, true), 0.125));
        // `Turtle.getAgeScale` is 0.3, and `TurtleRenderer` takes 0.83 of it.
        assert!(close(radius(&EntityType::TURTLE, true), 0.7 * 0.3 * 0.83));
        assert!(close(radius(&EntityType::GOAT, true), 0.7 * 0.55));
        assert!(close(radius(&EntityType::ARMOR_STAND, false), 0.0));
        assert!(close(radius(&EntityType::GIANT, false), 3.0));
        let mut cube = spawned(&EntityType::SULFUR_CUBE);
        cube.appearance.size = 2;
        cube.appearance.baby = true;
        assert!(close(shadow_radius(&cube), 0.5));
        assert!(close(shadow_strength(&spawned(&EntityType::ITEM)), 0.75));
    }

    #[test]
    fn the_new_things_have_their_layers() {
        let mut skull = spawned(&EntityType::WITHER_SKULL);
        assert_eq!(body(&skull)[0].texture, projectile::WITHER_TEXTURE);
        skull.appearance.thing.dangerous = true;
        assert_eq!(
            body(&skull)[0].texture,
            projectile::WITHER_INVULNERABLE_TEXTURE
        );
        let bullet = body(&spawned(&EntityType::SHULKER_BULLET));
        assert_eq!(bullet.len(), 2);
        assert!(bullet[1].translucent && bullet[1].alpha < 0.2);
        let cube = body(&spawned(&EntityType::SULFUR_CUBE));
        assert_eq!(cube.len(), 2, "core then shell");
        assert_eq!(cube[0].model, ModelId::SulfurCubeInner);
        assert!(body(&spawned(&EntityType::EVOKER_FANGS)).is_empty());
        let mut arrow = spawned(&EntityType::ARROW);
        assert_eq!(body(&arrow)[0].texture, projectile::ARROW_TEXTURE);
        arrow.appearance.thing.arrow_color = 0xFF0000;
        assert_eq!(body(&arrow)[0].texture, projectile::TIPPED_ARROW_TEXTURE);
        assert_eq!(
            body(&spawned(&EntityType::PARCHED))[0].texture,
            "minecraft:entity/skeleton/parched"
        );
    }

    #[test]
    fn magic_names_change_a_sheeps_wool_and_a_rabbits_coat() {
        let mut sheep = spawned(&EntityType::SHEEP);
        let plain = body(&sheep);
        sheep.appearance.magic = Magic::Jeb;
        let at = |age: f32| {
            let mut out = Vec::new();
            layers(&sheep, age, &|_| villager::Hat::None, &mut out);
            out
        };
        assert_eq!(at(0.0).len(), plain.len() + 1);
        assert_ne!(at(0.0).last().unwrap().tint, at(30.0).last().unwrap().tint);
        let mut rabbit = spawned(&EntityType::RABBIT);
        assert!(body(&rabbit)[0].texture.ends_with("brown"));
        rabbit.appearance.magic = Magic::Toast;
        assert!(body(&rabbit)[0].texture.ends_with("rabbit_toast"));
        rabbit.appearance.baby = true;
        assert!(body(&rabbit)[0].texture.ends_with("rabbit_toast_baby"));
    }

    #[test]
    fn a_hurt_or_dying_body_is_mixed_toward_red_for_every_kind_of_entity() {
        assert_eq!(hurt_overlay(false), 0.0);
        let mut cow = spawned(&EntityType::COW);
        let calm = LivingRenderState::default();
        assert_eq!(overlay(&cow, &calm, 0.0), 0.0);
        let hurt = LivingRenderState { hurt: true, ..calm };
        assert_eq!(overlay(&cow, &hurt, 0.0), hurt_overlay(true));
        cow.appearance.health = 0.0;
        assert!(hurt_overlay(true) > 0.2 && hurt_overlay(true) < 0.4);
    }

    #[test]
    fn a_hopping_rabbit_moves_its_legs_and_a_still_one_follows_the_look() {
        let rabbit = MobModel::new(ModelId::Rabbit);
        let head = rabbit.model.part("head");
        let leg = rabbit.model.part("right_hind_leg");
        let looking = PoseInput {
            living: LivingRenderState {
                y_rot: 30.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let still = rabbit.pose(&looking);
        assert!((still[head].pose.rotation.y - 30f32.to_radians()).abs() < 1e-5);
        let mut hopping = looking;
        hopping.anim[rabbit::HOP].start(0);
        hopping.living.age_in_ticks = 6.0;
        let mid_air = rabbit.pose(&hopping);
        assert!(
            (mid_air[leg].pose.rotation - still[leg].pose.rotation).length() > 0.05
                || (mid_air[leg].pose.offset - still[leg].pose.offset).length() > 0.05
        );
        let mut idle = looking;
        idle.anim[rabbit::IDLE].start(0);
        idle.living.age_in_ticks = 20.0;
        assert_ne!(
            rabbit.pose(&idle)[head].pose.rotation.y,
            still[head].pose.rotation.y
        );
    }

    #[test]
    fn a_hurt_iron_golem_wears_cracks_over_its_body() {
        let mut golem = spawned(&EntityType::IRON_GOLEM);
        golem.appearance.health = 100.0;
        assert_eq!(body(&golem).len(), 1);
        golem.appearance.health = 40.0;
        let hurt = body(&golem);
        assert_eq!(hurt.len(), 2);
        assert_eq!(hurt[1].model, ModelId::IronGolem);
        assert!(hurt[1].texture.ends_with("crackiness_medium"));
    }

    #[test]
    fn cracks_follow_the_synced_max_health() {
        let mut golem = spawned(&EntityType::IRON_GOLEM);
        golem.max_health = 200.0;
        golem.appearance.health = 100.0;
        assert!(body(&golem)[1].texture.ends_with("crackiness_low"));
    }

    #[test]
    fn a_copper_golem_shows_how_far_it_has_oxidised() {
        let mut golem = spawned(&EntityType::COPPER_GOLEM);
        assert!(body(&golem)[0].texture.ends_with("/copper_golem"));
        golem.appearance.copper_weather = copper_golem::WEATHER_OXIDIZED;
        let layers = body(&golem);
        assert!(layers[0].texture.ends_with("copper_golem_oxidized"));
        assert!(layers[1].texture.ends_with("copper_golem_eyes_oxidized"));
    }

    #[test]
    fn a_wither_wears_its_armor_at_half_health_not_while_invulnerable() {
        let mut wither = spawned(&EntityType::WITHER);
        wither.max_health = 300.0;
        wither.appearance.health = 300.0;
        wither.appearance.wither_invul = 200;
        assert_eq!(body(&wither).len(), 1);
        wither.appearance.wither_invul = 0;
        wither.appearance.health = 150.0;
        let layers = body(&wither);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[1].model, ModelId::WitherArmor);
    }

    #[test]
    fn a_llama_wears_its_carpet_and_a_trader_its_blanket_only_without_one() {
        use crate::network::play::game::EquipmentSlot;
        use pumpkin_data::item::Item;

        let decor = |entity: &ClientEntity| {
            body(entity)
                .into_iter()
                .filter(|l| matches!(l.model, ModelId::LlamaDecor | ModelId::LlamaBabyDecor))
                .map(|l| l.texture.into_owned())
                .collect::<Vec<_>>()
        };
        let mut trader = spawned(&EntityType::TRADER_LLAMA);
        assert_eq!(
            decor(&trader),
            ["minecraft:entity/equipment/llama_body/trader_llama"]
        );
        trader
            .equipment
            .set(EquipmentSlot::Body, ItemStack::new(1, &Item::LIME_CARPET));
        assert_eq!(
            decor(&trader),
            ["minecraft:entity/equipment/llama_body/lime"]
        );

        let mut llama = spawned(&EntityType::LLAMA);
        assert!(decor(&llama).is_empty());
        llama
            .equipment
            .set(EquipmentSlot::Body, ItemStack::new(1, &Item::WHITE_CARPET));
        assert_eq!(
            decor(&llama),
            ["minecraft:entity/equipment/llama_body/white"]
        );
        // A young llama shows no carpet.
        llama.appearance.baby = true;
        assert!(decor(&llama).is_empty());
    }

    #[test]
    fn a_charged_creeper_and_a_powered_wither_wear_a_swirl() {
        let mut creeper = spawned(&EntityType::CREEPER);
        assert_eq!(body(&creeper).len(), 1);
        creeper.appearance.powered = true;
        let swirl = &body(&creeper)[1];
        assert_eq!(swirl.model, ModelId::CreeperArmor);
        assert!(swirl.additive && swirl.scroll.is_some());
        assert_eq!(swirl.overlay, Some(0.0));

        let mut wither = spawned(&EntityType::WITHER);
        wither.max_health = 300.0;
        wither.appearance.health = 100.0;
        let armor = &body(&wither)[1];
        assert!(armor.additive);
        // `cos(0) * 3 % 1`: the swirl starts slid by nothing.
        assert_eq!(armor.scroll, Some([0.0, 0.0]));
    }

    #[test]
    fn the_breeze_wind_slides_and_its_eyes_blend() {
        let breeze = spawned(&EntityType::BREEZE);
        let layers = body(&breeze);
        assert_eq!(layers[1].model, ModelId::BreezeWind);
        assert!(layers[1].scroll.is_some() && !layers[1].additive && layers[1].translucent);
        assert!(layers[2].full_bright && layers[2].translucent);
    }

    #[test]
    fn a_nautilus_wears_armor_and_a_zombie_nautilus_a_saddle() {
        use crate::network::play::game::EquipmentSlot;
        use pumpkin_data::item::Item;

        let mut nautilus = spawned(&EntityType::ZOMBIE_NAUTILUS);
        nautilus.equipment.set(
            EquipmentSlot::Body,
            ItemStack::new(1, &Item::IRON_NAUTILUS_ARMOR),
        );
        nautilus
            .equipment
            .set(EquipmentSlot::Saddle, ItemStack::new(1, &Item::SADDLE));
        let layers = body(&nautilus);
        let armor = layers
            .iter()
            .find(|l| l.model == ModelId::NautilusArmor)
            .unwrap();
        assert_eq!(
            armor.texture,
            "minecraft:entity/equipment/nautilus_body/iron"
        );
        assert!(layers.iter().any(|l| l.model == ModelId::NautilusSaddle));
    }

    #[test]
    fn a_camel_husk_has_no_young_form() {
        let mut husk = spawned(&EntityType::CAMEL_HUSK);
        husk.appearance.baby = true;
        let layers = body(&husk);
        assert_eq!(layers[0].model, ModelId::Camel);
        assert_eq!(layers[0].texture, "minecraft:entity/camel/camel_husk");
    }

    #[test]
    fn a_bat_hangs_upside_down_and_flaps_while_flying() {
        let bat = MobModel::new(ModelId::Bat);
        let head = bat.model.part("head");
        let wing = bat.model.part("right_wing");
        let input = |slot: usize, age: f32| {
            let mut input = PoseInput {
                living: LivingRenderState {
                    age_in_ticks: age,
                    ..Default::default()
                },
                bat_resting: slot == bat::REST,
                ..Default::default()
            };
            input.anim[slot].start(0);
            input
        };
        let resting = bat.pose(&input(bat::REST, 3.0));
        assert!((resting[head].pose.rotation.x - crate::math::angle::PI).abs() < 1e-4);
        let early = bat.pose(&input(bat::FLY, 1.0));
        let later = bat.pose(&input(bat::FLY, 4.0));
        assert!((early[wing].pose.rotation - later[wing].pose.rotation).length() > 0.1);
        let idle = bat.pose(&PoseInput::default());
        assert_eq!(idle[head].pose, bat.model.initial(head));
    }

    /// The variant tables assume the order the server syncs its registries in.
    #[test]
    fn the_variant_tables_follow_the_synced_registry_order() {
        use pumpkin_util::version::JavaMinecraftVersion;
        let all = pumpkin_data::registry::Registry::get_synced(JavaMinecraftVersion::V_26_3);
        let ids = |registry: &str| -> Vec<String> {
            all.iter()
                .find(|r| r.registry_id.trim_start_matches("minecraft:") == registry)
                .unwrap_or_else(|| {
                    panic!(
                        "{registry} is synced: {:?}",
                        all.iter().map(|r| &r.registry_id).collect::<Vec<_>>()
                    )
                })
                .registry_entries
                .iter()
                .map(|e| e.entry_id.trim_start_matches("minecraft:").to_owned())
                .collect()
        };
        assert_eq!(ids("pig_variant"), ["cold", "temperate", "warm"]);
        assert_eq!(ids("cow_variant"), ["cold", "temperate", "warm"]);
        assert_eq!(ids("chicken_variant"), ["cold", "temperate", "warm"]);
        assert_eq!(
            ids("wolf_variant"),
            [
                "ashen", "black", "chestnut", "pale", "rusty", "snowy", "spotted", "striped",
                "woods"
            ]
        );
        assert_eq!(
            ids("cat_variant"),
            [
                "all_black",
                "black",
                "british_shorthair",
                "calico",
                "jellie",
                "persian",
                "ragdoll",
                "red",
                "siamese",
                "tabby",
                "white"
            ]
        );
    }

    #[test]
    fn variants_and_babies_pick_their_model_and_texture() {
        let mut pig = spawned(&EntityType::PIG);
        assert_eq!(
            body(&pig),
            [LayerSpec::plain(
                ModelId::Pig,
                "minecraft:entity/pig/pig_temperate"
            )]
        );
        pig.appearance.variant = 0;
        assert_eq!(body(&pig)[0].model, ModelId::PigCold);
        pig.appearance.baby = true;
        assert_eq!(
            body(&pig),
            [LayerSpec::plain(
                ModelId::PigBaby,
                "minecraft:entity/pig/pig_cold_baby"
            )]
        );
    }

    #[test]
    fn effect_layers_glow_blend_and_tint() {
        let spider = body(&spawned(&EntityType::SPIDER));
        assert_eq!(spider.len(), 2);
        assert!(!spider[0].full_bright && spider[1].full_bright);
        let slime = body(&spawned(&EntityType::SLIME));
        assert!(!slime[0].translucent && slime[1].translucent);
        let mut wolf = spawned(&EntityType::WOLF);
        assert_eq!(body(&wolf).len(), 1);
        wolf.appearance.tamable_flags = 4;
        let tamed = body(&wolf);
        assert_eq!(tamed.len(), 2);
        assert_eq!(tamed[1].model, ModelId::Wolf);
    }

    #[test]
    fn a_saddle_wolf_armor_horse_armor_and_harness_are_equipment_layers() {
        use crate::network::play::game::EquipmentSlot;
        use pumpkin_data::item::Item;

        let mut pig = spawned(&EntityType::PIG);
        pig.equipment
            .set(EquipmentSlot::Saddle, ItemStack::new(1, &Item::SADDLE));
        assert!(
            body(&pig)
                .iter()
                .any(|layer| layer.model == ModelId::PigSaddle)
        );

        let mut wolf = spawned(&EntityType::WOLF);
        wolf.equipment
            .set(EquipmentSlot::Body, ItemStack::new(1, &Item::WOLF_ARMOR));
        assert!(
            body(&wolf)
                .iter()
                .any(|layer| layer.model == ModelId::WolfArmor)
        );

        let mut horse = spawned(&EntityType::HORSE);
        horse.equipment.set(
            EquipmentSlot::Body,
            ItemStack::new(1, &Item::DIAMOND_HORSE_ARMOR),
        );
        horse
            .equipment
            .set(EquipmentSlot::Saddle, ItemStack::new(1, &Item::SADDLE));
        let horse_layers = body(&horse);
        assert!(
            horse_layers
                .iter()
                .any(|layer| layer.model == ModelId::HorseArmor)
        );
        assert!(
            horse_layers
                .iter()
                .any(|layer| layer.model == ModelId::HorseSaddle)
        );

        let mut ghast = spawned(&EntityType::HAPPY_GHAST);
        ghast
            .equipment
            .set(EquipmentSlot::Body, ItemStack::new(1, &Item::WHITE_HARNESS));
        assert!(
            body(&ghast)
                .iter()
                .any(|layer| layer.model == ModelId::HappyGhastHarness)
        );

        let mut zombie = spawned(&EntityType::ZOMBIE);
        zombie.equipment.set(
            EquipmentSlot::Head,
            ItemStack::new(1, &Item::LEATHER_HELMET),
        );
        assert!(body(&zombie).iter().any(|layer| layer.copy_humanoid));
    }

    #[test]
    fn the_armor_glint_goes_with_the_first_layer() {
        use pumpkin_data::data_component_impl::EnchantmentGlintOverrideImpl;
        let empty = ItemStack::EMPTY.clone();
        let mut leather = ItemStack::new(1, &Item::LEATHER_CHESTPLATE);
        leather.set_data_component(EnchantmentGlintOverrideImpl { glint: true });
        let mut layers = Vec::new();
        push_humanoid_armor(
            EntityKind::Player,
            false,
            [&empty, &leather, &empty, &empty],
            &mut layers,
        );
        let foil: Vec<bool> = layers.iter().map(|layer| layer.foil).collect();
        assert_eq!(foil, [true, false], "the dyed leather, not its overlay");
    }

    #[test]
    fn a_zero_count_armor_piece_is_not_worn() {
        let empty = ItemStack::EMPTY.clone();
        let mut leftover = ItemStack::new(1, &Item::DIAMOND_HELMET);
        leftover.item_count = 0;
        assert!(leftover.is_empty());
        assert!(armor_textures(&leftover, ArmorSlot::Head, false).is_empty());
        let mut layers = Vec::new();
        push_humanoid_armor(
            EntityKind::Player,
            false,
            [&leftover, &empty, &empty, &empty],
            &mut layers,
        );
        assert!(layers.is_empty());
    }

    #[test]
    fn the_wool_is_posed_like_the_sheep_under_it() {
        let body = MobModel::new(ModelId::Sheep);
        let wool = MobModel::new(ModelId::SheepFur);
        let input = PoseInput {
            living: LivingRenderState {
                walk_position: 3.0,
                walk_speed: 0.7,
                y_rot: 15.0,
                x_rot: -10.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(wool.model.shares_tree(&body.model));
        assert_eq!(wool.pose(&input), body.pose(&input));
    }

    #[test]
    fn every_mob_stands_on_the_ground_about_as_tall_as_its_box() {
        let cases: [(ModelId, &EntityType, f32); 40] = [
            (ModelId::SnowGolem, &EntityType::SNOW_GOLEM, 1.0),
            (ModelId::Pig, &EntityType::PIG, 1.0),
            (ModelId::IronGolem, &EntityType::IRON_GOLEM, 1.0),
            (ModelId::Rabbit, &EntityType::RABBIT, 1.7),
            (ModelId::RabbitBaby, &EntityType::RABBIT, 1.0),
            (ModelId::Enderman, &EntityType::ENDERMAN, 1.0),
            (ModelId::Endermite, &EntityType::ENDERMITE, 1.0),
            (ModelId::Silverfish, &EntityType::SILVERFISH, 1.7),
            (ModelId::PigCold, &EntityType::PIG, 1.0),
            (ModelId::PigBaby, &EntityType::PIG, 0.5),
            (ModelId::Cow, &EntityType::COW, 1.0),
            (ModelId::CowCold, &EntityType::COW, 1.0),
            (ModelId::CowBaby, &EntityType::COW, 0.5),
            (ModelId::Sheep, &EntityType::SHEEP, 1.0),
            (ModelId::SheepBaby, &EntityType::SHEEP, 0.5),
            (ModelId::Chicken, &EntityType::CHICKEN, 1.0),
            (ModelId::Creeper, &EntityType::CREEPER, 1.0),
            (ModelId::Zombie, &EntityType::ZOMBIE, 1.0),
            (ModelId::ZombieBaby, &EntityType::ZOMBIE, 0.5),
            (ModelId::Skeleton, &EntityType::SKELETON, 1.0),
            (ModelId::Drowned, &EntityType::DROWNED, 1.0),
            (ModelId::Piglin, &EntityType::PIGLIN, 1.0),
            (ModelId::Villager, &EntityType::VILLAGER, 1.0),
            (ModelId::VillagerBaby, &EntityType::VILLAGER, 0.5),
            (ModelId::Witch, &EntityType::WITCH, 1.0),
            (ModelId::ZombieVillager, &EntityType::ZOMBIE_VILLAGER, 1.0),
            (ModelId::PiglinBaby, &EntityType::PIGLIN, 0.5),
            (ModelId::SlimeOuter, &EntityType::SLIME, 1.0),
            (ModelId::MagmaCube, &EntityType::MAGMA_CUBE, 1.0),
            (ModelId::DrownedOuter, &EntityType::DROWNED, 1.0),
            (ModelId::StrayClothes, &EntityType::STRAY, 1.0),
            (ModelId::Wolf, &EntityType::WOLF, 1.0),
            (ModelId::WolfBaby, &EntityType::WOLF, 0.8),
            (ModelId::Feline, &EntityType::OCELOT, 1.0),
            (ModelId::Feline, &EntityType::CAT, 1.25),
            (ModelId::FelineBaby, &EntityType::OCELOT, 0.5),
            (ModelId::Horse, &EntityType::HORSE, 1.35 / 1.1),
            (ModelId::Donkey, &EntityType::DONKEY, 1.25 / 0.87),
            (ModelId::HorseBaby, &EntityType::HORSE, 1.0),
            (ModelId::DonkeyBaby, &EntityType::DONKEY, 0.9),
        ];
        for (id, ty, scale) in cases {
            let (lo, hi) = extent(id);
            let height = ty.dimension[1] * scale;
            assert!(lo.abs() < 0.07, "{id:?} floats or sinks: {lo}");
            assert!(
                hi > height * 0.65 && hi < height * 1.45,
                "{id:?}: {hi} vs box {height}"
            );
        }
        for id in [ModelId::Ghast, ModelId::HappyGhast] {
            let (lo, hi) = extent(id);
            assert!(hi > 3.5 && hi < 4.8, "{id:?} top {hi}");
            assert!(lo < -1.0 && lo > -5.0, "{id:?} tentacles {lo}");
        }
        let (lo, hi) = extent(ModelId::Blaze);
        assert!((lo - 0.25).abs() < 0.07 && hi > 1.5, "blaze {lo}..{hi}");
        let (lo, _) = extent(ModelId::Spider);
        assert!(lo.abs() < 0.2, "spider {lo}");
    }

    #[test]
    fn the_sea_creatures_stand_about_where_their_boxes_are() {
        use ModelId as M;
        for id in [
            M::Cod,
            M::Salmon,
            M::PufferBig,
            M::PufferSmall,
            M::TropicalSmall,
            M::TropicalLarge,
            M::Dolphin,
            M::DolphinBaby,
            M::Turtle,
            M::TurtleBaby,
            M::Tadpole,
            M::Nautilus,
            M::NautilusBaby,
            M::Axolotl,
            M::AxolotlBaby,
            M::Guardian,
            M::ElderGuardian,
            M::Squid,
            M::SquidBaby,
        ] {
            let (lo, hi) = extent(id);
            let squid = matches!(id, M::Squid | M::SquidBaby);
            assert!(lo > if squid { -0.7 } else { -0.2 }, "{id:?} sinks: {lo}");
            assert!(hi > 0.15 && hi < 2.5, "{id:?} height: {hi}");
        }
    }

    #[test]
    fn every_model_bakes_with_its_rig() {
        for id in ModelId::ALL {
            let mob = MobModel::new(id);
            assert!(!mob.model.is_empty());
            assert_eq!(ModelId::ALL[id.index()], id);
        }
    }

    #[test]
    fn walking_moves_the_legs_of_every_rig() {
        let input = PoseInput {
            living: LivingRenderState {
                walk_position: 1.0,
                walk_speed: 1.0,
                ..Default::default()
            },
            humanoid: HumanoidRenderState {
                walk_position: 1.0,
                walk_speed: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        for id in [
            ModelId::Pig,
            ModelId::Chicken,
            ModelId::Creeper,
            ModelId::Spider,
            ModelId::IronGolem,
            ModelId::Enderman,
            ModelId::Zombie,
            ModelId::Wolf,
            ModelId::Feline,
            ModelId::Horse,
            ModelId::Fox,
            ModelId::PolarBear,
            ModelId::Frog,
            ModelId::Goat,
            ModelId::Llama,
            ModelId::Panda,
            ModelId::Hoglin,
            ModelId::Strider,
            ModelId::Ravager,
        ] {
            let mob = MobModel::new(id);
            let rest = mob.pose(&PoseInput::default());
            let walking = mob.pose(&input);
            let moved = (0..mob.model.len()).any(|i| {
                let p = crate::world::entity::model::PartId(i);
                (walking[p].pose.rotation - rest[p].pose.rotation).length() > 0.1
            });
            assert!(moved, "{id:?} doesn't walk");
        }
    }
}
