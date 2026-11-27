//! What a mob wears and holds.

use super::*;
use crate::entity::equipment_asset;

/// Whether the item in hand is drawn.
pub fn shows_held_item(entity: &ClientEntity) -> bool {
    let a = &entity.appearance;
    match entity.kind {
        EntityKind::Vindicator => a.aggressive(),
        EntityKind::Evoker => a.casting_spell,
        EntityKind::Illusioner => a.aggressive() || a.casting_spell,
        _ => true,
    }
}

/// The armor slots an entity shows (`HumanoidArmorLayer`): humanoids only.
pub fn wears_armor(kind: EntityKind) -> bool {
    kind.is_avatar()
        || kind == EntityKind::ArmorStand
        || kind.is_zombie()
        || kind.is_skeleton()
        || matches!(kind, EntityKind::Piglin | EntityKind::PiglinBrute)
}

/// Humanoid armor pieces as extra layers, posed like the wearer.
pub fn push_humanoid_armor(
    kind: EntityKind,
    baby: bool,
    armor: [&ItemStack; 4],
    out: &mut Vec<LayerSpec>,
) {
    if !wears_armor(kind) {
        return;
    }
    for (slot, stack) in ArmorSlot::ALL.into_iter().zip(armor) {
        if stack.is_empty() {
            continue;
        }
        // `EquipmentLayerRenderer.renderLayers`: the glint goes with the
        // first layer (the leather overlay above it shows none); with a
        // trim, vanilla glints the whole model after it, here the first
        // layer and the trim each where their texels are.
        let foil = crate::glint::has_foil(stack);
        let model = armor_model(kind, slot, baby);
        let mut first = true;
        let folder = armor_folder(slot, baby && kind != EntityKind::ArmorStand);
        equipment_asset::each_texture(stack, folder, |texture, tint| {
            out.push(LayerSpec::humanoid_armor(model, texture, tint).with_foil(foil && first));
            first = false;
        });
        if let Some(trim) = trim_overlay(stack, slot, baby) {
            out.push(
                LayerSpec::humanoid_armor(armor_model(kind, slot, baby), trim, [1.0; 3])
                    .with_foil(foil),
            );
        }
    }
}

/// Saddle, body gear, harness and humanoid armor, in draw order.
pub fn equipment_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let baby = if entity.kind == EntityKind::ArmorStand {
        entity.appearance.stand_small()
    } else {
        entity.appearance.baby
    };
    push_equipment(entity.kind, baby, &entity.equipment, out);
}

/// The worn extra layers of `kind`, from what it carries.
pub fn push_equipment(
    kind: EntityKind,
    baby: bool,
    equipment: &Equipment,
    out: &mut Vec<LayerSpec>,
) {
    push_humanoid_armor(kind, baby, equipment.armor(), out);
    let body = equipment.get(EquipmentSlot::Body);
    if !body.is_empty() {
        match kind {
            EntityKind::Wolf if !baby => {
                push_equipment_textures(ModelId::WolfArmor, body, "wolf_body", out);
            }
            EntityKind::Horse | EntityKind::ZombieHorse | EntityKind::SkeletonHorse if !baby => {
                push_equipment_textures(ModelId::HorseArmor, body, "horse_body", out);
            }
            EntityKind::HappyGhast => {
                let model = if baby {
                    ModelId::HappyGhastBabyHarness
                } else {
                    ModelId::HappyGhastHarness
                };
                push_equipment_textures(model, body, "happy_ghast_body", out);
            }
            EntityKind::Nautilus | EntityKind::ZombieNautilus if !baby => {
                push_equipment_textures(ModelId::NautilusArmor, body, "nautilus_body", out);
            }
            _ => {}
        }
    }
    let saddle = equipment.get(EquipmentSlot::Saddle);
    if !saddle.is_empty()
        && let Some((model, folder)) = saddle_look(kind, baby)
    {
        push_equipment_textures(model, saddle, folder, out);
    }
}

pub(super) fn push_equipment_textures(
    model: ModelId,
    stack: &ItemStack,
    folder: &str,
    out: &mut Vec<LayerSpec>,
) {
    let foil = crate::glint::has_foil(stack);
    let mut first = true;
    equipment_asset::each_texture(stack, folder, |texture, tint| {
        out.push(LayerSpec::pooled(model, texture, tint).with_foil(foil && first));
        first = false;
    });
}

/// Armor posed like the wearer (`HumanoidArmorLayer.copyPropertiesTo`).
pub fn pose_worn(wearer: (&Model, &HumanoidParts, &ModelPose), armor: &MobModel) -> ModelPose {
    let mut pose = armor.model.initial_pose();
    let Some(parts) = armor.humanoid() else {
        return pose;
    };
    humanoid::copy_pose(wearer, (&armor.model, parts, &mut pose));
    for i in 0..armor.model.len() {
        let id = PartId(i);
        if !armor.model.part_vertices(id).is_empty() {
            pose[id].visible = true;
        }
    }
    pose
}

