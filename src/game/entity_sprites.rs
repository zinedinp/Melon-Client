//! What is drawn facing the camera rather than as a model: thrown items
//! (`ThrownItemRenderer`), firework rockets, ominous item spawners,
//! experience orbs and dragon fireballs. The orbs' and fireball's quads
//! are built once and turned by the draw's matrix.

use std::borrow::Cow;

use glam::{Mat4, Quat, Vec3};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use super::entities::{Placed, submit_item, submit_item_cluster};
use super::world_resources::WorldResources;
use crate::math::angle::{self, Rotation};
use crate::renderer::{Blending, EntityDraw, Glint, MeshId, Renderer, WorldVertex};
use crate::world::entity::display;
use crate::world::entity::kind::EntityKind;
use crate::world::entity::mobs::projectile::thrown_look;
use crate::world::entity::tracker::ClientEntity;
use crate::world::item_model::HandPose;

/// `ExperienceOrbRenderer.EXPERIENCE_ORB_LOCATION`.
pub const ORB_TEXTURE: &str = "minecraft:entity/experience/experience_orb";
/// `DragonFireballRenderer.TEXTURE_LOCATION`.
pub const DRAGON_FIREBALL_TEXTURE: &str = "minecraft:entity/enderdragon/dragon_fireball";

/// The orbs' eleven sizes on their sheet, and the fireball's quad.
#[derive(Default)]
pub struct SpriteMeshes {
    orbs: [Option<MeshId>; 11],
    fireball: Option<MeshId>,
    /// Each lightning bolt's shape, by entity, and whether it was drawn
    /// this frame.
    bolts: std::collections::HashMap<i32, (Option<MeshId>, bool)>,
}

/// One camera-facing quad from `x0..x0 + 1` and `y0..y0 + 1` with the
/// texture's `u`/`v` rectangle, both sides.
fn quad(x0: f32, y0: f32, [u0, v0, u1, v1]: [f32; 4]) -> Vec<WorldVertex> {
    let corner = |x: f32, y: f32, u: f32, v: f32| WorldVertex::new([x, y, 0.0], [u, v], [1.0; 3]);
    let [a, b, c, d] = [
        corner(x0, y0, u0, v1),
        corner(x0 + 1.0, y0, u1, v1),
        corner(x0 + 1.0, y0 + 1.0, u1, v0),
        corner(x0, y0 + 1.0, u0, v0),
    ];
    vec![a, b, c, d, d, c, b, a]
}

/// `ExperienceOrbRenderer.submit`'s texture rectangle for `icon`.
fn orb_uv(icon: usize) -> [f32; 4] {
    let (col, row) = ((icon % 4) as f32, (icon / 4) as f32);
    [
        col * 16.0 / 64.0,
        row * 16.0 / 64.0,
        (col * 16.0 + 16.0) / 64.0,
        (row * 16.0 + 16.0) / 64.0,
    ]
}

/// `ExperienceOrb.getIcon`: its picture by how much it is worth.
pub fn orb_icon(value: i32) -> usize {
    const STEPS: [i32; 10] = [3, 7, 17, 37, 73, 149, 307, 617, 1237, 2477];
    STEPS.iter().filter(|&&step| value >= step).count()
}

/// `ExperienceOrbRenderer`'s shimmer: red and blue swing, green stays.
pub fn orb_tint(age_in_ticks: f32) -> [f32; 3] {
    let rr = age_in_ticks / 2.0;
    let r = ((angle::sin(rr) + 1.0) * 0.5 * 255.0) as i32;
    let b = ((angle::sin(rr + std::f32::consts::PI * 4.0 / 3.0) + 1.0) * 0.1 * 255.0) as i32;
    [r as f32 / 255.0, 1.0, b as f32 / 255.0]
}

impl SpriteMeshes {
    fn upload(slot: &mut Option<MeshId>, renderer: &mut Renderer, quads: Vec<WorldVertex>) {
        if slot.is_none() {
            match renderer.upload_mesh(&quads) {
                Ok(mesh) => *slot = Some(mesh),
                Err(e) => eprintln!("Sprite mesh failed: {e:#}"),
            }
        }
    }

