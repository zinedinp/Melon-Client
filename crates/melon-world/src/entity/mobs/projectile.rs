//! The models of things that fly or lie about rather than live: the
//! wither's skulls, shulker bullets, llama spit, wind charges, tridents
//! and arrows, evoker fangs, end crystals and cushions. Each renderer
//! places its model its own way ([`root`]); the few parts that move are
//! turned by [`setup_anim`].

use super::rounded;
use glam::{Mat3, Mat4, Quat, Vec3};

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::kind::EntityKind;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, DEG_TO_RAD, PI, Rotation};

pub const WITHER_TEXTURE: &str = "minecraft:entity/wither/wither";
pub const WITHER_INVULNERABLE_TEXTURE: &str = "minecraft:entity/wither/wither_invulnerable";
pub const SHULKER_BULLET_TEXTURE: &str = "minecraft:entity/shulker/spark";
pub const LLAMA_SPIT_TEXTURE: &str = "minecraft:entity/llama/llama_spit";
pub const WIND_CHARGE_TEXTURE: &str = "minecraft:entity/projectiles/wind_charge";
pub const EVOKER_FANGS_TEXTURE: &str = "minecraft:entity/illager/evoker_fangs";
pub const END_CRYSTAL_TEXTURE: &str = "minecraft:entity/end_crystal/end_crystal";
pub const TRIDENT_TEXTURE: &str = "minecraft:entity/trident/trident";
pub const ARROW_TEXTURE: &str = "minecraft:entity/projectiles/arrow";
pub const TIPPED_ARROW_TEXTURE: &str = "minecraft:entity/projectiles/arrow_tipped";
pub const SPECTRAL_ARROW_TEXTURE: &str = "minecraft:entity/projectiles/arrow_spectral";

/// `CushionRenderer.TEXTURES_BY_COLOR`, by dye id.
pub fn cushion_texture(color: u8) -> &'static str {
    const BY_COLOR: [&str; 16] = [
        "minecraft:entity/cushion/white_cushion",
        "minecraft:entity/cushion/orange_cushion",
        "minecraft:entity/cushion/magenta_cushion",
        "minecraft:entity/cushion/light_blue_cushion",
        "minecraft:entity/cushion/yellow_cushion",
        "minecraft:entity/cushion/lime_cushion",
        "minecraft:entity/cushion/pink_cushion",
        "minecraft:entity/cushion/gray_cushion",
        "minecraft:entity/cushion/light_gray_cushion",
        "minecraft:entity/cushion/cyan_cushion",
        "minecraft:entity/cushion/purple_cushion",
        "minecraft:entity/cushion/blue_cushion",
        "minecraft:entity/cushion/brown_cushion",
        "minecraft:entity/cushion/green_cushion",
        "minecraft:entity/cushion/red_cushion",
        "minecraft:entity/cushion/black_cushion",
    ];
    BY_COLOR[usize::from(color.min(15))]
}

/// `WitherSkullRenderer.createSkullLayer`.
pub fn wither_skull() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 35.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `ShulkerBulletModel.createBodyLayer`: three crossed slabs.
pub fn shulker_bullet() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "main",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -4.0, -1.0), (8.0, 8.0, 2.0)),
            Cube::new((0.0, 10.0), (-1.0, -4.0, -4.0), (2.0, 8.0, 8.0)),
            Cube::new((20.0, 0.0), (-4.0, -1.0, -4.0), (8.0, 2.0, 8.0)),
        ],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// The bullet under `ShulkerBulletRenderer`'s `scale(1.5)`: uniform, so
/// the same as growing the one part that turns.
pub fn shulker_bullet_glow() -> LayerDefinition {
    let mut layer = shulker_bullet();
    layer.root.child_mut("main").pose.scale = Vec3::splat(SHULKER_GLOW_SCALE);
    layer
}

