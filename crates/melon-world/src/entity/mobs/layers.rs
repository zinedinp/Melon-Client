//! Which layers a mob shows; each family's `push_layers` sits beside its model.

use super::*;

/// `DyeColor.getTextureDiffuseColor` as factors (collars).
pub(super) fn dye_tint(color: pumpkin_data::dye_color::DyeColor) -> [f32; 3] {
    let rgb = color.texture_diffuse_color();
    [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0)
}

/// A layer's texture id: a literal, or a path built from an item and
/// pooled ([`melon_core::util::intern`]), so a frame's layers allocate nothing.
#[derive(Clone, Debug)]
pub enum TexturePath {
    Static(&'static str),
    Pooled(std::sync::Arc<str>),
}

impl std::ops::Deref for TexturePath {
    type Target = str;

    fn deref(&self) -> &str {
        match self {
            Self::Static(path) => path,
            Self::Pooled(path) => path,
        }
    }
}

impl AsRef<str> for TexturePath {
    fn as_ref(&self) -> &str {
        self
    }
}

impl PartialEq for TexturePath {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl PartialEq<&str> for TexturePath {
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}

/// One model drawn for an entity, with its texture and how it is blended.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSpec {
    pub model: ModelId,
    pub texture: TexturePath,
    pub tint: [f32; 3],
    pub full_bright: bool,
    /// Blended over what is behind it (a slime's jelly).
    pub translucent: bool,
    /// `RenderTypes.waterMask`: depth only.
    pub water_mask: bool,
    /// Humanoid armor: posed by copying the wearer, not `setupAnim`.
    pub copy_humanoid: bool,
    /// `ItemStack.hasFoil`: the armor or equipment glint overlay.
    pub foil: bool,
    /// Its own overlay instead of the body's (a sulfur cube's lit core).
    pub overlay: Option<f32>,
    /// The colour's alpha it is drawn with (a shulker bullet's glow).
    pub alpha: f32,
    /// `OffsetTextureTransform`: the texture slid this far.
    pub scroll: Option<[f32; 2]>,
    /// Added onto what is behind it (`RenderTypes.energySwirl`).
    pub additive: bool,
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
    pub(super) fn pooled(model: ModelId, texture: std::sync::Arc<str>, tint: [f32; 3]) -> Self {
        Self {
            model,
            texture: TexturePath::Pooled(texture),
            tint,
            full_bright: false,
            translucent: false,
            water_mask: false,
            copy_humanoid: false,
            foil: false,
            overlay: None,
            alpha: 1.0,
            scroll: None,
            additive: false,
        }
    }

    pub(super) fn humanoid_armor(
        model: ModelId,
        texture: std::sync::Arc<str>,
        tint: [f32; 3],
    ) -> Self {
        Self {
            copy_humanoid: true,
            ..Self::pooled(model, texture, tint)
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
            texture: TexturePath::Static(texture),
            tint: [1.0; 3],
            full_bright: false,
            translucent: false,
            water_mask: false,
            copy_humanoid: false,
            foil: false,
            overlay: None,
            alpha: 1.0,
            scroll: None,
            additive: false,
        }
    }

    /// Drawn with `overlay` rather than the body's.
    pub(super) fn with_overlay(self, overlay: f32) -> Self {
        Self {
            overlay: Some(overlay),
            ..self
        }
    }

    /// `RenderTypes.energySwirl`, coloured `0xFF808080`. Keeps the faces' baked
    /// shade, which vanilla's `NO_CARDINAL_LIGHTING` leaves off.
    pub(super) fn energy_swirl(self, u: f32, v: f32) -> Self {
        Self {
            scroll: Some([u % 1.0, v % 1.0]),
            additive: true,
            full_bright: true,
            tint: [128.0 / 255.0; 3],
            overlay: Some(0.0),
            ..self
        }
    }

