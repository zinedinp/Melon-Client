//! Boats, rafts and minecarts: `BoatModel`, `RaftModel`, `MinecartModel` and

use crate::math::angle::{EIGHTH_PI, HALF_PI as FRAC_PI_2, PI, QUARTER_PI as FRAC_PI_4, THIRD_PI};

use glam::{Mat4, Vec3};

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::math::angle::{self, Rotation};
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

pub const MINECART_TEXTURE: &str = "minecraft:entity/minecart/minecart";

/// How far a paddle clock moves in a tick while it is in the water.
pub const PADDLE_STEP: f32 = EIGHTH_PI;

/// `PartPose.offsetAndRotation`.
fn at(offset: (f32, f32, f32), rotation: (f32, f32, f32)) -> Pose {
    Pose::offset_rotation(offset, rotation)
}

/// The two paddles, which `AbstractBoatModel` turns.
fn add_paddles(root: &mut PartDefinition, left_uv: (f32, f32), right_uv: (f32, f32), y: f32) {
    root.add_or_replace_child(
        "left_paddle",
        vec![
            Cube::new(left_uv, (-1.0, 0.0, -5.0), (2.0, 2.0, 18.0)),
            Cube::new(left_uv, (-1.001, -3.0, 8.0), (1.0, 6.0, 7.0)),
        ],
        at((3.0, y, 9.0), (0.0, 0.0, PI / 16.0)),
    );
    root.add_or_replace_child(
        "right_paddle",
        vec![
            Cube::new(right_uv, (-1.0, 0.0, -5.0), (2.0, 2.0, 18.0)),
            Cube::new(right_uv, (0.001, -3.0, 8.0), (1.0, 6.0, 7.0)),
        ],
        at((3.0, y, -9.0), (0.0, PI, PI / 16.0)),
    );
}

/// The chest a chest boat carries: `chest_bottom`, `chest_lid`, `chest_lock`,
fn add_chest(root: &mut PartDefinition, lift: f32) {
    let turn = -FRAC_PI_2;
    root.add_or_replace_child(
        "chest_bottom",
        vec![Cube::new((0.0, 76.0), (0.0, 0.0, 0.0), (12.0, 8.0, 12.0))],
        at((-2.0, -5.0 + lift, -6.0), (0.0, turn, 0.0)),
    );
    root.add_or_replace_child(
        "chest_lid",
        vec![Cube::new((0.0, 59.0), (0.0, 0.0, 0.0), (12.0, 4.0, 12.0))],
        at((-2.0, -9.0 + lift, -6.0), (0.0, turn, 0.0)),
    );
    root.add_or_replace_child(
        "chest_lock",
        vec![Cube::new((0.0, 59.0), (0.0, 0.0, 0.0), (2.0, 4.0, 1.0))],
        at((-1.0, -6.0 + lift, -1.0), (0.0, turn, 0.0)),
    );
}

/// `BoatModel.createBoatModel` and `createChestBoatModel`.
pub fn boat(chest: bool) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "bottom",
        vec![Cube::new(
            (0.0, 0.0),
            (-14.0, -9.0, -3.0),
            (28.0, 16.0, 3.0),
        )],
        at((0.0, 3.0, 1.0), (FRAC_PI_2, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "back",
        vec![Cube::new(
            (0.0, 19.0),
            (-13.0, -7.0, -1.0),
            (18.0, 6.0, 2.0),
        )],
        at((-15.0, 4.0, 4.0), (0.0, PI * 1.5, 0.0)),
    );
    root.add_or_replace_child(
        "front",
        vec![Cube::new((0.0, 27.0), (-8.0, -7.0, -1.0), (16.0, 6.0, 2.0))],
        at((15.0, 4.0, 0.0), (0.0, FRAC_PI_2, 0.0)),
    );
    root.add_or_replace_child(
        "right",
        vec![Cube::new(
            (0.0, 35.0),
            (-14.0, -7.0, -1.0),
            (28.0, 6.0, 2.0),
        )],
        at((0.0, 4.0, -9.0), (0.0, PI, 0.0)),
    );
    root.add_or_replace_child(
        "left",
        vec![Cube::new(
            (0.0, 43.0),
            (-14.0, -7.0, -1.0),
            (28.0, 6.0, 2.0),
        )],
        Pose::offset(0.0, 4.0, 9.0),
    );
    add_paddles(&mut root, (62.0, 0.0), (62.0, 20.0), -5.0);
    if chest {
        add_chest(&mut root, 0.0);
    }
    LayerDefinition::new(root, 128.0, if chest { 128.0 } else { 64.0 })
}

