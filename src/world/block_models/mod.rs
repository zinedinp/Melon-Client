//! Every block state's model, baked once into quads the mesher copies.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use image::RgbaImage;
use pumpkin_data::{Block, BlockStateId};

use crate::assets::json_model::{Model, Quad, element_quads};
use crate::renderer::{CornerLight, TerrainQuad, WorldVertex};
use crate::world::block::direction::Direction;
use crate::world::block_tint::TintKind;
use crate::world::geometry::UV_INSET;
use crate::world::texture_animation::TextureAnimator;
use crate::world::transparency::{AlphaMap, AlphaRule, Transparency};

pub mod ao_shape;
mod atlas;
mod block_state;
pub mod mipmap;
mod mod_hints;
mod tint;
pub mod variants;

use crate::math::angle::Rotation;
use ao_shape::AoShape;
use atlas::{PackedAtlas, pack_atlas};
use block_state::{BlockState, ModelRef, Options, condition_holds, parse_block_state};
use tint::bake_tint;
use variants::{Part, RandomModel, StateModel};

/// One quad of a baked block: what the mesher reads per face. Its corners
/// are in [`BlockModels::corners_of`], once for all equal quads.
#[derive(Clone, Copy, Debug)]
pub struct BakedQuad {
    /// Dropped when the neighbour on this side is solid.
    pub cullface: Option<Direction>,
    /// The way its front faces, if along an axis; `None` for slanted quads.
    pub facing: Option<Direction>,
    /// The chunk layer it is drawn in.
    pub layer: ChunkLayer,
    /// Its corners' place in [`BlockModels::quad_corners`].
    pub index: u32,
    /// Its element's `light_emission`, `0..=15`.
    pub emission: u8,
    /// Its model's `ambientocclusion`.
    pub ambient_occlusion: bool,
    /// Biome colour the mesher multiplies onto the baked shade.
    pub tint: TintKind,
}

/// Vanilla `ChunkSectionLayer`, from the quad's texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkLayer {
    /// Every pixel opaque: drawn without `discard`.
    Solid = 0,
    /// Some pixels fully clear (leaves, plants, glass panes' frames).
    Cutout = 1,
    /// Partly see-through pixels (ice, stained glass), or a model that forces it.
    Translucent = 2,
}

impl ChunkLayer {
    /// `ChunkSectionLayer.byTransparency`.
    pub fn of(transparency: Transparency) -> Self {
        if transparency.translucent {
            Self::Translucent
        } else if transparency.holes {
            Self::Cutout
        } else {
            Self::Solid
        }
    }
}

/// The baked quads of every block state, and the atlas they sample.
pub struct BlockModels {
    by_state: Vec<StateModel<BakedQuad>>,
    /// Per state, what the mesher asks of every block.
    flags: Vec<MeshFlags>,
    /// Every atlas texture's index in `rects`, by texture ID.
    sprites: HashMap<String, usize>,
    /// Every distinct baked quad's corners, by [`BakedQuad::index`].
    corners: Vec<[WorldVertex; 4]>,
    /// What smooth lighting reads off each of them, by the same index.
    shapes: Vec<AoShape>,
    /// Per state, whether its model takes smooth lighting.
    smooth: Vec<bool>,
    /// Per state, the texture its model names `particle`, as an index into `rects`.
    particle: Vec<u16>,
    /// Every atlas texture's rectangle and its texels' transparency.
    rects: Vec<([f32; 4], AlphaMap)>,
}

/// No particle texture: the first quad's stands in.
const NO_TEXTURE: u16 = u16::MAX;

/// What a block's particles look like ([`BlockModels::particle_look`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleLook {
    /// `[u0, v0, u1, v1]` in the atlas.
    pub uv: [f32; 4],
    pub translucent: bool,
    /// A constant tint (spruce, birch), `1,1,1` for none.
    pub rgb: [f32; 3],
    /// The biome colour on top of it.
    pub tint: TintKind,
    /// `BlockColors` has a tint source for it, even one giving white.
    pub tint_source: bool,
}

/// A texture the atlas holds besides those the block models use.
pub struct ExtraTexture {
    pub id: String,
    pub image: Option<RgbaImage>,
}

/// What the mesher needs to know of a block state before looking at its quads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshFlags(u8);

impl MeshFlags {
    const SOLID: u8 = 1;
    const DRAWS: u8 = 2;
    const BURIED_HIDDEN: u8 = 4;
    const OFFSET: u8 = 8;
    /// Air, unknown states: nothing to draw.
    const NOTHING: Self = Self(Self::BURIED_HIDDEN);

    fn of(id: u16, model: &StateModel<BakedQuad>) -> Self {
        let has_quads = model.all_quads().next().is_some();
        let fluid = crate::world::fluid::fluid_of(id).is_some();
        let mut bits = 0;
        if crate::world::block::state_flags::is_solid(id)
            || BlockStateId::new(id).is_some_and(|s| s.to_state().is_solid_render())
        {
            bits |= Self::SOLID;
        }
        if has_quads || fluid {
            bits |= Self::DRAWS;
        }
        if !fluid && model.all_quads().all(|quad| quad.cullface.is_some()) {
            bits |= Self::BURIED_HIDDEN;
        }
        if crate::world::block::offset::has_offset(id) {
            bits |= Self::OFFSET;
        }
        let emission = if !has_quads {
            0
        } else {
            BlockStateId::new(id).map_or(0, |state| state.to_state().luminance.min(15))
        };
        Self(bits | emission << 4)
    }

