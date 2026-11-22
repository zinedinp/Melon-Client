//! Section terrain meshes (vanilla `SectionCompiler`).

use std::ops::Range;

use crate::math::section_to_block;
use glam::Vec3;

use crate::renderer::{TerrainQuad, WorldVertex as Vertex};
use crate::world::block::direction::Direction;
use crate::world::block::offset;
use crate::world::block::skip_rendering;
use crate::world::block::state_flags;
use crate::world::block_models::{BlockModels, ChunkLayer};
use crate::world::block_tint::{TintKind, TintSettings};
use crate::world::chunk::{BLOCKS_PER_SECTION, Chunk, Section};
use crate::world::culling::visgraph::{self, VisibilitySet};
use crate::world::fluid::{self, FluidSprites};
use crate::world::smooth_light::{LightCache, flat_corners, smooth_corners};

/// A chunk and the eight around it.
pub struct Neighborhood<'a> {
    /// Row-major from north-west.
    pub chunks: [Option<&'a Chunk>; 9],
    /// The section being meshed, unpacked: `(index, blocks)`.
    flat: Option<(usize, &'a [u16; BLOCKS_PER_SECTION])>,
}

impl<'a> Neighborhood<'a> {
    /// The centre chunk alone, nothing around it.
    pub fn alone(center: &'a Chunk) -> Self {
        let mut chunks = [None; 9];
        chunks[4] = Some(center);
        Self { chunks, flat: None }
    }

    /// The nine chunks, centre at index 4, and an optional unpacked centre section.
    pub fn with_chunks(
        chunks: [Option<&'a Chunk>; 9],
        decoded: Option<(usize, &'a [u16; BLOCKS_PER_SECTION])>,
    ) -> Self {
        Self {
            chunks,
            flat: decoded,
        }
    }

    pub fn center(&self) -> &'a Chunk {
        self.chunks[4].expect("a neighbourhood has its centre")
    }

    /// The centre section unpacked, if [`mesh_section`] decoded it.
    pub fn decoded_center(&self) -> Option<(usize, &'a [u16; BLOCKS_PER_SECTION])> {
        self.flat
    }

    /// The block at chunk-local `(x, z)` (`-16..32`) and `y` relative to section `s`.
    #[inline]
    pub fn block(&self, s: usize, x: i32, y: i32, z: i32) -> Option<u16> {
        if let Some((flat_s, flat)) = self.flat
            && flat_s == s
            && ((x | y | z) as u32) < 16
        {
            return Some(flat[Section::index(x as usize, y as usize, z as usize)]);
        }
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        if !(-1..=1).contains(&cx) || !(-1..=1).contains(&cz) {
            return None;
        }
        let chunk = self.chunks[((cx + 1) + (cz + 1) * 3) as usize]?;
        let section = s as i32 + y.div_euclid(16);
        let Some(section) = usize::try_from(section)
            .ok()
            .and_then(|i| chunk.sections.get(i))
        else {
            return Some(0);
        };
        let (lx, ly, lz) = (x.rem_euclid(16), y.rem_euclid(16), z.rem_euclid(16));
        Some(section.get(Section::index(lx as usize, ly as usize, lz as usize)))
    }

    /// Whether the block there hides the faces against it.
    fn solid_at(&self, s: usize, x: i32, y: i32, z: i32) -> bool {
        self.block(s, x, y, z).is_some_and(state_flags::is_solid)
    }

    /// Block and sky light at section-relative `(x, y, z)` of section `s`.
    pub fn light(&self, s: usize, x: i32, y: i32, z: i32) -> (u8, u8) {
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        if !(-1..=1).contains(&cx) || !(-1..=1).contains(&cz) {
            return (0, 0);
        }
        let Some(chunk) = self.chunks[((cx + 1) + (cz + 1) * 3) as usize] else {
            return (0, 0);
        };
        let world_y = chunk.light.min_y() + (s as i32 + 1) * 16 + y;
        chunk.light.at(x, world_y, z)
    }

    /// Registry biome at a world position. A missing neighbour is plains (0).
    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> u16 {
        let center = self.center();
        let (dx, dz) = (x.div_euclid(16) - center.x, z.div_euclid(16) - center.z);
        if !(-1..=1).contains(&dx) || !(-1..=1).contains(&dz) {
            return 0;
        }
        self.chunks[((dx + 1) + (dz + 1) * 3) as usize]
            .map_or(0, |chunk| chunk.biomes.at(x, y - chunk.min_y(), z))
    }
}

/// Facing groups a layer's quads are sorted into.
pub const FACINGS: usize = 7;
/// The group of quads not along an axis (plants' crosses, fluid surfaces).
pub const SLANTED: usize = 6;

/// One layer of a section.
pub struct LayerMesh<T = TerrainQuad> {
    /// Quads are placed above [`SectionMesh::base_y`].
    pub items: Vec<T>,
    /// Per facing group, its slice of `items`.
    pub groups: [Range<u32>; FACINGS],
}

impl<T> Default for LayerMesh<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            groups: Default::default(),
        }
    }
}

/// A section's translucent faces, block faces and water together, as plain vertices.
#[derive(Clone, Default)]
pub struct TranslucentMesh {
    pub vertices: Vec<Vertex>,
    /// The eye (chunk-local, world Y) they were last sorted from.
    pub sorted_from: Vec3,
    /// The box around every quad's centroid, from the last sort; `None`
    pub centroids: Option<(Vec3, Vec3)>,
}

impl TranslucentMesh {
    pub fn quad_count(&self) -> usize {
        self.vertices.len() / 4
    }

