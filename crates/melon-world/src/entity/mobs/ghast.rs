//! `GhastModel` and `HappyGhastModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::java_random::JavaRandom;
use crate::model_part::{Cube, Pose};
use melon_core::math::angle;

/// `MeshTransformer.scaling(4.5)`.
pub const SCALE: f32 = 4.5;

/// `MeshTransformer.scaling(factor)` on a root pose.
pub fn scaling(pose: Pose, factor: f32) -> Pose {
    let mut pose = pose.scaled(factor);
    pose.offset.y += 24.016 * (1.0 - factor);
    pose
}
const TENTACLES: usize = 9;

fn tentacle_name(i: usize) -> &'static str {
    const NAMES: [&str; TENTACLES] = [
        "tentacle0",
        "tentacle1",
        "tentacle2",
        "tentacle3",
        "tentacle4",
        "tentacle5",
        "tentacle6",
        "tentacle7",
        "tentacle8",
    ];
    NAMES[i]
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (0.0, 0.0),
            (-8.0, -8.0, -8.0),
            (16.0, 16.0, 16.0),
        )],
        Pose::offset(0.0, 17.6, 0.0),
    );
    let mut random = JavaRandom::with_seed(1660);
    for i in 0..TENTACLES {
        let (column, row) = ((i % 3) as f32, (i / 3) as f32);
        let x = ((column - (i / 3 % 2) as f32 * 0.5 + 0.25) / 2.0 * 2.0 - 1.0) * 5.0;
        let z = (row / 2.0 * 2.0 - 1.0) * 5.0;
        let length = random.next_int(7) + 8;
        root.add_or_replace_child(
            tentacle_name(i),
            vec![Cube::new(
                (0.0, 0.0),
                (-1.0, 0.0, -1.0),
                (2.0, length as f32, 2.0),
            )],
            Pose::offset(x, 24.6, z),
        );
    }
    root.pose = scaling(Pose::ZERO, SCALE);
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `HappyGhastModel.BABY_TRANSFORMER`.
const BABY_SCALE: f32 = 0.2375;
const HAPPY_SCALE: f32 = 4.0;
/// `HappyGhastModel.BODY_SQUEEZE`: under a harness the body is slimmer.
const BODY_SQUEEZE: f32 = 0.9375;

/// The root of a happy ghast's models.
fn happy_root(baby: bool) -> Pose {
    let pose = scaling(Pose::ZERO, HAPPY_SCALE);
    if baby {
        scaling(pose, BABY_SCALE)
    } else {
        pose
    }
}

/// `(x, z, length)` of each of the happy ghast's tentacles, all at y 7.
const HAPPY_TENTACLES: [(f32, f32, f32); TENTACLES] = [
    (-3.75, -5.0, 5.0),
    (1.25, -5.0, 7.0),
    (6.25, -5.0, 4.0),
    (-6.25, 0.0, 5.0),
    (-1.25, 0.0, 5.0),
    (3.75, 0.0, 7.0),
    (-3.75, 5.0, 8.0),
    (1.25, 5.0, 8.0),
    (6.25, 5.0, 5.0),
];

/// `HappyGhastModel.createBodyLayer`.
pub fn happy(baby: bool) -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (0.0, 0.0),
            (-8.0, -8.0, -8.0),
            (16.0, 16.0, 16.0),
        )],
        Pose::offset(0.0, 16.0, 0.0),
    );
    if baby {
        body.add_or_replace_child(
            "inner_body",
            vec![Cube::new((0.0, 32.0), (-8.0, -16.0, -8.0), (16.0, 16.0, 16.0)).grown(-0.5)],
            Pose::offset(0.0, 8.0, 0.0),
        );
    }
    for (i, (x, z, length)) in HAPPY_TENTACLES.into_iter().enumerate() {
        body.add_or_replace_child(
            tentacle_name(i),
            vec![Cube::new((0.0, 0.0), (-1.0, 0.0, -1.0), (2.0, length, 2.0))],
            Pose::offset(x, 7.0, z),
        );
    }
    root.pose = happy_root(baby);
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `HappyGhastHarnessModel.createHarnessLayer`.
pub fn harness(baby: bool) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "harness",
        vec![Cube::new(
            (0.0, 0.0),
            (-8.0, -16.0, -8.0),
            (16.0, 16.0, 16.0),
        )],
        Pose::offset(0.0, 24.0, 0.0),
    );
    root.add_or_replace_child(
        "goggles",
        vec![Cube::new((0.0, 32.0), (-8.0, -2.5, -2.5), (16.0, 5.0, 5.0)).grown(0.15)],
        Pose::offset(0.0, 14.0, -5.5),
    );
    root.pose = happy_root(baby);
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct HappyGhastParts {
    body: PartId,
    tentacles: [PartId; TENTACLES],
}

impl HappyGhastParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
            tentacles: std::array::from_fn(|i| model.part(tentacle_name(i))),
        }
    }
}

/// What the happy ghast's pose reads besides the time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HappyGhastPose {
    /// Wears a harness: the body is squeezed.
    pub harnessed: bool,
    /// Has riders: the goggles come down.
    pub ridden: bool,
}

