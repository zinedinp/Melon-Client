//! `BreezeModel`.

use super::{LayerSpec, ModelId as M};
use crate::entity::animations::breeze as anim;
use crate::entity::keyframe::{AnimationDefinition, AnimationState};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{EIGHTH_PI, PI, THIRD_PI};

/// Animation slots (`PoseInput::anim`).
pub const IDLE: usize = 0;
pub const SHOOT: usize = 1;
pub const JUMP: usize = 2;
pub const INHALE: usize = 3;
pub const SLIDE: usize = 4;
pub const SLIDE_BACK: usize = 5;

/// `Pose.LONG_JUMPING`, `SLIDING`, `SHOOTING`, `INHALING`.
pub const POSE_LONG_JUMPING: i32 = 6;
pub const POSE_SLIDING: i32 = 15;
pub const POSE_SHOOTING: i32 = 16;
pub const POSE_INHALING: i32 = 17;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 6] = [
    (&anim::IDLE, IDLE),
    (&anim::SHOOT, SHOOT),
    (&anim::JUMP, JUMP),
    (&anim::INHALE, INHALE),
    (&anim::SLIDE, SLIDE),
    (&anim::SLIDE_BACK, SLIDE_BACK),
];

pub fn texture() -> &'static str {
    "minecraft:entity/breeze/breeze"
}

pub fn wind_texture() -> &'static str {
    "minecraft:entity/breeze/breeze_wind"
}

pub fn eyes_texture() -> &'static str {
    "minecraft:entity/breeze/breeze_eyes"
}

fn base() -> PartDefinition {
    let mut root = PartDefinition::root();
    {
        let body = root.add_or_replace_child("body", vec![], Pose::offset(0.0, 0.0, 0.0));
        {
            let rods = body.add_or_replace_child("rods", vec![], Pose::offset(0.0, 8.0, 0.0));
            rods.add_or_replace_child(
                "rod_1",
                vec![Cube::new((0.0, 17.0), (-1.0, 0.0, -3.0), (2.0, 8.0, 2.0))],
                Pose::offset_rotation((2.5981, -3.0, 1.5), (-2.7489, -THIRD_PI, PI)),
            );
            rods.add_or_replace_child(
                "rod_2",
                vec![Cube::new((0.0, 17.0), (-1.0, 0.0, -3.0), (2.0, 8.0, 2.0))],
                Pose::offset_rotation((-2.5981, -3.0, 1.5), (-2.7489, THIRD_PI, PI)),
            );
            rods.add_or_replace_child(
                "rod_3",
                vec![Cube::new((0.0, 17.0), (-1.0, 0.0, -3.0), (2.0, 8.0, 2.0))],
                Pose::offset_rotation((0.0, -3.0, -3.0), (EIGHTH_PI, 0.0, 0.0)),
            );
        }
        let head = body.add_or_replace_child(
            "head",
            vec![
                Cube::new((4.0, 24.0), (-5.0, -5.0, -4.2), (10.0, 3.0, 4.0)),
                Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)),
            ],
            Pose::offset(0.0, 4.0, 0.0),
        );
        head.add_or_replace_child(
            "eyes",
            vec![
                Cube::new((4.0, 24.0), (-5.0, -5.0, -4.2), (10.0, 3.0, 4.0)),
                Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)),
            ],
            Pose::offset(0.0, 0.0, 0.0),
        );
    }
    let wind_body = root.add_or_replace_child("wind_body", vec![], Pose::offset(0.0, 0.0, 0.0));
    let wind_bottom = wind_body.add_or_replace_child(
        "wind_bottom",
        vec![Cube::new((1.0, 83.0), (-2.5, -7.0, -2.5), (5.0, 7.0, 5.0))],
        Pose::offset(0.0, 24.0, 0.0),
    );
    let wind_mid = wind_bottom.add_or_replace_child(
        "wind_mid",
        vec![
            Cube::new((74.0, 28.0), (-6.0, -6.0, -6.0), (12.0, 6.0, 12.0)),
            Cube::new((78.0, 32.0), (-4.0, -6.0, -4.0), (8.0, 6.0, 8.0)),
            Cube::new((49.0, 71.0), (-2.5, -6.0, -2.5), (5.0, 6.0, 5.0)),
        ],
        Pose::offset(0.0, -7.0, 0.0),
    );
    wind_mid.add_or_replace_child(
        "wind_top",
        vec![
            Cube::new((0.0, 0.0), (-9.0, -8.0, -9.0), (18.0, 8.0, 18.0)),
            Cube::new((6.0, 6.0), (-6.0, -8.0, -6.0), (12.0, 8.0, 12.0)),
            Cube::new((105.0, 57.0), (-2.5, -8.0, -2.5), (5.0, 8.0, 5.0)),
        ],
        Pose::offset(0.0, -6.0, 0.0),
    );
    root
}