/// `LlamaSpitModel.createBodyLayer`: seven little cubes.
pub fn llama_spit() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let at = |x, y, z| Cube::new((0.0, 0.0), (x, y, z), (2.0, 2.0, 2.0));
    root.add_or_replace_child(
        "main",
        vec![
            at(-4.0, 0.0, 0.0),
            at(0.0, -4.0, 0.0),
            at(0.0, 0.0, -4.0),
            at(0.0, 0.0, 0.0),
            at(2.0, 0.0, 0.0),
            at(0.0, 2.0, 0.0),
            at(0.0, 0.0, 2.0),
        ],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `WindChargeModel.createBodyLayer`: a core in two whirls of wind.
pub fn wind_charge() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let bone = root.add_or_replace_child("bone", vec![], Pose::ZERO);
    bone.add_or_replace_child(
        "wind",
        vec![
            Cube::new((15.0, 20.0), (-4.0, -1.0, -4.0), (8.0, 2.0, 8.0)),
            Cube::new((0.0, 9.0), (-3.0, -2.0, -3.0), (6.0, 4.0, 6.0)),
        ],
        Pose::offset_rotation((0.0, 0.0, 0.0), (0.0, -rounded::PI_4, 0.0)),
    );
    bone.add_or_replace_child(
        "wind_charge",
        vec![Cube::new((0.0, 0.0), (-2.0, -2.0, -2.0), (4.0, 4.0, 4.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `EvokerFangsModel.createBodyLayer`.
pub fn evoker_fangs() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let base = root.add_or_replace_child(
        "base",
        vec![Cube::new((0.0, 0.0), (0.0, 0.0, 0.0), (10.0, 12.0, 10.0))],
        Pose::offset(-5.0, 24.0, -5.0),
    );
    let jaw = || vec![Cube::new((40.0, 0.0), (0.0, 0.0, 0.0), (4.0, 14.0, 8.0))];
    base.add_or_replace_child(
        "upper_jaw",
        jaw(),
        Pose::offset_rotation((6.5, 0.0, 1.0), (0.0, 0.0, 2.042_035)),
    );
    base.add_or_replace_child(
        "lower_jaw",
        jaw(),
        Pose::offset_rotation((3.5, 0.0, 9.0), (0.0, PI, 4.241_150_4)),
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `EndCrystalModel.createBodyLayer`: a cube in two glass cubes, on a base.
pub fn end_crystal() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let glass = || vec![Cube::new((0.0, 0.0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))];
    let outer = root.add_or_replace_child("outer_glass", glass(), Pose::offset(0.0, 24.0, 0.0));
    let inner = outer.add_or_replace_child(
        "inner_glass",
        glass(),
        Pose {
            scale: Vec3::splat(0.875),
            ..Pose::ZERO
        },
    );
    inner.add_or_replace_child(
        "cube",
        vec![Cube::new((32.0, 0.0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
        Pose {
            scale: Vec3::splat(0.765_625),
            ..Pose::ZERO
        },
    );
    root.add_or_replace_child(
        "base",
        vec![Cube::new((0.0, 16.0), (-6.0, 0.0, -6.0), (12.0, 4.0, 12.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `TridentModel.createLayer`.
pub fn trident() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let pole = root.add_or_replace_child(
        "pole",
        vec![Cube::new((0.0, 6.0), (-0.5, 2.0, -0.5), (1.0, 25.0, 1.0))],
        Pose::ZERO,
    );
    pole.add_or_replace_child(
        "base",
        vec![Cube::new((4.0, 0.0), (-1.5, 0.0, -0.5), (3.0, 2.0, 1.0))],
        Pose::ZERO,
    );
    pole.add_or_replace_child(
        "left_spike",
        vec![Cube::new((4.0, 3.0), (-2.5, -3.0, -0.5), (1.0, 4.0, 1.0))],
        Pose::ZERO,
    );
    pole.add_or_replace_child(
        "middle_spike",
        vec![Cube::new((0.0, 0.0), (-0.5, -4.0, -0.5), (1.0, 4.0, 1.0))],
        Pose::ZERO,
    );
    pole.add_or_replace_child(
        "right_spike",
        vec![Cube::new((4.0, 3.0), (1.5, -3.0, -0.5), (1.0, 4.0, 1.0)).mirrored()],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

/// `ArrowModel.createBodyLayer`: two crossed planes and the back, the
/// whole grown by `mesh.transformed(pose.scaled(0.9))`.
pub fn arrow() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "back",
        vec![Cube::new((0.0, 0.0), (0.0, -2.5, -2.5), (0.0, 5.0, 5.0))],
        // `withScale(0.8)` scales the boxes but not the offset.
        Pose {
            scale: Vec3::splat(0.8),
            ..Pose::offset_rotation((-11.0, 0.0, 0.0), (PI / 4.0, 0.0, 0.0))
        }
        .scaled(0.9),
    );
    let cross =
        || vec![Cube::new((0.0, 0.0), (-12.0, -2.0, 0.0), (16.0, 4.0, 0.0)).tex_scaled(1.0, 0.8)];
    root.add_or_replace_child(
        "cross_1",
        cross(),
        Pose::offset_rotation((0.0, 0.0, 0.0), (PI / 4.0, 0.0, 0.0)).scaled(0.9),
    );
    root.add_or_replace_child(
        "cross_2",
        cross(),
        Pose::offset_rotation((0.0, 0.0, 0.0), (PI * 3.0 / 4.0, 0.0, 0.0)).scaled(0.9),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

/// `CushionModel.createBodyLayer`.
pub fn cushion() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "cushion",
        vec![Cube::new((0.0, 0.0), (-31.0, -4.0, -1.0), (16.0, 4.0, 16.0)).grown(-0.005)],
        Pose::offset(23.0, 4.0, -7.0),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// The parts a thing's `setupAnim` moves.
#[derive(Clone, Copy, Debug)]
pub enum ThingParts {
    /// `SkullModel` / `ShulkerBulletModel`: one part turned by the look.
    Look(PartId),
    WindCharge {
        charge: PartId,
        wind: PartId,
    },
    Fangs {
        base: PartId,
        upper: PartId,
        lower: PartId,
    },
    Crystal {
        base: PartId,
        outer: PartId,
        inner: PartId,
        cube: PartId,
    },
    /// `ArrowModel`: the root wobbles when it sticks.
    Arrow,
}

impl ThingParts {
    pub fn look(model: &Model, name: &str) -> Self {
        Self::Look(model.part(name))
    }

    pub fn wind_charge(model: &Model) -> Self {
        Self::WindCharge {
            charge: model.part("wind_charge"),
            wind: model.part("wind"),
        }
    }

    pub fn fangs(model: &Model) -> Self {
        Self::Fangs {
            base: model.part("base"),
            upper: model.part("upper_jaw"),
            lower: model.part("lower_jaw"),
        }
    }

    pub fn crystal(model: &Model) -> Self {
        Self::Crystal {
            base: model.part("base"),
            outer: model.part("outer_glass"),
            inner: model.part("inner_glass"),
            cube: model.part("cube"),
        }
    }
}

/// What the things' `setupAnim`s read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThingPose {
    /// The entity's look, degrees (`getYRot`, `getXRot`).
    pub y_rot: f32,
    pub x_rot: f32,
    pub age_in_ticks: f32,
    /// `EvokerFangsRenderState.biteProgress`.
    pub bite: f32,
    /// `ArrowRenderState.shake`.
    pub shake: f32,
    /// `EndCrystal.showsBottom`.
    pub shows_bottom: bool,
}

pub fn setup_anim(model: &Model, parts: ThingParts, pose: &mut ModelPose, input: ThingPose) {
    match parts {
        ThingParts::Look(part) => {
            let p = &mut pose.get_mut(part).pose;
            p.rotation.y = input.y_rot * DEG_TO_RAD;
            p.rotation.x = input.x_rot * DEG_TO_RAD;
        }
        ThingParts::WindCharge { charge, wind } => {
            pose.get_mut(charge).pose.rotation.y = -input.age_in_ticks * 16.0 * DEG_TO_RAD;
            pose.get_mut(wind).pose.rotation.y = input.age_in_ticks * 16.0 * DEG_TO_RAD;
        }
        ThingParts::Fangs { base, upper, lower } => {
            let bite = input.bite;
            let amount = (bite * 2.0).min(1.0);
            let amount = 1.0 - amount * amount * amount;
            pose.get_mut(upper).pose.rotation.z = PI - amount * 0.35 * PI;
            pose.get_mut(lower).pose.rotation.z = PI + amount * 0.35 * PI;
            pose.get_mut(base).pose.offset.y -= (bite + angle::sin(bite * 2.7)) * 7.2;
            let mut pre_scale = 1.0;
            if bite > 0.9 {
                pre_scale *= (1.0 - bite) / 0.1;
            }
            let root = &mut pose.get_mut(model.root()).pose;
            root.offset.y = 24.0 - 20.0 * pre_scale;
            root.scale = Vec3::splat(pre_scale);
        }
        ThingParts::Crystal {
            base,
            outer,
            inner,
            cube,
        } => {
            pose.get_mut(base).visible = input.shows_bottom;
            let speed = input.age_in_ticks * 3.0;
            let crystal_y = crystal_y(input.age_in_ticks) * 16.0;
            pose.get_mut(outer).pose.offset.y += crystal_y / 2.0;
            let sin45 = std::f32::consts::FRAC_PI_4.sin();
            let axis = Vec3::new(sin45, 0.0, sin45);
            let tilt = angle::axis_angle(axis, PI / 3.0);
            let spin = Quat::from_rotation_y(speed * DEG_TO_RAD);
            rotate_by(&mut pose.get_mut(outer).pose, spin * tilt);
            rotate_by(&mut pose.get_mut(inner).pose, tilt * spin);
            rotate_by(&mut pose.get_mut(cube).pose, tilt * spin);
        }
        ThingParts::Arrow => {
            if input.shake > 0.0 {
                let pow = -angle::sin(input.shake * 3.0) * input.shake;
                pose.get_mut(model.root()).pose.rotation.z += pow * DEG_TO_RAD;
            }
        }
    }
}

/// `ModelPart.rotateBy`: the part's angles turned further by `rotation`.
fn rotate_by(pose: &mut Pose, rotation: Quat) {
    let m = Mat3::euler_zyx(pose.rotation) * Mat3::from_quat(rotation);
    // `Matrix3f.getEulerAnglesZYX`.
    pose.rotation = Vec3::new(
        m.y_axis.z.atan2(m.z_axis.z),
        (-m.x_axis.z).clamp(-1.0, 1.0).asin(),
        m.x_axis.y.atan2(m.x_axis.x),
    );
}

/// `EndCrystalRenderer.getY`: how far the crystal bobs, in blocks.
pub fn crystal_y(age_in_ticks: f32) -> f32 {
    let hh = angle::sin(age_in_ticks * 0.2) / 2.0 + 0.5;
    let hh = (hh * hh + hh) * 0.4;
    hh - 1.4
}

/// `EvokerFangs.getAnimationProgress`: 0 until the bite (event 4)
/// starts it, then up to 1 over 20 ticks.
pub fn bite_progress(life_ticks: Option<i32>, partial: f32) -> f32 {
    let Some(life) = life_ticks else {
        return 0.0;
    };
    let life = life - 2;
    if life <= 0 {
        1.0
    } else {
        1.0 - (life as f32 - partial) / 20.0
    }
}

/// `EvokerFangs.handleEntityEvent`'s attack: the bite starts.
pub const EVENT_FANGS_ATTACK: u8 = 4;
/// `EvokerFangs.lifeTicks` at the start.
pub const FANGS_LIFE_TICKS: i32 = 22;
/// `AbstractArrow.shakeTime` when it sticks.
pub const ARROW_SHAKE_TICKS: i32 = 7;

/// Each renderer's `submit` placement before the model: from the
/// entity's feet (`position`, relative to the draw's block) with its
/// look in degrees. Vanilla flips models with `scale(-1, -1, 1)` and
/// lifts them by 1.501 where `LivingEntityRenderer` would; the things
/// do it themselves, or not.
pub fn root(kind: EntityKind, position: Vec3, y_rot: f32, x_rot: f32, age: f32) -> Mat4 {
    let at = Mat4::from_translation(position);
    let flip = Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0));
    match kind {
        // `WitherSkullRenderer`: the skull's own look turns the head.
        EntityKind::WitherSkull => at * flip,
        EntityKind::ShulkerBullet => {
            at * Mat4::from_translation(Vec3::new(0.0, 0.15, 0.0))
                * Mat4::rot_y_deg(angle::sin(age * 0.1) * 180.0)
                * Mat4::rot_x_deg(angle::cos(age * 0.1) * 180.0)
                * Mat4::rot_z_deg(angle::sin(age * 0.15) * 360.0)
                * Mat4::from_scale(Vec3::new(-0.5, -0.5, 0.5))
        }
        EntityKind::LlamaSpit => {
            at * Mat4::from_translation(Vec3::new(0.0, 0.15, 0.0))
                * Mat4::rot_y_deg(y_rot - 90.0)
                * Mat4::rot_z_deg(x_rot)
        }
        EntityKind::EvokerFangs => {
            at * Mat4::rot_y_deg(90.0 - y_rot)
                * flip
                * Mat4::from_translation(Vec3::new(0.0, -1.501, 0.0))
        }
        EntityKind::EndCrystal => {
            at * Mat4::from_scale(Vec3::splat(2.0))
                * Mat4::from_translation(Vec3::new(0.0, -0.5, 0.0))
        }
        EntityKind::Trident => at * Mat4::rot_y_deg(y_rot - 90.0) * Mat4::rot_z_deg(x_rot + 90.0),
        EntityKind::Arrow | EntityKind::SpectralArrow => {
            at * Mat4::rot_y_deg(y_rot - 90.0) * Mat4::rot_z_deg(x_rot)
        }
        // `CushionRenderer`: facing its snapped direction, upside down.
        EntityKind::Cushion => {
            at * Mat4::rot_y_deg(180.0 - snapped_yaw(y_rot))
                * Mat4::rot_x_deg(180.0)
                * Mat4::from_translation(Vec3::new(0.0, -0.25, 0.0))
        }
        // `WindChargeRenderer` draws the model as it is.
        _ => at,
    }
}

/// `ThrownItemRenderer`'s scale and full brightness by type: fireballs
/// are big and lit, an eye of ender lit.
pub fn thrown_look(ty: &pumpkin_data::entity::EntityType) -> (f32, bool) {
    use pumpkin_data::entity::EntityType;
    if ty.id == EntityType::FIREBALL.id {
        (3.0, true)
    } else if ty.id == EntityType::SMALL_FIREBALL.id {
        (0.75, true)
    } else if ty.id == EntityType::EYE_OF_ENDER.id {
        (1.0, true)
    } else {
        (1.0, false)
    }
}

/// `Direction.fromYRot(yRot).toYRot()`: the look snapped to a side.
pub fn snapped_yaw(y_rot: f32) -> f32 {
    let quarter = ((y_rot / 90.0 + 0.5).floor() as i32).rem_euclid(4);
    // `Direction.from2DDataValue`: south, west, north, east.
    quarter as f32 * 90.0
}

/// `ShulkerBulletRenderer`'s glow: the model again, half as big again,
/// at `0x26` alpha (`654311423` is `0x26FFFFFF`).
pub const SHULKER_GLOW_SCALE: f32 = 1.5;
pub const SHULKER_GLOW_ALPHA: f32 = 0x26 as f32 / 255.0;

/// The projectile and end crystal renderers.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Cushion => out.push(plain(M::Cushion, cushion_texture(a.thing.color))),
        EntityKind::WitherSkull => out.push(plain(
            M::WitherSkull,
            if a.thing.dangerous {
                WITHER_INVULNERABLE_TEXTURE
            } else {
                WITHER_TEXTURE
            },
        )),
        EntityKind::ShulkerBullet => {
            out.push(plain(M::ShulkerBullet, SHULKER_BULLET_TEXTURE));
            out.push(plain(M::ShulkerBulletGlow, SHULKER_BULLET_TEXTURE).faded(SHULKER_GLOW_ALPHA));
        }
        EntityKind::LlamaSpit => out.push(plain(M::LlamaSpit, LLAMA_SPIT_TEXTURE)),
        EntityKind::WindCharge => out.push(plain(M::WindCharge, WIND_CHARGE_TEXTURE).translucent()),
        EntityKind::EvokerFangs => {
            // `EvokerFangsRenderer`: nothing until the bite starts.
            if entity.fangs_life.is_some() {
                out.push(plain(M::EvokerFangs, EVOKER_FANGS_TEXTURE));
            }
        }
        EntityKind::EndCrystal => out.push(plain(M::EndCrystal, END_CRYSTAL_TEXTURE)),
        EntityKind::Trident => out.push(plain(M::Trident, TRIDENT_TEXTURE).with_foil(a.thing.foil)),
        EntityKind::Arrow => out.push(plain(
            M::Arrow,
            if a.thing.arrow_color > 0 {
                TIPPED_ARROW_TEXTURE
            } else {
                ARROW_TEXTURE
            },
        )),
        EntityKind::SpectralArrow => out.push(plain(M::Arrow, SPECTRAL_ARROW_TEXTURE)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bite_runs_from_nothing_to_whole() {
        assert_eq!(bite_progress(None, 0.5), 0.0);
        assert!((bite_progress(Some(FANGS_LIFE_TICKS), 0.0) - 0.0).abs() < 1e-6);
        assert_eq!(bite_progress(Some(2), 0.3), 1.0);
    }

    #[test]
    fn rotate_by_the_identity_keeps_the_angles() {
        let mut pose = Pose::offset_rotation((0.0, 0.0, 0.0), (0.3, -0.2, 0.1));
        rotate_by(&mut pose, Quat::IDENTITY);
        assert!((pose.rotation - Vec3::new(0.3, -0.2, 0.1)).length() < 1e-5);
    }

    #[test]
    fn a_cushion_snaps_to_the_nearest_side() {
        assert_eq!(snapped_yaw(10.0), 0.0);
        assert_eq!(snapped_yaw(80.0), 90.0);
        assert_eq!(snapped_yaw(-100.0), 270.0);
    }
}
