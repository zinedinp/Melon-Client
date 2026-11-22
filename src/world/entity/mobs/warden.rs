//! `WardenModel`: walk, idle wobble and tendrils are procedural; emerge,

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::{self, HALF_PI, PI};
use crate::world::entity::animations::warden as anim;
use crate::world::entity::keyframe::{AnimationDefinition, AnimationState};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// Animation slots (`PoseInput::anim`).
pub const EMERGE: usize = 0;
pub const DIG: usize = 1;
pub const ROAR: usize = 2;
pub const SNIFF: usize = 3;
pub const ATTACK: usize = 4;
pub const SONIC: usize = 5;

/// `EntityEvent` ids that start attack, tendrils and the sonic boom.
pub const EVENT_ATTACK: u8 = 4;
pub const EVENT_TENDRIL: u8 = 61;
pub const EVENT_SONIC: u8 = 62;

/// `Pose.ROARING`, `SNIFFING`, `EMERGING`, `DIGGING`.
pub const POSE_ROARING: i32 = 11;
pub const POSE_SNIFFING: i32 = 12;
pub const POSE_EMERGING: i32 = 13;
pub const POSE_DIGGING: i32 = 14;

/// Default arm pose (`DEFAULT_ARM_X_Y`, `DEFAULT_ARM_Z`).
const ARM_X: f32 = 13.0;
const ARM_Y: f32 = -13.0;
const ARM_Z: f32 = 1.0;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 6] = [
    (&anim::WARDEN_EMERGE, EMERGE),
    (&anim::WARDEN_DIG, DIG),
    (&anim::WARDEN_ROAR, ROAR),
    (&anim::WARDEN_SNIFF, SNIFF),
    (&anim::WARDEN_ATTACK, ATTACK),
    (&anim::WARDEN_SONIC_BOOM, SONIC),
];

pub fn texture() -> &'static str {
    "minecraft:entity/warden/warden"
}

pub fn bioluminescent_texture() -> &'static str {
    "minecraft:entity/warden/warden_bioluminescent_layer"
}

pub fn heart_texture() -> &'static str {
    "minecraft:entity/warden/warden_heart"
}

pub fn pulsating_spots_1_texture() -> &'static str {
    "minecraft:entity/warden/warden_pulsating_spots_1"
}

pub fn pulsating_spots_2_texture() -> &'static str {
    "minecraft:entity/warden/warden_pulsating_spots_2"
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let bone = root.add_or_replace_child("bone", vec![], Pose::offset(0.0, 24.0, 0.0));
    {
        let body = bone.add_or_replace_child(
            "body",
            vec![Cube::new(
                (0.0, 0.0),
                (-9.0, -13.0, -4.0),
                (18.0, 21.0, 11.0),
            )],
            Pose::offset(0.0, -21.0, 0.0),
        );
        body.add_or_replace_child(
            "right_ribcage",
            vec![Cube::new(
                (90.0, 11.0),
                (-2.0, -11.0, -0.1),
                (9.0, 21.0, 0.0),
            )],
            Pose::offset(-7.0, -2.0, -4.0),
        );
        body.add_or_replace_child(
            "left_ribcage",
            vec![Cube::new((90.0, 11.0), (-7.0, -11.0, -0.1), (9.0, 21.0, 0.0)).mirrored()],
            Pose::offset(7.0, -2.0, -4.0),
        );
        {
            let head = body.add_or_replace_child(
                "head",
                vec![Cube::new(
                    (0.0, 32.0),
                    (-8.0, -16.0, -5.0),
                    (16.0, 16.0, 10.0),
                )],
                Pose::offset(0.0, -13.0, 0.0),
            );
            head.add_or_replace_child(
                "right_tendril",
                vec![Cube::new(
                    (52.0, 32.0),
                    (-16.0, -13.0, 0.0),
                    (16.0, 16.0, 0.0),
                )],
                Pose::offset(-8.0, -12.0, 0.0),
            );
            head.add_or_replace_child(
                "left_tendril",
                vec![Cube::new((58.0, 0.0), (0.0, -13.0, 0.0), (16.0, 16.0, 0.0))],
                Pose::offset(8.0, -12.0, 0.0),
            );
        }
        body.add_or_replace_child(
            "right_arm",
            vec![Cube::new((44.0, 50.0), (-4.0, 0.0, -4.0), (8.0, 28.0, 8.0))],
            Pose::offset(-ARM_X, ARM_Y, ARM_Z),
        );
        body.add_or_replace_child(
            "left_arm",
            vec![Cube::new((0.0, 58.0), (-4.0, 0.0, -4.0), (8.0, 28.0, 8.0))],
            Pose::offset(ARM_X, ARM_Y, ARM_Z),
        );
    }
    bone.add_or_replace_child(
        "right_leg",
        vec![Cube::new((76.0, 48.0), (-3.1, 0.0, -3.0), (6.0, 13.0, 6.0))],
        Pose::offset(-5.9, -13.0, 0.0),
    );
    bone.add_or_replace_child(
        "left_leg",
        vec![Cube::new((76.0, 76.0), (-2.9, 0.0, -3.0), (6.0, 13.0, 6.0))],
        Pose::offset(5.9, -13.0, 0.0),
    );
    LayerDefinition::new(root, 128.0, 128.0)
}

