//! `BatModel`: a body, a head with ears, wings of two parts and feet.

use super::{LayerSpec, ModelId as M};
use crate::world::entity::animations::bat::{BAT_FLYING, BAT_RESTING};
use crate::world::entity::keyframe::AnimationDefinition;
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// `Bat.FLAG_RESTING`.
pub const FLAG_RESTING: u8 = 1;

/// The animation states a bat uses, by slot (`PoseInput::anim`).
pub const FLY: usize = 0;
pub const REST: usize = 1;

/// Its animations and which state clock drives each.
pub static ANIMATIONS: [(&AnimationDefinition, usize); 2] =
    [(&BAT_FLYING, FLY), (&BAT_RESTING, REST)];

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-1.5, 0.0, -1.0), (3.0, 5.0, 2.0))],
        Pose::offset(0.0, 17.0, 0.0),
    );
    let right_wing = body.add_or_replace_child(
        "right_wing",
        vec![Cube::new((12.0, 0.0), (-2.0, -2.0, 0.0), (2.0, 7.0, 0.0))],
        Pose::offset(-1.5, 0.0, 0.0),
    );
    right_wing.add_or_replace_child(
        "right_wing_tip",
        vec![Cube::new((16.0, 0.0), (-6.0, -2.0, 0.0), (6.0, 8.0, 0.0))],
        Pose::offset(-2.0, 0.0, 0.0),
    );
    let left_wing = body.add_or_replace_child(
        "left_wing",
        vec![Cube::new((12.0, 7.0), (0.0, -2.0, 0.0), (2.0, 7.0, 0.0))],
        Pose::offset(1.5, 0.0, 0.0),
    );
    left_wing.add_or_replace_child(
        "left_wing_tip",
        vec![Cube::new((16.0, 8.0), (0.0, -2.0, 0.0), (6.0, 8.0, 0.0))],
        Pose::offset(2.0, 0.0, 0.0),
    );
    body.add_or_replace_child(
        "feet",
        vec![Cube::new((16.0, 16.0), (-1.5, 0.0, 0.0), (3.0, 2.0, 0.0))],
        Pose::offset(0.0, 5.0, 0.0),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 7.0), (-2.0, -3.0, -1.0), (4.0, 3.0, 2.0))],
        Pose::offset(0.0, 17.0, 0.0),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((1.0, 15.0), (-2.5, -4.0, 0.0), (3.0, 5.0, 0.0))],
        Pose::offset(-1.5, -2.0, 0.0),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((8.0, 15.0), (-0.1, -3.0, 0.0), (3.0, 5.0, 0.0))],
        Pose::offset(1.1, -3.0, 0.0),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct BatParts {
    head: PartId,
}

impl BatParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
        }
    }
}

/// `BatModel.setupAnim` before the animations: a resting bat turns its
pub fn setup_anim(
    parts: &BatParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    resting: bool,
) {
    if resting {
        pose[parts.head].pose.rotation.y = state.y_rot.to_radians();
    }
}

/// `BatRenderer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Bat, "minecraft:entity/bat/bat"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::keyframe::KeyframeAnimation;

    #[test]
    fn both_animations_bake_for_the_model() {
        let model = Model::bake(&layer());
        for (definition, _) in &ANIMATIONS {
            assert!(KeyframeAnimation::bake(&model, definition).is_ok());
        }
    }
}
