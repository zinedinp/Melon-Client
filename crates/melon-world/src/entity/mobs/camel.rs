//! `AdultCamelModel` and `BabyCamelModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::animations::{camel as adult_anim, camel_baby as baby_anim};
use crate::entity::keyframe::{AnimationDefinition, AnimationState, KeyframeAnimation};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const SIT: usize = 0;
pub const SIT_POSE: usize = 1;
pub const STANDUP: usize = 2;
pub const IDLE: usize = 3;
pub const DASH: usize = 4;

/// `Pose.SITTING`.
pub const POSE_SITTING: i32 = 10;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 5] = [
    (&adult_anim::CAMEL_SIT, SIT),
    (&adult_anim::CAMEL_SIT_POSE, SIT_POSE),
    (&adult_anim::CAMEL_STANDUP, STANDUP),
    (&adult_anim::CAMEL_IDLE, IDLE),
    (&adult_anim::CAMEL_DASH, DASH),
];
pub static BABY_ANIMATIONS: [(&AnimationDefinition, usize); 5] = [
    (&baby_anim::CAMEL_BABY_SIT, SIT),
    (&baby_anim::CAMEL_BABY_SIT_POSE, SIT_POSE),
    (&baby_anim::CAMEL_BABY_STANDUP, STANDUP),
    (&baby_anim::CAMEL_BABY_IDLE, IDLE),
    (&baby_anim::CAMEL_BABY_DASH, DASH),
];
pub static WALK: &AnimationDefinition = &adult_anim::CAMEL_WALK;
pub static BABY_WALK: &AnimationDefinition = &baby_anim::CAMEL_BABY_WALK;

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (0.0, 25.0),
            (-7.5, -12.0, -23.5),
            (15.0, 12.0, 27.0),
        )],
        Pose::offset(0.0, 4.0, 9.5),
    );
    body.add_or_replace_child(
        "hump",
        vec![Cube::new((74.0, 0.0), (-4.5, -5.0, -5.5), (9.0, 5.0, 11.0))],
        Pose::offset(0.0, -12.0, -10.0),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((122.0, 0.0), (-1.5, 0.0, 0.0), (3.0, 14.0, 0.0))],
        Pose::offset(0.0, -9.0, 3.5),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![
            Cube::new((60.0, 24.0), (-3.5, -7.0, -15.0), (7.0, 8.0, 19.0)),
            Cube::new((21.0, 0.0), (-3.5, -21.0, -15.0), (7.0, 14.0, 7.0)),
            Cube::new((50.0, 0.0), (-2.5, -21.0, -21.0), (5.0, 5.0, 6.0)),
        ],
        Pose::offset(0.0, -3.0, -19.5),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((45.0, 0.0), (-0.5, 0.5, -1.0), (3.0, 1.0, 2.0))],
        Pose::offset(2.5, -21.0, -9.5),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((67.0, 0.0), (-2.5, 0.5, -1.0), (3.0, 1.0, 2.0))],
        Pose::offset(-2.5, -21.0, -9.5),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((58.0, 16.0), (-2.5, 2.0, -2.5), (5.0, 21.0, 5.0))],
        Pose::offset(4.9, 1.0, 9.5),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((94.0, 16.0), (-2.5, 2.0, -2.5), (5.0, 21.0, 5.0))],
        Pose::offset(-4.9, 1.0, 9.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((0.0, 0.0), (-2.5, 2.0, -2.5), (5.0, 21.0, 5.0))],
        Pose::offset(4.9, 1.0, -10.5),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((0.0, 26.0), (-2.5, 2.0, -2.5), (5.0, 21.0, 5.0))],
        Pose::offset(-4.9, 1.0, -10.5),
    );
    LayerDefinition::new(root, 128.0, 128.0)
}

