//! `PiglinModel` and `ZombifiedPiglinModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::animation::{animate_crossbow_charge, animate_crossbow_hold, bob_arms};
use crate::entity::humanoid::HumanoidParts;
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::player::Arm;
use crate::math::angle::{self, PI};
use crate::model_part::{Cube, Pose};

/// `AdultPiglinModel.getDefaultEarAngleInDegrees` and the baby's.
const ADULT_EAR_DEGREES: f32 = 30.0;
const BABY_EAR_DEGREES: f32 = 5.0;

/// `AbstractPiglinModel.addHead`.
fn add_head(root: &mut PartDefinition) {
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-5.0, -8.0, -4.0), (10.0, 8.0, 8.0)),
            Cube::new((31.0, 1.0), (-2.0, -4.0, -5.0), (4.0, 4.0, 1.0)),
            Cube::new((2.0, 4.0), (2.0, -2.0, -5.0), (1.0, 2.0, 1.0)),
            Cube::new((2.0, 0.0), (-3.0, -2.0, -5.0), (1.0, 2.0, 1.0)),
        ],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((51.0, 6.0), (0.0, 0.0, -2.0), (1.0, 5.0, 4.0))],
        Pose::offset_rotation((4.5, -6.0, 0.0), (0.0, 0.0, -PI / 6.0)),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((39.0, 6.0), (-1.0, 0.0, -2.0), (1.0, 5.0, 4.0))],
        Pose::offset_rotation((-4.5, -6.0, 0.0), (0.0, 0.0, PI / 6.0)),
    );
}

/// `AdultPiglinModel.createBodyLayer`.
pub fn armor() -> [LayerDefinition; 4] {
    crate::entity::humanoid::armor_layers(crate::entity::humanoid::base_armor_mesh, 0.5, 1.02)
}

/// `AbstractPiglinModel.createBabyArmorMeshSet`.
pub fn baby_armor() -> [LayerDefinition; 4] {
    crate::entity::humanoid::baby_armor_layers(
        glam::Vec3::splat(0.7),
        glam::Vec3::splat(0.7),
        glam::Vec3::new(0.5, -0.5, 0.0),
    )
}

pub fn adult() -> LayerDefinition {
    let mut layer = super::zombie::drowned(0.0);
    add_head(&mut layer.root);
    layer
        .root
        .child_mut("head")
        .add_or_replace_child("hat", Vec::new(), Pose::ZERO);
    layer
}

/// `BabyPiglinModel.createBodyLayer`.
pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 13.0), (-3.0, -3.0, -1.0), (6.0, 5.0, 3.0))],
        Pose::offset(0.0, 18.0, -0.5),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((21.0, 30.0), (-1.5, -3.0, -4.5), (3.0, 3.0, 1.0)),
            Cube::new((0.0, 0.0), (-4.5, -6.0, -3.5), (9.0, 6.0, 7.0)),
        ],
        Pose::offset(0.0, 15.0, 0.0),
    );
    head.add_or_replace_child("hat", Vec::new(), Pose::ZERO);
    for (name, inner, uv, x, tilt) in [
        ("left_ear", "left_ear_r1", (0.0, 21.0), 4.2, -0.6109),
        ("right_ear", "right_ear_r1", (18.0, 13.0), -4.2, 0.6109),
    ] {
        let side = if x > 0.0 { 1.0 } else { -1.0 };
        head.add_or_replace_child(name, Vec::new(), Pose::offset(x, -4.0, 0.0))
            .add_or_replace_child(
                inner,
                vec![Cube::new(uv, (-0.5, -3.0, -2.0), (1.0, 6.0, 4.0))],
                Pose::offset_rotation((side, 1.75, 0.0), (0.0, 0.0, tilt)),
            );
    }
    for (name, uv, x) in [
        ("left_arm", (28.0, 13.0), 4.0),
        ("right_arm", (10.0, 30.0), -4.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, 0.0, -1.5), (2.0, 5.0, 3.0))],
            Pose::offset(x, 15.0, 0.0),
        );
    }
    for (name, uv, x) in [
        ("right_leg", (22.0, 23.0), -1.5),
        ("left_leg", (10.0, 23.0), 1.5),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.5, 0.0, -1.5), (3.0, 4.0, 3.0))],
            Pose::offset(x, 20.0, 0.0),
        );
    }
    LayerDefinition::new(root, 64.0, 64.0)
}

