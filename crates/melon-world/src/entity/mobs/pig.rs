//! `PigModel`, `ColdPigModel` and `BabyPigModel`: quadrupeds with a snout.

use super::{ClientEntity, LayerSpec, ModelId as M, variant_body};
use crate::entity::model::{LayerDefinition, PartDefinition};
use crate::entity::quadruped;
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::PI;
use pumpkin_data::pig_variant::PigVariant;

/// `PigModel.createBasePigModel`.
fn base() -> PartDefinition {
    let mut root = quadruped::create_body_mesh(6.0, true, false, 0.0);
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -4.0, -8.0), (8.0, 8.0, 8.0)),
            Cube::new((16.0, 16.0), (-2.0, 0.0, -9.0), (4.0, 3.0, 1.0)),
        ],
        Pose::offset(0.0, 12.0, -6.0),
    );
    root
}

pub fn adult() -> LayerDefinition {
    LayerDefinition::new(base(), 64.0, 64.0)
}

/// `PIG_SADDLE`: the pig grown by half a pixel.
pub fn saddle() -> LayerDefinition {
    let mut root = quadruped::create_body_mesh(6.0, true, false, 0.5);
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -4.0, -8.0), (8.0, 8.0, 8.0)).grown(0.5),
            Cube::new((16.0, 16.0), (-2.0, 0.0, -9.0), (4.0, 3.0, 1.0)).grown(0.5),
        ],
        Pose::offset(0.0, 12.0, -6.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `ColdPigModel`: a woolly coat over the body.
pub fn cold() -> LayerDefinition {
    let mut root = base();
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((28.0, 8.0), (-5.0, -10.0, -7.0), (10.0, 16.0, 8.0)),
            Cube::new((28.0, 32.0), (-5.0, -10.0, -7.0), (10.0, 16.0, 8.0)).grown(0.5),
        ],
        Pose::offset_rotation((0.0, 11.0, 2.0), (PI / 2.0, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-3.5, -3.0, -4.5), (7.0, 6.0, 9.0))],
        Pose::offset(0.0, 19.0, 0.5),
    );
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 15.0), (-3.5, -5.0, -5.0), (7.0, 6.0, 6.0)).grown(0.025),
            Cube::new((6.0, 27.0), (-1.5, -1.975, -6.0), (3.0, 2.0, 1.0)).grown(0.015),
        ],
        Pose::offset(0.0, 19.0, -2.0),
    );
    for (name, uv, x, z) in [
        ("left_front_leg", (0.0, 0.0), 2.5, -3.0),
        ("right_front_leg", (23.0, 0.0), -2.5, -3.0),
        ("left_hind_leg", (0.0, 4.0), 2.5, 4.0),
        ("right_hind_leg", (23.0, 4.0), -2.5, 4.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
            Pose::offset(x, 22.0, z),
        );
    }
    LayerDefinition::new(root, 32.0, 32.0)
}

/// `PigRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
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