    /// `RenderTypes.breezeWind`; the faces' shade stays, as for [`Self::energy_swirl`].
    pub(super) fn scrolled(self, u: f32, v: f32) -> Self {
        Self {
            scroll: Some([u % 1.0, v % 1.0]),
            translucent: true,
            overlay: Some(0.0),
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

/// The `variant`'s entry of a cold, temperate, warm `table`.
pub(super) fn by_variant<T: Copy>(variant: i32, table: [T; 3]) -> T {
    table[usize::try_from(variant)
        .ok()
        .filter(|&i| i < 3)
        .unwrap_or(1)]
}

/// A variant mob's body, or the shared `young` model for a baby.
pub(super) fn variant_body(
    baby: bool,
    variant: i32,
    young: ModelId,
    table: [(ModelId, &'static str, &'static str); 3],
) -> LayerSpec {
    let (model, adult, baby_texture) = by_variant(variant, table);
    LayerSpec::aged(baby, (model, adult), (young, baby_texture))
}

/// The layers of `entity`, the main body first, then what it wears.
pub fn layers(
    entity: &ClientEntity,
    partial: f32,
    hat: &dyn Fn(&str) -> villager::Hat,
    out: &mut Vec<LayerSpec>,
) {
    use EntityKind as K;
    let age_in_ticks = entity.age_in_ticks(partial);
    match entity.kind {
        K::Pig => pig::push_layers(entity, out),
        K::Horse | K::Donkey | K::Mule | K::SkeletonHorse | K::ZombieHorse => {
            horse::push_layers(entity, out)
        }
        K::Wolf => wolf::push_layers(entity, out),
        K::Cat | K::Ocelot => feline::push_layers(entity, out),
        K::Mooshroom | K::Cow => cow::push_layers(entity, out),
        K::Chicken => chicken::push_layers(entity, out),
        K::Sheep => sheep::push_layers(entity, age_in_ticks, out),
        K::Rabbit => rabbit::push_layers(entity, out),
        K::Bat => bat::push_layers(out),
        K::Creeper => creeper::push_layers(entity, age_in_ticks, out),
        K::Enderman => enderman::push_layers(out),
        K::Blaze => blaze::push_layers(out),
        K::SnowGolem => snow_golem::push_layers(out),
        K::Ghast | K::HappyGhast => ghast::push_layers(entity, out),
        K::Squid | K::GlowSquid => squid::push_layers(entity, out),
        K::Cod | K::Salmon | K::Pufferfish | K::TropicalFish | K::Tadpole => {
            fish::push_layers(entity, out)
        }
        K::Turtle => turtle::push_layers(entity, out),
        K::Guardian | K::ElderGuardian => guardian::push_layers(entity, out),
        K::Nautilus | K::ZombieNautilus => nautilus::push_layers(entity, out),
        K::Axolotl => axolotl::push_layers(entity, out),
        K::Fox => fox::push_layers(entity, out),
        K::Frog => frog::push_layers(entity, out),
        K::Camel | K::CamelHusk => camel::push_layers(entity, out),
        K::Armadillo => armadillo::push_layers(entity, out),
        K::PolarBear => polar_bear::push_layers(entity, out),
        K::Bogged | K::Skeleton | K::WitherSkeleton | K::Stray | K::Parched => {
            skeleton::push_layers(entity, out)
        }
        K::Illusioner | K::Pillager | K::Vindicator | K::Evoker => {
            illager::push_layers(entity, out)
        }
        K::Bee => bee::push_layers(entity, out),
        K::Goat => goat::push_layers(entity, out),
        K::Llama | K::TraderLlama => llama::push_layers(entity, out),
        K::Panda => panda::push_layers(entity, out),
        K::Parrot => parrot::push_layers(entity, out),
        K::Hoglin | K::Zoglin => hoglin::push_layers(entity, out),
        K::Phantom => phantom::push_layers(out),
        K::Vex => vex::push_layers(entity, out),
        K::Allay => allay::push_layers(out),
        K::Strider => strider::push_layers(entity, out),
        K::Ravager => ravager::push_layers(out),
        K::Wither => wither::push_layers(entity, age_in_ticks, out),
        K::EnderDragon => dragon::push_layers(out),
        K::Shulker => shulker::push_layers(entity, out),
        K::Sniffer => sniffer::push_layers(entity, out),
        K::Warden => warden::push_layers(entity, partial, out),
        K::Breeze => breeze::push_layers(age_in_ticks, out),
        K::Creaking => creaking::push_layers(entity, out),
        K::CopperGolem => copper_golem::push_layers(entity, out),
        K::Dolphin => dolphin::push_layers(entity, out),
        K::Endermite | K::Silverfish => segmented::push_layers(entity, out),
        K::IronGolem => golem::push_layers(entity, out),
        K::Zombie | K::Drowned | K::Husk | K::Giant => zombie::push_layers(entity, out),
        K::Piglin | K::PiglinBrute | K::ZombifiedPiglin => piglin::push_layers(entity, out),
        K::Villager | K::ZombieVillager | K::WanderingTrader | K::Witch => {
            villager::push_layers(entity, hat, out)
        }
        K::Spider | K::CaveSpider => spider::push_layers(entity, out),
        K::Slime | K::MagmaCube => slime::push_layers(entity, out),
        K::LeashKnot => leash_knot::push_layers(out),
        K::ArmorStand => armor_stand::push_layers(entity, out),
        K::Boat | K::Minecart => vehicle::push_layers(entity, out),
        K::SulfurCube => sulfur_cube::push_layers(entity, out),
        K::Cushion
        | K::WitherSkull
        | K::ShulkerBullet
        | K::LlamaSpit
        | K::WindCharge
        | K::EvokerFangs
        | K::EndCrystal
        | K::Trident
        | K::Arrow
        | K::SpectralArrow => projectile::push_layers(entity, out),
        K::DragonFireball
        | K::ExperienceOrb
        | K::ThrownItem
        | K::FireworkRocket
        | K::OminousItemSpawner
        | K::Painting
        | K::LightningBolt => {}
        K::Player
        | K::Mannequin
        | K::FallingBlock
        | K::PrimedTnt
        | K::Item
        | K::ItemDisplay
        | K::BlockDisplay
        | K::TextDisplay
        | K::ItemFrame
        | K::GlowItemFrame
        | K::Interaction
        | K::FishingBobber
        | K::Other => {}
    }
    equipment_layers(entity, out);
}