    /// TODO(perf): reinvestigate. Copies every quad twice (into `keyed`,
    /// back into `vertices`) and runs a comparison sort. Sort `u32` indices
    /// by the keys' `f32` bits instead (radix sort, distances are never
    /// negative), then gather once; the squared distances vectorise.
    /// `VertexSorting.byDistance`: the quads far to near from `eye`
    pub fn sort(&mut self, eye: Vec3) {
        let (quads, _) = self.vertices.as_chunks::<4>();
        let (mut low, mut high) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        let mut keyed: Vec<(f32, [Vertex; 4])> = quads
            .iter()
            .map(|quad| {
                let centroid = quad_centroid(quad);
                (low, high) = (low.min(centroid), high.max(centroid));
                (centroid.distance_squared(eye), *quad)
            })
            .collect();
        keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.sorted_from = eye;
        self.centroids = Some((low, high));
        self.vertices.clear();
        self.vertices
            .extend(keyed.into_iter().flat_map(|(_, quad)| quad));
    }

    /// Whether `eye` and [`Self::sorted_from`] sit on opposite sides of any
    /// TODO(perf): recomputes every quad's centroid from the vertices each
    /// time. Keep the centroids as structure of arrays from the last sort,
    /// then test 8/16 at once (miraculix, as `section_cull`).
    pub fn camera_crossed_a_face(&self, eye: Vec3) -> bool {
        let delta = self.sorted_from - eye;
        if delta.abs().max_element() < 1e-5 {
            return false;
        }
        if let Some((low, high)) = self.centroids {
            let (from, to) = (self.sorted_from.min(eye), self.sorted_from.max(eye));
            if (0..3).all(|axis| high[axis] <= from[axis] || low[axis] >= to[axis]) {
                return false;
            }
        }
        let (quads, _) = self.vertices.as_chunks::<4>();
        quads.iter().any(|quad| {
            let c = quad_centroid(quad);
            (0..3).any(|axis| (self.sorted_from[axis] - c[axis]) * (eye[axis] - c[axis]) < 0.0)
        })
    }
}

/// `MeshData.decodeQuadCentroids`.
fn quad_centroid(quad: &[Vertex; 4]) -> Vec3 {
    (Vec3::from_array(quad[0].pos) + Vec3::from_array(quad[2].pos)) * 0.5
}

/// One section's terrain, ready to upload.
pub struct SectionMesh {
    pub solid: LayerMesh,
    pub cutout: LayerMesh,
    /// Lava, drawn with the solid layer.
    pub lava: LayerMesh<Vertex>,
    /// Water, glass, ice: blended, sorted far to near.
    pub translucent: TranslucentMesh,
    /// What can be seen through the section.
    pub visibility: VisibilitySet,
    /// The world's bottom: quads' y counts from here.
    pub base_y: i32,
}

impl Default for SectionMesh {
    /// Empty: nothing drawn, sight passes.
    fn default() -> Self {
        Self {
            solid: LayerMesh::default(),
            cutout: LayerMesh::default(),
            lava: LayerMesh::default(),
            translucent: TranslucentMesh::default(),
            visibility: VisibilitySet::ALL,
            base_y: 0,
        }
    }
}

impl SectionMesh {
    /// Nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.solid.items.is_empty()
            && self.cutout.items.is_empty()
            && self.lava.items.is_empty()
            && self.translucent.vertices.is_empty()
    }

    /// Vertices per layer (solid, cutout, translucent), four per quad.
    pub fn vertex_counts(&self) -> [usize; 3] {
        [
            self.solid.items.len() * 4 + self.lava.items.len(),
            self.cutout.items.len() * 4,
            self.translucent.vertices.len(),
        ]
    }

    /// Bytes the GPU holds for it.
    pub fn bytes(&self) -> usize {
        let quads = self.solid.items.len() + self.cutout.items.len();
        let vertices = self.lava.items.len() + self.translucent.vertices.len();
        quads * std::mem::size_of::<TerrainQuad>() + vertices * std::mem::size_of::<Vertex>()
    }
}

/// What meshing needs besides the blocks.
#[derive(Clone, Copy)]
pub struct MeshContext<'a> {
    pub models: &'a BlockModels,
    pub fluids: &'a FluidSprites,
    /// The world's bottom, where section 0 starts.
    pub min_y: i32,
    /// `Options.ambientOcclusion` ("Smooth Lighting").
    pub smooth_lighting: bool,
    /// The dimension's face shading relative to the default.
    pub cardinal: [f32; 6],
    /// Biome colours and the blend radius.
    pub tints: &'a TintSettings,
}

/// A layer being built: a list per facing group.
struct LayerBuilder<T> {
    groups: [Vec<T>; FACINGS],
}

impl<T> Default for LayerBuilder<T> {
    fn default() -> Self {
        Self {
            groups: Default::default(),
        }
    }
}

impl<T: Copy> LayerBuilder<T> {
    fn start(&mut self) {
        self.groups.iter_mut().for_each(Vec::clear);
    }

    /// One exactly sized list, the groups one after another.
    fn finish(&self) -> LayerMesh<T> {
        let total = self.groups.iter().map(Vec::len).sum();
        let mut items = Vec::with_capacity(total);
        let groups = std::array::from_fn(|g| {
            let start = items.len() as u32;
            items.extend_from_slice(&self.groups[g]);
            start..items.len() as u32
        });
        LayerMesh { items, groups }
    }
}

/// Block faces (solid, cutout), lava, the translucent faces and the light cache.
#[derive(Default)]
struct Builders {
    quads: [LayerBuilder<TerrainQuad>; 2],
    lava: LayerBuilder<Vertex>,
    translucent: Vec<Vertex>,
    light: LightCache,
}

