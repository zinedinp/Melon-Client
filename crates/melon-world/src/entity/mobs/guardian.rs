//! `GuardianModel`: a spiked head with an eye and a three-part tail.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, PI};

/// `MeshTransformer.scaling(2.35)` of the elder.
pub const ELDER_SCALE: f32 = 2.35;

const SPIKE_X_ROT: [f32; 12] = [
    1.75, 0.25, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.25, 0.75, 0.0, 0.0,
];
const SPIKE_Y_ROT: [f32; 12] = [
    0.0, 0.0, 0.0, 0.0, 0.25, 1.75, 1.25, 0.75, 0.0, 0.0, 0.0, 0.0,
];
const SPIKE_Z_ROT: [f32; 12] = [
    0.0, 0.0, 0.25, 1.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, 1.25,
];
const SPIKE_X: [f32; 12] = [
    0.0, 0.0, 8.0, -8.0, -8.0, 8.0, 8.0, -8.0, 0.0, 0.0, 8.0, -8.0,
];
const SPIKE_Y: [f32; 12] = [
    -8.0, -8.0, -8.0, -8.0, 0.0, 0.0, 0.0, 0.0, 8.0, 8.0, 8.0, 8.0,
];
const SPIKE_Z: [f32; 12] = [
    8.0, -8.0, 0.0, 0.0, -8.0, -8.0, 8.0, 8.0, 8.0, -8.0, 0.0, 0.0,
];

fn spike_name(i: usize) -> &'static str {
    const NAMES: [&str; 12] = [
        "spike0", "spike1", "spike2", "spike3", "spike4", "spike5", "spike6", "spike7", "spike8",
        "spike9", "spike10", "spike11",
    ];
    NAMES[i]
}

fn spike_offset(i: usize, age: f32, withdrawal: f32) -> f32 {
    1.0 + angle::cos(age * 1.5 + i as f32) * 0.01 - withdrawal
}

fn spike_position(i: usize, age: f32, withdrawal: f32) -> (f32, f32, f32) {
    let o = spike_offset(i, age, withdrawal);
    (SPIKE_X[i] * o, 16.0 + SPIKE_Y[i] * o, SPIKE_Z[i] * o)
}

pub fn layer(elder: bool) -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-6.0, 10.0, -8.0), (12.0, 12.0, 16.0)),
            Cube::new((0.0, 28.0), (-8.0, 10.0, -6.0), (2.0, 12.0, 12.0)),
            Cube::new((0.0, 28.0), (6.0, 10.0, -6.0), (2.0, 12.0, 12.0)).mirrored(),
            Cube::new((16.0, 40.0), (-6.0, 8.0, -6.0), (12.0, 2.0, 12.0)),
            Cube::new((16.0, 40.0), (-6.0, 22.0, -6.0), (12.0, 2.0, 12.0)),
        ],
        Pose::ZERO,
    );
    for i in 0..12 {
        let (x, y, z) = spike_position(i, 0.0, 0.0);
        let rot = (
            PI * SPIKE_X_ROT[i],
            PI * SPIKE_Y_ROT[i],
            PI * SPIKE_Z_ROT[i],
        );
        head.add_or_replace_child(
            spike_name(i),
            vec![Cube::new((0.0, 0.0), (-1.0, -4.5, -1.0), (2.0, 9.0, 2.0))],
            Pose::offset_rotation((x, y, z), rot),
        );
    }
    head.add_or_replace_child(
        "eye",
        vec![Cube::new((8.0, 0.0), (-1.0, 15.0, 0.0), (2.0, 2.0, 1.0))],
        Pose::offset(0.0, 0.0, -8.25),
    );
    let tail0 = head.add_or_replace_child(
        "tail0",
        vec![Cube::new((40.0, 0.0), (-2.0, 14.0, 7.0), (4.0, 4.0, 8.0))],
        Pose::ZERO,
    );
    let tail1 = tail0.add_or_replace_child(
        "tail1",
        vec![Cube::new((0.0, 54.0), (0.0, 14.0, 0.0), (3.0, 3.0, 7.0))],
        Pose::offset(-1.5, 0.5, 14.0),
    );
    tail1.add_or_replace_child(
        "tail2",
        vec![
            Cube::new((41.0, 32.0), (0.0, 14.0, 0.0), (2.0, 2.0, 6.0)),
            Cube::new((25.0, 19.0), (1.0, 10.5, 3.0), (1.0, 9.0, 9.0)),
        ],
        Pose::offset(0.5, 0.5, 6.0),
    );
    if elder {
        root.pose = super::ghast::scaling(Pose::ZERO, ELDER_SCALE);
    }
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct GuardianParts {
    head: PartId,
    eye: PartId,
    spikes: [PartId; 12],
    tails: [PartId; 3],
}

