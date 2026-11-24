//! `GoatModel` and `BabyGoatModel`.

use super::rounded;
use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::EIGHTH_PI;
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::{self, QuadrupedParts};
use crate::world::model_part::{Cube, Pose};

/// `GoatRenderer.getTextureLocation`.
pub fn texture(baby: bool) -> &'static str {
    if baby {
        "minecraft:entity/goat/goat_baby"
    } else {
        "minecraft:entity/goat/goat"
    }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((2.0, 61.0), (-6.0, -11.0, -10.0), (3.0, 2.0, 1.0)),
            Cube::new((2.0, 61.0), (2.0, -11.0, -10.0), (3.0, 2.0, 1.0)).mirrored(),
            Cube::new((23.0, 52.0), (-0.5, -3.0, -14.0), (0.0, 7.0, 5.0)),
        ],
        Pose::offset(1.0, 14.0, 0.0),
    );
    head.add_or_replace_child(
        "left_horn",
        vec![Cube::new(
            (12.0, 55.0),
            (-0.01, -16.0, -10.0),
            (2.0, 7.0, 2.0),
        )],
        Pose::offset(0.0, 0.0, 0.0),
    );
    head.add_or_replace_child(
        "right_horn",
        vec![Cube::new(
            (12.0, 55.0),
            (-2.99, -16.0, -10.0),
            (2.0, 7.0, 2.0),
        )],
        Pose::offset(0.0, 0.0, 0.0),
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new(
            (34.0, 46.0),
            (-3.0, -4.0, -8.0),
            (5.0, 7.0, 10.0),
        )],
        Pose::offset_rotation((0.0, -8.0, -8.0), (0.9599, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((1.0, 1.0), (-4.0, -17.0, -7.0), (9.0, 11.0, 16.0)),
            Cube::new((0.0, 28.0), (-5.0, -18.0, -8.0), (11.0, 14.0, 11.0)),
        ],
        Pose::offset(0.0, 24.0, 0.0),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((36.0, 29.0), (0.0, 4.0, 0.0), (3.0, 6.0, 3.0))],
        Pose::offset(1.0, 14.0, 4.0),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((49.0, 29.0), (0.0, 4.0, 0.0), (3.0, 6.0, 3.0))],
        Pose::offset(-3.0, 14.0, 4.0),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((49.0, 2.0), (0.0, 0.0, 0.0), (3.0, 10.0, 3.0))],
        Pose::offset(1.0, 14.0, -6.0),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((35.0, 2.0), (0.0, 0.0, 0.0), (3.0, 10.0, 3.0))],
        Pose::offset(-3.0, 14.0, -6.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((29.0, 12.0), (-1.0, -0.5, -1.0), (2.0, 5.0, 2.0))],
        Pose::offset(1.5, 19.5, 3.0),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((21.0, 12.0), (-1.0, -0.5, -1.0), (2.0, 5.0, 2.0))],
        Pose::offset(-1.5, 19.5, 3.0),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((21.0, 5.0), (-1.0, -0.5, -1.0), (2.0, 5.0, 2.0))],
        Pose::offset(-1.5, 19.5, -2.0),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((29.0, 5.0), (-1.0, -0.5, -1.0), (2.0, 5.0, 2.0))],
        Pose::offset(1.5, 19.5, -2.0),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 10.0), (-3.0, -2.3, -4.5), (6.0, 5.0, 9.0)),
            Cube::new((0.0, 24.0), (-2.5, -2.2, -4.0), (5.0, 4.0, 8.0)),
        ],
        Pose::offset(0.0, 17.8, 0.0),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new(
            (0.0, 0.0),
            (-2.0, -3.8126, -5.1548),
            (4.0, 4.0, 6.0),
        )],
        Pose::offset_rotation((0.0, 15.5, -3.0), (0.4363, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "right_horn",
        vec![Cube::new((24.0, 0.0), (0.0, -4.5, 0.0), (1.0, 2.0, 1.0)).mirrored()],
        Pose::offset_rotation((-1.5, -1.5, -1.0), (-EIGHTH_PI, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "left_horn",
        vec![Cube::new((24.0, 0.0), (2.0, -4.5, 0.0), (1.0, 2.0, 1.0)).mirrored()],
        Pose::offset_rotation((-1.5, -1.5, -1.0), (-EIGHTH_PI, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((0.0, 12.0), (-2.0, -0.5, -0.5), (2.0, 1.0, 1.0)).mirrored()],
        Pose::offset_rotation((-1.7, -2.3126, 0.1452), (0.0, -rounded::PI_6, 0.0)),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((0.0, 12.0), (0.0, -0.5, -0.5), (2.0, 1.0, 1.0))],
        Pose::offset_rotation((1.7, -2.3126, 0.1452), (0.0, rounded::PI_6, 0.0)),
    );
    head.add_or_replace_child(
        "HeadMain",
        vec![Cube::new((0.0, 0.0), (-2.0, -2.5, -4.0), (4.0, 4.0, 6.0))],
        Pose::offset(0.0, -1.3126, -1.1548),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct GoatParts {
    quadruped: QuadrupedParts,
    left_horn: PartId,
    right_horn: PartId,
    baby: bool,
}

impl GoatParts {
    pub fn find(model: &Model) -> Self {
        Self {
            quadruped: QuadrupedParts::find(model),
            left_horn: model.part("left_horn"),
            right_horn: model.part("right_horn"),
            baby: model.find("HeadMain").is_some(),
        }
    }
}

/// `GoatRenderState`: which horns remain, and the ram's head pitch.
#[derive(Clone, Copy, Debug)]
pub struct GoatPose {
    pub has_left_horn: bool,
    pub has_right_horn: bool,
    /// `Goat.getRammingXHeadRot`, radians.
    pub ramming_x_head_rot: f32,
}

impl Default for GoatPose {
    fn default() -> Self {
        Self {
            has_left_horn: true,
            has_right_horn: true,
            ramming_x_head_rot: 0.0,
        }
    }
}

/// `GoatModel.setupAnim` (and the kid's rest pitch).
pub fn setup_anim(
    parts: &GoatParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    goat: GoatPose,
) {
    quadruped::setup_anim(&parts.quadruped, pose, living);
    pose[parts.left_horn].visible = goat.has_left_horn;
    pose[parts.right_horn].visible = goat.has_right_horn;
    if goat.ramming_x_head_rot != 0.0 {
        pose[parts.quadruped.head].pose.rotation.x = goat.ramming_x_head_rot;
    } else if parts.baby {
        pose[parts.quadruped.head].pose.rotation.x = EIGHTH_PI;
    }
}

/// `GoatRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::Goat, texture(false)),
        (M::GoatBaby, texture(true)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_and_ramming_tips_the_head() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = GoatParts::find(&model);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                GoatPose::default(),
            );
            assert!(pose[parts.left_horn].visible && pose[parts.right_horn].visible);
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                GoatPose {
                    has_left_horn: false,
                    has_right_horn: true,
                    ramming_x_head_rot: 0.5,
                },
            );
            assert!(!pose[parts.left_horn].visible);
            assert!((pose[parts.quadruped.head].pose.rotation.x - 0.5).abs() < 1e-5);
        }
    }
}
