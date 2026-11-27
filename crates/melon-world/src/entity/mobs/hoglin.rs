//! `HoglinModel` and `BabyHoglinModel`: also used for a zoglin.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// Rest pitch of an adult hoglin's head (`DEFAULT_HEAD_X_ROT`).
const DEFAULT_HEAD_X_ROT: f32 = 0.87266463;
/// `ATTACK_HEAD_X_ROT_END`.
const ATTACK_HEAD_X_ROT_END: f32 = -PI / 9.0;

/// `HoglinRenderer` / `ZoglinRenderer.getTextureLocation`.
pub fn texture(zoglin: bool, baby: bool) -> &'static str {
    match (zoglin, baby) {
        (false, false) => "minecraft:entity/hoglin/hoglin",
        (false, true) => "minecraft:entity/hoglin/hoglin_baby",
        (true, false) => "minecraft:entity/hoglin/zoglin",
        (true, true) => "minecraft:entity/hoglin/zoglin_baby",
    }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (1.0, 1.0),
            (-8.0, -7.0, -13.0),
            (16.0, 14.0, 26.0),
        )],
        Pose::offset(0.0, 7.0, 0.0),
    );
    body.add_or_replace_child(
        "mane",
        vec![Cube::new((90.0, 33.0), (0.0, 0.0, -9.0), (0.0, 10.0, 19.0)).grown(0.001)],
        Pose::offset(0.0, -14.0, -7.0),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new(
            (61.0, 1.0),
            (-7.0, -3.0, -19.0),
            (14.0, 6.0, 19.0),
        )],
        Pose::offset_rotation((0.0, 2.0, -12.0), (DEFAULT_HEAD_X_ROT, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((1.0, 1.0), (-6.0, -1.0, -2.0), (6.0, 1.0, 4.0))],
        Pose::offset_rotation((-6.0, -2.0, -3.0), (0.0, 0.0, -PI * 2.0 / 9.0)),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((1.0, 6.0), (0.0, -1.0, -2.0), (6.0, 1.0, 4.0))],
        Pose::offset_rotation((6.0, -2.0, -3.0), (0.0, 0.0, PI * 2.0 / 9.0)),
    );
    head.add_or_replace_child(
        "right_horn",
        vec![Cube::new(
            (10.0, 13.0),
            (-1.0, -11.0, -1.0),
            (2.0, 11.0, 2.0),
        )],
        Pose::offset(-7.0, 2.0, -12.0),
    );
    head.add_or_replace_child(
        "left_horn",
        vec![Cube::new(
            (1.0, 13.0),
            (-1.0, -11.0, -1.0),
            (2.0, 11.0, 2.0),
        )],
        Pose::offset(7.0, 2.0, -12.0),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((66.0, 42.0), (-3.0, 0.0, -3.0), (6.0, 14.0, 6.0))],
        Pose::offset(-4.0, 10.0, -8.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((41.0, 42.0), (-3.0, 0.0, -3.0), (6.0, 14.0, 6.0))],
        Pose::offset(4.0, 10.0, -8.5),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((21.0, 45.0), (-2.5, 0.0, -2.5), (5.0, 11.0, 5.0))],
        Pose::offset(-5.0, 13.0, 10.0),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((0.0, 45.0), (-2.5, 0.0, -2.5), (5.0, 11.0, 5.0))],
        Pose::offset(5.0, 13.0, 10.0),
    );
    LayerDefinition::new(root, 128.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-5.0, -2.2605, -10.547), (10.0, 4.0, 12.0)),
            Cube::new((44.0, 29.0), (-7.0, -4.0981, -8.4879), (2.0, 5.0, 2.0)),
            Cube::new((52.0, 29.0), (5.0, -4.0981, -8.4879), (2.0, 5.0, 2.0)),
        ],
        Pose::offset_rotation((0.0, 13.0, -7.0), (0.8727, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((32.0, 5.0), (-5.1, -0.5, -2.0), (6.0, 1.0, 4.0))],
        Pose::offset_rotation((-5.0, -1.0, -1.5), (0.0, 0.0, -0.8727)),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((32.0, 0.0), (-0.9, -0.5, -2.0), (6.0, 1.0, 4.0)).mirrored()],
        Pose::offset_rotation((5.0, -1.0, -1.5), (0.0, 0.0, 0.8727)),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 16.0), (-4.0, -14.0, -7.0), (8.0, 8.0, 14.0)).grown(0.02),
            Cube::new((24.0, 39.0), (0.0, -18.0, -8.0), (0.0, 6.0, 11.0)).grown(0.02),
        ],
        Pose::offset(0.0, 24.0, 0.0),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((0.0, 47.0), (-1.5, 0.0, -1.5), (3.0, 6.0, 3.0))],
        Pose::offset(-2.5, 18.0, 4.5),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((12.0, 47.0), (-1.5, 0.0, -1.5), (3.0, 6.0, 3.0))],
        Pose::offset(2.5, 18.0, 4.5),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((0.0, 38.0), (-1.5, 0.0, -1.5), (3.0, 6.0, 3.0))],
        Pose::offset(-2.5, 18.0, -4.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((12.0, 38.0), (-1.5, 0.0, -1.5), (3.0, 6.0, 3.0))],
        Pose::offset(2.5, 18.0, -4.5),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct HoglinParts {
    head: PartId,
    right_ear: PartId,
    left_ear: PartId,
    right_front: PartId,
    left_front: PartId,
    right_hind: PartId,
    left_hind: PartId,
    baby: bool,
}