#[derive(Clone, Copy, Debug)]
pub struct WardenParts {
    body: PartId,
    head: PartId,
    right_tendril: PartId,
    left_tendril: PartId,
    left_leg: PartId,
    right_leg: PartId,
    left_arm: PartId,
    right_arm: PartId,
}

impl WardenParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
            head: model.part("head"),
            right_tendril: model.part("right_tendril"),
            left_tendril: model.part("left_tendril"),
            left_leg: model.part("left_leg"),
            right_leg: model.part("right_leg"),
            left_arm: model.part("left_arm"),
            right_arm: model.part("right_arm"),
        }
    }
}

/// `WardenModel.setupAnim`: look, walk, idle wobble, then the tendrils.
pub fn setup_anim(
    parts: &WardenParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    tendril_animation: f32,
) {
    pose[parts.head].pose.rotation.x = living.x_rot.to_radians();
    pose[parts.head].pose.rotation.y = living.y_rot.to_radians();
    animate_walk(parts, pose, living.walk_position, living.walk_speed);
    animate_idle(parts, pose, living.age_in_ticks);
    let tendril = tendril_animation * ((living.age_in_ticks * 2.25).cos() * PI * 0.1);
    pose[parts.left_tendril].pose.rotation.x = tendril;
    pose[parts.right_tendril].pose.rotation.x = -tendril;
}

fn animate_walk(
    parts: &WardenParts,
    pose: &mut ModelPose,
    animation_pos: f32,
    animation_speed: f32,
) {
    let speed = (3.0 * animation_speed).min(0.5);
    let speed_small = speed.min(0.35);
    let adjusted = animation_pos * 0.8662;
    let (sine, cosine) = angle::sin_cos(adjusted);
    pose[parts.head].pose.rotation.z += 0.3 * sine * speed;
    pose[parts.head].pose.rotation.x += 1.2 * angle::cos(adjusted + HALF_PI) * speed_small;
    pose[parts.body].pose.rotation.z = 0.1 * sine * speed;
    pose[parts.body].pose.rotation.x = cosine * speed_small;
    pose[parts.left_leg].pose.rotation.x = cosine * speed;
    pose[parts.right_leg].pose.rotation.x = angle::cos(adjusted + PI) * speed;
    pose[parts.left_arm].pose.rotation.x = -(0.8 * cosine * speed);
    pose[parts.left_arm].pose.rotation.z = 0.0;
    pose[parts.right_arm].pose.rotation.x = -(0.8 * sine * speed);
    pose[parts.right_arm].pose.rotation.z = 0.0;
    pose[parts.left_arm].pose.rotation.y = 0.0;
    pose[parts.left_arm].pose.offset.x = ARM_X;
    pose[parts.left_arm].pose.offset.y = ARM_Y;
    pose[parts.left_arm].pose.offset.z = ARM_Z;
    pose[parts.right_arm].pose.rotation.y = 0.0;
    pose[parts.right_arm].pose.offset.x = -ARM_X;
    pose[parts.right_arm].pose.offset.y = ARM_Y;
    pose[parts.right_arm].pose.offset.z = ARM_Z;
}

fn animate_idle(parts: &WardenParts, pose: &mut ModelPose, age_in_ticks: f32) {
    let scaled = age_in_ticks * 0.1;
    let (sine, cosine) = angle::sin_cos(scaled);
    pose[parts.head].pose.rotation.z += 0.06 * cosine;
    pose[parts.head].pose.rotation.x += 0.06 * sine;
    pose[parts.body].pose.rotation.z += 0.025 * sine;
    pose[parts.body].pose.rotation.x += 0.025 * cosine;
}

/// Emerge, dig, roar and sniff follow `Pose`; attack and sonic boom are
pub fn tick_anim(anim: &mut [AnimationState], tick: i32, pose: i32, attack: bool, sonic: bool) {
    anim[EMERGE].animate_when(pose == POSE_EMERGING, tick);
    anim[DIG].animate_when(pose == POSE_DIGGING, tick);
    anim[ROAR].animate_when(pose == POSE_ROARING, tick);
    anim[SNIFF].animate_when(pose == POSE_SNIFFING, tick);
    anim[ATTACK].animate_when(attack, tick);
    anim[SONIC].animate_when(sonic, tick);
}

