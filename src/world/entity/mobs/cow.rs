//! `CowModel` and its cold, warm and baby forms.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M, variant_body};
use crate::math::angle::PI;
use crate::world::entity::model::{LayerDefinition, PartDefinition};
use crate::world::model_part::{Cube, Pose};
use pumpkin_data::cow_variant::CowVariant;

const BODY_POSE: ((f32, f32, f32), (f32, f32, f32)) = ((0.0, 5.0, 2.0), (PI / 2.0, 0.0, 0.0));

/// `CowModel.createBaseCowModel`.
fn base() -> PartDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -4.0, -6.0), (8.0, 8.0, 6.0)),
            Cube::new((1.0, 33.0), (-3.0, 1.0, -7.0), (6.0, 3.0, 1.0)),
            Cube::new((22.0, 0.0), (-5.0, -5.0, -5.0), (1.0, 3.0, 1.0)),
            Cube::new((22.0, 0.0), (4.0, -5.0, -5.0), (1.0, 3.0, 1.0)),
        ],
        Pose::offset(0.0, 4.0, -8.0),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((18.0, 4.0), (-6.0, -10.0, -7.0), (12.0, 18.0, 10.0)),
            Cube::new((52.0, 0.0), (-2.0, 2.0, -8.0), (4.0, 6.0, 1.0)),
        ],
        Pose::offset_rotation(BODY_POSE.0, BODY_POSE.1),
    );
    let leg = |mirror: bool| {
        let cube = Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0));
        vec![if mirror { cube.mirrored() } else { cube }]
    };
    root.add_or_replace_child("right_hind_leg", leg(false), Pose::offset(-4.0, 12.0, 7.0));
    root.add_or_replace_child("left_hind_leg", leg(true), Pose::offset(4.0, 12.0, 7.0));
    root.add_or_replace_child(
        "right_front_leg",
        leg(false),
        Pose::offset(-4.0, 12.0, -5.0),
    );
    root.add_or_replace_child("left_front_leg", leg(true), Pose::offset(4.0, 12.0, -5.0));
    root
}

pub fn adult() -> LayerDefinition {
    LayerDefinition::new(base(), 64.0, 64.0)
}

/// `ColdCowModel`: a shaggy coat and long horns, turned by vanilla's own
#[allow(clippy::approx_constant)]
pub fn cold() -> LayerDefinition {
    let mut root = base();
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((20.0, 32.0), (-6.0, -10.0, -7.0), (12.0, 18.0, 10.0)).grown(0.5),
            Cube::new((18.0, 4.0), (-6.0, -10.0, -7.0), (12.0, 18.0, 10.0)),
            Cube::new((52.0, 0.0), (-2.0, 2.0, -8.0), (4.0, 6.0, 1.0)),
        ],
        Pose::offset_rotation(BODY_POSE.0, BODY_POSE.1),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -4.0, -6.0), (8.0, 8.0, 6.0)),
            Cube::new((9.0, 33.0), (-3.0, 1.0, -7.0), (6.0, 3.0, 1.0)),
        ],
        Pose::offset(0.0, 4.0, -8.0),
    );
    head.add_or_replace_child(
        "right_horn",
        vec![Cube::new((0.0, 40.0), (-1.5, -4.5, -0.5), (2.0, 6.0, 2.0))],
        Pose::offset_rotation((-4.5, -2.5, -3.5), (1.5708, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "left_horn",
        vec![Cube::new((0.0, 32.0), (-1.5, -3.0, -0.5), (2.0, 6.0, 2.0))],
        Pose::offset_rotation((5.5, -2.5, -5.0), (1.5708, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `WarmCowModel`: wide horns.
pub fn warm() -> LayerDefinition {
    let mut root = base();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -4.0, -6.0), (8.0, 8.0, 6.0)),
            Cube::new((1.0, 33.0), (-3.0, 1.0, -7.0), (6.0, 3.0, 1.0)),
            Cube::new((27.0, 0.0), (-8.0, -3.0, -5.0), (4.0, 2.0, 2.0)),
            Cube::new((39.0, 0.0), (-8.0, -5.0, -5.0), (2.0, 2.0, 2.0)),
            Cube::new((27.0, 0.0), (4.0, -3.0, -5.0), (4.0, 2.0, 2.0)).mirrored(),
            Cube::new((39.0, 0.0), (6.0, -5.0, -5.0), (2.0, 2.0, 2.0)).mirrored(),
        ],
        Pose::offset(0.0, 4.0, -8.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyCowModel`, every variant's calf.
pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 18.0), (-3.0, -4.569, -4.8333), (6.0, 6.0, 5.0)),
            Cube::new((8.0, 29.0), (3.0, -5.569, -3.8333), (1.0, 2.0, 1.0)),
            Cube::new((4.0, 29.0), (-4.0, -5.569, -3.8333), (1.0, 2.0, 1.0)).mirrored(),
            Cube::new((12.0, 29.0), (-2.0, -1.569, -5.8333), (4.0, 3.0, 1.0)),
        ],
        Pose::offset(0.0, 13.569, -5.1667),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-7.0, -7.0, -1.0), (8.0, 6.0, 12.0))],
        Pose::offset(3.0, 19.0, -5.0),
    );
    for (name, uv, x, z) in [
        ("right_front_leg", (22.0, 18.0), -2.5, -3.5),
        ("left_front_leg", (34.0, 18.0), 2.5, -3.5),
        ("right_hind_leg", (22.0, 27.0), -2.5, 3.5),
        ("left_hind_leg", (34.0, 27.0), 2.5, 3.5),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.5, 0.0, -1.5), (3.0, 6.0, 3.0))],
            Pose::offset(x, 18.0, z),
        );
    }
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `MushroomCowMushroomLayer`'s block model.
pub fn mushroom_model(entity: &ClientEntity) -> Option<&'static str> {
    if entity.kind != EntityKind::Mooshroom || entity.appearance.baby {
        return None;
    }
    Some(if entity.appearance.brown_mushrooms {
        "minecraft:block/brown_mushroom"
    } else {
        "minecraft:block/red_mushroom"
    })
}

/// `CowRenderer` and `MushroomCowRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Mooshroom => {
            let brown = a.brown_mushrooms;
            out.push(match (baby, brown) {
                (false, false) => plain(M::Cow, "minecraft:entity/cow/mooshroom_red"),
                (false, true) => plain(M::Cow, "minecraft:entity/cow/mooshroom_brown"),
                (true, false) => plain(M::CowBaby, "minecraft:entity/cow/mooshroom_red_baby"),
                (true, true) => plain(M::CowBaby, "minecraft:entity/cow/mooshroom_brown_baby"),
            });
        }
        EntityKind::Cow => {
            debug_assert_eq!(CowVariant::Cold as i32, 0);
            out.push(variant_body(
                baby,
                a.variant,
                M::CowBaby,
                [
                    (
                        M::CowCold,
                        "minecraft:entity/cow/cow_cold",
                        "minecraft:entity/cow/cow_cold_baby",
                    ),
                    (
                        M::Cow,
                        "minecraft:entity/cow/cow_temperate",
                        "minecraft:entity/cow/cow_temperate_baby",
                    ),
                    (
                        M::CowWarm,
                        "minecraft:entity/cow/cow_warm",
                        "minecraft:entity/cow/cow_warm_baby",
                    ),
                ],
            ));
        }
        _ => {}
    }
}
