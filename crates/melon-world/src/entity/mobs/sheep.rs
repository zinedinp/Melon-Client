//! `SheepModel`, `BabySheepModel` and `SheepFurModel`.

use pumpkin_data::dye_color::DyeColor;

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::magic_name::Magic;
use crate::entity::model::{LayerDefinition, PartDefinition};
use crate::entity::quadruped;
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::PI;

pub fn adult() -> LayerDefinition {
    let mut root = quadruped::create_body_mesh(12.0, false, true, 0.0);
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-3.0, -4.0, -6.0), (6.0, 6.0, 8.0))],
        Pose::offset(0.0, 6.0, -8.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (28.0, 8.0),
            (-4.0, -10.0, -7.0),
            (8.0, 16.0, 6.0),
        )],
        Pose::offset_rotation((0.0, 5.0, 2.0), (PI / 2.0, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 10.0), (-3.0, -2.0, -4.5), (6.0, 4.0, 9.0))],
        Pose::offset(0.0, 17.0, 0.5),
    );
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-2.5, -4.5, -3.5), (5.0, 5.0, 5.0))],
        Pose::offset(0.0, 15.5, -2.5),
    );
    for (name, uv, x, z) in [
        ("right_hind_leg", (0.0, 23.0), -2.0, 3.0),
        ("left_hind_leg", (24.0, 12.0), 2.0, 3.0),
        ("right_front_leg", (8.0, 23.0), -2.0, -2.0),
        ("left_front_leg", (24.0, 5.0), 2.0, -2.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, 0.0, -1.0), (2.0, 5.0, 2.0))],
            Pose::offset(x, 19.0, z),
        );
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `SheepFurModel.createFurLayer`: the wool, grown out around the body.
pub fn fur() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-3.0, -4.0, -4.0), (6.0, 6.0, 6.0)).grown(0.6)],
        Pose::offset(0.0, 6.0, -8.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((28.0, 8.0), (-4.0, -10.0, -7.0), (8.0, 16.0, 6.0)).grown(1.75)],
        Pose::offset_rotation((0.0, 5.0, 2.0), (PI / 2.0, 0.0, 0.0)),
    );
    let leg = || vec![Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 6.0, 4.0)).grown(0.5)];
    root.add_or_replace_child("right_hind_leg", leg(), Pose::offset(-3.0, 12.0, 7.0));
    root.add_or_replace_child("left_hind_leg", leg(), Pose::offset(3.0, 12.0, 7.0));
    root.add_or_replace_child("right_front_leg", leg(), Pose::offset(-3.0, 12.0, -5.0));
    root.add_or_replace_child("left_front_leg", leg(), Pose::offset(3.0, 12.0, -5.0));
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `ColorLerper.Type.SHEEP.getColor`.
pub fn wool_tint(color: DyeColor) -> [f32; 3] {
    let rgb = if color == DyeColor::White {
        0xE6E6E6
    } else {
        let src = color.texture_diffuse_color();
        let channel = |shift: u32| (((src >> shift) & 0xFF) as f32 * 0.75).floor() as u32;
        (channel(16) << 16) | (channel(8) << 8) | channel(0)
    };
    [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0)
}

/// `ColorLerper.getLerpedColor(SHEEP, ageInTicks)` for a sheep named jeb_.
pub fn jeb_tint(age_in_ticks: f32) -> [f32; 3] {
    const DURATION: i32 = 25;
    const DYES: i32 = 16;
    let whole = age_in_ticks.floor() as i32;
    let step = whole / DURATION;
    let dye = |i: i32| DyeColor::by_id(i.rem_euclid(DYES) as u8).unwrap_or(DyeColor::White);
    let sub_step =
        ((whole % DURATION) as f32 + (age_in_ticks - age_in_ticks.floor())) / DURATION as f32;
    let (from, to) = (wool_tint(dye(step)), wool_tint(dye(step + 1)));
    std::array::from_fn(|i| {
        let (a, b) = (
            (from[i] * 255.0).round() as i32,
            (to[i] * 255.0).round() as i32,
        );
        (a + (sub_step * (b - a) as f32).floor() as i32) as f32 / 255.0
    })
}

/// `SheepRenderer` with its wool layers.
pub(super) fn push_layers(entity: &ClientEntity, age_in_ticks: f32, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    let jeb = a.magic == Magic::Jeb;
    let tint = if jeb {
        jeb_tint(age_in_ticks)
    } else {
        wool_tint(a.wool_color())
    };
    if baby {
        out.push(plain(M::SheepBaby, "minecraft:entity/sheep/sheep_baby"));
    } else {
        out.push(plain(M::Sheep, "minecraft:entity/sheep/sheep"));
        if jeb || a.wool_color() != pumpkin_data::dye_color::DyeColor::White {
            out.push(
                plain(M::Sheep, "minecraft:entity/sheep/sheep_wool_undercoat").with_tint(tint),
            );
        }
    }
    if !a.sheared() {
        let (model, texture) = if baby {
            (M::SheepBaby, "minecraft:entity/sheep/sheep_wool_baby")
        } else {
            (M::SheepFur, "minecraft:entity/sheep/sheep_wool")
        };
        out.push(plain(model, texture).with_tint(tint));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jeb_sheep_cycles_through_the_dyes_every_25_ticks() {
        assert_eq!(jeb_tint(0.0), wool_tint(DyeColor::by_id(0).unwrap()));
        assert_eq!(jeb_tint(25.0), wool_tint(DyeColor::by_id(1).unwrap()));
        let middle = jeb_tint(12.5);
        let (a, b) = (
            wool_tint(DyeColor::by_id(0).unwrap()),
            wool_tint(DyeColor::by_id(1).unwrap()),
        );
        for i in 0..3 {
            assert!(middle[i] >= a[i].min(b[i]) - 0.01 && middle[i] <= a[i].max(b[i]) + 0.01);
        }
        assert_eq!(jeb_tint(25.0 * 16.0), jeb_tint(0.0));
    }

    #[test]
    fn wool_is_darkened_but_white_stays_light() {
        assert_eq!(wool_tint(DyeColor::White), [230.0 / 255.0; 3]);
        let [r, g, b] = wool_tint(DyeColor::Red);
        assert!(r > g && r > b && r < 0.75);
    }
}