    fn orb(&mut self, renderer: &mut Renderer, icon: usize) -> Option<MeshId> {
        let icon = icon.min(10);
        Self::upload(
            &mut self.orbs[icon],
            renderer,
            quad(-0.5, -0.25, orb_uv(icon)),
        );
        self.orbs[icon]
    }

    fn fireball(&mut self, renderer: &mut Renderer) -> Option<MeshId> {
        Self::upload(
            &mut self.fireball,
            renderer,
            quad(-0.5, -0.25, [0.0, 0.0, 1.0, 1.0]),
        );
        self.fireball
    }

    /// A bolt's mesh: its shape comes from a seed of its own (vanilla's
    /// is the client's random too), here its UUID.
    fn bolt(&mut self, renderer: &mut Renderer, entity: &ClientEntity) -> Option<MeshId> {
        let (mesh, seen) = self.bolts.entry(entity.id).or_insert_with(|| {
            let seed = entity.uuid.as_u64_pair().1 as i64;
            let quads = crate::world::entity::lightning::quads(seed);
            let mesh = renderer
                .upload_mesh(&quads)
                .map_err(|e| eprintln!("Lightning mesh failed: {e:#}"))
                .ok();
            (mesh, false)
        });
        *seen = true;
        *mesh
    }

    /// Frees the bolts not drawn since the last call.
    pub fn end_frame(&mut self, renderer: &mut Renderer) {
        self.bolts.retain(|_, (mesh, seen)| {
            let keep = std::mem::take(seen);
            if !keep && let Some(mesh) = mesh.take() {
                renderer.free_mesh(mesh);
            }
            keep
        });
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        for mesh in self.orbs.iter_mut().chain([&mut self.fireball]) {
            if let Some(mesh) = mesh.take() {
                renderer.free_mesh(mesh);
            }
        }
        for (_, (mesh, _)) in self.bolts.drain() {
            if let Some(mesh) = mesh {
                renderer.free_mesh(mesh);
            }
        }
    }
}

/// `getDefaultItem` / `getItem`'s fallback: what a projectile shows when
/// the server never sent a stack (the default is not synced).
fn default_item(ty: &EntityType) -> Option<&'static Item> {
    let by_type: [(&EntityType, &'static Item); 10] = [
        (&EntityType::SNOWBALL, &Item::SNOWBALL),
        (&EntityType::EGG, &Item::EGG),
        (&EntityType::ENDER_PEARL, &Item::ENDER_PEARL),
        (&EntityType::EXPERIENCE_BOTTLE, &Item::EXPERIENCE_BOTTLE),
        (&EntityType::SPLASH_POTION, &Item::SPLASH_POTION),
        (&EntityType::LINGERING_POTION, &Item::LINGERING_POTION),
        (&EntityType::EYE_OF_ENDER, &Item::ENDER_EYE),
        (&EntityType::FIREBALL, &Item::FIRE_CHARGE),
        (&EntityType::SMALL_FIREBALL, &Item::FIRE_CHARGE),
        (&EntityType::FIREWORK_ROCKET, &Item::FIREWORK_ROCKET),
    ];
    by_type
        .into_iter()
        .find(|(t, _)| t.id == ty.id)
        .map(|(_, item)| item)
}

/// The stack a thrown item, rocket or spawner shows.
pub fn shown_item(entity: &ClientEntity) -> Option<Cow<'_, ItemStack>> {
    if !matches!(
        entity.kind,
        EntityKind::ThrownItem | EntityKind::FireworkRocket | EntityKind::OminousItemSpawner
    ) {
        return None;
    }
    if !entity.item.is_empty() {
        return Some(Cow::Borrowed(&entity.item));
    }
    default_item(entity.ty).map(|item| Cow::Owned(ItemStack::new(1, item)))
}

/// The camera's turn (`CameraRenderState.orientation`).
fn facing(view: (f32, f32)) -> Mat4 {
    Mat4::from_quat(display::orientation(3, (0.0, 0.0), view))
}

