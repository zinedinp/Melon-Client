//! `AdultRabbitModel` and `BabyRabbitModel`: hopping and the idle head tilt

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::world::entity::animations::{baby_rabbit, rabbit};
use crate::world::entity::keyframe::{AnimationDefinition, AnimationState};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::magic_name::Magic;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// `EntityEvent.RABBIT_JUMP_ANIMATION`.
pub const EVENT_JUMP: u8 = 1;

/// The animation state slots (`PoseInput::anim`).
pub const HOP: usize = 0;
pub const IDLE: usize = 1;

pub static ANIMATIONS: [(&AnimationDefinition, usize); 2] =
    [(&rabbit::HOP, HOP), (&rabbit::IDLE_HEAD_TILT, IDLE)];
pub static BABY_ANIMATIONS: [(&AnimationDefinition, usize); 2] = [
    (&baby_rabbit::HOP, HOP),
    (&baby_rabbit::IDLE_HEAD_TILT, IDLE),
];

/// `Rabbit.Variant`'s ids and their texture names, `Variant.EVIL` last.
const VARIANTS: [(i32, &str); 7] = [
    (0, "brown"),
    (1, "white"),
    (2, "black"),
    (3, "white_splotched"),
    (4, "gold"),
    (5, "salt"),
    (99, "caerbannog"),
];

/// A stand-in for the coat of a rabbit named `Toast`, which no variant id has.
pub const TOAST: i32 = -1;

