//! Which renderer an entity type gets.

use pumpkin_data::entity::EntityType;

/// The entity types Melon draws, by how they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Player,
    /// `ClientMannequin`: a player's look with no player behind it, drawn
    /// by the `AvatarRenderer` in the skin its profile names.
    Mannequin,
    Pig,
    Cow,
    /// A cow with a mushroom coat and mushrooms on its back.
    Mooshroom,
    Sheep,
    Chicken,
    Creeper,
    Zombie,
    Skeleton,
    Spider,
    /// Drawn like a zombie, with its own textures and a 1.0625 scale.
    Husk,
    /// A zombie with its own left limbs and an outer layer.
    Drowned,
    /// A skeleton in a clothing layer.
    Stray,
    /// A skeleton scaled by 1.2.
    WitherSkeleton,
    /// A skeleton with mushrooms on its head.
    Bogged,
    /// A spider scaled by 0.7.
    CaveSpider,
    /// `VillagerModel` with biome and profession clothes.
    Villager,
    /// The villager's model in a blue robe.
    WanderingTrader,
    /// The villager's model with a hat and a mole.
    Witch,
    /// A zombie in a villager's head.
    ZombieVillager,
    /// A piglin: ears, snout, a golden sword.
    Piglin,
    PiglinBrute,
    /// A zombie in a piglin's body.
    ZombifiedPiglin,
    /// `WolfModel`: tail, collar, sitting.
    Wolf,
    /// `AdultFelineModel` scaled by 0.8, with a collar.
    Cat,
    Ocelot,
    /// `HorseModel` scaled by 1.1, with markings.
    Horse,
    /// `DonkeyModel` scaled by 0.87.
    Donkey,
    /// `DonkeyModel` scaled by 0.92.
    Mule,
    SkeletonHorse,
    ZombieHorse,
    /// `IllagerModel`, a crossbow in hand.
    Pillager,
    /// `IllagerModel`, an axe.
    Vindicator,
    /// `IllagerModel`, casting spells.
    Evoker,
    /// `IllagerModel` with its hat on.
    Illusioner,
    /// `SlimeModel` in a translucent shell, `size` blocks across.
    Slime,
    /// Eight slices of lava, likewise.
    MagmaCube,
    /// `RabbitModel`: hops by keyframe animation.
    Rabbit,
    /// `BatModel`: flies and hangs by keyframe animations.
    Bat,
    /// `EndermiteModel`: four segments.
    Endermite,
    /// `SilverfishModel`: seven segments and three layers.
    Silverfish,
    /// `IronGolemModel`.
    IronGolem,
    /// `EndermanModel`: a humanoid with long limbs.
    Enderman,
    /// `BlazeModel`: a head and three rings of rods; glows.
    Blaze,
    /// `SnowGolemModel`: three balls of snow, a pumpkin on the head.
    SnowGolem,
    /// `GhastModel`: a cube with tentacles, 4.5 times over.
    Ghast,
    /// `HappyGhastModel`: a gentle ghast with a harness for riders.
    HappyGhast,
    /// `SquidModel`: tumbles through the water on its tentacles.
    Squid,
    /// A squid that lights up.
    GlowSquid,
    /// `CodModel`.
    Cod,
    /// `SalmonModel` in one of three sizes.
    Salmon,
    /// `PufferfishRenderer`: three models by how puffed it is.
    Pufferfish,
    /// Two shapes, a pattern over each, two colours.
    TropicalFish,
    /// `DolphinModel`.
    Dolphin,
    /// `TurtleModel`: rows through the water and shuffles on land.
    Turtle,
    Guardian,
    /// A guardian 2.35 times over.
    ElderGuardian,
    Tadpole,
    /// `NautilusModel`.
    Nautilus,
    Axolotl,
    /// `AdultFoxModel` / `BabyFoxModel`.
    Fox,
    /// `FrogModel`.
    Frog,
    /// `AdultCamelModel` / `BabyCamelModel`.
    Camel,
    /// `AdultArmadilloModel` / `BabyArmadilloModel`.
    Armadillo,
    /// `PolarBearModel`, adults scaled by 1.2.
    PolarBear,
    /// `AdultBeeModel` / `BabyBeeModel`.
    Bee,
    /// `GoatModel` / `BabyGoatModel`.
    Goat,
    /// `LlamaModel` / `BabyLlamaModel`.
    Llama,
    /// The wandering trader's llama.
    TraderLlama,
    /// `PandaModel` / `BabyPandaModel`.
    Panda,
    /// `ParrotModel`.
    Parrot,
    /// `HoglinModel` / `BabyHoglinModel`.
    Hoglin,
    /// A hoglin after zombification.
    Zoglin,
    /// `PhantomModel`.
    Phantom,
    /// `VexModel`.
    Vex,
    /// `AllayModel`.
    Allay,
    /// `AdultStriderModel` / `BabyStriderModel`.
    Strider,
    /// `RavagerModel`.
    Ravager,
    /// `WitherBossModel`.
    Wither,
    /// `EnderDragonModel`.
    EnderDragon,
    /// `ShulkerModel`.
    Shulker,
    /// `SnifferModel` / `SniffletModel`.
    Sniffer,
    /// `WardenModel`.
    Warden,
    /// `BreezeModel`.
    Breeze,
    /// `CreakingModel`.
    Creaking,
    /// `CopperGolemModel`.
    CopperGolem,
    /// The camel's model in a husk's coat.
    CamelHusk,
    /// `ArmorStand`: a posed humanoid frame, often invisible as a hologram.
    ArmorStand,
    /// `LeashFenceKnotEntity`: the knot a lead is tied to a fence with.
    LeashKnot,
    /// Every boat and raft, with or without a chest.
    Boat,
    /// Every minecart: `MinecartModel` (the contents are not drawn yet).
    Minecart,
    /// `FallingBlockEntity`: a block state's model, falling.
    FallingBlock,
    /// `PrimedTnt`: a block that flashes and swells until it blows.
    PrimedTnt,
    /// `ItemEntity`: a dropped stack, spinning and bobbing.
    Item,
    /// `Display.ItemDisplay`, `BlockDisplay` and `TextDisplay`.
    ItemDisplay,
    BlockDisplay,
    TextDisplay,
    /// `ItemFrame` and `GlowItemFrame`: the frame and what is in it.
    ItemFrame,
    GlowItemFrame,
    /// `Interaction`: an invisible box only the crosshair sees.
    Interaction,
    /// `FishingHook`: the bobber of a rod cast, drawn with its line.
    FishingBobber,
    /// `SulfurCubeModel` / `SmallSulfurCubeModel`: a cube mob that swallows
    /// blocks and swells like TNT before it blows.
    SulfurCube,
    /// A skeleton in its own coat.
    Parched,
    /// `GiantMobRenderer`: a zombie six times over.
    Giant,
    /// The nautilus's model in rotten flesh, or with corals for the warm one.
    ZombieNautilus,
    /// `CushionModel`: a dyed pillow on the ground.
    Cushion,
    /// `WitherSkullRenderer`: the wither's head, flying.
    WitherSkull,
    /// `ShulkerBulletModel` in a translucent glow.
    ShulkerBullet,
    /// `LlamaSpitModel`.
    LlamaSpit,
    /// `WindChargeModel`: the player's and the breeze's.
    WindCharge,
    /// `DragonFireballRenderer`: a sprite facing the camera.
    DragonFireball,
    /// `EvokerFangsModel`: jaws that snap up out of the ground.
    EvokerFangs,
    /// `EndCrystalModel`: spinning glass, and the beam to what it heals.
    EndCrystal,
    /// `TridentModel`, thrown.
    Trident,
    /// `ArrowModel` with the plain or tipped texture.
    Arrow,
    /// `ArrowModel` with the spectral texture.
    SpectralArrow,
    /// `ExperienceOrbRenderer`: a sprite by value, shimmering.
    ExperienceOrb,
    /// `ThrownItemRenderer`: its item facing the camera (snowballs, eggs,
    /// pearls, potions, eyes of ender and fireballs).
    ThrownItem,
    /// `FireworkEntityRenderer`.
    FireworkRocket,
    /// `OminousItemSpawnerRenderer`: a spinning item that grows in.
    OminousItemSpawner,
    /// `PaintingRenderer`.
    Painting,
    /// `LightningBoltRenderer`.
    LightningBolt,
    /// Tracked but not drawn yet.
    Other,
}