/// A piglin's humanoid parts and its ears.
#[derive(Clone, Copy, Debug)]
pub struct PiglinParts {
    pub humanoid: HumanoidParts,
    left_ear: PartId,
    right_ear: PartId,
    /// The ears' resting angle, radians.
    ear_angle: f32,
}

impl PiglinParts {
    pub fn find(model: &Model, baby: bool) -> Self {
        Self {
            humanoid: HumanoidParts::find(model),
            left_ear: model.part("left_ear"),
            right_ear: model.part("right_ear"),
            ear_angle: if baby {
                BABY_EAR_DEGREES
            } else {
                ADULT_EAR_DEGREES
            }
            .to_radians(),
        }
    }
}

/// `PiglinArmPose`, the ones Melon sets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ArmsPose {
    #[default]
    Default,
    Dancing,
    /// Looking at a loved item in the off hand.
    Admiring,
    /// A raised melee weapon, swinging down by `swing` (0 at rest).
    Melee {
        swing: f32,
    },
    /// A loaded crossbow, aimed.
    CrossbowHold,
    /// Loading one: `ticks` of `max` ticks used.
    CrossbowCharge {
        ticks: f32,
        max: f32,
    },
}

/// What a piglin's pose adds to the humanoid's.
#[derive(Clone, Copy, Debug, Default)]
pub struct PiglinPose {
    pub arms: ArmsPose,
    pub main_arm: Arm,
}

/// `AbstractPiglinModel.setupAnim` and `PiglinModel.setupAnim`, on top of the humanoid's.
pub fn setup_anim(
    parts: &PiglinParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    piglin: PiglinPose,
) {
    let freq = living.age_in_ticks * 0.1 + living.walk_position * 0.5;
    let amplitude = 0.08 + living.walk_speed * 0.4;
    pose[parts.left_ear].pose.rotation.z = -parts.ear_angle - angle::cos(freq * 1.2) * amplitude;
    pose[parts.right_ear].pose.rotation.z = parts.ear_angle + angle::cos(freq) * amplitude;

    let h = &parts.humanoid;
    let rad = PI / 180.0;
    match piglin.arms {
        ArmsPose::Default => {}
        ArmsPose::Dancing => {
            let dance = living.age_in_ticks / 60.0;
            pose[parts.right_ear].pose.rotation.z =
                PI / 6.0 + rad * angle::sin(dance * 30.0) * 10.0;
            pose[parts.left_ear].pose.rotation.z =
                -PI / 6.0 - rad * angle::cos(dance * 30.0) * 10.0;
            pose[h.head].pose.offset.x += angle::sin(dance * 10.0);
            pose[h.head].pose.offset.y += angle::sin(dance * 40.0) + 0.4;
            let arm = rad * (70.0 + angle::cos(dance * 40.0) * 10.0);
            pose[h.right_arm].pose.rotation.z = arm;
            pose[h.left_arm].pose.rotation.z = -arm;
            pose[h.right_arm].pose.offset.y += angle::sin(dance * 40.0) * 0.5 - 0.5;
            pose[h.left_arm].pose.offset.y += angle::sin(dance * 40.0) * 0.5 + 0.5;
            pose[h.body].pose.offset.y += angle::sin(dance * 40.0) * 0.35;
        }
        ArmsPose::Melee { swing } if swing > 0.0 => {
            swing_weapon_down(h, pose, piglin.main_arm, swing, living.age_in_ticks);
        }
        ArmsPose::Melee { .. } => {
            let arm = match piglin.main_arm {
                Arm::Right => h.right_arm,
                Arm::Left => h.left_arm,
            };
            pose[arm].pose.rotation.x = -1.8;
        }
        ArmsPose::CrossbowHold => {
            let head = (pose[h.head].pose.rotation.x, pose[h.head].pose.rotation.y);
            let (mut right, mut left) = (pose[h.right_arm].pose, pose[h.left_arm].pose);
            animate_crossbow_hold(&mut right, &mut left, head, piglin.main_arm == Arm::Right);
            pose[h.right_arm].pose = right;
            pose[h.left_arm].pose = left;
        }
        ArmsPose::CrossbowCharge { ticks, max } => {
            let (mut right, mut left) = (pose[h.right_arm].pose, pose[h.left_arm].pose);
            animate_crossbow_charge(
                &mut right,
                &mut left,
                max,
                ticks,
                piglin.main_arm == Arm::Right,
            );
            pose[h.right_arm].pose = right;
            pose[h.left_arm].pose = left;
        }
        ArmsPose::Admiring => {
            pose[h.head].pose.rotation.x = 0.5;
            pose[h.head].pose.rotation.y = 0.0;
            let (arm, y) = match piglin.main_arm {
                Arm::Left => (h.right_arm, -0.5),
                Arm::Right => (h.left_arm, 0.5),
            };
            pose[arm].pose.rotation.y = y;
            pose[arm].pose.rotation.x = -0.9;
        }
    }
}

