//! `AdultStriderModel` and `BabyStriderModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::math::angle::{self, PI};
use crate::model_part::{Cube, Pose};

/// Walk cycle scale (`StriderModel.SPEED`).
const SPEED: f32 = 1.5;

/// `StriderRenderer.getTextureLocation`: warm, or cold while suffocating.
pub fn texture(suffocating: bool, baby: bool) -> &'static str {
    match (suffocating, baby) {
        (false, false) => "minecraft:entity/strider/strider",
        (false, true) => "minecraft:entity/strider/strider_baby",
        (true, false) => "minecraft:entity/strider/strider_cold",
        (true, true) => "minecraft:entity/strider/strider_cold_baby",
    }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((0.0, 32.0), (-2.0, 0.0, -2.0), (4.0, 16.0, 4.0))],
        Pose::offset(-4.0, 8.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((0.0, 55.0), (-2.0, 0.0, -2.0), (4.0, 16.0, 4.0))],
        Pose::offset(4.0, 8.0, 0.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (0.0, 0.0),
            (-8.0, -6.0, -8.0),
            (16.0, 14.0, 16.0),
        )],
        Pose::offset(0.0, 1.0, 0.0),
    );
    body.add_or_replace_child(
        "right_bottom_bristle",
        vec![Cube::new((16.0, 65.0), (-12.0, 0.0, 0.0), (12.0, 0.0, 16.0)).mirrored()],
        Pose::offset_rotation((-8.0, 4.0, -8.0), (0.0, 0.0, -1.2217305)),
    );
    body.add_or_replace_child(
        "right_middle_bristle",
        vec![Cube::new((16.0, 49.0), (-12.0, 0.0, 0.0), (12.0, 0.0, 16.0)).mirrored()],
        Pose::offset_rotation((-8.0, -1.0, -8.0), (0.0, 0.0, -1.134464)),
    );
    body.add_or_replace_child(
        "right_top_bristle",
        vec![Cube::new((16.0, 33.0), (-12.0, 0.0, 0.0), (12.0, 0.0, 16.0)).mirrored()],
        Pose::offset_rotation((-8.0, -5.0, -8.0), (0.0, 0.0, -0.87266463)),
    );
    body.add_or_replace_child(
        "left_top_bristle",
        vec![Cube::new((16.0, 33.0), (0.0, 0.0, 0.0), (12.0, 0.0, 16.0))],
        Pose::offset_rotation((8.0, -6.0, -8.0), (0.0, 0.0, 0.87266463)),
    );
    body.add_or_replace_child(
        "left_middle_bristle",
        vec![Cube::new((16.0, 49.0), (0.0, 0.0, 0.0), (12.0, 0.0, 16.0))],
        Pose::offset_rotation((8.0, -2.0, -8.0), (0.0, 0.0, 1.134464)),
    );
    body.add_or_replace_child(
        "left_bottom_bristle",
        vec![Cube::new((16.0, 65.0), (0.0, 0.0, 0.0), (12.0, 0.0, 16.0))],
        Pose::offset_rotation((8.0, 3.0, -8.0), (0.0, 0.0, 1.2217305)),
    );
    LayerDefinition::new(root, 64.0, 128.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-3.5, -3.75, -4.0), (7.0, 7.0, 8.0))],
        Pose::offset(0.0, 16.75, 0.0),
    );
    body.add_or_replace_child(
        "bristle0",
        vec![Cube::new((0.0, 21.0), (-3.5, -2.5, 0.0), (7.0, 3.0, 0.0))],
        Pose::offset(0.0, -4.25, 2.0),
    );
    body.add_or_replace_child(
        "bristle1",
        vec![Cube::new((0.0, 18.0), (-3.5, -2.5, 0.0), (7.0, 3.0, 0.0))],
        Pose::offset(0.0, -4.25, 0.0),
    );
    body.add_or_replace_child(
        "bristle2",
        vec![Cube::new((0.0, 15.0), (-3.5, -2.5, 0.0), (7.0, 3.0, 0.0))],
        Pose::offset(0.0, -4.25, -2.0),
    );
    root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((0.0, 24.0), (-1.0, 0.0, -1.0), (2.0, 4.0, 2.0))],
        Pose::offset(-1.5, 20.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((8.0, 24.0), (-1.0, 0.0, -1.0), (2.0, 4.0, 2.0))],
        Pose::offset(1.5, 20.0, 0.0),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct StriderParts {
    body: PartId,
    right_leg: PartId,
    left_leg: PartId,
    /// Adult: right top/middle/bottom then left top/middle/bottom.
    bristles: [PartId; 3],
    left_bristles: Option<[PartId; 3]>,
    baby: bool,
}

impl StriderParts {
    pub fn find(model: &Model) -> Self {
        let baby = model.find("bristle0").is_some();
        let (bristles, left_bristles) = if baby {
            (
                [
                    model.part("bristle2"),
                    model.part("bristle1"),
                    model.part("bristle0"),
                ],
                None,
            )
        } else {
            (
                [
                    model.part("right_top_bristle"),
                    model.part("right_middle_bristle"),
                    model.part("right_bottom_bristle"),
                ],
                Some([
                    model.part("left_top_bristle"),
                    model.part("left_middle_bristle"),
                    model.part("left_bottom_bristle"),
                ]),
            )
        };
        Self {
            body: model.part("body"),
            right_leg: model.part("right_leg"),
            left_leg: model.part("left_leg"),
            bristles,
            left_bristles,
            baby,
        }
    }
}

