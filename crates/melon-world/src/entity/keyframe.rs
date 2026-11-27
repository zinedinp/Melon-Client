//! Vanilla's keyframe animations (`AnimationDefinition`, `AnimationChannel`, `Keyframe`).

use glam::Vec3;

use super::model::{Model, ModelPose, PartId};

/// What a channel moves (`AnimationChannel.Targets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// `ModelPart.offsetPos`, in model pixels with Y down.
    Position,
    /// `ModelPart.offsetRotation`, radians ([`degree_vec`]).
    Rotation,
    /// `ModelPart.offsetScale`, added to a scale of 1 ([`scale_vec`]).
    Scale,
}

/// How the value between two keyframes is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    /// `Interpolations.LINEAR`.
    Linear,
    /// `Interpolations.CATMULLROM`: a smooth curve through the keyframes' values.
    CatmullRom,
}

/// Any other curve.
pub type Curve = fn(&Sample) -> Vec3;

/// What a [`Curve`] gets.
pub struct Sample<'a> {
    pub alpha: f32,
    pub keyframes: &'a [Keyframe],
    pub prev: usize,
    pub next: usize,
    pub target_scale: f32,
}

/// `Keyframe`.
#[derive(Clone, Copy, Debug)]
pub struct Keyframe {
    pub time: f32,
    pub pre: Vec3,
    pub post: Vec3,
    pub interpolation: Interpolation,
}

impl Keyframe {
    /// One value, interpolated linearly.
    pub const fn linear(time: f32, value: Vec3) -> Self {
        Self::new(time, value, Interpolation::Linear)
    }

    /// One value, on a Catmull-Rom curve.
    pub const fn catmull_rom(time: f32, value: Vec3) -> Self {
        Self::new(time, value, Interpolation::CatmullRom)
    }

    pub const fn new(time: f32, value: Vec3, interpolation: Interpolation) -> Self {
        Self {
            time,
            pre: value,
            post: value,
            interpolation,
        }
    }

    /// The value jumps at `time`.
    pub const fn jump(time: f32, pre: Vec3, post: Vec3, interpolation: Interpolation) -> Self {
        Self {
            time,
            pre,
            post,
            interpolation,
        }
    }
}

/// `AnimationChannel`: one target of one part over time.
#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub target: Target,
    pub keyframes: &'static [Keyframe],
    /// Replaces the keyframes' interpolation, if set.
    pub curve: Option<Curve>,
}

impl Channel {
    pub const fn new(target: Target, keyframes: &'static [Keyframe]) -> Self {
        Self {
            target,
            keyframes,
            curve: None,
        }
    }

    /// The same, on a curve of your own.
    pub const fn with_curve(self, curve: Curve) -> Self {
        Self {
            curve: Some(curve),
            ..self
        }
    }
}

/// A part's channels, by the part's name.
#[derive(Clone, Copy, Debug)]
pub struct Bone {
    pub name: &'static str,
    pub channels: &'static [Channel],
}

impl Bone {
    pub const fn new(name: &'static str, channels: &'static [Channel]) -> Self {
        Self { name, channels }
    }
}

/// `AnimationDefinition`.
#[derive(Clone, Copy, Debug)]
pub struct AnimationDefinition {
    pub length_in_seconds: f32,
    pub looping: bool,
    pub bones: &'static [Bone],
}

/// `KeyframeAnimations.posVec`: Y is down in model space.
pub const fn pos_vec(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, -y, z)
}

/// `KeyframeAnimations.degreeVec`.
pub const fn degree_vec(x: f32, y: f32, z: f32) -> Vec3 {
    const RADIANS: f32 = (crate::math::angle::double::PI / 180.0) as f32;
    Vec3::new(x * RADIANS, y * RADIANS, z * RADIANS)
}

/// `KeyframeAnimations.scaleVec`: the offset from a scale of 1.
pub const fn scale_vec(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new((x - 1.0) as f32, (y - 1.0) as f32, (z - 1.0) as f32)
}

/// `Mth.catmullrom`.
fn catmull_rom(alpha: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * alpha
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * alpha * alpha
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * alpha * alpha * alpha)
}