    /// [`state_flags::is_solid`](crate::world::block::state_flags::is_solid).
    #[inline]
    pub fn solid(self) -> bool {
        self.0 & Self::SOLID != 0
    }

    /// Has quads or a fluid.
    #[inline]
    pub fn draws(self) -> bool {
        self.0 & Self::DRAWS != 0
    }

    /// Every quad has a cullface and there is no fluid.
    #[inline]
    pub fn buried_hidden(self) -> bool {
        self.0 & Self::BURIED_HIDDEN != 0
    }

    /// Drawn off the block's centre ([`offset`](crate::world::block::offset)).
    #[inline]
    pub fn offset(self) -> bool {
        self.0 & Self::OFFSET != 0
    }

    /// The light level its own quads get at least.
    #[inline]
    pub fn emission(self) -> u8 {
        self.0 >> 4
    }
}

/// The atlas image and what animates in it.
pub struct BakedAtlas {
    pub image: RgbaImage,
    /// [`mipmap::LEVELS`] smaller levels of `image`, half the size each.
    pub mips: Vec<RgbaImage>,
    pub animator: TextureAnimator,
}

/// A second atlas packed like the block atlas, without mipmaps (vanilla's
/// `items` atlas: `AtlasManager.AtlasConfig(..., false)`).
pub struct SpriteSheet {
    pub image: RgbaImage,
    /// `[u0, v0, u1, v1]` and the texels' transparency, by texture ID as
    /// given.
    pub sprites: HashMap<String, ([f32; 4], AlphaMap)>,
    pub animator: TextureAnimator,
}

/// Packs the textures `ids` (missing ones are left out) into one atlas no
/// wider or taller than `max_side`. Unlike block textures, a still one is
/// kept whole when it isn't square (vanilla's `SpriteLoader` keeps any
/// size; custom item packs use tall ones).
pub fn pack_sprites(assets_root: &Path, ids: Vec<String>, max_side: u32) -> SpriteSheet {
    let loaded = atlas::parallel_map(ids, |id| {
        let Some((png, meta)) = crate::assets::texture_with_meta(assets_root, &id) else {
            // A paletted permutation: no file of its own.
            let image = crate::assets::texture_image(assets_root, &id)?;
            return Some((id, Some(image)));
        };
        let animated = meta.as_ref().is_some_and(|m| m.get("animation").is_some());
        // An animated one is loaded with its frames by the packer.
        let image = if animated {
            None
        } else {
            Some(crate::assets::decode_png(&png)?)
        };
        Some((id, image))
    });
    let images: Vec<(String, Option<RgbaImage>)> = loaded.into_iter().flatten().collect();
    let ids: Vec<String> = images.iter().map(|(id, _)| id.clone()).collect();
    let packed = pack_atlas(assets_root, images, max_side);
    let sprites = ids
        .into_iter()
        .zip(packed.rects.into_iter().zip(packed.alpha))
        .collect();
    SpriteSheet {
        image: packed.image,
        sprites,
        animator: packed.animator,
    }
}

impl BlockModels {
    /// Hands `out` the quads a block of `state` at world position `pos`
    #[inline]
    pub fn quads_at(&self, state: u16, pos: [i32; 3], out: impl FnMut(&[BakedQuad])) {
        if let Some(model) = self.by_state.get(usize::from(state)) {
            model.quads_at(pos, out);
        }
    }

    /// Whether a face of `state` is drawn with the translucent terrain
    /// (`BlockStateModel.hasMaterialFlag(1)`): its outline then goes after it.
    pub fn has_translucent_quads(&self, state: u16) -> bool {
        let mut translucent = false;
        self.quads_at(state, [0, 0, 0], |quads| {
            translucent |= quads.iter().any(|q| q.layer == ChunkLayer::Translucent);
        });
        translucent
    }

    /// What the mesher asks of every block, in one lookup.
    #[inline]
    pub fn mesh_flags(&self, state: u16) -> MeshFlags {
        self.flags
            .get(usize::from(state))
            .copied()
            .unwrap_or(MeshFlags::NOTHING)
    }

    /// What smooth lighting reads off `quad`'s geometry.
    #[inline]
    pub fn ao_shape(&self, quad: &BakedQuad) -> &AoShape {
        &self.shapes[quad.index as usize]
    }

    /// `quad`'s corners as ready vertices in block space (`0..1`),
    /// counter-clockwise from outside.
    #[inline]
    pub fn corners_of(&self, quad: &BakedQuad) -> &[WorldVertex; 4] {
        &self.corners[quad.index as usize]
    }

    /// Whether a block of `state` may be smooth lit.
    #[inline]
    pub fn smooth_lighting(&self, state: u16) -> bool {
        self.smooth
            .get(usize::from(state))
            .copied()
            .unwrap_or(false)
    }

    /// Every distinct baked quad's four corners in index order, for the GPU's quad table.
    pub fn quad_corners(&self) -> &[WorldVertex] {
        self.corners.as_flattened()
    }