/// The texture of a rabbit of this variant (unknown ids are brown,
pub fn texture(variant: i32, baby: bool) -> &'static str {
    if variant == TOAST {
        return if baby {
            "minecraft:entity/rabbit/rabbit_toast_baby"
        } else {
            "minecraft:entity/rabbit/rabbit_toast"
        };
    }
    const ADULT: [&str; 7] = [
        "minecraft:entity/rabbit/rabbit_brown",
        "minecraft:entity/rabbit/rabbit_white",
        "minecraft:entity/rabbit/rabbit_black",
        "minecraft:entity/rabbit/rabbit_white_splotched",
        "minecraft:entity/rabbit/rabbit_gold",
        "minecraft:entity/rabbit/rabbit_salt",
        "minecraft:entity/rabbit/rabbit_caerbannog",
    ];
    const BABY: [&str; 7] = [
        "minecraft:entity/rabbit/rabbit_brown_baby",
        "minecraft:entity/rabbit/rabbit_white_baby",
        "minecraft:entity/rabbit/rabbit_black_baby",
        "minecraft:entity/rabbit/rabbit_white_splotched_baby",
        "minecraft:entity/rabbit/rabbit_gold_baby",
        "minecraft:entity/rabbit/rabbit_salt_baby",
        "minecraft:entity/rabbit/rabbit_caerbannog_baby",
    ];
    let index = VARIANTS
        .iter()
        .position(|&(id, _)| id == variant)
        .unwrap_or(0);
    if baby { BABY[index] } else { ADULT[index] }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-4.0, -6.0, -9.0), (8.0, 6.0, 10.0))],
        Pose::offset_rotation((0.0, 23.0, 4.0), (-crate::math::angle::EIGHTH_PI, 0.0, 0.0)),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new(
            (20.0, 16.0),
            (-2.0, -3.0084, -1.0125),
            (4.0, 4.0, 4.0),
        )],
        Pose::offset(0.0, -4.9916, 0.0125),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 16.0), (-2.5, -3.0, -4.0), (5.0, 5.0, 5.0))],
        Pose::offset_rotation(
            (0.0, -5.2929, -8.1213),
            (crate::math::angle::EIGHTH_PI, 0.0, 0.0),
        ),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new(
            (32.0, 0.0),
            (-1.0, -4.2929, -0.1213),
            (2.0, 5.0, 1.0),
        )],
        Pose::offset(1.5, -3.7071, -0.8787),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new(
            (26.0, 0.0),
            (-1.0, -4.2929, -0.1213),
            (2.0, 5.0, 1.0),
        )],
        Pose::offset(-1.5, -3.7071, -0.8787),
    );
    let front_legs =
        body.add_or_replace_child("frontlegs", Vec::new(), Pose::offset(0.0, -1.5349, -6.3108));
    front_legs.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((36.0, 18.0), (-0.9, -1.0, -0.9), (2.0, 4.0, 2.0))],
        Pose::offset_rotation(
            (-2.0, 1.9239, 0.3827),
            (crate::math::angle::EIGHTH_PI, 0.0, 0.0),
        ),
    );
    front_legs.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((44.0, 18.0), (-1.0, -1.0, -1.0), (2.0, 4.0, 2.0))],
        Pose::offset_rotation(
            (2.0, 1.9239, 0.4827),
            (crate::math::angle::EIGHTH_PI, 0.0, 0.0),
        ),
    );
    let back_legs = root.add_or_replace_child("backlegs", Vec::new(), Pose::offset(0.0, 23.0, 4.0));
    let right =
        back_legs.add_or_replace_child("right_hind_leg", Vec::new(), Pose::offset(-3.0, 0.5, 0.0));
    right.add_or_replace_child(
        "right_haunch",
        vec![Cube::new((20.0, 24.0), (-1.0, 0.0, -5.0), (2.0, 1.0, 6.0))],
        Pose::offset_rotation((0.0, -0.5, 0.0), (0.0, crate::math::angle::EIGHTH_PI, 0.0)),
    );
    let left =
        back_legs.add_or_replace_child("left_hind_leg", Vec::new(), Pose::offset(3.0, 0.5, 0.0));
    left.add_or_replace_child(
        "left_haunch",
        vec![Cube::new((36.0, 24.0), (-1.0, 0.0, -5.0), (2.0, 1.0, 6.0))],
        Pose::offset_rotation((0.0, -0.5, 0.0), (0.0, -crate::math::angle::EIGHTH_PI, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child("body", Vec::new(), Pose::offset(0.0, 23.0, 1.6));
    body.add_or_replace_child(
        "body_r1",
        vec![Cube::new((0.0, 8.0), (-2.0, -2.0, -3.0), (4.0, 3.0, 6.0))],
        Pose::offset_rotation((0.0, -2.0, -1.6), (-crate::math::angle::SIXTH_PI, 0.0, 0.0)),
    );
    let tail = body.add_or_replace_child("tail", Vec::new(), Pose::offset(0.0, -2.2, 2.0));
    tail.add_or_replace_child(
        "tail_r1",
        vec![Cube::new(
            (0.0, 21.0),
            (-1.4, -2.0268, -1.0177),
            (3.0, 3.0, 3.0),
        )],
        Pose::offset_rotation((-0.1, 0.0, 0.0), (-crate::math::angle::SIXTH_PI, 0.0, 0.0)),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-2.5, -3.0, -3.0), (5.0, 4.0, 4.0))],
        Pose::offset(0.0, -5.0, -2.6),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((18.0, 0.0), (-1.0, -3.5, -0.5), (2.0, 4.0, 1.0))],
        Pose::offset(-1.5, -3.5, -0.5),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((24.0, 0.0), (-1.0, -3.5, -0.5), (2.0, 4.0, 1.0))],
        Pose::offset(1.5, -3.5, -0.5),
    );
    let front_legs =
        body.add_or_replace_child("frontlegs", Vec::new(), Pose::offset(0.0, -2.5, -2.6));
    for (name, inner, x, u) in [
        ("left_front_leg", "left_front_leg_r1", 1.0, 18.0),
        ("right_front_leg", "right_front_leg_r1", -1.0, 14.0),
    ] {
        let leg = front_legs.add_or_replace_child(
            name,
            Vec::new(),
            Pose::offset_rotation((x, 1.0, -0.5), (crate::math::angle::EIGHTH_PI, 0.0, 0.0)),
        );
        leg.add_or_replace_child(
            inner,
            vec![Cube::new((u, 8.0), (-0.5, -1.5, -0.5), (1.0, 3.0, 1.0))],
            Pose::offset_rotation((0.0, 1.0, 0.0), (-crate::math::angle::EIGHTH_PI, 0.0, 0.0)),
        );
    }
    let back_legs = root.add_or_replace_child("backlegs", Vec::new(), Pose::offset(0.0, 23.0, 2.0));
    let left = back_legs.add_or_replace_child(
        "left_hind_leg",
        Vec::new(),
        Pose::offset_rotation((1.5, 0.5, 0.5), (0.0, crate::math::angle::PI, 0.0)),
    );
    left.add_or_replace_child(
        "left_haunch",
        vec![Cube::new((10.0, 17.0), (-2.0, -0.5, 0.0), (2.0, 1.0, 3.0))],
        Pose::offset_rotation((1.0, 0.0, 0.5), (0.0, -crate::math::angle::QUARTER_PI, 0.0)),
    );
    let right = back_legs.add_or_replace_child(
        "right_hind_leg",
        Vec::new(),
        Pose::offset_rotation((-1.5, 0.5, 0.5), (0.0, crate::math::angle::PI, 0.0)),
    );
    right.add_or_replace_child(
        "right_haunch",
        vec![Cube::new((0.0, 17.0), (-2.0, -0.5, 0.0), (2.0, 1.0, 3.0))],
        Pose::offset_rotation((0.5, 0.0, -0.9), (0.0, crate::math::angle::QUARTER_PI, 0.0)),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct RabbitParts {
    head: PartId,
}

impl RabbitParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
        }
    }
}