impl Interpolation {
    fn apply(self, sample: &Sample) -> Vec3 {
        let Sample {
            alpha,
            keyframes,
            prev,
            next,
            target_scale,
        } = *sample;
        match self {
            Self::Linear => keyframes[prev].post.lerp(keyframes[next].pre, alpha) * target_scale,
            Self::CatmullRom => {
                let last = keyframes.len() - 1;
                let p0 = keyframes[prev.saturating_sub(1)].post;
                let p1 = keyframes[prev].post;
                let p2 = keyframes[next].post;
                let p3 = keyframes[(next + 1).min(last)].post;
                Vec3::new(
                    catmull_rom(alpha, p0.x, p1.x, p2.x, p3.x),
                    catmull_rom(alpha, p0.y, p1.y, p2.y, p3.y),
                    catmull_rom(alpha, p0.z, p1.z, p2.z, p3.z),
                ) * target_scale
            }
        }
    }
}

/// `AnimationState`: when an animation started, in the entity's ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct AnimationState {
    start_tick: Option<i32>,
}

impl AnimationState {
    pub fn start(&mut self, tick: i32) {
        self.start_tick = Some(tick);
    }

    pub fn start_if_stopped(&mut self, tick: i32) {
        if self.start_tick.is_none() {
            self.start(tick);
        }
    }

    /// Runs while `condition` holds and stops when it ends.
    pub fn animate_when(&mut self, condition: bool, tick: i32) {
        if condition {
            self.start_if_stopped(tick);
        } else {
            self.stop();
        }
    }

    pub fn stop(&mut self) {
        self.start_tick = None;
    }

    pub fn is_started(&self) -> bool {
        self.start_tick.is_some()
    }

    /// Moves the start back, as if `ticks` (at `time_scale`) had passed.
    pub fn fast_forward(&mut self, ticks: i32, time_scale: f32) {
        if let Some(start) = &mut self.start_tick {
            *start -= (ticks as f32 * time_scale) as i32;
        }
    }

    /// `getTimeInMillis`, if it runs.
    pub fn time_in_millis(&self, age_in_ticks: f32) -> Option<i64> {
        self.start_tick
            .map(|start| ((age_in_ticks - start as f32) * 50.0) as i64)
    }
}

/// One channel bound to the part it moves.
#[derive(Clone, Debug)]
enum Entry {
    /// A channel of one keyframe never changes.
    Constant {
        part: PartId,
        target: Target,
        value: Vec3,
    },
    Curve {
        part: PartId,
        target: Target,
        keyframes: &'static [Keyframe],
        curve: Option<Curve>,
    },
}

fn add(pose: &mut ModelPose, part: PartId, target: Target, value: Vec3) {
    let part = &mut pose[part].pose;
    match target {
        Target::Position => part.offset += value,
        Target::Rotation => part.rotation += value,
        Target::Scale => part.scale += value,
    }
}

impl Entry {
    fn new(part: PartId, channel: &Channel) -> Self {
        match (channel.keyframes, channel.curve) {
            ([only], None) => Self::Constant {
                part,
                target: channel.target,
                value: only.post,
            },
            (keyframes, curve) => Self::Curve {
                part,
                target: channel.target,
                keyframes,
                curve,
            },
        }
    }

    fn apply(&self, seconds: f32, target_scale: f32, pose: &mut ModelPose) {
        let (part, target, keys, curve) = match self {
            Self::Constant {
                part,
                target,
                value,
            } => return add(pose, *part, *target, *value * target_scale),
            Self::Curve {
                part,
                target,
                keyframes,
                curve,
            } => (*part, *target, *keyframes, *curve),
        };
        // Past every key not after `seconds`, NaN included (`!(seconds <= time)`).
        let first_after = keys.partition_point(|k| {
            !matches!(
                seconds.partial_cmp(&k.time),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            )
        });
        let prev = first_after.saturating_sub(1);
        let next = (prev + 1).min(keys.len() - 1);
        let alpha = if next == prev {
            0.0
        } else {
            ((seconds - keys[prev].time) / (keys[next].time - keys[prev].time)).clamp(0.0, 1.0)
        };
        let sample = Sample {
            alpha,
            keyframes: keys,
            prev,
            next,
            target_scale,
        };
        let value = match curve {
            Some(curve) => curve(&sample),
            None => keys[next].interpolation.apply(&sample),
        };
        add(pose, part, target, value);
    }
}

/// A definition that names a part its model doesn't have.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingPart(pub &'static str);

impl std::fmt::Display for MissingPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cannot animate {}, which does not exist in model",
            self.0
        )
    }
}

