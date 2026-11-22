//! `VexModel`: a small humanoid (head, body, two arms) with a pair of

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::{self, DEG_TO_RAD, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// `Vex.FLAG_IS_CHARGING`.
pub const FLAG_CHARGING: u8 = 1;

/// What the pose and texture read besides the living state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VexPose {
    pub charging: bool,
    pub right_item: bool,
    pub left_item: bool,
}

/// `VexRenderer.getTextureLocation`.
pub fn texture(charging: bool) -> &'static str {
    if charging {
        "minecraft:entity/illager/vex_charging"
    } else {
        "minecraft:entity/illager/vex"
    }
}

pub fn layer() -> LayerDefinition {
    let mut mesh = PartDefinition::root();
    let root = mesh.add_or_replace_child("root", vec![], Pose::offset(0.0, -2.5, 0.0));
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-2.5, -5.0, -2.5), (5.0, 5.0, 5.0))],
        Pose::offset(0.0, 20.0, 0.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 10.0), (-1.5, 0.0, -1.0), (3.0, 4.0, 2.0)),
            Cube::new((0.0, 16.0), (-1.5, 1.0, -1.0), (3.0, 5.0, 2.0)).grown(-0.2),
        ],
        Pose::offset(0.0, 20.0, 0.0),
    );
    body.add_or_replace_child(
        "right_arm",
        vec![Cube::new((23.0, 0.0), (-1.25, -0.5, -1.0), (2.0, 4.0, 2.0)).grown(-0.1)],
        Pose::offset(-1.75, 0.25, 0.0),
    );
    body.add_or_replace_child(
        "left_arm",
        vec![Cube::new((23.0, 6.0), (-0.75, -0.5, -1.0), (2.0, 4.0, 2.0)).grown(-0.1)],
        Pose::offset(1.75, 0.25, 0.0),
    );
    body.add_or_replace_child(
        "left_wing",
        vec![Cube::new((16.0, 14.0), (0.0, 0.0, 0.0), (0.0, 5.0, 8.0)).mirrored()],
        Pose::offset(0.5, 1.0, 1.0),
    );
    body.add_or_replace_child(
        "right_wing",
        vec![Cube::new((16.0, 14.0), (0.0, 0.0, 0.0), (0.0, 5.0, 8.0))],
        Pose::offset(-0.5, 1.0, 1.0),
    );
    LayerDefinition::new(mesh, 32.0, 32.0)
}

/// Humanoid-named parts plus the wings.
#[derive(Clone, Copy, Debug)]
pub struct VexParts {
    pub head: PartId,
    pub body: PartId,
    pub right_arm: PartId,
    pub left_arm: PartId,
    pub right_wing: PartId,
    pub left_wing: PartId,
}

impl VexParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            body: model.part("body"),
            right_arm: model.part("right_arm"),
            left_arm: model.part("left_arm"),
            right_wing: model.part("right_wing"),
            left_wing: model.part("left_wing"),
        }
    }
}

fn set_arms_charging(parts: &VexParts, pose: &mut ModelPose, vex: VexPose, bob: f32) {
    if !vex.right_item && !vex.left_item {
        let right = &mut pose[parts.right_arm].pose.rotation;
        right.x = -1.2217305;
        right.y = PI / 12.0;
        right.z = -0.47123888 - bob;
        let left = &mut pose[parts.left_arm].pose.rotation;
        left.x = -1.2217305;
        left.y = -PI / 12.0;
        left.z = 0.47123888 + bob;
        return;
    }
    if vex.right_item {
        let right = &mut pose[parts.right_arm].pose.rotation;
        right.x = PI * 7.0 / 6.0;
        right.y = PI / 12.0;
        right.z = -0.47123888 - bob;
    }
    if vex.left_item {
        let left = &mut pose[parts.left_arm].pose.rotation;
        left.x = PI * 7.0 / 6.0;
        left.y = -PI / 12.0;
        left.z = 0.47123888 + bob;
    }
}

/// `VexModel.setupAnim`.
pub fn setup_anim(parts: &VexParts, pose: &mut ModelPose, state: &LivingRenderState, vex: VexPose) {
    pose[parts.head].pose.rotation.y = state.y_rot.to_radians();
    pose[parts.head].pose.rotation.x = state.x_rot.to_radians();
    let bob = angle::cos(state.age_in_ticks * 5.5 * DEG_TO_RAD) * 0.1;
    pose[parts.right_arm].pose.rotation.z = PI / 5.0 + bob;
    pose[parts.left_arm].pose.rotation.z = -(PI / 5.0 + bob);
    if vex.charging {
        pose[parts.body].pose.rotation.x = 0.0;
        set_arms_charging(parts, pose, vex, bob);
    } else {
        pose[parts.body].pose.rotation.x = PI / 20.0;
    }
    let left_y =
        1.0995574 + angle::cos(state.age_in_ticks * 45.836624 * DEG_TO_RAD) * DEG_TO_RAD * 16.2;
    pose[parts.left_wing].pose.rotation.y = left_y;
    pose[parts.right_wing].pose.rotation.y = -left_y;
    pose[parts.left_wing].pose.rotation.x = 0.47123888;
    pose[parts.left_wing].pose.rotation.z = -0.47123888;
    pose[parts.right_wing].pose.rotation.x = 0.47123888;
    pose[parts.right_wing].pose.rotation.z = 0.47123888;
}

/// `VexRenderer`: charging or not.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(M::Vex, texture(a.charging)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mesh_bakes_and_charging_throws_the_arms_forward() {
        let model = Model::bake(&layer());
        let parts = VexParts::find(&model);
        let state = LivingRenderState {
            x_rot: 10.0,
            y_rot: 20.0,
            age_in_ticks: 0.0,
            ..Default::default()
        };
        let mut idle = model.initial_pose();
        setup_anim(&parts, &mut idle, &state, VexPose::default());
        assert!((idle[parts.body].pose.rotation.x - PI / 20.0).abs() < 1e-5);
        assert!((idle[parts.head].pose.rotation.y - 20.0_f32.to_radians()).abs() < 1e-5);
        assert_eq!(
            idle[parts.right_wing].pose.rotation.y,
            -idle[parts.left_wing].pose.rotation.y
        );

        let mut charge = model.initial_pose();
        setup_anim(
            &parts,
            &mut charge,
            &state,
            VexPose {
                charging: true,
                ..Default::default()
            },
        );
        assert_eq!(charge[parts.body].pose.rotation.x, 0.0);
        assert!((charge[parts.right_arm].pose.rotation.x + 1.2217305).abs() < 1e-5);

        let mut sword = model.initial_pose();
        setup_anim(
            &parts,
            &mut sword,
            &state,
            VexPose {
                charging: true,
                right_item: true,
                left_item: false,
            },
        );
        assert!((sword[parts.right_arm].pose.rotation.x - PI * 7.0 / 6.0).abs() < 1e-5);
        assert!((sword[parts.left_arm].pose.rotation.x).abs() < 1e-5);
    }

    #[test]
    fn charging_swaps_the_texture() {
        assert_eq!(texture(false), "minecraft:entity/illager/vex");
        assert_eq!(texture(true), "minecraft:entity/illager/vex_charging");
        assert_eq!(FLAG_CHARGING, 1);
    }
}
