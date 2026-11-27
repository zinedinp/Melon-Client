//! `CreakingModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::animations::creaking as anim;
use crate::entity::keyframe::{AnimationDefinition, AnimationState, KeyframeAnimation};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::look;
use crate::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const ATTACK: usize = 0;
pub const INVULNERABLE: usize = 1;
pub const DEATH: usize = 2;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 3] = [
    (&anim::CREAKING_ATTACK, ATTACK),
    (&anim::CREAKING_INVULNERABLE, INVULNERABLE),
    (&anim::CREAKING_DEATH, DEATH),
];

pub static WALK: &AnimationDefinition = &anim::CREAKING_WALK;

pub fn texture() -> &'static str {
    "minecraft:entity/creaking/creaking"
}

pub fn eyes_texture() -> &'static str {
    "minecraft:entity/creaking/creaking_eyes"
}

/// Eyes overlay: tearing-down uses `has_glowing_eyes`; otherwise `IS_ACTIVE`.
pub fn eyes_glowing(active: bool, tearing_down: bool, has_glowing_eyes: bool) -> bool {
    if tearing_down {
        has_glowing_eyes
    } else {
        active
    }
}

pub fn layer() -> LayerDefinition {
    let mut mesh = PartDefinition::root();
    let root = mesh.add_or_replace_child("root", vec![], Pose::offset(0.0, 24.0, 0.0));
    {
        let upper_body =
            root.add_or_replace_child("upper_body", vec![], Pose::offset(-1.0, -19.0, 0.0));
        upper_body.add_or_replace_child(
            "head",
            vec![
                Cube::new((0.0, 0.0), (-3.0, -10.0, -3.0), (6.0, 10.0, 6.0)),
                Cube::new((28.0, 31.0), (-3.0, -13.0, -3.0), (6.0, 3.0, 6.0)),
                Cube::new((12.0, 40.0), (3.0, -13.0, 0.0), (9.0, 14.0, 0.0)),
                Cube::new((34.0, 12.0), (-12.0, -14.0, 0.0), (9.0, 14.0, 0.0)),
            ],
            Pose::offset(-3.0, -11.0, 0.0),
        );
        upper_body.add_or_replace_child(
            "body",
            vec![
                Cube::new((0.0, 16.0), (0.0, -3.0, -3.0), (6.0, 13.0, 5.0)),
                Cube::new((24.0, 0.0), (-6.0, -4.0, -3.0), (6.0, 7.0, 5.0)),
            ],
            Pose::offset(0.0, -7.0, 1.0),
        );
        upper_body.add_or_replace_child(
            "right_arm",
            vec![
                Cube::new((22.0, 13.0), (-2.0, -1.5, -1.5), (3.0, 21.0, 3.0)),
                Cube::new((46.0, 0.0), (-2.0, 19.5, -1.5), (3.0, 4.0, 3.0)),
            ],
            Pose::offset(-7.0, -9.5, 1.5),
        );
        upper_body.add_or_replace_child(
            "left_arm",
            vec![
                Cube::new((30.0, 40.0), (0.0, -1.0, -1.5), (3.0, 16.0, 3.0)),
                Cube::new((52.0, 12.0), (0.0, -5.0, -1.5), (3.0, 4.0, 3.0)),
                Cube::new((52.0, 19.0), (0.0, 15.0, -1.5), (3.0, 4.0, 3.0)),
            ],
            Pose::offset(6.0, -9.0, 0.5),
        );
    }
    root.add_or_replace_child(
        "left_leg",
        vec![
            Cube::new((42.0, 40.0), (-1.5, 0.0, -1.5), (3.0, 16.0, 3.0)),
            Cube::new((45.0, 55.0), (-1.5, 15.7, -4.5), (5.0, 0.0, 9.0)),
        ],
        Pose::offset(1.5, -16.0, 0.5),
    );
    root.add_or_replace_child(
        "right_leg",
        vec![
            Cube::new((0.0, 34.0), (-3.0, -1.5, -1.5), (3.0, 19.0, 3.0)),
            Cube::new((45.0, 46.0), (-5.0, 17.2, -4.5), (5.0, 0.0, 9.0)),
            Cube::new((12.0, 34.0), (-3.0, -4.5, -1.5), (3.0, 3.0, 3.0)),
        ],
        Pose::offset(-1.0, -17.5, 0.5),
    );
    LayerDefinition::new(mesh, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct CreakingParts {
    head: PartId,
}

impl CreakingParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
        }
    }
}

/// `CreakingModel.setupAnim`.
pub fn setup_anim(
    parts: &CreakingParts,
    walk: &KeyframeAnimation,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    can_move: bool,
) {
    look(&mut pose[parts.head].pose, living);
    if can_move {
        walk.apply_walk(pose, living.walk_position, living.walk_speed, 1.0, 1.0);
    }
}

/// Attack, invulnerable and death clocks (`Creaking.setupAnimationStates`).
pub fn tick_anim(
    anim: &mut [AnimationState],
    tick: i32,
    attacking: bool,
    invulnerable: bool,
    tearing_down: bool,
) {
    anim[ATTACK].animate_when(attacking, tick);
    anim[INVULNERABLE].animate_when(invulnerable, tick);
    anim[DEATH].animate_when(tearing_down, tick);
}

/// `CreakingRenderer` and its eyes.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(M::Creaking, texture()));
    if eyes_glowing(a.creaking_active, false, false) {
        out.push(plain(M::Creaking, eyes_texture()).glowing());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animations_bake_and_inactive_eyes_hide() {
        let model = Model::bake(&layer());
        for (definition, _) in ANIMATIONS {
            KeyframeAnimation::bake(&model, definition).unwrap_or_else(|e| panic!("{e}"));
        }
        let walk = KeyframeAnimation::bake(&model, WALK).unwrap_or_else(|e| panic!("{e}"));
        let parts = CreakingParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &walk,
            &mut pose,
            &LivingRenderState::default(),
            false,
        );
        assert!(!eyes_glowing(false, false, false));
        assert!(eyes_glowing(true, false, false));
        assert!(eyes_glowing(false, true, true));
        assert_eq!(eyes_texture(), "minecraft:entity/creaking/creaking_eyes");
    }
}
