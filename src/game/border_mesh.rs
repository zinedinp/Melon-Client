//! The world border's wall (`WorldBorderRenderer`).

use glam::{DVec3, Mat4};

use melon_base::gfx::TextureId;
use melon_render::{EntityDraw, MeshId, RenderLayer, Renderer};
use melon_world::border::WorldBorder;

/// How far above and below the camera the wall reaches.
const HEIGHT: f64 = 256.0;
/// The texture slides once around in this many milliseconds.
const SCROLL_MILLIS: u128 = 3000;

#[derive(Default)]
pub struct BorderMesh {
    mesh: Option<MeshId>,
}

impl BorderMesh {
    /// The wall as seen from `camera`, `range` blocks around it, into `out`.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        border: &WorldBorder,
        camera: DVec3,
        range: f64,
        texture: TextureId,
        out: &mut Vec<EntityDraw>,
    ) {
        if let Some(old) = self.mesh.take() {
            renderer.free_mesh(old);
        }
        let origin = camera.floor().as_ivec3();
        let scroll = scroll_now();
        let vertices = border.wall(camera, origin, range, HEIGHT, scroll);
        if vertices.is_empty() {
            return;
        }
        match renderer.upload_mesh(&vertices) {
            Ok(mesh) => {
                self.mesh = Some(mesh);
                out.push(EntityDraw {
                    scroll: None,
                    alpha: 1.0,
                    mesh,
                    atlas: texture,
                    model: Mat4::IDENTITY,
                    origin,
                    layer: RenderLayer::Shadows,
                    blend: melon_render::Blending::Opaque,
                    light: None,
                    overlay: 0.0,
                    glint: melon_render::Glint::None,
                });
            }
            Err(e) => melon_base::warn!("World border mesh failed: {e:#}"),
        }
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        if let Some(mesh) = self.mesh.take() {
            renderer.free_mesh(mesh);
        }
    }
}

/// `Util.getMillis() % 3000 / 3000`.
fn scroll_now() -> f32 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    (millis % SCROLL_MILLIS) as f32 / SCROLL_MILLIS as f32
}
