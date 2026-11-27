//! Cracks on blocks being broken: vanilla `LevelRenderer.destroyBlockProgress`

use rustc_hash::FxHashMap;

use melon_render::{EntityDraw, MeshId, RenderLayer, Renderer, TextureId, WorldVertex};
use melon_world::block_models::BlockModels;
use melon_world::level::Level;

/// `LevelRenderer`: a breaker not heard of for this many ticks is dropped.
const EXPIRY_TICKS: u64 = 400;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Crack {
    pos: [i32; 3],
    stage: u8,
    updated: u64,
}

/// One block's overlay on the GPU, and what it was built for.
struct Overlay {
    pos: [i32; 3],
    stage: u8,
    state: u16,
    mesh: MeshId,
}

#[derive(Default)]
pub struct BlockCracks {
    /// By breaker's entity id.
    cracks: FxHashMap<i32, Crack>,
    overlays: Vec<Overlay>,
}

impl BlockCracks {
    /// `destroyBlockProgress`.
    pub fn set(&mut self, breaker: i32, pos: [i32; 3], stage: i32, tick: u64) {
        match u8::try_from(stage).ok().filter(|stage| *stage < 10) {
            Some(stage) => {
                self.cracks.insert(
                    breaker,
                    Crack {
                        pos,
                        stage,
                        updated: tick,
                    },
                );
            }
            None => {
                self.cracks.remove(&breaker);
            }
        }
    }

    /// Drops cracks nobody has updated in a long while.
    pub fn tick(&mut self, tick: u64) {
        self.cracks
            .retain(|_, crack| tick.saturating_sub(crack.updated) <= EXPIRY_TICKS);
    }

    /// Forgets everything, for a new level.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        self.cracks.clear();
        for overlay in self.overlays.drain(..) {
            renderer.free_mesh(overlay.mesh);
        }
    }

    /// The overlays to draw.
    pub fn draws(
        &mut self,
        renderer: &mut Renderer,
        level: &Level,
        models: &BlockModels,
        stages: &[TextureId; 10],
        out: &mut Vec<EntityDraw>,
    ) {
        let mut wanted: Vec<([i32; 3], u8)> = Vec::new();
        for crack in self.cracks.values() {
            match wanted.iter_mut().find(|(pos, _)| *pos == crack.pos) {
                Some((_, stage)) => *stage = (*stage).max(crack.stage),
                None => wanted.push((crack.pos, crack.stage)),
            }
        }
        let state_at = |[x, y, z]: [i32; 3]| level.block_state(x, y, z).unwrap_or(0);
        let same = wanted.len() == self.overlays.len()
            && wanted.iter().all(|&(pos, stage)| {
                self.overlays.iter().any(|overlay| {
                    overlay.pos == pos && overlay.stage == stage && overlay.state == state_at(pos)
                })
            });
        if !same {
            for overlay in self.overlays.drain(..) {
                renderer.free_mesh(overlay.mesh);
            }
            for (pos, stage) in wanted {
                let state = state_at(pos);
                let vertices = crack_vertices(models, state, pos);
                if vertices.is_empty() {
                    continue;
                }
                match renderer.upload_mesh(&vertices) {
                    Ok(mesh) => self.overlays.push(Overlay {
                        pos,
                        stage,
                        state,
                        mesh,
                    }),
                    Err(e) => melon_base::warn!("Block cracks not uploaded: {e:#}"),
                }
            }
        }
        out.extend(self.overlays.iter().map(|overlay| EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh: overlay.mesh,
            atlas: stages[usize::from(overlay.stage)],
            model: glam::Mat4::IDENTITY,
            origin: glam::IVec3::from_array(overlay.pos),
            layer: RenderLayer::Crumbling,
            blend: melon_render::Blending::Translucent,
            light: None,
            overlay: 0.0,
            glint: melon_render::Glint::None,
        }));
    }
}

/// The faces of a block of `state` at `pos`, each textured with the whole crack texture.
fn crack_vertices(models: &BlockModels, state: u16, pos: [i32; 3]) -> Vec<WorldVertex> {
    let mut out = Vec::new();
    models.quads_at(state, pos, |quads| {
        for quad in quads {
            for vertex in *models.corners_of(quad) {
                let local = glam::Vec3::from_array(vertex.pos);
                let [x, y, z] = local.to_array();
                let uv = match vertex.face {
                    0 | 1 => [x, z],
                    2 | 3 => [x, 1.0 - y],
                    _ => [z, 1.0 - y],
                };
                out.push(WorldVertex {
                    pos: local.to_array(),
                    uv,
                    color: [255; 4],
                    light: [240, 240],
                    ..vertex
                });
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_breaker_stops_and_old_cracks_fade() {
        let mut cracks = BlockCracks::default();
        cracks.set(1, [0, 64, 0], 3, 10);
        cracks.set(2, [5, 64, 0], 9, 10);
        assert_eq!(cracks.cracks.len(), 2);
        cracks.set(1, [0, 64, 0], -1, 11);
        assert_eq!(cracks.cracks.len(), 1);
        cracks.set(3, [1, 64, 0], 10, 12);
        assert_eq!(cracks.cracks.len(), 1);
        cracks.tick(10 + EXPIRY_TICKS + 1);
        assert!(cracks.cracks.is_empty());
    }
}
