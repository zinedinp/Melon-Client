//! The end crystals' beams on the GPU (`EnderDragonRenderer.submitCrystalBeams`):
//! rebuilt each frame, since the texture scrolls and either end may move.

use rustc_hash::FxHashMap;

use glam::{DVec3, Mat4};

use super::world_resources::WorldResources;
use crate::renderer::{Blending, EntityDraw, Glint, MeshId, RenderLayer, Renderer};
use crate::world::entity::crystal_beam;

/// Last frame's beam of each entity casting one, and whether it was drawn
/// this frame.
#[derive(Default)]
pub struct BeamMeshes {
    by_entity: FxHashMap<i32, (Option<MeshId>, bool)>,
}

impl BeamMeshes {
    /// The beam `entity` casts from `from` toward `delta` (in blocks), lit
    /// with the caster's light.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        entity: i32,
        from: DVec3,
        delta: glam::Vec3,
        time_in_ticks: f32,
        light: [f32; 3],
        out: &mut Vec<EntityDraw>,
    ) {
        let Some(texture) = assets.mobs.texture(crystal_beam::TEXTURE) else {
            return;
        };
        let (mesh, seen) = self.by_entity.entry(entity).or_insert((None, false));
        *seen = true;
        if let Some(old) = mesh.take() {
            renderer.free_mesh(old);
        }
        match renderer.upload_mesh(&crystal_beam::vertices(delta, time_in_ticks)) {
            Ok(new) => *mesh = Some(new),
            Err(e) => {
                crate::warn!("Crystal beam mesh failed: {e:#}");
                return;
            }
        }
        let origin = from.floor().as_ivec3();
        out.push(EntityDraw {
            scroll: None,
            mesh: mesh.expect("just uploaded"),
            atlas: texture,
            model: Mat4::from_translation((from - origin.as_dvec3()).as_vec3()),
            origin,
            layer: RenderLayer::Entities,
            blend: Blending::Opaque,
            light: Some(light),
            overlay: 0.0,
            glint: Glint::None,
            alpha: 1.0,
        });
    }

    /// Frees the beams not drawn since the last call.
    pub fn end_frame(&mut self, renderer: &mut Renderer) {
        self.by_entity.retain(|_, (mesh, seen)| {
            let keep = std::mem::take(seen);
            if !keep && let Some(mesh) = mesh.take() {
                renderer.free_mesh(mesh);
            }
            keep
        });
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        for (mesh, _) in self.by_entity.drain().map(|(_, v)| v) {
            if let Some(mesh) = mesh {
                renderer.free_mesh(mesh);
            }
        }
    }
}
