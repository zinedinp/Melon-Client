//! Blocks drawn as entities: an enderman's carried one (`CarriedBlockLayer`)

use std::collections::HashMap;

use crate::renderer::{MeshId, Renderer, WorldVertex};
use crate::world::block_models::BlockModels;
use crate::world::block_tint::TintKind;

/// What one block state draws as: its tint, and a mesh per colour of it.
struct Entry {
    kind: TintKind,
    /// `None` for a state with nothing to draw (air, a fluid).
    meshes: Vec<([u8; 3], Option<MeshId>)>,
}

#[derive(Default)]
pub struct BlockMeshes {
    by_state: HashMap<u16, Entry, crate::math::ChunkHash>,
}

fn tint_kind(models: &BlockModels, state: u16) -> TintKind {
    models.tint_kind(state)
}

/// The vertices of block `state` as an entity mesh.
pub fn vertices(models: &BlockModels, state: u16, rgb: [u8; 3]) -> Vec<WorldVertex> {
    let mut out = Vec::new();
    models.quads_at(state, [0, 0, 0], |quads| {
        for quad in quads {
            let tint = if quad.tint == TintKind::None {
                [255; 3]
            } else {
                rgb
            };
            out.extend(models.corners_of(quad).map(|v| WorldVertex {
                light: WorldVertex::SENTINEL_LIGHT,
                color: [
                    mul(v.color[0], tint[0]),
                    mul(v.color[1], tint[1]),
                    mul(v.color[2], tint[2]),
                    v.color[3],
                ],
                ..v
            }));
        }
    });
    // A chest's or banner's own model has no quads: its look is the block
    // entity's (`BuiltInBlockModels`).
    crate::world::block_entity::at_rest(state, models, &mut out);
    out
}

fn mul(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}

impl BlockMeshes {
    /// Makes sure `state` can be drawn by [`Self::mesh`].
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        models: &BlockModels,
        state: u16,
        tint: impl FnOnce(TintKind) -> [u8; 3],
    ) {
        let entry = self.by_state.entry(state).or_insert_with(|| Entry {
            kind: tint_kind(models, state),
            meshes: Vec::new(),
        });
        let rgb = if entry.kind == TintKind::None {
            [255; 3]
        } else {
            tint(entry.kind)
        };
        if entry.meshes.iter().any(|(c, _)| *c == rgb) {
            return;
        }
        let vertices = vertices(models, state, rgb);
        let mesh = if vertices.is_empty() {
            None
        } else {
            renderer
                .upload_mesh(&vertices)
                .map_err(|e| crate::warn!("Block {state} not uploaded: {e:#}"))
                .ok()
        };
        entry.meshes.push((rgb, mesh));
    }

    /// The mesh of `state` as [`Self::prepare`] made it, tinted by `tint`.
    pub fn mesh(&self, state: u16, tint: impl FnOnce(TintKind) -> [u8; 3]) -> Option<MeshId> {
        let entry = self.by_state.get(&state)?;
        let rgb = if entry.kind == TintKind::None {
            [255; 3]
        } else {
            tint(entry.kind)
        };
        entry
            .meshes
            .iter()
            .find(|(c, _)| *c == rgb)
            .and_then(|(_, mesh)| *mesh)
    }

    pub fn release(&mut self, renderer: &mut Renderer) {
        for (_, entry) in self.by_state.drain() {
            for (_, mesh) in entry.meshes {
                if let Some(mesh) = mesh {
                    renderer.free_mesh(mesh);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block_models::BlockModels;

    #[test]
    fn a_full_block_is_six_quads_in_the_unit_cube_lit_like_an_entity() {
        let root = crate::assets::default_root();
        if !root.join("minecraft/blockstates/stone.json").exists() {
            eprintln!("no assets: skipped");
            return;
        }
        let (models, _) = BlockModels::bake(&root, Vec::new(), 4096);
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        let vertices = vertices(&models, stone, [255; 3]);
        assert_eq!(vertices.len(), 24);
        for v in &vertices {
            assert!(
                v.pos.iter().all(|c| (-0.001..=1.001).contains(c)),
                "{:?}",
                v.pos
            );
            assert_eq!(v.light, WorldVertex::SENTINEL_LIGHT);
        }
        assert!(super::vertices(&models, 0, [255; 3]).is_empty());
    }
}

#[cfg(test)]
mod tint_tests {
    use super::*;

    #[test]
    fn leaves_take_the_biome_colour_and_stone_does_not() {
        let root = crate::assets::default_root();
        if !root.join("minecraft/blockstates/stone.json").exists() {
            eprintln!("no assets: skipped");
            return;
        }
        let (models, _) = BlockModels::bake(&root, Vec::new(), 4096);
        let leaves = pumpkin_data::Block::OAK_LEAVES.default_state.id.as_u16();
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        assert_eq!(tint_kind(&models, stone), TintKind::None);
        assert_eq!(tint_kind(&models, leaves), TintKind::Foliage);
        let plain = vertices(&models, leaves, [255; 3]);
        let green = vertices(&models, leaves, [0, 255, 0]);
        assert_eq!(plain.len(), green.len());
        assert!(green.iter().all(|v| v.color[0] == 0));
        assert!(plain.iter().any(|v| v.color[0] > 0));
    }
}