/// `BoatModel.createWaterPatch`: the boat's bottom, but lifted into the
pub fn water_patch() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "water_patch",
        vec![Cube::new(
            (0.0, 0.0),
            (-14.0, -9.0, -3.0),
            (28.0, 16.0, 3.0),
        )],
        at((0.0, -3.0, 1.0), (FRAC_PI_2, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `RaftModel.createRaftModel` and `createChestRaftModel`.
pub fn raft(chest: bool) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "bottom",
        vec![
            Cube::new((0.0, 0.0), (-14.0, -11.0, -4.0), (28.0, 20.0, 4.0)),
            Cube::new((0.0, 0.0), (-14.0, -9.0, -8.0), (28.0, 16.0, 4.0)),
        ],
        at((0.0, -2.1, 1.0), (crate::math::angle::HALF_PI, 0.0, 0.0)),
    );
    add_paddles(&mut root, (0.0, 24.0), (40.0, 24.0), -4.0);
    if chest {
        add_chest(&mut root, -5.1);
    }
    LayerDefinition::new(root, 128.0, if chest { 128.0 } else { 64.0 })
}

/// `MinecartModel.createBodyLayer`.
pub fn minecart() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "bottom",
        vec![Cube::new(
            (0.0, 10.0),
            (-10.0, -8.0, -1.0),
            (20.0, 16.0, 2.0),
        )],
        at((0.0, 4.0, 0.0), (FRAC_PI_2, 0.0, 0.0)),
    );
    let wall = || vec![Cube::new((0.0, 0.0), (-8.0, -9.0, -1.0), (16.0, 8.0, 2.0))];
    root.add_or_replace_child("front", wall(), at((-9.0, 4.0, 0.0), (0.0, PI * 1.5, 0.0)));
    root.add_or_replace_child("back", wall(), at((9.0, 4.0, 0.0), (0.0, FRAC_PI_2, 0.0)));
    root.add_or_replace_child("left", wall(), at((0.0, 4.0, -7.0), (0.0, PI, 0.0)));
    root.add_or_replace_child("right", wall(), Pose::offset(0.0, 4.0, 7.0));
    LayerDefinition::new(root, 64.0, 32.0)
}

/// The texture of a boat type, `entity/boat/<wood>` or `entity/chest_boat/<wood>`,
pub fn boat_look(resource_name: &str) -> Option<(&'static str, bool, bool)> {
    let chest = resource_name.contains("chest");
    let raft = resource_name.ends_with("raft");
    let wood = resource_name
        .trim_end_matches("_boat")
        .trim_end_matches("_raft")
        .trim_end_matches("_chest");
    macro_rules! woods {
        ($($name:literal),*) => {
            match (wood, chest) {
                $(
                    ($name, false) => concat!("minecraft:entity/boat/", $name),
                    ($name, true) => concat!("minecraft:entity/chest_boat/", $name),
                )*
                _ => return None,
            }
        };
    }
    let texture = woods!(
        "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
        "poplar", "bamboo"
    );
    Some((texture, raft, chest))
}

/// The paddles to turn.
#[derive(Clone, Copy, Debug)]
pub struct BoatParts {
    left: PartId,
    right: PartId,
}

impl BoatParts {
    pub fn find(model: &Model) -> Self {
        Self {
            left: model.part("left_paddle"),
            right: model.part("right_paddle"),
        }
    }
}

/// `AbstractBoat.getRowingTime`: where a paddle's clock is, 0 while it is
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rowing {
    pub left: f32,
    pub right: f32,
}

/// The paddle clocks, `AbstractBoat.tick`'s: each runs `PADDLE_STEP` a tick
#[derive(Clone, Copy, Debug, Default)]
pub struct Paddles {
    clock: [f32; 2],
}

impl Paddles {
    pub fn tick(&mut self, in_water: (bool, bool)) {
        for (clock, wet) in self.clock.iter_mut().zip([in_water.0, in_water.1]) {
            *clock = if wet { *clock + PADDLE_STEP } else { 0.0 };
        }
    }

