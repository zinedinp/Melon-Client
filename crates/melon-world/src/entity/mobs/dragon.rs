//! `EnderDragonModel`.

use super::{LayerSpec, ModelId as M};
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, DEG_TO_RAD, PI, wrap_degrees};

const NECK: usize = 5;
const TAIL: usize = 12;

pub const TEXTURE: &str = "minecraft:entity/enderdragon/dragon";
pub const EYES_TEXTURE: &str = "minecraft:entity/enderdragon/dragon_eyes";
pub const EXPLODING_TEXTURE: &str = "minecraft:entity/enderdragon/dragon_exploding";

pub fn texture() -> &'static str {
    TEXTURE
}

pub fn eyes_texture() -> &'static str {
    EYES_TEXTURE
}

fn neck_name(i: usize) -> &'static str {
    ["neck0", "neck1", "neck2", "neck3", "neck4"][i]
}

fn tail_name(i: usize) -> &'static str {
    [
        "tail0", "tail1", "tail2", "tail3", "tail4", "tail5", "tail6", "tail7", "tail8", "tail9",
        "tail10", "tail11",
    ][i]
}

fn spine() -> Vec<Cube> {
    vec![
        Cube::new((192.0, 104.0), (-5.0, -5.0, -5.0), (10.0, 10.0, 10.0)),
        Cube::new((48.0, 0.0), (-1.0, -9.0, -3.0), (2.0, 4.0, 6.0)),
    ]
}

