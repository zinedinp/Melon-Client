//! Items in hand on the GPU: each item's [`HeldItem`] parts as meshes.
//! Parts sample the item or the block atlas, built with the resources;
//! only what neither holds (a downloaded skin, a composed banner) gets a
//! texture of its own, so a pack's custom items don't add textures while
//! playing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;

use image::RgbaImage;

use crate::renderer::{MeshId, Renderer, TextureId};
use crate::world::block_models::BlockModels;
use crate::world::head_profile::HeadProfile;
use crate::world::item_model::{
    self, Count, HandPose, HeldAnimation, HeldItem, HeldUse, ItemKey, ItemModel, Sheet,
};
use crate::world::transparency::AlphaMap;

/// The atlases held items sample, made with the resources.
pub struct Sheets {
    pub items: TextureId,
    /// The item atlas's sprites and their texels' transparency, by
    /// `namespace:path`.
    pub item_sprites: Arc<HashMap<String, ([f32; 4], AlphaMap)>>,
    pub blocks: TextureId,
    pub models: Arc<BlockModels>,
    /// The atlases' widths in texels, for the glint's scale.
    pub item_side: u32,
    pub block_side: u32,
}

impl Sheets {
    /// Where texture `id` is: the item atlas first (vanilla's item models
    /// use it), then the block atlas.
    fn find(&self, id: &str) -> Option<(Sheet, [f32; 4], AlphaMap)> {
        let canonical = crate::world::item_atlas::canonical(id);
        if let Some((rect, alpha)) = self.item_sprites.get(&canonical) {
            return Some((Sheet::Items, *rect, alpha.clone()));
        }
        let (namespace, path) = crate::assets::split_id(id);
        let block = self.models.sprite_info(id).or_else(|| {
            let other = if id.contains(':') {
                (namespace == "minecraft").then(|| path.to_owned())
            } else {
                Some(canonical)
            };
            self.models.sprite_info(&other?)
        });
        block.map(|(rect, alpha)| (Sheet::Blocks, rect, alpha))
    }
}

/// One uploaded part: its meshes by [`HandPose`] and the texture they sample.
struct Part {
    texture: TextureId,
    translucent: bool,
    meshes: [MeshId; 10],
}

/// One item's uploaded parts.
struct Uploaded {
    parts: Vec<Part>,
    /// The item's own texture, when some part samples it.
    own: Option<TextureId>,
    /// The ground mesh's lowest point and its depth (`getModelBoundingBox`
    ground: (f32, f32),
    /// Animated own texture: the frames and the one showing. (Animated
    /// atlas sprites move with their atlas.)
    animation: Option<(HeldAnimation, usize)>,
}

pub struct HeldItems {
    assets_root: PathBuf,
    sheets: Sheets,
    /// `None` for an item with nothing to draw in hand; by item and
    items: HashMap<(ItemKey, HeldUse), Option<Uploaded>>,
    /// Blocks drawn by a mob (a mooshroom's mushrooms), by model id.
    blocks: HashMap<&'static str, Option<Uploaded>>,
    /// Player heads with a profile, by [`HeadProfile::skin_key`] and use:
    heads: HashMap<(uuid::Uuid, bool), (bool, Option<Uploaded>)>,
}

impl HeldItems {
    pub fn new(assets_root: &Path, sheets: Sheets) -> Self {
        Self {
            assets_root: assets_root.to_path_buf(),
            sheets,
            items: HashMap::new(),
            blocks: HashMap::new(),
            heads: HashMap::new(),
        }
    }