/// What a renderer fixes about its model, whatever the entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// `shadowRadius`, before a baby halves it.
    pub shadow: f32,
    /// Scales the model; a baby's own model is already small, so most of them keep the adult
    /// scale.
    pub scale: f32,
    pub adult_scale: bool,
    /// How far a dying entity tips: 180 for those that end on their back.
    pub flip_degrees: f32,
}

impl Look {
    const fn new(shadow: f32) -> Self {
        Self {
            shadow,
            scale: 1.0,
            adult_scale: false,
            flip_degrees: 90.0,
        }
    }

    const fn scaled(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// Scales adults only: the baby model is built smaller already.
    const fn adults_scaled(self, scale: f32) -> Self {
        Self {
            scale,
            adult_scale: true,
            ..self
        }
    }

    const fn on_back(self) -> Self {
        Self {
            flip_degrees: 180.0,
            ..self
        }
    }
}

impl EntityKind {
    /// Drawn as one block's model: a falling block, primed TNT.
    pub fn is_block(self) -> bool {
        matches!(self, Self::FallingBlock | Self::PrimedTnt)
    }

    /// `MoveSimulationType.SERVER_AND_CLIENT`.
    pub fn simulates_movement(self) -> bool {
        self.is_block() || self == Self::Item
    }

    /// The renderer's fixed values for this kind.
    pub const fn look(self) -> Look {
        match self {
            Self::Pig | Self::Cow | Self::Mooshroom | Self::Sheep => Look::new(0.7),
            Self::Chicken => Look::new(0.3),
            Self::Bat => Look::new(0.25),
            Self::Rabbit => Look::new(0.3),
            Self::Spider => Look::new(0.8).on_back(),
            Self::CaveSpider => Look::new(0.56).scaled(0.7).on_back(),
            Self::Endermite | Self::Silverfish => Look::new(0.3).on_back(),
            Self::IronGolem => Look::new(0.7),
            Self::Blaze | Self::SnowGolem => Look::new(0.5),
            Self::Ghast => Look::new(1.5),
            Self::HappyGhast => Look::new(2.0),
            Self::Squid | Self::GlowSquid => Look::new(0.7),
            Self::Cod => Look::new(0.3),
            Self::Salmon => Look::new(0.4),
            Self::Pufferfish => Look::new(0.2),
            Self::TropicalFish => Look::new(0.15),
            Self::Dolphin => Look::new(0.7),
            Self::Turtle => Look::new(0.7),
            Self::Guardian => Look::new(0.5),
            Self::ElderGuardian => Look::new(1.2),
            Self::Tadpole => Look::new(0.14),
            Self::Nautilus => Look::new(0.7),
            Self::Axolotl => Look::new(0.5),
            Self::Wolf => Look::new(0.5),
            Self::Horse => Look::new(0.75).adults_scaled(1.1),
            Self::Donkey => Look::new(0.75).adults_scaled(0.87),
            Self::Mule => Look::new(0.75).adults_scaled(0.92),
            Self::SkeletonHorse | Self::ZombieHorse => Look::new(0.75),
            Self::Cat => Look::new(0.4).adults_scaled(0.8),
            Self::Ocelot => Look::new(0.4),
            Self::Slime | Self::MagmaCube => Look::new(0.0),
            Self::Witch | Self::Pillager | Self::Vindicator | Self::Evoker | Self::Illusioner => {
                Look::new(0.5).scaled(0.9375)
            }
            Self::Villager | Self::WanderingTrader => Look::new(0.5).adults_scaled(0.9375),
            Self::ZombieVillager => Look::new(0.5),
            Self::Player | Self::Mannequin => Look::new(0.5).scaled(0.9375),
            Self::Husk => Look::new(0.5).adults_scaled(1.0625),
            Self::WitherSkeleton => Look::new(0.5).scaled(1.2),
            Self::Fox => Look::new(0.4),
            Self::Frog => Look::new(0.3),
            Self::Camel | Self::CamelHusk => Look::new(0.7),
            Self::Armadillo => Look::new(0.4),
            Self::PolarBear => Look::new(0.9).adults_scaled(1.2),
            Self::Bee | Self::Allay => Look::new(0.4),
            Self::Goat | Self::Llama | Self::TraderLlama | Self::Hoglin | Self::Zoglin => {
                Look::new(0.7)
            }
            Self::Panda | Self::Warden => Look::new(0.9),
            Self::Parrot | Self::Vex => Look::new(0.3),
            Self::Phantom => Look::new(0.75),
            Self::Strider | Self::Breeze | Self::CopperGolem => Look::new(0.5),
            Self::Ravager => Look::new(1.1),
            Self::Wither => Look::new(1.0),
            Self::EnderDragon => Look::new(0.5),
            Self::Shulker => Look::new(0.0),
            Self::Sniffer => Look::new(1.1),
            Self::Creaking => Look::new(0.6),
            Self::Creeper
            | Self::Enderman
            | Self::Zombie
            | Self::Drowned
            | Self::Piglin
            | Self::PiglinBrute
            | Self::ZombifiedPiglin
            | Self::Skeleton
            | Self::Stray
            | Self::Bogged => Look::new(0.5),
            Self::FallingBlock | Self::PrimedTnt => Look::new(0.5),
            Self::Item => Look::new(0.15),
            Self::Boat => Look::new(0.8),
            Self::Minecart => Look::new(0.7),
            Self::ArmorStand => Look::new(0.0),
            Self::SulfurCube => Look::new(0.25),
            Self::Parched => Look::new(0.5),
            Self::Giant => Look::new(3.0).scaled(6.0),
            Self::ZombieNautilus => Look::new(0.7),
            Self::EndCrystal => Look::new(0.5),
            Self::ExperienceOrb => Look::new(0.15),
            Self::Cushion
            | Self::WitherSkull
            | Self::ShulkerBullet
            | Self::LlamaSpit
            | Self::WindCharge
            | Self::DragonFireball
            | Self::EvokerFangs
            | Self::Trident
            | Self::Arrow
            | Self::SpectralArrow
            | Self::ThrownItem
            | Self::FireworkRocket
            | Self::OminousItemSpawner
            | Self::Painting
            | Self::LightningBolt => Look::new(0.0),
            Self::LeashKnot
            | Self::Other
            | Self::ItemDisplay
            | Self::BlockDisplay
            | Self::TextDisplay
            | Self::ItemFrame
            | Self::GlowItemFrame
            | Self::Interaction
            | Self::FishingBobber => Look::new(0.0),
        }
    }

