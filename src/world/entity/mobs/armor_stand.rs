//! `ArmorStandModel` and `ArmorStandArmorModel`: a frame of sticks that

use glam::Vec3;

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::world::entity::appearance::{
    Appearance, DEFAULT_STAND_POSE, STAND_ARMS, STAND_NO_BASE_PLATE, StandPose,
};
use crate::world::entity::humanoid::{self, HumanoidParts};
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

pub const TEXTURE: &str = "minecraft:entity/armorstand/armorstand";

/// `ArmorStandModel.createBodyLayer`.
pub fn body() -> LayerDefinition {
    let mut root = humanoid::create_mesh(0.0, 0.0);
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-1.0, -7.0, -1.0), (2.0, 7.0, 2.0))],
        Pose::offset(0.0, 1.0, 0.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 26.0), (-6.0, 0.0, -1.5), (12.0, 3.0, 3.0))],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "right_arm",
        vec![Cube::new((24.0, 0.0), (-2.0, -2.0, -1.0), (2.0, 12.0, 2.0))],
        Pose::offset(-5.0, 2.0, 0.0),
    );
    root.add_or_replace_child(
        "left_arm",
        vec![Cube::new((32.0, 16.0), (0.0, -2.0, -1.0), (2.0, 12.0, 2.0)).mirrored()],
        Pose::offset(5.0, 2.0, 0.0),
    );
    root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((8.0, 0.0), (-1.0, 0.0, -1.0), (2.0, 11.0, 2.0))],
        Pose::offset(-1.9, 12.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((40.0, 16.0), (-1.0, 0.0, -1.0), (2.0, 11.0, 2.0)).mirrored()],
        Pose::offset(1.9, 12.0, 0.0),
    );
    root.add_or_replace_child(
        "right_body_stick",
        vec![Cube::new((16.0, 0.0), (-3.0, 3.0, -1.0), (2.0, 7.0, 2.0))],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "left_body_stick",
        vec![Cube::new((48.0, 16.0), (1.0, 3.0, -1.0), (2.0, 7.0, 2.0))],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "shoulder_stick",
        vec![Cube::new((0.0, 48.0), (-4.0, 10.0, -1.0), (8.0, 2.0, 2.0))],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "base_plate",
        vec![Cube::new(
            (0.0, 32.0),
            (-6.0, 11.0, -6.0),
            (12.0, 1.0, 12.0),
        )],
        Pose::offset(0.0, 12.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `ArmorStandArmorModel.createBaseMesh`: the humanoid's, with the head a
fn base_armor_mesh(grow: f32) -> PartDefinition {
    let mut root = humanoid::create_mesh(grow, 0.0);
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)).grown(grow)],
        Pose::offset(0.0, 1.0, 0.0),
    );
    for (name, x, mirror) in [("right_leg", -1.9, false), ("left_leg", 1.9, true)] {
        let mut cube =
            Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(grow - 0.1);
        if mirror {
            cube = cube.mirrored();
        }
        root.add_or_replace_child(name, vec![cube], Pose::offset(x, 11.0, 0.0));
    }
    root
}

/// The armor of a full-size stand, by [`ArmorSlot::ALL`].
pub fn armor() -> [LayerDefinition; 4] {
    humanoid::armor_layers(base_armor_mesh, 0.5, 1.0)
}

/// `HumanoidModel.BABY_TRANSFORMER` for a small stand: the head moved down
fn small(mut layer: LayerDefinition) -> LayerDefinition {
    for part in &mut layer.root.children {
        let (down, factor) = if part.name == "head" {
            (16.0, 0.75)
        } else {
            (24.0, 0.5)
        };
        part.pose.offset += Vec3::new(0.0, down, 0.0);
        part.pose = part.pose.scaled(factor);
    }
    layer
}

/// `ARMOR_STAND_SMALL`.
pub fn small_body() -> LayerDefinition {
    small(body())
}

/// `ARMOR_STAND_SMALL_ARMOR`, by [`ArmorSlot::ALL`].
pub fn small_armor() -> [LayerDefinition; 4] {
    armor().map(small)
}

/// The parts a stand poses: the humanoid's, and its extra sticks.
#[derive(Clone, Copy, Debug)]
pub struct ArmorStandParts {
    pub humanoid: HumanoidParts,
    pub right_stick: PartId,
    pub left_stick: PartId,
    pub shoulder_stick: PartId,
    pub base_plate: PartId,
}