    /// Makes sure `stack` can be drawn in hand, in use or not, building it
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        stack: &ItemStack,
        using: impl Into<HeldUse>,
    ) {
        // Most hands are empty: no key for them (it takes a lock and a
        // hash of the item's name).
        if stack.is_empty() {
            return;
        }
        let key = (ItemKey::of(&self.assets_root, stack), using.into());
        if self.items.contains_key(&key) {
            return;
        }
        let name = stack.item.registry_key;
        let held = ItemModel::held_poses(
            &self.assets_root,
            name,
            key.0.potion_color,
            (key.1, key.0.count()),
            key.0.variant.as_deref(),
            None,
            &|id| self.sheets.find(id),
        )
        .and_then(|held| {
            upload(renderer, &self.sheets, held)
                .map_err(|e| crate::warn!("Held item {name} not uploaded: {e:#}"))
                .ok()
        });
        self.items.insert(key, held);
    }

    /// Makes sure a player head with `profile` can be drawn in hand with
    pub fn prepare_head(
        &mut self,
        renderer: &mut Renderer,
        stack: &ItemStack,
        profile: &HeadProfile,
        (skin, own_skin): (&RgbaImage, bool),
        using: bool,
    ) {
        let key = (profile.skin_key(), using);
        if let Some(&(built_own, _)) = self.heads.get(&key)
            && (built_own || !own_skin)
        {
            return;
        }
        let name = stack.item.registry_key;
        let held =
            ItemModel::resolve_in_use(&self.assets_root, name, None, (using, Count::ONE), None)
                .and_then(|item| {
                    item.held_with_skin(&self.assets_root, Some(skin), &|id| self.sheets.find(id))
                })
                .and_then(|held| {
                    upload(renderer, &self.sheets, held)
                        .map_err(|e| crate::warn!("Held head not uploaded: {e:#}"))
                        .ok()
                });
        if let Some((_, Some(old))) = self.heads.insert(key, (own_skin, held)) {
            free(renderer, old);
        }
    }

    /// Makes sure the block model `model` can be drawn by [`Self::block`].
    pub fn prepare_block(&mut self, renderer: &mut Renderer, model: &'static str) {
        if self.blocks.contains_key(model) {
            return;
        }
        let held = ItemModel::block(&self.assets_root, model)
            .and_then(|item| item.held(&self.assets_root, &|id| self.sheets.find(id)))
            .and_then(|held| {
                upload(renderer, &self.sheets, held)
                    .map_err(|e| crate::warn!("Block {model} not uploaded: {e:#}"))
                    .ok()
            });
        self.blocks.insert(model, held);
    }

    /// The meshes and textures of a prepared block model, centred on the
    pub fn block(&self, model: &str) -> impl Iterator<Item = (MeshId, TextureId, bool)> + '_ {
        draws(
            self.blocks.get(model).and_then(Option::as_ref),
            HandPose::Ground,
        )
    }

    /// Moves the animated items' textures to the frame showing at `tick`
    pub fn animate(&mut self, renderer: &mut Renderer, tick: u64) {
        let uploaded = self
            .items
            .values_mut()
            .chain(self.blocks.values_mut())
            .flatten()
            .chain(self.heads.values_mut().filter_map(|(_, u)| u.as_mut()));
        for item in uploaded {
            let (Some((animation, shown)), Some(own)) = (&mut item.animation, item.own) else {
                continue;
            };
            let frame = animation.frame_at(tick);
            if frame != *shown {
                *shown = frame;
                renderer.update_texture(own, 0, 0, 0, animation.frames[frame].clone());
            }
        }
    }

    /// Frees every uploaded item.
    pub fn release(&mut self, renderer: &mut Renderer) {
        item_model::forget_count_cuts();
        let blocks = self.blocks.drain().filter_map(|(_, u)| u);
        let heads = self.heads.drain().filter_map(|(_, (_, u))| u);
        for uploaded in blocks
            .chain(self.items.drain().filter_map(|(_, u)| u))
            .chain(heads)
        {
            free(renderer, uploaded);
        }
    }

    /// The meshes and textures for `stack` held in `pose`, in use or not,
    /// one per part (opaque ones first), each blended or not.
    /// Texels per UV unit of `texture`, for [`crate::renderer::Glint`]: an
    /// atlas's side, or 16 for an item's own texture (UVs over the sprite).
    pub fn glint_texels(&self, texture: TextureId) -> u32 {
        if texture == self.sheets.items {
            self.sheets.item_side
        } else if texture == self.sheets.blocks {
            self.sheets.block_side
        } else {
            16
        }
    }

    pub fn get(
        &self,
        stack: &ItemStack,
        pose: HandPose,
        using: impl Into<HeldUse>,
    ) -> impl Iterator<Item = (MeshId, TextureId, bool)> + '_ {
        draws(self.uploaded(stack, using.into()), pose)
    }

    /// The lowest point and the depth of `stack`'s model on the ground.
    pub fn ground_bounds(&self, stack: &ItemStack) -> Option<(f32, f32)> {
        Some(self.uploaded(stack, HeldUse::Idle)?.ground)
    }

    fn uploaded(&self, stack: &ItemStack, using: HeldUse) -> Option<&Uploaded> {
        if stack.is_empty() {
            return None;
        }
        let head = HeadProfile::of_stack(stack)
            .and_then(|profile| {
                self.heads
                    .get(&(profile.skin_key(), using == HeldUse::Using))
            })
            .and_then(|(_, uploaded)| uploaded.as_ref());
        let uploaded = match head {
            Some(head) => head,
            None => self
                .items
                .get(&(ItemKey::of(&self.assets_root, stack), using))?
                .as_ref()?,
        };
        Some(uploaded)
    }
}

