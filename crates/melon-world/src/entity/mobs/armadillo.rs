//! `AdultArmadilloModel` and `BabyArmadilloModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::animations::{armadillo as adult_anim, baby_armadillo as baby_anim};
use crate::entity::keyframe::{AnimationDefinition, AnimationState, KeyframeAnimation};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::math::angle::{EIGHTH_PI, THIRD_PI};
use crate::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const ROLL_OUT: usize = 0;
pub const ROLL_UP: usize = 1;
pub const PEEK: usize = 2;

/// `Armadillo.ArmadilloState` ids.
pub const STATE_IDLE: i32 = 0;
pub const STATE_ROLLING: i32 = 1;
pub const STATE_SCARED: i32 = 2;
pub const STATE_UNROLLING: i32 = 3;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 3] = [
    (&adult_anim::ARMADILLO_ROLL_OUT, ROLL_OUT),
    (&adult_anim::ARMADILLO_ROLL_UP, ROLL_UP),
    (&adult_anim::ARMADILLO_PEEK, PEEK),
];
pub static BABY_ANIMATIONS: [(&AnimationDefinition, usize); 3] = [
    (&baby_anim::ARMADILLO_BABY_ROLL_OUT, ROLL_OUT),
    (&baby_anim::ARMADILLO_BABY_ROLL_UP, ROLL_UP),
    (&baby_anim::ARMADILLO_BABY_PEEK, PEEK),
];
pub static WALK: &AnimationDefinition = &adult_anim::ARMADILLO_WALK;
pub static BABY_WALK: &AnimationDefinition = &baby_anim::ARMADILLO_BABY_WALK;

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 20.0), (-4.0, -7.0, -10.0), (8.0, 8.0, 12.0)).grown(0.3),
            Cube::new((0.0, 40.0), (-4.0, -7.0, -10.0), (8.0, 8.0, 12.0)),
        ],
        Pose::offset(0.0, 21.0, 4.0),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new(
            (44.0, 53.0),
            (-0.5, -0.0865, 0.0933),
            (1.0, 6.0, 1.0),
        )],
        Pose::offset_rotation((0.0, -3.0, 1.0), (0.5061, 0.0, 0.0)),
    );
    let head = body.add_or_replace_child("head", vec![], Pose::offset(0.0, -2.0, -11.0));
    head.add_or_replace_child(
        "head_cube",
        vec![Cube::new((43.0, 15.0), (-1.5, -1.0, -1.0), (3.0, 5.0, 2.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-EIGHTH_PI, 0.0, 0.0)),
    );
    let right_ear = head.add_or_replace_child("right_ear", vec![], Pose::offset(-1.0, -1.0, 0.0));
    right_ear.add_or_replace_child(
        "right_ear_cube",
        vec![Cube::new((43.0, 10.0), (-2.0, -3.0, 0.0), (2.0, 5.0, 0.0))],
        Pose::offset_rotation((-0.5, 0.0, -0.6), (0.1886, -0.3864, -0.0718)),
    );
    let left_ear = head.add_or_replace_child("left_ear", vec![], Pose::offset(1.0, -2.0, 0.0));
    left_ear.add_or_replace_child(
        "left_ear_cube",
        vec![Cube::new((47.0, 10.0), (0.0, -3.0, 0.0), (2.0, 5.0, 0.0))],
        Pose::offset_rotation((0.5, 1.0, -0.6), (0.1886, 0.3864, 0.0718)),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((51.0, 31.0), (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
        Pose::offset(-2.0, 21.0, 4.0),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((42.0, 31.0), (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
        Pose::offset(2.0, 21.0, 4.0),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((51.0, 43.0), (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
        Pose::offset(-2.0, 21.0, -4.0),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((42.0, 43.0), (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
        Pose::offset(2.0, 21.0, -4.0),
    );
    root.add_or_replace_child(
        "cube",
        vec![Cube::new(
            (0.0, 0.0),
            (-5.0, -10.0, -6.0),
            (10.0, 10.0, 10.0),
        )],
        Pose::offset(0.0, 24.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 0.0), (-2.5, -2.0, -3.5), (5.0, 4.0, 7.0)).grown(0.3),
            Cube::new((0.0, 11.0), (-2.5, -2.0, -3.0), (5.0, 4.0, 6.0)),
        ],
        Pose::offset(0.0, 20.0, 0.5),
    );
    let tail = body.add_or_replace_child("tail", vec![], Pose::offset(0.0, 0.0, 3.4));
    tail.add_or_replace_child(
        "right_ear_cube",
        vec![Cube::new((22.0, 11.0), (-0.5, -0.5, -2.0), (1.0, 1.0, 4.0))],
        Pose::offset_rotation((0.0, 1.5, 1.0), (-THIRD_PI, 0.0, 0.0)),
    );
    let head = body.add_or_replace_child("head", vec![], Pose::offset(0.0, 0.0, -3.2));
    let head_cube = head.add_or_replace_child(
        "head_cube",
        vec![Cube::new((20.0, 17.0), (-1.0, -2.0, -4.0), (2.0, 2.0, 4.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (0.741_764_9, 0.0, 0.0)),
    );
    head_cube.add_or_replace_child(
        "right_ear",
        vec![Cube::new((28.0, 8.0), (-1.8, -2.0, 0.0), (2.0, 3.0, 0.0)).mirrored()],
        Pose::offset_rotation((-1.0, -2.0, -0.3), (-0.4363, -0.1134, 0.0524)),
    );
    head_cube.add_or_replace_child(
        "left_ear",
        vec![Cube::new((28.0, 8.0), (-0.2, -2.0, 0.0), (2.0, 3.0, 0.0))],
        Pose::offset_rotation((1.0, -2.0, -0.3), (-0.4363, 0.1134, -0.0524)),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((20.0, 27.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0)).mirrored()],
        Pose::offset(-1.5, 22.0, 2.5),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((20.0, 27.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
        Pose::offset(1.5, 22.0, 2.5),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((20.0, 23.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0))],
        Pose::offset(1.5, 22.0, -1.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((24.0, 0.0), (-1.0, 0.0, -1.0), (2.0, 2.0, 2.0)).mirrored()],
        Pose::offset(-1.5, 22.0, -1.5),
    );
    root.add_or_replace_child(
        "cube",
        vec![Cube::new((0.0, 25.0), (-3.0, -3.0, -3.0), (6.0, 6.0, 6.0)).grown(0.3)],
        Pose::offset(0.0, 20.7, 0.5),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct ArmadilloParts {
    body: PartId,
    head: PartId,
    tail: PartId,
    cube: PartId,
    right_hind: PartId,
    left_hind: PartId,
}

impl ArmadilloParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
            head: model.part("head"),
            tail: model.part("tail"),
            cube: model.part("cube"),
            right_hind: model.part("right_hind_leg"),
            left_hind: model.part("left_hind_leg"),
        }
    }
}

/// `Armadillo.shouldHideInShell` from the state and how long it has been in it.
pub fn hide_in_shell(state: i32, ticks: u32) -> bool {
    match state {
        STATE_ROLLING => ticks > 5,
        STATE_SCARED => true,
        STATE_UNROLLING => ticks < 26,
        _ => false,
    }
}

/// `ArmadilloModel.setupAnim`.
pub fn setup_anim(
    parts: &ArmadilloParts,
    walk: &KeyframeAnimation,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    hiding: bool,
) {
    if hiding {
        pose[parts.body].skip_draw = true;
        pose[parts.left_hind].visible = false;
        pose[parts.right_hind].visible = false;
        pose[parts.tail].visible = false;
        pose[parts.cube].visible = true;
    } else {
        pose[parts.body].skip_draw = false;
        pose[parts.left_hind].visible = true;
        pose[parts.right_hind].visible = true;
        pose[parts.tail].visible = true;
        pose[parts.cube].visible = false;
        pose[parts.head].pose.rotation.x = living.x_rot.clamp(-22.5, 25.0).to_radians();
        pose[parts.head].pose.rotation.y = living.y_rot.clamp(-32.5, 32.5).to_radians();
        walk.apply_walk(pose, living.walk_position, living.walk_speed, 16.5, 2.5);
    }
}

/// Start the roll/peek clocks from the synced state.
pub fn tick_anim(anim: &mut [AnimationState], tick: i32, state: i32, ticks_in_state: u32) {
    match state {
        STATE_IDLE => {
            anim[ROLL_OUT].stop();
            anim[ROLL_UP].stop();
            anim[PEEK].stop();
        }
        STATE_ROLLING => {
            anim[ROLL_OUT].stop();
            anim[ROLL_UP].start_if_stopped(tick);
            anim[PEEK].stop();
        }
        STATE_SCARED => {
            anim[ROLL_OUT].stop();
            anim[ROLL_UP].stop();
            if ticks_in_state == 0 {
                anim[PEEK].start(tick);
            } else {
                anim[PEEK].start_if_stopped(tick);
            }
        }
        STATE_UNROLLING => {
            anim[ROLL_OUT].start_if_stopped(tick);
            anim[ROLL_UP].stop();
            anim[PEEK].stop();
        }
        _ => {}
    }
}

/// `ArmadilloRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::Armadillo, "minecraft:entity/armadillo/armadillo"),
        (
            M::ArmadilloBaby,
            "minecraft:entity/armadillo/armadillo_baby",
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_and_the_shell_hides_the_body() {
        for (layer, animations, walk_def) in [
            (adult(), &ANIMATIONS[..], WALK),
            (baby(), &BABY_ANIMATIONS[..], BABY_WALK),
        ] {
            let model = Model::bake(&layer);
            for (definition, _) in animations {
                assert!(KeyframeAnimation::bake(&model, definition).is_ok());
            }
            let walk = KeyframeAnimation::bake(&model, walk_def).unwrap();
            let parts = ArmadilloParts::find(&model);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &walk,
                &mut pose,
                &LivingRenderState::default(),
                true,
            );
            assert!(pose[parts.body].skip_draw);
            assert!(pose[parts.cube].visible);
            assert!(!pose[parts.left_hind].visible);
        }
        assert!(hide_in_shell(STATE_SCARED, 0));
        assert!(!hide_in_shell(STATE_ROLLING, 3));
        assert!(hide_in_shell(STATE_ROLLING, 6));
    }
}
