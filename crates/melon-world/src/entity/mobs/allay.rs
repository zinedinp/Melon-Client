//! `AllayModel`.

use super::{LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::math::angle::{self, DEG_TO_RAD, PI, QUARTER_PI, THIRD_PI};
use crate::model_part::{Cube, Pose};

pub const TEXTURE: &str = "minecraft:entity/allay/allay";

/// `AllayModel.FLYING_ANIMATION_X_ROT`.
const FLYING_X: f32 = QUARTER_PI;
/// `AllayModel.MAX_HAND_HOLDING_ITEM_X_ROT_RAD`.
const HOLD_FLYING: f32 = -1.134464;
/// `AllayModel.MIN_HAND_HOLDING_ITEM_X_ROT_RAD`.
const HOLD_IDLE: f32 = -THIRD_PI;

/// `AllayRenderState` as far as the pose reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AllayPose {
    pub dancing: bool,
    pub spinning: bool,
    pub spinning_progress: f32,
    /// `holdingAnimationProgress`, 0 empty to 1 holding.
    pub holding: f32,
}

pub fn layer() -> LayerDefinition {
    let mut mesh = PartDefinition::root();
    let root = mesh.add_or_replace_child("root", vec![], Pose::offset(0.0, 23.5, 0.0));
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-2.5, -5.0, -2.5), (5.0, 5.0, 5.0))],
        Pose::offset(0.0, -3.99, 0.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 10.0), (-1.5, 0.0, -1.0), (3.0, 4.0, 2.0)),
            Cube::new((0.0, 16.0), (-1.5, 0.0, -1.0), (3.0, 5.0, 2.0)).grown(-0.2),
        ],
        Pose::offset(0.0, -4.0, 0.0),
    );
    body.add_or_replace_child(
        "right_arm",
        vec![Cube::new((23.0, 0.0), (-0.75, -0.5, -1.0), (1.0, 4.0, 2.0)).grown(-0.01)],
        Pose::offset(-1.75, 0.5, 0.0),
    );
    body.add_or_replace_child(
        "left_arm",
        vec![Cube::new((23.0, 6.0), (-0.25, -0.5, -1.0), (1.0, 4.0, 2.0)).grown(-0.01)],
        Pose::offset(1.75, 0.5, 0.0),
    );
    body.add_or_replace_child(
        "right_wing",
        vec![Cube::new((16.0, 14.0), (0.0, 1.0, 0.0), (0.0, 5.0, 8.0))],
        Pose::offset(-0.5, 0.0, 0.6),
    );
    body.add_or_replace_child(
        "left_wing",
        vec![Cube::new((16.0, 14.0), (0.0, 1.0, 0.0), (0.0, 5.0, 8.0))],
        Pose::offset(0.5, 0.0, 0.6),
    );
    LayerDefinition::new(mesh, 32.0, 32.0)
}

/// Vanilla's nested `root` (the mesh root is also named `root`).
fn vanilla_root(model: &Model) -> PartId {
    (1..model.len())
        .map(PartId)
        .find(|&id| model.name(id) == "root")
        .expect("allay nested root")
}

#[derive(Clone, Copy, Debug)]
pub struct AllayParts {
    root: PartId,
    head: PartId,
    body: PartId,
    right_arm: PartId,
    left_arm: PartId,
    right_wing: PartId,
    left_wing: PartId,
}

impl AllayParts {
    pub fn find(model: &Model) -> Self {
        Self {
            root: vanilla_root(model),
            head: model.part("head"),
            body: model.part("body"),
            right_arm: model.part("right_arm"),
            left_arm: model.part("left_arm"),
            right_wing: model.part("right_wing"),
            left_wing: model.part("left_wing"),
        }
    }
}