/// `AnimationUtils.swingWeaponDown`.
pub(super) fn swing_weapon_down(
    h: &HumanoidParts,
    pose: &mut ModelPose,
    main_arm: Arm,
    swing: f32,
    age_in_ticks: f32,
) {
    let attack2 = angle::sin(swing * PI);
    let attack = angle::sin((1.0 - (1.0 - swing) * (1.0 - swing)) * PI);
    let (main, off) = match main_arm {
        Arm::Right => (h.right_arm, h.left_arm),
        Arm::Left => (h.left_arm, h.right_arm),
    };
    let (main_turn, off_turn) = match main_arm {
        Arm::Right => (PI / 20.0, -PI / 20.0),
        Arm::Left => (-PI / 20.0, PI / 20.0),
    };
    for (arm, turn) in [(main, main_turn), (off, off_turn)] {
        pose[arm].pose.rotation.z = 0.0;
        pose[arm].pose.rotation.y = turn;
    }
    pose[main].pose.rotation.x =
        -1.884_955_8 + angle::cos(age_in_ticks * 0.09) * 0.15 + attack2 * 2.2 - attack * 0.4;
    pose[off].pose.rotation.x =
        angle::cos(age_in_ticks * 0.19) * 0.5 + attack2 * 1.2 - attack * 0.4;
    let [right, left] = pose.get_two_mut(h.right_arm, h.left_arm);
    bob_arms(&mut right.pose, &mut left.pose, age_in_ticks);
}

/// The piglin renderers.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Piglin => out.push(if baby {
            plain(M::PiglinBaby, "minecraft:entity/piglin/piglin_baby")
        } else {
            plain(M::Piglin, "minecraft:entity/piglin/piglin")
        }),
        EntityKind::PiglinBrute => {
            out.push(plain(M::Piglin, "minecraft:entity/piglin/piglin_brute"))
        }
        EntityKind::ZombifiedPiglin => out.push(if baby {
            plain(
                M::PiglinBaby,
                "minecraft:entity/piglin/zombified_piglin_baby",
            )
        } else {
            plain(M::Piglin, "minecraft:entity/piglin/zombified_piglin")
        }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ears_sway_around_their_resting_angle() {
        for (layer, baby) in [(adult(), false), (baby(), true)] {
            let model = Model::bake(&layer);
            let parts = PiglinParts::find(&model, baby);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                PiglinPose::default(),
            );
            let left = pose[parts.left_ear].pose.rotation.z;
            let right = pose[parts.right_ear].pose.rotation.z;
            assert!(left < 0.0 && right > 0.0, "{left} {right}");
            assert!((right - parts.ear_angle).abs() < 0.5);
        }
    }

    #[test]
    fn a_raised_weapon_goes_up_and_swings_down() {
        let model = Model::bake(&adult());
        let parts = PiglinParts::find(&model, false);
        let living = LivingRenderState::default();
        let mut high = model.initial_pose();
        setup_anim(
            &parts,
            &mut high,
            &living,
            PiglinPose {
                arms: ArmsPose::Melee { swing: 0.0 },
                main_arm: Arm::Right,
            },
        );
        assert_eq!(high[parts.humanoid.right_arm].pose.rotation.x, -1.8);
        let mut down = model.initial_pose();
        setup_anim(
            &parts,
            &mut down,
            &living,
            PiglinPose {
                arms: ArmsPose::Melee { swing: 0.5 },
                main_arm: Arm::Right,
            },
        );
        assert!(down[parts.humanoid.right_arm].pose.rotation.x > -1.0);
    }
}