    /// The vertices `terrain.vert` makes of `quad` in a mesh whose base is `base`.
    pub fn quad_vertices(
        &self,
        quad: TerrainQuad,
        base: [i32; 3],
        cardinal: &[f32; 6],
    ) -> [WorldVertex; 4] {
        let [x, y, z] = quad.block();
        let [ox, oy, oz] = crate::world::block::offset::unpack(quad.offset());
        let at = [
            (base[0] + x as i32) as f32 + ox,
            (base[1] + y as i32) as f32 + oy,
            (base[2] + z as i32) as f32 + oz,
        ];
        let mut i = 0;
        self.corners[quad.index() as usize].map(|v| {
            let corner = quad.corner(i);
            i += 1;
            let [px, py, pz] = v.pos;
            let gray = u32::from(corner.ao) * 255 / u32::from(CornerLight::AO_ONE);
            let [r, g, b, a] = v.color;
            let [tr, tg, tb] = quad.tint();
            let scale = cardinal[usize::from(v.face).min(5)];
            let shade = |c: u8, t: u8| {
                ((u32::from(c) * gray / 255 * u32::from(t) / 255) as f32 * scale).min(255.0) as u8
            };
            WorldVertex {
                pos: [at[0] + px, at[1] + py, at[2] + pz],
                color: [shade(r, tr), shade(g, tg), shade(b, tb), a],
                light: [corner.block * 4, corner.sky * 4],
                ..v
            }
        })
    }

    /// The atlas rectangle `[u0, v0, u1, v1]` of the texture a state shows in particles.
    pub fn particle_sprite(&self, state: u16) -> Option<[f32; 4]> {
        self.particle_texture(state)
            .map(|(rect, _)| rect)
            .or_else(|| self.first_quad_sprite(state))
    }

    fn particle_texture(&self, state: u16) -> Option<([f32; 4], bool)> {
        let index = *self.particle.get(usize::from(state))?;
        (index != NO_TEXTURE)
            .then(|| self.rects.get(usize::from(index)))
            .flatten()
            .map(|(rect, alpha)| (*rect, alpha.whole().translucent))
    }

    fn first_quad_sprite(&self, state: u16) -> Option<[f32; 4]> {
        let mut rect = None;
        self.quads_at(state, [0, 0, 0], |quads| {
            if rect.is_none()
                && let Some(quad) = quads.first()
            {
                let corners = self.corners_of(quad);
                let us = corners.map(|v| v.uv[0]);
                let vs = corners.map(|v| v.uv[1]);
                let min = |a: [f32; 4]| a.into_iter().fold(f32::MAX, f32::min);
                let max = |a: [f32; 4]| a.into_iter().fold(f32::MIN, f32::max);
                rect = Some([min(us), min(vs), max(us), max(vs)]);
            }
        });
        rect
    }

    /// The tint the faces of `state` have.
    pub fn tint_kind(&self, state: u16) -> TintKind {
        let mut kind = TintKind::None;
        self.quads_at(state, [0, 0, 0], |quads| {
            if kind == TintKind::None
                && let Some(quad) = quads.iter().find(|q| q.tint != TintKind::None)
            {
                kind = quad.tint;
            }
        });
        kind
    }

    /// What a block's particles show (`TerrainParticle`).
    pub fn particle_look(&self, state: u16) -> Option<ParticleLook> {
        let (uv, translucent) = match self.particle_texture(state) {
            Some(found) => found,
            None => {
                let uv = self.first_quad_sprite(state)?;
                let mut translucent = false;
                self.quads_at(state, [0, 0, 0], |quads| {
                    if let Some(quad) = quads.first() {
                        translucent |= quad.layer == ChunkLayer::Translucent;
                    }
                });
                (uv, translucent)
            }
        };
        let block = BlockStateId::new(state).map(Block::from_state_id);
        let tint_source = block.is_some_and(|b| tint::has_tint_source(b.name));
        let (rgb, tint) = BlockStateId::new(state).map_or(([1.0; 3], TintKind::None), |id| {
            let block = Block::from_state_id(id);
            let props = block
                .properties(id)
                .map(|p| p.to_props())
                .unwrap_or_default();
            let props: Vec<(&str, &str)> = props
                .iter()
                .map(|prop| (prop.key_str(), prop.value_str()))
                .collect();
            tint::particle_tint(block.name, &props)
        });
        Some(ParticleLook {
            uv,
            translucent,
            rgb,
            tint,
            tint_source,
        })
    }

    /// An atlas texture's `[u0, v0, u1, v1]`, e.g. `minecraft:block/water_still`.
    pub fn sprite(&self, id: &str) -> Option<[f32; 4]> {
        self.sprite_info(id).map(|(rect, _)| rect)
    }

    /// [`Self::sprite`] and its texels' transparency.
    pub fn sprite_info(&self, id: &str) -> Option<([f32; 4], AlphaMap)> {
        self.rects.get(*self.sprites.get(id)?).cloned()
    }