impl GuardianParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            eye: model.part("eye"),
            spikes: std::array::from_fn(|i| model.part(spike_name(i))),
            tails: [
                model.part("tail0"),
                model.part("tail1"),
                model.part("tail2"),
            ],
        }
    }
}

/// What the pose reads from the client-side swimming.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GuardianPose {
    pub spikes: f32,
    pub tail: f32,
}

/// `GuardianModel.setupAnim`.
pub fn setup_anim(
    parts: &GuardianParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    guardian: GuardianPose,
) {
    let head = &mut pose[parts.head].pose;
    head.rotation.y = state.y_rot.to_radians();
    head.rotation.x = state.x_rot.to_radians();
    let withdrawal = (1.0 - guardian.spikes) * 0.55;
    for (i, spike) in parts.spikes.iter().enumerate() {
        let (x, y, z) = spike_position(i, state.age_in_ticks, withdrawal);
        let spike = &mut pose[*spike].pose;
        spike.offset = glam::Vec3::new(x, y, z);
    }
    pose[parts.eye].pose.offset.y = 1.0;
    let swim = angle::sin(guardian.tail);
    for (tail, factor) in parts.tails.iter().zip([0.05, 0.1, 0.15]) {
        pose[*tail].pose.rotation.y = swim * PI * factor;
    }
}

/// `Guardian.aiStep`'s client half: the tail beats and the spikes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swim {
    tail_speed: f32,
    tail: f32,
    old_tail: f32,
    spikes: f32,
    old_spikes: f32,
    random: u32,
}

impl Swim {
    pub fn new(id: i32) -> Self {
        Self {
            tail_speed: 0.0,
            tail: 0.0,
            old_tail: 0.0,
            spikes: 0.0,
            old_spikes: 0.0,
            random: (id as u32).wrapping_mul(2_654_435_761) | 1,
        }
    }

    fn next_float(&mut self) -> f32 {
        let mut x = self.random;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.random = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn tick(&mut self, in_water: bool, moving: bool) {
        self.old_tail = self.tail;
        if !in_water {
            self.tail_speed = 2.0;
        } else if moving {
            if self.tail_speed < 0.5 {
                self.tail_speed = 4.0;
            } else {
                self.tail_speed += (0.5 - self.tail_speed) * 0.1;
            }
        } else {
            self.tail_speed += (0.125 - self.tail_speed) * 0.2;
        }
        self.tail += self.tail_speed;
        self.old_spikes = self.spikes;
        if !in_water {
            self.spikes = self.next_float();
        } else if moving {
            self.spikes += (0.0 - self.spikes) * 0.25;
        } else {
            self.spikes += (1.0 - self.spikes) * 0.06;
        }
    }

    pub fn pose(&self, partial: f32) -> GuardianPose {
        GuardianPose {
            spikes: self.old_spikes + (self.spikes - self.old_spikes) * partial,
            tail: self.old_tail + (self.tail - self.old_tail) * partial,
        }
    }
}

/// `GuardianRenderer` and `ElderGuardianRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Guardian => out.push(plain(M::Guardian, "minecraft:entity/guardian/guardian")),
        EntityKind::ElderGuardian => out.push(plain(
            M::ElderGuardian,
            "minecraft:entity/guardian/guardian_elder",
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spikes_withdraw_when_it_swims_and_the_tail_beats_faster() {
        let model = Model::bake(&layer(false));
        let parts = GuardianParts::find(&model);
        let state = LivingRenderState::default();
        let out = |spikes: f32| {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &state,
                GuardianPose { spikes, tail: 0.0 },
            );
            pose[parts.spikes[2]].pose.offset.x
        };
        assert!((out(1.0) - 8.0).abs() < 0.2);
        assert!(out(0.0) < out(1.0) - 3.0);

        let mut swimming = Swim::new(5);
        let mut idle = Swim::new(5);
        for _ in 0..20 {
            swimming.tick(true, true);
            idle.tick(true, false);
        }
        assert!(swimming.pose(1.0).tail > idle.pose(1.0).tail);
        assert!(swimming.pose(1.0).spikes < idle.pose(1.0).spikes);
    }

    #[test]
    fn elder_and_normal_guardians_share_their_parts() {
        for elder in [false, true] {
            let model = Model::bake(&layer(elder));
            let _ = GuardianParts::find(&model);
        }
    }
}
