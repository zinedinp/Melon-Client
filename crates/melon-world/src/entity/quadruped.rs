//! Vanilla `QuadrupedModel`.

use super::living::LivingRenderState;
use super::model::{Model, ModelPose, PartDefinition, PartId};
use melon_core::math::angle::{self, PI};
use crate::model_part::{Cube, Pose};

/// `QuadrupedModel.createBodyMesh(legSize, mirrorLeftLeg, mirrorRightLeg, g)`.
pub fn create_body_mesh(
    leg_size: f32,
    mirror_left: bool,
    mirror_right: bool,
    g: f32,
) -> PartDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -4.0, -8.0), (8.0, 8.0, 8.0)).grown(g)],
        Pose::offset(0.0, 18.0 - leg_size, -6.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((28.0, 8.0), (-5.0, -10.0, -7.0), (10.0, 16.0, 8.0)).grown(g)],
        Pose::offset_rotation((0.0, 17.0 - leg_size, 2.0), (PI / 2.0, 0.0, 0.0)),
    );
    create_legs(&mut root, mirror_left, mirror_right, leg_size, g);
    root
}

/// `QuadrupedModel.createLegs`.
pub fn create_legs(
    root: &mut PartDefinition,
    mirror_left: bool,
    mirror_right: bool,
    leg_size: f32,
    g: f32,
) {
    let leg = |mirror: bool| {
        let cube = Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, leg_size, 4.0)).grown(g);
        vec![if mirror { cube.mirrored() } else { cube }]
    };
    let y = 24.0 - leg_size;
    root.add_or_replace_child(
        "right_hind_leg",
        leg(mirror_right),
        Pose::offset(-3.0, y, 7.0),
    );
    root.add_or_replace_child("left_hind_leg", leg(mirror_left), Pose::offset(3.0, y, 7.0));
    root.add_or_replace_child(
        "right_front_leg",
        leg(mirror_right),
        Pose::offset(-3.0, y, -5.0),
    );
    root.add_or_replace_child(
        "left_front_leg",
        leg(mirror_left),
        Pose::offset(3.0, y, -5.0),
    );
}

/// The legs any four-legged model names the same way.
#[derive(Clone, Copy, Debug)]
pub struct Legs {
    pub right_hind: PartId,
    pub left_hind: PartId,
    pub right_front: PartId,
    pub left_front: PartId,
}

impl Legs {
    pub fn find(model: &Model) -> Self {
        Self {
            right_hind: model.part("right_hind_leg"),
            left_hind: model.part("left_hind_leg"),
            right_front: model.part("right_front_leg"),
            left_front: model.part("left_front_leg"),
        }
    }

    /// The trot: diagonal legs swing together.
    pub fn walk(&self, pose: &mut ModelPose, position: f32, speed: f32, phase_shift: f32) {
        let swing =
            |offset: f32| angle::cos(position * 0.6662 + offset + phase_shift) * 1.4 * speed;
        pose[self.right_hind].pose.rotation.x = swing(0.0);
        pose[self.left_hind].pose.rotation.x = swing(PI);
        pose[self.right_front].pose.rotation.x = swing(PI);
        pose[self.left_front].pose.rotation.x = swing(0.0);
    }
}

/// `QuadrupedModel`'s parts.
#[derive(Clone, Copy, Debug)]
pub struct QuadrupedParts {
    pub head: PartId,
    pub legs: Legs,
}

impl QuadrupedParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            legs: Legs::find(model),
        }
    }
}

/// `QuadrupedModel.setupAnim`.
pub fn setup_anim(parts: &QuadrupedParts, pose: &mut ModelPose, state: &LivingRenderState) {
    look(&mut pose[parts.head].pose, state);
    parts
        .legs
        .walk(pose, state.walk_position, state.walk_speed, 0.0);
}

/// The head turned to where the entity looks (`xRot`, `yRot`).
pub fn look(head: &mut Pose, state: &LivingRenderState) {
    head.rotation.x = state.x_rot.to_radians();
    head.rotation.y = state.y_rot.to_radians();
}