    /// Bakes every state from the models under `assets_root` and packs one atlas.
    pub fn bake(
        assets_root: &Path,
        extras: Vec<ExtraTexture>,
        max_atlas_side: u32,
    ) -> (Self, BakedAtlas) {
        let mut baker = Baker::new(assets_root);
        let extra = pumpkin_data::mod_registry::extra_state_ids();
        let last = extra
            .iter()
            .copied()
            .max()
            .unwrap_or(BlockStateId::COUNT.saturating_sub(1));
        let pending: Vec<StateModel<PendingQuad>> = (0..=last).map(|id| baker.state(id)).collect();
        let particle: Vec<u16> = (0..=last)
            .map(|id| {
                baker
                    .particle(id)
                    .and_then(|i| u16::try_from(i).ok())
                    .unwrap_or(NO_TEXTURE)
            })
            .collect();
        let mut images: Vec<(String, Option<RgbaImage>)> = baker
            .texture_ids
            .iter()
            .map(|id| (id.clone(), None))
            .collect();
        for extra in extras {
            if !baker.texture_index.contains_key(&extra.id) {
                baker.texture_index.insert(extra.id.clone(), images.len());
                images.push((extra.id, extra.image));
            }
        }
        let packed = pack_atlas(assets_root, images, max_atlas_side);
        let mut finished: HashMap<*const [PendingQuad], Arc<[BakedQuad]>> = HashMap::new();
        let mut corners: Vec<[WorldVertex; 4]> = Vec::new();
        let mut shapes: Vec<AoShape> = Vec::new();
        let mut index_of: HashMap<[u32; 28], u32> = HashMap::new();
        let mut finish = |quads: &Arc<[PendingQuad]>| {
            finished
                .entry(Arc::as_ptr(quads))
                .or_insert_with(|| {
                    quads
                        .iter()
                        .map(|q| {
                            let (vertices, mut quad) = q.finish(&packed);
                            quad.index =
                                *index_of.entry(corner_key(&vertices)).or_insert_with(|| {
                                    corners.push(vertices);
                                    shapes.push(AoShape::of(vertices.map(|v| Vec3::from(v.pos))));
                                    (corners.len() - 1) as u32
                                });
                            quad
                        })
                        .collect()
                })
                .clone()
        };
        let by_state: Vec<StateModel<BakedQuad>> = pending
            .iter()
            .enumerate()
            .map(|(id, model)| {
                let model = model.map(&mut finish);
                mod_hints::apply(id as u16, &model).unwrap_or(model)
            })
            .collect();
        let sprites = baker
            .texture_index
            .iter()
            .map(|(id, &index)| (id.clone(), index))
            .collect();
        let rects = packed
            .rects
            .iter()
            .copied()
            .zip(packed.alpha.iter().cloned())
            .collect();
        let atlas = BakedAtlas {
            image: packed.image,
            mips: packed.mips,
            animator: packed.animator,
        };
        let flags = by_state
            .iter()
            .enumerate()
            .map(|(id, model)| MeshFlags::of(id as u16, model))
            .collect();
        let smooth = by_state
            .iter()
            .map(|model| {
                model
                    .all_quads()
                    .next()
                    .is_some_and(|q| q.ambient_occlusion)
            })
            .collect();
        assert!(
            corners.len() < TerrainQuad::MAX_QUADS as usize,
            "{} baked quads: more than a TerrainQuad can name",
            corners.len()
        );
        (
            Self {
                by_state,
                flags,
                sprites,
                corners,
                shapes,
                smooth,
                particle,
                rects,
            },
            atlas,
        )
    }
}

/// A quad's corners as plain bits, to find equal quads.
fn corner_key(vertices: &[WorldVertex; 4]) -> [u32; 28] {
    let mut key = [0; 28];
    for (out, v) in key.as_chunks_mut::<7>().0.iter_mut().zip(vertices) {
        let [x, y, z] = v.pos.map(f32::to_bits);
        let [u, w] = v.uv;
        out.copy_from_slice(&[
            x,
            y,
            z,
            u.to_bits(),
            w.to_bits(),
            u32::from_le_bytes(v.color),
            u32::from(v.light[0]) | u32::from(v.light[1]) << 8,
        ]);
    }
    key
}

/// A quad waiting for the atlas: its texture as an index.
struct PendingQuad {
    corners: [Vec3; 4],
    /// `0..1` within the texture.
    uvs: [Vec2; 4],
    texture: usize,
    color: [f32; 3],
    /// The direction its shade was taken for.
    shade_face: Direction,
    cullface: Option<Direction>,
    /// What its chunk layer is read from: the texels its `uv` covers
    /// (before `uvlock`), or `force_translucent`.
    alpha: AlphaRule,
    emission: u8,
    ambient_occlusion: bool,
    tint: TintKind,
}

impl PendingQuad {
    /// Its corners as vertices, and the quad still without their index.
    fn finish(&self, packed: &PackedAtlas) -> ([WorldVertex; 4], BakedQuad) {
        let [u0, v0, u1, v1] = packed.rects[self.texture];
        let (w, h) = packed.sizes[self.texture];
        let texel_inset = (16.0 * UV_INSET).max(packed.longest_side() as f32 / 65535.0);
        let inset = Vec2::new(
            (texel_inset / w.max(1) as f32).min(0.25),
            (texel_inset / h.max(1) as f32).min(0.25),
        );
        let vertices = std::array::from_fn(|i| {
            let uv = self.uvs[i].clamp(inset, Vec2::ONE - inset);
            let uv = [u0 + uv.x * (u1 - u0), v0 + uv.y * (v1 - v0)];
            WorldVertex::new(self.corners[i].to_array(), uv, self.color)
                .with_face(self.shade_face.index() as u8)
        });
        let quad = BakedQuad {
            cullface: self.cullface,
            facing: facing_of(&self.corners),
            layer: ChunkLayer::of(self.alpha.judge(&packed.alpha[self.texture])),
            index: 0,
            emission: self.emission,
            ambient_occlusion: self.ambient_occlusion,
            tint: self.tint,
        };
        (vertices, quad)
    }
}

