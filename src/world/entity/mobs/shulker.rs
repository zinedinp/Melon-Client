//! `ShulkerModel`: a lid and a base that hide a head.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

const COLORS: [&str; 16] = [
    "minecraft:entity/shulker/shulker_white",
    "minecraft:entity/shulker/shulker_orange",
    "minecraft:entity/shulker/shulker_magenta",
    "minecraft:entity/shulker/shulker_light_blue",
    "minecraft:entity/shulker/shulker_yellow",
    "minecraft:entity/shulker/shulker_lime",
    "minecraft:entity/shulker/shulker_pink",
    "minecraft:entity/shulker/shulker_gray",
    "minecraft:entity/shulker/shulker_light_gray",
    "minecraft:entity/shulker/shulker_cyan",
    "minecraft:entity/shulker/shulker_purple",
    "minecraft:entity/shulker/shulker_blue",
    "minecraft:entity/shulker/shulker_brown",
    "minecraft:entity/shulker/shulker_green",
    "minecraft:entity/shulker/shulker_red",
    "minecraft:entity/shulker/shulker_black",
];

/// Default (undyed) purple shell; `DATA_COLOR` 16.
pub const TEXTURE: &str = "minecraft:entity/shulker/shulker";

/// `ShulkerRenderer.getTextureLocation`: 0..15 dyed, 16 the default.
pub fn texture(color: i32) -> &'static str {
    usize::try_from(color)
        .ok()
        .and_then(|i| COLORS.get(i).copied())
        .unwrap_or(TEXTURE)
}

/// `DATA_PEEK` as 0..1 (`byte / 30`).
pub fn peek_from_data(peek: u8) -> f32 {
    (peek as f32 / 30.0).clamp(0.0, 1.0)
}

fn shell(root: &mut PartDefinition) {
    root.add_or_replace_child(
        "lid",
        vec![Cube::new(
            (0.0, 0.0),
            (-8.0, -16.0, -8.0),
            (16.0, 12.0, 16.0),
        )],
        Pose::offset(0.0, 24.0, 0.0),
    );
    root.add_or_replace_child(
        "base",
        vec![Cube::new(
            (0.0, 28.0),
            (-8.0, -8.0, -8.0),
            (16.0, 8.0, 16.0),
        )],
        Pose::offset(0.0, 24.0, 0.0),
    );
}

/// `ShulkerModel.createBodyLayer`.
pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    shell(&mut root);
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 52.0), (-3.0, 0.0, -3.0), (6.0, 6.0, 6.0))],
        Pose::offset(0.0, 12.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `ShulkerModel.createBoxLayer`: the shell without a head.
pub fn box_layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    shell(&mut root);
    LayerDefinition::new(root, 64.0, 64.0)
}

/// Peek 0..1 and the face it sits on (`Direction` ordinal, 0 down .. 5 up).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShulkerPose {
    pub peek: f32,
    pub attach: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct ShulkerParts {
    lid: PartId,
    head: PartId,
    base: PartId,
}

impl ShulkerParts {
    pub fn find(model: &Model) -> Self {
        Self {
            lid: model.part("lid"),
            head: model.part("head"),
            base: model.part("base"),
        }
    }
}

/// `ShulkerModel.setupAnim`.
pub fn setup_anim(
    parts: &ShulkerParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    shulker: ShulkerPose,
) {
    let _ = (parts.base, shulker.attach);
    let bs = (0.5 + shulker.peek) * PI;
    let q = -1.0 + angle::sin(bs);
    let extra = if bs > PI {
        angle::sin(state.age_in_ticks * 0.1) * 0.7
    } else {
        0.0
    };
    let lid = &mut pose[parts.lid].pose;
    lid.offset.x = 0.0;
    lid.offset.y = 16.0 + angle::sin(bs) * 8.0 + extra;
    lid.offset.z = 0.0;
    lid.rotation.y = if shulker.peek > 0.3 {
        q * q * q * q * PI * 0.125
    } else {
        0.0
    };
    let head = &mut pose[parts.head].pose;
    head.rotation.x = state.x_rot.to_radians();
    head.rotation.y = (state.y_rot - 180.0).to_radians();
}

/// `ShulkerRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(M::Shulker, texture(i32::from(a.shulker_color))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peeking_lifts_the_lid_and_the_default_shell_is_purple() {
        let model = Model::bake(&layer());
        let parts = ShulkerParts::find(&model);
        let closed = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                ShulkerPose::default(),
            );
            pose
        };
        let open = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                ShulkerPose {
                    peek: 1.0,
                    attach: 1,
                },
            );
            pose
        };
        assert!(open[parts.lid].pose.offset.y < closed[parts.lid].pose.offset.y - 10.0);
        assert_ne!(open[parts.lid].pose.rotation.y, 0.0);
        assert_eq!(texture(16), TEXTURE);
        assert!(texture(0).ends_with("shulker_white"));
        assert!((peek_from_data(30) - 1.0).abs() < 1e-5);
        let _ = Model::bake(&box_layer());
    }
}