/// `EnderDragonModel.createBodyLayer`.
pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    {
        let head = root.add_or_replace_child(
            "head",
            vec![
                Cube::new((176.0, 44.0), (-6.0, -1.0, -24.0), (12.0, 5.0, 16.0)),
                Cube::new((112.0, 30.0), (-8.0, -8.0, -10.0), (16.0, 16.0, 16.0)),
                Cube::new((0.0, 0.0), (-5.0, -12.0, -4.0), (2.0, 4.0, 6.0)).mirrored(),
                Cube::new((112.0, 0.0), (-5.0, -3.0, -22.0), (2.0, 2.0, 4.0)).mirrored(),
                Cube::new((0.0, 0.0), (3.0, -12.0, -4.0), (2.0, 4.0, 6.0)),
                Cube::new((112.0, 0.0), (3.0, -3.0, -22.0), (2.0, 2.0, 4.0)),
            ],
            Pose::offset(0.0, 20.0, -62.0),
        );
        head.add_or_replace_child(
            "jaw",
            vec![Cube::new(
                (176.0, 65.0),
                (-6.0, 0.0, -16.0),
                (12.0, 4.0, 16.0),
            )],
            Pose::offset(0.0, 4.0, -8.0),
        );
    }
    for i in 0..NECK {
        root.add_or_replace_child(
            neck_name(i),
            spine(),
            Pose::offset(0.0, 20.0, -12.0 - i as f32 * 10.0),
        );
    }
    for i in 0..TAIL {
        root.add_or_replace_child(
            tail_name(i),
            spine(),
            Pose::offset(0.0, 10.0, 60.0 + i as f32 * 10.0),
        );
    }
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 0.0), (-12.0, -16.0, -16.0), (24.0, 24.0, 64.0)),
            Cube::new((220.0, 53.0), (-1.0, -22.0, -10.0), (2.0, 6.0, 12.0)),
            Cube::new((220.0, 53.0), (-1.0, -22.0, 10.0), (2.0, 6.0, 12.0)),
            Cube::new((220.0, 53.0), (-1.0, -22.0, 30.0), (2.0, 6.0, 12.0)),
        ],
        Pose::offset(0.0, 20.0, 8.0),
    );
    {
        let left_wing = body.add_or_replace_child(
            "left_wing",
            vec![
                Cube::new((112.0, 88.0), (0.0, -4.0, -4.0), (56.0, 8.0, 8.0)).mirrored(),
                Cube::new((-56.0, 88.0), (0.0, 0.0, 2.0), (56.0, 0.0, 56.0)).mirrored(),
            ],
            Pose::offset(12.0, -15.0, -6.0),
        );
        left_wing.add_or_replace_child(
            "left_wing_tip",
            vec![
                Cube::new((112.0, 136.0), (0.0, -2.0, -2.0), (56.0, 4.0, 4.0)).mirrored(),
                Cube::new((-56.0, 144.0), (0.0, 0.0, 2.0), (56.0, 0.0, 56.0)).mirrored(),
            ],
            Pose::offset(56.0, 0.0, 0.0),
        );
    }
    {
        let left_front_leg = body.add_or_replace_child(
            "left_front_leg",
            vec![Cube::new(
                (112.0, 104.0),
                (-4.0, -4.0, -4.0),
                (8.0, 24.0, 8.0),
            )],
            Pose::offset_rotation((12.0, 0.0, -6.0), (1.3, 0.0, 0.0)),
        );
        let left_front_leg_tip = left_front_leg.add_or_replace_child(
            "left_front_leg_tip",
            vec![Cube::new(
                (226.0, 138.0),
                (-3.0, -1.0, -3.0),
                (6.0, 24.0, 6.0),
            )],
            Pose::offset_rotation((0.0, 20.0, -1.0), (-0.5, 0.0, 0.0)),
        );
        left_front_leg_tip.add_or_replace_child(
            "left_front_foot",
            vec![Cube::new(
                (144.0, 104.0),
                (-4.0, 0.0, -12.0),
                (8.0, 4.0, 16.0),
            )],
            Pose::offset_rotation((0.0, 23.0, 0.0), (0.75, 0.0, 0.0)),
        );
    }
    {
        let left_hind_leg = body.add_or_replace_child(
            "left_hind_leg",
            vec![Cube::new(
                (0.0, 0.0),
                (-8.0, -4.0, -8.0),
                (16.0, 32.0, 16.0),
            )],
            Pose::offset_rotation((16.0, -4.0, 34.0), (1.0, 0.0, 0.0)),
        );
        let left_hind_leg_tip = left_hind_leg.add_or_replace_child(
            "left_hind_leg_tip",
            vec![Cube::new(
                (196.0, 0.0),
                (-6.0, -2.0, 0.0),
                (12.0, 32.0, 12.0),
            )],
            Pose::offset_rotation((0.0, 32.0, -4.0), (0.5, 0.0, 0.0)),
        );
        left_hind_leg_tip.add_or_replace_child(
            "left_hind_foot",
            vec![Cube::new(
                (112.0, 0.0),
                (-9.0, 0.0, -20.0),
                (18.0, 6.0, 24.0),
            )],
            Pose::offset_rotation((0.0, 31.0, 4.0), (0.75, 0.0, 0.0)),
        );
    }
    {
        let right_wing = body.add_or_replace_child(
            "right_wing",
            vec![
                Cube::new((112.0, 88.0), (-56.0, -4.0, -4.0), (56.0, 8.0, 8.0)),
                Cube::new((-56.0, 88.0), (-56.0, 0.0, 2.0), (56.0, 0.0, 56.0)),
            ],
            Pose::offset(-12.0, -15.0, -6.0),
        );
        right_wing.add_or_replace_child(
            "right_wing_tip",
            vec![
                Cube::new((112.0, 136.0), (-56.0, -2.0, -2.0), (56.0, 4.0, 4.0)),
                Cube::new((-56.0, 144.0), (-56.0, 0.0, 2.0), (56.0, 0.0, 56.0)),
            ],
            Pose::offset(-56.0, 0.0, 0.0),
        );
    }
    {
        let right_front_leg = body.add_or_replace_child(
            "right_front_leg",
            vec![Cube::new(
                (112.0, 104.0),
                (-4.0, -4.0, -4.0),
                (8.0, 24.0, 8.0),
            )],
            Pose::offset_rotation((-12.0, 0.0, -6.0), (1.3, 0.0, 0.0)),
        );
        let right_front_leg_tip = right_front_leg.add_or_replace_child(
            "right_front_leg_tip",
            vec![Cube::new(
                (226.0, 138.0),
                (-3.0, -1.0, -3.0),
                (6.0, 24.0, 6.0),
            )],
            Pose::offset_rotation((0.0, 20.0, -1.0), (-0.5, 0.0, 0.0)),
        );
        right_front_leg_tip.add_or_replace_child(
            "right_front_foot",
            vec![Cube::new(
                (144.0, 104.0),
                (-4.0, 0.0, -12.0),
                (8.0, 4.0, 16.0),
            )],
            Pose::offset_rotation((0.0, 23.0, 0.0), (0.75, 0.0, 0.0)),
        );
    }
    {
        let right_hind_leg = body.add_or_replace_child(
            "right_hind_leg",
            vec![Cube::new(
                (0.0, 0.0),
                (-8.0, -4.0, -8.0),
                (16.0, 32.0, 16.0),
            )],
            Pose::offset_rotation((-16.0, -4.0, 34.0), (1.0, 0.0, 0.0)),
        );
        let right_hind_leg_tip = right_hind_leg.add_or_replace_child(
            "right_hind_leg_tip",
            vec![Cube::new(
                (196.0, 0.0),
                (-6.0, -2.0, 0.0),
                (12.0, 32.0, 12.0),
            )],
            Pose::offset_rotation((0.0, 32.0, -4.0), (0.5, 0.0, 0.0)),
        );
        right_hind_leg_tip.add_or_replace_child(
            "right_hind_foot",
            vec![Cube::new(
                (112.0, 0.0),
                (-9.0, 0.0, -20.0),
                (18.0, 6.0, 24.0),
            )],
            Pose::offset_rotation((0.0, 31.0, 4.0), (0.75, 0.0, 0.0)),
        );
    }
    LayerDefinition::new(root, 256.0, 256.0)
}

