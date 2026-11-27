//! `DolphinModel` and `BabyDolphinModel`.

use crate::math::angle::THIRD_PI as THIRD;

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle;
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

fn turned(offset: (f32, f32, f32), rotation: (f32, f32, f32)) -> Pose {
    Pose::offset_rotation(offset, rotation)
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((22.0, 0.0), (-4.0, -7.0, 0.0), (8.0, 7.0, 13.0))],
        Pose::offset(0.0, 22.0, -5.0),
    );
    body.add_or_replace_child(
        "back_fin",
        vec![Cube::new((51.0, 0.0), (-0.5, 0.0, 8.0), (1.0, 4.0, 5.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (THIRD, 0.0, 0.0)),
    );
    body.add_or_replace_child(
        "left_fin",
        vec![Cube::new((48.0, 20.0), (-0.5, -4.0, 0.0), (1.0, 4.0, 7.0)).mirrored()],
        turned((2.0, -2.0, 4.0), (THIRD, 0.0, THIRD * 2.0)),
    );
    body.add_or_replace_child(
        "right_fin",
        vec![Cube::new((48.0, 20.0), (-0.5, -4.0, 0.0), (1.0, 4.0, 7.0))],
        turned((-2.0, -2.0, 4.0), (THIRD, 0.0, -THIRD * 2.0)),
    );
    let tail = body.add_or_replace_child(
        "tail",
        vec![Cube::new((0.0, 19.0), (-2.0, -2.5, 0.0), (4.0, 5.0, 11.0))],
        turned((0.0, -2.5, 11.0), (-0.104_719_76, 0.0, 0.0)),
    );
    tail.add_or_replace_child(
        "tail_fin",
        vec![Cube::new((19.0, 20.0), (-5.0, -0.5, 0.0), (10.0, 1.0, 6.0))],
        Pose::offset(0.0, 0.0, 9.0),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -3.0, -3.0), (8.0, 7.0, 6.0))],
        Pose::offset(0.0, -4.0, -3.0),
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((0.0, 13.0), (-1.0, 2.0, -7.0), (2.0, 2.0, 4.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((20.0, 0.0), (-3.0, -2.5, -4.0), (6.0, 5.0, 8.0))],
        Pose::offset(0.0, 21.5, 0.0),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-3.0, -3.5, -4.0), (6.0, 5.0, 4.0))],
        Pose::offset(0.0, 1.0, -4.0),
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((0.0, 9.0), (-1.0, -1.0, -2.0), (2.0, 2.0, 2.0))],
        Pose::offset(0.0, 0.5, -4.0),
    );
    body.add_or_replace_child(
        "left_fin",
        vec![Cube::new((34.0, 18.0), (-0.5, -1.5, -0.5), (1.0, 3.0, 6.0))],
        turned((1.8, 0.85, -2.6), (0.8727, 0.0, 1.7017)),
    );
    body.add_or_replace_child(
        "right_fin",
        vec![Cube::new((48.0, 18.0), (-0.5, -1.5, -0.5), (1.0, 3.0, 6.0))],
        turned((-1.8, 0.85, -2.6), (0.8727, 0.0, -1.7017)),
    );
    let tail = body.add_or_replace_child(
        "tail",
        vec![Cube::new((0.0, 13.0), (-2.0, -1.5, 0.0), (4.0, 3.0, 7.0))],
        Pose::offset(0.0, 1.0, 4.0),
    );
    tail.add_or_replace_child(
        "tail_fin",
        vec![Cube::new((22.0, 13.0), (-4.0, -0.5, -1.0), (8.0, 1.0, 4.0))],
        Pose::offset(0.0, 0.0, 6.0),
    );
    body.add_or_replace_child(
        "back_fin",
        vec![Cube::new((42.0, 0.0), (-0.5, -1.0, 1.0), (1.0, 3.0, 4.0))],
        turned((0.0, -1.0, -2.7), (0.8727, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct DolphinParts {
    body: PartId,
    tail: PartId,
    tail_fin: PartId,
}

impl DolphinParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
            tail: model.part("tail"),
            tail_fin: model.part("tail_fin"),
        }
    }
}

/// `DolphinModel.setupAnim`.
pub fn setup_anim(
    parts: &DolphinParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    moving: bool,
) {
    let body = &mut pose[parts.body].pose;
    body.rotation.x = state.x_rot.to_radians();
    body.rotation.y = state.y_rot.to_radians();
    if moving {
        let beat = angle::cos(state.age_in_ticks * 0.3);
        body.rotation.x += -0.05 - 0.05 * beat;
        pose[parts.tail].pose.rotation.x = -0.1 * beat;
        pose[parts.tail_fin].pose.rotation.x = -0.2 * beat;
    }
}

/// `DolphinRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::Dolphin, "minecraft:entity/dolphin/dolphin"),
        (M::DolphinBaby, "minecraft:entity/dolphin/dolphin_baby"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_follows_the_look_and_the_tail_beats_only_when_moving() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = DolphinParts::find(&model);
            let state = LivingRenderState {
                x_rot: 20.0,
                y_rot: -30.0,
                age_in_ticks: 4.0,
                ..Default::default()
            };
            let mut still = model.initial_pose();
            setup_anim(&parts, &mut still, &state, false);
            assert!((still[parts.body].pose.rotation.y + 30.0_f32.to_radians()).abs() < 1e-5);
            let rest_tail = model.initial(parts.tail).rotation.x;
            assert_eq!(still[parts.tail].pose.rotation.x, rest_tail);
            let mut moving = model.initial_pose();
            setup_anim(&parts, &mut moving, &state, true);
            assert_ne!(moving[parts.tail].pose.rotation.x, rest_tail);
        }
    }
}
