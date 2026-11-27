//! The particles' GPU side: their quads uploaded each frame, item
//! particles' textures.

use std::collections::HashMap;
use std::path::Path;

use melon_world::particle::Particle;
use melon_world::particle::base::Atlas;
use melon_world::particle::mesh::{Batch, ParticleQuad, View, build};
use melon_render::{ParticleDraw, ParticleMeshId, Renderer, TextureId};
use melon_world::level::Level;

/// The particles' GPU side.
#[derive(Default)]
pub struct ParticleMeshes {
    meshes: Vec<ParticleMeshId>,
    /// By item id; `None` for one without a texture.
    items: HashMap<u16, Option<TextureId>>,
    batches: HashMap<(Atlas, bool), Batch>,
    /// The axes' F16 bits on their way into the quads.
    bits: Vec<u16>,
    /// Quads drawn last frame.
    pub drawn: usize,
}

impl ParticleMeshes {
    /// Uploads this frame's quads and adds their draws to `out`. Item textures
    /// load from `assets_root`; block particles use the `blocks` atlas.
    pub fn draws(
        &mut self,
        renderer: &mut Renderer,
        (assets_root, blocks): (&Path, TextureId),
        particles: &[Particle],
        level: &Level,
        view: &View,
        out: &mut Vec<ParticleDraw>,
    ) {
        for mesh in self.meshes.drain(..) {
            renderer.free_particles(mesh);
        }
        for batch in self.batches.values_mut() {
            batch.clear();
        }
        build(particles, level, view, &mut self.batches);
        self.drawn = 0;
        for (&(atlas, translucent), batch) in &mut self.batches {
            if batch.quads.is_empty() {
                continue;
            }
            let texture = match atlas {
                Atlas::Blocks => Some(blocks),
                Atlas::Item(item) => *self
                    .items
                    .entry(item)
                    .or_insert_with(|| item_texture(renderer, assets_root, item)),
            };
            let Some(texture) = texture else {
                continue;
            };
            ParticleQuad::encode_axes(&mut batch.quads, &batch.axes, &mut self.bits);
            match renderer.upload_particles(&batch.quads) {
                Ok(mesh) => {
                    self.meshes.push(mesh);
                    self.drawn += batch.quads.len();
                    out.push(ParticleDraw {
                        mesh,
                        atlas: texture,
                        origin: view.origin,
                        translucent,
                    });
                }
                Err(e) => melon_base::warn!("Particle mesh failed: {e:#}"),
            }
        }
        self.batches.retain(|_, batch| !batch.quads.is_empty());
    }

    /// Frees everything, for new resources or a new level.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for mesh in self.meshes.drain(..) {
            renderer.free_particles(mesh);
        }
        for texture in self.items.drain().filter_map(|(_, t)| t) {
            renderer.free_texture(texture);
        }
        self.batches.clear();
    }
}

/// An item's particle texture (its model's `particle`, else `layer0`).
fn item_texture(renderer: &mut Renderer, assets_root: &Path, item: u16) -> Option<TextureId> {
    let key = pumpkin_data::item::Item::from_id(item)?.registry_key;
    let model = melon_world::item_model::ItemModel::resolve(assets_root, key, None)?;
    let id = model
        .model
        .texture("particle")
        .or_else(|| model.model.texture("layer0"))?;
    let mut image = melon_assets::assets::texture_image(assets_root, &id)?;
    if image.height() > image.width() {
        let side = image.width();
        image = image::imageops::crop_imm(&image, 0, 0, side, side).to_image();
    }
    renderer.load_texture(&image).ok()
}