impl Builders {
    fn start(&mut self) {
        self.quads.iter_mut().for_each(LayerBuilder::start);
        self.lava.start();
        self.translucent.clear();
    }
}

thread_local! {
    static BUILDERS: std::cell::RefCell<Builders> = std::cell::RefCell::default();
    /// The section being meshed, unpacked (see [`Neighborhood::block`]).
    static FLAT: std::cell::RefCell<Box<[u16; BLOCKS_PER_SECTION]>> =
        std::cell::RefCell::new(Box::new([0; BLOCKS_PER_SECTION]));
}

#[cfg(test)]
static PLAINS_TINTS: std::sync::OnceLock<TintSettings> = std::sync::OnceLock::new();

/// TODO(perf): the biggest CPU load left in the world. `perf_world`
/// (2026-11-15, 290 chunks, Ryzen 9800X3D): `mesh_chunk` 0.89 ms a chunk,
/// about 7200 quads; 0.70 ms with flat light. Filling the `LightCache`
/// (both modes) is ~12 µs a section, ~145 µs a chunk. Decoding and the
/// flag rows are 0.2 µs a section now, the visibility graph ~1%. Still to
/// split up, with a profiler: in `fill_section`'s quad loop the cullface
/// test (`neighborhood.block` + `skip_rendering::hides` when the solid
/// cache says no), `tints.color` per quad, `quads_at`'s closure. Here,
/// `LayerBuilder::finish` copies every quad once more and the translucent
/// vertices are cloned out of the builder. Meshes section `s` of
/// `neighborhood`'s centre chunk.
pub fn mesh_section(neighborhood: &Neighborhood, s: usize, context: &MeshContext) -> SectionMesh {
    let empty = || SectionMesh {
        base_y: context.min_y,
        ..SectionMesh::default()
    };
    let Some(section) = neighborhood.center().sections.get(s) else {
        return empty();
    };
    if is_empty(section) {
        return empty();
    }
    FLAT.with_borrow_mut(|flat| {
        let neighborhood = &if section.uniform().is_none() {
            section.copy_to(flat);
            Neighborhood {
                chunks: neighborhood.chunks,
                flat: Some((s, &**flat)),
            }
        } else {
            Neighborhood {
                chunks: neighborhood.chunks,
                flat: None,
            }
        };
        BUILDERS.with_borrow_mut(|builders| {
            builders.start();
            let rows = SectionRows::of(section, context.models);
            let visibility = visgraph::compute_rows(&rows.solid);
            fill_section(builders, neighborhood, context, s, &rows);
            let [solid, cutout] = builders.quads.each_ref().map(LayerBuilder::finish);
            SectionMesh {
                solid,
                cutout,
                lava: builders.lava.finish(),
                translucent: TranslucentMesh {
                    vertices: builders.translucent.clone(),
                    ..TranslucentMesh::default()
                },
                visibility,
                base_y: context.min_y,
            }
        })
    })
}

/// Every section of `neighborhood`'s centre chunk, bottom to top, unsorted:
pub fn mesh_chunk(neighborhood: &Neighborhood, context: &MeshContext) -> Vec<SectionMesh> {
    (0..neighborhood.center().sections.len())
        .map(|s| mesh_section(neighborhood, s, context))
        .collect()
}

/// What can be seen through each of a chunk's sections, bottom to top.
pub fn section_visibility(chunk: &Chunk) -> Vec<VisibilitySet> {
    chunk
        .sections
        .iter()
        .map(|section| {
            if is_empty(section) {
                return VisibilitySet::ALL;
            }
            let [solid] = section.flag_rows(|id| u8::from(state_flags::is_solid(id)));
            visgraph::compute_rows(&solid)
        })
        .collect()
}

/// All air (state 0): nothing to draw, nothing to stop sight.
pub fn is_empty(section: &Section) -> bool {
    section.is_empty()
}

/// A section's blocks as rows of x (index `y * 16 + z`, bit `x`).
struct SectionRows {
    /// Hides the faces against it, stops sight.
    solid: [u16; 256],
    /// Has something to draw, unless buried.
    draws: [u16; 256],
    /// Draws nothing with solid blocks on all six sides.
    buried_hidden: [u16; 256],
}

impl SectionRows {
    fn of(section: &Section, models: &BlockModels) -> Self {
        let [solid, draws, buried_hidden] = section.flag_rows(|id| {
            let flags = models.mesh_flags(id);
            u8::from(flags.solid())
                | u8::from(flags.draws()) << 1
                | u8::from(flags.buried_hidden()) << 2
        });
        Self {
            solid,
            draws,
            buried_hidden,
        }
    }
}