/// `AllayModel.setupAnim`.
pub fn setup_anim(
    parts: &AllayParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    allay: AllayPose,
) {
    let speed = state.walk_speed;
    let pos = state.walk_position;
    let flap_speed = state.age_in_ticks * 20.0 * DEG_TO_RAD + pos;
    let flap = angle::cos(flap_speed) * PI * 0.15 + speed;
    let idle_bob_speed = state.age_in_ticks * 9.0 * DEG_TO_RAD;
    let flying = (speed / 0.3).min(1.0);
    let idle = 1.0 - flying;
    let holding = allay.holding;
    if allay.dancing {
        let dance = state.age_in_ticks * 8.0 * DEG_TO_RAD + speed;
        let spin = allay.spinning_progress;
        if allay.spinning {
            pose[parts.root].pose.rotation.y = PI * 4.0 * spin;
        }
        pose[parts.root].pose.rotation.z = angle::cos(dance) * 16.0 * DEG_TO_RAD * (1.0 - spin);
        pose[parts.head].pose.rotation.y = angle::cos(dance) * 30.0 * DEG_TO_RAD * (1.0 - spin);
        pose[parts.head].pose.rotation.z = angle::cos(dance) * 14.0 * DEG_TO_RAD * (1.0 - spin);
    } else {
        pose[parts.head].pose.rotation.x = state.x_rot.to_radians();
        pose[parts.head].pose.rotation.y = state.y_rot.to_radians();
    }
    pose[parts.right_wing].pose.rotation.x = 0.43633232 * (1.0 - flying);
    pose[parts.right_wing].pose.rotation.y = -QUARTER_PI + flap;
    pose[parts.left_wing].pose.rotation.x = 0.43633232 * (1.0 - flying);
    pose[parts.left_wing].pose.rotation.y = QUARTER_PI - flap;
    pose[parts.body].pose.rotation.x = flying * FLYING_X;
    let arm_x = holding * (HOLD_IDLE + flying * (HOLD_FLYING - HOLD_IDLE));
    pose[parts.root].pose.offset.y += idle_bob_speed.cos() * 0.25 * idle;
    pose[parts.right_arm].pose.rotation.x = arm_x;
    pose[parts.left_arm].pose.rotation.x = arm_x;
    let arm_idle = idle * (1.0 - holding);
    let arm_z = 0.43633232 - angle::cos(idle_bob_speed + PI * 3.0 / 2.0) * PI * 0.075 * arm_idle;
    pose[parts.left_arm].pose.rotation.z = -arm_z;
    pose[parts.right_arm].pose.rotation.z = arm_z;
    pose[parts.right_arm].pose.rotation.y = 0.27925268 * holding;
    pose[parts.left_arm].pose.rotation.y = -0.27925268 * holding;
}

/// `AllayRenderer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Allay, TEXTURE));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mesh_bakes_and_flight_leans_the_body() {
        let model = Model::bake(&layer());
        let parts = AllayParts::find(&model);
        assert!((model.initial(parts.root).offset.y - 23.5).abs() < 1e-4);
        let mut idle = model.initial_pose();
        setup_anim(
            &parts,
            &mut idle,
            &LivingRenderState::default(),
            AllayPose::default(),
        );
        assert!((idle[parts.body].pose.rotation.x).abs() < 1e-5);
        assert!((idle[parts.right_wing].pose.rotation.x - 0.43633232).abs() < 1e-5);

        let mut flying = model.initial_pose();
        setup_anim(
            &parts,
            &mut flying,
            &LivingRenderState {
                walk_speed: 0.3,
                x_rot: 15.0,
                ..Default::default()
            },
            AllayPose::default(),
        );
        assert!((flying[parts.body].pose.rotation.x - FLYING_X).abs() < 1e-5);
        assert!(flying[parts.right_wing].pose.rotation.x.abs() < 1e-5);
        assert!((flying[parts.head].pose.rotation.x - 15.0_f32.to_radians()).abs() < 1e-5);
    }

    #[test]
    fn holding_raises_the_arms_and_dancing_turns_the_body() {
        let model = Model::bake(&layer());
        let parts = AllayParts::find(&model);
        let mut hold = model.initial_pose();
        setup_anim(
            &parts,
            &mut hold,
            &LivingRenderState::default(),
            AllayPose {
                holding: 1.0,
                ..Default::default()
            },
        );
        assert!((hold[parts.right_arm].pose.rotation.x - HOLD_IDLE).abs() < 1e-5);
        assert!((hold[parts.right_arm].pose.rotation.y - 0.27925268).abs() < 1e-5);
        assert_eq!(
            hold[parts.left_arm].pose.rotation.x,
            hold[parts.right_arm].pose.rotation.x
        );

        let mut dance = model.initial_pose();
        setup_anim(
            &parts,
            &mut dance,
            &LivingRenderState {
                age_in_ticks: 0.0,
                ..Default::default()
            },
            AllayPose {
                dancing: true,
                spinning: true,
                spinning_progress: 0.25,
                holding: 0.0,
            },
        );
        assert!((dance[parts.root].pose.rotation.y - PI).abs() < 1e-5);
        assert_eq!(TEXTURE, "minecraft:entity/allay/allay");
    }
}