#[derive(Clone, Copy, Debug)]
pub struct DragonParts {
    root: PartId,
    head: PartId,
    jaw: PartId,
    neck: [PartId; NECK],
    tail: [PartId; TAIL],
    body: PartId,
    left_wing: PartId,
    left_wing_tip: PartId,
    left_front_leg: PartId,
    left_front_leg_tip: PartId,
    left_front_foot: PartId,
    left_hind_leg: PartId,
    left_hind_leg_tip: PartId,
    left_hind_foot: PartId,
    right_wing: PartId,
    right_wing_tip: PartId,
    right_front_leg: PartId,
    right_front_leg_tip: PartId,
    right_front_foot: PartId,
    right_hind_leg: PartId,
    right_hind_leg_tip: PartId,
    right_hind_foot: PartId,
}

impl DragonParts {
    pub fn find(model: &Model) -> Self {
        Self {
            root: model.root(),
            head: model.part("head"),
            jaw: model.part("jaw"),
            neck: std::array::from_fn(|i| model.part(neck_name(i))),
            tail: std::array::from_fn(|i| model.part(tail_name(i))),
            body: model.part("body"),
            left_wing: model.part("left_wing"),
            left_wing_tip: model.part("left_wing_tip"),
            left_front_leg: model.part("left_front_leg"),
            left_front_leg_tip: model.part("left_front_leg_tip"),
            left_front_foot: model.part("left_front_foot"),
            left_hind_leg: model.part("left_hind_leg"),
            left_hind_leg_tip: model.part("left_hind_leg_tip"),
            left_hind_foot: model.part("left_hind_foot"),
            right_wing: model.part("right_wing"),
            right_wing_tip: model.part("right_wing_tip"),
            right_front_leg: model.part("right_front_leg"),
            right_front_leg_tip: model.part("right_front_leg_tip"),
            right_front_foot: model.part("right_front_foot"),
            right_hind_leg: model.part("right_hind_leg"),
            right_hind_leg_tip: model.part("right_hind_leg_tip"),
            right_hind_foot: model.part("right_hind_foot"),
        }
    }
}

/// `flapTime` from `EnderDragonRenderState` (0..1 around a wingbeat).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DragonPose {
    pub flap_time: f32,
}

/// `EnderDragonModel.posesLimbs`: each leg `[leg, tip, foot]`.
fn pose_limbs(
    pose: &mut ModelPose,
    bounce: f32,
    [front, front_tip, front_foot]: [PartId; 3],
    [rear, rear_tip, rear_foot]: [PartId; 3],
) {
    pose[rear].pose.rotation.x = 1.0 + bounce * 0.1;
    pose[rear_tip].pose.rotation.x = 0.5 + bounce * 0.1;
    pose[rear_foot].pose.rotation.x = 0.75 + bounce * 0.1;
    pose[front].pose.rotation.x = 1.3 + bounce * 0.1;
    pose[front_tip].pose.rotation.x = -0.5 - bounce * 0.1;
    pose[front_foot].pose.rotation.x = 0.75 + bounce * 0.1;
}