impl HoglinParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            right_ear: model.part("right_ear"),
            left_ear: model.part("left_ear"),
            right_front: model.part("right_front_leg"),
            left_front: model.part("left_front_leg"),
            right_hind: model.part("right_hind_leg"),
            left_hind: model.part("left_hind_leg"),
            baby: model.find("right_horn").is_none(),
        }
    }
}

/// `HoglinRenderState.attackAnimationRemainingTicks`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HoglinPose {
    pub attack_animation_remaining_ticks: i32,
}

/// `HoglinModel.setupAnim`.
pub fn setup_anim(
    parts: &HoglinParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    hoglin: HoglinPose,
) {
    let speed = living.walk_speed;
    let pos = living.walk_position;
    pose[parts.right_ear].pose.rotation.z = -PI * 2.0 / 9.0 - speed * angle::sin(pos);
    pose[parts.left_ear].pose.rotation.z = PI * 2.0 / 9.0 + speed * angle::sin(pos);
    pose[parts.head].pose.rotation.y = living.y_rot.to_radians();
    let remaining = hoglin.attack_animation_remaining_ticks;
    let headbutt = 1.0 - (10 - 2 * remaining).unsigned_abs() as f32 / 10.0;
    animate_headbutt(parts, pose, headbutt);
    let right_front = angle::cos(pos) * 1.2 * speed;
    let left_front = angle::cos(pos + PI) * 1.2 * speed;
    pose[parts.right_front].pose.rotation.x = right_front;
    pose[parts.left_front].pose.rotation.x = left_front;
    pose[parts.right_hind].pose.rotation.x = left_front;
    pose[parts.left_hind].pose.rotation.x = right_front;
}

/// `HoglinModel.animateHeadbutt` (a cub also lifts the head).
fn animate_headbutt(parts: &HoglinParts, pose: &mut ModelPose, headbutt: f32) {
    pose[parts.head].pose.rotation.x =
        DEFAULT_HEAD_X_ROT + (ATTACK_HEAD_X_ROT_END - DEFAULT_HEAD_X_ROT) * headbutt;
    if parts.baby {
        pose[parts.head].pose.offset.y += headbutt * 2.5;
    }
}

/// `HoglinRenderer` and `ZoglinRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    match entity.kind {
        EntityKind::Hoglin => out.push(LayerSpec::aged(
            baby,
            (M::Hoglin, texture(false, false)),
            (M::HoglinBaby, texture(false, true)),
        )),
        EntityKind::Zoglin => out.push(LayerSpec::aged(
            baby,
            (M::Hoglin, texture(true, false)),
            (M::HoglinBaby, texture(true, true)),
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_and_a_headbutt_tips_the_head() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = HoglinParts::find(&model);
            let mut rest = model.initial_pose();
            let mut attack = model.initial_pose();
            let living = LivingRenderState::default();
            setup_anim(&parts, &mut rest, &living, HoglinPose::default());
            setup_anim(
                &parts,
                &mut attack,
                &living,
                HoglinPose {
                    attack_animation_remaining_ticks: 5,
                },
            );
            assert!(attack[parts.head].pose.rotation.x < rest[parts.head].pose.rotation.x);
        }
        assert_eq!(texture(true, true), "minecraft:entity/hoglin/zoglin_baby");
    }
}