/// `WardenRenderer`: the body, then its `LivingEntityEmissiveLayer`s in
/// order, each translucent at full brightness and left out while its alpha
/// is nought: the bioluminescent layer, the two sets of spots pulsing in
/// turn, and the heart on each beat.
///
/// The tendrils' layer (the body texture on `WARDEN_TENDRILS`, at
/// `tendrilAnimation`) is missing: the tendrils' twitch from event 61 is
/// not tracked yet.
pub(super) fn push_layers(entity: &ClientEntity, age_in_ticks: f32, out: &mut Vec<LayerSpec>) {
    out.push(LayerSpec::plain(M::Warden, texture()));
    let mut emissive = |texture, alpha: f32| {
        if alpha > 1.0e-5 {
            out.push(LayerSpec::plain(M::Warden, texture).glowing().faded(alpha));
        }
    };
    emissive(bioluminescent_texture(), 1.0);
    emissive(pulsating_spots_1_texture(), spots_alpha(age_in_ticks, 0.0));
    emissive(pulsating_spots_2_texture(), spots_alpha(age_in_ticks, PI));
    emissive(
        heart_texture(),
        heart_alpha(age_in_ticks, entity.appearance.warden_anger),
    );
}

/// The spots' alpha: `max(0, cos(ageInTicks * 0.045 + phase) * 0.25)`.
fn spots_alpha(age_in_ticks: f32, phase: f32) -> f32 {
    (angle::cos(age_in_ticks * 0.045 + phase) * 0.25).max(0.0)
}

/// `Warden.getHeartAnimation`: 1 on the tick of a beat, fading over ten
/// ticks. A beat comes on each tick that `getHeartBeatDelay` divides: every
/// 40 ticks when calm, down to every 10 as `anger` reaches
/// `AngerLevel.ANGRY` (80). Vanilla counts down from the last beat; this
/// reads the same off the age, which differs only for the first beat after
/// the anger changes the delay.
pub fn heart_alpha(age_in_ticks: f32, anger: i32) -> f32 {
    let delay = 40 - ((anger as f32 / 80.0).clamp(0.0, 1.0) * 30.0).floor() as i32;
    let tick = age_in_ticks.floor();
    let partial = age_in_ticks - tick;
    let since = (tick as i32).rem_euclid(delay);
    let before = (10 - since).max(0) as f32;
    let now = (9 - since).max(0) as f32;
    (before + partial * (now - before)) / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::keyframe::KeyframeAnimation;

    #[test]
    fn the_heart_beats_faster_with_anger() {
        // The tick of a beat, then fading: `lerp(partial, 10 - k, 9 - k) / 10`.
        assert_eq!(heart_alpha(40.0, 0), 1.0);
        assert!((heart_alpha(40.5, 0) - 0.95).abs() < 1e-6);
        assert!((heart_alpha(45.0, 0) - 0.5).abs() < 1e-6);
        assert_eq!(heart_alpha(50.0, 0), 0.0);
        assert_eq!(heart_alpha(70.0, 0), 0.0);
        // Angry: every 10 ticks, so it never quite goes out.
        assert_eq!(heart_alpha(70.0, 80), 1.0);
        assert!(heart_alpha(79.9, 80) > 0.0);
        // Half way: `40 - floor(0.5 * 30)` = 25.
        assert_eq!(heart_alpha(50.0, 40), 1.0);
        assert_eq!(heart_alpha(40.0, 40), 0.0);
    }

    #[test]
    fn the_spots_pulse_in_turn_and_never_together() {
        for age in (0..400).map(|t| t as f32 * 0.5) {
            let (one, two) = (spots_alpha(age, 0.0), spots_alpha(age, PI));
            assert!(one <= 0.25 && two <= 0.25);
            assert!(one == 0.0 || two == 0.0, "{age}: {one} {two}");
        }
        assert!((spots_alpha(0.0, 0.0) - 0.25).abs() < 1e-3);
    }

    #[test]
    fn the_animations_bake_and_the_arms_reset() {
        let model = Model::bake(&layer());
        for (definition, _) in ANIMATIONS {
            KeyframeAnimation::bake(&model, definition).unwrap_or_else(|e| panic!("{e}"));
        }
        let parts = WardenParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState {
                walk_speed: 0.4,
                walk_position: 1.0,
                age_in_ticks: 8.0,
                ..Default::default()
            },
            1.0,
        );
        assert!((pose[parts.left_arm].pose.offset.x - ARM_X).abs() < 1e-5);
        assert!((pose[parts.right_arm].pose.offset.x + ARM_X).abs() < 1e-5);
        assert_eq!(
            bioluminescent_texture(),
            "minecraft:entity/warden/warden_bioluminescent_layer"
        );
    }
}
