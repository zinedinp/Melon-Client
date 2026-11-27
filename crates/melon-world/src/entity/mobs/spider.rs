//! `SpiderModel`: a head, two body segments and eight legs that scuttle.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::look;
use crate::world::model_part::{Cube, Pose};

/// The legs from the back pair forward.
const LEGS: [(&str, &str, f32, f32, f32); 4] = [
    ("right_hind_leg", "left_hind_leg", 2.0, PI / 4.0, -PI / 4.0),
    (
        "right_middle_hind_leg",
        "left_middle_hind_leg",
        1.0,
        PI / 8.0,
        -0.581_194_64,
    ),
    (
        "right_middle_front_leg",
        "left_middle_front_leg",
        0.0,
        -PI / 8.0,
        -0.581_194_64,
    ),
    (
        "right_front_leg",
        "left_front_leg",
        -1.0,
        -PI / 4.0,
        -PI / 4.0,
    ),
];

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((32.0, 4.0), (-4.0, -4.0, -8.0), (8.0, 8.0, 8.0))],
        Pose::offset(0.0, 15.0, -3.0),
    );
    root.add_or_replace_child(
        "body0",
        vec![Cube::new((0.0, 0.0), (-3.0, -3.0, -3.0), (6.0, 6.0, 6.0))],
        Pose::offset(0.0, 15.0, 0.0),
    );
    root.add_or_replace_child(
        "body1",
        vec![Cube::new(
            (0.0, 12.0),
            (-5.0, -4.0, -6.0),
            (10.0, 8.0, 12.0),
        )],
        Pose::offset(0.0, 15.0, 9.0),
    );
    let right = || {
        vec![Cube::new(
            (18.0, 0.0),
            (-15.0, -1.0, -1.0),
            (16.0, 2.0, 2.0),
        )]
    };
    let left = || vec![Cube::new((18.0, 0.0), (-1.0, -1.0, -1.0), (16.0, 2.0, 2.0)).mirrored()];
    for (right_name, left_name, z, yaw, roll) in LEGS {
        root.add_or_replace_child(
            right_name,
            right(),
            Pose::offset_rotation((-4.0, 15.0, z), (0.0, yaw, roll)),
        );
        root.add_or_replace_child(
            left_name,
            left(),
            Pose::offset_rotation((4.0, 15.0, z), (0.0, -yaw, -roll)),
        );
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct SpiderParts {
    head: PartId,
    /// Right and left leg, back pair first.
    legs: [(PartId, PartId); 4],
}

impl SpiderParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            legs: LEGS.map(|(r, l, ..)| (model.part(r), model.part(l))),
        }
    }
}

/// `SpiderModel.setupAnim`.
pub fn setup_anim(parts: &SpiderParts, pose: &mut ModelPose, state: &LivingRenderState) {
    look(&mut pose[parts.head].pose, state);
    let position = state.walk_position * 0.6662;
    let speed = state.walk_speed;
    let phases = [0.0, PI, PI / 2.0, PI * 3.0 / 2.0];
    for ((right, left), phase) in parts.legs.into_iter().zip(phases) {
        let swing = -(angle::cos(position * 2.0 + phase) * 0.4) * speed;
        let step = (angle::sin(position + phase) * 0.4).abs() * speed;
        let r = &mut pose[right].pose.rotation;
        r.y += swing;
        r.z += step;
        let l = &mut pose[left].pose.rotation;
        l.y -= swing;
        l.z -= step;
    }
}

/// `SpiderRenderer`, `CaveSpiderRenderer` and `SpiderEyesLayer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Spider | EntityKind::CaveSpider => {
            out.push(plain(
                M::Spider,
                if entity.kind == EntityKind::Spider {
                    "minecraft:entity/spider/spider"
                } else {
                    "minecraft:entity/spider/cave_spider"
                },
            ));
            out.push(plain(M::Spider, "minecraft:entity/spider/spider_eyes").glowing());
        }
        _ => {}
    }
}
