//! Fishing bobbers and their lines (`FishingHookRenderer`).

use std::collections::HashMap;

use glam::{IVec3, Mat4, Vec3};

use super::InWorld;
use super::entities::EntityFrame;
use super::world_resources::WorldResources;
use melon_core::math::ChunkHash;
use melon_render::{EntityDraw, Glint, MeshId, RenderLayer, Renderer, WorldVertex};
use melon_world::entity::display;
use melon_world::entity::fishing::{self, NearPlane};
use melon_world::entity::kind::EntityKind;
use melon_world::entity::player::Arm;
use melon_world::item_model::HeldUse;
use melon_world::player::hands::Hand;
use pumpkin_data::item_stack::ItemStack;

/// `FishingHookRenderer.TEXTURE_LOCATION`.
pub const TEXTURE: &str = "minecraft:entity/fishing/fishing_hook";

struct Kept {
    mesh: Option<MeshId>,
    origin: IVec3,
    /// What the mesh was built from.
    from: (Vec3, Vec3, Vec3, [u8; 2]),
    /// Drawn this frame; the others are freed by [`FishingMeshes::end_frame`].
    seen: bool,
}

#[derive(Default)]
pub struct FishingMeshes {
    sprite: Option<MeshId>,
    lines: HashMap<i32, Kept, ChunkHash>,
}

/// The sprite, half a block across and centred (`vertex(.. x - 0.5, y - 0.5)`).
fn sprite_quads() -> Vec<WorldVertex> {
    let corner = |x: f32, y: f32, u: f32, v: f32| {
        WorldVertex::new([x - 0.5, y - 0.5, 0.0], [u, v], [1.0; 3])
    };
    let [a, b, c, d] = [
        corner(0.0, 0.0, 0.0, 1.0),
        corner(1.0, 0.0, 1.0, 1.0),
        corner(1.0, 1.0, 1.0, 0.0),
        corner(0.0, 1.0, 0.0, 0.0),
    ];
    vec![a, b, c, d, d, c, b, a]
}

impl FishingMeshes {
    fn sprite(&mut self, renderer: &mut Renderer) -> Option<MeshId> {
        if self.sprite.is_none() {
            match renderer.upload_mesh(&sprite_quads()) {
                Ok(mesh) => self.sprite = Some(mesh),
                Err(e) => melon_base::warn!("Bobber mesh failed: {e:#}"),
            }
        }
        self.sprite
    }

    fn line(
        &mut self,
        renderer: &mut Renderer,
        id: i32,
        from: (Vec3, Vec3, Vec3, [u8; 2]),
        white: melon_render::TextureId,
        out: &mut Vec<EntityDraw>,
    ) {
        let kept = self.lines.entry(id).or_insert_with(|| Kept {
            mesh: None,
            origin: IVec3::ZERO,
            from,
            seen: false,
        });
        kept.seen = true;
        if kept.mesh.is_none() || kept.from != from {
            if let Some(old) = kept.mesh.take() {
                renderer.free_mesh(old);
            }
            kept.from = from;
            kept.origin = from.0.floor().as_ivec3();
            let quads = fishing::line_quads(from.0, from.1, from.2, kept.origin.as_vec3(), from.3);
            match renderer.upload_mesh(&quads) {
                Ok(mesh) => kept.mesh = Some(mesh),
                Err(e) => melon_base::warn!("Fishing line mesh failed: {e:#}"),
            }
        }
        if let Some(mesh) = kept.mesh {
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas: white,
                model: Mat4::IDENTITY,
                origin: kept.origin,
                layer: RenderLayer::Entities,
                blend: melon_render::Blending::Opaque,
                light: None,
                overlay: 0.0,
                glint: Glint::None,
            });
        }
    }

    /// Frees the lines not drawn since the last call.
    pub fn end_frame(&mut self, renderer: &mut Renderer) {
        self.lines.retain(|_, kept| {
            let keep = std::mem::take(&mut kept.seen);
            if !keep && let Some(mesh) = kept.mesh.take() {
                renderer.free_mesh(mesh);
            }
            keep
        });
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        for kept in self.lines.drain().map(|(_, k)| k) {
            if let Some(mesh) = kept.mesh {
                renderer.free_mesh(mesh);
            }
        }
        if let Some(mesh) = self.sprite.take() {
            renderer.free_mesh(mesh);
        }
    }
}