/// The axis direction the counter-clockwise quad `corners` face, if any.
fn facing_of(corners: &[Vec3; 4]) -> Option<Direction> {
    let normal = (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .normalize_or_zero();
    Direction::ALL.into_iter().find(|direction| {
        let [x, y, z] = direction.step();
        normal.dot(Vec3::new(x as f32, y as f32, z as f32)) > 0.999
    })
}

/// A model with its parents merged, and its elements' quads.
type ResolvedModel = (Model, Vec<Quad>);

struct Baker<'a> {
    assets_root: &'a Path,
    states: HashMap<&'static str, Option<BlockState>>,
    models: HashMap<String, Option<Arc<ResolvedModel>>>,
    /// Baked models per block and distinct model lists, so 30k states share them.
    baked: HashMap<(String, String), StateModel<PendingQuad>>,
    /// Each weighted option's quads, per block (the tint) and model ref.
    options: HashMap<(String, String), Arc<[PendingQuad]>>,
    texture_index: HashMap<String, usize>,
    texture_ids: Vec<String>,
    /// Whether each texture named so far exists.
    existing_textures: HashMap<String, bool>,
}

impl<'a> Baker<'a> {
    fn new(assets_root: &'a Path) -> Self {
        Self {
            assets_root,
            states: HashMap::new(),
            models: HashMap::new(),
            baked: HashMap::new(),
            options: HashMap::new(),
            texture_index: HashMap::new(),
            texture_ids: Vec::new(),
            existing_textures: HashMap::new(),
        }
    }