/// `EnderDragonModel.setupAnim` for idle flight.
pub fn setup_anim(parts: &DragonParts, pose: &mut ModelPose, dragon: DragonPose) {
    let flap = dragon.flap_time * (PI * 2.0);
    pose[parts.jaw].pose.rotation.x = (angle::sin(flap) + 1.0) * 0.2;
    let mut bounce = angle::sin(flap - 1.0) + 1.0;
    bounce = (bounce * bounce + bounce * 2.0) * 0.05;
    {
        let root = &mut pose[parts.root].pose;
        root.offset.y = (bounce - 2.0) * 16.0;
        root.offset.z = -48.0;
        root.rotation.x = bounce * 2.0 * (DEG_TO_RAD);
    }
    let mut xx = pose[parts.neck[0]].pose.offset.x;
    let mut yy = pose[parts.neck[0]].pose.offset.y;
    let mut zz = pose[parts.neck[0]].pose.offset.z;
    let rot2 = wrap_degrees(0.0);
    let rot = wrap_degrees(rot2 / 2.0);
    for i in 0..NECK {
        let neck_x_rot = angle::cos(i as f32 * 0.45 + flap) * 0.15;
        let neck = &mut pose[parts.neck[i]].pose;
        neck.rotation.y = wrap_degrees(0.0) * (DEG_TO_RAD) * 1.5;
        neck.rotation.x = neck_x_rot + 0.0;
        neck.rotation.z = -wrap_degrees(0.0 - rot) * (DEG_TO_RAD) * 1.5;
        neck.offset.y = yy;
        neck.offset.z = zz;
        neck.offset.x = xx;
        let (sy, cy) = (angle::sin(neck.rotation.y), angle::cos(neck.rotation.y));
        let (sx, cx) = (angle::sin(neck.rotation.x), angle::cos(neck.rotation.x));
        xx -= sy * cx * 10.0;
        yy += sx * 10.0;
        zz -= cy * cx * 10.0;
    }
    {
        let head = &mut pose[parts.head].pose;
        head.offset.y = yy;
        head.offset.z = zz;
        head.offset.x = xx;
        head.rotation.y = wrap_degrees(0.0) * (DEG_TO_RAD);
        head.rotation.x = wrap_degrees(0.0) * (DEG_TO_RAD) * 1.5 * 5.0;
        head.rotation.z = -wrap_degrees(0.0 - rot) * (DEG_TO_RAD);
    }
    pose[parts.body].pose.rotation.z = -rot2 * 1.5 * (DEG_TO_RAD);
    {
        let wing = &mut pose[parts.left_wing].pose;
        wing.rotation.x = 0.125 - angle::cos(flap) * 0.2;
        wing.rotation.y = -0.25;
        wing.rotation.z = -(angle::sin(flap) + 0.125) * 0.8;
    }
    pose[parts.left_wing_tip].pose.rotation.z = (angle::sin(flap + 2.0) + 0.5) * 0.75;
    pose[parts.right_wing].pose.rotation.x = pose[parts.left_wing].pose.rotation.x;
    pose[parts.right_wing].pose.rotation.y = -pose[parts.left_wing].pose.rotation.y;
    pose[parts.right_wing].pose.rotation.z = -pose[parts.left_wing].pose.rotation.z;
    pose[parts.right_wing_tip].pose.rotation.z = -pose[parts.left_wing_tip].pose.rotation.z;
    pose_limbs(
        pose,
        bounce,
        [
            parts.left_front_leg,
            parts.left_front_leg_tip,
            parts.left_front_foot,
        ],
        [
            parts.left_hind_leg,
            parts.left_hind_leg_tip,
            parts.left_hind_foot,
        ],
    );
    pose_limbs(
        pose,
        bounce,
        [
            parts.right_front_leg,
            parts.right_front_leg_tip,
            parts.right_front_foot,
        ],
        [
            parts.right_hind_leg,
            parts.right_hind_leg_tip,
            parts.right_hind_foot,
        ],
    );
    let mut tail_x_rot = 0.0;
    yy = pose[parts.tail[0]].pose.offset.y;
    zz = pose[parts.tail[0]].pose.offset.z;
    xx = pose[parts.tail[0]].pose.offset.x;
    for i in 0..TAIL {
        tail_x_rot += angle::sin(i as f32 * 0.45 + flap) * 0.05;
        let tail = &mut pose[parts.tail[i]].pose;
        tail.rotation.y = (wrap_degrees(0.0) * 1.5 + 180.0) * (DEG_TO_RAD);
        tail.rotation.x = tail_x_rot;
        tail.rotation.z = wrap_degrees(0.0 - rot) * (DEG_TO_RAD) * 1.5;
        tail.offset.y = yy;
        tail.offset.z = zz;
        tail.offset.x = xx;
        let (sy, cy) = (angle::sin(tail.rotation.y), angle::cos(tail.rotation.y));
        let (sx, cx) = (angle::sin(tail.rotation.x), angle::cos(tail.rotation.x));
        yy += sx * 10.0;
        zz -= cy * cx * 10.0;
        xx -= sy * cx * 10.0;
    }
}

/// `EnderDragonRenderer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Dragon, TEXTURE));
    out.push(plain(M::Dragon, EYES_TEXTURE).glowing());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flapping_turns_the_wings_and_the_jaw() {
        let model = Model::bake(&layer());
        let parts = DragonParts::find(&model);
        let at = |flap_time: f32| {
            let mut pose = model.initial_pose();
            setup_anim(&parts, &mut pose, DragonPose { flap_time });
            pose
        };
        let (a, b) = (at(0.0), at(0.25));
        assert_ne!(
            a[parts.left_wing].pose.rotation.z,
            b[parts.left_wing].pose.rotation.z
        );
        assert_ne!(a[parts.jaw].pose.rotation.x, b[parts.jaw].pose.rotation.x);
        assert_ne!(
            a[parts.neck[2]].pose.rotation.x,
            b[parts.neck[2]].pose.rotation.x
        );
        assert_eq!(texture(), TEXTURE);
        assert_eq!(eyes_texture(), EYES_TEXTURE);
    }
}
