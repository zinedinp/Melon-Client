//! `LlamaModel` and `BabyLlamaModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M, push_equipment_textures};
use crate::entity::equipment_asset;
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::{self, QuadrupedParts};
use crate::entity::wire::EquipmentSlot;
use crate::math::angle::HALF_PI;
use crate::model_part::{Cube, Pose};

/// `Llama.Variant` ids: creamy, white, brown, gray.
pub fn texture(variant: i32, baby: bool) -> &'static str {
    const ADULT: [&str; 4] = [
        "minecraft:entity/llama/llama_creamy",
        "minecraft:entity/llama/llama_white",
        "minecraft:entity/llama/llama_brown",
        "minecraft:entity/llama/llama_gray",
    ];
    const BABY: [&str; 4] = [
        "minecraft:entity/llama/llama_creamy_baby",
        "minecraft:entity/llama/llama_white_baby",
        "minecraft:entity/llama/llama_brown_baby",
        "minecraft:entity/llama/llama_gray_baby",
    ];
    let i = usize::try_from(variant)
        .ok()
        .filter(|&i| i < 4)
        .unwrap_or(0);
    if baby { BABY[i] } else { ADULT[i] }
}

fn adult_root(g: f32) -> PartDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-2.0, -14.0, -10.0), (4.0, 4.0, 9.0)).grown(g),
            Cube::new((0.0, 14.0), (-4.0, -16.0, -6.0), (8.0, 18.0, 6.0)).grown(g),
            Cube::new((17.0, 0.0), (-4.0, -19.0, -4.0), (3.0, 3.0, 2.0)).grown(g),
            Cube::new((17.0, 0.0), (1.0, -19.0, -4.0), (3.0, 3.0, 2.0)).grown(g),
        ],
        Pose::offset(0.0, 7.0, -6.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((29.0, 0.0), (-6.0, -10.0, -7.0), (12.0, 18.0, 10.0)).grown(g)],
        Pose::offset_rotation((0.0, 5.0, 2.0), (HALF_PI, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "right_chest",
        vec![Cube::new((45.0, 28.0), (-3.0, 0.0, 0.0), (8.0, 8.0, 3.0)).grown(g)],
        Pose::offset_rotation((-8.5, 3.0, 3.0), (0.0, HALF_PI, 0.0)),
    );
    root.add_or_replace_child(
        "left_chest",
        vec![Cube::new((45.0, 41.0), (-3.0, 0.0, 0.0), (8.0, 8.0, 3.0)).grown(g)],
        Pose::offset_rotation((5.5, 3.0, 3.0), (0.0, HALF_PI, 0.0)),
    );
    let leg = || vec![Cube::new((29.0, 29.0), (-2.0, 0.0, -2.0), (4.0, 14.0, 4.0)).grown(g)];
    root.add_or_replace_child("right_hind_leg", leg(), Pose::offset(-3.5, 10.0, 6.0));
    root.add_or_replace_child("left_hind_leg", leg(), Pose::offset(3.5, 10.0, 6.0));
    root.add_or_replace_child("right_front_leg", leg(), Pose::offset(-3.5, 10.0, -5.0));
    root.add_or_replace_child("left_front_leg", leg(), Pose::offset(3.5, 10.0, -5.0));
    root
}