    pub fn of(ty: &EntityType) -> Self {
        if ty.boat.is_some() {
            return Self::Boat;
        }
        if ty.resource_name.ends_with("minecart") {
            return Self::Minecart;
        }
        let kinds: [(&EntityType, Self); 130] = [
            (&EntityType::PLAYER, Self::Player),
            (&EntityType::MANNEQUIN, Self::Mannequin),
            (&EntityType::PIG, Self::Pig),
            (&EntityType::COW, Self::Cow),
            (&EntityType::MOOSHROOM, Self::Mooshroom),
            (&EntityType::SHEEP, Self::Sheep),
            (&EntityType::CHICKEN, Self::Chicken),
            (&EntityType::CREEPER, Self::Creeper),
            (&EntityType::ZOMBIE, Self::Zombie),
            (&EntityType::SKELETON, Self::Skeleton),
            (&EntityType::SPIDER, Self::Spider),
            (&EntityType::HUSK, Self::Husk),
            (&EntityType::DROWNED, Self::Drowned),
            (&EntityType::STRAY, Self::Stray),
            (&EntityType::WITHER_SKELETON, Self::WitherSkeleton),
            (&EntityType::BOGGED, Self::Bogged),
            (&EntityType::CAVE_SPIDER, Self::CaveSpider),
            (&EntityType::VILLAGER, Self::Villager),
            (&EntityType::WANDERING_TRADER, Self::WanderingTrader),
            (&EntityType::WITCH, Self::Witch),
            (&EntityType::ZOMBIE_VILLAGER, Self::ZombieVillager),
            (&EntityType::PIGLIN, Self::Piglin),
            (&EntityType::PIGLIN_BRUTE, Self::PiglinBrute),
            (&EntityType::ZOMBIFIED_PIGLIN, Self::ZombifiedPiglin),
            (&EntityType::HORSE, Self::Horse),
            (&EntityType::DONKEY, Self::Donkey),
            (&EntityType::MULE, Self::Mule),
            (&EntityType::SKELETON_HORSE, Self::SkeletonHorse),
            (&EntityType::ZOMBIE_HORSE, Self::ZombieHorse),
            (&EntityType::WOLF, Self::Wolf),
            (&EntityType::CAT, Self::Cat),
            (&EntityType::OCELOT, Self::Ocelot),
            (&EntityType::PILLAGER, Self::Pillager),
            (&EntityType::VINDICATOR, Self::Vindicator),
            (&EntityType::EVOKER, Self::Evoker),
            (&EntityType::ILLUSIONER, Self::Illusioner),
            (&EntityType::SLIME, Self::Slime),
            (&EntityType::MAGMA_CUBE, Self::MagmaCube),
            (&EntityType::RABBIT, Self::Rabbit),
            (&EntityType::BAT, Self::Bat),
            (&EntityType::ENDERMITE, Self::Endermite),
            (&EntityType::SILVERFISH, Self::Silverfish),
            (&EntityType::IRON_GOLEM, Self::IronGolem),
            (&EntityType::ENDERMAN, Self::Enderman),
            (&EntityType::BLAZE, Self::Blaze),
            (&EntityType::SNOW_GOLEM, Self::SnowGolem),
            (&EntityType::GHAST, Self::Ghast),
            (&EntityType::HAPPY_GHAST, Self::HappyGhast),
            (&EntityType::SQUID, Self::Squid),
            (&EntityType::GLOW_SQUID, Self::GlowSquid),
            (&EntityType::COD, Self::Cod),
            (&EntityType::SALMON, Self::Salmon),
            (&EntityType::PUFFERFISH, Self::Pufferfish),
            (&EntityType::TROPICAL_FISH, Self::TropicalFish),
            (&EntityType::DOLPHIN, Self::Dolphin),
            (&EntityType::TURTLE, Self::Turtle),
            (&EntityType::GUARDIAN, Self::Guardian),
            (&EntityType::ELDER_GUARDIAN, Self::ElderGuardian),
            (&EntityType::TADPOLE, Self::Tadpole),
            (&EntityType::NAUTILUS, Self::Nautilus),
            (&EntityType::AXOLOTL, Self::Axolotl),
            (&EntityType::FOX, Self::Fox),
            (&EntityType::FROG, Self::Frog),
            (&EntityType::CAMEL, Self::Camel),
            (&EntityType::ARMADILLO, Self::Armadillo),
            (&EntityType::POLAR_BEAR, Self::PolarBear),
            (&EntityType::BEE, Self::Bee),
            (&EntityType::GOAT, Self::Goat),
            (&EntityType::LLAMA, Self::Llama),
            (&EntityType::TRADER_LLAMA, Self::TraderLlama),
            (&EntityType::PANDA, Self::Panda),
            (&EntityType::PARROT, Self::Parrot),
            (&EntityType::HOGLIN, Self::Hoglin),
            (&EntityType::ZOGLIN, Self::Zoglin),
            (&EntityType::PHANTOM, Self::Phantom),
            (&EntityType::VEX, Self::Vex),
            (&EntityType::ALLAY, Self::Allay),
            (&EntityType::STRIDER, Self::Strider),
            (&EntityType::RAVAGER, Self::Ravager),
            (&EntityType::WITHER, Self::Wither),
            (&EntityType::ENDER_DRAGON, Self::EnderDragon),
            (&EntityType::SHULKER, Self::Shulker),
            (&EntityType::SNIFFER, Self::Sniffer),
            (&EntityType::WARDEN, Self::Warden),
            (&EntityType::BREEZE, Self::Breeze),
            (&EntityType::CREAKING, Self::Creaking),
            (&EntityType::COPPER_GOLEM, Self::CopperGolem),
            (&EntityType::CAMEL_HUSK, Self::CamelHusk),
            (&EntityType::LEASH_KNOT, Self::LeashKnot),
            (&EntityType::ARMOR_STAND, Self::ArmorStand),
            (&EntityType::FALLING_BLOCK, Self::FallingBlock),
            (&EntityType::TNT, Self::PrimedTnt),
            (&EntityType::ITEM, Self::Item),
            (&EntityType::ITEM_DISPLAY, Self::ItemDisplay),
            (&EntityType::BLOCK_DISPLAY, Self::BlockDisplay),
            (&EntityType::TEXT_DISPLAY, Self::TextDisplay),
            (&EntityType::ITEM_FRAME, Self::ItemFrame),
            (&EntityType::GLOW_ITEM_FRAME, Self::GlowItemFrame),
            (&EntityType::INTERACTION, Self::Interaction),
            (&EntityType::FISHING_BOBBER, Self::FishingBobber),
            (&EntityType::SULFUR_CUBE, Self::SulfurCube),
            (&EntityType::PARCHED, Self::Parched),
            (&EntityType::GIANT, Self::Giant),
            (&EntityType::ZOMBIE_NAUTILUS, Self::ZombieNautilus),
            (&EntityType::CUSHION, Self::Cushion),
            (&EntityType::WITHER_SKULL, Self::WitherSkull),
            (&EntityType::SHULKER_BULLET, Self::ShulkerBullet),
            (&EntityType::LLAMA_SPIT, Self::LlamaSpit),
            (&EntityType::WIND_CHARGE, Self::WindCharge),
            (&EntityType::BREEZE_WIND_CHARGE, Self::WindCharge),
            (&EntityType::DRAGON_FIREBALL, Self::DragonFireball),
            (&EntityType::EVOKER_FANGS, Self::EvokerFangs),
            (&EntityType::END_CRYSTAL, Self::EndCrystal),
            (&EntityType::TRIDENT, Self::Trident),
            (&EntityType::ARROW, Self::Arrow),
            (&EntityType::SPECTRAL_ARROW, Self::SpectralArrow),
            (&EntityType::EXPERIENCE_ORB, Self::ExperienceOrb),
            (&EntityType::SNOWBALL, Self::ThrownItem),
            (&EntityType::EGG, Self::ThrownItem),
            (&EntityType::ENDER_PEARL, Self::ThrownItem),
            (&EntityType::EXPERIENCE_BOTTLE, Self::ThrownItem),
            (&EntityType::SPLASH_POTION, Self::ThrownItem),
            (&EntityType::LINGERING_POTION, Self::ThrownItem),
            (&EntityType::EYE_OF_ENDER, Self::ThrownItem),
            (&EntityType::FIREBALL, Self::ThrownItem),
            (&EntityType::SMALL_FIREBALL, Self::ThrownItem),
            (&EntityType::FIREWORK_ROCKET, Self::FireworkRocket),
            (&EntityType::OMINOUS_ITEM_SPAWNER, Self::OminousItemSpawner),
            (&EntityType::PAINTING, Self::Painting),
            (&EntityType::LIGHTNING_BOLT, Self::LightningBolt),
        ];
        kinds
            .into_iter()
            .find(|(t, _)| t.id == ty.id)
            .map_or(Self::Other, |(_, kind)| kind)
    }

