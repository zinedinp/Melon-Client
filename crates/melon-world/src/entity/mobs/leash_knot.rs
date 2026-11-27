//! `LeashKnotModel`: the knot a lead is tied to a fence with.

use super::{LayerSpec, ModelId as M};
use crate::world::entity::model::{LayerDefinition, PartDefinition};
use crate::world::model_part::{Cube, Pose};

pub const TEXTURE: &str = "minecraft:entity/lead_knot/lead_knot";

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "knot",
        vec![Cube::new((0.0, 0.0), (-3.0, -8.0, -3.0), (6.0, 8.0, 6.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

/// `LeashKnotRenderer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::LeashKnot, TEXTURE));
}