impl std::error::Error for MissingPart {}

/// `KeyframeAnimation`: a definition baked for a model.
#[derive(Clone, Debug)]
pub struct KeyframeAnimation {
    definition: &'static AnimationDefinition,
    entries: Vec<Entry>,
}

impl KeyframeAnimation {
    /// `AnimationDefinition.bake`: finds the parts the definition names.
    pub fn bake(
        model: &Model,
        definition: &'static AnimationDefinition,
    ) -> Result<Self, MissingPart> {
        let mut entries = Vec::new();
        for bone in definition.bones {
            let part = model.find(bone.name).ok_or(MissingPart(bone.name))?;
            for channel in bone.channels {
                if !channel.keyframes.is_empty() {
                    entries.push(Entry::new(part, channel));
                }
            }
        }
        Ok(Self {
            definition,
            entries,
        })
    }

    pub fn definition(&self) -> &'static AnimationDefinition {
        self.definition
    }

    /// `applyStatic`.
    pub fn apply_static(&self, pose: &mut ModelPose) {
        self.apply(pose, 0, 1.0);
    }

    /// `applyWalk`: driven by the walk animation.
    pub fn apply_walk(
        &self,
        pose: &mut ModelPose,
        walk_position: f32,
        walk_speed: f32,
        speed_factor: f32,
        scale_factor: f32,
    ) {
        let millis = (walk_position * 50.0 * speed_factor) as i64;
        self.apply(pose, millis, (walk_speed * scale_factor).min(1.0));
    }

    /// `apply(state, time, speed)`.
    pub fn apply_state(
        &self,
        pose: &mut ModelPose,
        state: &AnimationState,
        age_in_ticks: f32,
        speed_factor: f32,
    ) {
        if let Some(millis) = state.time_in_millis(age_in_ticks) {
            self.apply(pose, (millis as f32 * speed_factor) as i64, 1.0);
        }
    }

    /// `apply(millis, targetScale)`.
    pub fn apply(&self, pose: &mut ModelPose, millis_since_start: i64, target_scale: f32) {
        let seconds = elapsed_seconds(self.definition, millis_since_start);
        for entry in &self.entries {
            entry.apply(seconds, target_scale, pose);
        }
    }
}

/// `getElapsedSeconds`: a loop starts over at its length.
fn elapsed_seconds(definition: &AnimationDefinition, millis: i64) -> f32 {
    let seconds = millis as f32 / 1000.0;
    if definition.looping {
        seconds % definition.length_in_seconds
    } else {
        seconds
    }
}

/// One animation of a [`KeyframeSet`].
#[derive(Clone, Debug)]
struct Run {
    slot: usize,
    definition: &'static AnimationDefinition,
    start: usize,
    end: usize,
}

/// All the animations of one model, baked into one list of channels.
#[derive(Clone, Debug, Default)]
pub struct KeyframeSet {
    entries: Vec<Entry>,
    runs: Vec<Run>,
}