    /// An item frame, glowing or not.
    pub fn is_item_frame(self) -> bool {
        matches!(self, Self::ItemFrame | Self::GlowItemFrame)
    }

    /// An `Avatar`: drawn by the `AvatarRenderer`, a player or a mannequin.
    pub fn is_avatar(self) -> bool {
        matches!(self, Self::Player | Self::Mannequin)
    }

    /// A `Display` entity.
    pub fn is_display(self) -> bool {
        matches!(
            self,
            Self::ItemDisplay | Self::BlockDisplay | Self::TextDisplay
        )
    }

    /// Whether it is a `Mob`, whose body turns by `BodyRotationControl`
    pub fn is_mob(self) -> bool {
        !matches!(
            self,
            Self::Player
                | Self::Mannequin
                | Self::LeashKnot
                | Self::ArmorStand
                | Self::FallingBlock
                | Self::PrimedTnt
                | Self::Item
                | Self::ItemDisplay
                | Self::BlockDisplay
                | Self::TextDisplay
                | Self::ItemFrame
                | Self::GlowItemFrame
                | Self::Boat
                | Self::Minecart
                | Self::Other
        ) && !self.is_projectile()
            && !matches!(
                self,
                Self::Cushion | Self::EndCrystal | Self::Painting | Self::LightningBolt
            )
    }