/// The draws of a sprite-like entity at `feet` (relative to the draw's
/// block): `view` is the camera's yaw and pitch.
#[allow(clippy::too_many_arguments)]
pub fn submit(
    sprites: &mut SpriteMeshes,
    renderer: &mut Renderer,
    assets: &WorldResources,
    entity: &ClientEntity,
    feet: Vec3,
    placed: Placed,
    partial: f32,
    view: (f32, f32),
    out: &mut Vec<EntityDraw>,
) {
    let at = Mat4::from_translation(feet);
    let age = entity.age_in_ticks(partial);
    match entity.kind {
        EntityKind::ThrownItem => {
            let Some(stack) = shown_item(entity) else {
                return;
            };
            let (scale, _) = thrown_look(entity.ty);
            let m = at * Mat4::from_scale(Vec3::splat(scale)) * facing(view);
            submit_item(assets, &stack, HandPose::Ground, false, m, placed, out);
        }
        EntityKind::FireworkRocket => {
            let Some(stack) = shown_item(entity) else {
                return;
            };
            let mut m = at * facing(view);
            if entity.appearance.thing.shot_at_angle {
                m = m * Mat4::rot_z_deg(180.0) * Mat4::rot_y_deg(180.0) * Mat4::rot_x_deg(90.0);
            }
            submit_item(assets, &stack, HandPose::Ground, false, m, placed, out);
        }
        EntityKind::OminousItemSpawner => {
            let Some(stack) = shown_item(entity) else {
                return;
            };
            let mut m = at;
            if age <= 50.0 {
                m *= Mat4::from_scale(Vec3::splat(age.min(50.0) / 50.0));
            }
            let spin = angle::wrap_degrees(age * 40.0);
            m *= Mat4::from_quat(Quat::from_rotation_y(spin.to_radians()));
            // `submitMultipleFromCount` at full light.
            let lit = Placed {
                light: [1.0; 3],
                ..placed
            };
            submit_item_cluster(assets, &stack, m, lit, out);
        }
        EntityKind::ExperienceOrb => {
            let (Some(mesh), Some(texture)) = (
                sprites.orb(renderer, orb_icon(entity.appearance.thing.xp_value)),
                assets.mobs.texture(ORB_TEXTURE),
            ) else {
                return;
            };
            let tint = orb_tint(age);
            out.push(EntityDraw {
                scroll: None,
                alpha: 128.0 / 255.0,
                mesh,
                atlas: texture,
                model: at
                    * Mat4::from_translation(Vec3::new(0.0, 0.1, 0.0))
                    * facing(view)
                    * Mat4::from_scale(Vec3::splat(0.3)),
                origin: placed.origin,
                layer: placed.layer,
                blend: Blending::Translucent,
                light: Some(std::array::from_fn(|i| placed.light[i] * tint[i])),
                overlay: 0.0,
                glint: Glint::None,
            });
        }
        EntityKind::LightningBolt => {
            let Some(mesh) = sprites.bolt(renderer, entity) else {
                return;
            };
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas: renderer.white_texture(),
                model: at,
                origin: placed.origin,
                layer: placed.layer,
                blend: Blending::Additive,
                light: Some([1.0; 3]),
                overlay: 0.0,
                glint: Glint::None,
            });
        }
        EntityKind::DragonFireball => {
            let (Some(mesh), Some(texture)) = (
                sprites.fireball(renderer),
                assets.mobs.texture(DRAGON_FIREBALL_TEXTURE),
            ) else {
                return;
            };
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas: texture,
                model: at * Mat4::from_scale(Vec3::splat(2.0)) * facing(view),
                origin: placed.origin,
                layer: placed.layer,
                blend: Blending::Opaque,
                light: Some(placed.light),
                overlay: 0.0,
                glint: Glint::None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbs_grow_with_their_worth() {
        assert_eq!(orb_icon(1), 0);
        assert_eq!(orb_icon(3), 1);
        assert_eq!(orb_icon(16), 2);
        assert_eq!(orb_icon(17), 3);
        assert_eq!(orb_icon(2477), 10);
        assert_eq!(orb_icon(i32::MAX), 10);
    }

    #[test]
    fn the_last_orb_sits_on_the_sheets_third_row() {
        assert_eq!(orb_uv(10), [0.5, 0.5, 0.75, 0.75]);
    }

    #[test]
    fn a_snowball_without_data_shows_a_snowball() {
        assert_eq!(
            default_item(&EntityType::SNOWBALL).unwrap().id,
            Item::SNOWBALL.id
        );
        assert!(default_item(&EntityType::OMINOUS_ITEM_SPAWNER).is_none());
    }
}
