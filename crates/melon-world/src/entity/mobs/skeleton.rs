//! `SkeletonModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::animation::bob_arms;
use crate::entity::humanoid::{self, HumanoidParts};
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use melon_core::math::angle::{self, PI};
use crate::model_part::{Cube, Pose};

pub fn layer() -> LayerDefinition {
    let mut root = humanoid::create_mesh(0.0, 0.0);
    for (name, x, mirror) in [("right_arm", -5.0, false), ("left_arm", 5.0, true)] {
        let cube = Cube::new((40.0, 16.0), (-1.0, -2.0, -1.0), (2.0, 12.0, 2.0));
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 2.0, 0.0),
        );
    }
    for (name, x, mirror) in [("right_leg", -2.0, false), ("left_leg", 2.0, true)] {
        let cube = Cube::new((0.0, 16.0), (-1.0, 0.0, -1.0), (2.0, 12.0, 2.0));
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 12.0, 0.0),
        );
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `SkeletonModel.createSingleModelDualBodyLayer` (the parched): every
/// bone wrapped once more, on a 64x64 sheet.
pub fn parched() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((16.0, 16.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0)),
            Cube::new((28.0, 0.0), (-4.0, 10.0, -2.0), (8.0, 1.0, 4.0)),
            Cube::new((16.0, 48.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0)).grown(0.025),
        ],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)),
            Cube::new((0.0, 32.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)).grown(0.2),
        ],
        Pose::ZERO,
    )
    .add_or_replace_child("hat", Vec::new(), Pose::ZERO);
    root.add_or_replace_child(
        "right_arm",
        vec![
            Cube::new((40.0, 16.0), (-1.0, -2.0, -1.0), (2.0, 12.0, 2.0)),
            Cube::new((42.0, 33.0), (-1.55, -2.025, -1.5), (3.0, 12.0, 3.0)),
        ],
        Pose::offset(-5.5, 2.0, 0.0),
    );
    root.add_or_replace_child(
        "left_arm",
        vec![
            Cube::new((56.0, 16.0), (-1.0, -2.0, -1.0), (2.0, 12.0, 2.0)),
            Cube::new((40.0, 48.0), (-1.45, -2.025, -1.5), (3.0, 12.0, 3.0)),
        ],
        Pose::offset(5.5, 2.0, 0.0),
    );
    root.add_or_replace_child(
        "right_leg",
        vec![
            Cube::new((0.0, 16.0), (-1.0, 0.0, -1.0), (2.0, 12.0, 2.0)),
            Cube::new((0.0, 49.0), (-1.5, 0.0, -1.5), (3.0, 12.0, 3.0)),
        ],
        Pose::offset(-2.0, 12.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![
            Cube::new((0.0, 16.0), (-1.0, 0.0, -1.0), (2.0, 12.0, 2.0)),
            Cube::new((4.0, 49.0), (-1.5, 0.0, -1.5), (3.0, 12.0, 3.0)),
        ],
        Pose::offset(2.0, 12.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `STRAY_OUTER_LAYER`.
pub fn clothes() -> LayerDefinition {
    LayerDefinition::new(humanoid::create_mesh(0.25, 0.0), 64.0, 32.0)
}

/// `BoggedModel.createBodyLayer`: the skeleton with mushrooms on the head.
pub fn bogged() -> LayerDefinition {
    let mut root = layer().root;
    let mushrooms =
        root.child_mut("head")
            .add_or_replace_child("mushrooms", Vec::new(), Pose::ZERO);
    let quarter = PI / 4.0;
    let three_quarters = PI * 3.0 / 4.0;
    mushrooms.add_or_replace_child(
        "red_mushroom_1",
        vec![Cube::new((50.0, 16.0), (-3.0, -3.0, 0.0), (6.0, 4.0, 0.0))],
        Pose::offset_rotation((3.0, -8.0, 3.0), (0.0, quarter, 0.0)),
    );
    mushrooms.add_or_replace_child(
        "red_mushroom_2",
        vec![Cube::new((50.0, 16.0), (-3.0, -3.0, 0.0), (6.0, 4.0, 0.0))],
        Pose::offset_rotation((3.0, -8.0, 3.0), (0.0, three_quarters, 0.0)),
    );
    mushrooms.add_or_replace_child(
        "brown_mushroom_1",
        vec![Cube::new((50.0, 22.0), (-3.0, -3.0, 0.0), (6.0, 4.0, 0.0))],
        Pose::offset_rotation((-3.0, -8.0, -3.0), (0.0, quarter, 0.0)),
    );
    mushrooms.add_or_replace_child(
        "brown_mushroom_2",
        vec![Cube::new((50.0, 22.0), (-3.0, -3.0, 0.0), (6.0, 4.0, 0.0))],
        Pose::offset_rotation((-3.0, -8.0, -3.0), (0.0, three_quarters, 0.0)),
    );
    mushrooms.add_or_replace_child(
        "brown_mushroom_3",
        vec![Cube::new((50.0, 28.0), (-3.0, -4.0, 0.0), (6.0, 4.0, 0.0))],
        Pose::offset_rotation((-2.0, -1.0, 4.0), (-PI / 2.0, 0.0, quarter)),
    );
    mushrooms.add_or_replace_child(
        "brown_mushroom_4",
        vec![Cube::new((50.0, 28.0), (-3.0, -4.0, 0.0), (6.0, 4.0, 0.0))],
        Pose::offset_rotation((-2.0, -1.0, 4.0), (-PI / 2.0, 0.0, three_quarters)),
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// The bogged's clothes, same grow as the stray's.
pub fn bogged_clothes() -> LayerDefinition {
    clothes()
}

/// The mushroom cluster, hidden when sheared.
#[derive(Clone, Copy, Debug)]
pub struct BoggedParts {
    pub humanoid: HumanoidParts,
    mushrooms: PartId,
}

impl BoggedParts {
    pub fn find(model: &Model) -> Self {
        Self {
            humanoid: HumanoidParts::find(model),
            mushrooms: model.part("mushrooms"),
        }
    }

    pub fn hide_mushrooms(&self, pose: &mut ModelPose, sheared: bool) {
        pose[self.mushrooms].visible = !sheared;
    }
}

/// `SkeletonModel.setupAnim`'s melee.
pub fn melee(parts: &HumanoidParts, pose: &mut ModelPose, swing: f32, age_in_ticks: f32) {
    let swing2 = angle::sin(swing * PI);
    let swing1 = angle::sin((1.0 - (1.0 - swing) * (1.0 - swing)) * PI);
    let x = -PI / 2.0 - (swing2 * 1.2 - swing1 * 0.4);
    let y = 0.1 - swing2 * 0.6;
    pose[parts.right_arm].pose.rotation = glam::Vec3::new(x, -y, 0.0);
    pose[parts.left_arm].pose.rotation = glam::Vec3::new(x, y, 0.0);
    let [right, left] = pose.get_two_mut(parts.right_arm, parts.left_arm);
    bob_arms(&mut right.pose, &mut left.pose, age_in_ticks);
}

/// The skeleton renderers.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Bogged => {
            out.push(plain(M::Bogged, "minecraft:entity/skeleton/bogged"));
            out.push(plain(
                M::BoggedClothes,
                "minecraft:entity/skeleton/bogged_overlay",
            ));
        }
        EntityKind::Skeleton => out.push(plain(M::Skeleton, "minecraft:entity/skeleton/skeleton")),
        EntityKind::WitherSkeleton => out.push(plain(
            M::Skeleton,
            "minecraft:entity/skeleton/wither_skeleton",
        )),
        EntityKind::Stray => {
            out.push(plain(M::Skeleton, "minecraft:entity/skeleton/stray"));
            out.push(plain(
                M::StrayClothes,
                "minecraft:entity/skeleton/stray_overlay",
            ));
        }
        EntityKind::Parched => out.push(plain(M::Parched, "minecraft:entity/skeleton/parched")),
        _ => {}
    }
}
