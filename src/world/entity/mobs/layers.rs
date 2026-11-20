//! Which layers a mob shows and with which texture: variants, coats,

use super::*;

/// `DyeColor.getTextureDiffuseColor` as factors (collars).
pub(super) fn dye_tint(color: pumpkin_data::dye_color::DyeColor) -> [f32; 3] {
    let rgb = color.texture_diffuse_color();
    [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0)
}

/// A wolf's textures by age and state: wild, tame, angry.
macro_rules! wolf_set {
    ($base:literal) => {
        [
            [
                concat!("minecraft:entity/wolf/", $base),
                concat!("minecraft:entity/wolf/", $base, "_tame"),
                concat!("minecraft:entity/wolf/", $base, "_angry"),
            ],
            [
                concat!("minecraft:entity/wolf/", $base, "_baby"),
                concat!("minecraft:entity/wolf/", $base, "_tame_baby"),
                concat!("minecraft:entity/wolf/", $base, "_angry_baby"),
            ],
        ]
    };
}

/// `WolfVariant` in registry order: ashen, black, chestnut, pale (the
pub(super) const WOLF_TEXTURES: [[[&str; 3]; 2]; 9] = [
    wolf_set!("wolf_ashen"),
    wolf_set!("wolf_black"),
    wolf_set!("wolf_chestnut"),
    wolf_set!("wolf"),
    wolf_set!("wolf_rusty"),
    wolf_set!("wolf_snowy"),
    wolf_set!("wolf_spotted"),
    wolf_set!("wolf_striped"),
    wolf_set!("wolf_woods"),
];

/// `Variant` by id, adult and foal.
pub(super) const HORSE_TEXTURES: [[&str; 2]; 7] = {
    macro_rules! coats {
        ($($v:literal),*) => { [$([
            concat!("minecraft:entity/horse/horse_", $v),
            concat!("minecraft:entity/horse/horse_", $v, "_baby"),
        ]),*] };
    }
    coats!(
        "white",
        "creamy",
        "chestnut",
        "brown",
        "black",
        "gray",
        "darkbrown"
    )
};

/// `Markings` after `NONE`, adult and foal.
pub(super) const HORSE_MARKINGS: [[&str; 2]; 4] = {
    macro_rules! markings {
        ($($v:literal),*) => { [$([
            concat!("minecraft:entity/horse/horse_markings_", $v),
            concat!("minecraft:entity/horse/horse_markings_", $v, "_baby"),
        ]),*] };
    }
    markings!("white", "whitefield", "whitedots", "blackdots")
};

/// `CatVariant` (registry order) by age.
pub(super) const CAT_TEXTURES: [[&str; 2]; 11] = {
    macro_rules! cats {
        ($($v:literal),*) => { [$([
            concat!("minecraft:entity/cat/cat_", $v),
            concat!("minecraft:entity/cat/cat_", $v, "_baby"),
        ]),*] };
    }
    cats!(
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
    )
};

/// `CrossbowItem.getChargeDuration` without Quick Charge (1.25 seconds).
pub(super) const CROSSBOW_CHARGE_TICKS: f32 = 25.0;

/// One model drawn for an entity: with which texture (an asset id), its
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSpec {
    pub model: ModelId,
    pub texture: Cow<'static, str>,
    pub tint: [f32; 3],
    pub full_bright: bool,
    /// Blended over what is behind it (a slime's jelly).
    pub translucent: bool,
    /// Writes only the depth, no colour (`RenderTypes.waterMask`): the
    pub water_mask: bool,
    /// Humanoid armor: posed by copying the wearer, not `setupAnim`.
    pub copy_humanoid: bool,
    /// `ItemStack.hasFoil`: the armor or equipment glint overlay.
    pub foil: bool,
    /// Its own overlay instead of the body's (a sulfur cube's lit core).
    pub overlay: Option<f32>,
    /// The colour's alpha it is drawn with (a shulker bullet's glow).
    pub alpha: f32,
}