/// `StriderRenderState`: cold while suffocating, and whether it is ridden.
#[derive(Clone, Copy, Debug, Default)]
pub struct StriderPose {
    pub suffocating: bool,
    /// `StriderRenderState.isRidden`.
    pub ridden: bool,
}

/// `StriderModel.setupAnim`.
pub fn setup_anim(
    parts: &StriderParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    strider: StriderPose,
) {
    let pos = living.walk_position;
    let speed = living.walk_speed.min(0.25);
    if !strider.ridden {
        pose[parts.body].pose.rotation.x = living.x_rot.to_radians();
        pose[parts.body].pose.rotation.y = living.y_rot.to_radians();
    } else {
        pose[parts.body].pose.rotation.x = 0.0;
        pose[parts.body].pose.rotation.y = 0.0;
    }
    pose[parts.body].pose.rotation.z = 0.1 * angle::sin(pos * SPEED) * 4.0 * speed;
    pose[parts.left_leg].pose.rotation.x = angle::sin(pos * SPEED * 0.5) * 2.0 * speed;
    pose[parts.right_leg].pose.rotation.x = angle::sin(pos * SPEED * 0.5 + PI) * 2.0 * speed;
    pose[parts.left_leg].pose.rotation.z = (PI / 18.0) * angle::cos(pos * SPEED * 0.5) * speed;
    pose[parts.right_leg].pose.rotation.z =
        (PI / 18.0) * angle::cos(pos * SPEED * 0.5 + PI) * speed;
    custom_animations(parts, pose, pos, speed, living.age_in_ticks);
}

/// `AdultStriderModel` / `BabyStriderModel.customAnimations`.
fn custom_animations(parts: &StriderParts, pose: &mut ModelPose, pos: f32, speed: f32, age: f32) {
    let flow = angle::cos(pos * SPEED + PI) * speed;
    if parts.baby {
        animate_bristle(pose, parts.bristles, flow, age, true);
        pose[parts.body].pose.offset.y = 17.25 - 1.0 * angle::cos(pos * SPEED) * 2.0 * speed;
        pose[parts.left_leg].pose.offset.y =
            20.0 + 2.0 * angle::sin(pos * SPEED * 0.5 + PI) * 2.0 * speed;
        pose[parts.right_leg].pose.offset.y =
            20.0 + 2.0 * angle::sin(pos * SPEED * 0.5) * 2.0 * speed;
    } else {
        pose[parts.bristles[0]].pose.rotation.z = -0.87266463;
        pose[parts.bristles[1]].pose.rotation.z = -1.134464;
        pose[parts.bristles[2]].pose.rotation.z = -1.2217305;
        if let Some(left) = parts.left_bristles {
            pose[left[0]].pose.rotation.z = 0.87266463;
            pose[left[1]].pose.rotation.z = 1.134464;
            pose[left[2]].pose.rotation.z = 1.2217305;
            animate_bristle(pose, left, flow, age, false);
        }
        animate_bristle(pose, parts.bristles, flow, age, false);
        pose[parts.body].pose.offset.y = 2.0 - 2.0 * angle::cos(pos * SPEED) * 2.0 * speed;
        pose[parts.left_leg].pose.offset.y =
            8.0 + 2.0 * angle::sin(pos * SPEED * 0.5 + PI) * 2.0 * speed;
        pose[parts.right_leg].pose.offset.y =
            8.0 + 2.0 * angle::sin(pos * SPEED * 0.5) * 2.0 * speed;
    }
}

/// `StriderModel.animateBristle`: `add` goes on `xRot` for a cub, `zRot` for an adult.
fn animate_bristle(pose: &mut ModelPose, bristles: [PartId; 3], flow: f32, age: f32, x_rot: bool) {
    let add = |part: &mut Pose, rotation: f32| {
        if x_rot {
            part.rotation.x += rotation;
        } else {
            part.rotation.z += rotation;
        }
    };
    add(&mut pose[bristles[0]].pose, flow * 0.6);
    add(&mut pose[bristles[1]].pose, flow * 1.2);
    add(&mut pose[bristles[2]].pose, flow * 1.3);
    add(&mut pose[bristles[0]].pose, 0.1 * angle::sin(age * 0.4));
    add(&mut pose[bristles[1]].pose, 0.1 * angle::sin(age * 0.2));
    add(&mut pose[bristles[2]].pose, 0.05 * angle::sin(age * -0.4));
}

/// `StriderRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::Strider, texture(a.strider_cold, false)),
        (M::StriderBaby, texture(a.strider_cold, true)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_and_the_walk_bobs_the_body() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = StriderParts::find(&model);
            let mut pose = model.initial_pose();
            let rest_y = pose[parts.body].pose.offset.y;
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState {
                    walk_position: 1.0,
                    walk_speed: 0.25,
                    age_in_ticks: 8.0,
                    ..Default::default()
                },
                StriderPose::default(),
            );
            assert_ne!(pose[parts.body].pose.offset.y, rest_y);
        }
        assert_eq!(
            texture(true, false),
            "minecraft:entity/strider/strider_cold"
        );
    }
}