fn baby_root(g: f32) -> PartDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-3.0, -9.0, -4.0), (6.0, 11.0, 4.0)).grown(g),
            Cube::new((0.0, 15.0), (-1.5, -7.0, -7.0), (3.0, 3.0, 3.0)).grown(g),
            Cube::new((20.0, 4.0), (0.5, -11.0, -3.0), (2.0, 2.0, 2.0)).grown(g),
            Cube::new((20.0, 0.0), (-2.5, -11.0, -3.0), (2.0, 2.0, 2.0)).grown(g),
        ],
        Pose::offset(0.0, 12.0, -4.0),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((0.0, 45.0), (-1.4, -0.5, -1.5), (3.0, 8.0, 3.0)).grown(g)],
        Pose::offset(-2.5, 16.5, 4.5),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((12.0, 45.0), (-1.6, -0.5, -1.5), (3.0, 8.0, 3.0)).grown(g)],
        Pose::offset(2.5, 16.5, 4.5),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((0.0, 34.0), (-1.4, -0.5, -1.5), (3.0, 8.0, 3.0)).grown(g)],
        Pose::offset(-2.5, 16.5, -3.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((12.0, 34.0), (-1.6, -0.5, -1.5), (3.0, 8.0, 3.0)).grown(g)],
        Pose::offset(2.5, 16.5, -3.5),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 15.0), (-4.0, -3.0, -8.5), (8.0, 6.0, 13.0)).grown(g)],
        Pose::offset(0.0, 14.0, 2.5),
    );
    root.add_or_replace_child(
        "right_chest",
        vec![Cube::new((45.0, 28.0), (-3.0, 0.0, 0.0), (8.0, 8.0, 3.0)).grown(g)],
        Pose::offset_rotation((-8.5, 4.0, 3.0), (0.0, HALF_PI, 0.0)),
    );
    root.add_or_replace_child(
        "left_chest",
        vec![Cube::new((45.0, 41.0), (-3.0, 0.0, 0.0), (8.0, 8.0, 3.0)).grown(g)],
        Pose::offset_rotation((5.5, 4.0, 3.0), (0.0, HALF_PI, 0.0)),
    );
    root
}

/// `LlamaModel.createBodyLayer(NONE)`.
pub fn adult() -> LayerDefinition {
    LayerDefinition::new(adult_root(0.0), 128.0, 64.0)
}

/// `LlamaModel.createBodyLayer(0.5)` (`LLAMA_DECOR`).
pub fn decor() -> LayerDefinition {
    LayerDefinition::new(adult_root(0.5), 128.0, 64.0)
}

/// `BabyLlamaModel.createBodyLayer(NONE)`.
pub fn baby() -> LayerDefinition {
    LayerDefinition::new(baby_root(0.0), 64.0, 64.0)
}

/// `BabyLlamaModel.createBodyLayer(0.2)` (`LLAMA_BABY_DECOR`).
pub fn baby_decor() -> LayerDefinition {
    LayerDefinition::new(baby_root(0.2), 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct LlamaParts {
    quadruped: QuadrupedParts,
    right_chest: PartId,
    left_chest: PartId,
}

impl LlamaParts {
    pub fn find(model: &Model) -> Self {
        Self {
            quadruped: QuadrupedParts::find(model),
            right_chest: model.part("right_chest"),
            left_chest: model.part("left_chest"),
        }
    }
}

/// `LlamaModel.setupAnim`: look, the quadruped walk, chest visibility.
pub fn setup_anim(
    parts: &LlamaParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    has_chest: bool,
) {
    quadruped::setup_anim(&parts.quadruped, pose, living);
    pose[parts.right_chest].visible = has_chest;
    pose[parts.left_chest].visible = has_chest;
}

/// `LlamaRenderer` and `LlamaDecorLayer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Llama | EntityKind::TraderLlama => {
            out.push(plain(
                if baby { M::LlamaBaby } else { M::Llama },
                texture(a.llama_variant, baby),
            ));
            // `LlamaDecorLayer`: an adult's carpet, else a trader's blanket.
            let decor = if baby {
                M::LlamaBabyDecor
            } else {
                M::LlamaDecor
            };
            let carpet = entity.equipment.get(EquipmentSlot::Body);
            if equipment_asset::asset_of(carpet).is_some() && !baby {
                push_equipment_textures(decor, carpet, "llama_body", out);
            } else if entity.kind == EntityKind::TraderLlama {
                let asset = if baby {
                    "minecraft:trader_llama_baby"
                } else {
                    "minecraft:trader_llama"
                };
                equipment_asset::each_asset_texture(asset, "llama_body", None, |texture, tint| {
                    out.push(LayerSpec::pooled(decor, texture, tint));
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_and_chests_hide() {
        for layer in [adult(), baby(), decor(), baby_decor()] {
            let model = Model::bake(&layer);
            let parts = LlamaParts::find(&model);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState {
                    walk_position: 1.0,
                    walk_speed: 1.0,
                    ..Default::default()
                },
                false,
            );
            assert!(!pose[parts.right_chest].visible && !pose[parts.left_chest].visible);
            assert!(pose[parts.quadruped.legs.right_hind].pose.rotation.x.abs() > 0.1);
        }
        assert_eq!(texture(0, false), "minecraft:entity/llama/llama_creamy");
        assert_eq!(texture(3, true), "minecraft:entity/llama/llama_gray_baby");
    }
}
