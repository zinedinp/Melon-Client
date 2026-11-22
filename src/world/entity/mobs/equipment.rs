//! What a mob wears and holds: armor, saddles and other equipment layers

use super::*;

/// Whether the item in hand is drawn: each illager shows it only in its
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
        let textures = armor_textures(stack, slot, baby && kind != EntityKind::ArmorStand);
        // `EquipmentLayerRenderer.renderLayers`: the glint goes with the
        // first layer (the leather overlay above it shows none); with a
        // trim, vanilla glints the whole model after it, here the first
        // layer and the trim each where their texels are.
        let foil = crate::world::glint::has_foil(stack);
        for (i, (texture, tint)) in textures.into_iter().enumerate() {
            out.push(
                LayerSpec::humanoid_armor(armor_model(kind, slot, baby), texture, tint)
                    .with_foil(foil && i == 0),
            );
        }
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
                if let Some(texture) = harness_texture(body) {
                    let model = if baby {
                        ModelId::HappyGhastBabyHarness
                    } else {
                        ModelId::HappyGhastHarness
                    };
                    out.push(LayerSpec::plain(model, texture));
                }
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

fn push_equipment_textures(
    model: ModelId,
    stack: &ItemStack,
    folder: &str,
    out: &mut Vec<LayerSpec>,
) {
    let textures = equipment_textures(stack, folder);
    let foil = crate::world::glint::has_foil(stack);
    for (i, (texture, tint)) in textures.into_iter().enumerate() {
        out.push(LayerSpec::owned(model, texture, tint).with_foil(foil && i == 0));
    }
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
        EntityKind::Nautilus => (ModelId::NautilusSaddle, "nautilus_saddle"),
        _ => return None,
    })
}

/// `EquipmentLayerRenderer` textures for `stack` in `folder`.
pub(super) fn equipment_textures(stack: &ItemStack, folder: &str) -> Vec<(String, [f32; 3])> {
    if stack.is_empty() {
        return Vec::new();
    }
    let Some(asset) = stack
        .get_data_component::<EquippableImpl>()
        .and_then(|e| e.asset_id.clone())
    else {
        return Vec::new();
    };
    let name = asset.strip_prefix("minecraft:").unwrap_or(&asset);
    let texture = |name: &str| format!("minecraft:entity/equipment/{folder}/{name}");
    if let Some(dyed) = stack.get_data_component::<DyedColorImpl>() {
        let rgb = dyed.rgb as u32;
        let tint = [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0);
        if name == "leather" {
            return vec![
                (texture("leather"), tint),
                (texture("leather_overlay"), [1.0; 3]),
            ];
        }
        return vec![(texture(name), tint)];
    }
    if name == "leather" {
        let tint = [16, 8, 0].map(|shift| ((0xA0_6540_u32 >> shift) & 0xFF) as f32 / 255.0);
        return vec![
            (texture("leather"), tint),
            (texture("leather_overlay"), [1.0; 3]),
        ];
    }
    vec![(texture(name), [1.0; 3])]
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

/// `EquipmentLayerRenderer`'s textures for `stack` worn in `slot`, with
pub fn armor_textures(stack: &ItemStack, slot: ArmorSlot, baby: bool) -> Vec<(String, [f32; 3])> {
    if stack.is_empty() {
        return Vec::new();
    }
    let Some(asset) = stack
        .get_data_component::<EquippableImpl>()
        .and_then(|e| e.asset_id.clone())
    else {
        return Vec::new();
    };
    let name = asset.strip_prefix("minecraft:").unwrap_or(&asset);
    let folder = match (baby, slot) {
        (true, _) => "humanoid_baby",
        (false, ArmorSlot::Legs) => "humanoid_leggings",
        (false, _) => "humanoid",
    };
    let texture = |name: &str| format!("minecraft:entity/equipment/{folder}/{name}");
    if name == "leather" {
        let rgb = stack
            .get_data_component::<DyedColorImpl>()
            .map_or(0xA0_6540, |dyed| dyed.rgb as u32);
        let tint = [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0);
        vec![
            (texture("leather"), tint),
            (texture("leather_overlay"), [1.0; 3]),
        ]
    } else {
        vec![(texture(name), [1.0; 3])]
    }
}

/// `EquipmentLayerRenderer` trim overlay. Babies have none (`HUMANOID_BABY`).
fn trim_overlay(stack: &ItemStack, slot: ArmorSlot, baby: bool) -> Option<String> {
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
    Some(format!(
        "minecraft:trims/entity/{folder}/{}#{}",
        pattern.to_name(),
        trim_palette(stack, material)
    ))
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
