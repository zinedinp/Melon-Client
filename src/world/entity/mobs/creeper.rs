//! `CreeperModel` and `CreeperRenderer`'s swelling before it explodes.

use glam::Vec3;

use super::{LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::{Legs, look};
use crate::world::model_part::{Cube, Pose};

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0))],
        Pose::offset(0.0, 6.0, 0.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((16.0, 16.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0))],
        Pose::offset(0.0, 6.0, 0.0),
    );
    let leg = || vec![Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 6.0, 4.0))];
    root.add_or_replace_child("right_hind_leg", leg(), Pose::offset(-2.0, 18.0, 4.0));
    root.add_or_replace_child("left_hind_leg", leg(), Pose::offset(2.0, 18.0, 4.0));
    root.add_or_replace_child("right_front_leg", leg(), Pose::offset(-2.0, 18.0, -4.0));
    root.add_or_replace_child("left_front_leg", leg(), Pose::offset(2.0, 18.0, -4.0));
    LayerDefinition::new(root, 64.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct CreeperParts {
    head: PartId,
    legs: Legs,
}

impl CreeperParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            legs: Legs::find(model),
        }
    }
}

/// `CreeperModel.setupAnim`. Its fields swap left and right against the
pub fn setup_anim(parts: &CreeperParts, pose: &mut ModelPose, state: &LivingRenderState) {
    look(&mut pose[parts.head].pose, state);
    parts
        .legs
        .walk(pose, state.walk_position, state.walk_speed, PI);
}

/// `CreeperRenderer.scale`: wider and a little taller as the fuse burns,
pub fn swell_scale(swelling: f32) -> Vec3 {
    let wobble = 1.0 + angle::sin(swelling * 100.0) * swelling * 0.01;
    let g = swelling.clamp(0.0, 1.0).powi(4);
    let s = (1.0 + g * 0.4) * wobble;
    let hs = (1.0 + g * 0.1) / wobble;
    Vec3::new(s, hs, s)
}

/// `CreeperRenderer.getWhiteOverlayProgress`, as how far toward white the
pub fn white_flash(swelling: f32) -> f32 {
    if ((swelling * 10.0) as i32) % 2 == 0 {
        return 0.0;
    }
    let progress = swelling.clamp(0.5, 1.0);
    (progress * 15.0).floor() / 15.0 * 0.75
}

/// `CreeperRenderer` (the charge is `CreeperPowerLayer`, not here).
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Creeper, "minecraft:entity/creeper/creeper"));
}