impl LayerSpec {
    /// The adult's or, for a baby, the young one's model and texture.
    pub(super) const fn aged(
        baby: bool,
        adult: (ModelId, &'static str),
        young: (ModelId, &'static str),
    ) -> Self {
        let (model, texture) = if baby { young } else { adult };
        Self::plain(model, texture)
    }

    /// An equipment texture whose path is built from the item.
    pub(super) fn owned(model: ModelId, texture: String, tint: [f32; 3]) -> Self {
        Self {
            model,
            texture: Cow::Owned(texture),
            tint,
            full_bright: false,
            translucent: false,
            water_mask: false,
            copy_humanoid: false,
            foil: false,
            overlay: None,
            alpha: 1.0,
        }
    }

    pub(super) fn humanoid_armor(model: ModelId, texture: String, tint: [f32; 3]) -> Self {
        Self {
            copy_humanoid: true,
            ..Self::owned(model, texture, tint)
        }
    }

    pub(super) fn with_foil(self, foil: bool) -> Self {
        Self { foil, ..self }
    }

    /// Drawn at full brightness (`RenderTypes.eyes`).
    pub(super) fn glowing(self) -> Self {
        Self {
            full_bright: true,
            ..self
        }
    }

    /// Blended over what is behind it.
    pub(super) fn translucent(self) -> Self {
        Self {
            translucent: true,
            ..self
        }
    }

    /// Only the depth (`RenderTypes.waterMask`).
    pub(super) fn water_mask(self) -> Self {
        Self {
            water_mask: true,
            ..self
        }
    }

    /// Multiplied by `tint` (a collar's dye).
    pub(super) fn with_tint(self, tint: [f32; 3]) -> Self {
        Self { tint, ..self }
    }

    pub(super) const fn plain(model: ModelId, texture: &'static str) -> Self {
        Self {
            model,
            texture: Cow::Borrowed(texture),
            tint: [1.0; 3],
            full_bright: false,
            translucent: false,
            water_mask: false,
            copy_humanoid: false,
            foil: false,
            overlay: None,
            alpha: 1.0,
        }
    }

    /// Drawn with `overlay` rather than the body's.
    pub(super) fn with_overlay(self, overlay: f32) -> Self {
        Self {
            overlay: Some(overlay),
            ..self
        }
    }

    /// Blended at `alpha`.
    pub(super) fn faded(self, alpha: f32) -> Self {
        Self {
            alpha,
            translucent: true,
            ..self
        }
    }
}

/// The variant's model (or `None` for the shared one) and texture, adult
pub(super) fn by_variant<T: Copy>(variant: i32, table: [T; 3]) -> T {
    table[usize::try_from(variant)
        .ok()
        .filter(|&i| i < 3)
        .unwrap_or(1)]
}

/// A variant mob's body: the model and textures of its variant, or the
pub(super) fn variant_body(
    baby: bool,
    variant: i32,
    young: ModelId,
    table: [(ModelId, &'static str, &'static str); 3],
) -> LayerSpec {
    let (model, adult, baby_texture) = by_variant(variant, table);
    LayerSpec::aged(baby, (model, adult), (young, baby_texture))
}

/// The body layers of `entity`, the first one the main body, into `out`.
pub fn layers(
    entity: &ClientEntity,
    age_in_ticks: f32,
    hat: &dyn Fn(&str) -> villager::Hat,
    out: &mut Vec<LayerSpec>,
) {
    use ModelId as M;
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Pig => {
            debug_assert_eq!(PigVariant::Cold as i32, 0);
            out.push(variant_body(
                baby,
                a.variant,
                M::PigBaby,
                [
                    (
                        M::PigCold,
                        "minecraft:entity/pig/pig_cold",
                        "minecraft:entity/pig/pig_cold_baby",
                    ),
                    (
                        M::Pig,
                        "minecraft:entity/pig/pig_temperate",
                        "minecraft:entity/pig/pig_temperate_baby",
                    ),
                    (
                        M::Pig,
                        "minecraft:entity/pig/pig_warm",
                        "minecraft:entity/pig/pig_warm_baby",
                    ),
                ],
            ));
        }
        EntityKind::Horse => {
            let colour =
                usize::try_from(a.horse_variant & 0xFF).unwrap_or(0) % HORSE_TEXTURES.len();
            let markings = usize::try_from((a.horse_variant >> 8) & 0xFF).unwrap_or(0) % 5;
            out.push(plain(
                if baby { M::HorseBaby } else { M::Horse },
                HORSE_TEXTURES[colour][usize::from(baby)],
            ));
            if markings > 0 {
                out.push(
                    plain(
                        if baby { M::HorseBaby } else { M::Horse },
                        HORSE_MARKINGS[markings - 1][usize::from(baby)],
                    )
                    .translucent(),
                );
            }
        }
        EntityKind::Donkey => out.push(LayerSpec::aged(
            baby,
            (M::Donkey, "minecraft:entity/horse/donkey"),
            (M::DonkeyBaby, "minecraft:entity/horse/donkey_baby"),
        )),
        EntityKind::Mule => out.push(LayerSpec::aged(
            baby,
            (M::Donkey, "minecraft:entity/horse/mule"),
            (M::DonkeyBaby, "minecraft:entity/horse/mule_baby"),
        )),
        EntityKind::SkeletonHorse => out.push(LayerSpec::aged(
            baby,
            (M::Horse, "minecraft:entity/horse/horse_skeleton"),
            (M::HorseBaby, "minecraft:entity/horse/horse_skeleton_baby"),
        )),
        EntityKind::ZombieHorse => out.push(LayerSpec::aged(
            baby,
            (M::Horse, "minecraft:entity/horse/horse_zombie"),
            (M::HorseBaby, "minecraft:entity/horse/horse_zombie_baby"),
        )),
        EntityKind::Wolf => {
            let variant = usize::try_from(a.wolf_variant)
                .ok()
                .filter(|&v| v < WOLF_TEXTURES.len())
                .unwrap_or(3);
            let state = if a.tame() {
                1
            } else if a.angry {
                2
            } else {
                0
            };
            let tints = WOLF_TEXTURES[variant][usize::from(baby)][state];
            out.push(plain(if baby { M::WolfBaby } else { M::Wolf }, tints));
            if a.tame() {
                out.push(
                    LayerSpec::aged(
                        baby,
                        (M::Wolf, "minecraft:entity/wolf/wolf_collar"),
                        (M::WolfBaby, "minecraft:entity/wolf/wolf_collar_baby"),
                    )
                    .with_tint(dye_tint(a.collar_color())),
                );
            }
        }
        EntityKind::Cat => {
            let variant = usize::try_from(a.cat_variant)
                .ok()
                .filter(|&v| v < CAT_TEXTURES.len())
                .unwrap_or(9);
            out.push(plain(
                if baby { M::FelineBaby } else { M::Feline },
                CAT_TEXTURES[variant][usize::from(baby)],
            ));
            if a.tame() {
                out.push(
                    LayerSpec::aged(
                        baby,
                        (M::FelineCollar, "minecraft:entity/cat/cat_collar"),
                        (M::FelineBabyCollar, "minecraft:entity/cat/cat_collar_baby"),
                    )
                    .with_tint(dye_tint(a.collar_color())),
                );
            }
        }
        EntityKind::Ocelot => out.push(LayerSpec::aged(
            baby,
            (M::Feline, "minecraft:entity/cat/ocelot"),
            (M::FelineBaby, "minecraft:entity/cat/ocelot_baby"),
        )),
        EntityKind::Mooshroom => {
            let brown = a.brown_mushrooms;
            out.push(match (baby, brown) {
                (false, false) => plain(M::Cow, "minecraft:entity/cow/mooshroom_red"),
                (false, true) => plain(M::Cow, "minecraft:entity/cow/mooshroom_brown"),
                (true, false) => plain(M::CowBaby, "minecraft:entity/cow/mooshroom_red_baby"),
                (true, true) => plain(M::CowBaby, "minecraft:entity/cow/mooshroom_brown_baby"),
            });
        }
        EntityKind::Cow => {
            debug_assert_eq!(CowVariant::Cold as i32, 0);
            out.push(variant_body(
                baby,
                a.variant,
                M::CowBaby,
                [
                    (
                        M::CowCold,
                        "minecraft:entity/cow/cow_cold",
                        "minecraft:entity/cow/cow_cold_baby",
                    ),
                    (
                        M::Cow,
                        "minecraft:entity/cow/cow_temperate",
                        "minecraft:entity/cow/cow_temperate_baby",
                    ),
                    (
                        M::CowWarm,
                        "minecraft:entity/cow/cow_warm",
                        "minecraft:entity/cow/cow_warm_baby",
                    ),
                ],
            ));
        }
        EntityKind::Chicken => {
            debug_assert_eq!(ChickenVariant::Cold as i32, 0);
            out.push(variant_body(
                baby,
                a.variant,
                M::ChickenBaby,
                [
                    (
                        M::ChickenCold,
                        "minecraft:entity/chicken/chicken_cold",
                        "minecraft:entity/chicken/chicken_cold_baby",
                    ),
                    (
                        M::Chicken,
                        "minecraft:entity/chicken/chicken_temperate",
                        "minecraft:entity/chicken/chicken_temperate_baby",
                    ),
                    (
                        M::Chicken,
                        "minecraft:entity/chicken/chicken_warm",
                        "minecraft:entity/chicken/chicken_warm_baby",
                    ),
                ],
            ));
        }
        EntityKind::Sheep => {
            let jeb = a.magic == Magic::Jeb;
            let tint = if jeb {
                sheep::jeb_tint(age_in_ticks)
            } else {
                sheep::wool_tint(a.wool_color())
            };
            if baby {
                out.push(plain(M::SheepBaby, "minecraft:entity/sheep/sheep_baby"));
            } else {
                out.push(plain(M::Sheep, "minecraft:entity/sheep/sheep"));
                if jeb || a.wool_color() != pumpkin_data::dye_color::DyeColor::White {
                    out.push(
                        plain(M::Sheep, "minecraft:entity/sheep/sheep_wool_undercoat")
                            .with_tint(tint),
                    );
                }
            }
            if !a.sheared() {
                let (model, texture) = if baby {
                    (M::SheepBaby, "minecraft:entity/sheep/sheep_wool_baby")
                } else {
                    (M::SheepFur, "minecraft:entity/sheep/sheep_wool")
                };
                out.push(plain(model, texture).with_tint(tint));
            }
        }
        EntityKind::Rabbit => {
            let coat = if a.magic == Magic::Toast {
                rabbit::TOAST
            } else {
                a.rabbit_variant
            };
            out.push(LayerSpec::aged(
                baby,
                (M::Rabbit, rabbit::texture(coat, false)),
                (M::RabbitBaby, rabbit::texture(coat, true)),
            ));
        }
        EntityKind::Bat => out.push(plain(M::Bat, "minecraft:entity/bat/bat")),
        EntityKind::Creeper => out.push(plain(M::Creeper, "minecraft:entity/creeper/creeper")),
        EntityKind::Enderman => {
            out.push(plain(M::Enderman, "minecraft:entity/enderman/enderman"));
            out.push(plain(M::Enderman, "minecraft:entity/enderman/enderman_eyes").glowing());
        }
        EntityKind::Blaze => out.push(plain(M::Blaze, "minecraft:entity/blaze/blaze")),
        EntityKind::SnowGolem => out.push(plain(
            M::SnowGolem,
            "minecraft:entity/snow_golem/snow_golem",
        )),
        EntityKind::Ghast => out.push(plain(
            M::Ghast,
            if a.charging {
                "minecraft:entity/ghast/ghast_shooting"
            } else {
                "minecraft:entity/ghast/ghast"
            },
        )),
        EntityKind::Squid => out.push(LayerSpec::aged(
            baby,
            (M::Squid, "minecraft:entity/squid/squid"),
            (M::SquidBaby, "minecraft:entity/squid/squid_baby"),
        )),
        EntityKind::GlowSquid => out.push(LayerSpec::aged(
            baby,
            (M::Squid, "minecraft:entity/squid/glow_squid"),
            (M::SquidBaby, "minecraft:entity/squid/glow_squid_baby"),
        )),
        EntityKind::Cod => out.push(plain(M::Cod, "minecraft:entity/fish/cod")),
        EntityKind::Salmon => {
            let model = match a.salmon_variant {
                0 => M::SalmonSmall,
                2 => M::SalmonLarge,
                _ => M::Salmon,
            };
            out.push(plain(model, "minecraft:entity/fish/salmon"));
        }
        EntityKind::Pufferfish => {
            let model = match a.puff_state {
                0 => M::PufferSmall,
                1 => M::PufferMid,
                _ => M::PufferBig,
            };
            out.push(plain(model, "minecraft:entity/fish/pufferfish"));
        }
        EntityKind::TropicalFish => {
            let (large, number) = fish::tropical_pattern(a.tropical_variant);
            let dye = |shift: u32| {
                dye_tint(pumpkin_data::dye_color::DyeColor::by_id_or_white(
                    (a.tropical_variant >> shift) & 0xFF,
                ))
            };
            let (base, pattern, texture) = if large {
                (
                    M::TropicalLarge,
                    M::TropicalLargePattern,
                    "minecraft:entity/fish/tropical_b",
                )
            } else {
                (
                    M::TropicalSmall,
                    M::TropicalSmallPattern,
                    "minecraft:entity/fish/tropical_a",
                )
            };
            out.push(plain(base, texture).with_tint(dye(16)));
            out.push(plain(pattern, fish::pattern_texture(large, number)).with_tint(dye(24)));
        }
        EntityKind::Turtle => out.push(LayerSpec::aged(
            baby,
            (M::Turtle, "minecraft:entity/turtle/turtle"),
            (M::TurtleBaby, "minecraft:entity/turtle/turtle_baby"),
        )),
        EntityKind::Guardian => out.push(plain(M::Guardian, "minecraft:entity/guardian/guardian")),
        EntityKind::ElderGuardian => out.push(plain(
            M::ElderGuardian,
            "minecraft:entity/guardian/guardian_elder",
        )),
        EntityKind::Tadpole => out.push(plain(M::Tadpole, "minecraft:entity/tadpole/tadpole")),
        EntityKind::Nautilus => out.push(LayerSpec::aged(
            baby,
            (M::Nautilus, "minecraft:entity/nautilus/nautilus"),
            (M::NautilusBaby, "minecraft:entity/nautilus/nautilus_baby"),
        )),
        EntityKind::Axolotl => {
            let texture = axolotl::texture(a.axolotl_variant, baby);
            out.push(plain(
                if baby { M::AxolotlBaby } else { M::Axolotl },
                texture,
            ));
        }
        EntityKind::Fox => out.push(plain(
            if baby { M::FoxBaby } else { M::Fox },
            fox::texture(
                a.fox_variant != 0,
                a.fox_flags & fox::FLAG_SLEEPING != 0,
                baby,
            ),
        )),
        EntityKind::Frog => out.push(plain(M::Frog, frog::texture(a.variant))),
        EntityKind::Camel => out.push(LayerSpec::aged(
            baby,
            (M::Camel, "minecraft:entity/camel/camel"),
            (M::CamelBaby, "minecraft:entity/camel/camel_baby"),
        )),
        EntityKind::Armadillo => out.push(LayerSpec::aged(
            baby,
            (M::Armadillo, "minecraft:entity/armadillo/armadillo"),
            (
                M::ArmadilloBaby,
                "minecraft:entity/armadillo/armadillo_baby",
            ),
        )),
        EntityKind::PolarBear => out.push(LayerSpec::aged(
            baby,
            (M::PolarBear, "minecraft:entity/bear/polarbear"),
            (M::PolarBearBaby, "minecraft:entity/bear/polarbear_baby"),
        )),
        EntityKind::Bogged => {
            out.push(plain(M::Bogged, "minecraft:entity/skeleton/bogged"));
            out.push(plain(
                M::BoggedClothes,
                "minecraft:entity/skeleton/bogged_overlay",
            ));
        }
        EntityKind::Illusioner => {
            out.push(plain(M::Illager, "minecraft:entity/illager/illusioner"));
        }
        EntityKind::Bee => out.push(plain(
            if baby { M::BeeBaby } else { M::Bee },
            bee::texture(
                bee::BeePose {
                    roll_amount: if a.bee_flags & bee::FLAG_ROLL != 0 {
                        1.0
                    } else {
                        0.0
                    },
                    has_stinger: a.bee_flags & bee::FLAG_HAS_STUNG == 0,
                    on_ground: entity.on_ground,
                    angry: a.angry,
                    nectar: a.bee_flags & bee::FLAG_HAS_NECTAR != 0,
                },
                baby,
            ),
        )),
        EntityKind::Goat => out.push(LayerSpec::aged(
            baby,
            (M::Goat, goat::texture(false)),
            (M::GoatBaby, goat::texture(true)),
        )),
        EntityKind::Llama | EntityKind::TraderLlama => {
            out.push(plain(
                if baby { M::LlamaBaby } else { M::Llama },
                llama::texture(a.llama_variant, baby),
            ));
            if entity.kind == EntityKind::TraderLlama {
                out.push(plain(
                    if baby {
                        M::LlamaBabyDecor
                    } else {
                        M::LlamaDecor
                    },
                    llama::trader_texture(baby),
                ));
            }
        }
        EntityKind::Panda => out.push(plain(
            if baby { M::PandaBaby } else { M::Panda },
            panda::texture(i32::from(a.panda_gene), baby),
        )),
        EntityKind::Parrot => out.push(plain(M::Parrot, parrot::texture(a.parrot_variant))),
        EntityKind::Hoglin => out.push(LayerSpec::aged(
            baby,
            (M::Hoglin, hoglin::texture(false, false)),
            (M::HoglinBaby, hoglin::texture(false, true)),
        )),
        EntityKind::Zoglin => out.push(LayerSpec::aged(
            baby,
            (M::Hoglin, hoglin::texture(true, false)),
            (M::HoglinBaby, hoglin::texture(true, true)),
        )),
        EntityKind::Phantom => {
            out.push(plain(M::Phantom, phantom::TEXTURE));
            out.push(plain(M::Phantom, phantom::EYES).glowing());
        }
        EntityKind::Vex => out.push(plain(M::Vex, vex::texture(a.charging))),
        EntityKind::Allay => out.push(plain(M::Allay, allay::TEXTURE)),
        EntityKind::Strider => out.push(LayerSpec::aged(
            baby,
            (M::Strider, strider::texture(a.strider_cold, false)),
            (M::StriderBaby, strider::texture(a.strider_cold, true)),
        )),
        EntityKind::Ravager => out.push(plain(M::Ravager, ravager::TEXTURE)),
        EntityKind::Wither => {
            out.push(plain(M::Wither, wither::texture(a.wither_invul as f32)));
            if a.wither_invul > 0 {
                out.push(plain(M::WitherArmor, wither::armor_texture()));
            }
        }
        EntityKind::EnderDragon => {
            out.push(plain(M::Dragon, dragon::TEXTURE));
            out.push(plain(M::Dragon, dragon::EYES_TEXTURE).glowing());
        }
        EntityKind::Shulker => out.push(plain(
            M::Shulker,
            shulker::texture(i32::from(a.shulker_color)),
        )),
        EntityKind::Sniffer => out.push(LayerSpec::aged(
            baby,
            (M::Sniffer, sniffer::texture(false)),
            (M::SnifferBaby, sniffer::texture(true)),
        )),
        EntityKind::Warden => {
            out.push(plain(M::Warden, warden::texture()));
            out.push(plain(M::Warden, warden::bioluminescent_texture()).glowing());
            out.push(plain(M::Warden, warden::heart_texture()).glowing());
            out.push(plain(M::Warden, warden::pulsating_spots_1_texture()).glowing());
            out.push(plain(M::Warden, warden::pulsating_spots_2_texture()).glowing());
        }
        EntityKind::Breeze => {
            out.push(plain(M::Breeze, breeze::texture()));
            out.push(plain(M::BreezeWind, breeze::wind_texture()));
            out.push(plain(M::Breeze, breeze::eyes_texture()).glowing());
        }
        EntityKind::Creaking => {
            out.push(plain(M::Creaking, creaking::texture()));
            if creaking::eyes_glowing(a.creaking_active, false, false) {
                out.push(plain(M::Creaking, creaking::eyes_texture()).glowing());
            }
        }
        EntityKind::CopperGolem => {
            out.push(plain(M::CopperGolem, copper_golem::texture(0)));
            out.push(plain(M::CopperGolem, copper_golem::eyes_texture(0)).glowing());
        }
        EntityKind::CamelHusk => out.push(LayerSpec::aged(
            baby,
            (M::Camel, "minecraft:entity/camel/camel_husk"),
            (M::CamelBaby, "minecraft:entity/camel/camel_husk_baby"),
        )),
        EntityKind::Dolphin => out.push(LayerSpec::aged(
            baby,
            (M::Dolphin, "minecraft:entity/dolphin/dolphin"),
            (M::DolphinBaby, "minecraft:entity/dolphin/dolphin_baby"),
        )),
        EntityKind::HappyGhast => out.push(LayerSpec::aged(
            baby,
            (M::HappyGhast, "minecraft:entity/ghast/happy_ghast"),
            (M::HappyGhastBaby, "minecraft:entity/ghast/happy_ghast_baby"),
        )),
        EntityKind::Endermite => {
            out.push(plain(M::Endermite, "minecraft:entity/endermite/endermite"))
        }
        EntityKind::Silverfish => out.push(plain(
            M::Silverfish,
            "minecraft:entity/silverfish/silverfish",
        )),
        EntityKind::IronGolem => {
            out.push(plain(
                M::IronGolem,
                "minecraft:entity/iron_golem/iron_golem",
            ));
            if let Some(cracks) = golem::crackiness(a.health) {
                out.push(plain(M::IronGolem, cracks));
            }
        }
        EntityKind::Zombie => out.push(if baby {
            plain(M::ZombieBaby, "minecraft:entity/zombie/zombie_baby")
        } else {
            plain(M::Zombie, "minecraft:entity/zombie/zombie")
        }),
        EntityKind::Drowned => {
            if baby {
                out.push(plain(M::ZombieBaby, "minecraft:entity/zombie/drowned_baby"));
                out.push(plain(
                    M::DrownedBabyOuter,
                    "minecraft:entity/zombie/drowned_outer_layer_baby",
                ));
            } else {
                out.push(plain(M::Drowned, "minecraft:entity/zombie/drowned"));
                out.push(plain(
                    M::DrownedOuter,
                    "minecraft:entity/zombie/drowned_outer_layer",
                ));
            }
        }
        EntityKind::Piglin => out.push(if baby {
            plain(M::PiglinBaby, "minecraft:entity/piglin/piglin_baby")
        } else {
            plain(M::Piglin, "minecraft:entity/piglin/piglin")
        }),
        EntityKind::PiglinBrute => {
            out.push(plain(M::Piglin, "minecraft:entity/piglin/piglin_brute"))
        }
        EntityKind::ZombifiedPiglin => out.push(if baby {
            plain(
                M::PiglinBaby,
                "minecraft:entity/piglin/zombified_piglin_baby",
            )
        } else {
            plain(M::Piglin, "minecraft:entity/piglin/zombified_piglin")
        }),
        EntityKind::Villager | EntityKind::ZombieVillager => villager_layers(entity, hat, out),
        EntityKind::WanderingTrader => out.push(plain(
            M::Villager,
            "minecraft:entity/wandering_trader/wandering_trader",
        )),
        EntityKind::Witch => out.push(plain(M::Witch, "minecraft:entity/witch/witch")),
        EntityKind::Pillager => out.push(plain(M::Illager, "minecraft:entity/illager/pillager")),
        EntityKind::Vindicator => {
            out.push(plain(M::Illager, "minecraft:entity/illager/vindicator"));
        }
        EntityKind::Evoker => out.push(plain(M::Illager, "minecraft:entity/illager/evoker")),
        EntityKind::Husk => out.push(if baby {
            plain(M::ZombieBaby, "minecraft:entity/zombie/husk_baby")
        } else {
            plain(M::Zombie, "minecraft:entity/zombie/husk")
        }),
        EntityKind::Skeleton => out.push(plain(M::Skeleton, "minecraft:entity/skeleton/skeleton")),
        EntityKind::WitherSkeleton => out.push(plain(
            M::Skeleton,
            "minecraft:entity/skeleton/wither_skeleton",
        )),
        EntityKind::Stray => {
            out.push(plain(M::Skeleton, "minecraft:entity/skeleton/stray"));
            out.push(plain(
                M::StrayClothes,
                "minecraft:entity/skeleton/stray_overlay",
            ));
        }
        EntityKind::Spider | EntityKind::CaveSpider => {
            out.push(plain(
                M::Spider,
                if entity.kind == EntityKind::Spider {
                    "minecraft:entity/spider/spider"
                } else {
                    "minecraft:entity/spider/cave_spider"
                },
            ));
            out.push(plain(M::Spider, "minecraft:entity/spider/spider_eyes").glowing());
        }
        EntityKind::Slime => {
            out.push(plain(M::SlimeInner, "minecraft:entity/slime/slime"));
            out.push(plain(M::SlimeOuter, "minecraft:entity/slime/slime").translucent());
        }
        EntityKind::MagmaCube => out.push(plain(M::MagmaCube, "minecraft:entity/slime/magmacube")),
        EntityKind::LeashKnot => out.push(plain(M::LeashKnot, leash_knot::TEXTURE)),
        EntityKind::ArmorStand => out.push(plain(
            if a.stand_small() {
                M::ArmorStandSmall
            } else {
                M::ArmorStand
            },
            armor_stand::TEXTURE,
        )),
        EntityKind::Boat => {
            if let Some((texture, raft, chest)) = vehicle::boat_look(entity.ty.resource_name) {
                let model = match (raft, chest) {
                    (false, false) => M::Boat,
                    (false, true) => M::ChestBoat,
                    (true, false) => M::Raft,
                    (true, true) => M::ChestRaft,
                };
                out.push(plain(model, texture));
                if !entity.in_water {
                    out.push(plain(M::BoatWaterPatch, texture).water_mask());
                }
            }
        }
        EntityKind::Minecart => out.push(plain(M::Minecart, vehicle::MINECART_TEXTURE)),
        EntityKind::SulfurCube => {
            let fuse = sulfur_cube::fuse_remaining(entity.fuse_ticks, 0.0);
            let holds_block = sulfur_cube::swallowed_block(entity).is_some();
            if !holds_block && !a.invisible() {
                let inner = if baby {
                    plain(M::SulfurCubeSmallInner, sulfur_cube::INNER_SMALL)
                } else {
                    plain(M::SulfurCubeInner, sulfur_cube::INNER)
                }
                .translucent();
                out.push(if sulfur_cube::core_flashes(fuse) {
                    inner.with_overlay(-1.0)
                } else {
                    inner
                });
            }
            out.push(
                if baby {
                    plain(M::SulfurCubeSmallOuter, sulfur_cube::OUTER_SMALL)
                } else {
                    plain(M::SulfurCubeOuter, sulfur_cube::OUTER)
                }
                .translucent(),
            );
        }
        EntityKind::Parched => out.push(plain(M::Skeleton, "minecraft:entity/skeleton/parched")),
        EntityKind::Giant => out.push(plain(M::Zombie, "minecraft:entity/zombie/zombie")),
        EntityKind::ZombieNautilus => {
            use pumpkin_data::zombie_nautilus_variant::ZombieNautilusVariant;
            let variant = u8::try_from(a.thing.variant)
                .ok()
                .and_then(ZombieNautilusVariant::from_id)
                .unwrap_or_default();
            // `ZombieNautilusCoralModel`: the corals hide under armor.
            let armored = !entity.equipment.get(EquipmentSlot::Body).is_empty();
            let model = if variant.model() == Some("warm") && !armored {
                M::ZombieNautilusCoral
            } else {
                M::Nautilus
            };
            out.push(plain(model, variant.asset_id()));
        }
        EntityKind::Cushion => out.push(plain(
            M::Cushion,
            projectile::cushion_texture(a.thing.color),
        )),
        EntityKind::WitherSkull => out.push(plain(
            M::WitherSkull,
            if a.thing.dangerous {
                projectile::WITHER_INVULNERABLE_TEXTURE
            } else {
                projectile::WITHER_TEXTURE
            },
        )),
        EntityKind::ShulkerBullet => {
            out.push(plain(M::ShulkerBullet, projectile::SHULKER_BULLET_TEXTURE));
            out.push(
                plain(M::ShulkerBulletGlow, projectile::SHULKER_BULLET_TEXTURE)
                    .faded(projectile::SHULKER_GLOW_ALPHA),
            );
        }
        EntityKind::LlamaSpit => out.push(plain(M::LlamaSpit, projectile::LLAMA_SPIT_TEXTURE)),
        EntityKind::WindCharge => {
            out.push(plain(M::WindCharge, projectile::WIND_CHARGE_TEXTURE).translucent())
        }
        EntityKind::EvokerFangs => {
            // `EvokerFangsRenderer`: nothing until the bite starts.
            if entity.fangs_life.is_some() {
                out.push(plain(M::EvokerFangs, projectile::EVOKER_FANGS_TEXTURE));
            }
        }
        EntityKind::EndCrystal => out.push(plain(M::EndCrystal, projectile::END_CRYSTAL_TEXTURE)),
        EntityKind::Trident => {
            out.push(plain(M::Trident, projectile::TRIDENT_TEXTURE).with_foil(a.thing.foil))
        }
        EntityKind::Arrow => out.push(plain(
            M::Arrow,
            if a.thing.arrow_color > 0 {
                projectile::TIPPED_ARROW_TEXTURE
            } else {
                projectile::ARROW_TEXTURE
            },
        )),
        EntityKind::SpectralArrow => out.push(plain(M::Arrow, projectile::SPECTRAL_ARROW_TEXTURE)),
        EntityKind::DragonFireball
        | EntityKind::ExperienceOrb
        | EntityKind::ThrownItem
        | EntityKind::FireworkRocket
        | EntityKind::OminousItemSpawner
        | EntityKind::Painting
        | EntityKind::LightningBolt => {}
        EntityKind::Player
        | EntityKind::Mannequin
        | EntityKind::FallingBlock
        | EntityKind::PrimedTnt
        | EntityKind::Item
        | EntityKind::ItemDisplay
        | EntityKind::BlockDisplay
        | EntityKind::TextDisplay
        | EntityKind::ItemFrame
        | EntityKind::GlowItemFrame
        | EntityKind::Interaction
        | EntityKind::FishingBobber
        | EntityKind::Other => {}
    }
    equipment_layers(entity, out);
}

macro_rules! textures {
    ($dir:literal, $kind:literal, [$($name:literal),*]) => {
        [$(concat!("minecraft:entity/", $dir, "/", $kind, "/", $name)),*]
    };
}

/// `VillagerType`'s and `VillagerProfession`'s names by registry id, and
macro_rules! clothes {
    ($dir:literal) => {
        Clothes {
            base: concat!("minecraft:entity/", $dir, "/", $dir),
            baby_base: concat!("minecraft:entity/", $dir, "/", $dir, "_baby"),
            types: textures!(
                $dir,
                "type",
                [
                    "desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"
                ]
            ),
            baby_types: textures!(
                $dir,
                "baby",
                [
                    "desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"
                ]
            ),
            professions: textures!(
                $dir,
                "profession",
                [
                    "none",
                    "armorer",
                    "butcher",
                    "cartographer",
                    "cleric",
                    "farmer",
                    "fisherman",
                    "fletcher",
                    "leatherworker",
                    "librarian",
                    "mason",
                    "nitwit",
                    "shepherd",
                    "toolsmith",
                    "weaponsmith"
                ]
            ),
            levels: textures!(
                $dir,
                "profession_level",
                ["stone", "iron", "gold", "emerald", "diamond"]
            ),
        }
    };
}

pub(super) struct Clothes {
    base: &'static str,
    baby_base: &'static str,
    types: [&'static str; 7],
    baby_types: [&'static str; 7],
    professions: [&'static str; 15],
    levels: [&'static str; 5],
}

pub(super) const VILLAGER_CLOTHES: Clothes = clothes!("villager");
pub(super) const ZOMBIE_VILLAGER_CLOTHES: Clothes = clothes!("zombie_villager");

/// `VillagerProfession`'s `none` and `nitwit` ids.
pub(super) const PROFESSION_NONE: usize = 0;
pub(super) const PROFESSION_NITWIT: usize = 11;

/// A villager's body and `VillagerProfessionLayer`: the biome's clothes,
pub(super) fn villager_layers(
    entity: &ClientEntity,
    hat: &dyn Fn(&str) -> villager::Hat,
    out: &mut Vec<LayerSpec>,
) {
    use ModelId as M;
    let zombie = entity.kind == EntityKind::ZombieVillager;
    let (clothes, full, no_hat, baby_full, baby_no_hat) = if zombie {
        (
            &ZOMBIE_VILLAGER_CLOTHES,
            M::ZombieVillager,
            M::ZombieVillagerNoHat,
            M::ZombieVillagerBaby,
            M::ZombieVillagerBabyNoHat,
        )
    } else {
        (
            &VILLAGER_CLOTHES,
            M::Villager,
            M::VillagerNoHat,
            M::VillagerBaby,
            M::VillagerBabyNoHat,
        )
    };
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    let (kind, job, level) = a.villager;
    let kind = usize::try_from(kind).ok().filter(|&k| k < 7).unwrap_or(2);
    let job = usize::try_from(job).ok().filter(|&j| j < 15).unwrap_or(0);
    out.push(if baby {
        plain(baby_full, clothes.baby_base)
    } else {
        plain(full, clothes.base)
    });
    let hat_visible = hat(clothes.professions[job]).type_hat_visible(hat(clothes.types[kind]));
    out.push(match (baby, hat_visible) {
        (true, true) => plain(baby_full, clothes.baby_types[kind]),
        (true, false) => plain(baby_no_hat, clothes.baby_types[kind]),
        (false, true) => plain(full, clothes.types[kind]),
        (false, false) => plain(no_hat, clothes.types[kind]),
    });
    if job != PROFESSION_NONE && !baby {
        out.push(plain(full, clothes.professions[job]));
        if job != PROFESSION_NITWIT {
            let badge = usize::try_from(level.clamp(1, 5) - 1).unwrap_or(0);
            out.push(plain(full, clothes.levels[badge]));
        }
    }
}

/// `MushroomCow.Variant`'s block model, drawn on the back and head of an
pub fn mushroom_model(entity: &ClientEntity) -> Option<&'static str> {
    if entity.kind != EntityKind::Mooshroom || entity.appearance.baby {
        return None;
    }
    Some(if entity.appearance.brown_mushrooms {
        "minecraft:block/brown_mushroom"
    } else {
        "minecraft:block/red_mushroom"
    })
}

/// The harness's texture by its dye (`EquippableImpl`'s asset id).
pub(super) fn harness_texture(stack: &ItemStack) -> Option<&'static str> {
    macro_rules! harnesses {
        ($($name:literal),*) => {
            [$(($name, concat!("minecraft:entity/equipment/happy_ghast_body/", $name, "_harness"))),*]
        };
    }
    const HARNESSES: [(&str, &str); 16] = harnesses!(
        "white",
        "orange",
        "magenta",
        "light_blue",
        "yellow",
        "lime",
        "pink",
        "gray",
        "light_gray",
        "cyan",
        "purple",
        "blue",
        "brown",
        "green",
        "red",
        "black"
    );
    let asset = stack
        .get_data_component::<EquippableImpl>()?
        .asset_id
        .clone()?;
    let name = asset.strip_prefix("minecraft:").unwrap_or(&asset);
    let dye = name.strip_suffix("_harness")?;
    HARNESSES
        .iter()
        .find_map(|(color, texture)| (*color == dye).then_some(*texture))
}

/// `SnowGolemHeadLayer`: the block on the head, if it has its pumpkin.
pub const SNOW_GOLEM_HEAD: &str = "minecraft:block/carved_pumpkin";
