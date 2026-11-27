//! `EndermanModel`: a humanoid with limbs thirty pixels long.

use super::super::humanoid::{self, HumanoidParts};
use super::super::model::{LayerDefinition, ModelPose};
use super::{LayerSpec, ModelId as M};
use crate::model_part::{Cube, Pose};

/// Where `createMesh(NONE, -14)` stands the body.
const Y_OFFSET: f32 = -14.0;

pub fn layer() -> LayerDefinition {
    let mut root = humanoid::create_mesh(0.0, Y_OFFSET);
    let limb = |y: f32| Cube::new((56.0, 0.0), (-1.0, y, -1.0), (2.0, 30.0, 2.0));
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0))],
        Pose::offset(0.0, -13.0, 0.0),
    );
    head.add_or_replace_child(
        "hat",
        vec![Cube::new((0.0, 16.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)).grown(-0.5)],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((32.0, 16.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0))],
        Pose::offset(0.0, -14.0, 0.0),
    );
    for (name, x, y, pose_y, mirrored) in [
        ("right_arm", -5.0, -2.0, -12.0, false),
        ("left_arm", 5.0, -2.0, -12.0, true),
        ("right_leg", -2.0, 0.0, -5.0, false),
        ("left_leg", 2.0, 0.0, -5.0, true),
    ] {
        let cube = limb(y);
        root.add_or_replace_child(
            name,
            vec![if mirrored { cube.mirrored() } else { cube }],
            Pose::offset(x, pose_y, 0.0),
        );
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `CarriedBlockLayer`'s transform, from the entity's root.
pub fn carried_block_transform(root: glam::Mat4) -> glam::Mat4 {
    use crate::math::angle::Rotation;
    use glam::{Mat4, Vec3};
    root * Mat4::from_translation(Vec3::new(0.0, 0.6875, -0.75))
        * Mat4::rot_x_deg(20.0)
        * Mat4::rot_y_deg(45.0)
        * Mat4::from_translation(Vec3::new(0.25, 0.1875, 0.25))
        * Mat4::from_scale(Vec3::new(-0.5, -0.5, 0.5))
        * Mat4::rot_y_deg(90.0)
}

/// `EndermanRenderer.getRenderOffset`.
pub fn creepy_shake(age_in_ticks: f32, scale: f32) -> (f32, f32) {
    let mut state = age_in_ticks.to_bits().wrapping_mul(2_654_435_761) | 1;
    let mut uniform = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32
    };
    let mut gaussian = || (0..12).map(|_| uniform()).sum::<f32>() - 6.0;
    let d = 0.02 * scale;
    (gaussian() * d, gaussian() * d)
}

/// What the pose reads besides the humanoid state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EndermanPose {
    /// Holds a block: arms forward and a little apart.
    pub carrying: bool,
    /// Staring at someone: the head drops forward out of the hat.
    pub creepy: bool,
}

/// `EndermanModel.setupAnim` after the humanoid's.
pub fn setup_anim(parts: &HumanoidParts, pose: &mut ModelPose, enderman: EndermanPose) {
    for limb in [
        parts.right_arm,
        parts.left_arm,
        parts.right_leg,
        parts.left_leg,
    ] {
        let x = &mut pose[limb].pose.rotation.x;
        *x = (*x * 0.5).clamp(-0.4, 0.4);
    }
    if enderman.carrying {
        pose[parts.right_arm].pose.rotation.x = -0.5;
        pose[parts.left_arm].pose.rotation.x = -0.5;
        pose[parts.right_arm].pose.rotation.z = 0.05;
        pose[parts.left_arm].pose.rotation.z = -0.05;
    }
    if enderman.creepy {
        pose[parts.head].pose.offset.y -= 5.0;
        pose[parts.hat].pose.offset.y += 5.0;
    }
}

/// `EndermanRenderer` and `EnderEyesLayer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Enderman, "minecraft:entity/enderman/enderman"));
    out.push(plain(M::Enderman, "minecraft:entity/enderman/enderman_eyes").glowing());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_creepy_enderman_trembles_by_a_few_hundredths_of_a_block() {
        let mut largest = 0.0f32;
        let mut differs = false;
        let first = creepy_shake(10.0, 1.0);
        for tick in 0..200 {
            let (x, z) = creepy_shake(tick as f32 * 0.37, 1.0);
            largest = largest.max(x.abs()).max(z.abs());
            differs |= (x, z) != first;
        }
        assert!(differs);
        assert!(largest > 0.005 && largest < 0.15, "{largest}");
    }

    #[test]
    fn the_carried_block_hangs_in_front_of_the_body() {
        use glam::Vec3;
        let at = carried_block_transform(glam::Mat4::IDENTITY).transform_point3(Vec3::splat(0.5));
        assert!((at.y - 0.6875).abs() < 0.5 && at.z < -0.4, "{at}");
    }

    use crate::entity::model::Model;

    #[test]
    fn a_carried_block_lifts_the_arms_and_a_creepy_head_drops() {
        let model = Model::bake(&layer());
        let parts = HumanoidParts::find(&model);
        let mut rest = model.initial_pose();
        setup_anim(&parts, &mut rest, EndermanPose::default());
        let mut carrying = model.initial_pose();
        setup_anim(
            &parts,
            &mut carrying,
            EndermanPose {
                carrying: true,
                creepy: true,
            },
        );
        assert_eq!(carrying[parts.right_arm].pose.rotation.x, -0.5);
        assert_eq!(
            carrying[parts.head].pose.offset.y,
            rest[parts.head].pose.offset.y - 5.0
        );
    }
}
