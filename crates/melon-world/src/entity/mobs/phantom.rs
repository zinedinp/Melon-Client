//! `PhantomModel`: a body with a two-part tail and two two-part wings.

use super::{LayerSpec, ModelId as M};
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, DEG_TO_RAD};

pub const TEXTURE: &str = "minecraft:entity/phantom/phantom";
/// `PhantomEyesLayer`, drawn glowing (`RenderTypes.eyes`).
pub const EYES: &str = "minecraft:entity/phantom/phantom_eyes";

/// `Phantom.getUniqueFlapTickOffset`: `id * 3`.
pub fn flap_time(id: i32, age_in_ticks: f32) -> f32 {
    id.wrapping_mul(3) as f32 + age_in_ticks
}

/// `PhantomRenderer.scale`: `1 + 0.15 * size`.
pub fn size_scale(size: i32) -> f32 {
    1.0 + 0.15 * size as f32
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 8.0), (-3.0, -2.0, -8.0), (5.0, 3.0, 9.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-0.1, 0.0, 0.0)),
    );
    let tail_base = body.add_or_replace_child(
        "tail_base",
        vec![Cube::new((3.0, 20.0), (-2.0, 0.0, 0.0), (3.0, 2.0, 6.0))],
        Pose::offset(0.0, -2.0, 1.0),
    );
    tail_base.add_or_replace_child(
        "tail_tip",
        vec![Cube::new((4.0, 29.0), (-1.0, 0.0, 0.0), (1.0, 1.0, 6.0))],
        Pose::offset(0.0, 0.5, 6.0),
    );
    let left_wing_base = body.add_or_replace_child(
        "left_wing_base",
        vec![Cube::new((23.0, 12.0), (0.0, 0.0, 0.0), (6.0, 2.0, 9.0))],
        Pose::offset_rotation((2.0, -2.0, -8.0), (0.0, 0.0, 0.1)),
    );
    left_wing_base.add_or_replace_child(
        "left_wing_tip",
        vec![Cube::new((16.0, 24.0), (0.0, 0.0, 0.0), (13.0, 1.0, 9.0))],
        Pose::offset_rotation((6.0, 0.0, 0.0), (0.0, 0.0, 0.1)),
    );
    let right_wing_base = body.add_or_replace_child(
        "right_wing_base",
        vec![Cube::new((23.0, 12.0), (-6.0, 0.0, 0.0), (6.0, 2.0, 9.0)).mirrored()],
        Pose::offset_rotation((-3.0, -2.0, -8.0), (0.0, 0.0, -0.1)),
    );
    right_wing_base.add_or_replace_child(
        "right_wing_tip",
        vec![Cube::new((16.0, 24.0), (-13.0, 0.0, 0.0), (13.0, 1.0, 9.0)).mirrored()],
        Pose::offset_rotation((-6.0, 0.0, 0.0), (0.0, 0.0, -0.1)),
    );
    body.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -2.0, -5.0), (7.0, 3.0, 5.0))],
        Pose::offset_rotation((0.0, 1.0, -7.0), (0.2, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct PhantomParts {
    left_wing_base: PartId,
    left_wing_tip: PartId,
    right_wing_base: PartId,
    right_wing_tip: PartId,
    tail_base: PartId,
    tail_tip: PartId,
}

impl PhantomParts {
    pub fn find(model: &Model) -> Self {
        Self {
            left_wing_base: model.part("left_wing_base"),
            left_wing_tip: model.part("left_wing_tip"),
            right_wing_base: model.part("right_wing_base"),
            right_wing_tip: model.part("right_wing_tip"),
            tail_base: model.part("tail_base"),
            tail_tip: model.part("tail_tip"),
        }
    }
}

/// `PhantomModel.setupAnim`.
pub fn setup_anim(parts: &PhantomParts, pose: &mut ModelPose, flap_time: f32) {
    let anim = flap_time * 7.448451 * DEG_TO_RAD;
    let flap = angle::cos(anim) * 16.0 * DEG_TO_RAD;
    pose[parts.left_wing_base].pose.rotation.z = flap;
    pose[parts.left_wing_tip].pose.rotation.z = flap;
    pose[parts.right_wing_base].pose.rotation.z = -flap;
    pose[parts.right_wing_tip].pose.rotation.z = -flap;
    let tail = -(5.0 + angle::cos(anim * 2.0) * 5.0) * DEG_TO_RAD;
    pose[parts.tail_base].pose.rotation.x = tail;
    pose[parts.tail_tip].pose.rotation.x = tail;
}

/// `PhantomRenderer` and `PhantomEyesLayer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Phantom, TEXTURE));
    out.push(plain(M::Phantom, EYES).glowing());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wings_beat_opposite_and_the_tail_follows() {
        let model = Model::bake(&layer());
        let parts = PhantomParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(&parts, &mut pose, 0.0);
        let left = pose[parts.left_wing_base].pose.rotation.z;
        assert!((left - 16.0 * DEG_TO_RAD).abs() < 1e-5);
        assert_eq!(pose[parts.right_wing_base].pose.rotation.z, -left);
        assert_eq!(
            pose[parts.tail_base].pose.rotation.x,
            pose[parts.tail_tip].pose.rotation.x
        );
        let mut later = model.initial_pose();
        setup_anim(&parts, &mut later, 10.0);
        assert_ne!(
            later[parts.left_wing_base].pose.rotation.z,
            pose[parts.left_wing_base].pose.rotation.z
        );
    }

    #[test]
    fn size_grows_the_model_and_the_eyes_are_the_same_mesh() {
        assert!((size_scale(0) - 1.0).abs() < 1e-6);
        assert!((size_scale(1) - 1.15).abs() < 1e-6);
        assert_eq!(flap_time(2, 1.0), 7.0);
        assert_eq!(EYES, "minecraft:entity/phantom/phantom_eyes");
        assert_eq!(TEXTURE, "minecraft:entity/phantom/phantom");
    }
}
