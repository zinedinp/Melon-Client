//! `AdultFoxModel` and `BabyFoxModel`: a quadruped that sits, sleeps,

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::animations::fox_baby::FOX_BABY_WALK;
use crate::world::entity::keyframe::AnimationDefinition;
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::Legs;
use crate::world::model_part::{Cube, Pose};

/// `Fox.DATA_FLAGS_ID` bits.
pub const FLAG_SITTING: u8 = 1;
pub const FLAG_CROUCHING: u8 = 4;
pub const FLAG_INTERESTED: u8 = 8;
pub const FLAG_POUNCING: u8 = 16;
pub const FLAG_SLEEPING: u8 = 32;
pub const FLAG_FACEPLANTED: u8 = 64;

/// The kit's walk, driven by the walk cycle.
pub static WALK_ANIMATION: &AnimationDefinition = &FOX_BABY_WALK;

/// `Fox.Variant`: red (0) and snow (1), idle and sleeping, adult and kit.
pub fn texture(snow: bool, sleeping: bool, baby: bool) -> &'static str {
    match (snow, sleeping, baby) {
        (false, false, false) => "minecraft:entity/fox/fox",
        (false, false, true) => "minecraft:entity/fox/fox_baby",
        (false, true, false) => "minecraft:entity/fox/fox_sleep",
        (false, true, true) => "minecraft:entity/fox/fox_sleep_baby",
        (true, false, false) => "minecraft:entity/fox/fox_snow",
        (true, false, true) => "minecraft:entity/fox/fox_snow_baby",
        (true, true, false) => "minecraft:entity/fox/fox_snow_sleep",
        (true, true, true) => "minecraft:entity/fox/fox_snow_sleep_baby",
    }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((1.0, 5.0), (-3.0, -2.0, -5.0), (8.0, 6.0, 6.0))],
        Pose::offset(-1.0, 16.5, -3.0),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((8.0, 1.0), (-3.0, -4.0, -4.0), (2.0, 2.0, 1.0))],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((15.0, 1.0), (3.0, -4.0, -4.0), (2.0, 2.0, 1.0))],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((6.0, 18.0), (-1.0, 2.01, -8.0), (4.0, 2.0, 3.0))],
        Pose::ZERO,
    );
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (24.0, 15.0),
            (-3.0, 3.999, -3.5),
            (6.0, 11.0, 6.0),
        )],
        Pose::offset_rotation((0.0, 16.0, -6.0), (PI / 2.0, 0.0, 0.0)),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((30.0, 0.0), (2.0, 0.0, -1.0), (4.0, 9.0, 5.0))],
        Pose::offset_rotation((-4.0, 15.0, -1.0), (-0.052_359_88, 0.0, 0.0)),
    );
    let fudge = 0.001;
    let left = || vec![Cube::new((4.0, 24.0), (2.0, 0.5, -1.0), (2.0, 6.0, 2.0)).grown(fudge)];
    let right = || vec![Cube::new((13.0, 24.0), (2.0, 0.5, -1.0), (2.0, 6.0, 2.0)).grown(fudge)];
    root.add_or_replace_child("right_hind_leg", right(), Pose::offset(-5.0, 17.5, 7.0));
    root.add_or_replace_child("left_hind_leg", left(), Pose::offset(-1.0, 17.5, 7.0));
    root.add_or_replace_child("right_front_leg", right(), Pose::offset(-5.0, 17.5, 0.0));
    root.add_or_replace_child("left_front_leg", left(), Pose::offset(-1.0, 17.5, 0.0));
    LayerDefinition::new(root, 48.0, 32.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-3.0, -2.125, -5.125), (6.0, 5.0, 5.0)),
            Cube::new((18.0, 20.0), (-1.0, 0.875, -7.125), (2.0, 2.0, 2.0)),
            Cube::new((22.0, 8.0), (-3.0, -4.125, -4.125), (2.0, 2.0, 1.0)),
            Cube::new((22.0, 11.0), (1.0, -4.125, -4.125), (2.0, 2.0, 1.0)),
        ],
        Pose::offset(0.0, 18.125, 0.125),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((22.0, 4.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
        Pose::offset(-1.5, 22.0, 4.0),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((22.0, 0.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
        Pose::offset(1.5, 22.0, 4.0),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((22.0, 4.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
        Pose::offset(-1.5, 22.0, 0.0),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((22.0, 0.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
        Pose::offset(1.5, 22.0, 0.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 10.0), (-2.5, -2.0, -3.0), (5.0, 4.0, 6.0))],
        Pose::offset(0.0, 20.0, 2.0),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((0.0, 20.0), (-1.5, -1.48, -1.0), (3.0, 3.0, 6.0))],
        Pose::offset(0.0, -0.5, 3.0),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct FoxParts {
    pub head: PartId,
    body: PartId,
    tail: PartId,
    legs: Legs,
    baby: bool,
}

impl FoxParts {
    pub fn find(model: &Model, baby: bool) -> Self {
        Self {
            head: model.part("head"),
            body: model.part("body"),
            tail: model.part("tail"),
            legs: Legs::find(model),
            baby,
        }
    }
}

/// What the pose adds to the walk.
#[derive(Clone, Copy, Debug, Default)]
pub struct FoxPose {
    pub sitting: bool,
    pub sleeping: bool,
    pub crouching: bool,
    pub pouncing: bool,
    pub faceplanted: bool,
    /// `Fox.getCrouchAmount`, pixels.
    pub crouch_amount: f32,
    /// `Fox.getHeadRollAngle`, radians.
    pub head_roll: f32,
}

/// The head's posed offset in blocks, for the item in its mouth.
pub fn head_blocks(parts: &FoxParts, pose: &ModelPose) -> glam::Vec3 {
    pose[parts.head].pose.offset / 16.0
}

/// `FoxModel.setupAnim`.
pub fn setup_anim(
    parts: &FoxParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    fox: FoxPose,
    walk: Option<&crate::world::entity::keyframe::KeyframeAnimation>,
) {
    pose[parts.head].pose.rotation.z = fox.head_roll;
    for leg in [
        parts.legs.right_hind,
        parts.legs.left_hind,
        parts.legs.right_front,
        parts.legs.left_front,
    ] {
        pose[leg].visible = true;
    }
    if parts.baby {
        if let Some(walk) = walk {
            walk.apply_walk(pose, living.walk_position, living.walk_speed, 1.0, 2.5);
        }
    } else {
        let pos = living.walk_position;
        let speed = living.walk_speed;
        pose[parts.legs.right_hind].pose.rotation.x = angle::cos(pos * 0.6662) * 1.4 * speed;
        pose[parts.legs.left_hind].pose.rotation.x = angle::cos(pos * 0.6662 + PI) * 1.4 * speed;
        pose[parts.legs.right_front].pose.rotation.x = angle::cos(pos * 0.6662 + PI) * 1.4 * speed;
        pose[parts.legs.left_front].pose.rotation.x = angle::cos(pos * 0.6662) * 1.4 * speed;
    }

    if fox.crouching {
        pose[parts.body].pose.rotation.x += 0.104_719_76;
        pose[parts.head].pose.offset.y += fox.crouch_amount * if living.baby { 0.5 } else { 1.0 };
        let wiggle = angle::cos(living.age_in_ticks) * 0.05;
        pose[parts.body].pose.rotation.y = wiggle;
        pose[parts.legs.right_hind].pose.rotation.z = wiggle;
        pose[parts.legs.left_hind].pose.rotation.z = wiggle;
        pose[parts.legs.right_front].pose.rotation.z = wiggle / 2.0;
        pose[parts.legs.left_front].pose.rotation.z = wiggle / 2.0;
        if parts.baby {
            pose[parts.body].pose.offset.y += fox.crouch_amount / 6.0;
        } else {
            pose[parts.body].pose.offset.y += fox.crouch_amount;
        }
    } else if fox.sleeping {
        sleep(parts, pose);
    } else if fox.sitting {
        sit(parts, pose);
    }

    if fox.pouncing {
        let crouch = fox.crouch_amount / 2.0;
        pose[parts.body].pose.offset.y -= crouch;
        pose[parts.head].pose.offset.y -= crouch;
    }

    if !fox.sleeping && !fox.faceplanted && !fox.crouching {
        pose[parts.head].pose.rotation.x = living.x_rot.to_radians();
        pose[parts.head].pose.rotation.y = living.y_rot.to_radians();
    }
    if fox.sleeping {
        pose[parts.head].pose.rotation.x = 0.0;
        pose[parts.head].pose.rotation.y = -PI * 2.0 / 3.0;
        pose[parts.head].pose.rotation.z = angle::cos(living.age_in_ticks * 0.027) / 22.0;
    }
    if fox.faceplanted {
        let t = living.age_in_ticks * 0.67;
        pose[parts.legs.right_hind].pose.rotation.x = angle::cos(t * 0.4662) * 0.1;
        pose[parts.legs.left_hind].pose.rotation.x = angle::cos(t * 0.4662 + PI) * 0.1;
        pose[parts.legs.right_front].pose.rotation.x = angle::cos(t * 0.4662 + PI) * 0.1;
        pose[parts.legs.left_front].pose.rotation.x = angle::cos(t * 0.4662) * 0.1;
    }
}

fn sit(parts: &FoxParts, pose: &mut ModelPose) {
    pose[parts.head].pose.rotation.x = 0.0;
    pose[parts.head].pose.rotation.y = 0.0;
    if parts.baby {
        pose[parts.body].pose.rotation.x = -0.959_931;
        pose[parts.body].pose.offset.z -= 4.5 * 0.5;
        pose[parts.body].pose.offset.y += 3.0 * 0.5;
        pose[parts.tail].pose.offset.y -= 0.6;
        pose[parts.tail].pose.offset.z -= 2.0 * 0.5;
        pose[parts.tail].pose.rotation.x = 0.959_931_14;
        pose[parts.head].pose.offset.y -= 0.75;
        pose[parts.legs.right_front].pose.rotation.x = -PI / 12.0;
        pose[parts.legs.left_front].pose.rotation.x = -PI / 12.0;
        pose[parts.legs.right_front].pose.offset.z -= 1.5;
        pose[parts.legs.left_front].pose.offset.z -= 1.5;
        pose[parts.legs.right_front].pose.offset.x += 0.01;
        pose[parts.legs.left_front].pose.offset.x -= 0.01;
        pose[parts.legs.right_hind].pose.offset.z -= 3.75;
        pose[parts.legs.left_hind].pose.offset.z -= 3.75;
        pose[parts.legs.right_hind].pose.offset.x += 0.01;
        pose[parts.legs.left_hind].pose.offset.x -= 0.01;
    } else {
        pose[parts.body].pose.rotation.x = PI / 6.0;
        pose[parts.body].pose.offset.y -= 7.0;
        pose[parts.body].pose.offset.z += 3.0;
        pose[parts.tail].pose.rotation.x = PI / 4.0;
        pose[parts.head].pose.offset.y -= 6.5;
        pose[parts.head].pose.offset.z += 2.75;
        pose[parts.legs.right_front].pose.rotation.x = -PI / 12.0;
        pose[parts.legs.left_front].pose.rotation.x = -PI / 12.0;
        pose[parts.legs.right_hind].pose.rotation.x = -PI * 5.0 / 12.0;
        pose[parts.legs.right_hind].pose.offset.y += 4.0;
        pose[parts.legs.right_hind].pose.offset.z -= 0.25;
        pose[parts.legs.left_hind].pose.rotation.x = -PI * 5.0 / 12.0;
        pose[parts.legs.left_hind].pose.offset.y += 4.0;
        pose[parts.legs.left_hind].pose.offset.z -= 0.25;
        pose[parts.tail].pose.offset.z -= 1.0;
    }
}

fn sleep(parts: &FoxParts, pose: &mut ModelPose) {
    for leg in [
        parts.legs.right_hind,
        parts.legs.left_hind,
        parts.legs.right_front,
        parts.legs.left_front,
    ] {
        pose[leg].visible = false;
    }
    if parts.baby {
        pose[parts.body].pose.rotation.z = -PI / 2.0;
        pose[parts.body].pose.rotation.x = -PI / 18.0;
        pose[parts.body].pose.offset.y += 1.5;
        pose[parts.body].pose.offset.z -= 1.5;
        pose[parts.body].pose.offset.x -= 1.5;
        pose[parts.tail].pose.rotation.x = -2.181_661_6;
        pose[parts.tail].pose.offset.x -= 0.7;
        pose[parts.tail].pose.offset.z += 0.6;
        pose[parts.tail].pose.offset.y += 0.9;
        pose[parts.head].pose.offset.x -= 2.0;
        pose[parts.head].pose.offset.y += 2.8;
        pose[parts.head].pose.offset.z -= 4.0;
        pose[parts.head].pose.rotation.y = -PI * 2.0 / 3.0;
        pose[parts.head].pose.rotation.z = 0.0;
    } else {
        pose[parts.body].pose.rotation.z = -PI / 2.0;
        pose[parts.body].pose.offset.y += 5.0;
        pose[parts.tail].pose.rotation.x = -PI * 5.0 / 6.0;
        pose[parts.head].pose.offset.x += 2.0;
        pose[parts.head].pose.offset.y += 2.99;
        pose[parts.head].pose.rotation.y = -PI * 2.0 / 3.0;
        pose[parts.head].pose.rotation.z = 0.0;
    }
}

/// `FoxRenderer`: red or snow, awake or asleep.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    out.push(plain(
        if baby { M::FoxBaby } else { M::Fox },
        texture(a.fox_variant != 0, a.fox_flags & FLAG_SLEEPING != 0, baby),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::keyframe::KeyframeAnimation;

    #[test]
    fn the_kit_walk_bakes_and_the_adult_stands_on_the_ground() {
        let adult = Model::bake(&adult());
        let kit = Model::bake(&baby());
        assert!(KeyframeAnimation::bake(&kit, WALK_ANIMATION).is_ok());
        let parts = FoxParts::find(&adult, false);
        let mut pose = adult.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState {
                walk_position: 1.0,
                walk_speed: 1.0,
                ..Default::default()
            },
            FoxPose::default(),
            None,
        );
        assert!(pose[parts.legs.right_hind].pose.rotation.x.abs() > 0.1);
    }
}