/// `BreezeModel.createBodyLayer`: head and rods, 32×32.
pub fn body() -> LayerDefinition {
    let mut root = base();
    root.retain_cubes(&["head", "rods"], true);
    LayerDefinition::new(root, 32.0, 32.0)
}

/// `BreezeModel.createWindLayer`: the wind body, 128×128.
pub fn wind() -> LayerDefinition {
    let mut root = base();
    root.retain_cubes(&["wind_body"], true);
    LayerDefinition::new(root, 128.0, 128.0)
}

#[derive(Clone, Copy, Debug)]
pub struct BreezeParts {}

impl BreezeParts {
    pub fn find(model: &Model) -> Self {
        let _ = model.part("head");
        let _ = model.part("wind_body");
        Self {}
    }
}

/// `BreezeModel.setupAnim` has no procedural pose; keyframes do the work.
pub fn setup_anim(_parts: &BreezeParts, _pose: &mut ModelPose, _living: &LivingRenderState) {}

/// Idle always runs; shoot, inhale, jump and slide follow `Pose`.
pub fn tick_anim(anim: &mut [AnimationState], tick: i32, pose: i32) {
    match pose {
        POSE_SHOOTING => {
            anim[SHOOT].start_if_stopped(tick);
            anim[INHALE].stop();
            anim[JUMP].stop();
        }
        POSE_INHALING => {
            anim[INHALE].start_if_stopped(tick);
            anim[SHOOT].stop();
            anim[JUMP].stop();
        }
        POSE_LONG_JUMPING => {
            anim[JUMP].start_if_stopped(tick);
            anim[SHOOT].stop();
            anim[INHALE].stop();
        }
        POSE_SLIDING => {
            anim[SLIDE].start_if_stopped(tick);
            anim[SLIDE_BACK].stop();
            anim[SHOOT].stop();
            anim[INHALE].stop();
            anim[JUMP].stop();
        }
        _ => {
            anim[SHOOT].stop();
            anim[INHALE].stop();
            anim[JUMP].stop();
            if anim[SLIDE].is_started() {
                anim[SLIDE_BACK].start(tick);
                anim[SLIDE].stop();
            }
        }
    }
    anim[IDLE].start_if_stopped(tick);
}

/// `BreezeRenderer` and its layers.
pub(super) fn push_layers(age_in_ticks: f32, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Breeze, texture()));
    out.push(plain(M::BreezeWind, wind_texture()).scrolled(age_in_ticks * 0.02, 0.0));
    out.push(plain(M::Breeze, eyes_texture()).glowing().translucent());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::keyframe::KeyframeAnimation;

    #[test]
    fn both_meshes_bake_every_animation() {
        for layer in [body(), wind()] {
            let model = Model::bake(&layer);
            for (definition, _) in ANIMATIONS {
                KeyframeAnimation::bake(&model, definition).unwrap_or_else(|e| panic!("{e}"));
            }
            let _ = BreezeParts::find(&model);
        }
        assert_eq!(eyes_texture(), "minecraft:entity/breeze/breeze_eyes");
        let mut states = [AnimationState::default(); 6];
        tick_anim(&mut states, 0, POSE_SLIDING);
        assert!(states[SLIDE].is_started());
        tick_anim(&mut states, 1, 0);
        assert!(states[SLIDE_BACK].is_started());
        assert!(!states[SLIDE].is_started());
    }
}
