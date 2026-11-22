//! The fish: cod, salmon (three sizes), pufferfish (three states) and

use glam::Vec3;

use crate::math::angle;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M, dye_tint};
use crate::math::angle::QUARTER_PI as Q;

/// A part with one box.
fn part(
    root: &mut PartDefinition,
    name: &'static str,
    uv: (f32, f32),
    origin: (f32, f32, f32),
    size: (f32, f32, f32),
    pose: Pose,
) {
    root.add_or_replace_child(name, vec![Cube::new(uv, origin, size)], pose);
}

fn at(x: f32, y: f32, z: f32) -> Pose {
    Pose::offset(x, y, z)
}

fn turned(offset: (f32, f32, f32), rotation: (f32, f32, f32)) -> Pose {
    Pose::offset_rotation(offset, rotation)
}

pub fn cod() -> LayerDefinition {
    let mut r = PartDefinition::root();
    part(
        &mut r,
        "body",
        (0.0, 0.0),
        (-1.0, -2.0, 0.0),
        (2.0, 4.0, 7.0),
        at(0.0, 22.0, 0.0),
    );
    part(
        &mut r,
        "head",
        (11.0, 0.0),
        (-1.0, -2.0, -3.0),
        (2.0, 4.0, 3.0),
        at(0.0, 22.0, 0.0),
    );
    part(
        &mut r,
        "nose",
        (0.0, 0.0),
        (-1.0, -2.0, -1.0),
        (2.0, 3.0, 1.0),
        at(0.0, 22.0, -3.0),
    );
    part(
        &mut r,
        "right_fin",
        (22.0, 1.0),
        (-2.0, 0.0, -1.0),
        (2.0, 0.0, 2.0),
        turned((-1.0, 23.0, 0.0), (0.0, 0.0, -Q)),
    );
    part(
        &mut r,
        "left_fin",
        (22.0, 4.0),
        (0.0, 0.0, -1.0),
        (2.0, 0.0, 2.0),
        turned((1.0, 23.0, 0.0), (0.0, 0.0, Q)),
    );
    part(
        &mut r,
        "tail_fin",
        (22.0, 3.0),
        (0.0, -2.0, 0.0),
        (0.0, 4.0, 4.0),
        at(0.0, 22.0, 7.0),
    );
    part(
        &mut r,
        "top_fin",
        (20.0, -6.0),
        (0.0, -1.0, -1.0),
        (0.0, 1.0, 6.0),
        at(0.0, 20.0, 0.0),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

/// `SalmonModel.createBodyLayer`, scaled for the `size` (0.5, 1, 1.5).
pub fn salmon(size: f32) -> LayerDefinition {
    let mut r = PartDefinition::root();
    let front = r.add_or_replace_child(
        "body_front",
        vec![Cube::new((0.0, 0.0), (-1.5, -2.5, 0.0), (3.0, 5.0, 8.0))],
        at(0.0, 20.0, -7.2),
    );
    front.add_or_replace_child(
        "top_front_fin",
        vec![Cube::new((2.0, 1.0), (0.0, 0.0, 0.0), (0.0, 2.0, 3.0))],
        at(0.0, -4.5, 5.0),
    );
    let back = r.add_or_replace_child(
        "body_back",
        vec![Cube::new((0.0, 13.0), (-1.5, -2.5, 0.0), (3.0, 5.0, 8.0))],
        at(0.0, 20.0, 0.800_000_2),
    );
    back.add_or_replace_child(
        "back_fin",
        vec![Cube::new((20.0, 10.0), (0.0, -2.5, 0.0), (0.0, 5.0, 6.0))],
        at(0.0, 0.0, 8.0),
    );
    back.add_or_replace_child(
        "top_back_fin",
        vec![Cube::new((0.0, 2.0), (0.0, 0.0, 0.0), (0.0, 2.0, 4.0))],
        at(0.0, -4.5, -1.0),
    );
    part(
        &mut r,
        "head",
        (22.0, 0.0),
        (-1.0, -2.0, -3.0),
        (2.0, 4.0, 3.0),
        at(0.0, 20.0, -7.2),
    );
    part(
        &mut r,
        "right_fin",
        (-4.0, 0.0),
        (-2.0, 0.0, 0.0),
        (2.0, 0.0, 2.0),
        turned((-1.5, 21.5, -7.2), (0.0, 0.0, -Q)),
    );
    part(
        &mut r,
        "left_fin",
        (0.0, 0.0),
        (0.0, 0.0, 0.0),
        (2.0, 0.0, 2.0),
        turned((1.5, 21.5, -7.2), (0.0, 0.0, Q)),
    );
    if size != 1.0 {
        r.pose = super::ghast::scaling(Pose::ZERO, size);
    }
    LayerDefinition::new(r, 32.0, 32.0)
}

pub fn pufferfish_big() -> LayerDefinition {
    let mut r = PartDefinition::root();
    part(
        &mut r,
        "body",
        (0.0, 0.0),
        (-4.0, -8.0, -4.0),
        (8.0, 8.0, 8.0),
        at(0.0, 22.0, 0.0),
    );
    part(
        &mut r,
        "right_blue_fin",
        (24.0, 0.0),
        (-2.0, 0.0, -1.0),
        (2.0, 1.0, 2.0),
        at(-4.0, 15.0, -2.0),
    );
    part(
        &mut r,
        "left_blue_fin",
        (24.0, 3.0),
        (0.0, 0.0, -1.0),
        (2.0, 1.0, 2.0),
        at(4.0, 15.0, -2.0),
    );
    part(
        &mut r,
        "top_front_fin",
        (15.0, 17.0),
        (-4.0, -1.0, 0.0),
        (8.0, 1.0, 0.0),
        turned((0.0, 14.0, -4.0), (Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "top_middle_fin",
        (14.0, 16.0),
        (-4.0, -1.0, 0.0),
        (8.0, 1.0, 1.0),
        at(0.0, 14.0, 0.0),
    );
    part(
        &mut r,
        "top_back_fin",
        (23.0, 18.0),
        (-4.0, -1.0, 0.0),
        (8.0, 1.0, 0.0),
        turned((0.0, 14.0, 4.0), (-Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "right_front_fin",
        (5.0, 17.0),
        (-1.0, -8.0, 0.0),
        (1.0, 8.0, 0.0),
        turned((-4.0, 22.0, -4.0), (0.0, -Q, 0.0)),
    );
    part(
        &mut r,
        "left_front_fin",
        (1.0, 17.0),
        (0.0, -8.0, 0.0),
        (1.0, 8.0, 0.0),
        turned((4.0, 22.0, -4.0), (0.0, Q, 0.0)),
    );
    part(
        &mut r,
        "bottom_front_fin",
        (15.0, 20.0),
        (-4.0, 0.0, 0.0),
        (8.0, 1.0, 0.0),
        turned((0.0, 22.0, -4.0), (-Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "bottom_middle_fin",
        (15.0, 20.0),
        (-4.0, 0.0, 0.0),
        (8.0, 1.0, 0.0),
        at(0.0, 22.0, 0.0),
    );
    part(
        &mut r,
        "bottom_back_fin",
        (15.0, 20.0),
        (-4.0, 0.0, 0.0),
        (8.0, 1.0, 0.0),
        turned((0.0, 22.0, 4.0), (Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "right_back_fin",
        (9.0, 17.0),
        (-1.0, -8.0, 0.0),
        (1.0, 8.0, 0.0),
        turned((-4.0, 22.0, 4.0), (0.0, Q, 0.0)),
    );
    part(
        &mut r,
        "left_back_fin",
        (9.0, 17.0),
        (0.0, -8.0, 0.0),
        (1.0, 8.0, 0.0),
        turned((4.0, 22.0, 4.0), (0.0, -Q, 0.0)),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

pub fn pufferfish_mid() -> LayerDefinition {
    let mut r = PartDefinition::root();
    part(
        &mut r,
        "body",
        (12.0, 22.0),
        (-2.5, -5.0, -2.5),
        (5.0, 5.0, 5.0),
        at(0.0, 22.0, 0.0),
    );
    part(
        &mut r,
        "right_blue_fin",
        (24.0, 0.0),
        (-2.0, 0.0, 0.0),
        (2.0, 0.0, 2.0),
        at(-2.5, 18.0, -1.5),
    );
    part(
        &mut r,
        "left_blue_fin",
        (24.0, 3.0),
        (0.0, 0.0, 0.0),
        (2.0, 0.0, 2.0),
        at(2.5, 18.0, -1.5),
    );
    part(
        &mut r,
        "top_front_fin",
        (19.0, 17.0),
        (-2.5, -1.0, 0.0),
        (5.0, 1.0, 0.0),
        turned((0.0, 17.0, -2.5), (Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "top_back_fin",
        (11.0, 17.0),
        (-2.5, -1.0, 0.0),
        (5.0, 1.0, 0.0),
        turned((0.0, 17.0, 2.5), (-Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "right_front_fin",
        (5.0, 17.0),
        (-1.0, -5.0, 0.0),
        (1.0, 5.0, 0.0),
        turned((-2.5, 22.0, -2.5), (0.0, -Q, 0.0)),
    );
    part(
        &mut r,
        "right_back_fin",
        (9.0, 17.0),
        (-1.0, -5.0, 0.0),
        (1.0, 5.0, 0.0),
        turned((-2.5, 22.0, 2.5), (0.0, Q, 0.0)),
    );
    part(
        &mut r,
        "left_back_fin",
        (1.0, 17.0),
        (0.0, -5.0, 0.0),
        (1.0, 5.0, 0.0),
        turned((2.5, 22.0, 2.5), (0.0, -Q, 0.0)),
    );
    part(
        &mut r,
        "left_front_fin",
        (1.0, 17.0),
        (0.0, -5.0, 0.0),
        (1.0, 5.0, 0.0),
        turned((2.5, 22.0, -2.5), (0.0, Q, 0.0)),
    );
    part(
        &mut r,
        "bottom_back_fin",
        (18.0, 20.0),
        (0.0, 0.0, 0.0),
        (5.0, 1.0, 0.0),
        turned((-2.5, 22.0, 2.5), (Q, 0.0, 0.0)),
    );
    part(
        &mut r,
        "bottom_front_fin",
        (17.0, 19.0),
        (-2.5, 0.0, 0.0),
        (5.0, 1.0, 1.0),
        turned((0.0, 22.0, -2.5), (-Q, 0.0, 0.0)),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

pub fn pufferfish_small() -> LayerDefinition {
    let mut r = PartDefinition::root();
    part(
        &mut r,
        "body",
        (0.0, 27.0),
        (-1.5, -2.0, -1.5),
        (3.0, 2.0, 3.0),
        at(0.0, 23.0, 0.0),
    );
    part(
        &mut r,
        "right_eye",
        (24.0, 6.0),
        (-1.5, 0.0, -1.5),
        (1.0, 1.0, 1.0),
        at(0.0, 20.0, 0.0),
    );
    part(
        &mut r,
        "left_eye",
        (28.0, 6.0),
        (0.5, 0.0, -1.5),
        (1.0, 1.0, 1.0),
        at(0.0, 20.0, 0.0),
    );
    part(
        &mut r,
        "back_fin",
        (-3.0, 0.0),
        (-1.5, 0.0, 0.0),
        (3.0, 0.0, 3.0),
        at(0.0, 22.0, 1.5),
    );
    part(
        &mut r,
        "right_fin",
        (25.0, 0.0),
        (-1.0, 0.0, 0.0),
        (1.0, 0.0, 2.0),
        at(-1.5, 22.0, -1.5),
    );
    part(
        &mut r,
        "left_fin",
        (25.0, 0.0),
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 2.0),
        at(1.5, 22.0, -1.5),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

/// `TadpoleModel`: a small body and a flat tail.
pub fn tadpole() -> LayerDefinition {
    let mut r = PartDefinition::root();
    part(
        &mut r,
        "body",
        (0.0, 0.0),
        (-1.5, -1.0, 0.0),
        (3.0, 2.0, 3.0),
        at(0.0, 22.0, -3.0),
    );
    part(
        &mut r,
        "tail",
        (0.0, 0.0),
        (0.0, -1.0, 0.0),
        (0.0, 2.0, 7.0),
        at(0.0, 22.0, 0.0),
    );
    LayerDefinition::new(r, 16.0, 16.0)
}

/// `TropicalFishSmallModel.createBodyLayer(g)`: the pattern's copy is a
pub fn tropical_small(grow: f32) -> LayerDefinition {
    let mut r = PartDefinition::root();
    let g = |c: Cube| c.grown(grow);
    r.add_or_replace_child(
        "body",
        vec![g(Cube::new(
            (0.0, 0.0),
            (-1.0, -1.5, -3.0),
            (2.0, 3.0, 6.0),
        ))],
        at(0.0, 22.0, 0.0),
    );
    r.add_or_replace_child(
        "tail",
        vec![g(Cube::new(
            (22.0, -6.0),
            (0.0, -1.5, 0.0),
            (0.0, 3.0, 6.0),
        ))],
        at(0.0, 22.0, 3.0),
    );
    r.add_or_replace_child(
        "right_fin",
        vec![g(Cube::new(
            (2.0, 16.0),
            (-2.0, -1.0, 0.0),
            (2.0, 2.0, 0.0),
        ))],
        turned((-1.0, 22.5, 0.0), (0.0, Q, 0.0)),
    );
    r.add_or_replace_child(
        "left_fin",
        vec![g(Cube::new((2.0, 12.0), (0.0, -1.0, 0.0), (2.0, 2.0, 0.0)))],
        turned((1.0, 22.5, 0.0), (0.0, -Q, 0.0)),
    );
    r.add_or_replace_child(
        "top_fin",
        vec![g(Cube::new(
            (10.0, -5.0),
            (0.0, -3.0, 0.0),
            (0.0, 3.0, 6.0),
        ))],
        at(0.0, 20.5, -3.0),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

pub fn tropical_large(grow: f32) -> LayerDefinition {
    let mut r = PartDefinition::root();
    let g = |c: Cube| c.grown(grow);
    r.add_or_replace_child(
        "body",
        vec![g(Cube::new(
            (0.0, 20.0),
            (-1.0, -3.0, -3.0),
            (2.0, 6.0, 6.0),
        ))],
        at(0.0, 19.0, 0.0),
    );
    r.add_or_replace_child(
        "tail",
        vec![g(Cube::new(
            (21.0, 16.0),
            (0.0, -3.0, 0.0),
            (0.0, 6.0, 5.0),
        ))],
        at(0.0, 19.0, 3.0),
    );
    r.add_or_replace_child(
        "right_fin",
        vec![g(Cube::new((2.0, 16.0), (-2.0, 0.0, 0.0), (2.0, 2.0, 0.0)))],
        turned((-1.0, 20.0, 0.0), (0.0, Q, 0.0)),
    );
    r.add_or_replace_child(
        "left_fin",
        vec![g(Cube::new((2.0, 12.0), (0.0, 0.0, 0.0), (2.0, 2.0, 0.0)))],
        turned((1.0, 20.0, 0.0), (0.0, -Q, 0.0)),
    );
    r.add_or_replace_child(
        "top_fin",
        vec![g(Cube::new(
            (20.0, 11.0),
            (0.0, -4.0, 0.0),
            (0.0, 4.0, 6.0),
        ))],
        at(0.0, 16.0, -3.0),
    );
    r.add_or_replace_child(
        "bottom_fin",
        vec![g(Cube::new((20.0, 21.0), (0.0, 0.0, 0.0), (0.0, 4.0, 6.0)))],
        at(0.0, 22.0, -3.0),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

/// `TropicalFishPatternLayer`'s `CubeDeformation`.
pub const PATTERN_GROW: f32 = 0.008;

/// How a fish moves its tail or fins.
#[derive(Clone, Copy, Debug)]
pub enum FishRig {
    /// Cod and tropical fish: the tail swings about the spine.
    Tail(PartId),
    /// A tadpole: a slower, smaller wag than the fishes'.
    Tadpole(PartId),
    /// Salmon: the back half of the body swings.
    Salmon(PartId),
    /// Pufferfish: the two side fins flap, by `(left, right)`.
    Fins(PartId, PartId),
}

impl FishRig {
    /// The tail of a model that has one of either name.
    pub fn find_tail(model: &Model) -> Self {
        Self::Tail(
            model
                .find("tail_fin")
                .or_else(|| model.find("tail"))
                .expect("a tail"),
        )
    }

    pub fn tadpole(model: &Model) -> Self {
        Self::Tadpole(model.part("tail"))
    }

    pub fn salmon(model: &Model) -> Self {
        Self::Salmon(model.part("body_back"))
    }

    pub fn puffer(model: &Model) -> Self {
        match model.find("left_blue_fin") {
            Some(left) => Self::Fins(left, model.part("right_blue_fin")),
            None => Self::Fins(model.part("left_fin"), model.part("right_fin")),
        }
    }
}

/// `setupAnim` of the fish models.
pub fn setup_anim(rig: &FishRig, pose: &mut ModelPose, age: f32, in_water: bool) {
    match *rig {
        FishRig::Tail(tail) => {
            let amplitude = if in_water { 1.0 } else { 1.5 };
            pose[tail].pose.rotation.y = -amplitude * 0.45 * angle::sin(0.6 * age);
        }
        FishRig::Tadpole(tail) => {
            let amplitude = if in_water { 1.0 } else { 1.5 };
            pose[tail].pose.rotation.y = -amplitude * 0.25 * angle::sin(0.3 * age);
        }
        FishRig::Salmon(back) => {
            let (amplitude, speed) = if in_water { (1.0, 1.0) } else { (1.3, 1.7) };
            pose[back].pose.rotation.y = -amplitude * 0.25 * angle::sin(speed * 0.6 * age);
        }
        FishRig::Fins(left, right) => {
            let wave = angle::sin(age * 0.2);
            pose[right].pose.rotation.z = -0.2 + 0.4 * wave;
            pose[left].pose.rotation.z = 0.2 - 0.4 * wave;
        }
    }
}

/// What a fish's `setupRotations` adds on top of the body's turn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wobble {
    /// Degrees about the vertical.
    pub yaw: f32,
    /// Out of water it lies on its side, nudged by this.
    pub flop: Option<Vec3>,
    /// Lifted by this before anything else (the pufferfish bobs).
    pub bob: f32,
}

impl Wobble {
    pub fn cod(age: f32, in_water: bool) -> Self {
        Self {
            yaw: 4.3 * angle::sin(0.6 * age),
            flop: (!in_water).then_some(Vec3::new(0.1, 0.1, -0.1)),
            bob: 0.0,
        }
    }

    pub fn salmon(age: f32, in_water: bool) -> Self {
        let (amplitude, speed) = if in_water { (1.0, 1.0) } else { (1.3, 1.7) };
        Self {
            yaw: amplitude * 4.3 * angle::sin(speed * 0.6 * age),
            flop: (!in_water).then_some(Vec3::new(0.2, 0.1, 0.0)),
            bob: 0.0,
        }
    }

    pub fn tropical(age: f32, in_water: bool) -> Self {
        Self {
            yaw: 4.3 * angle::sin(0.6 * age),
            flop: (!in_water).then_some(Vec3::new(0.2, 0.1, 0.0)),
            bob: 0.0,
        }
    }

    pub fn pufferfish(age: f32) -> Self {
        Self {
            bob: angle::cos(age * 0.05) * 0.08,
            ..Default::default()
        }
    }
}

/// `TropicalFish.Pattern`'s packed id (`base | index << 8`) to the shape
pub fn tropical_pattern(packed: i32) -> (bool, usize) {
    let large = packed & 0xFF == 1;
    (large, ((packed >> 8) & 0xFF).clamp(0, 5) as usize)
}

/// The pattern texture of a shape and number.
pub fn pattern_texture(large: bool, number: usize) -> &'static str {
    const SMALL: [&str; 6] = [
        "minecraft:entity/fish/tropical_a_pattern_1",
        "minecraft:entity/fish/tropical_a_pattern_2",
        "minecraft:entity/fish/tropical_a_pattern_3",
        "minecraft:entity/fish/tropical_a_pattern_4",
        "minecraft:entity/fish/tropical_a_pattern_5",
        "minecraft:entity/fish/tropical_a_pattern_6",
    ];
    const LARGE: [&str; 6] = [
        "minecraft:entity/fish/tropical_b_pattern_1",
        "minecraft:entity/fish/tropical_b_pattern_2",
        "minecraft:entity/fish/tropical_b_pattern_3",
        "minecraft:entity/fish/tropical_b_pattern_4",
        "minecraft:entity/fish/tropical_b_pattern_5",
        "minecraft:entity/fish/tropical_b_pattern_6",
    ];
    if large { LARGE[number] } else { SMALL[number] }
}

/// The default `DATA_ID_TYPE_VARIANT`: kob, orange on orange.
pub const DEFAULT_TROPICAL: i32 = (1 << 16) | (1 << 24);

/// The fish renderers and `TadpoleRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Cod => out.push(plain(M::Cod, "minecraft:entity/fish/cod")),
        EntityKind::Salmon => {
            let model = match a.salmon_variant {
                0 => M::SalmonSmall,
                2 => M::SalmonLarge,
                _ => M::Salmon,
            };
            out.push(plain(model, "minecraft:entity/fish/salmon"));
        }
        EntityKind::Pufferfish => {
            let model = match a.puff_state {
                0 => M::PufferSmall,
                1 => M::PufferMid,
                _ => M::PufferBig,
            };
            out.push(plain(model, "minecraft:entity/fish/pufferfish"));
        }
        EntityKind::TropicalFish => {
            let (large, number) = tropical_pattern(a.tropical_variant);
            let dye = |shift: u32| {
                dye_tint(pumpkin_data::dye_color::DyeColor::by_id_or_white(
                    (a.tropical_variant >> shift) & 0xFF,
                ))
            };
            let (base, pattern, texture) = if large {
                (
                    M::TropicalLarge,
                    M::TropicalLargePattern,
                    "minecraft:entity/fish/tropical_b",
                )
            } else {
                (
                    M::TropicalSmall,
                    M::TropicalSmallPattern,
                    "minecraft:entity/fish/tropical_a",
                )
            };
            out.push(plain(base, texture).with_tint(dye(16)));
            out.push(plain(pattern, pattern_texture(large, number)).with_tint(dye(24)));
        }
        EntityKind::Tadpole => out.push(plain(M::Tadpole, "minecraft:entity/tadpole/tadpole")),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type FindRig = fn(&Model) -> FishRig;

    #[test]
    fn every_fish_model_bakes_and_its_rig_moves() {
        let cases: [(LayerDefinition, FindRig); 8] = [
            (cod(), FishRig::find_tail),
            (salmon(1.0), FishRig::salmon),
            (pufferfish_big(), FishRig::puffer),
            (pufferfish_mid(), FishRig::puffer),
            (pufferfish_small(), FishRig::puffer),
            (tropical_small(0.0), FishRig::find_tail),
            (tropical_large(PATTERN_GROW), FishRig::find_tail),
            (salmon(0.5), FishRig::salmon),
        ];
        for (layer, rig) in cases {
            let model = Model::bake(&layer);
            let rig = rig(&model);
            let rest = model.initial_pose();
            let mut pose = model.initial_pose();
            setup_anim(&rig, &mut pose, 5.0, true);
            assert_ne!(pose, rest, "{rig:?}");
        }
    }

    #[test]
    fn a_fish_out_of_water_thrashes_harder_and_lies_down() {
        let wet = Wobble::cod(3.0, true);
        let dry = Wobble::cod(3.0, false);
        assert_eq!(wet.flop, None);
        assert!(dry.flop.is_some());
        assert!(Wobble::salmon(3.0, false).yaw.abs() != Wobble::salmon(3.0, true).yaw.abs());
        assert!(Wobble::pufferfish(0.0).bob > 0.0);
    }

    #[test]
    fn the_tropical_variant_unpacks_shape_and_pattern() {
        assert_eq!(tropical_pattern(0), (false, 0));
        assert_eq!(tropical_pattern(1 | 4 << 8), (true, 4));
        assert!(pattern_texture(true, 4).ends_with("tropical_b_pattern_5"));
        assert_eq!(tropical_pattern(DEFAULT_TROPICAL), (false, 0));
    }
}
