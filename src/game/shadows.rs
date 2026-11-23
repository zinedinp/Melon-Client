//! Blob shadows under the player and every drawn entity.

use rustc_hash::FxHashMap;

use glam::{IVec3, Mat4, Vec3};

use crate::renderer::{EntityDraw, MeshId, RenderLayer, Renderer, TextureId, WorldVertex};
use crate::world::level::Level;
use crate::world::shadow;

/// The local player's key among the casters (entity ids are the others).
pub const OWN: i32 = i32::MIN;

struct Shadow {
    mesh: Option<MeshId>,
    /// The block the mesh is relative to (`EntityDraw::origin`).
    origin: IVec3,
    /// In the world, to tell when the shadow changed.
    vertices: Vec<WorldVertex>,
    /// Drawn this frame; the others are freed by [`ShadowMeshes::end_frame`].
    seen: bool,
}

#[derive(Default)]
pub struct ShadowMeshes {
    by_caster: FxHashMap<i32, Shadow>,
}

/// What the frame's shadows share: the ground they fall on, its light, the
/// texture and the camera.
#[derive(Clone, Copy)]
pub struct ShadowScene<'a> {
    pub level: &'a Level,
    pub light: shadow::ShadowLight,
    pub texture: TextureId,
    pub camera: Vec3,
}

impl ShadowMeshes {
    /// The shadow of `caster` standing at `feet`, `radius` blocks wide,
    /// `strength` as dark as a full one (`getShadowStrength`).
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        scene: ShadowScene<'_>,
        caster: i32,
        feet: Vec3,
        (radius, strength): (f32, f32),
        out: &mut Vec<EntityDraw>,
    ) {
        if radius <= 0.0 || strength <= 0.0 {
            return;
        }
        let shadow = self.by_caster.entry(caster).or_insert_with(|| Shadow {
            mesh: None,
            origin: IVec3::ZERO,
            vertices: Vec::new(),
            seen: false,
        });
        shadow.seen = true;
        let vertices = shadow::quads(scene.level, feet, scene.camera, radius, strength, scene.light);
        if vertices != shadow.vertices {
            if let Some(old) = shadow.mesh.take() {
                renderer.free_mesh(old);
            }
            shadow.vertices = vertices;
            shadow.origin = feet.floor().as_ivec3();
            let origin = shadow.origin.as_vec3();
            let relative: Vec<_> = shadow
                .vertices
                .iter()
                .map(|v| WorldVertex {
                    pos: (Vec3::from_array(v.pos) - origin).to_array(),
                    ..*v
                })
                .collect();
            if !relative.is_empty() {
                match renderer.upload_mesh(&relative) {
                    Ok(mesh) => shadow.mesh = Some(mesh),
                    Err(e) => crate::warn!("Shadow mesh failed: {e:#}"),
                }
            }
        }
        if let Some(mesh) = shadow.mesh {
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas: scene.texture,
                model: Mat4::IDENTITY,
                origin: shadow.origin,
                layer: RenderLayer::Shadows,
                blend: crate::renderer::Blending::Opaque,
                light: None,
                overlay: 0.0,
                glint: crate::renderer::Glint::None,
            });
        }
    }

    /// Frees the shadows of casters not drawn since the last call.
    pub fn end_frame(&mut self, renderer: &mut Renderer) {
        self.by_caster.retain(|_, shadow| {
            let keep = std::mem::take(&mut shadow.seen);
            if !keep && let Some(mesh) = shadow.mesh.take() {
                renderer.free_mesh(mesh);
            }
            keep
        });
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        for shadow in self.by_caster.drain().map(|(_, s)| s) {
            if let Some(mesh) = shadow.mesh {
                renderer.free_mesh(mesh);
            }
        }
    }
}
