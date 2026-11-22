//! `ChickenModel`: two legs and two flapping wings; the adult

use super::{ClientEntity, LayerSpec, ModelId as M, variant_body};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::look;
use crate::world::model_part::{Cube, Pose};
use pumpkin_data::chicken_variant::ChickenVariant;

/// `AdultChickenModel.createBaseChickenModel`.
fn base() -> PartDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-2.0, -6.0, -2.0), (4.0, 6.0, 3.0))],
        Pose::offset(0.0, 15.0, -4.0),
    );
    head.add_or_replace_child(
        "beak",
        vec![Cube::new((14.0, 0.0), (-2.0, -4.0, -4.0), (4.0, 2.0, 2.0))],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "red_thing",
        vec![Cube::new((14.0, 4.0), (-1.0, -2.0, -3.0), (2.0, 2.0, 2.0))],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 9.0), (-3.0, -4.0, -3.0), (6.0, 8.0, 6.0))],
        Pose::offset_rotation((0.0, 16.0, 0.0), (PI / 2.0, 0.0, 0.0)),
    );
    let leg = || vec![Cube::new((26.0, 0.0), (-1.0, 0.0, -3.0), (3.0, 5.0, 3.0))];
    root.add_or_replace_child("right_leg", leg(), Pose::offset(-2.0, 19.0, 1.0));
    root.add_or_replace_child("left_leg", leg(), Pose::offset(1.0, 19.0, 1.0));
    root.add_or_replace_child(
        "right_wing",
        vec![Cube::new((24.0, 13.0), (0.0, 0.0, -3.0), (1.0, 4.0, 6.0))],
        Pose::offset(-4.0, 13.0, 0.0),
    );
    root.add_or_replace_child(
        "left_wing",
        vec![Cube::new((24.0, 13.0), (-1.0, 0.0, -3.0), (1.0, 4.0, 6.0))],
        Pose::offset(4.0, 13.0, 0.0),
    );
    root
}

pub fn adult() -> LayerDefinition {
    LayerDefinition::new(base(), 64.0, 32.0)
}

/// `ColdChickenModel`: a fluffy tail and a hood.
pub fn cold() -> LayerDefinition {
    let mut root = base();
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 9.0), (-3.0, -4.0, -3.0), (6.0, 8.0, 6.0)),
            Cube::new((38.0, 9.0), (0.0, 3.0, -1.0), (0.0, 3.0, 5.0)),
        ],
        Pose::offset_rotation((0.0, 16.0, 0.0), (PI / 2.0, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-2.0, -6.0, -2.0), (4.0, 6.0, 3.0)),
            Cube::new((44.0, 0.0), (-3.0, -7.0, -2.015), (6.0, 3.0, 4.0)),
        ],
        Pose::offset(0.0, 15.0, -4.0),
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 0.0), (-2.0, -2.25, -0.75), (4.0, 4.0, 4.0)),
            Cube::new((10.0, 8.0), (-1.0, -0.25, -1.75), (2.0, 1.0, 1.0)),
        ],
        Pose::offset(0.0, 20.25, -1.25),
    );
    for (name, leg_uv, foot_uv, x) in [
        ("left_leg", (2.0, 2.0), (0.0, 1.0), 1.0),
        ("right_leg", (0.0, 2.0), (0.0, 0.0), -1.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![
                Cube::new(leg_uv, (-0.5, 0.0, 0.0), (1.0, 2.0, 0.0)),
                Cube::new(foot_uv, (-0.5, 2.0, -1.0), (1.0, 0.0, 1.0)),
            ],
            Pose::offset(x, 22.0, 0.5),
        );
    }
    root.add_or_replace_child(
        "right_wing",
        vec![Cube::new((6.0, 8.0), (0.0, 0.0, -1.0), (1.0, 0.0, 2.0))],
        Pose::offset(2.0, 20.0, 0.0),
    );
    root.add_or_replace_child(
        "left_wing",
        vec![Cube::new((4.0, 8.0), (-1.0, 0.0, -1.0), (1.0, 0.0, 2.0))],
        Pose::offset(-2.0, 20.0, 0.0),
    );
    LayerDefinition::new(root, 16.0, 16.0)
}

#[derive(Clone, Copy, Debug)]
pub struct ChickenParts {
    head: Option<PartId>,
    right_leg: PartId,
    left_leg: PartId,
    right_wing: PartId,
    left_wing: PartId,
}

impl ChickenParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.find("head"),
            right_leg: model.part("right_leg"),
            left_leg: model.part("left_leg"),
            right_wing: model.part("right_wing"),
            left_wing: model.part("left_wing"),
        }
    }
}

/// `ChickenModel.setupAnim` (and the adult's head).
pub fn setup_anim(
    parts: &ChickenParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    (flap, flap_speed): (f32, f32),
) {
    if let Some(head) = parts.head {
        look(&mut pose[head].pose, state);
    }
    let angle = (angle::sin(flap) + 1.0) * flap_speed;
    let swing =
        |offset: f32| angle::cos(state.walk_position * 0.6662 + offset) * 1.4 * state.walk_speed;
    pose[parts.right_leg].pose.rotation.x = swing(0.0);
    pose[parts.left_leg].pose.rotation.x = swing(PI);
    pose[parts.right_wing].pose.rotation.z = angle;
    pose[parts.left_wing].pose.rotation.z = -angle;
}

/// `ChickenRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
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