    /// The clocks between ticks, 0 for a paddle that is out of the water.
    pub fn rowing(&self, in_water: (bool, bool), partial: f32) -> Rowing {
        let time = |clock: f32, wet: bool| {
            if wet {
                (clock - PADDLE_STEP) + PADDLE_STEP * partial.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        Rowing {
            left: time(self.clock[0], in_water.0),
            right: time(self.clock[1], in_water.1),
        }
    }
}

/// `AbstractBoatModel.setupAnim`.
pub fn setup_anim(parts: &BoatParts, pose: &mut ModelPose, rowing: Rowing) {
    for (part, time, right) in [
        (parts.left, rowing.left, false),
        (parts.right, rowing.right, true),
    ] {
        let blend = |t: f32| t.clamp(0.0, 1.0);
        let lerp = |t: f32, a: f32, b: f32| a + (b - a) * blend(t);
        let rotation = &mut pose[part].pose.rotation;
        rotation.x = lerp((angle::sin(-time) + 1.0) / 2.0, -THIRD_PI, -PI / 12.0);
        rotation.y = lerp((angle::sin(-time + 1.0) + 1.0) / 2.0, -FRAC_PI_4, FRAC_PI_4);
        if right {
            rotation.y = PI - rotation.y;
        }
    }
}

/// How a hit vehicle shakes: `hurt` ticks left (with the partial tick
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shake {
    pub hurt: f32,
    pub damage: f32,
    pub dir: i32,
}

impl Shake {
    /// `Mth.sin(hurt) * hurt * damage / 10 * dir`, degrees about the X axis.
    pub fn degrees(self) -> f32 {
        if self.hurt <= 0.0 {
            return 0.0;
        }
        angle::sin(self.hurt) * self.hurt * self.damage / 10.0 * self.dir as f32
    }
}

/// `AbstractBoatRenderer.submit`: up 0.375, turned by the yaw (vanilla's
pub fn boat_root(position: Vec3, yaw: f32, shake: Shake) -> Mat4 {
    Mat4::from_translation(position + Vec3::new(0.0, 0.375, 0.0))
        * Mat4::rot_y_deg(180.0 - yaw)
        * Mat4::rot_x_deg(shake.degrees())
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
        * Mat4::rot_y_deg(90.0)
}

/// The block a minecart carries (`getDisplayBlockState`): the synced one,
/// or its type's own; and how high (`getDisplayOffset`, in pixels).
pub fn minecart_block(
    name: &str,
    custom: i32,
    offset: Option<i32>,
    fuel: bool,
) -> Option<(u16, i32)> {
    use pumpkin_data::Block;
    let (default, default_offset) = match name {
        "chest_minecart" => (Some(Block::CHEST.default_state.id.as_u16()), 8),
        "hopper_minecart" => (Some(Block::HOPPER.default_state.id.as_u16()), 1),
        "tnt_minecart" => (Some(Block::TNT.default_state.id.as_u16()), 6),
        "spawner_minecart" => (Some(Block::SPAWNER.default_state.id.as_u16()), 6),
        "command_block_minecart" => (Some(Block::COMMAND_BLOCK.default_state.id.as_u16()), 6),
        // Facing north (the default), lit while it has fuel.
        "furnace_minecart" => {
            let default = Block::FURNACE.default_state.id.as_u16();
            let lit = Block::FURNACE
                .states
                .iter()
                .map(|s| s.id.as_u16())
                .find(|&id| {
                    crate::particle::client::lit(id)
                        && crate::particle::client::prop(id, "facing").as_deref() == Some("north")
                });
            (
                Some(if fuel {
                    lit.unwrap_or(default)
                } else {
                    default
                }),
                6,
            )
        }
        _ => (None, 6),
    };
    let state = u16::try_from(custom).ok().filter(|&s| s != 0).or(default)?;
    Some((state, offset.unwrap_or(default_offset)))
}

/// Where the carried block sits, from the cart's `minecart_root`: unflipped,
/// three quarters big, `offset` pixels up, turned a quarter.
pub fn minecart_block_matrix(root: Mat4, offset: i32, tnt_swell: Option<f32>) -> Mat4 {
    let mut m = root
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
        * Mat4::from_scale(Vec3::splat(0.75))
        * Mat4::from_translation(Vec3::new(-0.5, (offset - 8) as f32 / 16.0, 0.5))
        * Mat4::rot_y_deg(90.0);
    if let Some(swell) = tnt_swell {
        m = m
            * Mat4::from_translation(Vec3::new(-swell * 0.5, 0.0, -swell * 0.5))
            * Mat4::from_scale(Vec3::splat(1.0 + swell));
    }
    m
}

/// `MinecartTNT.handleEntityEvent`: the fuse is lit.
pub const EVENT_PRIME_TNT: u8 = 10;

/// `AbstractMinecartRenderer.oldRender` and its shake: up 0.375, turned by
pub fn minecart_root(position: Vec3, yaw: f32, pitch: f32, shake: Shake) -> Mat4 {
    Mat4::from_translation(position + Vec3::new(0.0, 0.375, 0.0))
        * Mat4::rot_y_deg(180.0 - yaw)
        * Mat4::rot_z_deg(-pitch)
        * Mat4::rot_x_deg(shake.degrees())
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
}

/// `BoatRenderer`, `RaftRenderer`, `MinecartRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Boat => {
            if let Some((texture, raft, chest)) = boat_look(entity.ty.resource_name) {
                let model = match (raft, chest) {
                    (false, false) => M::Boat,
                    (false, true) => M::ChestBoat,
                    (true, false) => M::Raft,
                    (true, true) => M::ChestRaft,
                };
                out.push(plain(model, texture));
                if !entity.in_water {
                    out.push(plain(M::BoatWaterPatch, texture).water_mask());
                }
            }
        }
        EntityKind::Minecart => out.push(plain(M::Minecart, MINECART_TEXTURE)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_boat_type_has_a_texture_and_a_shape() {
        use pumpkin_data::entity::EntityType;
        let names = [
            "oak_boat",
            "dark_oak_chest_boat",
            "bamboo_raft",
            "bamboo_chest_raft",
            "pale_oak_boat",
            "poplar_chest_boat",
        ];
        let looks: Vec<_> = names.iter().map(|n| boat_look(n).unwrap()).collect();
        assert_eq!(looks[0], ("minecraft:entity/boat/oak", false, false));
        assert_eq!(
            looks[1],
            ("minecraft:entity/chest_boat/dark_oak", false, true)
        );
        assert_eq!(looks[2], ("minecraft:entity/boat/bamboo", true, false));
        assert_eq!(looks[3], ("minecraft:entity/chest_boat/bamboo", true, true));
        assert!(boat_look("minecart").is_none());
        for id in 0..400 {
            if let Some(ty) = EntityType::from_raw(id)
                && ty.boat.is_some()
            {
                assert!(
                    boat_look(ty.resource_name).is_some(),
                    "{}",
                    ty.resource_name
                );
            }
        }
    }

    #[test]
    fn a_paddle_rests_out_of_the_water_and_turns_in_it() {
        let mut paddles = Paddles::default();
        let dry = (false, false);
        let left = (true, false);
        paddles.tick(left);
        paddles.tick(left);
        let rowing = paddles.rowing(left, 1.0);
        assert!((rowing.left - 2.0 * PADDLE_STEP).abs() < 1e-6);
        assert_eq!(rowing.right, 0.0);
        assert!((paddles.rowing(left, 0.5).left - 1.5 * PADDLE_STEP).abs() < 1e-6);
        paddles.tick(dry);
        assert_eq!(paddles.rowing(dry, 1.0), Rowing::default());
    }

    #[test]
    fn a_hit_boat_rocks_by_the_damage_and_stops() {
        let still = Shake::default();
        assert_eq!(still.degrees(), 0.0);
        let hit = Shake {
            hurt: 5.0,
            damage: 20.0,
            dir: 1,
        };
        assert!(hit.degrees().abs() > 1.0);
        let other_way = Shake { dir: -1, ..hit };
        assert!((hit.degrees() + other_way.degrees()).abs() < 1e-5);
        assert_eq!(Shake { hurt: 0.0, ..hit }.degrees(), 0.0);
    }

    #[test]
    fn a_boat_sits_three_eighths_up_and_a_cart_too() {
        let boat = boat_root(Vec3::new(1.0, 64.0, 2.0), 0.0, Shake::default());
        let at = boat.transform_point3(Vec3::ZERO);
        assert!((at - Vec3::new(1.0, 64.375, 2.0)).length() < 1e-5);
        let cart = minecart_root(Vec3::new(1.0, 64.0, 2.0), 90.0, 0.0, Shake::default());
        assert!((cart.transform_point3(Vec3::ZERO) - Vec3::new(1.0, 64.375, 2.0)).length() < 1e-5);
    }

    #[test]
    fn the_models_bake_with_their_paddles() {
        for chest in [false, true] {
            for layer in [boat(chest), raft(chest)] {
                let model = Model::bake(&layer);
                let parts = BoatParts::find(&model);
                let mut pose = model.initial_pose();
                setup_anim(&parts, &mut pose, Rowing::default());
            }
        }
        Model::bake(&minecart());
    }
}
