//! Painting meshes (`PaintingRenderer`): built per painting from its
//! variant, facing and the light of each block it covers, and rebuilt
//! only when one of those changes.

use std::collections::HashMap;

use glam::{DVec3, IVec3, Mat4};

use super::world_resources::WorldResources;
use crate::math::ChunkHash;
use crate::renderer::{Blending, EntityDraw, Glint, MeshId, RenderLayer, Renderer};
use crate::world::entity::painting;
use crate::world::entity::tracker::ClientEntity;
use crate::world::level::Level;

#[derive(PartialEq)]
struct Key {
    variant: i32,
    direction: i32,
    center: DVec3,
    lights: Vec<[u8; 2]>,
}

struct Kept {
    key: Key,
    origin: IVec3,
    front: Option<MeshId>,
    back: Option<MeshId>,
    seen: bool,
}

#[derive(Default)]
pub struct PaintingMeshes {
    by_entity: HashMap<i32, Kept, ChunkHash>,
}

fn upload(renderer: &mut Renderer, vertices: &[crate::renderer::WorldVertex]) -> Option<MeshId> {
    if vertices.is_empty() {
        return None;
    }
    renderer
        .upload_mesh(vertices)
        .map_err(|e| eprintln!("Painting mesh failed: {e:#}"))
        .ok()
}

fn free(renderer: &mut Renderer, kept: &mut Kept) {
    for mesh in [kept.front.take(), kept.back.take()].into_iter().flatten() {
        renderer.free_mesh(mesh);
    }
}

impl PaintingMeshes {
    /// The draws of `entity`, a painting.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        level: &Level,
        entity: &ClientEntity,
        out: &mut Vec<EntityDraw>,
    ) {
        let thing = &entity.appearance.thing;
        let Some(variant) = painting::variant(thing.variant) else {
            return;
        };
        let center = entity.pos(1.0);
        let lights = painting::light_blocks(variant, thing.direction, center)
            .into_iter()
            .map(|at| {
                let (block, sky) = level.light_at(at.x, at.y, at.z);
                [block, sky]
            })
            .collect();
        let key = Key {
            variant: thing.variant,
            direction: thing.direction,
            center,
            lights,
        };
        let kept = self.by_entity.entry(entity.id).or_insert_with(|| Kept {
            key: Key {
                variant: -1,
                direction: -1,
                center: DVec3::NAN,
                lights: Vec::new(),
            },
            origin: IVec3::ZERO,
            front: None,
            back: None,
            seen: false,
        });
        kept.seen = true;
        if kept.key != key {
            free(renderer, kept);
            kept.origin = center.floor().as_ivec3();
            if let Some((front, back)) =
                painting::quads(variant, key.direction, center, kept.origin, &key.lights)
            {
                kept.front = upload(renderer, &front);
                kept.back = upload(renderer, &back);
            }
            kept.key = key;
        }
        let front_texture = assets.mobs.texture(&painting::front_texture(variant));
        let back_texture = assets.mobs.texture(painting::BACK_TEXTURE);
        for (mesh, texture) in [(kept.front, front_texture), (kept.back, back_texture)] {
            let (Some(mesh), Some(atlas)) = (mesh, texture) else {
                continue;
            };
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas,
                model: Mat4::IDENTITY,
                origin: kept.origin,
                layer: RenderLayer::Entities,
                blend: Blending::Opaque,
                light: None,
                overlay: 0.0,
                glint: Glint::None,
            });
        }
    }

    /// Frees the meshes of paintings not drawn since the last call.
    pub fn end_frame(&mut self, renderer: &mut Renderer) {
        self.by_entity.retain(|_, kept| {
            let keep = std::mem::take(&mut kept.seen);
            if !keep {
                free(renderer, kept);
            }
            keep
        });
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        for (_, mut kept) in self.by_entity.drain() {
            free(renderer, &mut kept);
        }
    }
}