/// `CamelSaddleModel.createSaddleLayer`: saddle, bridle and reins on the adult.
pub fn saddle() -> LayerDefinition {
    let mut layer = adult();
    let inflate = 0.05;
    let body = layer.root.child_mut("body");
    body.add_or_replace_child(
        "saddle",
        vec![
            Cube::new((74.0, 64.0), (-4.5, -17.0, -15.5), (9.0, 5.0, 11.0)).grown(inflate),
            Cube::new((92.0, 114.0), (-3.5, -20.0, -15.5), (7.0, 3.0, 11.0)).grown(inflate),
            Cube::new((0.0, 89.0), (-7.5, -12.0, -23.5), (15.0, 12.0, 27.0)).grown(inflate),
        ],
        Pose::ZERO,
    );
    let head = body.child_mut("head");
    head.add_or_replace_child(
        "reins",
        vec![
            Cube::new((98.0, 42.0), (3.51, -18.0, -17.0), (0.0, 7.0, 15.0)),
            Cube::new((84.0, 57.0), (-3.5, -18.0, -2.0), (7.0, 7.0, 0.0)),
            Cube::new((98.0, 42.0), (-3.51, -18.0, -17.0), (0.0, 7.0, 15.0)),
        ],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "bridle",
        vec![
            Cube::new((60.0, 87.0), (-3.5, -7.0, -15.0), (7.0, 8.0, 19.0)).grown(inflate),
            Cube::new((21.0, 64.0), (-3.5, -21.0, -15.0), (7.0, 14.0, 7.0)).grown(inflate),
            Cube::new((50.0, 64.0), (-2.5, -21.0, -21.0), (5.0, 5.0, 6.0)).grown(inflate),
            Cube::new((74.0, 70.0), (2.5, -19.0, -18.0), (1.0, 2.0, 2.0)),
            Cube::new((74.0, 70.0), (-3.5, -19.0, -18.0), (1.0, 2.0, 2.0)).mirrored(),
        ],
        Pose::ZERO,
    );
    layer
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 14.0), (-4.5, -4.0, -8.0), (9.0, 8.0, 16.0))],
        Pose::offset(0.0, 7.0, 0.0),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((50.0, 38.0), (-1.5, -0.5, 0.0), (3.0, 9.0, 0.0))],
        Pose::offset(0.0, -1.5, 8.05),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![
            Cube::new((20.0, 0.0), (-2.5, -3.0, -7.5), (5.0, 5.0, 7.0)),
            Cube::new((0.0, 0.0), (-2.5, -12.0, -7.5), (5.0, 9.0, 5.0)),
            Cube::new((0.0, 14.0), (-2.5, -12.0, -10.5), (5.0, 4.0, 3.0)),
        ],
        Pose::offset(0.0, 1.0, -7.5),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((37.0, 0.0), (-3.0, -0.5, -1.0), (3.0, 1.0, 2.0))],
        Pose::offset(-2.5, -11.0, -4.0),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((47.0, 0.0), (0.0, -0.5, -1.0), (3.0, 1.0, 2.0))],
        Pose::offset(2.5, -11.0, -4.0),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new(
            (36.0, 14.0),
            (-1.5, -0.5, -1.5),
            (3.0, 13.0, 3.0),
        )],
        Pose::offset(-3.0, 11.5, -5.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new(
            (48.0, 14.0),
            (-1.5, -0.5, -1.5),
            (3.0, 13.0, 3.0),
        )],
        Pose::offset(3.0, 11.5, -5.5),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new(
            (12.0, 38.0),
            (-1.5, -0.5, -1.5),
            (3.0, 13.0, 3.0),
        )],
        Pose::offset(3.0, 11.5, 5.5),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((0.0, 38.0), (-1.5, -0.5, -1.5), (3.0, 13.0, 3.0))],
        Pose::offset(-3.0, 11.5, 5.5),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct CamelParts {
    head: PartId,
    reins: Option<PartId>,
}

impl CamelParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            reins: model.find("reins"),
        }
    }
}

/// `CamelModel.setupAnim`: the head follows the look, then the walk.
pub fn setup_anim(
    parts: &CamelParts,
    walk: &KeyframeAnimation,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    jump_cooldown: f32,
) {
    let y = living.y_rot.clamp(-30.0, 30.0);
    let mut x = living.x_rot.clamp(-25.0, 45.0);
    if jump_cooldown > 0.0 {
        x = (x + 45.0 * jump_cooldown / 55.0).clamp(-25.0, 70.0);
    }
    pose[parts.head].pose.rotation.y = y.to_radians();
    pose[parts.head].pose.rotation.x = x.to_radians();
    if let Some(reins) = parts.reins {
        pose[reins].visible = living.riding;
    }
    walk.apply_walk(pose, living.walk_position, living.walk_speed, 2.0, 2.5);
}

/// Idle timeout like a rabbit's: `random.nextInt(40) + 80`.
#[derive(Clone)]
pub struct CamelClock {
    idle_timeout: i32,
    rng: u32,
}

impl CamelClock {
    pub fn new(seed: i32) -> Self {
        Self {
            idle_timeout: 0,
            rng: (seed as u32).wrapping_mul(2_654_435_761) | 1,
        }
    }

    fn next_timeout(&mut self) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 40) as i32 + 80
    }

    /// `Camel.setupAnimationStates`, given whether it sits.
    pub fn tick(
        &mut self,
        anim: &mut [AnimationState],
        tick: i32,
        sitting: bool,
        sitting_down: bool,
        dashing: bool,
        standing_up: bool,
    ) {
        if self.idle_timeout <= 0 {
            self.idle_timeout = self.next_timeout();
            anim[IDLE].start(tick);
        } else {
            self.idle_timeout -= 1;
        }
        if sitting {
            anim[STANDUP].stop();
            anim[DASH].stop();
            if sitting_down {
                anim[SIT].start_if_stopped(tick);
                anim[SIT_POSE].stop();
            } else {
                anim[SIT].stop();
                anim[SIT_POSE].start_if_stopped(tick);
            }
        } else {
            anim[SIT].stop();
            anim[SIT_POSE].stop();
            anim[DASH].animate_when(dashing, tick);
            anim[STANDUP].animate_when(standing_up, tick);
        }
    }
}

/// `CamelRenderer` and `CamelHuskRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    match entity.kind {
        EntityKind::Camel => out.push(LayerSpec::aged(
            baby,
            (M::Camel, "minecraft:entity/camel/camel"),
            (M::CamelBaby, "minecraft:entity/camel/camel_baby"),
        )),
        // `CamelHuskRenderer`: adult only.
        EntityKind::CamelHusk => out.push(LayerSpec::plain(
            M::Camel,
            "minecraft:entity/camel/camel_husk",
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_their_animations() {
        for (layer, animations, walk) in [
            (adult(), &ANIMATIONS[..], WALK),
            (baby(), &BABY_ANIMATIONS[..], BABY_WALK),
        ] {
            let model = Model::bake(&layer);
            for (definition, _) in animations {
                assert!(KeyframeAnimation::bake(&model, definition).is_ok());
            }
            assert!(KeyframeAnimation::bake(&model, walk).is_ok());
        }
    }
}
