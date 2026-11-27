//! `CopperGolemModel`: walk (or a held-item walk) is a keyframe cycle;

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::world::entity::animations::copper_golem as anim;
use crate::world::entity::keyframe::{AnimationDefinition, AnimationState, KeyframeAnimation};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::look;
use crate::world::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const IDLE: usize = 0;
pub const WALK_ITEM: usize = 1;
pub const CHEST_ITEM_DROP: usize = 2;
pub const CHEST_ITEM_NODROP: usize = 3;
pub const CHEST_NOITEM_GET: usize = 4;
pub const CHEST_NOITEM_NOGET: usize = 5;

/// `CopperGolemState` ids (`COPPER_GOLEM_STATE`).
pub const STATE_IDLE: i32 = 0;
pub const STATE_GETTING_ITEM: i32 = 1;
pub const STATE_GETTING_NO_ITEM: i32 = 2;
pub const STATE_DROPPING_ITEM: i32 = 3;
pub const STATE_DROPPING_NO_ITEM: i32 = 4;

/// `WeatheringCopper.WeatherState` ordinals.
pub const WEATHER_UNAFFECTED: i32 = 0;
pub const WEATHER_EXPOSED: i32 = 1;
pub const WEATHER_WEATHERED: i32 = 2;
pub const WEATHER_OXIDIZED: i32 = 3;

/// `Z_FIGHT_MITIGATION` on the head.
const Z_FIGHT: f32 = 0.015;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 6] = [
    (&anim::COPPER_GOLEM_IDLE, IDLE),
    (&anim::COPPER_GOLEM_WALK_ITEM, WALK_ITEM),
    (
        &anim::COPPER_GOLEM_CHEST_INTERACTION_ITEM_DROP,
        CHEST_ITEM_DROP,
    ),
    (
        &anim::COPPER_GOLEM_CHEST_INTERACTION_ITEM_NODROP,
        CHEST_ITEM_NODROP,
    ),
    (
        &anim::COPPER_GOLEM_CHEST_INTERACTION_NOITEM_GET,
        CHEST_NOITEM_GET,
    ),
    (
        &anim::COPPER_GOLEM_CHEST_INTERACTION_NOITEM_NOGET,
        CHEST_NOITEM_NOGET,
    ),
];

pub static WALK: &AnimationDefinition = &anim::COPPER_GOLEM_WALK;
pub static WALK_WITH_ITEM: &AnimationDefinition = &anim::COPPER_GOLEM_WALK_ITEM;

const BODY: [&str; 4] = [
    "minecraft:entity/copper_golem/copper_golem",
    "minecraft:entity/copper_golem/copper_golem_exposed",
    "minecraft:entity/copper_golem/copper_golem_weathered",
    "minecraft:entity/copper_golem/copper_golem_oxidized",
];
const EYES: [&str; 4] = [
    "minecraft:entity/copper_golem/copper_golem_eyes",
    "minecraft:entity/copper_golem/copper_golem_eyes_exposed",
    "minecraft:entity/copper_golem/copper_golem_eyes_weathered",
    "minecraft:entity/copper_golem/copper_golem_eyes_oxidized",
];

fn weathering_index(weathering: i32) -> usize {
    usize::try_from(weathering)
        .ok()
        .filter(|&i| i < 4)
        .unwrap_or(0)
}

pub fn texture(weathering: i32) -> &'static str {
    BODY[weathering_index(weathering)]
}

pub fn eyes_texture(weathering: i32) -> &'static str {
    EYES[weathering_index(weathering)]
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.pose = Pose::offset(0.0, 24.0, 0.0);
    {
        let body = root.add_or_replace_child(
            "body",
            vec![Cube::new((0.0, 15.0), (-4.0, -6.0, -3.0), (8.0, 6.0, 6.0))],
            Pose::offset(0.0, -5.0, 0.0),
        );
        body.add_or_replace_child(
            "head",
            vec![
                Cube::new((0.0, 0.0), (-4.0, -5.0, -5.0), (8.0, 5.0, 10.0)).grown(Z_FIGHT),
                Cube::new((56.0, 0.0), (-1.0, -2.0, -6.0), (2.0, 3.0, 2.0)),
                Cube::new((37.0, 8.0), (-1.0, -9.0, -1.0), (2.0, 4.0, 2.0)).grown(-Z_FIGHT),
                Cube::new((37.0, 0.0), (-2.0, -13.0, -2.0), (4.0, 4.0, 4.0)).grown(-Z_FIGHT),
            ],
            Pose::offset(0.0, -6.0, 0.0),
        );
        body.add_or_replace_child(
            "right_arm",
            vec![Cube::new(
                (36.0, 16.0),
                (-3.0, -1.0, -2.0),
                (3.0, 10.0, 4.0),
            )],
            Pose::offset(-4.0, -6.0, 0.0),
        );
        body.add_or_replace_child(
            "left_arm",
            vec![Cube::new((50.0, 16.0), (0.0, -1.0, -2.0), (3.0, 10.0, 4.0))],
            Pose::offset(4.0, -6.0, 0.0),
        );
    }
    root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((0.0, 27.0), (-4.0, 0.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(0.0, -5.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((16.0, 27.0), (0.0, 0.0, -2.0), (4.0, 5.0, 4.0))],
        Pose::offset(0.0, -5.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct CopperGolemParts {
    head: PartId,
    right_arm: PartId,
    left_arm: PartId,
}

impl CopperGolemParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            right_arm: model.part("right_arm"),
            left_arm: model.part("left_arm"),
        }
    }
}