impl KeyframeSet {
    /// Bakes `animations` (a definition and the clock slot that plays it)
    pub fn bake(
        model: &Model,
        animations: &[(&'static AnimationDefinition, usize)],
    ) -> Result<Self, MissingPart> {
        let mut set = Self::default();
        for &(definition, slot) in animations {
            let start = set.entries.len();
            let baked = KeyframeAnimation::bake(model, definition)?;
            set.entries.extend(baked.entries);
            set.runs.push(Run {
                slot,
                definition,
                start,
                end: set.entries.len(),
            });
        }
        Ok(set)
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Every running animation at its time, added to the pose in the order they were baked.
    pub fn apply(&self, pose: &mut ModelPose, states: &[AnimationState], age_in_ticks: f32) {
        for run in &self.runs {
            let Some(millis) = states[run.slot].time_in_millis(age_in_ticks) else {
                continue;
            };
            let seconds = elapsed_seconds(run.definition, millis);
            for entry in &self.entries[run.start..run.end] {
                entry.apply(seconds, 1.0, pose);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::model::{LayerDefinition, PartDefinition};
    use crate::model_part::{Cube, Pose};

    fn model() -> Model {
        let mut root = PartDefinition::root();
        let body = root.add_or_replace_child(
            "body",
            vec![Cube::new((0.0, 0.0), (-1.0, -1.0, -1.0), (2.0, 2.0, 2.0))],
            Pose::offset(0.0, 10.0, 0.0),
        );
        body.add_or_replace_child("head", Vec::new(), Pose::ZERO);
        Model::bake(&LayerDefinition::new(root, 16.0, 16.0))
    }

    const RISE: [Keyframe; 3] = [
        Keyframe::linear(0.0, Vec3::ZERO),
        Keyframe::linear(1.0, Vec3::new(10.0, 0.0, 0.0)),
        Keyframe::linear(2.0, Vec3::new(10.0, 20.0, 0.0)),
    ];
    const CHANNELS: [Channel; 1] = [Channel::new(Target::Position, &RISE)];
    const BONES: [Bone; 1] = [Bone::new("body", &CHANNELS)];
    const ONE_SHOT: AnimationDefinition = AnimationDefinition {
        length_in_seconds: 2.0,
        looping: false,
        bones: &BONES,
    };
    const LOOP: AnimationDefinition = AnimationDefinition {
        looping: true,
        ..ONE_SHOT
    };

    fn offset(animation: &KeyframeAnimation, millis: i64, scale: f32) -> Vec3 {
        let model = model();
        let mut pose = model.initial_pose();
        animation.apply(&mut pose, millis, scale);
        pose[model.part("body")].pose.offset - Vec3::new(0.0, 10.0, 0.0)
    }

    #[test]
    fn values_are_found_between_keyframes_and_held_after_the_last() {
        let anim = KeyframeAnimation::bake(&model(), &ONE_SHOT).unwrap();
        assert_eq!(offset(&anim, 0, 1.0), Vec3::ZERO);
        assert_eq!(offset(&anim, 500, 1.0), Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(offset(&anim, 1500, 1.0), Vec3::new(10.0, 10.0, 0.0));
        assert_eq!(offset(&anim, 9000, 1.0), Vec3::new(10.0, 20.0, 0.0));
        assert_eq!(offset(&anim, -500, 1.0), Vec3::ZERO);
    }

    #[test]
    fn a_looping_animation_starts_over_and_the_target_scale_fades_it() {
        let anim = KeyframeAnimation::bake(&model(), &LOOP).unwrap();
        assert_eq!(offset(&anim, 2500, 1.0), offset(&anim, 500, 1.0));
        assert_eq!(offset(&anim, 500, 0.5), Vec3::new(2.5, 0.0, 0.0));
        assert_eq!(offset(&anim, 500, 0.0), Vec3::ZERO);
    }

    #[test]
    fn animations_add_to_the_pose_and_to_each_other() {
        let model = model();
        let anim = KeyframeAnimation::bake(&model, &ONE_SHOT).unwrap();
        let mut pose = model.initial_pose();
        anim.apply(&mut pose, 500, 1.0);
        anim.apply(&mut pose, 500, 1.0);
        assert_eq!(
            pose[model.part("body")].pose.offset,
            Vec3::new(10.0, 10.0, 0.0)
        );
    }

    #[test]
    fn a_state_runs_from_its_start_and_a_stopped_one_does_nothing() {
        let model = model();
        let anim = KeyframeAnimation::bake(&model, &ONE_SHOT).unwrap();
        let mut state = AnimationState::default();
        let mut pose = model.initial_pose();
        anim.apply_state(&mut pose, &state, 100.0, 1.0);
        assert_eq!(pose, model.initial_pose());
        state.start(100);
        anim.apply_state(&mut pose, &state, 110.0, 1.0);
        assert_eq!(
            pose[model.part("body")].pose.offset,
            Vec3::new(5.0, 10.0, 0.0)
        );
        state.stop();
        assert!(!state.is_started());
        state.animate_when(true, 7);
        state.animate_when(true, 99);
        assert_eq!(state.time_in_millis(8.0), Some(50));
        state.fast_forward(20, 1.0);
        assert_eq!(state.time_in_millis(8.0), Some(1050));
    }

    #[test]
    fn a_walk_cycle_follows_position_and_speed() {
        let model = model();
        let anim = KeyframeAnimation::bake(&model, &LOOP).unwrap();
        let mut pose = model.initial_pose();
        anim.apply_walk(&mut pose, 10.0, 0.5, 2.0, 1.0);
        assert_eq!(
            pose[model.part("body")].pose.offset,
            Vec3::new(5.0, 10.0, 0.0)
        );
    }

    #[test]
    fn a_catmull_rom_curve_passes_through_its_keyframes() {
        static CURVE: [Keyframe; 4] = [
            Keyframe::catmull_rom(0.0, Vec3::new(0.0, 0.0, 0.0)),
            Keyframe::catmull_rom(1.0, Vec3::new(4.0, 0.0, 0.0)),
            Keyframe::catmull_rom(2.0, Vec3::new(4.0, 8.0, 0.0)),
            Keyframe::catmull_rom(3.0, Vec3::new(0.0, 8.0, 0.0)),
        ];
        static CH: [Channel; 1] = [Channel::new(Target::Position, &CURVE)];
        static B: [Bone; 1] = [Bone::new("body", &CH)];
        static DEF: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 3.0,
            looping: false,
            bones: &B,
        };
        let anim = KeyframeAnimation::bake(&model(), &DEF).unwrap();
        for (millis, at) in [(0, CURVE[0]), (1000, CURVE[1]), (2000, CURVE[2])] {
            let got = offset(&anim, millis, 1.0);
            assert!((got - at.post).length() < 1e-4, "{millis}: {got}");
        }
        let middle = offset(&anim, 1500, 1.0);
        assert!((middle - Vec3::new(4.0, 4.0, 0.0)).length() > 1e-3);
    }

    #[test]
    fn a_jump_arrives_and_leaves_with_different_values() {
        static KEYS: [Keyframe; 3] = [
            Keyframe::linear(0.0, Vec3::ZERO),
            Keyframe::jump(
                1.0,
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(8.0, 0.0, 0.0),
                Interpolation::Linear,
            ),
            Keyframe::linear(2.0, Vec3::new(10.0, 0.0, 0.0)),
        ];
        static CH: [Channel; 1] = [Channel::new(Target::Position, &KEYS)];
        static B: [Bone; 1] = [Bone::new("body", &CH)];
        static DEF: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 2.0,
            looping: false,
            bones: &B,
        };
        let anim = KeyframeAnimation::bake(&model(), &DEF).unwrap();
        assert_eq!(offset(&anim, 999, 1.0).x.round(), 2.0);
        assert_eq!(offset(&anim, 1001, 1.0).x.round(), 8.0);
    }

    #[test]
    fn a_custom_curve_gets_the_sample_and_scale_and_other_targets_apply() {
        fn eased(sample: &Sample) -> Vec3 {
            let a = sample.alpha * sample.alpha;
            sample.keyframes[sample.prev]
                .post
                .lerp(sample.keyframes[sample.next].pre, a)
                * sample.target_scale
        }
        static KEYS: [Keyframe; 2] = [
            Keyframe::linear(0.0, Vec3::ZERO),
            Keyframe::linear(1.0, Vec3::new(8.0, 0.0, 0.0)),
        ];
        static CH: [Channel; 2] = [
            Channel::new(Target::Rotation, &KEYS).with_curve(eased),
            Channel::new(Target::Scale, &KEYS).with_curve(eased),
        ];
        static B: [Bone; 1] = [Bone::new("head", &CH)];
        static DEF: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 1.0,
            looping: false,
            bones: &B,
        };
        let model = model();
        let anim = KeyframeAnimation::bake(&model, &DEF).unwrap();
        let mut pose = model.initial_pose();
        anim.apply(&mut pose, 500, 1.0);
        let head = pose[model.part("head")].pose;
        assert_eq!(head.rotation, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(head.scale, Vec3::new(3.0, 1.0, 1.0));
    }

    #[test]
    fn a_definition_for_a_part_the_model_lacks_is_refused() {
        static B: [Bone; 1] = [Bone::new("wing", &CHANNELS)];
        static DEF: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 1.0,
            looping: false,
            bones: &B,
        };
        assert_eq!(
            KeyframeAnimation::bake(&model(), &DEF).unwrap_err(),
            MissingPart("wing")
        );
    }

    #[test]
    fn a_channel_of_one_keyframe_is_a_constant_that_only_the_target_scale_changes() {
        const POSE: [Keyframe; 1] = [Keyframe::linear(0.0, Vec3::new(0.0, 4.0, 0.0))];
        const CH: [Channel; 1] = [Channel::new(Target::Position, &POSE)];
        const B: [Bone; 1] = [Bone::new("body", &CH)];
        const DEF: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 0.5,
            looping: true,
            bones: &B,
        };
        let anim = KeyframeAnimation::bake(&model(), &DEF).unwrap();
        assert!(matches!(anim.entries[0], Entry::Constant { .. }));
        assert_eq!(offset(&anim, 0, 1.0), Vec3::new(0.0, 4.0, 0.0));
        assert_eq!(offset(&anim, 12_345, 1.0), Vec3::new(0.0, 4.0, 0.0));
        assert_eq!(offset(&anim, 777, 0.5), Vec3::new(0.0, 2.0, 0.0));
    }

    #[test]
    fn the_data_stays_small() {
        use crate::entity::animations::ALL;
        assert!(size_of::<Keyframe>() <= 32, "{}", size_of::<Keyframe>());
        let keyframes: usize = ALL
            .iter()
            .flat_map(|(_, def)| def.bones)
            .flat_map(|bone| bone.channels)
            .map(|channel| channel.keyframes.len())
            .sum();
        assert!(keyframes * size_of::<Keyframe>() < 170_000);
    }

    #[test]
    fn every_vanilla_definition_is_well_formed() {
        use crate::entity::animations::ALL;
        assert!(ALL.len() >= 70, "{}", ALL.len());
        for (name, def) in ALL {
            assert!(def.length_in_seconds >= 0.0, "{name}");
            assert!(!def.bones.is_empty(), "{name}");
            for bone in def.bones {
                for channel in bone.channels {
                    assert!(!channel.keyframes.is_empty(), "{name} {}", bone.name);
                    for pair in channel.keyframes.windows(2) {
                        assert!(pair[0].time <= pair[1].time, "{name} {}", bone.name);
                    }
                    let last = channel.keyframes.last().unwrap().time;
                    assert!(last <= def.length_in_seconds + 1e-4, "{name} {}", bone.name);
                }
            }
        }
    }

    #[test]
    fn a_set_plays_only_the_animations_whose_clock_runs() {
        const SHIFT: [Keyframe; 1] = [Keyframe::linear(0.0, Vec3::new(1.0, 0.0, 0.0))];
        const SHIFT_CH: [Channel; 1] = [Channel::new(Target::Position, &SHIFT)];
        const SHIFT_BONE: [Bone; 1] = [Bone::new("body", &SHIFT_CH)];
        const SHIFTED: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 1.0,
            looping: true,
            bones: &SHIFT_BONE,
        };
        let model = model();
        let set = KeyframeSet::bake(&model, &[(&ONE_SHOT, 0), (&SHIFTED, 1)]).unwrap();
        assert!(!set.is_empty());
        let mut states = [AnimationState::default(); 2];
        let at = |states: &[AnimationState; 2]| {
            let mut pose = model.initial_pose();
            set.apply(&mut pose, states, 10.0);
            pose[model.part("body")].pose.offset - Vec3::new(0.0, 10.0, 0.0)
        };
        assert_eq!(at(&states), Vec3::ZERO);
        states[1].start(0);
        assert_eq!(at(&states), Vec3::new(1.0, 0.0, 0.0));
        states[0].start(0);
        assert_eq!(at(&states), Vec3::new(6.0, 0.0, 0.0));
        let single = KeyframeAnimation::bake(&model, &ONE_SHOT).unwrap();
        assert_eq!(offset(&single, 500, 1.0), Vec3::new(5.0, 0.0, 0.0),);
    }

    #[test]
    fn a_looping_animation_of_no_length_holds_its_last_keyframe() {
        const HELD: AnimationDefinition = AnimationDefinition {
            length_in_seconds: 0.0,
            looping: true,
            ..ONE_SHOT
        };
        let anim = KeyframeAnimation::bake(&model(), &HELD).unwrap();
        assert_eq!(offset(&anim, 300, 1.0), Vec3::new(10.0, 20.0, 0.0));
    }

    #[test]
    fn the_vector_helpers_are_vanillas() {
        assert_eq!(pos_vec(1.0, 2.0, 3.0), Vec3::new(1.0, -2.0, 3.0));
        assert!(
            (degree_vec(180.0, 0.0, 90.0)
                - Vec3::new(crate::math::angle::PI, 0.0, crate::math::angle::HALF_PI))
            .length()
                < 1e-6
        );
        assert_eq!(scale_vec(1.5, 1.0, 0.5), Vec3::new(0.5, 0.0, -0.5));
    }
}