/// What the line's far end needs of the camera.
pub(super) struct View<'a> {
    pub frame: EntityFrame<'a>,
    pub plane: NearPlane,
}

impl InWorld {
    /// Makes sure the sprite's texture is there when a bobber is.
    pub(super) fn prepare_fishing(&self, renderer: &mut Renderer, assets: &mut WorldResources) {
        if self
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::FishingBobber)
        {
            assets.mobs.prepare_texture(renderer, TEXTURE);
        }
    }

    /// How the own `stack` in `hand` is used, as its model sees it.
    pub(super) fn held_use(&self, hand: Hand, stack: &ItemStack, using: bool) -> HeldUse {
        if using {
            return HeldUse::Using;
        }
        let rod = |s: &ItemStack| s.item.registry_key == "fishing_rod";
        let fishing = self.entities.is_fishing(self.joined.session.info.entity_id);
        let main_rod = rod(self.hand_stack(Hand::Main));
        if rod(stack) && fishing::rod_looks_cast(fishing, hand == Hand::Main, main_rod) {
            HeldUse::Cast
        } else {
            HeldUse::Idle
        }
    }

    /// The bobbers in `visible` with their lines.
    pub(super) fn fishing_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        visible: &[i32],
        view: &View,
        main_arm: Arm,
        out: &mut Vec<EntityDraw>,
    ) {
        let (partial, params) = (view.frame.partial, view.frame.lightmap);
        let position = view.frame.camera.as_vec3();
        let own = self.joined.session.info.entity_id;
        let first_person = self.camera_mode == super::CameraMode::FirstPerson;
        let white = renderer.white_texture();
        let texture = assets.mobs.texture(TEXTURE);
        let sprite = self.entity_meshes.fishing.sprite(renderer);
        for bobber in visible
            .iter()
            .filter_map(|&id| self.entities.get(id))
            .filter(|e| e.kind == EntityKind::FishingBobber)
        {
            let Some(owner) = bobber.owner else {
                continue;
            };
            let hook = bobber.pos(partial).as_vec3();
            let origin = hook.floor().as_ivec3();
            let light = self.light_at_eye(hook, 0, params);
            let hand = if owner == own {
                let rod_in_main = self.hands.shown(Hand::Main).item.registry_key == "fishing_rod";
                let right = fishing::holds_right(main_arm == Arm::Right, rod_in_main);
                if first_person {
                    let attack = self.hands.swing.animation(partial);
                    fishing::hand_first_person(
                        position,
                        &view.plane,
                        right,
                        fishing::swing_of(attack),
                    )
                } else {
                    self.player.rod_hand(right)
                }
            } else if let Some(player) = self.entities.get(owner) {
                let rod_in_main =
                    player.equipment.hand(Hand::Main).item.registry_key == "fishing_rod";
                let right = fishing::holds_right(
                    player.appearance.main_arm(EntityKind::Player) == Arm::Right,
                    rod_in_main,
                );
                fishing::hand_third_person(
                    player.eye(partial),
                    player.body_yaw(partial),
                    right,
                    player.appearance.crouching(),
                    player.scale(),
                )
            } else {
                continue;
            };
            if let (Some(mesh), Some(texture)) = (sprite, texture) {
                let facing = display::orientation(3, (0.0, 0.0), view.frame.angles);
                out.push(EntityDraw {
                    scroll: None,
                    alpha: 1.0,
                    mesh,
                    atlas: texture,
                    model: Mat4::from_translation(hook - origin.as_vec3())
                        * Mat4::from_quat(facing)
                        * Mat4::from_scale(Vec3::splat(0.5)),
                    origin,
                    layer: RenderLayer::Entities,
                    blend: melon_render::Blending::Opaque,
                    light: Some(light),
                    overlay: 0.0,
                    glint: Glint::None,
                });
            }
            let at = hook.floor().as_ivec3();
            let (block, sky) = self.level.light_at(at.x, at.y, at.z);
            self.entity_meshes.fishing.line(
                renderer,
                bobber.id,
                (hook, hand, position, [block * 16, sky * 16]),
                white,
                out,
            );
        }
    }
}