impl ArmorStandParts {
    pub fn find(model: &Model) -> Self {
        Self {
            humanoid: HumanoidParts::find(model),
            right_stick: model.part("right_body_stick"),
            left_stick: model.part("left_body_stick"),
            shoulder_stick: model.part("shoulder_stick"),
            base_plate: model.part("base_plate"),
        }
    }
}

/// What a stand's pose reads: the synced angles (degrees), whether it
#[derive(Clone, Copy, Debug)]
pub struct StandPoseInput {
    pub angles: [[f32; 3]; 6],
    pub arms: bool,
    pub base_plate: bool,
    /// `ArmorStandRenderState.yRot`, vanilla's degrees.
    pub y_rot: f32,
}

impl Default for StandPoseInput {
    fn default() -> Self {
        Self {
            angles: DEFAULT_STAND_POSE,
            arms: false,
            base_plate: true,
            y_rot: 0.0,
        }
    }
}

impl StandPoseInput {
    pub fn of(a: &Appearance, y_rot: f32) -> Self {
        Self {
            angles: a.stand_pose,
            arms: a.stand_flags & STAND_ARMS != 0,
            base_plate: a.stand_flags & STAND_NO_BASE_PLATE == 0,
            y_rot,
        }
    }
}

fn radians(angles: [f32; 3]) -> Vec3 {
    Vec3::from(angles) * (std::f32::consts::PI / 180.0)
}

/// `ArmorStandModel.setupAnim`: the limbs turned by the synced angles
pub fn setup_anim(parts: &ArmorStandParts, pose: &mut ModelPose, input: &StandPoseInput) {
    let h = &parts.humanoid;
    let angle = |limb: StandPose| radians(input.angles[limb as usize]);
    pose[h.head].pose.rotation = angle(StandPose::Head);
    pose[h.hat].visible = false;
    let body = angle(StandPose::Body);
    pose[h.body].pose.rotation = body;
    pose[h.left_arm].pose.rotation = angle(StandPose::LeftArm);
    pose[h.right_arm].pose.rotation = angle(StandPose::RightArm);
    pose[h.left_leg].pose.rotation = angle(StandPose::LeftLeg);
    pose[h.right_leg].pose.rotation = angle(StandPose::RightLeg);
    pose[h.left_arm].visible = input.arms;
    pose[h.right_arm].visible = input.arms;
    for stick in [parts.right_stick, parts.left_stick, parts.shoulder_stick] {
        pose[stick].pose.rotation = body;
    }
    pose[parts.base_plate].visible = input.base_plate;
    pose[parts.base_plate].pose.rotation.y = -input.y_rot * (std::f32::consts::PI / 180.0);
}

/// `ArmorStandRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(
        if a.stand_small() {
            M::ArmorStandSmall
        } else {
            M::ArmorStand
        },
        TEXTURE,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_angles_are_vanillas() {
        let input = StandPoseInput::default();
        assert_eq!(
            input.angles[StandPose::LeftArm as usize],
            [-10.0, 0.0, -10.0]
        );
        assert_eq!(
            input.angles[StandPose::RightArm as usize],
            [-15.0, 0.0, 10.0]
        );
        assert_eq!(input.angles[StandPose::LeftLeg as usize], [-1.0, 0.0, -1.0]);
        assert_eq!(input.angles[StandPose::RightLeg as usize], [1.0, 0.0, 1.0]);
    }

    #[test]
    fn the_pose_turns_limbs_hides_arms_and_the_hat() {
        let model = Model::bake(&body());
        let parts = ArmorStandParts::find(&model);
        let mut pose = model.initial_pose();
        let mut input = StandPoseInput::default();
        input.angles[StandPose::Head as usize] = [90.0, 0.0, 0.0];
        input.base_plate = false;
        setup_anim(&parts, &mut pose, &input);
        let h = &parts.humanoid;
        assert!((pose[h.head].pose.rotation.x - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!(!pose[h.hat].visible);
        assert!(!pose[h.left_arm].visible && !pose[h.right_arm].visible);
        assert!(!pose[parts.base_plate].visible);
        input.arms = true;
        setup_anim(&parts, &mut pose, &input);
        assert!(pose[h.left_arm].visible);
    }

    #[test]
    fn a_small_stand_is_half_the_size_and_its_head_three_quarters() {
        let big = Model::bake(&body());
        let little = Model::bake(&small_body());
        let body_part = little.part("body");
        assert_eq!(little.initial(body_part).scale, Vec3::splat(0.5));
        assert_eq!(little.initial(little.part("head")).scale, Vec3::splat(0.75));
        assert_eq!(big.initial(big.part("body")).scale, Vec3::ONE);
    }
}