fn draws(
    uploaded: Option<&Uploaded>,
    pose: HandPose,
) -> impl Iterator<Item = (MeshId, TextureId, bool)> + '_ {
    uploaded
        .into_iter()
        .flat_map(|u| u.parts.iter())
        .map(move |part| (part.meshes[pose as usize], part.texture, part.translucent))
}

fn free(renderer: &mut Renderer, uploaded: Uploaded) {
    if let Some(own) = uploaded.own {
        renderer.free_texture(own);
    }
    for part in uploaded.parts {
        for mesh in part.meshes {
            renderer.free_mesh(mesh);
        }
    }
}

fn upload(renderer: &mut Renderer, sheets: &Sheets, held: HeldItem) -> anyhow::Result<Uploaded> {
    let own = match &held.texture {
        Some(texture) => Some(renderer.load_texture(texture)?),
        None => None,
    };
    let mut uploaded = Uploaded {
        parts: Vec::with_capacity(held.parts.len()),
        own,
        ground: ground_bounds(&held),
        animation: held.animation.map(|animation| (animation, 0)),
    };
    for part in &held.parts {
        let texture = match part.sheet {
            Sheet::Items => sheets.items,
            Sheet::Blocks => sheets.blocks,
            Sheet::Own => own.expect("an own part comes with its texture"),
        };
        let mut meshes = Vec::with_capacity(HandPose::ALL.len());
        for vertices in &part.meshes {
            match renderer.upload_mesh(vertices) {
                Ok(mesh) => meshes.push(mesh),
                Err(e) => {
                    meshes.into_iter().for_each(|m| renderer.free_mesh(m));
                    free(renderer, uploaded);
                    return Err(e);
                }
            }
        }
        let meshes =
            <[MeshId; 10]>::try_from(meshes).unwrap_or_else(|_| unreachable!("one per pose"));
        uploaded.parts.push(Part {
            texture,
            translucent: part.translucent,
            meshes,
        });
    }
    Ok(uploaded)
}

/// The lowest point of the ground pose over every part, and its depth.
fn ground_bounds(held: &HeldItem) -> (f32, f32) {
    let mut points = held
        .parts
        .iter()
        .flat_map(|part| &part.meshes[HandPose::Ground as usize])
        .map(|v| v.pos)
        .peekable();
    if points.peek().is_none() {
        return (0.0, 0.0);
    }
    let (mut min_y, mut min_z, mut max_z) = (f32::MAX, f32::MAX, f32::MIN);
    for [_, y, z] in points {
        min_y = min_y.min(y);
        min_z = min_z.min(z);
        max_z = max_z.max(z);
    }
    (min_y, max_z - min_z)
}