/// `CopperGolemModel.setupAnim`: look, then walk or the held-item walk.
pub fn setup_anim(
    parts: &CopperGolemParts,
    walk: &KeyframeAnimation,
    walk_item: &KeyframeAnimation,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    holding: bool,
) {
    look(&mut pose[parts.head].pose, living);
    if holding {
        walk_item.apply_walk(pose, living.walk_position, living.walk_speed, 2.0, 2.5);
        pose_held_item_arms(parts, pose);
    } else {
        walk.apply_walk(pose, living.walk_position, living.walk_speed, 2.0, 2.5);
    }
}

fn pose_held_item_arms(parts: &CopperGolemParts, pose: &mut ModelPose) {
    let right = &mut pose[parts.right_arm].pose.rotation;
    right.x = right.x.min(-0.872_664_63);
    right.y = right.y.min(-0.113_446_4);
    right.z = right.z.min(-0.064_577_185);
    let left = &mut pose[parts.left_arm].pose.rotation;
    left.x = left.x.min(-0.872_664_63);
    left.y = left.y.max(0.113_446_4);
    left.z = left.z.max(0.064_577_185);
}

/// Idle timeout: `random.nextInt(200, 240)`.
#[derive(Clone)]
pub struct CopperGolemClock {
    idle_start: i32,
    rng: u32,
}

impl CopperGolemClock {
    pub fn new(seed: i32) -> Self {
        Self {
            idle_start: 0,
            rng: (seed as u32).wrapping_mul(2_654_435_761) | 1,
        }
    }

    fn next_delay(&mut self) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        200 + (self.rng % 40) as i32
    }

    /// `CopperGolem.setupAnimationStates`.
    pub fn tick(&mut self, anim: &mut [AnimationState], tick: i32, state: i32) {
        match state {
            STATE_IDLE => {
                anim[CHEST_NOITEM_GET].stop();
                anim[CHEST_NOITEM_NOGET].stop();
                anim[CHEST_ITEM_DROP].stop();
                anim[CHEST_ITEM_NODROP].stop();
                if self.idle_start == tick {
                    anim[IDLE].start(tick);
                } else if self.idle_start == 0 {
                    self.idle_start = tick + self.next_delay();
                }
                if tick == self.idle_start + 10 {
                    self.idle_start = 0;
                }
            }
            STATE_GETTING_ITEM => {
                anim[IDLE].stop();
                self.idle_start = 0;
                anim[CHEST_NOITEM_NOGET].stop();
                anim[CHEST_ITEM_DROP].stop();
                anim[CHEST_ITEM_NODROP].stop();
                anim[CHEST_NOITEM_GET].start_if_stopped(tick);
            }
            STATE_GETTING_NO_ITEM => {
                anim[IDLE].stop();
                self.idle_start = 0;
                anim[CHEST_NOITEM_GET].stop();
                anim[CHEST_ITEM_NODROP].stop();
                anim[CHEST_ITEM_DROP].stop();
                anim[CHEST_NOITEM_NOGET].start_if_stopped(tick);
            }
            STATE_DROPPING_ITEM => {
                anim[IDLE].stop();
                self.idle_start = 0;
                anim[CHEST_NOITEM_GET].stop();
                anim[CHEST_NOITEM_NOGET].stop();
                anim[CHEST_ITEM_NODROP].stop();
                anim[CHEST_ITEM_DROP].start_if_stopped(tick);
            }
            STATE_DROPPING_NO_ITEM => {
                anim[IDLE].stop();
                self.idle_start = 0;
                anim[CHEST_NOITEM_GET].stop();
                anim[CHEST_NOITEM_NOGET].stop();
                anim[CHEST_ITEM_DROP].stop();
                anim[CHEST_ITEM_NODROP].start_if_stopped(tick);
            }
            _ => {}
        }
    }
}

/// `CopperGolemRenderer` and its eyes.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let weathering = entity.appearance.copper_weather;
    let plain = LayerSpec::plain;
    out.push(plain(M::CopperGolem, texture(weathering)));
    out.push(plain(M::CopperGolem, eyes_texture(weathering)).glowing());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animations_bake_and_weathering_picks_a_texture() {
        let model = Model::bake(&layer());
        for (definition, _) in ANIMATIONS {
            KeyframeAnimation::bake(&model, definition).unwrap_or_else(|e| panic!("{e}"));
        }
        let walk = KeyframeAnimation::bake(&model, WALK).unwrap_or_else(|e| panic!("{e}"));
        let walk_item =
            KeyframeAnimation::bake(&model, WALK_WITH_ITEM).unwrap_or_else(|e| panic!("{e}"));
        let parts = CopperGolemParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &walk,
            &walk_item,
            &mut pose,
            &LivingRenderState::default(),
            true,
        );
        assert!(pose[parts.right_arm].pose.rotation.x <= -0.872_664_63);
        assert_eq!(texture(WEATHER_OXIDIZED), BODY[3]);
        assert_eq!(eyes_texture(WEATHER_EXPOSED), EYES[1]);
        assert_eq!(texture(99), BODY[0]);
    }
}
