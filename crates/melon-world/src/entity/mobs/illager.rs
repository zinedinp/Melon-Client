//! `IllagerModel`: pillager, vindicator and evoker.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::animation::{
    animate_crossbow_charge, animate_crossbow_hold, animate_zombie_arms, bob_arms,
};
use crate::entity::humanoid::HumanoidParts;
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::player::Arm;
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, PI};

/// `IllagerModel.createBodyLayer`; the renderer scales it by 0.9375.
pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 10.0, 8.0))],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "hat",
        vec![Cube::new((32.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 12.0, 8.0)).grown(0.45)],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((24.0, 0.0), (-1.0, -1.0, -6.0), (2.0, 4.0, 2.0))],
        Pose::offset(0.0, -2.0, 0.0),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((16.0, 20.0), (-4.0, 0.0, -3.0), (8.0, 12.0, 6.0)),
            Cube::new((0.0, 38.0), (-4.0, 0.0, -3.0), (8.0, 20.0, 6.0)).grown(0.5),
        ],
        Pose::ZERO,
    );
    let arms = root.add_or_replace_child(
        "arms",
        vec![
            Cube::new((44.0, 22.0), (-8.0, -2.0, -2.0), (4.0, 8.0, 4.0)),
            Cube::new((40.0, 38.0), (-4.0, 2.0, -2.0), (8.0, 4.0, 4.0)),
        ],
        Pose::offset_rotation((0.0, 3.0, -1.0), (-0.75, 0.0, 0.0)),
    );
    arms.add_or_replace_child(
        "left_shoulder",
        vec![Cube::new((44.0, 22.0), (4.0, -2.0, -2.0), (4.0, 8.0, 4.0)).mirrored()],
        Pose::ZERO,
    );
    for (name, x, mirror) in [("right_leg", -2.0, false), ("left_leg", 2.0, true)] {
        let cube = Cube::new((0.0, 22.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0));
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 12.0, 0.0),
        );
    }
    for (name, x, mirror) in [("right_arm", -5.0, false), ("left_arm", 5.0, true)] {
        let cube = Cube::new(
            (40.0, 46.0),
            if mirror {
                (-1.0, -2.0, -2.0)
            } else {
                (-3.0, -2.0, -2.0)
            },
            (4.0, 12.0, 4.0),
        );
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 2.0, 0.0),
        );
    }
    LayerDefinition::new(root, 64.0, 64.0)
}

/// The body parts (the humanoid's names) and the folded arms.
#[derive(Clone, Copy, Debug)]
pub struct IllagerParts {
    pub humanoid: HumanoidParts,
    arms: PartId,
}

impl IllagerParts {
    pub fn find(model: &Model) -> Self {
        Self {
            humanoid: HumanoidParts::find(model),
            arms: model.part("arms"),
        }
    }
}

/// `AbstractIllager.IllagerArmPose`, the ones an illager can have.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ArmPose {
    /// Arms folded in front.
    #[default]
    Crossed,
    /// Arms free, only walking.
    Neutral,
    /// Raised to strike; swinging by `swing` when it has a weapon.
    Attacking {
        weapon: bool,
        swing: f32,
    },
    Spellcasting,
    CrossbowHold,
    CrossbowCharge {
        ticks: f32,
        max: f32,
    },
    Celebrating,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IllagerPose {
    pub arms: ArmPose,
    pub main_arm: Arm,
    /// The illusioner's hat stays on (`IllagerModel.getHat().visible`).
    pub hat: bool,
}

