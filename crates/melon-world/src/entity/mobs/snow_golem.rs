//! `SnowGolemModel`.

use super::{LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::look;
use crate::math::angle;
use crate::model_part::{Cube, Pose};

/// `CubeDeformation(-0.5)`: every ball is a pixel smaller than its box.
const SHRINK: f32 = -0.5;

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)).grown(SHRINK)],
        Pose::offset(0.0, 4.0, 0.0),
    );
    let arm = || vec![Cube::new((32.0, 0.0), (-1.0, 0.0, -1.0), (12.0, 2.0, 2.0)).grown(SHRINK)];
    root.add_or_replace_child(
        "left_arm",
        arm(),
        Pose::offset_rotation((5.0, 6.0, 1.0), (0.0, 0.0, 1.0)),
    );
    root.add_or_replace_child(
        "right_arm",
        arm(),
        Pose::offset_rotation((-5.0, 6.0, -1.0), (0.0, crate::math::angle::PI, -1.0)),
    );
    root.add_or_replace_child(
        "upper_body",
        vec![Cube::new((0.0, 16.0), (-5.0, -10.0, -5.0), (10.0, 10.0, 10.0)).grown(SHRINK)],
        Pose::offset(0.0, 13.0, 0.0),
    );
    root.add_or_replace_child(
        "lower_body",
        vec![Cube::new((0.0, 36.0), (-6.0, -12.0, -6.0), (12.0, 12.0, 12.0)).grown(SHRINK)],
        Pose::offset(0.0, 24.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct SnowGolemParts {
    head: PartId,
    left_arm: PartId,
    right_arm: PartId,
    upper_body: PartId,
}

impl SnowGolemParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            left_arm: model.part("left_arm"),
            right_arm: model.part("right_arm"),
            upper_body: model.part("upper_body"),
        }
    }

    /// The head, which wears the pumpkin.
    pub fn head(&self) -> PartId {
        self.head
    }
}

/// `SnowGolemModel.setupAnim`.
pub fn setup_anim(parts: &SnowGolemParts, pose: &mut ModelPose, state: &LivingRenderState) {
    look(&mut pose[parts.head].pose, state);
    let turn = state.y_rot.to_radians() * 0.25;
    pose[parts.upper_body].pose.rotation.y = turn;
    let (sin, cos) = (angle::sin(turn), angle::cos(turn));
    let left = &mut pose[parts.left_arm].pose;
    left.rotation.y = turn;
    left.offset.x = cos * 5.0;
    left.offset.z = -sin * 5.0;
    let right = &mut pose[parts.right_arm].pose;
    right.rotation.y = turn + crate::math::angle::PI;
    right.offset.x = -cos * 5.0;
    right.offset.z = sin * 5.0;
}

/// `SnowGolemHeadLayer`: the block on the head, if it has its pumpkin.
pub const HEAD: &str = "minecraft:block/carved_pumpkin";

/// `SnowGolemRenderer`; the pumpkin is [`HEAD`].
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(
        M::SnowGolem,
        "minecraft:entity/snow_golem/snow_golem",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arms_follow_the_turn_of_the_body() {
        let model = Model::bake(&layer());
        let parts = SnowGolemParts::find(&model);
        let mut pose = model.initial_pose();
        let state = LivingRenderState {
            y_rot: 80.0,
            ..Default::default()
        };
        setup_anim(&parts, &mut pose, &state);
        let turn = 20.0_f32.to_radians();
        assert!((pose[parts.upper_body].pose.rotation.y - turn).abs() < 1e-5);
        let (l, r) = (
            pose[parts.left_arm].pose.offset,
            pose[parts.right_arm].pose.offset,
        );
        assert!((l.x + r.x).abs() < 1e-5 && (l.z + r.z).abs() < 1e-5);
        assert!((l.x.hypot(l.z) - 5.0).abs() < 1e-4);
    }
}