    /// A `Projectile` (or an orb, a spawner): not alive, drawn by its own
    /// renderer rather than `LivingEntityRenderer`.
    pub fn is_projectile(self) -> bool {
        matches!(
            self,
            Self::WitherSkull
                | Self::ShulkerBullet
                | Self::LlamaSpit
                | Self::WindCharge
                | Self::DragonFireball
                | Self::EvokerFangs
                | Self::Trident
                | Self::Arrow
                | Self::SpectralArrow
                | Self::ExperienceOrb
                | Self::ThrownItem
                | Self::FireworkRocket
                | Self::OminousItemSpawner
        )
    }

    /// Drawn as a camera-facing sprite or item, not a model.
    pub fn is_sprite(self) -> bool {
        matches!(
            self,
            Self::LightningBolt
                | Self::DragonFireball
                | Self::ExperienceOrb
                | Self::ThrownItem
                | Self::FireworkRocket
                | Self::OminousItemSpawner
        )
    }

    /// A `VehicleEntity` (a boat or a minecart).
    pub fn is_vehicle(self) -> bool {
        matches!(self, Self::Boat | Self::Minecart)
    }

    /// The saddle and armor slots of the mount menu `handleMountScreenOpen`
    /// opens (`AbstractHorse`s and `AbstractNautilus`es only), by the
    /// `can_equip_saddle` and `can_wear_horse_armor` tags; a llama's carpet
    /// takes the armor slot. Tameness and age are not tracked, so the slots
    /// show whatever `canUseSlot` says.
    pub fn mount_slots(self) -> Option<(bool, bool)> {
        match self {
            Self::Horse | Self::ZombieHorse | Self::Nautilus | Self::ZombieNautilus => {
                Some((true, true))
            }
            Self::Donkey | Self::Mule | Self::SkeletonHorse | Self::Camel | Self::CamelHusk => {
                Some((true, false))
            }
            Self::Llama | Self::TraderLlama => Some((false, true)),
            _ => None,
        }
    }