    fn state(&mut self, id: u16) -> StateModel<PendingQuad> {
        let Some(state) = BlockStateId::new(id) else {
            return StateModel::Fixed(Arc::from([]));
        };
        let block = Block::from_state_id(state);
        let props: Vec<(&str, &str)> = block
            .properties(state)
            .map(|p| {
                p.to_props()
                    .into_iter()
                    .map(|prop| (prop.key_str(), prop.value_str()))
                    .collect()
            })
            .unwrap_or_default();
        let (multipart, parts) = self.model_refs(block.name, &props);
        let structure = parts
            .iter()
            .map(|options| {
                options
                    .iter()
                    .map(|(weight, r)| format!("{weight}*{}", r.key()))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join(";");
        let key = (block.name.to_owned(), format!("{multipart}|{structure}"));
        if let Some(done) = self.baked.get(&key) {
            return done.clone();
        }
        let model = if parts.iter().all(|options| options.len() == 1) {
            let mut quads = Vec::new();
            for options in &parts {
                self.bake_ref(&options[0].1, block.name, &props, &mut quads);
            }
            StateModel::Fixed(quads.into())
        } else {
            let mut baked_parts = Vec::with_capacity(parts.len());
            for options in &parts {
                let mut baked = Vec::with_capacity(options.len());
                for (weight, model_ref) in options {
                    baked.push((*weight, self.bake_option(block.name, model_ref, &props)));
                }
                baked_parts.push(Part {
                    total: options.iter().map(|&(weight, _)| weight).sum(),
                    options: baked,
                });
            }
            StateModel::Random(Arc::new(RandomModel {
                multipart,
                parts: baked_parts,
            }))
        };
        self.baked.insert(key, model.clone());
        model
    }

    /// The texture a state's model names `particle`.
    fn particle(&mut self, id: u16) -> Option<usize> {
        let state = BlockStateId::new(id)?;
        let block = Block::from_state_id(state);
        let props = block
            .properties(state)
            .map(|p| p.to_props())
            .unwrap_or_default();
        let props: Vec<(&str, &str)> = props
            .iter()
            .map(|prop| (prop.key_str(), prop.value_str()))
            .collect();
        let (_, parts) = self.model_refs(block.name, &props);
        let model_ref = &parts.first()?.first()?.1;
        let assets_root = self.assets_root;
        let resolved = self
            .models
            .entry(model_ref.model.clone())
            .or_insert_with(|| {
                let model = Model::resolve(assets_root, &model_ref.model)?;
                let quads = model.elements.iter().flat_map(element_quads).collect();
                Some(Arc::new((model, quads)))
            })
            .clone()?;
        let texture_id = resolved.0.texture("particle")?;
        if !self.texture_exists(&texture_id) {
            return None;
        }
        Some(self.texture(texture_id))
    }

    /// One weighted option's quads, baked once per block.
    fn bake_option(
        &mut self,
        block: &str,
        model_ref: &ModelRef,
        props: &[(&str, &str)],
    ) -> Arc<[PendingQuad]> {
        let key = (block.to_owned(), model_ref.key());
        if let Some(done) = self.options.get(&key) {
            return done.clone();
        }
        let mut quads = Vec::new();
        self.bake_ref(model_ref, block, props, &mut quads);
        let quads: Arc<[PendingQuad]> = quads.into();
        self.options.insert(key, quads.clone());
        quads
    }

    /// The models a state with `props` shows.
    fn model_refs(&mut self, block: &'static str, props: &[(&str, &str)]) -> (bool, Vec<Options>) {
        let assets_root = self.assets_root;
        let parsed = self.states.entry(block).or_insert_with(|| {
            let parsed = parse_block_state(assets_root, block);
            if crate::probe::enabled() {
                probe_block_state(assets_root, block, parsed.as_ref());
            }
            parsed
        });
        let has = |key: &str, value: &str| props.iter().any(|&(k, v)| k == key && v == value);
        match parsed {
            None => (false, Vec::new()),
            Some(BlockState::Variants(variants)) => (
                false,
                variants
                    .iter()
                    .find(|(conditions, _)| conditions.iter().all(|(k, v)| has(k, v)))
                    .map(|(_, options)| vec![options.clone()])
                    .unwrap_or_default(),
            ),
            Some(BlockState::Multipart(parts)) => (
                true,
                parts
                    .iter()
                    .filter(|(when, _)| condition_holds(when, props))
                    .map(|(_, options)| options.clone())
                    .collect(),
            ),
        }
    }

    fn bake_ref(
        &mut self,
        model_ref: &ModelRef,
        block: &str,
        props: &[(&str, &str)],
        out: &mut Vec<PendingQuad>,
    ) {
        let assets_root = self.assets_root;
        let Some(resolved) = self
            .models
            .entry(model_ref.model.clone())
            .or_insert_with(|| {
                let model = Model::resolve(assets_root, &model_ref.model)?;
                let quads = model.elements.iter().flat_map(element_quads).collect();
                Some(Arc::new((model, quads)))
            })
            .clone()
        else {
            if crate::probe::enabled() {
                crate::probe::note("block.model_missing", &model_ref.model, || {
                    format!("for {block}")
                });
            }
            return;
        };
        let (model, quads) = &*resolved;
        let rotation = variant_rotation(model_ref.x, model_ref.y);
        let center = Vec3::splat(8.0);
        for quad in quads {
            let Some(texture_id) = model.texture(&quad.texture) else {
                continue;
            };
            if !self.texture_exists(&texture_id) {
                if crate::probe::enabled() {
                    crate::probe::note("block.texture_missing", &texture_id, || {
                        format!("in {}", model_ref.model)
                    });
                }
                continue;
            }
            let alpha = if model.forced_translucent.contains(&texture_id) {
                AlphaRule::Translucent
            } else {
                AlphaRule::region(&quad.uvs)
            };
            let texture = self.texture(texture_id);
            let corners = quad.corners.map(|c| rotation * (c - center) + center);
            let normal = (rotation * quad.normal).normalize_or_zero();
            let uvs = if model_ref.uvlock && (model_ref.x != 0 || model_ref.y != 0) {
                locked_uvs(&corners, normal)
            } else {
                quad.uvs
            };
            let direction = quad
                .shade_override
                .or_else(|| crate::world::geometry::closest_direction(normal))
                .unwrap_or(Direction::Up);
            let shade = crate::world::geometry::cardinal_shade(direction);
            let (tint, tint_kind) = bake_tint(block, quad.tint, props);
            out.push(PendingQuad {
                corners: corners.map(|c| c / 16.0),
                uvs,
                texture,
                color: tint.map(|c| c * shade),
                shade_face: direction,
                cullface: quad
                    .cullface
                    .as_deref()
                    .and_then(Direction::from_name)
                    .map(|d| rotate_direction(rotation, d)),
                alpha,
                emission: quad.light_emission,
                ambient_occlusion: model.ambient_occlusion,
                tint: tint_kind,
            });
        }
    }

    fn texture_exists(&mut self, id: &str) -> bool {
        let assets_root = self.assets_root;
        *self
            .existing_textures
            .entry(id.to_owned())
            .or_insert_with(|| crate::assets::resource_exists(assets_root, "textures", id, "png"))
    }

    fn texture(&mut self, id: String) -> usize {
        if let Some(&index) = self.texture_index.get(&id) {
            return index;
        }
        let index = self.texture_ids.len();
        self.texture_index.insert(id.clone(), index);
        self.texture_ids.push(id);
        index
    }
}

/// The tester.
fn probe_block_state(assets_root: &Path, block: &str, parsed: Option<&BlockState>) {
    let name = block.trim_start_matches("minecraft:");
    let from_pack = crate::assets::packs::active()
        .find(
            assets_root,
            "minecraft",
            &format!("blockstates/{name}.json"),
        )
        .is_some_and(|(_, layer)| layer > 0);
    let Some(parsed) = parsed else {
        if !name.contains("air") {
            crate::probe::count("block.no_model", name);
        }
        return;
    };
    if from_pack {
        let describe = match parsed {
            BlockState::Variants(v) => format!("{} variants", v.len()),
            BlockState::Multipart(p) => format!("{} multipart parts", p.len()),
        };
        crate::probe::note("block.pack_blockstate", name, || describe);
    }
}

/// `BlockModelRotation`.
fn variant_rotation(x: i64, y: i64) -> Mat3 {
    let turn = Mat3::rot_y(-(y as f32).to_radians()) * Mat3::rot_x(-(x as f32).to_radians());
    Mat3::from_cols_array(&turn.to_cols_array().map(f32::round))
}

/// `uvlock`.
fn locked_uvs(corners: &[Vec3; 4], normal: Vec3) -> [Vec2; 4] {
    let face = nearest_direction(normal);
    corners.map(|c| {
        let (u, v) = match face {
            Direction::Down => (c.x, 16.0 - c.z),
            Direction::Up => (c.x, c.z),
            Direction::North => (16.0 - c.x, 16.0 - c.y),
            Direction::South => (c.x, 16.0 - c.y),
            Direction::West => (c.z, 16.0 - c.y),
            Direction::East => (16.0 - c.z, 16.0 - c.y),
        };
        Vec2::new(u, v) / 16.0
    })
}

fn nearest_direction(v: Vec3) -> Direction {
    Direction::ALL
        .into_iter()
        .max_by(|a, b| {
            let dot = |d: Direction| {
                let [x, y, z] = d.step();
                v.dot(Vec3::new(x as f32, y as f32, z as f32))
            };
            dot(*a).total_cmp(&dot(*b))
        })
        .unwrap_or(Direction::Up)
}

fn rotate_direction(rotation: Mat3, direction: Direction) -> Direction {
    let [x, y, z] = direction.step();
    nearest_direction(rotation * Vec3::new(x as f32, y as f32, z as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn assets() -> Option<std::path::PathBuf> {
        let root = crate::assets::default_root();
        root.join("minecraft/blockstates/stone.json")
            .exists()
            .then_some(root)
    }

    fn state_of(block: &Block) -> u16 {
        block.default_state.id.as_u16()
    }

    #[test]
    fn a_baked_quad_is_only_what_the_mesher_reads() {
        assert!(std::mem::size_of::<BakedQuad>() <= 12);
    }

    impl BlockModels {
        /// The quads at the world's origin.
        fn quads(&self, state: u16) -> Vec<BakedQuad> {
            let mut all = Vec::new();
            self.quads_at(state, [0, 0, 0], |quads| all.extend_from_slice(quads));
            all
        }
    }

    #[test]
    fn particles_show_the_models_particle_texture() {
        let Some(root) = assets() else {
            eprintln!("skipped: no extracted assets");
            return;
        };
        let (models, _) = BlockModels::bake(&root, Vec::new(), 16384);
        let grass = models
            .particle_look(state_of(&Block::GRASS_BLOCK))
            .expect("grass block");
        assert_eq!(Some(grass.uv), models.sprite("minecraft:block/dirt"));
        assert_eq!(grass.tint, TintKind::None);
        assert_eq!(grass.rgb, [1.0; 3]);
        let oak = models
            .particle_look(state_of(&Block::OAK_LEAVES))
            .expect("oak leaves");
        assert_eq!(oak.tint, TintKind::Foliage);
        let spruce = models
            .particle_look(state_of(&Block::SPRUCE_LEAVES))
            .expect("spruce leaves");
        assert_eq!(spruce.tint, TintKind::None);
        assert_ne!(spruce.rgb, [1.0; 3]);
    }

    #[test]
    fn smooth_lighting_data_follows_the_models() {
        let Some(root) = assets() else {
            eprintln!("skipped: no extracted assets");
            return;
        };
        let (models, _) = BlockModels::bake(&root, Vec::new(), 16384);
        for state in 0..pumpkin_data::BlockStateId::COUNT {
            for quad in models.quads(state) {
                let shape = models.ao_shape(&quad);
                if let Some(facing) = quad.facing {
                    assert_eq!(shape.direction, facing, "state {state}: {shape:?}");
                }
            }
        }
        assert!(models.smooth_lighting(state_of(&Block::STONE)));
        assert!(!models.smooth_lighting(state_of(&Block::POPPY)));
        let poppy = models.quads(state_of(&Block::POPPY));
        assert!(
            poppy
                .iter()
                .all(|q| models.corners_of(q)[0].color[..3] == [255; 3])
        );
        let eyeblossom = models.quads(state_of(&Block::OPEN_EYEBLOSSOM));
        assert!(eyeblossom.iter().any(|q| q.emission == 15));
        assert!(eyeblossom.iter().any(|q| q.emission == 0));
    }

    #[test]
    fn weighted_variants_change_with_position() {
        let Some(root) = assets() else {
            eprintln!("skipped: no extracted assets");
            return;
        };
        let (models, _) = BlockModels::bake(&root, Vec::new(), 16384);
        let uvs_at = |state: u16, pos: [i32; 3]| {
            let mut uvs = Vec::new();
            models.quads_at(state, pos, |quads| {
                uvs.extend(quads.iter().flat_map(|q| {
                    models
                        .corners_of(q)
                        .map(|v| (v.pos.map(f32::to_bits), v.uv))
                }));
            });
            uvs
        };
        for block in [&Block::STONE, &Block::GRASS_BLOCK, &Block::SAND] {
            let state = state_of(block);
            let first = uvs_at(state, [0, 64, 0]);
            assert_eq!(first, uvs_at(state, [0, 64, 0]), "{}", block.name);
            let differs = (0..32).any(|x| uvs_at(state, [x, 64, 7]) != first);
            assert!(differs, "{} shows one variant everywhere", block.name);
        }
        let planks = state_of(&Block::OAK_PLANKS);
        assert!((0..32).all(|x| uvs_at(planks, [x, 1, 2]) == uvs_at(planks, [0, 0, 0])));
    }

    #[test]
    fn cubes_slabs_and_stairs_bake_with_cullfaces() {
        let Some(root) = assets() else {
            eprintln!("skipped: no extracted assets");
            return;
        };
        let (models, atlas) = BlockModels::bake(&root, Vec::new(), 16384);
        let (w, h) = atlas.image.dimensions();
        eprintln!("block atlas: {w}x{h}");
        assert!(w % mipmap::ALIGN == 0 && h % mipmap::ALIGN == 0);
        assert!(u64::from(w) * u64::from(h) < 1024 * 1024, "{w}x{h}");
        assert!(!atlas.animator.sprites.is_empty());
        let glass = models.quads(state_of(&Block::GLASS));
        assert!(glass.iter().all(|q| q.layer == ChunkLayer::Translucent));
        assert!(
            models
                .quads(state_of(&Block::STONE))
                .iter()
                .all(|q| q.layer == ChunkLayer::Solid)
        );
        assert!(
            models
                .quads(state_of(&Block::OAK_LEAVES))
                .iter()
                .all(|q| q.layer == ChunkLayer::Cutout)
        );

        let stone = models.quads(state_of(&Block::STONE));
        assert_eq!(stone.len(), 6);
        assert!(stone.iter().all(|q| q.cullface.is_some()));
        assert!(stone.iter().all(|q| q.facing == q.cullface));
        assert!(
            models
                .quads(state_of(&Block::SHORT_GRASS))
                .iter()
                .all(|q| q.facing.is_none())
        );
        assert!(models.quads(0).is_empty());

        let slab = models.quads(state_of(&Block::OAK_SLAB));
        assert_eq!(slab.len(), 6);
        let top = slab
            .iter()
            .find(|q| {
                models
                    .corners_of(q)
                    .iter()
                    .all(|v| (v.pos[1] - 0.5).abs() < 1e-4)
            })
            .expect("a top face at half height");
        assert_eq!(top.cullface, None);

        let stairs = models.quads(state_of(&Block::OAK_STAIRS));
        assert!(stairs.len() >= 10);
        for quad in stairs {
            for v in *models.corners_of(&quad) {
                let c = Vec3::from(v.pos);
                assert!(
                    c.min_element() > -1e-4 && c.max_element() < 1.0 + 1e-4,
                    "{c}"
                );
            }
        }

        let grass = models.quads(state_of(&Block::GRASS_BLOCK));
        assert!(grass.iter().any(|q| q.tint == TintKind::Grass));
        assert!(grass.iter().any(|q| q.tint == TintKind::None));
    }

    /// `FaceBakery`: a face's layer is read from the texels its `uv`
    /// shows, so one texture sheet can hold an opaque, a cut out and a
    /// see-through part.
    #[test]
    fn each_face_takes_the_layer_of_the_texels_it_shows() {
        use image::Rgba;
        // Columns 0-3 opaque, 4-7 holes, 8-11 half see-through,
        // 12-15 opaque.
        let mut sheet = RgbaImage::from_pixel(16, 16, Rgba([90, 90, 90, 255]));
        for y in 0..16 {
            for x in 4..8 {
                sheet.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
            for x in 8..12 {
                sheet.put_pixel(x, y, Rgba([200, 200, 255, 128]));
            }
        }
        let packed = pack_atlas(
            Path::new("/nonexistent"),
            vec![("test:block/sheet".to_owned(), Some(sheet))],
            256,
        );
        let quad = |u0: f32, u1: f32, alpha: Option<AlphaRule>| {
            PendingQuad {
                corners: [Vec3::ZERO, Vec3::Y, Vec3::ONE, Vec3::X],
                uvs: [
                    Vec2::new(u0, 0.0),
                    Vec2::new(u0, 1.0),
                    Vec2::new(u1, 1.0),
                    Vec2::new(u1, 0.0),
                ],
                texture: 0,
                color: [1.0; 3],
                shade_face: Direction::North,
                cullface: None,
                alpha: alpha.unwrap_or(AlphaRule::Region {
                    min: Vec2::new(u0, 0.0),
                    max: Vec2::new(u1, 1.0),
                }),
                emission: 0,
                ambient_occlusion: true,
                tint: TintKind::None,
            }
            .finish(&packed)
            .1
            .layer
        };
        assert_eq!(quad(0.0, 0.25, None), ChunkLayer::Solid);
        assert_eq!(quad(0.75, 1.0, None), ChunkLayer::Solid);
        assert_eq!(quad(0.25, 0.5, None), ChunkLayer::Cutout);
        assert_eq!(quad(0.5, 0.75, None), ChunkLayer::Translucent);
        assert_eq!(quad(0.0, 1.0, None), ChunkLayer::Translucent);
        assert_eq!(
            quad(0.0, 0.25, Some(AlphaRule::Translucent)),
            ChunkLayer::Translucent
        );
    }

    #[test]
    fn rotations_turn_cullfaces_with_the_model() {
        let rotation = variant_rotation(0, 270);
        assert_eq!(
            rotate_direction(rotation, Direction::East),
            Direction::North
        );
        let flipped = variant_rotation(180, 0);
        assert_eq!(rotate_direction(flipped, Direction::Up), Direction::Down);
    }

    #[test]
    fn multipart_conditions() {
        let props = [("north", "true"), ("east", "false")];
        let when = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        assert!(condition_holds(&when(r#"{"north":"true"}"#), &props));
        assert!(!condition_holds(&when(r#"{"east":"true"}"#), &props));
        assert!(condition_holds(
            &when(r#"{"OR":[{"east":"true"},{"north":"true|false"}]}"#),
            &props
        ));
        assert!(condition_holds(&Value::Null, &props));
    }
}