/// `HappyGhastModel.setupAnim`.
pub fn happy_setup_anim(
    parts: &HappyGhastParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    happy: HappyGhastPose,
) {
    if happy.harnessed {
        pose[parts.body].pose.scale = glam::Vec3::splat(BODY_SQUEEZE);
    }
    sway(&parts.tentacles, pose, state);
}

#[derive(Clone, Copy, Debug)]
pub struct HarnessParts {
    goggles: PartId,
}

impl HarnessParts {
    pub fn find(model: &Model) -> Self {
        Self {
            goggles: model.part("goggles"),
        }
    }
}

/// `HappyGhastHarnessModel.setupAnim`.
pub fn harness_setup_anim(parts: &HarnessParts, pose: &mut ModelPose, happy: HappyGhastPose) {
    let goggles = &mut pose[parts.goggles].pose;
    if happy.ridden {
        goggles.rotation.x = 0.0;
        goggles.offset.y = 14.0;
    } else {
        goggles.rotation.x = -melon_core::math::angle::QUARTER_PI;
        goggles.offset.y = 9.0;
    }
}

/// `GhastModel.animateTentacles`.
fn sway(tentacles: &[PartId; TENTACLES], pose: &mut ModelPose, state: &LivingRenderState) {
    for (i, tentacle) in tentacles.iter().enumerate() {
        pose[*tentacle].pose.rotation.x =
            0.2 * angle::sin(state.age_in_ticks * 0.3 + i as f32) + 0.4;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GhastParts {
    tentacles: [PartId; TENTACLES],
}

impl GhastParts {
    pub fn find(model: &Model) -> Self {
        Self {
            tentacles: std::array::from_fn(|i| model.part(tentacle_name(i))),
        }
    }
}

/// `GhastModel.animateTentacles`: each sways with its own phase.
pub fn setup_anim(parts: &GhastParts, pose: &mut ModelPose, state: &LivingRenderState) {
    sway(&parts.tentacles, pose, state);
}

/// `GhastRenderer` and `HappyGhastRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Ghast => out.push(plain(
            M::Ghast,
            if a.charging {
                "minecraft:entity/ghast/ghast_shooting"
            } else {
                "minecraft:entity/ghast/ghast"
            },
        )),
        EntityKind::HappyGhast => out.push(LayerSpec::aged(
            baby,
            (M::HappyGhast, "minecraft:entity/ghast/happy_ghast"),
            (M::HappyGhastBaby, "minecraft:entity/ghast/happy_ghast_baby"),
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tentacles_sway_out_of_step_and_hang_below_the_body() {
        let model = Model::bake(&layer());
        let parts = GhastParts::find(&model);
        let mut pose = model.initial_pose();
        let state = LivingRenderState {
            age_in_ticks: 3.0,
            ..Default::default()
        };
        setup_anim(&parts, &mut pose, &state);
        let angles: Vec<f32> = parts
            .tentacles
            .iter()
            .map(|t| pose[*t].pose.rotation.x)
            .collect();
        assert!(angles.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-3));
        assert!(angles.iter().all(|a| (0.2..=0.6).contains(a)));
    }

    #[test]
    fn scaling_keeps_the_feet_line_where_it_was() {
        let pose = scaling(Pose::ZERO, 4.0);
        assert!((pose.offset.y + 24.016 * 3.0).abs() < 1e-3);
        assert!((pose.offset.y + pose.scale.y * 24.016 - 24.016).abs() < 1e-3);
    }

    #[test]
    fn a_harness_squeezes_the_body_and_riders_lower_the_goggles() {
        let model = Model::bake(&happy(false));
        let parts = HappyGhastParts::find(&model);
        let mut pose = model.initial_pose();
        let state = LivingRenderState::default();
        happy_setup_anim(&parts, &mut pose, &state, HappyGhastPose::default());
        assert_eq!(pose[parts.body].pose.scale, glam::Vec3::ONE);
        happy_setup_anim(
            &parts,
            &mut pose,
            &state,
            HappyGhastPose {
                harnessed: true,
                ridden: false,
            },
        );
        assert!((pose[parts.body].pose.scale.x - 0.9375).abs() < 1e-6);

        let model = Model::bake(&harness(false));
        let parts = HarnessParts::find(&model);
        let mut pose = model.initial_pose();
        harness_setup_anim(&parts, &mut pose, HappyGhastPose::default());
        assert_eq!(pose[parts.goggles].pose.offset.y, 9.0);
        harness_setup_anim(
            &parts,
            &mut pose,
            HappyGhastPose {
                harnessed: true,
                ridden: true,
            },
        );
        assert_eq!(pose[parts.goggles].pose.offset.y, 14.0);
    }

    #[test]
    fn a_baby_is_the_adult_at_just_under_a_quarter() {
        let adult = happy_root(false);
        let baby = happy_root(true);
        assert!((baby.scale.x / adult.scale.x - BABY_SCALE).abs() < 1e-5);
    }
}
