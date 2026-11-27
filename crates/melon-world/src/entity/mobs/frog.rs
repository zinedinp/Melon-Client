//! `FrogModel`: a keyframe-driven hopper.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::animations::frog as anim;
use crate::entity::keyframe::{AnimationDefinition, KeyframeAnimation};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const JUMP: usize = 0;
pub const CROAK: usize = 1;
pub const TONGUE: usize = 2;
pub const IDLE_WATER: usize = 3;

/// `Pose.LONG_JUMPING`, `CROAKING`, `USING_TONGUE`.
pub const POSE_LONG_JUMPING: i32 = 6;
pub const POSE_CROAKING: i32 = 8;
pub const POSE_USING_TONGUE: i32 = 9;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 4] = [
    (&anim::FROG_JUMP, JUMP),
    (&anim::FROG_CROAK, CROAK),
    (&anim::FROG_TONGUE, TONGUE),
    (&anim::FROG_IDLE_WATER, IDLE_WATER),
];

pub static WALK: &AnimationDefinition = &anim::FROG_WALK;
pub static SWIM: &AnimationDefinition = &anim::FROG_SWIM;

/// `FrogVariant` textures: cold, temperate, warm.
pub fn texture(variant: i32) -> &'static str {
    const TEXTURES: [&str; 3] = [
        "minecraft:entity/frog/frog_cold",
        "minecraft:entity/frog/frog_temperate",
        "minecraft:entity/frog/frog_warm",
    ];
    TEXTURES[usize::try_from(variant)
        .ok()
        .filter(|&i| i < 3)
        .unwrap_or(1)]
}

pub fn layer() -> LayerDefinition {
    let mut mesh = PartDefinition::root();
    let root = mesh.add_or_replace_child("root", vec![], Pose::offset(0.0, 24.0, 0.0));
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((3.0, 1.0), (-3.5, -2.0, -8.0), (7.0, 3.0, 9.0)),
            Cube::new((23.0, 22.0), (-3.5, -1.0, -8.0), (7.0, 0.0, 9.0)),
        ],
        Pose::offset(0.0, -2.0, 4.0),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![
            Cube::new((23.0, 13.0), (-3.5, -1.0, -7.0), (7.0, 0.0, 9.0)),
            Cube::new((0.0, 13.0), (-3.5, -2.0, -7.0), (7.0, 3.0, 9.0)),
        ],
        Pose::offset(0.0, -2.0, -1.0),
    );
    let eyes = head.add_or_replace_child("eyes", vec![], Pose::offset(-0.5, 0.0, 2.0));
    eyes.add_or_replace_child(
        "right_eye",
        vec![Cube::new((0.0, 0.0), (-1.5, -1.0, -1.5), (3.0, 2.0, 3.0))],
        Pose::offset(-1.5, -3.0, -6.5),
    );
    eyes.add_or_replace_child(
        "left_eye",
        vec![Cube::new((0.0, 5.0), (-1.5, -1.0, -1.5), (3.0, 2.0, 3.0))],
        Pose::offset(2.5, -3.0, -6.5),
    );
    body.add_or_replace_child(
        "croaking_body",
        vec![Cube::new((26.0, 5.0), (-3.5, -0.1, -2.9), (7.0, 2.0, 3.0)).grown(-0.1)],
        Pose::offset(0.0, -1.0, -5.0),
    );
    body.add_or_replace_child(
        "tongue",
        vec![Cube::new((17.0, 13.0), (-2.0, 0.0, -7.1), (4.0, 0.0, 7.0))],
        Pose::offset(0.0, -1.01, 1.0),
    );
    let left_arm = body.add_or_replace_child(
        "left_arm",
        vec![Cube::new((0.0, 32.0), (-1.0, 0.0, -1.0), (2.0, 3.0, 3.0))],
        Pose::offset(4.0, -1.0, -6.5),
    );
    left_arm.add_or_replace_child(
        "left_hand",
        vec![Cube::new((18.0, 40.0), (-4.0, 0.01, -4.0), (8.0, 0.0, 8.0))],
        Pose::offset(0.0, 3.0, -1.0),
    );
    let right_arm = body.add_or_replace_child(
        "right_arm",
        vec![Cube::new((0.0, 38.0), (-1.0, 0.0, -1.0), (2.0, 3.0, 3.0))],
        Pose::offset(-4.0, -1.0, -6.5),
    );
    right_arm.add_or_replace_child(
        "right_hand",
        vec![Cube::new((2.0, 40.0), (-4.0, 0.01, -5.0), (8.0, 0.0, 8.0))],
        Pose::offset(0.0, 3.0, 0.0),
    );
    let left_leg = root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((14.0, 25.0), (-1.0, 0.0, -2.0), (3.0, 3.0, 4.0))],
        Pose::offset(3.5, -3.0, 4.0),
    );
    left_leg.add_or_replace_child(
        "left_foot",
        vec![Cube::new((2.0, 32.0), (-4.0, 0.01, -4.0), (8.0, 0.0, 8.0))],
        Pose::offset(2.0, 3.0, 0.0),
    );
    let right_leg = root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((0.0, 25.0), (-2.0, 0.0, -2.0), (3.0, 3.0, 4.0))],
        Pose::offset(-3.5, -3.0, 4.0),
    );
    right_leg.add_or_replace_child(
        "right_foot",
        vec![Cube::new((18.0, 32.0), (-4.0, 0.01, -4.0), (8.0, 0.0, 8.0))],
        Pose::offset(-2.0, 3.0, 0.0),
    );
    LayerDefinition::new(mesh, 48.0, 48.0)
}

#[derive(Clone, Copy, Debug)]
pub struct FrogParts {
    croaking_body: PartId,
}

impl FrogParts {
    pub fn find(model: &Model) -> Self {
        Self {
            croaking_body: model.part("croaking_body"),
        }
    }
}

/// Walk or swim by whether it is in the water, then hide the throat.
pub fn setup_anim(
    parts: &FrogParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    in_water: bool,
    croaking: bool,
    walk: &KeyframeAnimation,
    swim: &KeyframeAnimation,
) {
    if in_water {
        swim.apply_walk(pose, living.walk_position, living.walk_speed, 1.0, 2.5);
    } else {
        walk.apply_walk(pose, living.walk_position, living.walk_speed, 1.5, 2.5);
    }
    pose[parts.croaking_body].visible = croaking;
}

/// `FrogRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(M::Frog, texture(a.variant)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animations_bake_and_the_throat_hides_until_it_croaks() {
        let model = Model::bake(&layer());
        for (definition, _) in ANIMATIONS {
            assert!(KeyframeAnimation::bake(&model, definition).is_ok());
        }
        let walk = KeyframeAnimation::bake(&model, WALK).unwrap();
        let swim = KeyframeAnimation::bake(&model, SWIM).unwrap();
        let parts = FrogParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            false,
            false,
            &walk,
            &swim,
        );
        assert!(!pose[parts.croaking_body].visible);
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            false,
            true,
            &walk,
            &swim,
        );
        assert!(pose[parts.croaking_body].visible);
    }
}
