//! `PolarBearModel` and `BabyPolarBearModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::{self, QuadrupedParts};
use melon_core::math::angle::PI;
use crate::model_part::{Cube, Pose};

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-3.5, -3.0, -3.0), (7.0, 7.0, 7.0)),
            Cube::new((0.0, 44.0), (-2.5, 1.0, -6.0), (5.0, 3.0, 3.0)),
            Cube::new((26.0, 0.0), (-4.5, -4.0, -1.0), (2.0, 2.0, 1.0)),
            Cube::new((26.0, 0.0), (2.5, -4.0, -1.0), (2.0, 2.0, 1.0)).mirrored(),
        ],
        Pose::offset(0.0, 10.0, -16.0),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 19.0), (-5.0, -13.0, -7.0), (14.0, 14.0, 11.0)),
            Cube::new((39.0, 0.0), (-4.0, -25.0, -7.0), (12.0, 12.0, 10.0)),
        ],
        Pose::offset_rotation((-2.0, 9.0, 12.0), (PI / 2.0, 0.0, 0.0)),
    );
    let hind = || vec![Cube::new((50.0, 22.0), (-2.0, 0.0, -2.0), (4.0, 10.0, 8.0))];
    root.add_or_replace_child("right_hind_leg", hind(), Pose::offset(-4.5, 14.0, 6.0));
    root.add_or_replace_child("left_hind_leg", hind(), Pose::offset(4.5, 14.0, 6.0));
    let front = || vec![Cube::new((50.0, 40.0), (-2.0, 0.0, -2.0), (4.0, 10.0, 6.0))];
    root.add_or_replace_child("right_front_leg", front(), Pose::offset(-3.5, 14.0, -8.0));
    root.add_or_replace_child("left_front_leg", front(), Pose::offset(3.5, 14.0, -8.0));
    LayerDefinition::new(root, 128.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 9.0), (-4.0, -3.5, -6.0), (8.0, 7.0, 12.0))],
        Pose::offset(0.0, 17.5, 0.0),
    );
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-3.0, -2.625, -4.25), (6.0, 5.0, 4.0)),
            Cube::new((20.0, 3.0), (-2.0, 0.375, -6.25), (4.0, 2.0, 2.0)),
            Cube::new((20.0, 0.0), (-4.0, -3.625, -2.75), (2.0, 2.0, 1.0)),
            Cube::new((26.0, 0.0), (2.0, -3.625, -2.75), (2.0, 2.0, 1.0)),
        ],
        Pose::offset(0.0, 18.625, -5.75),
    );
    for (name, uv, x, z) in [
        ("right_hind_leg", (0.0, 34.0), -2.5, 4.5),
        ("left_hind_leg", (12.0, 34.0), 2.5, 4.5),
        ("right_front_leg", (0.0, 28.0), -2.5, -4.5),
        ("left_front_leg", (12.0, 28.0), 2.5, -4.5),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.5, -0.5, -1.5), (3.0, 3.0, 3.0))],
            Pose::offset(x, 21.5, z),
        );
    }
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct PolarBearParts {
    quadruped: QuadrupedParts,
    body: PartId,
}

impl PolarBearParts {
    pub fn find(model: &Model) -> Self {
        Self {
            quadruped: QuadrupedParts::find(model),
            body: model.part("body"),
        }
    }
}

/// `PolarBearModel.setupAnim`: the quadruped walk, then a rear.
pub fn setup_anim(
    parts: &PolarBearParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    stand: f32,
) {
    quadruped::setup_anim(&parts.quadruped, pose, living);
    let stand = stand * stand;
    let scale = if living.baby { 0.5 } else { 1.0 };
    pose[parts.body].pose.rotation.x -= stand * PI * 0.35;
    pose[parts.body].pose.offset.y += stand * scale * 2.0;
    let front_y = pose[parts.quadruped.legs.right_front].pose.offset.y - stand * scale * 20.0;
    let front_z = pose[parts.quadruped.legs.right_front].pose.offset.z + stand * scale * 4.0;
    for front in [
        parts.quadruped.legs.right_front,
        parts.quadruped.legs.left_front,
    ] {
        pose[front].pose.offset.y = front_y;
        pose[front].pose.offset.z = front_z;
        pose[front].pose.rotation.x -= stand * PI * 0.45;
    }
    pose[parts.quadruped.head].pose.offset.y -= stand * 24.0;
    pose[parts.quadruped.head].pose.offset.z += stand * 13.0;
    pose[parts.quadruped.head].pose.rotation.x += stand * PI * 0.15;
}

/// `PolarBearRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::PolarBear, "minecraft:entity/bear/polarbear"),
        (M::PolarBearBaby, "minecraft:entity/bear/polarbear_baby"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_lifts_the_head_and_the_front_legs() {
        let model = Model::bake(&adult());
        let parts = PolarBearParts::find(&model);
        let mut pose = model.initial_pose();
        let rest_head = pose[parts.quadruped.head].pose.offset;
        setup_anim(&parts, &mut pose, &LivingRenderState::default(), 1.0);
        assert!(pose[parts.quadruped.head].pose.offset.y < rest_head.y - 10.0);
    }
}