fn fill_section(
    builders: &mut Builders,
    neighborhood: &Neighborhood,
    context: &MeshContext,
    s: usize,
    rows: &SectionRows,
) {
    if rows.draws.iter().all(|&row| row == 0) {
        return;
    }
    let local_y = section_to_block(s as i32);
    let base_y = context.min_y + local_y;
    debug_assert!(local_y + 16 <= TerrainQuad::MAX_HEIGHT as i32);
    let center = neighborhood.center();
    let (world_x, world_z) = (center.x * 16, center.z * 16);
    let solid = SolidCache::new(neighborhood, s, &rows.solid);
    let mut tints = context
        .tints
        .sampler(|qx, qy, qz| neighborhood.biome_at(qx * 4, qy * 4, qz * 4));
    let Builders {
        quads: quad_builders,
        lava,
        translucent,
        light: light_cache,
    } = builders;
    light_cache.start(neighborhood, s);
    for y in 0..16 {
        for z in 0..16 {
            let row = (y * 16 + z) as usize;
            let mut todo =
                rows.draws[row] & !(solid.enclosed(y, z) as u16 & rows.buried_hidden[row]);
            while todo != 0 {
                let x = todo.trailing_zeros() as i32;
                todo &= todo - 1;
                let id = neighborhood.block(s, x, y, z).unwrap_or(0);
                let fluid = fluid::fluid_of(id);
                let flags = context.models.mesh_flags(id);
                let pos = [world_x + x, base_y + y, world_z + z];
                let offset = if flags.offset() {
                    offset::packed(id, pos[0], pos[2])
                } else {
                    0
                };
                let block = [x as u32, (local_y + y) as u32, z as u32];
                let here = [x, y, z];
                let center = light_cache.flags(here);
                let smooth = context.smooth_lighting
                    && center.emission() == 0
                    && context.models.smooth_lighting(id);
                context.models.quads_at(id, pos, |quads| {
                    for quad in quads {
                        let hidden = quad.cullface.is_some_and(|side| {
                            let [dx, dy, dz] = side.step();
                            solid.at(x + dx, y + dy, z + dz)
                                || neighborhood
                                    .block(s, x + dx, y + dy, z + dz)
                                    .is_some_and(|next| skip_rendering::hides(id, next, side))
                        });
                        if hidden {
                            continue;
                        }
                        let shape = context.models.ao_shape(quad);
                        let corners = if smooth {
                            smooth_corners(light_cache, shape, here, center, quad.emission)
                        } else {
                            let side = quad.cullface.or_else(|| {
                                shape
                                    .cubic(center.collision_full())
                                    .then_some(shape.direction)
                            });
                            let light_pos = side.map_or(here, |side| {
                                let [dx, dy, dz] = side.step();
                                [x + dx, y + dy, z + dz]
                            });
                            flat_corners(light_cache, light_pos, center, quad.emission)
                        };
                        let tint = tints.color(quad.tint, pos);
                        let terrain = TerrainQuad::tinted(quad.index, block, corners, offset, tint);
                        if quad.layer == ChunkLayer::Translucent {
                            translucent.extend(context.models.quad_vertices(
                                terrain,
                                [0, context.min_y, 0],
                                &context.cardinal,
                            ));
                            continue;
                        }
                        let layer = &mut quad_builders[quad.layer as usize];
                        let group = quad.facing.map_or(SLANTED, Direction::index);
                        layer.groups[group].push(terrain);
                    }
                });
                if let Some(fluid) = fluid {
                    let origin = [x as f32, (base_y + y) as f32, z as f32];
                    let water_tint = tints
                        .color(TintKind::Water, pos)
                        .map(|c| f32::from(c) / 255.0);
                    fluid::mesh(
                        neighborhood,
                        s,
                        [x, y, z],
                        origin,
                        id,
                        fluid,
                        context.fluids,
                        &context.cardinal,
                        water_tint,
                        &mut lava.groups[SLANTED],
                        translucent,
                    );
                }
            }
        }
    }
}

/// Which blocks of a section and the one-block shell around it hide the faces against them.
struct SolidCache {
    /// Indexed `(y + 1) * 18 + (z + 1)` for `y, z` in `-1..=16`; bit `x + 1`
    rows: [u32; PADDED * PADDED],
}

const PADDED: usize = 18;

impl SolidCache {
    /// `inside`.
    fn new(neighborhood: &Neighborhood, s: usize, inside: &[u16; 256]) -> Self {
        let mut rows = [0u32; PADDED * PADDED];
        for y in -1..=16i32 {
            for z in -1..=16i32 {
                let inside_yz = (0..16).contains(&y) && (0..16).contains(&z);
                let row = if inside_yz {
                    u32::from(inside[(y * 16 + z) as usize]) << 1
                        | u32::from(neighborhood.solid_at(s, -1, y, z))
                        | u32::from(neighborhood.solid_at(s, 16, y, z)) << 17
                } else {
                    (-1..=16i32).fold(0, |row, x| {
                        row | u32::from(neighborhood.solid_at(s, x, y, z)) << (x + 1)
                    })
                };
                rows[Self::row(y, z)] = row;
            }
        }
        Self { rows }
    }

    #[inline]
    fn row(y: i32, z: i32) -> usize {
        (y + 1) as usize * PADDED + (z + 1) as usize
    }

    /// Whether the block at section-local `(x, y, z)` (`-1..=16`) is solid.
    #[inline]
    fn at(&self, x: i32, y: i32, z: i32) -> bool {
        self.rows[Self::row(y, z)] >> (x + 1) & 1 != 0
    }

    /// Bit `x` set for each block of row `(y, z)` (`0..16`) with solid blocks on all six sides.
    #[inline]
    fn enclosed(&self, y: i32, z: i32) -> u32 {
        let here = self.rows[Self::row(y, z)];
        let west = here;
        let east = here >> 2;
        let around = [
            self.rows[Self::row(y - 1, z)],
            self.rows[Self::row(y + 1, z)],
            self.rows[Self::row(y, z - 1)],
            self.rows[Self::row(y, z + 1)],
        ];
        around.iter().fold(west & east, |acc, row| acc & (row >> 1)) & 0xFFFF
    }
}