pub(super) fn saddle_look(kind: EntityKind, baby: bool) -> Option<(ModelId, &'static str)> {
    if baby {
        return None;
    }
    Some(match kind {
        EntityKind::Horse => (ModelId::HorseSaddle, "horse_saddle"),
        EntityKind::Donkey => (ModelId::HorseSaddle, "donkey_saddle"),
        EntityKind::Mule => (ModelId::HorseSaddle, "mule_saddle"),
        EntityKind::ZombieHorse => (ModelId::HorseSaddle, "zombie_horse_saddle"),
        EntityKind::SkeletonHorse => (ModelId::HorseSaddle, "skeleton_horse_saddle"),
        EntityKind::Pig => (ModelId::PigSaddle, "pig_saddle"),
        EntityKind::Strider => (ModelId::Strider, "strider_saddle"),
        EntityKind::Camel => (ModelId::CamelSaddle, "camel_saddle"),
        EntityKind::CamelHusk => (ModelId::CamelSaddle, "camel_husk_saddle"),
        EntityKind::Nautilus | EntityKind::ZombieNautilus => {
            (ModelId::NautilusSaddle, "nautilus_saddle")
        }
        _ => return None,
    })
}

/// The armor model for `slot` on an entity of this age.
pub fn armor_model(kind: EntityKind, slot: ArmorSlot, baby: bool) -> ModelId {
    let style = match kind {
        EntityKind::ArmorStand => ArmorStyle::ArmorStand,
        EntityKind::ZombieVillager if !baby => ArmorStyle::ZombieVillager,
        k if k.is_piglin() => ArmorStyle::Piglin,
        _ => ArmorStyle::Humanoid,
    };
    if baby {
        ModelId::BabyArmor(style, slot)
    } else {
        ModelId::Armor(style, slot)
    }
}

/// The equipment layer type armor in `slot` is drawn from.
fn armor_folder(slot: ArmorSlot, baby: bool) -> &'static str {
    match (baby, slot) {
        (true, _) => "humanoid_baby",
        (false, ArmorSlot::Legs) => "humanoid_leggings",
        (false, _) => "humanoid",
    }
}

/// `EquipmentLayerRenderer`'s textures for `stack` worn in `slot`.
pub fn armor_textures(
    stack: &ItemStack,
    slot: ArmorSlot,
    baby: bool,
) -> Vec<(std::sync::Arc<str>, [f32; 3])> {
    equipment_asset::textures(stack, armor_folder(slot, baby))
}

/// Whether `stack` has anything to draw as armor in `slot`
/// (`HumanoidArmorLayer.shouldRender`), without building a path.
pub fn wears_as_armor(stack: &ItemStack, slot: ArmorSlot) -> bool {
    equipment_asset::has_textures(stack, armor_folder(slot, false))
}

/// `EquipmentLayerRenderer` trim overlay. Babies have none (`HUMANOID_BABY`).
fn trim_overlay(stack: &ItemStack, slot: ArmorSlot, baby: bool) -> Option<std::sync::Arc<str>> {
    if baby {
        return None;
    }
    let trim = stack.get_data_component::<TrimImpl>()?;
    let material = trim.material_enum()?;
    let pattern = trim.pattern_enum()?;
    let folder = match slot {
        ArmorSlot::Legs => "humanoid_leggings",
        _ => "humanoid",
    };
    let palette = trim_palette(stack, material);
    Some(crate::util::intern::intern(|s| {
        for part in [
            "minecraft:trims/entity/",
            folder,
            "/",
            pattern.to_name(),
            "#",
            palette,
        ] {
            s.push_str(part);
        }
    }))
}

/// Same-material trims use the `*_darker` palette (`equipment/*.json` `trim_overrides`).
fn trim_palette(
    stack: &ItemStack,
    material: pumpkin_data::trim_material::TrimMaterial,
) -> &'static str {
    use pumpkin_data::trim_material::TrimMaterial;
    let asset = stack
        .get_data_component::<EquippableImpl>()
        .and_then(|e| e.asset_id.as_deref())
        .map(|id| id.trim_start_matches("minecraft:"))
        .unwrap_or("");
    if asset == material.to_name() {
        match material {
            TrimMaterial::Copper => "minecraft:trim/copper_darker",
            TrimMaterial::Diamond => "minecraft:trim/diamond_darker",
            TrimMaterial::Gold => "minecraft:trim/gold_darker",
            TrimMaterial::Iron => "minecraft:trim/iron_darker",
            TrimMaterial::Netherite => "minecraft:trim/netherite_darker",
            _ => material.palette_id(),
        }
    } else {
        material.palette_id()
    }
}