/// `RabbitModel.setupAnim` before the animations: the head follows the look
pub fn setup_anim(
    parts: &RabbitParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    idle: &AnimationState,
) {
    if !idle.is_started() {
        let head = &mut pose[parts.head].pose;
        head.rotation.y = state.y_rot.to_radians();
        head.rotation.x = state.x_rot.to_radians();
    }
}

/// `Rabbit`'s client-side timers: a hop lasts 15 ticks from the server's
#[derive(Clone, Copy, Debug)]
pub struct RabbitClock {
    jump_ticks: i32,
    jump_duration: i32,
    idle_timeout: i32,
    /// The client's own dice: the tilt needs no agreement with the server.
    rng: u32,
}

impl RabbitClock {
    /// `Rabbit.idleAnimationTimeout`'s start, from the entity's id.
    pub fn new(seed: i32) -> Self {
        let mut clock = Self {
            jump_ticks: 0,
            jump_duration: 0,
            idle_timeout: 0,
            rng: (seed as u32).wrapping_mul(2_654_435_761) | 1,
        };
        clock.idle_timeout = clock.next_timeout();
        clock
    }

    /// `random.nextInt(40) + 180`.
    fn next_timeout(&mut self) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 40) as i32 + 180
    }

    /// `handleEntityEvent(1)`.
    pub fn jump(&mut self) {
        self.jump_duration = 15;
        self.jump_ticks = 0;
    }

    /// One tick: `setupAnimationStates` (the clocks, with the entity's
    pub fn tick(&mut self, anim: &mut [AnimationState], tick_count: i32, leashed: bool) {
        if self.idle_timeout <= 0 && !leashed {
            self.idle_timeout = self.next_timeout();
            anim[IDLE].start(tick_count);
        } else if self.jump_ticks > 0 {
            anim[HOP].start_if_stopped(tick_count);
            anim[IDLE].stop();
        } else {
            self.idle_timeout -= 1;
            anim[HOP].stop();
        }
        if leashed {
            anim[IDLE].stop();
        }
        if self.jump_ticks != self.jump_duration {
            self.jump_ticks += 1;
        } else if self.jump_duration != 0 {
            self.jump_ticks = 0;
            self.jump_duration = 0;
        }
    }
}

/// `RabbitRenderer`: the coat, or Toast's.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let coat = if a.magic == Magic::Toast {
        TOAST
    } else {
        a.rabbit_variant
    };
    out.push(LayerSpec::aged(
        baby,
        (M::Rabbit, texture(coat, false)),
        (M::RabbitBaby, texture(coat, true)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::keyframe::KeyframeAnimation;

    #[test]
    fn the_animations_bake_for_both_models() {
        for (layer, animations) in [(adult(), &ANIMATIONS), (baby(), &BABY_ANIMATIONS)] {
            let model = Model::bake(&layer);
            for (definition, _) in animations {
                assert!(KeyframeAnimation::bake(&model, definition).is_ok());
            }
        }
    }

    #[test]
    fn variants_pick_their_coat_and_unknown_ones_are_brown() {
        assert!(texture(3, false).ends_with("white_splotched"));
        assert!(texture(99, true).ends_with("caerbannog_baby"));
        assert!(texture(42, false).ends_with("brown"));
    }

    #[test]
    fn a_hop_event_runs_the_hop_for_fifteen_ticks() {
        let mut clock = RabbitClock::new(5);
        let mut anim = [AnimationState::default(); 4];
        clock.tick(&mut anim, 0, false);
        assert!(!anim[HOP].is_started());
        clock.jump();
        clock.tick(&mut anim, 1, false);
        clock.tick(&mut anim, 2, false);
        assert!(anim[HOP].is_started());
        for tick in 3..30 {
            clock.tick(&mut anim, tick, false);
        }
        assert!(!anim[HOP].is_started());
    }

    #[test]
    fn an_idle_rabbit_tilts_its_head_after_a_few_seconds_but_not_on_a_lead() {
        let mut anim = [AnimationState::default(); 4];
        let mut clock = RabbitClock::new(11);
        for tick in 0..179 {
            clock.tick(&mut anim, tick, false);
            assert!(!anim[IDLE].is_started());
        }
        for tick in 179..260 {
            clock.tick(&mut anim, tick, false);
        }
        assert!(anim[IDLE].is_started());
        let mut leashed = RabbitClock::new(11);
        let mut anim = [AnimationState::default(); 4];
        for tick in 0..400 {
            leashed.tick(&mut anim, tick, true);
        }
        assert!(!anim[IDLE].is_started());
    }
}