/// The baked models and fluid sprites from `assets/`, once for every test.
#[cfg(test)]
pub(crate) fn test_models() -> Option<(&'static std::sync::Arc<BlockModels>, FluidSprites)> {
    use std::sync::{Arc, OnceLock};
    static MODELS: OnceLock<Option<(Arc<BlockModels>, FluidSprites)>> = OnceLock::new();
    MODELS
        .get_or_init(|| {
            let root = crate::assets::default_root();
            if !root.join("minecraft/blockstates/stone.json").exists() {
                return None;
            }
            let (models, _) = BlockModels::bake(&root, Vec::new(), 16384);
            let s = |id: &str| models.sprite(id).unwrap_or([0.0; 4]);
            let fluids = FluidSprites {
                water_still: s(FluidSprites::TEXTURES[0]),
                water_flow: s(FluidSprites::TEXTURES[1]),
                water_overlay: s(FluidSprites::TEXTURES[2]),
                lava_still: s(FluidSprites::TEXTURES[3]),
                lava_flow: s(FluidSprites::TEXTURES[4]),
            };
            Some((Arc::new(models), fluids))
        })
        .as_ref()
        .map(|(models, fluids)| (models, *fluids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::CornerLight;

    const STONE: u16 = 1;

    /// Every section of a chunk, meshed.
    struct Built {
        sections: Vec<SectionMesh>,
    }

    impl Built {
        fn solid(&self) -> Vec<TerrainQuad> {
            self.sections
                .iter()
                .flat_map(|s| s.solid.items.iter().copied())
                .collect()
        }

        fn cutout(&self) -> Vec<TerrainQuad> {
            self.sections
                .iter()
                .flat_map(|s| s.cutout.items.iter().copied())
                .collect()
        }

        fn translucent(&self) -> Vec<Vertex> {
            self.sections
                .iter()
                .flat_map(|s| s.translucent.vertices.iter().copied())
                .collect()
        }
    }

    fn build(neighborhood: &Neighborhood) -> Option<Built> {
        let (models, fluids) = test_models()?;
        let context = MeshContext {
            smooth_lighting: true,
            models,
            fluids: &fluids,
            min_y: 0,
            cardinal: [1.0; 6],
            tints: PLAINS_TINTS.get_or_init(TintSettings::plains),
        };
        Some(Built {
            sections: mesh_chunk(neighborhood, &context),
        })
    }

    fn chunk_with(blocks: &[(usize, usize, usize, u16)]) -> Chunk {
        chunk_with_light(blocks, crate::world::chunk::ChunkLight::default())
    }

    fn chunk_with_light(
        blocks: &[(usize, usize, usize, u16)],
        light: crate::world::chunk::ChunkLight,
    ) -> Chunk {
        let mut sections: Vec<Section> = (0..2).map(|_| Section::empty()).collect();
        for &(x, y, z, id) in blocks {
            sections[y / 16].set(Section::index(x, y % 16, z), id);
        }
        Chunk::new(0, 0, sections).with_light(light)
    }

    #[test]
    fn a_buried_block_adds_nothing_to_a_cube() {
        let mut blocks = Vec::new();
        for y in 14..17 {
            for z in 4..7 {
                for x in 4..7 {
                    blocks.push((x, y, z, STONE));
                }
            }
        }
        let cube = chunk_with(&blocks);
        let Some(mesh) = build(&Neighborhood::alone(&cube)) else {
            return;
        };
        assert_eq!(mesh.solid().len(), 54);
    }

    #[test]
    fn touching_blocks_hide_the_faces_between_them() {
        let lone = chunk_with(&[(4, 4, 4, STONE)]);
        let Some(mesh) = build(&Neighborhood::alone(&lone)) else {
            return;
        };
        assert_eq!(mesh.solid().len(), 6);

        let pair = chunk_with(&[(4, 15, 4, STONE), (4, 16, 4, STONE)]);
        let mesh = build(&Neighborhood::alone(&pair)).unwrap();
        assert_eq!(mesh.solid().len(), 10);
        assert_eq!(mesh.sections[0].solid.items.len(), 5);
        let up = Direction::Up.index();
        assert!(mesh.sections[0].solid.groups[up].is_empty());
        assert_eq!(mesh.sections[1].solid.groups[up].len(), 1);
    }

    #[test]
    fn a_loaded_neighbor_hides_border_faces() {
        let edge = chunk_with(&[(15, 4, 4, STONE)]);
        let beside = chunk_with(&[(0, 4, 4, STONE)]);
        let Some(mesh) = build(&Neighborhood::alone(&edge)) else {
            return;
        };
        assert_eq!(mesh.solid().len(), 6);
        let mut joined = Neighborhood::alone(&edge);
        joined.chunks[5] = Some(&beside);
        assert_eq!(build(&joined).unwrap().solid().len(), 5);
    }

    #[test]
    fn water_is_translucent_and_its_surface_sits_below_the_top() {
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let pool = chunk_with(&[(4, 4, 4, water), (5, 4, 4, water), (4, 3, 4, STONE)]);
        let Some(mesh) = build(&Neighborhood::alone(&pool)) else {
            return;
        };
        let water = mesh.translucent();
        assert!(!water.is_empty());
        let top = water.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max);
        let corner = (10.0 * 8.0 / 9.0 * 2.0) / 22.0;
        assert!((top - (4.0 + corner - 0.001)).abs() < 1e-3, "{top}");
        assert_eq!(water.len() / 4, 2 * 2 + 1 + 2 * 3 * 2);
    }

    #[test]
    fn fluids_take_the_brighter_light_above_or_below() {
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let mut light = crate::world::chunk::ChunkLight::default();
        let world_y = |y: i32| crate::world::dimension::Height::OVERWORLD.light_min_y() + 16 + y;
        light.set_layer(false, 4, world_y(5), 4, 12);
        light.set_layer(false, 4, world_y(3), 4, 9);
        let pool = chunk_with_light(&[(4, 4, 4, water)], light);
        let Some(mesh) = build(&Neighborhood::alone(&pool)) else {
            return;
        };
        let vertices = mesh.translucent();
        let bottom = vertices
            .iter()
            .filter(|v| v.pos[1] < 4.01)
            .map(|v| v.light[0])
            .collect::<Vec<_>>();
        let top = vertices
            .iter()
            .filter(|v| v.pos[1] > 4.5)
            .map(|v| v.light[0])
            .collect::<Vec<_>>();
        assert!(
            !top.is_empty() && top.iter().all(|&l| l == 12 << 4),
            "{top:?}"
        );
        assert!(bottom.contains(&(9 << 4)), "{bottom:?}");
        assert!(bottom.iter().all(|&l| l == 9 << 4 || l == 12 << 4));
    }

    /// The corners of `block`'s top face as (x, z) -> AO in sixtieths.
    fn top_ao(mesh: &Built, block: [i32; 3]) -> Vec<((i32, i32), u8)> {
        let (models, _) = test_models().unwrap();
        let [bx, by, bz] = block;
        let top = (by + 1) as f32;
        let mut out = Vec::new();
        for quad in mesh.solid() {
            let vertices = models.quad_vertices(quad, [0, 0, 0], &[1.0; 6]);
            let on_top = vertices.iter().all(|v| {
                v.pos[1] == top
                    && (bx as f32..=(bx + 1) as f32).contains(&v.pos[0])
                    && (bz as f32..=(bz + 1) as f32).contains(&v.pos[2])
            });
            if on_top {
                for (i, v) in vertices.iter().enumerate() {
                    out.push(((v.pos[0] as i32, v.pos[2] as i32), quad.corner(i).ao));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn smooth_lighting_darkens_corners_like_vanilla() {
        let lit = crate::world::chunk::ChunkLight::filled_sky;
        let chunk = chunk_with_light(
            &[(4, 4, 4, STONE), (5, 5, 4, STONE), (4, 5, 3, STONE)],
            lit(15),
        );
        let Some(mesh) = build(&Neighborhood::alone(&chunk)) else {
            return;
        };
        assert_eq!(
            top_ao(&mesh, [4, 4, 4]),
            vec![((4, 4), 48), ((4, 5), 60), ((5, 4), 36), ((5, 5), 48)]
        );

        let chunk = chunk_with_light(
            &[
                (4, 4, 4, STONE),
                (5, 5, 4, STONE),
                (4, 5, 3, STONE),
                (5, 6, 4, STONE),
                (4, 6, 3, STONE),
            ],
            lit(15),
        );
        let mesh = build(&Neighborhood::alone(&chunk)).unwrap();
        assert_eq!(top_ao(&mesh, [4, 4, 4])[2], ((5, 4), 24));
    }

    #[test]
    fn faces_at_section_edges_look_two_blocks_out() {
        let mut blocks = Vec::new();
        for &x in &[0, 15] {
            for &y in &[0, 15, 16] {
                for &z in &[0, 15] {
                    blocks.push((x, y, z, STONE));
                }
            }
        }
        let chunk = chunk_with_light(&blocks, crate::world::chunk::ChunkLight::filled_sky(15));
        let Some(mesh) = build(&Neighborhood::alone(&chunk)) else {
            return;
        };
        assert!(!mesh.solid().is_empty());
    }

    #[test]
    fn glowing_and_open_blocks_stay_flat() {
        let lit = crate::world::chunk::ChunkLight::filled_sky;
        let glowstone = pumpkin_data::Block::GLOWSTONE.default_state.id.as_u16();
        let chunk = chunk_with_light(&[(4, 4, 4, glowstone), (5, 5, 4, STONE)], lit(15));
        let Some(mesh) = build(&Neighborhood::alone(&chunk)) else {
            return;
        };
        assert!(top_ao(&mesh, [4, 4, 4]).iter().all(|&(_, ao)| ao == 60));
        let chunk = chunk_with_light(&[(4, 4, 4, STONE)], lit(15));
        let mesh = build(&Neighborhood::alone(&chunk)).unwrap();
        assert!(top_ao(&mesh, [4, 4, 4]).iter().all(|&(_, ao)| ao == 60));
    }

    #[test]
    fn culled_faces_sample_neighbour_sky_light() {
        let stone = chunk_with_light(
            &[(4, 4, 4, STONE)],
            crate::world::chunk::ChunkLight::filled_sky(15),
        );
        let Some(mesh) = build(&Neighborhood::alone(&stone)) else {
            return;
        };
        assert!(
            mesh.solid()
                .iter()
                .all(|q| (0..4).all(|i| q.corner(i) == CornerLight::flat(0, 15))),
            "surface faces should sample the lit air beside them: {:?}",
            mesh.solid()
                .iter()
                .map(|q| (0..4).map(|i| q.corner(i)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );

        let cave = chunk_with(&[(4, 4, 4, STONE)]);
        let mesh = build(&Neighborhood::alone(&cave)).unwrap();
        assert!(
            mesh.solid()
                .iter()
                .all(|q| q.corner(0) == CornerLight::flat(0, 0)),
            "unlit interiors stay at light 0"
        );
    }

    #[test]
    fn quads_rebuild_the_vertices_they_stand_for() {
        let poppy = pumpkin_data::Block::POPPY.default_state.id.as_u16();
        let chunk = chunk_with(&[(4, 20, 9, STONE), (7, 3, 2, poppy)]);
        let Some(mesh) = build(&Neighborhood::alone(&chunk)) else {
            return;
        };
        let (models, _) = test_models().unwrap();
        for (id, [x, y, z], layer) in [
            (STONE, [4, 20, 9], mesh.solid()),
            (poppy, [7, 3, 2], mesh.cutout()),
        ] {
            let [ox, oy, oz] = offset::offset(id, x, z);
            let mut expected = Vec::new();
            models.quads_at(id, [x, y, z], |quads| {
                for quad in quads {
                    expected.push(models.corners_of(quad).map(|v| {
                        [
                            v.pos[0] + x as f32 + ox,
                            v.pos[1] + y as f32 + oy,
                            v.pos[2] + z as f32 + oz,
                        ]
                    }));
                }
            });
            let mut got: Vec<_> = layer
                .iter()
                .map(|&q| models.quad_vertices(q, [0, 0, 0], &[1.0; 6]).map(|v| v.pos))
                .collect();
            let key = |c: &[[f32; 3]; 4]| c.map(|p| p.map(|v| (v * 1024.0).round() as i32));
            expected.sort_by_key(key);
            got.sort_by_key(key);
            assert_eq!(got.len(), expected.len(), "state {id}");
            for (a, b) in got.iter().zip(&expected) {
                for (p, q) in a.iter().flatten().zip(b.iter().flatten()) {
                    assert!((p - q).abs() < 1e-5, "state {id}: {a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn grass_uses_the_biome_colormap() {
        let Some((models, fluids)) = test_models() else {
            return;
        };
        let grass = pumpkin_data::Block::GRASS_BLOCK.default_state.id.as_u16();
        let biomes = std::sync::Arc::new(crate::world::biome::Biomes::new(vec![
            (
                "minecraft:plains".into(),
                crate::world::biome::Biome::default(),
            ),
            (
                "minecraft:desert".into(),
                crate::world::biome::Biome {
                    temperature: 2.0,
                    downfall: 0.0,
                    ..crate::world::biome::Biome::default()
                },
            ),
        ]));
        let tints = TintSettings::new(
            &biomes,
            crate::world::colormap::ColorMaps::cached(),
            0,
            None,
        );
        let chunk = chunk_with(&[(4, 4, 4, grass)])
            .with_biomes(crate::world::biome::ChunkBiomes::uniform(1, 2));
        let context = MeshContext {
            smooth_lighting: true,
            models,
            fluids: &fluids,
            min_y: chunk.min_y(),
            cardinal: [1.0; 6],
            tints: &tints,
        };
        let mesh = mesh_section(&Neighborhood::alone(&chunk), 0, &context);
        let desert = crate::world::colormap::unpack(
            crate::world::colormap::ColorMaps::cached()
                .grass
                .get(2.0, 0.0),
        );
        let plains = crate::world::colormap::unpack(
            crate::world::colormap::ColorMaps::cached()
                .grass
                .get(0.8, 0.4),
        );
        assert_ne!(desert, plains);
        assert!(
            mesh.solid.items.iter().any(|q| q.tint() == desert),
            "desert grass should tint the top, got {:?}",
            mesh.solid
                .items
                .iter()
                .map(|q| q.tint())
                .collect::<Vec<_>>()
        );
        assert!(mesh.solid.items.iter().all(|q| q.tint() != plains));
    }

    #[test]
    fn glass_and_water_share_one_list_sorted_far_to_near() {
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let glass = pumpkin_data::Block::GLASS.default_state.id.as_u16();
        let chunk = chunk_with(&[(4, 4, 4, water), (4, 5, 4, glass)]);
        let Some(mut mesh) = build(&Neighborhood::alone(&chunk)) else {
            return;
        };
        let is_glass = |quad: &[Vertex]| quad.iter().all(|v| v.pos[1] >= 5.0);
        assert!(mesh.translucent().chunks(4).any(is_glass));
        assert!(mesh.translucent().chunks(4).any(|q| !is_glass(q)));
        let order = |section: &mut SectionMesh, eye: Vec3| {
            section.translucent.sort(eye);
            section
                .translucent
                .vertices
                .chunks(4)
                .map(is_glass)
                .collect::<Vec<_>>()
        };
        let section = &mut mesh.sections[0];
        let above = order(section, Vec3::new(4.5, 20.0, 4.5));
        assert!(!above[0] && *above.last().unwrap(), "{above:?}");
        let below = order(section, Vec3::new(4.5, -10.0, 4.5));
        assert!(below[0] && !*below.last().unwrap(), "{below:?}");
    }

    #[test]
    fn sorting_keeps_both_sides_of_a_water_face_together() {
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let chunk = chunk_with(&[(4, 4, 4, water)]);
        let Some(mut mesh) = build(&Neighborhood::alone(&chunk)) else {
            return;
        };
        let before = mesh.sections[0].translucent.quad_count();
        mesh.sections[0].translucent.sort(Vec3::new(0.0, 8.0, 0.0));
        let after = &mesh.sections[0].translucent;
        assert_eq!(after.quad_count(), before);
        for quad in after.vertices.chunks(4) {
            let planar = (0..3).any(|axis| quad.iter().all(|v| v.pos[axis] == quad[0].pos[axis]));
            let sloped_top = quad.iter().all(|v| v.pos[1] > 4.5);
            assert!(planar || sloped_top, "{quad:?}");
        }
    }

    #[test]
    fn water_under_a_block_shows_its_surface_from_below_beside_open_water() {
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let dirt = pumpkin_data::Block::DIRT.default_state.id.as_u16();
        let below = |blocks: &[(usize, usize, usize, u16)]| {
            let chunk = chunk_with(blocks);
            let mesh = build(&Neighborhood::alone(&chunk))?;
            Some(
                mesh.translucent()
                    .chunks(4)
                    .filter(|q| {
                        q.iter().all(|v| {
                            v.pos[1] > 4.5
                                && (4.0..=5.0).contains(&v.pos[0])
                                && (4.0..=5.0).contains(&v.pos[2])
                        })
                    })
                    .count(),
            )
        };
        let mut blocks = vec![(4, 4, 4, water), (4, 5, 4, dirt)];
        let Some(open) = below(&blocks) else {
            return;
        };
        assert_eq!(open, 2);
        for x in 3..6 {
            for z in 3..6 {
                blocks.push((x, 5, z, dirt));
            }
        }
        assert_eq!(below(&blocks), Some(1));
    }

    #[test]
    fn water_under_ice_keeps_its_underside_and_sorts_in_front_from_below() {
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let ice = pumpkin_data::Block::ICE.default_state.id.as_u16();
        let glass = pumpkin_data::Block::GLASS.default_state.id.as_u16();
        let cover = |block: u16| {
            let mut blocks = vec![(4, 4, 4, water)];
            for x in 3..6 {
                for z in 3..6 {
                    blocks.push((x, 5, z, block));
                }
            }
            let chunk = chunk_with(&blocks);
            let mesh = build(&Neighborhood::alone(&chunk))?;
            Some(
                mesh.translucent()
                    .chunks(4)
                    .filter(|q| {
                        q.iter().all(|v| {
                            v.pos[1] > 4.5
                                && v.pos[1] < 5.0
                                && (4.0..=5.0).contains(&v.pos[0])
                                && (4.0..=5.0).contains(&v.pos[2])
                        })
                    })
                    .count(),
            )
        };
        let Some(ice_faces) = cover(ice) else {
            return;
        };
        assert_eq!(ice_faces, 2);
        assert_eq!(cover(glass), Some(2));

        let chunk = chunk_with(&[(4, 4, 4, water), (4, 5, 4, ice)]);
        let mut mesh = build(&Neighborhood::alone(&chunk)).unwrap();
        let is_cover = |q: &[Vertex]| q.iter().all(|v| v.pos[1] >= 5.0);
        let is_water_top = |q: &[Vertex]| {
            q.iter()
                .all(|v| v.pos[1] > 4.5 && v.pos[1] < 5.0 && (4.0..=5.0).contains(&v.pos[0]))
        };
        let order = |section: &mut SectionMesh, eye: Vec3| {
            section.translucent.sort(eye);
            let quads: Vec<_> = section.translucent.vertices.chunks(4).collect();
            (
                quads.iter().position(|q| is_cover(q)),
                quads.iter().position(|q| is_water_top(q)),
            )
        };
        let section = &mut mesh.sections[0];
        let (ice_i, water_i) = order(section, Vec3::new(4.5, 3.0, 4.5));
        assert!(
            ice_i.is_some() && water_i.is_some() && ice_i < water_i,
            "from below: ice {ice_i:?} then water {water_i:?}"
        );
        let (ice_i, water_i) = order(section, Vec3::new(4.5, 8.0, 4.5));
        assert!(
            ice_i.is_some() && water_i.is_some() && water_i < ice_i,
            "from above: water {water_i:?} then ice {ice_i:?}"
        );
    }

    #[test]
    fn camera_crossed_a_face_sees_the_water_plane_in_the_same_block() {
        let quad = |y: f32| {
            [[0.0, y, 0.0], [0.0, y, 1.0], [1.0, y, 1.0], [1.0, y, 0.0]]
                .map(|p| Vertex::new(p, [0.0, 0.0], [1.0; 3]))
        };
        let mut faces = TranslucentMesh {
            vertices: [quad(8.888_889), quad(9.0)].concat(),
            ..TranslucentMesh::default()
        };
        faces.sort(Vec3::new(0.5, 8.95, 0.5));
        assert!(faces.camera_crossed_a_face(Vec3::new(0.5, 8.5, 0.5)));
        assert!(!faces.camera_crossed_a_face(Vec3::new(0.5, 8.96, 0.5)));
        assert!(!faces.camera_crossed_a_face(Vec3::new(0.5, 8.95, 0.5)));
        faces.sort(Vec3::new(0.5, 8.5, 0.5));
        assert!(!faces.camera_crossed_a_face(Vec3::new(0.5, 8.4, 0.5)));
        assert!(faces.camera_crossed_a_face(Vec3::new(0.5, 8.95, 0.5)));
        faces.sort(Vec3::new(40.0, 30.0, -20.0));
        assert!(!faces.camera_crossed_a_face(Vec3::new(45.0, 31.0, -25.0)));
        let unsorted = TranslucentMesh {
            vertices: faces.vertices.clone(),
            sorted_from: Vec3::new(0.5, 8.95, 0.5),
            centroids: None,
        };
        assert!(unsorted.camera_crossed_a_face(Vec3::new(0.5, 8.5, 0.5)));
    }

    #[test]
    fn the_centroid_box_never_hides_a_crossing() {
        let mut seed = 0x1234_5678_9abc_def1u64;
        let mut random = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 2000) as f32 / 100.0 - 2.0
        };
        for _ in 0..500 {
            let vertices: Vec<Vertex> = (0..12)
                .map(|_| Vertex::new([random(), random(), random()], [0.0, 0.0], [1.0; 3]))
                .collect();
            let mut faces = TranslucentMesh {
                vertices,
                ..TranslucentMesh::default()
            };
            faces.sort(Vec3::new(random(), random(), random()));
            let eye = Vec3::new(random(), random(), random());
            let full = TranslucentMesh {
                centroids: None,
                ..faces.clone()
            };
            assert_eq!(
                faces.camera_crossed_a_face(eye),
                full.camera_crossed_a_face(eye)
            );
        }
    }
}
