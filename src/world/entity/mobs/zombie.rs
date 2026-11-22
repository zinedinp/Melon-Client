//! `ZombieModel` (the humanoid, arms held out) and `BabyZombieModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::world::entity::humanoid;
use crate::world::entity::model::{LayerDefinition, PartDefinition};
use crate::world::model_part::{Cube, Pose};

pub fn adult() -> LayerDefinition {
    LayerDefinition::new(humanoid::create_mesh(0.0, 0.0), 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    baby_grown(0.0)
}

/// `DrownedModel.createBodyLayer(g)`: the zombie with a left arm and leg
pub fn drowned(g: f32) -> LayerDefinition {
    let mut root = humanoid::create_mesh(g, 0.0);
    root.add_or_replace_child(
        "left_arm",
        vec![Cube::new((32.0, 48.0), (-1.0, -2.0, -2.0), (4.0, 12.0, 4.0)).grown(g)],
        Pose::offset(5.0, 2.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((16.0, 48.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(g)],
        Pose::offset(1.9, 12.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyZombieModel.createBodyLayer(g)`.
pub fn baby_grown(g: f32) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((16.0, 16.0), (-2.0, -2.5, -1.0), (4.0, 5.0, 2.0)).grown(g)],
        Pose::offset(0.0, 17.5, 0.0),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((3.0, 3.0), (-3.0, -6.25, -3.0), (6.0, 6.0, 6.0)),
            Cube::new((35.0, 3.0), (-3.0, -6.15, -3.0), (6.0, 6.0, 6.0)).grown(0.25),
        ],
        Pose::offset(0.0, 15.25, 0.0),
    );
    head.add_or_replace_child("hat", Vec::new(), Pose::ZERO);
    for (name, uv, x) in [
        ("right_arm", (36.0, 16.0), -3.0),
        ("left_arm", (28.0, 16.0), 3.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, -0.5, -1.0), (2.0, 5.0, 2.0)).grown(g)],
            Pose::offset(x, 15.5, 0.0),
        );
    }
    for (name, uv, x) in [
        ("right_leg", (8.0, 16.0), -1.0),
        ("left_leg", (0.0, 16.0), 1.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, 0.0, -1.0), (2.0, 4.0, 2.0)).grown(g)],
            Pose::offset(x, 20.0, 0.0),
        );
    }
    LayerDefinition::new(root, 64.0, 64.0)
}

/// The zombie renderers.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Zombie => out.push(if baby {
            plain(M::ZombieBaby, "minecraft:entity/zombie/zombie_baby")
        } else {
            plain(M::Zombie, "minecraft:entity/zombie/zombie")
        }),
        EntityKind::Drowned => {
            if baby {
                out.push(plain(M::ZombieBaby, "minecraft:entity/zombie/drowned_baby"));
                out.push(plain(
                    M::DrownedBabyOuter,
                    "minecraft:entity/zombie/drowned_outer_layer_baby",
                ));
            } else {
                out.push(plain(M::Drowned, "minecraft:entity/zombie/drowned"));
                out.push(plain(
                    M::DrownedOuter,
                    "minecraft:entity/zombie/drowned_outer_layer",
                ));
            }
        }
        EntityKind::Husk => out.push(if baby {
            plain(M::ZombieBaby, "minecraft:entity/zombie/husk_baby")
        } else {
            plain(M::Zombie, "minecraft:entity/zombie/husk")
        }),
        EntityKind::Giant => out.push(plain(M::Zombie, "minecraft:entity/zombie/zombie")),
        _ => {}
    }
}