    /// `LivingEntity.getAgeScale` applies: `AgeableMob`s and zombies.
    pub fn can_be_baby(self) -> bool {
        matches!(
            self,
            Self::Pig
                | Self::Rabbit
                | Self::Cow
                | Self::Mooshroom
                | Self::Wolf
                | Self::Horse
                | Self::Donkey
                | Self::Mule
                | Self::SkeletonHorse
                | Self::ZombieHorse
                | Self::Cat
                | Self::Ocelot
                | Self::Sheep
                | Self::Chicken
                | Self::Zombie
                | Self::Husk
                | Self::Drowned
                | Self::Piglin
                | Self::ZombifiedPiglin
                | Self::Villager
                | Self::ZombieVillager
                | Self::HappyGhast
                | Self::Squid
                | Self::GlowSquid
                | Self::Dolphin
                | Self::Turtle
                | Self::Nautilus
                | Self::Axolotl
                | Self::Fox
                | Self::Camel
                | Self::CamelHusk
                | Self::Armadillo
                | Self::PolarBear
                | Self::Bee
                | Self::Goat
                | Self::Llama
                | Self::TraderLlama
                | Self::Panda
                | Self::Hoglin
                | Self::Zoglin
                | Self::Strider
                | Self::Sniffer
                | Self::SulfurCube
                | Self::ZombieNautilus
        )
    }

