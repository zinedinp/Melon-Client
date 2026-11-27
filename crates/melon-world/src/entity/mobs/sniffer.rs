//! `SnifferModel` and `SniffletModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::world::entity::animations::sniffer as anim;
use crate::world::entity::keyframe::{AnimationDefinition, AnimationState, KeyframeAnimation};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::look;
use crate::world::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const HAPPY: usize = 0;
pub const SCENTING: usize = 1;
pub const SNIFFING: usize = 2;
pub const SEARCHING: usize = 3;
pub const DIG: usize = 4;
pub const STAND_UP: usize = 5;

/// `Sniffer.State` ids (`DATA_STATE`).
pub const STATE_IDLING: i32 = 0;
pub const STATE_FEELING_HAPPY: i32 = 1;
pub const STATE_SCENTING: i32 = 2;
pub const STATE_SNIFFING: i32 = 3;
pub const STATE_SEARCHING: i32 = 4;
pub const STATE_DIGGING: i32 = 5;
pub const STATE_RISING: i32 = 6;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 6] = [
    (&anim::SNIFFER_HAPPY, HAPPY),
    (&anim::SNIFFER_SNIFFSNIFF, SCENTING),
    (&anim::SNIFFER_LONGSNIFF, SNIFFING),
    (&anim::SNIFFER_SNIFF_SEARCH, SEARCHING),
    (&anim::SNIFFER_DIG, DIG),
    (&anim::SNIFFER_STAND_UP, STAND_UP),
];

pub static WALK: &AnimationDefinition = &anim::SNIFFER_WALK;
pub static SEARCH: &AnimationDefinition = &anim::SNIFFER_SNIFF_SEARCH;
pub static BABY_TRANSFORM: &AnimationDefinition = &anim::BABY_TRANSFORM;