/// `IllagerModel.setupAnim`.
pub fn setup_anim(
    parts: &IllagerParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    illager: IllagerPose,
) {
    let h = &parts.humanoid;
    let age = living.age_in_ticks;
    pose[h.hat].visible = illager.hat;
    let head = &mut pose[h.head].pose.rotation;
    head.y = living.y_rot.to_radians();
    head.x = living.x_rot.to_radians();
    let head = (head.x, head.y);

    let speed = living.walk_speed;
    let phase = living.walk_position * 0.6662;
    let arm = |pose: &mut ModelPose, id: PartId, x: f32| {
        pose[id].pose.rotation = glam::Vec3::new(x, 0.0, 0.0);
    };
    arm(
        pose,
        h.right_arm,
        angle::cos(phase + PI) * 2.0 * speed * 0.5,
    );
    arm(pose, h.left_arm, angle::cos(phase) * 2.0 * speed * 0.5);
    arm(pose, h.right_leg, angle::cos(phase) * 1.4 * speed * 0.5);
    arm(pose, h.left_leg, angle::cos(phase + PI) * 1.4 * speed * 0.5);
    if living.riding {
        arm(pose, h.right_arm, -PI / 5.0);
        arm(pose, h.left_arm, -PI / 5.0);
        crate::entity::humanoid::sit(h, pose);
    }

    match illager.arms {
        ArmPose::Attacking {
            weapon: false,
            swing,
        } => {
            let [right, left] = pose.get_two_mut(h.right_arm, h.left_arm);
            animate_zombie_arms(&mut left.pose, &mut right.pose, true, false, swing);
            bob_arms(&mut right.pose, &mut left.pose, age);
        }
        ArmPose::Attacking {
            weapon: true,
            swing,
        } => super::piglin::swing_weapon_down(h, pose, illager.main_arm, swing, age),
        ArmPose::Spellcasting => {
            let sway = angle::cos(age * 0.6662) * 0.25;
            for (id, side) in [(h.right_arm, 1.0), (h.left_arm, -1.0)] {
                let p = &mut pose[id].pose;
                p.offset.z = 0.0;
                p.offset.x = -5.0 * side;
                p.rotation = glam::Vec3::new(sway, 0.0, PI * 3.0 / 4.0 * side);
            }
        }
        ArmPose::CrossbowHold => {
            let [right, left] = pose.get_two_mut(h.right_arm, h.left_arm);
            animate_crossbow_hold(&mut right.pose, &mut left.pose, head, true);
        }
        ArmPose::CrossbowCharge { ticks, max } => {
            let [right, left] = pose.get_two_mut(h.right_arm, h.left_arm);
            animate_crossbow_charge(&mut right.pose, &mut left.pose, max, ticks, true);
        }
        ArmPose::Celebrating => {
            let sway = angle::cos(age * 0.6662) * 0.05;
            for (id, side, z) in [
                (h.right_arm, 1.0, 2.670_354),
                (h.left_arm, -1.0, -PI * 3.0 / 4.0),
            ] {
                let p = &mut pose[id].pose;
                p.offset.z = 0.0;
                p.offset.x = -5.0 * side;
                p.rotation = glam::Vec3::new(sway, 0.0, z);
            }
        }
        ArmPose::Crossed | ArmPose::Neutral => {}
    }

    let crossed = illager.arms == ArmPose::Crossed;
    pose[parts.arms].visible = crossed;
    pose[h.left_arm].visible = !crossed;
    pose[h.right_arm].visible = !crossed;
}

/// The illager renderers.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Illusioner => {
            out.push(plain(M::Illager, "minecraft:entity/illager/illusioner"));
        }
        EntityKind::Pillager => out.push(plain(M::Illager, "minecraft:entity/illager/pillager")),
        EntityKind::Vindicator => {
            out.push(plain(M::Illager, "minecraft:entity/illager/vindicator"));
        }
        EntityKind::Evoker => out.push(plain(M::Illager, "minecraft:entity/illager/evoker")),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posed(arms: ArmPose) -> (ModelPose, IllagerParts) {
        let model = Model::bake(&layer());
        let parts = IllagerParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            IllagerPose {
                arms,
                main_arm: Arm::Right,
                hat: false,
            },
        );
        (pose, parts)
    }

    #[test]
    fn crossed_arms_show_the_folded_part_and_hide_the_arms() {
        let (pose, parts) = posed(ArmPose::Crossed);
        assert!(pose[parts.arms].visible && !pose[parts.humanoid.right_arm].visible);
        assert!(!pose[parts.humanoid.hat].visible);
        let (pose, parts) = posed(ArmPose::Neutral);
        assert!(!pose[parts.arms].visible && pose[parts.humanoid.left_arm].visible);
    }

    #[test]
    fn casting_raises_both_arms_out_and_up() {
        let (pose, parts) = posed(ArmPose::Spellcasting);
        let right = pose[parts.humanoid.right_arm].pose;
        let left = pose[parts.humanoid.left_arm].pose;
        assert!(right.rotation.z > 2.0 && left.rotation.z < -2.0);
        assert_eq!((right.offset.x, left.offset.x), (-5.0, 5.0));
    }
}