    /// Lives in the water: whether it is in it changes how it moves.
    pub fn is_aquatic(self) -> bool {
        matches!(
            self,
            Self::Squid
                | Self::GlowSquid
                | Self::Cod
                | Self::Salmon
                | Self::Pufferfish
                | Self::TropicalFish
                | Self::Dolphin
                | Self::Turtle
                | Self::Guardian
                | Self::ElderGuardian
                | Self::Tadpole
                | Self::Nautilus
                | Self::Axolotl
                | Self::Frog
        )
    }

    pub fn is_squid(self) -> bool {
        matches!(self, Self::Squid | Self::GlowSquid)
    }

    /// `AbstractCubeMob`: sized by metadata, squashes when it lands.
    pub fn is_cube(self) -> bool {
        matches!(self, Self::Slime | Self::MagmaCube | Self::SulfurCube)
    }

    /// Drawn with the zombie's model and arms.
    pub fn is_zombie(self) -> bool {
        matches!(
            self,
            Self::Zombie
                | Self::Husk
                | Self::Drowned
                | Self::ZombifiedPiglin
                | Self::ZombieVillager
                | Self::Giant
        )
    }

    /// Drawn with a piglin's model.
    pub fn is_piglin(self) -> bool {
        matches!(
            self,
            Self::Piglin | Self::PiglinBrute | Self::ZombifiedPiglin
        )
    }