pub fn texture(baby: bool) -> &'static str {
    if baby {
        "minecraft:entity/sniffer/snifflet"
    } else {
        "minecraft:entity/sniffer/sniffer"
    }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let bone = root.add_or_replace_child("bone", vec![], Pose::offset(0.0, 5.0, 0.0));
    {
        let body = bone.add_or_replace_child(
            "body",
            vec![
                Cube::new((62.0, 68.0), (-12.5, -14.0, -20.0), (25.0, 29.0, 40.0)),
                Cube::new((62.0, 0.0), (-12.5, -14.0, -20.0), (25.0, 24.0, 40.0)).grown(0.5),
                Cube::new((87.0, 68.0), (-12.5, 12.0, -20.0), (25.0, 0.0, 40.0)),
            ],
            Pose::offset(0.0, 0.0, 0.0),
        );
        let head = body.add_or_replace_child(
            "head",
            vec![
                Cube::new((8.0, 15.0), (-6.5, -7.5, -11.5), (13.0, 18.0, 11.0)),
                Cube::new((8.0, 4.0), (-6.5, 7.5, -11.5), (13.0, 0.0, 11.0)),
            ],
            Pose::offset(0.0, 6.5, -19.48),
        );
        head.add_or_replace_child(
            "left_ear",
            vec![Cube::new((2.0, 0.0), (0.0, 0.0, -3.0), (1.0, 19.0, 7.0))],
            Pose::offset(6.51, -7.5, -4.51),
        );
        head.add_or_replace_child(
            "right_ear",
            vec![Cube::new((48.0, 0.0), (-1.0, 0.0, -3.0), (1.0, 19.0, 7.0))],
            Pose::offset(-6.51, -7.5, -4.51),
        );
        head.add_or_replace_child(
            "nose",
            vec![Cube::new(
                (10.0, 45.0),
                (-6.5, -2.0, -9.0),
                (13.0, 2.0, 9.0),
            )],
            Pose::offset(0.0, -4.5, -11.5),
        );
        head.add_or_replace_child(
            "lower_beak",
            vec![Cube::new(
                (10.0, 57.0),
                (-6.5, -7.0, -8.0),
                (13.0, 12.0, 9.0),
            )],
            Pose::offset(0.0, 2.5, -12.5),
        );
    }
    bone.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new(
            (32.0, 87.0),
            (-3.5, -1.0, -4.0),
            (7.0, 10.0, 8.0),
        )],
        Pose::offset(-7.5, 10.0, -15.0),
    );
    bone.add_or_replace_child(
        "right_mid_leg",
        vec![Cube::new(
            (32.0, 105.0),
            (-3.5, -1.0, -4.0),
            (7.0, 10.0, 8.0),
        )],
        Pose::offset(-7.5, 10.0, 0.0),
    );
    bone.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new(
            (32.0, 123.0),
            (-3.5, -1.0, -4.0),
            (7.0, 10.0, 8.0),
        )],
        Pose::offset(-7.5, 10.0, 15.0),
    );
    bone.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((0.0, 87.0), (-3.5, -1.0, -4.0), (7.0, 10.0, 8.0))],
        Pose::offset(7.5, 10.0, -15.0),
    );
    bone.add_or_replace_child(
        "left_mid_leg",
        vec![Cube::new(
            (0.0, 105.0),
            (-3.5, -1.0, -4.0),
            (7.0, 10.0, 8.0),
        )],
        Pose::offset(7.5, 10.0, 0.0),
    );
    bone.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new(
            (0.0, 123.0),
            (-3.5, -1.0, -4.0),
            (7.0, 10.0, 8.0),
        )],
        Pose::offset(7.5, 10.0, 15.0),
    );
    LayerDefinition::new(root, 192.0, 192.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let bone = root.add_or_replace_child("bone", vec![], Pose::offset(0.0, 24.0, 0.0));
    {
        let body = bone.add_or_replace_child(
            "body",
            vec![
                Cube::new((0.0, 35.0), (-13.0, -14.0, -0.5), (14.0, 14.0, 20.0)).grown(0.25),
                Cube::new((0.0, 0.0), (-13.0, -14.0, -0.5), (14.0, 15.0, 20.0)),
                Cube::new((68.0, 0.0), (-13.0, 0.0, -0.5), (14.0, 0.0, 20.0)),
            ],
            Pose::offset(6.0, -3.0, -9.5),
        );
        let head = body.add_or_replace_child(
            "head",
            vec![
                Cube::new((68.0, 20.0), (-5.0, -4.25, -7.5), (10.0, 9.0, 9.0)),
                Cube::new((88.0, 20.0), (-5.0, 3.75, -7.5), (10.0, 0.0, 9.0)),
            ],
            Pose::offset(-6.0, -4.75, 0.0),
        );
        head.add_or_replace_child(
            "left_ear",
            vec![Cube::new((104.0, 38.0), (0.0, 0.0, -2.0), (1.0, 11.0, 3.0))],
            Pose::offset(5.0, -4.25, -1.5),
        );
        head.add_or_replace_child(
            "right_ear",
            vec![Cube::new((96.0, 38.0), (-1.0, 0.0, -2.0), (1.0, 11.0, 3.0))],
            Pose::offset(-5.0, -4.25, -1.5),
        );
        head.add_or_replace_child(
            "nose",
            vec![Cube::new(
                (68.0, 47.0),
                (-5.0, -3.0, -2.0),
                (10.0, 3.0, 4.0),
            )],
            Pose::offset(0.0, -1.25, -9.5),
        );
        head.add_or_replace_child(
            "lower_beak",
            vec![Cube::new(
                (68.0, 38.0),
                (-5.0, -2.5, -2.0),
                (10.0, 5.0, 4.0),
            )],
            Pose::offset(0.0, 1.25, -9.5),
        );
    }
    bone.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((0.0, 69.0), (-2.0, -1.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(-4.0, -4.0, -7.0),
    );
    bone.add_or_replace_child(
        "right_mid_leg",
        vec![Cube::new((0.0, 78.0), (-2.0, -1.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(-4.0, -4.0, 0.0),
    );
    bone.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((0.0, 87.0), (-2.0, -1.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(-4.0, -4.0, 7.0),
    );
    bone.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((16.0, 69.0), (-2.0, -1.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(4.0, -4.0, -7.0),
    );
    bone.add_or_replace_child(
        "left_mid_leg",
        vec![Cube::new((16.0, 78.0), (-2.0, -1.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(4.0, -4.0, 0.0),
    );
    bone.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((16.0, 87.0), (-2.0, -1.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(4.0, -4.0, 7.0),
    );
    LayerDefinition::new(root, 128.0, 128.0)
}

#[derive(Clone, Copy, Debug)]
pub struct SnifferParts {
    head: PartId,
}

impl SnifferParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
        }
    }
}

/// `SnifferModel.setupAnim`.
pub fn setup_anim(
    parts: &SnifferParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    searching: bool,
    walk: &KeyframeAnimation,
    search: &KeyframeAnimation,
    baby_transform: Option<&KeyframeAnimation>,
) {
    look(&mut pose[parts.head].pose, living);
    if searching {
        search.apply_walk(pose, living.walk_position, living.walk_speed, 9.0, 100.0);
    } else {
        walk.apply_walk(pose, living.walk_position, living.walk_speed, 9.0, 100.0);
    }
    if let Some(baby) = baby_transform {
        baby.apply_static(pose);
    }
}

/// Start happy/scent/sniff/dig/rise from `DATA_STATE`.
pub fn tick_anim(anim: &mut [AnimationState], tick: i32, state: i32) {
    for slot in [HAPPY, SCENTING, SNIFFING, SEARCHING, DIG, STAND_UP] {
        anim[slot].stop();
    }
    match state {
        STATE_FEELING_HAPPY => anim[HAPPY].start_if_stopped(tick),
        STATE_SCENTING => anim[SCENTING].start_if_stopped(tick),
        STATE_SNIFFING => anim[SNIFFING].start_if_stopped(tick),
        STATE_DIGGING => anim[DIG].start_if_stopped(tick),
        STATE_RISING => anim[STAND_UP].start_if_stopped(tick),
        _ => {}
    }
}

/// `SnifferRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::Sniffer, texture(false)),
        (M::SnifferBaby, texture(true)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bake_listed(model: &Model) {
        for (definition, _) in ANIMATIONS {
            KeyframeAnimation::bake(model, definition).unwrap_or_else(|e| panic!("{e}"));
        }
        KeyframeAnimation::bake(model, WALK).unwrap_or_else(|e| panic!("{e}"));
        KeyframeAnimation::bake(model, SEARCH).unwrap_or_else(|e| panic!("{e}"));
        KeyframeAnimation::bake(model, BABY_TRANSFORM).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn both_models_bake_their_animations() {
        let adult_model = Model::bake(&adult());
        bake_listed(&adult_model);
        let baby_model = Model::bake(&baby());
        bake_listed(&baby_model);
        KeyframeAnimation::bake(&baby_model, &anim::SNIFFER_BABY_FALL)
            .unwrap_or_else(|e| panic!("{e}"));

        let walk = KeyframeAnimation::bake(&adult_model, WALK).unwrap();
        let search = KeyframeAnimation::bake(&adult_model, SEARCH).unwrap();
        let parts = SnifferParts::find(&adult_model);
        let mut pose = adult_model.initial_pose();
        let living = LivingRenderState {
            x_rot: 10.0,
            ..Default::default()
        };
        setup_anim(&parts, &mut pose, &living, false, &walk, &search, None);
        assert!((pose[parts.head].pose.rotation.x - 10.0_f32.to_radians()).abs() < 1e-5);
        let mut states = [AnimationState::default(); 6];
        tick_anim(&mut states, 0, STATE_DIGGING);
        assert!(states[DIG].is_started());
    }
}
