//! `WitherBossModel`: three heads on a ribcage and a short tail.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::look;
use crate::world::model_part::{Cube, Pose};

const RIBCAGE_X_ROT: f32 = 0.204_203_52;
const RIBCAGE_X_ROT_OFFSET: f32 = 0.065;
const TAIL_X_ROT_OFFSET: f32 = 0.265;

pub const TEXTURE: &str = "minecraft:entity/wither/wither";
pub const INVULNERABLE_TEXTURE: &str = "minecraft:entity/wither/wither_invulnerable";
pub const ARMOR_TEXTURE: &str = "minecraft:entity/wither/wither_armor";

/// `WitherBossRenderer.getTextureLocation`.
pub fn texture(invulnerable_ticks: f32) -> &'static str {
    let ticks = invulnerable_ticks.floor() as i32;
    if ticks > 0 && (ticks > 80 || ticks / 5 % 2 != 1) {
        INVULNERABLE_TEXTURE
    } else {
        TEXTURE
    }
}

/// `WitherBoss.isPowered`: at half health or less it wears its armor.
pub fn powered(health: f32, max_health: f32) -> bool {
    health <= max_health / 2.0
}

pub fn armor_texture() -> &'static str {
    ARMOR_TEXTURE
}

fn body_layer(g: f32) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "shoulders",
        vec![Cube::new((0.0, 16.0), (-10.0, 3.9, -0.5), (20.0, 3.0, 3.0)).grown(g)],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "ribcage",
        vec![
            Cube::new((0.0, 22.0), (0.0, 0.0, 0.0), (3.0, 10.0, 3.0)).grown(g),
            Cube::new((24.0, 22.0), (-4.0, 1.5, 0.5), (11.0, 2.0, 2.0)).grown(g),
            Cube::new((24.0, 22.0), (-4.0, 4.0, 0.5), (11.0, 2.0, 2.0)).grown(g),
            Cube::new((24.0, 22.0), (-4.0, 6.5, 0.5), (11.0, 2.0, 2.0)).grown(g),
        ],
        Pose::offset_rotation((-2.0, 6.9, -0.5), (RIBCAGE_X_ROT, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "tail",
        vec![Cube::new((12.0, 22.0), (0.0, 0.0, 0.0), (3.0, 6.0, 3.0)).grown(g)],
        Pose::offset_rotation(
            (
                -2.0,
                6.9 + angle::cos(RIBCAGE_X_ROT) * 10.0,
                -0.5 + angle::sin(RIBCAGE_X_ROT) * 10.0,
            ),
            (0.832_522_03, 0.0, 0.0),
        ),
    );
    root.add_or_replace_child(
        "center_head",
        vec![Cube::new((0.0, 0.0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0)).grown(g)],
        Pose::ZERO,
    );
    let side = || vec![Cube::new((32.0, 0.0), (-4.0, -4.0, -4.0), (6.0, 6.0, 6.0)).grown(g)];
    root.add_or_replace_child("right_head", side(), Pose::offset(-8.0, 4.0, 0.0));
    root.add_or_replace_child("left_head", side(), Pose::offset(10.0, 4.0, 0.0));
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `WitherBossModel.createBodyLayer(NONE)`.
pub fn layer() -> LayerDefinition {
    body_layer(0.0)
}

/// `WITHER_ARMOR`: `createBodyLayer(INNER_ARMOR_DEFORMATION)` of 0.5.
pub fn armor_layer() -> LayerDefinition {
    body_layer(0.5)
}

/// Overlay / invulnerable skin while `DATA_ID_INV > 0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WitherPose {
    pub invul: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WitherParts {
    center_head: PartId,
    right_head: PartId,
    left_head: PartId,
    ribcage: PartId,
    tail: PartId,
}

impl WitherParts {
    pub fn find(model: &Model) -> Self {
        Self {
            center_head: model.part("center_head"),
            right_head: model.part("right_head"),
            left_head: model.part("left_head"),
            ribcage: model.part("ribcage"),
            tail: model.part("tail"),
        }
    }
}

/// `WitherBossModel.setupAnim`.
pub fn setup_anim(
    parts: &WitherParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    _wither: WitherPose,
) {
    look(&mut pose[parts.right_head].pose, state);
    look(&mut pose[parts.left_head].pose, state);
    look(&mut pose[parts.center_head].pose, state);
    let anim = angle::cos(state.age_in_ticks * 0.1);
    pose[parts.ribcage].pose.rotation.x = (RIBCAGE_X_ROT_OFFSET + 0.05 * anim) * PI;
    let rib_x = pose[parts.ribcage].pose.rotation.x;
    {
        let tail = &mut pose[parts.tail].pose;
        tail.offset.x = -2.0;
        tail.offset.y = 6.9 + angle::cos(rib_x) * 10.0;
        tail.offset.z = -0.5 + angle::sin(rib_x) * 10.0;
        tail.rotation.x = (TAIL_X_ROT_OFFSET + 0.1 * anim) * PI;
    }
}

/// `WitherBossRenderer` and `WitherArmorLayer`.
pub(super) fn push_layers(entity: &ClientEntity, age_in_ticks: f32, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(M::Wither, texture(a.wither_invul as f32)));
    if powered(a.health, entity.max_health) {
        let t = age_in_ticks;
        out.push(
            plain(M::WitherArmor, armor_texture())
                .energy_swirl(angle::cos(t * 0.02) * 3.0, t * 0.01),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ribs_breathe_and_the_invulnerable_skin_blinks() {
        let model = Model::bake(&layer());
        let parts = WitherParts::find(&model);
        let a = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                WitherPose::default(),
            );
            pose
        };
        let b = {
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState {
                    age_in_ticks: 8.0,
                    ..Default::default()
                },
                WitherPose { invul: true },
            );
            pose
        };
        assert_ne!(
            a[parts.ribcage].pose.rotation.x,
            b[parts.ribcage].pose.rotation.x
        );
        let _ = WitherParts::find(&Model::bake(&armor_layer()));
        assert_eq!(texture(0.0), TEXTURE);
        assert_eq!(texture(100.0), INVULNERABLE_TEXTURE);
        assert_eq!(armor_texture(), ARMOR_TEXTURE);
    }
}