    /// A `TamableAnimal` with the tamed and sitting flags.
    pub fn is_tamable(self) -> bool {
        matches!(self, Self::Wolf | Self::Cat | Self::Parrot)
    }

    /// Drawn with `AbstractEquineModel`.
    pub fn is_equine(self) -> bool {
        matches!(
            self,
            Self::Horse | Self::Donkey | Self::Mule | Self::SkeletonHorse | Self::ZombieHorse
        )
    }

    /// Drawn with `IllagerModel`.
    pub fn is_illager(self) -> bool {
        matches!(
            self,
            Self::Pillager | Self::Vindicator | Self::Evoker | Self::Illusioner
        )
    }

    /// Drawn with `SkeletonModel`, which holds a bow.
    pub fn is_skeleton(self) -> bool {
        matches!(
            self,
            Self::Skeleton | Self::Stray | Self::WitherSkeleton | Self::Bogged | Self::Parched
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_come_from_the_registry() {
        let pig = EntityType::from_raw(EntityType::PIG.id).unwrap();
        assert_eq!(EntityKind::of(pig), EntityKind::Pig);
        assert_eq!(EntityKind::of(&EntityType::ARROW), EntityKind::Arrow);
        assert_eq!(EntityKind::of(&EntityType::MARKER), EntityKind::Other);
        assert_eq!(
            EntityKind::of(&EntityType::WITHER_SKULL),
            EntityKind::WitherSkull
        );
        assert_eq!(
            EntityKind::of(&EntityType::SULFUR_CUBE),
            EntityKind::SulfurCube
        );
        assert!(EntityKind::SulfurCube.is_cube() && EntityKind::SulfurCube.is_mob());
        assert!(!EntityKind::WitherSkull.is_mob());
        assert_eq!(
            EntityKind::of(&EntityType::MOOSHROOM),
            EntityKind::Mooshroom
        );
        assert!(EntityKind::Mooshroom.can_be_baby());
        assert_eq!(EntityKind::of(&EntityType::FOX), EntityKind::Fox);
        assert_eq!(EntityKind::of(&EntityType::BOGGED), EntityKind::Bogged);
        assert_eq!(
            EntityKind::of(&EntityType::ILLUSIONER),
            EntityKind::Illusioner
        );
        assert!(EntityKind::Fox.can_be_baby());
        assert!(EntityKind::Bogged.is_skeleton());
        assert!(EntityKind::Illusioner.is_illager());
        assert_eq!(EntityKind::of(&EntityType::BEE), EntityKind::Bee);
        assert_eq!(EntityKind::of(&EntityType::WARDEN), EntityKind::Warden);
        assert_eq!(
            EntityKind::of(&EntityType::ENDER_DRAGON),
            EntityKind::EnderDragon
        );
        assert_eq!(
            EntityKind::of(&EntityType::CAMEL_HUSK),
            EntityKind::CamelHusk
        );
        assert!(EntityKind::Sniffer.can_be_baby());
        assert!(EntityKind::Parrot.is_tamable());
    }

    /// `HorseInventoryMenu`'s slots by the saddle and horse armor tags; no
    /// menu for anything else (a strider has a saddle but no inventory).
    #[test]
    fn mount_menus_follow_the_vanilla_tags() {
        use EntityKind as K;
        assert_eq!(K::Horse.mount_slots(), Some((true, true)));
        assert_eq!(K::ZombieNautilus.mount_slots(), Some((true, true)));
        assert_eq!(K::SkeletonHorse.mount_slots(), Some((true, false)));
        assert_eq!(K::CamelHusk.mount_slots(), Some((true, false)));
        assert_eq!(K::TraderLlama.mount_slots(), Some((false, true)));
        assert_eq!(K::Strider.mount_slots(), None);
        assert_eq!(K::Pig.mount_slots(), None);
    }
}
