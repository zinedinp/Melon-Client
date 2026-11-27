//! `RavagerModel`.

use super::{LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::look;
use melon_core::math::angle::{self, HALF_PI, PI};
use crate::model_part::{Cube, Pose};

/// `RavagerRenderer`'s texture.
pub const TEXTURE: &str = "minecraft:entity/illager/ravager";

pub fn texture() -> &'static str {
    TEXTURE
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let neck = root.add_or_replace_child(
        "neck",
        vec![Cube::new(
            (68.0, 73.0),
            (-5.0, -1.0, -18.0),
            (10.0, 10.0, 18.0),
        )],
        Pose::offset(0.0, -7.0, 5.5),
    );
    let head = neck.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-8.0, -20.0, -14.0), (16.0, 20.0, 16.0)),
            Cube::new((0.0, 0.0), (-2.0, -6.0, -18.0), (4.0, 8.0, 4.0)),
        ],
        Pose::offset(0.0, 16.0, -17.0),
    );
    head.add_or_replace_child(
        "right_horn",
        vec![Cube::new(
            (74.0, 55.0),
            (0.0, -14.0, -2.0),
            (2.0, 14.0, 4.0),
        )],
        Pose::offset_rotation((-10.0, -14.0, -8.0), (1.099_557_4, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "left_horn",
        vec![Cube::new((74.0, 55.0), (0.0, -14.0, -2.0), (2.0, 14.0, 4.0)).mirrored()],
        Pose::offset_rotation((8.0, -14.0, -8.0), (1.099_557_4, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "mouth",
        vec![Cube::new(
            (0.0, 36.0),
            (-8.0, 0.0, -16.0),
            (16.0, 3.0, 16.0),
        )],
        Pose::offset(0.0, -2.0, 2.0),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 55.0), (-7.0, -10.0, -7.0), (14.0, 16.0, 20.0)),
            Cube::new((0.0, 91.0), (-6.0, 6.0, -7.0), (12.0, 13.0, 18.0)),
        ],
        Pose::offset_rotation((0.0, 1.0, 2.0), (HALF_PI, 0.0, 0.0)),
    );
    let hind = || vec![Cube::new((96.0, 0.0), (-4.0, 0.0, -4.0), (8.0, 37.0, 8.0))];
    root.add_or_replace_child("right_hind_leg", hind(), Pose::offset(-8.0, -13.0, 18.0));
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((96.0, 0.0), (-4.0, 0.0, -4.0), (8.0, 37.0, 8.0)).mirrored()],
        Pose::offset(8.0, -13.0, 18.0),
    );
    let front = || vec![Cube::new((64.0, 0.0), (-4.0, 0.0, -4.0), (8.0, 37.0, 8.0))];
    root.add_or_replace_child("right_front_leg", front(), Pose::offset(-8.0, -13.0, -5.0));
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((64.0, 0.0), (-4.0, 0.0, -4.0), (8.0, 37.0, 8.0)).mirrored()],
        Pose::offset(8.0, -13.0, -5.0),
    );
    LayerDefinition::new(root, 128.0, 128.0)
}

/// What the pose reads besides the walk.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RavagerPose {
    pub stunned: f32,
    pub roaring: f32,
    pub attacking: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RavagerParts {
    head: PartId,
    mouth: PartId,
    neck: PartId,
    right_hind_leg: PartId,
    left_hind_leg: PartId,
    right_front_leg: PartId,
    left_front_leg: PartId,
}

impl RavagerParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            mouth: model.part("mouth"),
            neck: model.part("neck"),
            right_hind_leg: model.part("right_hind_leg"),
            left_hind_leg: model.part("left_hind_leg"),
            right_front_leg: model.part("right_front_leg"),
            left_front_leg: model.part("left_front_leg"),
        }
    }
}

/// `Mth.triangleWave`.
fn triangle_wave(index: f32, period: f32) -> f32 {
    ((index % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

/// `RavagerModel.setupAnim`.
pub fn setup_anim(
    parts: &RavagerParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    ravager: RavagerPose,
) {
    let neck_x_rot = pose[parts.neck].pose.rotation.x;
    if ravager.attacking > 0.0 {
        let head_anim = triangle_wave(ravager.attacking, 10.0);
        let scaled = (1.0 + head_anim) * 0.5;
        let head_pos = scaled * scaled * scaled * 12.0;
        let y_offset = head_pos * angle::sin(neck_x_rot);
        let neck = &mut pose[parts.neck].pose;
        neck.offset.z = -6.5 + head_pos;
        neck.offset.y = -7.0 - y_offset;
        pose[parts.mouth].pose.rotation.x = if ravager.attacking > 5.0 {
            angle::sin((-4.0 + ravager.attacking) / 4.0) * PI * 0.4
        } else {
            (PI / 20.0) * angle::sin(PI * ravager.attacking / 10.0)
        };
    } else {
        let y_offset = -angle::sin(neck_x_rot);
        let stunned = ravager.stunned > 0.0;
        {
            let neck = &mut pose[parts.neck].pose;
            neck.offset.x = 0.0;
            neck.offset.y = -7.0 - y_offset;
            neck.offset.z = 5.5;
            neck.rotation.x = if stunned { 0.219_911_49 } else { 0.0 };
        }
        pose[parts.mouth].pose.rotation.x = PI * if stunned { 0.05 } else { 0.01 };
        if stunned {
            pose[parts.neck].pose.offset.x = angle::sin(ravager.stunned / 40.0 * 10.0) * 3.0;
        } else if ravager.roaring > 0.0 {
            let mouth = angle::sin(ravager.roaring * PI * 0.25);
            pose[parts.mouth].pose.rotation.x = (HALF_PI) * mouth;
        }
    }
    look(&mut pose[parts.head].pose, state);
    let position = state.walk_position * 0.6662;
    let leg_rot = 0.4 * state.walk_speed;
    pose[parts.right_hind_leg].pose.rotation.x = angle::cos(position) * leg_rot;
    pose[parts.left_hind_leg].pose.rotation.x = angle::cos(position + PI) * leg_rot;
    pose[parts.right_front_leg].pose.rotation.x = angle::cos(position + PI) * leg_rot;
    pose[parts.left_front_leg].pose.rotation.x = angle::cos(position) * leg_rot;
}

/// `RavagerRenderer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Ravager, TEXTURE));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_legs_walk_and_a_roar_opens_the_mouth() {
        let model = Model::bake(&layer());
        let parts = RavagerParts::find(&model);
        let rest = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                RavagerPose::default(),
            );
            pose
        };
        let walking = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState {
                    walk_position: 1.2,
                    walk_speed: 1.0,
                    ..Default::default()
                },
                RavagerPose::default(),
            );
            pose
        };
        assert_ne!(
            walking[parts.right_hind_leg].pose.rotation.x,
            rest[parts.right_hind_leg].pose.rotation.x
        );
        let roaring = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                RavagerPose {
                    roaring: 1.0,
                    ..Default::default()
                },
            );
            pose
        };
        assert!(roaring[parts.mouth].pose.rotation.x > rest[parts.mouth].pose.rotation.x + 0.5);
        assert_eq!(texture(), TEXTURE);
    }
}
