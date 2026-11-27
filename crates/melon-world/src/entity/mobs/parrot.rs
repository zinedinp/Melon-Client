//! `ParrotModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, HALF_PI, PI, SIXTH_PI};

/// `ParrotModel.Pose`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParrotPose {
    Flying,
    #[default]
    Standing,
    Sitting,
    Party,
    OnShoulder,
}

/// `ParrotRenderer.getVariantTexture`: red_blue, blue, green, yellow_blue, grey.
pub fn texture(variant: i32) -> &'static str {
    const TEXTURES: [&str; 5] = [
        "minecraft:entity/parrot/parrot_red_blue",
        "minecraft:entity/parrot/parrot_blue",
        "minecraft:entity/parrot/parrot_green",
        "minecraft:entity/parrot/parrot_yellow_blue",
        "minecraft:entity/parrot/parrot_grey",
    ];
    TEXTURES[usize::try_from(variant)
        .ok()
        .filter(|&i| i < 5)
        .unwrap_or(0)]
}

/// `ParrotModel.getPose`.
pub fn pose(party: bool, sitting: bool, flying: bool) -> ParrotPose {
    if party {
        ParrotPose::Party
    } else if sitting {
        ParrotPose::Sitting
    } else if flying {
        ParrotPose::Flying
    } else {
        ParrotPose::Standing
    }
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((2.0, 8.0), (-1.5, 0.0, -1.5), (3.0, 6.0, 3.0))],
        Pose::offset_rotation((0.0, 16.5, -3.0), (0.4937, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "tail",
        vec![Cube::new((22.0, 1.0), (-1.5, -1.0, -1.0), (3.0, 4.0, 1.0))],
        Pose::offset_rotation((0.0, 21.07, 1.16), (1.015, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "left_wing",
        vec![Cube::new((19.0, 8.0), (-0.5, 0.0, -1.5), (1.0, 5.0, 3.0))],
        Pose::offset_rotation((1.5, 16.94, -2.76), (-0.6981, -PI, 0.0)),
    );
    root.add_or_replace_child(
        "right_wing",
        vec![Cube::new((19.0, 8.0), (-0.5, 0.0, -1.5), (1.0, 5.0, 3.0))],
        Pose::offset_rotation((-1.5, 16.94, -2.76), (-0.6981, -PI, 0.0)),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((2.0, 2.0), (-1.0, -1.5, -1.0), (2.0, 3.0, 2.0))],
        Pose::offset(0.0, 15.69, -2.76),
    );
    head.add_or_replace_child(
        "head2",
        vec![Cube::new((10.0, 0.0), (-1.0, -0.5, -2.0), (2.0, 1.0, 4.0))],
        Pose::offset(0.0, -2.0, -1.0),
    );
    head.add_or_replace_child(
        "beak1",
        vec![Cube::new((11.0, 7.0), (-0.5, -1.0, -0.5), (1.0, 2.0, 1.0))],
        Pose::offset(0.0, -0.5, -1.5),
    );
    head.add_or_replace_child(
        "beak2",
        vec![Cube::new((16.0, 7.0), (-0.5, 0.0, -0.5), (1.0, 2.0, 1.0))],
        Pose::offset(0.0, -1.75, -2.45),
    );
    head.add_or_replace_child(
        "feather",
        vec![Cube::new((2.0, 18.0), (0.0, -4.0, -2.0), (0.0, 5.0, 4.0))],
        Pose::offset_rotation((0.0, -2.15, 0.15), (-0.2214, 0.0, 0.0)),
    );
    let leg = || vec![Cube::new((14.0, 18.0), (-0.5, 0.0, -0.5), (1.0, 2.0, 1.0))];
    root.add_or_replace_child(
        "left_leg",
        leg(),
        Pose::offset_rotation((1.0, 22.0, -1.05), (-0.0299, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "right_leg",
        leg(),
        Pose::offset_rotation((-1.0, 22.0, -1.05), (-0.0299, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct ParrotParts {
    body: PartId,
    tail: PartId,
    left_wing: PartId,
    right_wing: PartId,
    head: PartId,
    left_leg: PartId,
    right_leg: PartId,
}

impl ParrotParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
            tail: model.part("tail"),
            left_wing: model.part("left_wing"),
            right_wing: model.part("right_wing"),
            head: model.part("head"),
            left_leg: model.part("left_leg"),
            right_leg: model.part("right_leg"),
        }
    }
}

/// `ParrotModel.prepare`.
fn prepare(parts: &ParrotParts, pose: &mut ModelPose, parrot: ParrotPose) {
    match parrot {
        ParrotPose::Flying => {
            pose[parts.left_leg].pose.rotation.x += PI * 2.0 / 9.0;
            pose[parts.right_leg].pose.rotation.x += PI * 2.0 / 9.0;
        }
        ParrotPose::Sitting => {
            let sit = 1.9;
            pose[parts.head].pose.offset.y += sit;
            pose[parts.tail].pose.rotation.x += SIXTH_PI;
            pose[parts.tail].pose.offset.y += sit;
            pose[parts.body].pose.offset.y += sit;
            pose[parts.left_wing].pose.rotation.z = -0.0873;
            pose[parts.left_wing].pose.offset.y += sit;
            pose[parts.right_wing].pose.rotation.z = 0.0873;
            pose[parts.right_wing].pose.offset.y += sit;
            pose[parts.left_leg].pose.offset.y += sit;
            pose[parts.right_leg].pose.offset.y += sit;
            pose[parts.left_leg].pose.rotation.x += HALF_PI;
            pose[parts.right_leg].pose.rotation.x += HALF_PI;
        }
        ParrotPose::Party => {
            pose[parts.left_leg].pose.rotation.z = -PI / 9.0;
            pose[parts.right_leg].pose.rotation.z = PI / 9.0;
        }
        ParrotPose::Standing | ParrotPose::OnShoulder => {}
    }
}

fn bob(parts: &ParrotParts, pose: &mut ModelPose, state: &LivingRenderState, flap_angle: f32) {
    let bobbing = flap_angle * 0.3;
    pose[parts.head].pose.offset.y += bobbing;
    pose[parts.tail].pose.rotation.x +=
        angle::cos(state.walk_position * 0.6662) * 0.3 * state.walk_speed;
    pose[parts.tail].pose.offset.y += bobbing;
    pose[parts.body].pose.offset.y += bobbing;
    pose[parts.left_wing].pose.rotation.z = -0.0873 - flap_angle;
    pose[parts.left_wing].pose.offset.y += bobbing;
    pose[parts.right_wing].pose.rotation.z = 0.0873 + flap_angle;
    pose[parts.right_wing].pose.offset.y += bobbing;
    pose[parts.left_leg].pose.offset.y += bobbing;
    pose[parts.right_leg].pose.offset.y += bobbing;
}

/// `ParrotModel.setupAnim`.
pub fn setup_anim(
    parts: &ParrotParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    parrot: ParrotPose,
    flap_angle: f32,
) {
    prepare(parts, pose, parrot);
    pose[parts.head].pose.rotation.x = state.x_rot.to_radians();
    pose[parts.head].pose.rotation.y = state.y_rot.to_radians();
    match parrot {
        ParrotPose::Standing => {
            pose[parts.left_leg].pose.rotation.x +=
                angle::cos(state.walk_position * 0.6662) * 1.4 * state.walk_speed;
            pose[parts.right_leg].pose.rotation.x +=
                angle::cos(state.walk_position * 0.6662 + PI) * 1.4 * state.walk_speed;
            bob(parts, pose, state, flap_angle);
        }
        ParrotPose::Flying | ParrotPose::OnShoulder => bob(parts, pose, state, flap_angle),
        ParrotPose::Sitting => {}
        ParrotPose::Party => {
            let x = angle::cos(state.age_in_ticks);
            let y = angle::sin(state.age_in_ticks);
            pose[parts.head].pose.offset.x += x;
            pose[parts.head].pose.offset.y += y;
            pose[parts.head].pose.rotation.x = 0.0;
            pose[parts.head].pose.rotation.y = 0.0;
            pose[parts.head].pose.rotation.z = angle::sin(state.age_in_ticks) * 0.4;
            pose[parts.body].pose.offset.x += x;
            pose[parts.body].pose.offset.y += y;
            pose[parts.left_wing].pose.rotation.z = -0.0873 - flap_angle;
            pose[parts.left_wing].pose.offset.x += x;
            pose[parts.left_wing].pose.offset.y += y;
            pose[parts.right_wing].pose.rotation.z = 0.0873 + flap_angle;
            pose[parts.right_wing].pose.offset.x += x;
            pose[parts.right_wing].pose.offset.y += y;
            pose[parts.tail].pose.offset.x += x;
            pose[parts.tail].pose.offset.y += y;
        }
    }
}

/// `ParrotRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(M::Parrot, texture(a.parrot_variant)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posed(
        parrot: ParrotPose,
        flap: f32,
        state: LivingRenderState,
    ) -> (Model, ParrotParts, ModelPose) {
        let model = Model::bake(&layer());
        let parts = ParrotParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(&parts, &mut pose, &state, parrot, flap);
        (model, parts, pose)
    }

    #[test]
    fn the_mesh_bakes_and_a_sitter_drops_while_a_dancer_sways() {
        let sitting_y = {
            let (model, parts, pose) =
                posed(ParrotPose::Sitting, 0.0, LivingRenderState::default());
            assert!((pose[parts.left_leg].pose.rotation.x - (-0.0299 + HALF_PI)).abs() < 1e-4);
            pose[parts.head].pose.offset.y - model.initial(parts.head).offset.y
        };
        assert!((sitting_y - 1.9).abs() < 1e-5);

        let state = LivingRenderState {
            age_in_ticks: 1.0,
            x_rot: 20.0,
            ..Default::default()
        };
        let (_, parts, dance) = posed(ParrotPose::Party, 0.5, state);
        assert_eq!(dance[parts.head].pose.rotation.x, 0.0);
        assert!(dance[parts.head].pose.offset.x.abs() > 0.1);
        assert!((dance[parts.left_leg].pose.rotation.z + PI / 9.0).abs() < 1e-5);
    }

    #[test]
    fn flying_tucks_the_legs_and_the_wings_follow_the_flap() {
        let (_, parts, pose) = posed(ParrotPose::Flying, 0.4, LivingRenderState::default());
        assert!((pose[parts.left_leg].pose.rotation.x - (-0.0299 + PI * 2.0 / 9.0)).abs() < 1e-4);
        assert!((pose[parts.left_wing].pose.rotation.z + 0.0873 + 0.4).abs() < 1e-5);
        assert!((pose[parts.right_wing].pose.rotation.z - (0.0873 + 0.4)).abs() < 1e-5);
    }

    #[test]
    fn variants_map_onto_the_five_textures() {
        assert_eq!(texture(0), "minecraft:entity/parrot/parrot_red_blue");
        assert_eq!(texture(4), "minecraft:entity/parrot/parrot_grey");
        assert_eq!(texture(99), "minecraft:entity/parrot/parrot_red_blue");
        assert_eq!(pose(true, true, true), ParrotPose::Party);
        assert_eq!(pose(false, true, true), ParrotPose::Sitting);
        assert_eq!(pose(false, false, true), ParrotPose::Flying);
        assert_eq!(pose(false, false, false), ParrotPose::Standing);
    }
}
