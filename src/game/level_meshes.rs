//! The level's terrain on the GPU (vanilla `LevelRenderer`'s section meshes).

use std::collections::{HashSet, VecDeque};
use std::ops::Range;

use glam::{DVec3, IVec3, Vec3};

use melon_core::math::{ChunkMap, SECTION_SIZE, radix, section_coord};
use melon_render::{
    ChunkDraw, IndexMeshId, MeshId, QuadMeshId, Renderer, SectionCull, SectionDraw, SectionRecord,
    TerrainMesh,
};
use melon_world::block::direction::Direction;
use melon_world::block_entity::BlockEntities;
use melon_world::block_models::BlockModels;
use melon_world::culling::Culler;
use melon_world::culling::occlusion::SectionPos;
use melon_world::dimension::Height;
use melon_world::level::{FinishedMesh, Level};
use melon_world::mesh::TerrainQuad;
use melon_world::mesh::WorldVertex;
use melon_world::terrain::{self, LayerMesh, SectionMesh, TranslucentMesh};

/// How many mesh bytes go to the GPU per frame at most.
const UPLOAD_BUDGET: usize = 16 << 20;
/// `Octree.isClose`'s 32 blocks.
const NEARBY_SECTIONS: i32 = 2;
/// Translucent quads sorted again per frame at most, nearest sections first.
const RESORT_BUDGET: usize = 32 << 10;
/// Sections per column the `u32` masks hold; ones above are not drawn (a level over 512 blocks tall).
const MASK_SECTIONS: usize = u32::BITS as usize;

/// Where section meshes go.
trait MeshSink {
    fn quads(&mut self, quads: &[TerrainQuad]) -> anyhow::Result<QuadMeshId>;
    fn vertices(&mut self, vertices: &[WorldVertex]) -> anyhow::Result<MeshId>;
    fn indices(&mut self, list: &[u32]) -> anyhow::Result<IndexMeshId>;
    /// Frees a mesh, both lists of a sorted one.
    fn free(&mut self, mesh: TerrainMesh);
    fn free_indices(&mut self, list: IndexMeshId);
    /// The quad block `quads` lies in ([`Renderer::quad_block`]).
    fn quad_block(&self, quads: QuadMeshId) -> u16;
    /// Keeps a section's record for the GPU's expansion ([`Renderer::set_section_record`]).
    fn record(&mut self, slot: u32, record: &SectionRecord);
}

impl MeshSink for Renderer {
    fn quads(&mut self, quads: &[TerrainQuad]) -> anyhow::Result<QuadMeshId> {
        self.upload_quads(quads)
    }

    fn vertices(&mut self, vertices: &[WorldVertex]) -> anyhow::Result<MeshId> {
        self.upload_mesh(vertices)
    }

    fn indices(&mut self, list: &[u32]) -> anyhow::Result<IndexMeshId> {
        self.upload_indices(list)
    }

    fn free(&mut self, mesh: TerrainMesh) {
        match mesh {
            TerrainMesh::Quads(id) => self.free_quads(id),
            TerrainMesh::Vertices(id) => self.free_mesh(id),
            TerrainMesh::Sorted { vertices, indices } => {
                self.free_mesh(vertices);
                self.free_indices(indices);
            }
        }
    }

    fn free_indices(&mut self, list: IndexMeshId) {
        Renderer::free_indices(self, list);
    }

    fn quad_block(&self, quads: QuadMeshId) -> u16 {
        Renderer::quad_block(self, quads).unwrap_or(0)
    }

    fn record(&mut self, slot: u32, record: &SectionRecord) {
        self.set_section_record(slot, record);
    }
}

/// Hands out fresh ids and keeps nothing.
struct Headless<F>(F);

impl<F: FnMut() -> usize> MeshSink for Headless<F> {
    fn quads(&mut self, _: &[TerrainQuad]) -> anyhow::Result<QuadMeshId> {
        Ok(QuadMeshId((self.0)()))
    }

    fn vertices(&mut self, _: &[WorldVertex]) -> anyhow::Result<MeshId> {
        Ok(MeshId((self.0)()))
    }

    fn indices(&mut self, _: &[u32]) -> anyhow::Result<IndexMeshId> {
        Ok(IndexMeshId((self.0)()))
    }

    fn free(&mut self, _: TerrainMesh) {}

    fn free_indices(&mut self, _: IndexMeshId) {}

    fn quad_block(&self, _: QuadMeshId) -> u16 {
        0
    }

    fn record(&mut self, _: u32, _: &SectionRecord) {}
}

/// A section's solid or cutout quads on the GPU, by facing group.
struct QuadLayer {
    mesh: QuadMeshId,
    groups: [Range<u32>; terrain::FACINGS],
    /// The quad block `mesh` lies in.
    block: u16,
}

impl QuadLayer {
    fn ends(&self) -> [u32; terrain::FACINGS] {
        self.groups.clone().map(|group| group.end)
    }
}

/// A section's translucent faces on the GPU, and the list they came from.
struct Translucent {
    /// Uploaded once; a sort uploads only the index list.
    mesh: MeshId,
    indices: IndexMeshId,
    faces: TranslucentMesh,
    /// `TranslucencyPointOfView`.
    view: IVec3,
    /// The camera's block when they were sorted.
    block: IVec3,
}

/// One section's meshes; `None` for a layer with nothing to draw.
#[derive(Default)]
struct SectionMeshes {
    solid: Option<QuadLayer>,
    cutout: Option<QuadLayer>,
    /// Lava vertices and their count.
    lava: Option<(MeshId, u32)>,
    translucent: Option<Translucent>,
}

impl SectionMeshes {
    /// Uploads every layer; on a failure frees what went up.
    fn upload(
        pos: SectionPos,
        built: SectionMesh,
        sink: &mut impl MeshSink,
    ) -> anyhow::Result<Self> {
        let mut out = Self::default();
        let result = out.fill(pos, built, sink);
        match result {
            Ok(()) => Ok(out),
            Err(e) => {
                out.free(sink);
                Err(e)
            }
        }
    }

    fn fill(
        &mut self,
        pos: SectionPos,
        built: SectionMesh,
        sink: &mut impl MeshSink,
    ) -> anyhow::Result<()> {
        let quads = |layer: LayerMesh, sink: &mut _| -> anyhow::Result<Option<QuadLayer>> {
            if layer.items.is_empty() {
                return Ok(None);
            }
            let mesh = MeshSink::quads(sink, &layer.items)?;
            Ok(Some(QuadLayer {
                mesh,
                groups: layer.groups,
                block: MeshSink::quad_block(sink, mesh),
            }))
        };
        self.solid = quads(built.solid, sink)?;
        self.cutout = quads(built.cutout, sink)?;
        if !built.lava.items.is_empty() {
            self.lava = Some((
                sink.vertices(&built.lava.items)?,
                built.lava.items.len() as u32,
            ));
        }
        let mut faces = built.translucent;
        if !faces.vertices.is_empty() {
            if faces.indices.is_empty() {
                faces.sort(faces.sorted_from);
            }
            let eye = faces.sorted_from.as_dvec3() + terrain::section_corner(pos, built.base_y);
            let mesh = sink.vertices(&faces.vertices)?;
            let indices = match sink.indices(&faces.indices) {
                Ok(indices) => indices,
                Err(e) => {
                    sink.free(TerrainMesh::Vertices(mesh));
                    return Err(e);
                }
            };
            self.translucent = Some(Translucent {
                mesh,
                indices,
                view: point_of_view(section_of_eye(eye, built.base_y), pos),
                block: eye.floor().as_ivec3(),
                faces,
            });
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.solid.is_none()
            && self.cutout.is_none()
            && self.lava.is_none()
            && self.translucent.is_none()
    }

    fn free(self, sink: &mut impl MeshSink) {
        for layer in [self.solid, self.cutout].into_iter().flatten() {
            sink.free(TerrainMesh::Quads(layer.mesh));
        }
        if let Some((mesh, _)) = self.lava {
            sink.free(TerrainMesh::Vertices(mesh));
        }
        if let Some(translucent) = self.translucent {
            sink.free(TerrainMesh::Sorted {
                vertices: translucent.mesh,
                indices: translucent.indices,
            });
        }
    }
}

/// A section layer's quads as the culling pass reads them: the mesh and
/// where each facing group ends (they follow one another from 0).
#[derive(Clone, Copy)]
struct QuadRuns {
    mesh: u32,
    ends: [u32; terrain::FACINGS],
    /// The quad block the mesh lies in.
    block: u16,
}

/// No mesh: the layer has nothing to draw.
const NO_MESH: u32 = u32::MAX;

impl QuadRuns {
    const NONE: Self = Self {
        mesh: NO_MESH,
        ends: [0; terrain::FACINGS],
        block: 0,
    };

    fn of(layer: Option<&QuadLayer>) -> Self {
        layer.map_or(Self::NONE, |layer| Self {
            mesh: mesh_index(layer.mesh.0),
            ends: layer.ends(),
            block: layer.block,
        })
    }
}

/// What drawing a section reads of its [`SectionMeshes`], packed in 96
/// bytes beside its neighbours: the culling pass touches a line or two
/// per section instead of the whole struct and its translucent faces.
#[derive(Clone, Copy)]
struct SectionDraws {
    solid: QuadRuns,
    cutout: QuadRuns,
    /// Mesh and vertex count, [`NO_MESH`] for none.
    lava: (u32, u32),
    /// Mesh, index list and vertex count, [`NO_MESH`] for none.
    translucent: (u32, u32, u32),
    /// Its occlusion culling slot ([`SectionCull::slot`]) while it has solid
    /// or cutout quads, [`NO_MESH`] otherwise.
    slot: u32,
}

impl Default for SectionDraws {
    fn default() -> Self {
        Self {
            solid: QuadRuns::NONE,
            cutout: QuadRuns::NONE,
            lava: (NO_MESH, 0),
            translucent: (NO_MESH, NO_MESH, 0),
            slot: NO_MESH,
        }
    }
}

impl SectionDraws {
    fn of(meshes: &SectionMeshes) -> Self {
        Self {
            solid: QuadRuns::of(meshes.solid.as_ref()),
            cutout: QuadRuns::of(meshes.cutout.as_ref()),
            lava: meshes
                .lava
                .map_or((NO_MESH, 0), |(mesh, count)| (mesh_index(mesh.0), count)),
            translucent: meshes
                .translucent
                .as_ref()
                .map_or((NO_MESH, NO_MESH, 0), |t| {
                    (
                        mesh_index(t.mesh.0),
                        mesh_index(t.indices.0),
                        t.faces.vertices.len() as u32,
                    )
                }),
            slot: NO_MESH,
        }
    }

    fn has_quads(&self) -> bool {
        self.solid.mesh != NO_MESH || self.cutout.mesh != NO_MESH
    }
}

/// Hands out [`SectionCull::slot`]s, the freed ones first. A slot handed
/// out again keeps its last section's visibility: that only decides which
/// culling phase draws the section first, never whether it is drawn.
#[derive(Default)]
struct Slots {
    /// One past the highest slot handed out.
    next: u32,
    free: Vec<u32>,
}

impl Slots {
    fn take(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            self.next += 1;
            self.next - 1
        })
    }

    fn give_back(&mut self, slot: u32) {
        if slot != NO_MESH {
            self.free.push(slot);
        }
    }
}

/// A mesh id as [`SectionDraws`] keeps it.
fn mesh_index(id: usize) -> u32 {
    debug_assert!(id < NO_MESH as usize);
    id as u32
}

/// The columns in one dense list, found by position through `index`: the
/// culling pass walks the list instead of a hash map's buckets.
#[derive(Default)]
struct Columns {
    index: ChunkMap<u32>,
    keys: Vec<(i32, i32)>,
    list: Vec<Column>,
}

impl Columns {
    fn get(&self, key: &(i32, i32)) -> Option<&Column> {
        self.index.get(key).map(|&i| &self.list[i as usize])
    }

    fn get_mut(&mut self, key: &(i32, i32)) -> Option<&mut Column> {
        self.index.get(key).map(|&i| &mut self.list[i as usize])
    }

    fn entry(&mut self, key: (i32, i32)) -> &mut Column {
        let i = *self.index.entry(key).or_insert_with(|| {
            self.keys.push(key);
            self.list.push(Column::default());
            (self.list.len() - 1) as u32
        });
        &mut self.list[i as usize]
    }

    fn remove(&mut self, key: &(i32, i32)) -> Option<Column> {
        let i = self.index.remove(key)? as usize;
        let column = self.list.swap_remove(i);
        self.keys.swap_remove(i);
        if let Some(&moved) = self.keys.get(i) {
            self.index.insert(moved, i as u32);
        }
        Some(column)
    }

    fn drain(&mut self) -> impl Iterator<Item = Column> + '_ {
        self.index.clear();
        self.keys.clear();
        self.list.drain(..)
    }

    fn iter(&self) -> impl Iterator<Item = ((i32, i32), &Column)> {
        self.keys.iter().copied().zip(&self.list)
    }
}

impl std::ops::Index<&(i32, i32)> for Columns {
    type Output = Column;
    fn index(&self, key: &(i32, i32)) -> &Column {
        self.get(key).expect("no such column")
    }
}

/// A chunk column's sections.
#[derive(Default)]
struct Column {
    /// By section, bottom to top.
    sections: Vec<SectionMeshes>,
    /// [`SectionDraws::of`] each of `sections`.
    draws: Vec<SectionDraws>,
    /// Bit `y` set for each section with anything to draw.
    filled: u32,
    /// Bit `y` set for each section a mesh came for, empty or not.
    compiled: u32,
}

/// The section `eye` is in, `y` counted from the world's bottom `min_y`.
fn section_of_eye(eye: DVec3, min_y: i32) -> IVec3 {
    let block = eye.floor().as_ivec3();
    IVec3::new(
        section_coord(block.x),
        section_coord(block.y - min_y),
        section_coord(block.z),
    )
}

/// `TranslucencyPointOfView`.
fn point_of_view(camera: IVec3, (x, y, z): SectionPos) -> IVec3 {
    (camera - IVec3::new(x, y, z)).clamp(IVec3::NEG_ONE, IVec3::ONE)
}

/// This frame's section draws, per layer.
#[derive(Default)]
pub struct VisibleChunks {
    pub solid: Vec<ChunkDraw>,
    pub cutout: Vec<ChunkDraw>,
    pub translucent: Vec<ChunkDraw>,
    /// The solid and cutout quads for the GPU to expand, near to far,
    /// in place of `solid`'s and `cutout`'s quad meshes.
    pub sections: Vec<SectionDraw>,
    pub count: SectionCount,
    /// One past the highest [`SectionCull::slot`] in use.
    pub slots: u32,
}

pub struct LevelMeshes {
    /// The world's bottom, where section 0 starts.
    min_y: i32,
    columns: Columns,
    /// Built meshes over the upload budget, uploaded in order on the next frames.
    waiting: VecDeque<FinishedMesh>,
    /// Per run of columns, its draws with their distance, sorted before
    /// they go out; one run when culling on one thread.
    parts: Vec<CulledColumns>,
    merge: MergeDraws,
    cull_pool: CullPool,
    /// Last frame's draw lists, handed back by [`Self::recycle`] to be reused.
    spare_solid: Vec<ChunkDraw>,
    spare_cutout: Vec<ChunkDraw>,
    spare_translucent: Vec<ChunkDraw>,
    spare_sections: Vec<SectionDraw>,
    /// The sections with translucent faces the last frame drew.
    seen_translucent: Vec<SectionPos>,
    /// The sections' occlusion culling slots.
    slots: Slots,
}

impl Default for LevelMeshes {
    fn default() -> Self {
        Self {
            min_y: Height::OVERWORLD.min_y,
            columns: Columns::default(),
            waiting: VecDeque::new(),
            parts: vec![CulledColumns::default()],
            merge: MergeDraws::default(),
            cull_pool: CullPool::new(),
            spare_solid: Vec::new(),
            spare_cutout: Vec::new(),
            spare_translucent: Vec::new(),
            spare_sections: Vec::new(),
            seen_translucent: Vec::new(),
            slots: Slots::default(),
        }
    }
}

impl LevelMeshes {
    /// The level's bottom (where its first section starts) and height.
    pub fn set_height(&mut self, height: Height) {
        self.min_y = height.min_y;
        let drawn = MASK_SECTIONS as i32 * SECTION_SIZE;
        if height.height > drawn {
            melon_base::warn!(
                "The level is {} blocks tall; blocks above its lowest {drawn} are not drawn",
                height.height
            );
        }
    }

    /// Uploads finished meshes in place of the sections' old ones.
    pub fn upload(&mut self, renderer: &mut Renderer, finished: Vec<FinishedMesh>) {
        self.upload_to(renderer, finished);
    }

    fn upload_to(&mut self, sink: &mut impl MeshSink, finished: Vec<FinishedMesh>) {
        if !finished.is_empty() && !self.waiting.is_empty() {
            let newer: melon_core::math::SectionSet =
                finished.iter().map(|mesh| mesh.pos).collect();
            self.waiting.retain(|waiting| !newer.contains(&waiting.pos));
        }
        for mesh in finished {
            if mesh.sync {
                self.install(sink, mesh);
            } else {
                self.waiting.push_back(mesh);
            }
        }
        let mut bytes = 0;
        while bytes < UPLOAD_BUDGET {
            let Some(mesh) = self.waiting.pop_front() else {
                break;
            };
            bytes += mesh.mesh.bytes();
            self.install(sink, mesh);
        }
    }

    /// Whether a mesh was ever built for the section, drawn or not.
    pub fn section_compiled(&self, (x, y, z): SectionPos) -> bool {
        usize::try_from(y)
            .ok()
            .filter(|&y| y < MASK_SECTIONS)
            .is_some_and(|y| {
                self.columns
                    .get(&(x, z))
                    .is_some_and(|column| column.compiled & (1 << y) != 0)
            })
    }

    /// Whether the section has anything to draw
    /// (`SectionMesh.hasRenderableLayers`).
    pub fn section_filled(&self, (x, y, z): SectionPos) -> bool {
        usize::try_from(y)
            .ok()
            .filter(|&y| y < MASK_SECTIONS)
            .is_some_and(|y| {
                self.columns
                    .get(&(x, z))
                    .is_some_and(|column| column.filled & (1 << y) != 0)
            })
    }

    /// Each chunk column with something to draw and which of its sections
    /// do (bit per section): what [`Self::visible_sections`] culls, a
    /// column's box first, then its sections.
    pub fn filled_columns(&self) -> impl Iterator<Item = ((i32, i32), u32)> + '_ {
        self.columns
            .iter()
            .filter(|(_, c)| c.filled != 0)
            .map(|(pos, c)| (pos, c.filled))
    }

    /// Puts a built section in place of its old meshes.
    fn install(&mut self, sink: &mut impl MeshSink, finished: FinishedMesh) {
        let FinishedMesh { pos, mesh, .. } = finished;
        let Ok(y) = usize::try_from(pos.1) else {
            return;
        };
        if y >= MASK_SECTIONS {
            return;
        }
        let meshes = match SectionMeshes::upload(pos, mesh, sink) {
            Ok(meshes) => meshes,
            Err(e) => {
                melon_base::warn!("Section mesh not uploaded: {e:#}");
                SectionMeshes::default()
            }
        };
        let column = self.columns.entry((pos.0, pos.2));
        if column.sections.len() <= y {
            column.sections.resize_with(y + 1, SectionMeshes::default);
            column.draws.resize_with(y + 1, SectionDraws::default);
        }
        let old_slot = column.draws[y].slot;
        let mut draws = SectionDraws::of(&meshes);
        if draws.has_quads() {
            draws.slot = if old_slot == NO_MESH {
                self.slots.take()
            } else {
                old_slot
            };
            let layer = |layer: &Option<QuadLayer>| layer.as_ref().map(|l| (l.mesh, l.ends()));
            sink.record(
                draws.slot,
                &SectionRecord {
                    origin: IVec3::new(pos.0 * SECTION_SIZE, self.min_y, pos.2 * SECTION_SIZE),
                    bottom: y as i32 * SECTION_SIZE,
                    layers: [layer(&meshes.solid), layer(&meshes.cutout)],
                },
            );
        } else {
            self.slots.give_back(old_slot);
        }
        column.draws[y] = draws;
        column.compiled |= 1 << y;
        if meshes.is_empty() {
            column.filled &= !(1 << y);
        } else {
            column.filled |= 1 << y;
        }
        std::mem::replace(&mut column.sections[y], meshes).free(sink);
    }

    /// Takes a section's meshes without a GPU, `ids` handing out an id for each.
    pub fn insert_headless(&mut self, mesh: FinishedMesh, ids: impl FnMut() -> usize) {
        self.install(&mut Headless(ids), mesh);
    }

    /// A chunk was unloaded or replaced: its meshes go.
    pub fn remove(&mut self, renderer: &mut Renderer, key: (i32, i32)) {
        self.waiting
            .retain(|waiting| (waiting.pos.0, waiting.pos.2) != key);
        if let Some(column) = self.columns.remove(&key) {
            for draws in &column.draws {
                self.slots.give_back(draws.slot);
            }
            for section in column.sections {
                section.free(renderer);
            }
        }
    }

    /// Frees every mesh. Call when leaving the world.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        self.waiting.clear();
        self.seen_translucent.clear();
        self.slots = Slots::default();
        for column in self.columns.drain() {
            for section in column.sections {
                section.free(renderer);
            }
        }
    }

    /// Sorts the translucent faces of sections the camera now sees from elsewhere.
    pub fn resort(&mut self, renderer: &mut Renderer, eye: DVec3) {
        self.resort_with(renderer, eye);
    }

    fn resort_with(&mut self, sink: &mut impl MeshSink, eye: DVec3) {
        let block = eye.floor().as_ivec3();
        let seen = std::mem::take(&mut self.seen_translucent);
        let camera = section_of_eye(eye, self.min_y);
        let mut due = Vec::new();
        let mut queued = HashSet::new();
        for &pos in &seen {
            if self.due_to_resort(pos, eye, camera, block, true) {
                queued.insert(pos);
                let offset = IVec3::new(pos.0, pos.1, pos.2) - camera;
                let nearby = offset.abs().max_element() <= NEARBY_SECTIONS;
                due.push((offset.length_squared(), pos, nearby));
            }
        }
        for dx in -NEARBY_SECTIONS..=NEARBY_SECTIONS {
            for dy in -NEARBY_SECTIONS..=NEARBY_SECTIONS {
                for dz in -NEARBY_SECTIONS..=NEARBY_SECTIONS {
                    let pos = (camera.x + dx, camera.y + dy, camera.z + dz);
                    if queued.contains(&pos) {
                        continue;
                    }
                    if self.due_to_resort(pos, eye, camera, block, false) {
                        queued.insert(pos);
                        due.push((IVec3::new(dx, dy, dz).length_squared(), pos, true));
                    }
                }
            }
        }
        due.sort_unstable_by_key(|&(distance, ..)| distance);
        let mut quads = 0;
        for (_, pos, nearby) in due {
            if nearby || quads < RESORT_BUDGET {
                quads += self.resort_section(sink, pos, eye, camera);
            }
        }
        self.seen_translucent = seen;
    }

    /// Vanilla's block and point-of-view tests when `from_seen`, plus a face check.
    fn due_to_resort(
        &self,
        pos: SectionPos,
        eye: DVec3,
        camera: IVec3,
        block: IVec3,
        from_seen: bool,
    ) -> bool {
        let Some(translucent) = self.translucent(pos) else {
            return false;
        };
        if translucent
            .faces
            .camera_crossed_a_face(terrain::sort_eye(eye, pos, self.min_y))
        {
            return true;
        }
        if !from_seen {
            return false;
        }
        let view = point_of_view(camera, pos);
        let offset = IVec3::new(pos.0, pos.1, pos.2) - camera;
        let nearby = offset.abs().max_element() <= NEARBY_SECTIONS;
        let moved = block.cmpne(translucent.block);
        let lined_up = (0..3).any(|axis| moved.test(axis) && view[axis] == 0);
        view != translucent.view || (moved.any() && nearby) || lined_up
    }

    fn translucent(&self, (x, y, z): SectionPos) -> Option<&Translucent> {
        self.columns
            .get(&(x, z))
            .and_then(|column| column.sections.get(y as usize))
            .and_then(|section| section.translucent.as_ref())
    }

    fn translucent_mut(&mut self, (x, y, z): SectionPos) -> Option<&mut Translucent> {
        self.columns
            .get_mut(&(x, z))
            .and_then(|column| column.sections.get_mut(y as usize))
            .and_then(|section| section.translucent.as_mut())
    }

    /// Sorts one section's translucent faces from `eye` and uploads their
    /// index list; how many quads that sorted.
    fn resort_section(
        &mut self,
        sink: &mut impl MeshSink,
        pos: SectionPos,
        eye: DVec3,
        camera: IVec3,
    ) -> usize {
        let min_y = self.min_y;
        let Some(translucent) = self.translucent_mut(pos) else {
            return 0;
        };
        translucent.faces.sort(terrain::sort_eye(eye, pos, min_y));
        translucent.view = point_of_view(camera, pos);
        translucent.block = eye.floor().as_ivec3();
        match sink.indices(&translucent.faces.indices) {
            Ok(indices) => {
                sink.free_indices(std::mem::replace(&mut translucent.indices, indices));
            }
            Err(e) => melon_base::warn!("Translucent mesh not resorted: {e:#}"),
        }
        let (quads, indices) = (
            translucent.faces.quad_count(),
            mesh_index(translucent.indices.0),
        );
        if let Some(draws) = self
            .columns
            .get_mut(&(pos.0, pos.2))
            .and_then(|column| column.draws.get_mut(pos.1 as usize))
        {
            draws.translucent.1 = indices;
        }
        quads
    }

    /// The parts of meshes `culler` lets through, near to far per layer;
    /// past [`PARALLEL_CULL_COLUMNS`] the columns are split across
    /// [`CullPool`]'s threads and merged in the order one thread finds them.
    /// With `expand`, the solid and cutout quads go to
    /// [`VisibleChunks::sections`] for the GPU to expand
    /// ([`Renderer::expands_terrain`]).
    pub fn visible_sections(&mut self, culler: &Culler, expand: bool) -> VisibleChunks {
        let eye = culler.eye();
        let mut visible = VisibleChunks {
            solid: std::mem::take(&mut self.spare_solid),
            cutout: std::mem::take(&mut self.spare_cutout),
            translucent: std::mem::take(&mut self.spare_translucent),
            sections: std::mem::take(&mut self.spare_sections),
            count: SectionCount::default(),
            slots: self.slots.next,
        };
        visible.solid.clear();
        visible.cutout.clear();
        visible.translucent.clear();
        visible.sections.clear();
        self.seen_translucent.clear();
        let min_y = self.min_y;
        let vertical = |group: usize| {
            (0..32).fold(0u32, |mask, y| {
                mask | u32::from(faces_camera(group, DVec3::ZERO, y, eye, min_y)) << y
            })
        };
        let frame = CullFrame {
            culler,
            cull: culler.section_cull(),
            eye,
            min_y,
            up: vertical(Direction::Up.index()),
            down: vertical(Direction::Down.index()),
            expand,
        };
        let columns = &self.columns;
        let helpers = self.cull_pool.helpers(columns.list.len());
        if helpers == 0 {
            let part = &mut self.parts[0];
            cull_columns(columns.iter(), &frame, part);
            visible.count.total = part.total;
            visible.count.drawn = part.drawn;
            let (vertices, draws) = part.counts();
            visible.count.vertices += vertices;
            visible.count.draws += draws;
            self.seen_translucent.append(&mut part.seen_translucent);
            part.solid.drain_sorted_into(&mut visible.solid);
            part.cutout.drain_sorted_into(&mut visible.cutout);
            part.translucent.drain_sorted_into(&mut visible.translucent);
            part.sections.drain_sorted_into(&mut visible.sections);
            part.total = 0;
            part.drawn = 0;
            return visible;
        }
        // Columns in runs of one size, taken in turn by the helpers and this
        // thread; each run's draws are its own until they are merged in run
        // order, so the lists come out as the loop above makes them.
        let runs = (helpers + 1) * CULL_RUNS_PER_THREAD;
        let per_run = columns.list.len().div_ceil(runs);
        self.parts.resize_with(runs, CulledColumns::default);
        {
            let work = std::sync::Mutex::new(
                columns
                    .keys
                    .chunks(per_run)
                    .zip(columns.list.chunks(per_run))
                    .zip(self.parts.iter_mut()),
            );
            let take = || {
                loop {
                    let next = work
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .next();
                    let Some(((keys, list), part)) = next else {
                        break;
                    };
                    cull_columns(keys.iter().copied().zip(list), &frame, part);
                }
            };
            self.cull_pool.run(helpers, &take);
        }
        let parts = &mut self.parts[..];
        for part in parts.iter_mut() {
            visible.count.total += part.total;
            visible.count.drawn += part.drawn;
            let (vertices, draws) = part.counts();
            visible.count.vertices += vertices;
            visible.count.draws += draws;
            self.seen_translucent.append(&mut part.seen_translucent);
            part.total = 0;
            part.drawn = 0;
        }
        let merge = &mut self.merge;
        merge.drain_sorted_into(parts, |part| &mut part.solid, &mut visible.solid);
        merge.drain_sorted_into(parts, |part| &mut part.cutout, &mut visible.cutout);
        merge.drain_sorted_into(
            parts,
            |part| &mut part.translucent,
            &mut visible.translucent,
        );
        merge.drain_sorted_into(parts, |part| &mut part.sections, &mut visible.sections);
        visible
    }

    /// Takes back the draw lists [`Self::visible_sections`] handed out, to reuse them.
    pub fn recycle(
        &mut self,
        (solid, cutout, translucent): (Vec<ChunkDraw>, Vec<ChunkDraw>, Vec<ChunkDraw>),
        sections: Vec<SectionDraw>,
    ) {
        self.spare_solid = solid;
        self.spare_cutout = cutout;
        self.spare_translucent = translucent;
        self.spare_sections = sections;
    }
}

/// Column runs per culling thread: more runs than threads, so a thread
/// that drew a light run takes another while the others finish.
const CULL_RUNS_PER_THREAD: usize = 4;
/// Fewer columns than this are culled on the frame's thread alone: waking
/// the helpers would cost more than they save.
const PARALLEL_CULL_COLUMNS: usize = 1024;

/// What every thread of one frame's section culling reads.
struct CullFrame<'a> {
    culler: &'a Culler<'a>,
    cull: melon_world::culling::section_cull::SectionCull,
    eye: DVec3,
    min_y: i32,
    /// Bit per section height: whether its up (down) faces can face the eye.
    up: u32,
    down: u32,
    /// Whether the quads go out as [`SectionDraw`]s.
    expand: bool,
}

/// One run of columns' share of [`LevelMeshes::visible_sections`].
#[derive(Default)]
struct CulledColumns {
    solid: SortedDraws<ChunkDraw>,
    cutout: SortedDraws<ChunkDraw>,
    translucent: SortedDraws<ChunkDraw>,
    sections: SortedDraws<SectionDraw>,
    seen_translucent: Vec<SectionPos>,
    /// Filled sections, and those drawn, for F3.
    total: usize,
    drawn: usize,
}

impl CulledColumns {
    /// The vertices and draws of every list, for F3.
    fn counts(&self) -> (usize, usize) {
        let lists = [
            self.solid.counts(),
            self.cutout.counts(),
            self.translucent.counts(),
            self.sections.counts(),
        ];
        lists
            .iter()
            .fold((0, 0), |(v, d), &(vertices, draws)| (v + vertices, d + draws))
    }
}

/// The draws of `columns` that `frame` lets through, into `out`.
fn cull_columns<'c>(
    columns: impl Iterator<Item = ((i32, i32), &'c Column)>,
    frame: &CullFrame<'_>,
    out: &mut CulledColumns,
) {
    let min_y = frame.min_y;
    for ((cx, cz), column) in columns {
        let filled = column.filled;
        if filled == 0 {
            continue;
        }
        out.total += filled.count_ones() as usize;
        let in_view = frame
            .culler
            .reached_of((cx, cz), frame.cull.column((cx, cz)) & filled);
        if in_view == 0 {
            continue;
        }
        let (x, z) = (f64::from(cx * SECTION_SIZE), f64::from(cz * SECTION_SIZE));
        let bottom = |y: u32| f64::from(min_y + y as i32 * SECTION_SIZE);
        let origin = DVec3::new(x, 0.0, z);
        // Bit per facing group: whether some face of it can face the eye.
        let sides = (0..terrain::FACINGS).fold(0u32, |mask, group| {
            mask | u32::from(faces_camera(group, origin, 0, frame.eye, min_y)) << group
        });
        let vertical = 1 << Direction::Up.index() | 1 << Direction::Down.index();
        let corner = (cx * SECTION_SIZE, cz * SECTION_SIZE);
        let quads_origin = IVec3::new(corner.0, min_y, corner.1);
        let mut rest = in_view;
        while rest != 0 {
            let y = rest.trailing_zeros();
            rest &= rest - 1;
            // Lava and translucent vertices count from the section's corner.
            let vertices_origin = IVec3::new(corner.0, min_y + y as i32 * SECTION_SIZE, corner.1);
            let Some(section) = column.draws.get(y as usize) else {
                continue;
            };
            out.drawn += 1;
            let center = DVec3::new(x + 8.0, bottom(y) + 8.0, z + 8.0);
            let distance = center.distance_squared(frame.eye) as f32;
            let shown = sides & !vertical
                | (frame.up >> y & 1) << Direction::Up.index()
                | (frame.down >> y & 1) << Direction::Down.index();
            for (cutout, layer, list) in [
                (false, &section.solid, &mut out.solid),
                (true, &section.cutout, &mut out.cutout),
            ] {
                if layer.mesh == NO_MESH {
                    continue;
                }
                if frame.expand {
                    let (draws, quads) = melon_render::expand::layer_draws(&layer.ends, shown);
                    if draws > 0 {
                        let draw = SectionDraw::new(
                            section.slot,
                            cutout,
                            shown,
                            layer.block,
                            draws as u16,
                        );
                        let counts = (quads as usize * 4, draws as usize);
                        out.sections.push(distance, draw, counts);
                    }
                    continue;
                }
                let mesh = TerrainMesh::Quads(QuadMeshId(layer.mesh as usize));
                let cull = Some(SectionCull {
                    slot: section.slot,
                    bottom: y as i32 * SECTION_SIZE,
                });
                melon_render::expand::layer_runs(&layer.ends, shown, |first, end| {
                    let draw = ChunkDraw {
                        mesh,
                        origin: quads_origin,
                        range: (first..end).into(),
                        cull,
                    };
                    list.push(distance, draw, (draw.vertex_count(), 1));
                });
            }
            let (lava, count) = section.lava;
            if lava != NO_MESH {
                let draw = ChunkDraw {
                    mesh: TerrainMesh::Vertices(MeshId(lava as usize)),
                    origin: vertices_origin,
                    range: (0..count).into(),
                    cull: None,
                };
                out.solid.push(distance, draw, (draw.vertex_count(), 1));
            }
            let (vertices, indices, count) = section.translucent;
            if vertices != NO_MESH {
                out.seen_translucent.push((cx, y as i32, cz));
                let draw = ChunkDraw {
                    mesh: TerrainMesh::Sorted {
                        vertices: MeshId(vertices as usize),
                        indices: IndexMeshId(indices as usize),
                    },
                    origin: vertices_origin,
                    range: (0..count).into(),
                    cull: None,
                };
                out.translucent
                    .push_far_first(distance, draw, (draw.vertex_count(), 1));
            }
        }
    }
}

/// The helper threads of the section culling, started on first use.
struct CullPool {
    /// Helpers wanted besides the frame's thread; 0 culls on it alone.
    helpers: usize,
    pool: Option<rayon::ThreadPool>,
}

impl CullPool {
    /// Half the cores less the frame's thread, at most 3: the meshers and
    /// the integrated server keep the rest.
    fn new() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self::with_helpers((cores / 2).saturating_sub(1).min(3))
    }

    fn with_helpers(helpers: usize) -> Self {
        Self {
            helpers,
            pool: None,
        }
    }

    /// Helpers to cull `columns` with.
    fn helpers(&mut self, columns: usize) -> usize {
        if self.helpers == 0 || columns < PARALLEL_CULL_COLUMNS {
            return 0;
        }
        if self.pool.is_none() {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(self.helpers)
                .thread_name(|i| format!("cull-{i}"))
                .start_handler(|_| melon_base::platform::raise_frame_thread_priority())
                .build();
            match pool {
                Ok(pool) => self.pool = Some(pool),
                Err(e) => {
                    melon_base::warn!("Section culling stays on one thread: {e}");
                    self.helpers = 0;
                    return 0;
                }
            }
        }
        self.helpers
    }

    /// Runs `work` on `helpers` pool threads and on this one, until each returns.
    fn run(&self, helpers: usize, work: &(dyn Fn() + Sync)) {
        let Some(pool) = &self.pool else {
            work();
            return;
        };
        pool.in_place_scope(|scope| {
            for _ in 0..helpers {
                scope.spawn(|_| work());
            }
            work();
        });
    }
}

/// Sorts several [`SortedDraws`] into one list, as if one had taken all
/// their draws in turn: the run index in each key counts on across them.
#[derive(Default)]
struct MergeDraws {
    keys: Vec<u64>,
    scratch: Vec<u64>,
    /// Per run, its part and its draws there.
    runs: Vec<(u32, u32, u32)>,
}

impl MergeDraws {
    /// Appends the draws of `layer` of every part to `out` in key order and empties them.
    fn drain_sorted_into<T: Copy>(
        &mut self,
        parts: &mut [CulledColumns],
        layer: impl Fn(&mut CulledColumns) -> &mut SortedDraws<T>,
        out: &mut Vec<T>,
    ) {
        self.keys.clear();
        self.runs.clear();
        let mut total = 0;
        for (index, part) in parts.iter_mut().enumerate() {
            let sorted = layer(part);
            total += sorted.draws.len();
            let first = self.runs.len() as u64;
            for (run, &start) in sorted.starts.iter().enumerate() {
                let end = sorted
                    .starts
                    .get(run + 1)
                    .map_or(sorted.draws.len() as u32, |&end| end);
                self.runs.push((index as u32, start, end));
            }
            self.keys.extend(
                sorted
                    .keys
                    .iter()
                    .map(|&key| (key & !0xFFFF_FFFF) | ((key & 0xFFFF_FFFF) + first)),
            );
        }
        radix::sort(&mut self.keys, &mut self.scratch);
        out.reserve(total);
        for &key in &self.keys {
            let (part, start, end) = self.runs[(key & 0xFFFF_FFFF) as usize];
            out.extend_from_slice(
                &layer(&mut parts[part as usize]).draws[start as usize..end as usize],
            );
        }
        for part in parts {
            layer(part).clear();
        }
    }
}

/// One layer's draws in the order culling found them, sorted by distance
/// as a stable sort would. Draws pushed one after another at the same
/// distance (a section's) are one run with one sort key ([`draw_key`]):
/// sorting the runs' 8-byte keys and copying each run whole moves far fewer
/// bytes than sorting the draws, and reads them in order.
struct SortedDraws<T> {
    draws: Vec<T>,
    /// Per run, its key; the low half counts the runs.
    keys: Vec<u64>,
    /// Per run, where its draws start.
    starts: Vec<u32>,
    /// The draws' vertices and draw calls, for F3.
    vertices: usize,
    calls: usize,
    /// [`radix::sort`]'s second buffer.
    scratch: Vec<u64>,
}

impl<T> Default for SortedDraws<T> {
    fn default() -> Self {
        Self {
            draws: Vec::new(),
            keys: Vec::new(),
            starts: Vec::new(),
            vertices: 0,
            calls: 0,
            scratch: Vec::new(),
        }
    }
}

impl<T: Copy> SortedDraws<T> {
    /// Adds `draw`, `distance` (squared) from the eye, near ones first;
    /// it shades `counts.0` vertices in `counts.1` draw calls.
    fn push(&mut self, distance: f32, draw: T, counts: (usize, usize)) {
        self.push_keyed(draw_key(distance, self.keys.len()), draw, counts);
    }

    /// Adds `draw`, far ones first: translucent faces blend back to front.
    fn push_far_first(&mut self, distance: f32, draw: T, counts: (usize, usize)) {
        self.push_keyed(draw_key(distance, self.keys.len()) ^ !0 << 32, draw, counts);
    }

    /// Adds `draw` to the last run if `key` sorts it with it, else to a new run.
    fn push_keyed(&mut self, key: u64, draw: T, (vertices, calls): (usize, usize)) {
        if self.keys.last().is_none_or(|&last| last >> 32 != key >> 32) {
            self.keys.push(key);
            self.starts.push(self.draws.len() as u32);
        }
        self.vertices += vertices;
        self.calls += calls;
        self.draws.push(draw);
    }

    /// The vertices and draw calls pushed since the last clear.
    fn counts(&self) -> (usize, usize) {
        (self.vertices, self.calls)
    }

    fn clear(&mut self) {
        self.draws.clear();
        self.keys.clear();
        self.starts.clear();
        self.vertices = 0;
        self.calls = 0;
    }

    /// Appends the draws to `out` in key order and empties `self`.
    fn drain_sorted_into(&mut self, out: &mut Vec<T>) {
        radix::sort(&mut self.keys, &mut self.scratch);
        out.reserve(self.draws.len());
        for &key in &self.keys {
            let run = (key & 0xFFFF_FFFF) as usize;
            let start = self.starts[run] as usize;
            let end = self
                .starts
                .get(run + 1)
                .map_or(self.draws.len(), |&end| end as usize);
            out.extend_from_slice(&self.draws[start..end]);
        }
        self.clear();
    }
}

/// A draw's place in the near-to-far order, as one integer: the squared
/// distance's bits on top (a non-negative float orders like its bits),
/// then where the draw was found, which also tells the draw apart.
fn draw_key(distance: f32, index: usize) -> u64 {
    u64::from(distance.max(0.0).to_bits()) << 32 | index as u64 & 0xFFFF_FFFF
}

/// What the culling let through this frame, for F3's `C:` and `V:` lines.
#[derive(Clone, Copy, Default)]
pub struct SectionCount {
    pub drawn: usize,
    pub total: usize,
    /// Terrain vertices and draw calls this frame, all layers.
    pub vertices: usize,
    pub draws: usize,
}

/// Whether some face of facing group `group` in section `y` of the chunk at `origin` can face
/// `eye`.
fn faces_camera(group: usize, origin: DVec3, y: usize, eye: DVec3, min_y: i32) -> bool {
    let size = f64::from(SECTION_SIZE);
    let min = DVec3::new(
        origin.x,
        f64::from(min_y + y as i32 * SECTION_SIZE),
        origin.z,
    );
    let max = min + DVec3::splat(size);
    let Some(direction) = Direction::ALL.get(group) else {
        return true;
    };
    let axis = direction.axis();
    if direction.is_positive() {
        eye[axis] > min[axis]
    } else {
        eye[axis] < max[axis]
    }
}

/// A vertex list on the GPU, uploaded again only when it changed.
#[derive(Default)]
struct ChangingMesh {
    mesh: Option<MeshId>,
    /// What `mesh` holds.
    uploaded: Vec<WorldVertex>,
    /// The latest build, kept between builds for its allocation.
    building: Vec<WorldVertex>,
}

impl ChangingMesh {
    /// Builds the vertices with `build` and uploads them if they differ from the last.
    fn update(
        &mut self,
        renderer: &mut Renderer,
        build: impl FnOnce(&mut Vec<WorldVertex>),
    ) -> Option<MeshId> {
        self.building.clear();
        build(&mut self.building);
        if self.building != self.uploaded {
            std::mem::swap(&mut self.building, &mut self.uploaded);
            if let Some(old) = self.mesh.take() {
                renderer.free_mesh(old);
            }
            if !self.uploaded.is_empty() {
                match renderer.upload_mesh(&self.uploaded) {
                    Ok(mesh) => self.mesh = Some(mesh),
                    Err(e) => {
                        melon_base::warn!("Block entity mesh failed: {e:#}");
                        self.uploaded.clear();
                    }
                }
            }
        }
        self.mesh
    }

    fn clear(&mut self, renderer: &mut Renderer) {
        if let Some(mesh) = self.mesh.take() {
            renderer.free_mesh(mesh);
        }
        self.uploaded.clear();
    }
}

/// How far the camera moves before the still block entities are gathered again.
const STILL_STEP: f32 = 8.0;

/// The block entities' geometry (chests, bells, books...), in two meshes.
#[derive(Default)]
pub struct BlockEntityMeshes {
    still: ChangingMesh,
    /// The level's and the animations' revision and the camera position the still mesh was
    /// built for.
    still_for: Option<(u64, u64, DVec3)>,
    /// The block `still`'s vertices are relative to.
    still_origin: IVec3,
    moving: ChangingMesh,
}

/// Makes `vertices` relative to the block `origin`, so they are drawn without float error far
/// out.
fn shift(vertices: &mut [WorldVertex], origin: IVec3) {
    let origin = origin.as_vec3();
    for vertex in vertices {
        vertex.pos = (Vec3::from_array(vertex.pos) - origin).to_array();
    }
}

impl BlockEntityMeshes {
    /// This frame's meshes, rebuilt as needed, each with the block its vertices are relative
    /// to.
    pub fn update(
        &mut self,
        renderer: &mut Renderer,
        block_entities: &BlockEntities,
        level: &Level,
        models: &BlockModels,
        culler: &Culler,
        partial: f32,
    ) -> [Option<(MeshId, IVec3)>; 2] {
        let eye = culler.eye();
        let origin = eye.floor().as_ivec3();
        let revisions = (level.revision(), block_entities.still_revision());
        let stale = self.still_for.is_none_or(|(level_rev, anim_rev, center)| {
            (level_rev, anim_rev) != revisions
                || center.distance_squared(eye) as f32 > STILL_STEP.powi(2)
        });
        if stale {
            self.still.update(renderer, |out| {
                block_entities.build_still(level, models, eye, out);
                shift(out, origin);
            });
            self.still_for = Some((revisions.0, revisions.1, eye));
            self.still_origin = origin;
        }
        let moving = self.moving.update(renderer, |out| {
            block_entities.build_moving(level, models, culler, eye, partial, out);
            shift(out, origin);
        });
        [
            self.still.mesh.map(|mesh| (mesh, self.still_origin)),
            moving.map(|mesh| (mesh, origin)),
        ]
    }

    /// Frees the meshes. Call when leaving the world.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        self.still.clear(renderer);
        self.still_for = None;
        self.moving.clear(renderer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use melon_core::util::xorshift::XorShift;

    #[test]
    fn draw_keys_order_like_distance_then_index() {
        let mut rng = XorShift(0x0bad_5eed_1234_5678);
        let mut random = move || rng.next_u64();
        let draws: Vec<(f32, usize)> = (0..400)
            .map(|index| ((random() % 50) as f32 * 37.25, index))
            .collect();
        for a in &draws {
            for b in &draws {
                let by_compare = a.0.total_cmp(&b.0).then(a.1.cmp(&b.1));
                assert_eq!(draw_key(a.0, a.1).cmp(&draw_key(b.0, b.1)), by_compare);
                let far_first = b.0.total_cmp(&a.0).then(a.1.cmp(&b.1));
                let far = |d: &(f32, usize)| draw_key(d.0, d.1) ^ !0 << 32;
                assert_eq!(far(a).cmp(&far(b)), far_first);
            }
        }
    }

    /// Times [`LevelMeshes::visible_sections`] at render distance 32: 65 x 65
    /// columns of ten filled sections, every facing group of the solid and
    /// cutout layers holding quads, the camera looking along a row. Run with
    /// `cargo test --profile fast --lib visible_sections_at_rd32 -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn visible_sections_at_rd32() {
        use melon_world::culling::frustum::Frustum;
        use melon_world::culling::{CullSettings, Culler};
        let layer = || LayerMesh {
            items: vec![TerrainQuad([0; 5]); 70],
            groups: std::array::from_fn(|g| g as u32 * 10..g as u32 * 10 + 10),
        };
        let mut meshes = LevelMeshes::default();
        meshes.set_height(Height::OVERWORLD);
        let mut next = 0;
        for x in -32..=32 {
            for z in -32..=32 {
                for y in 4..14 {
                    let mesh = SectionMesh {
                        solid: layer(),
                        cutout: layer(),
                        ..SectionMesh::default()
                    };
                    let finished = FinishedMesh {
                        pos: (x, y, z),
                        mesh,
                        sync: true,
                    };
                    meshes.insert_headless(finished, || {
                        next += 1;
                        next
                    });
                }
            }
        }
        let eye = DVec3::new(0.5, 80.0, 0.5);
        // Turns +x to the view's -z: looking along the row.
        let view = glam::Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let frustum = Frustum::from_view_projection(
            melon_world::projection::world_projection(70.0, 16.0 / 9.0, 512.0) * view,
        );
        let culler = Culler::new(frustum, None, eye, (-64, 24), CullSettings::default());
        for (helpers, expand) in [(0, false), (3, false), (0, true), (3, true)] {
            meshes.cull_pool = CullPool::with_helpers(helpers);
            let rounds = 300;
            let mut draws = (0, 0, 0);
            let started = std::time::Instant::now();
            for _ in 0..rounds {
                let visible = meshes.visible_sections(&culler, expand);
                draws = (
                    visible.count.draws,
                    visible.count.drawn,
                    visible.count.total,
                );
                meshes.recycle(
                    (visible.solid, visible.cutout, visible.translucent),
                    visible.sections,
                );
            }
            let each = started.elapsed() / rounds;
            eprintln!(
                "visible_sections, {helpers} helpers, expand {expand}: {each:?} per frame, (draws, sections drawn, of) {draws:?}"
            );
        }
    }

    /// Columns around the origin, `radius` in each direction, their sections
    /// solid, cutout or translucent by a seeded draw; mesh ids count up.
    fn random_columns(meshes: &mut LevelMeshes, radius: i32, seed: u64) {
        meshes.set_height(Height::OVERWORLD);
        let mut rng = XorShift(seed);
        let mut next = 0;
        let eye = DVec3::new(0.5, 80.0, 0.5);
        for x in -radius..=radius {
            for z in -radius..=radius {
                for y in 2..16 {
                    let roll = rng.next_u64();
                    if roll.is_multiple_of(4) {
                        continue;
                    }
                    let groups = |shift: u64| -> [Range<u32>; terrain::FACINGS] {
                        std::array::from_fn(|g| {
                            let n = (roll >> (shift + g as u64)) as u32 & 1;
                            g as u32 * 2..g as u32 * 2 + n
                        })
                    };
                    let layer = |shift: u64| LayerMesh {
                        items: vec![TerrainQuad([0; 5]); 2 * terrain::FACINGS],
                        groups: groups(shift),
                    };
                    let finished = if roll.is_multiple_of(7) {
                        two_quads((x, y, z), 0.25, 0.75, eye)
                    } else {
                        FinishedMesh {
                            pos: (x, y, z),
                            mesh: SectionMesh {
                                solid: layer(8),
                                cutout: if roll.is_multiple_of(3) {
                                    layer(24)
                                } else {
                                    LayerMesh::default()
                                },
                                ..SectionMesh::default()
                            },
                            sync: true,
                        }
                    };
                    meshes.insert_headless(finished, || {
                        next += 1;
                        next
                    });
                }
            }
        }
    }

    /// Culling across threads gives the lists culling on one thread gives,
    /// in the same order: equal distances included, which ties break by
    /// where the draws were found.
    #[test]
    fn parallel_culling_draws_what_one_thread_draws() {
        use melon_world::culling::frustum::Frustum;
        use melon_world::culling::{CullSettings, Culler};
        let mut one = LevelMeshes {
            cull_pool: CullPool::with_helpers(0),
            ..LevelMeshes::default()
        };
        let mut many = LevelMeshes {
            cull_pool: CullPool::with_helpers(3),
            ..LevelMeshes::default()
        };
        // 41 x 41 columns: past `PARALLEL_CULL_COLUMNS`.
        random_columns(&mut one, 20, 0x5eed_0001);
        random_columns(&mut many, 20, 0x5eed_0001);
        let eye = DVec3::new(0.5, 80.0, 0.5);
        for turn in 0..8 {
            let view = glam::Mat4::from_rotation_y(turn as f32 * 1.3);
            let frustum = Frustum::from_view_projection(
                melon_world::projection::world_projection(70.0, 16.0 / 9.0, 512.0) * view,
            );
            let culler = Culler::new(frustum, None, eye, (-64, 24), CullSettings::default());
            let expand = turn % 2 == 1;
            let a = one.visible_sections(&culler, expand);
            let b = many.visible_sections(&culler, expand);
            assert!(many.cull_pool.pool.is_some(), "culled on helpers");
            assert!(!a.translucent.is_empty());
            // No lava here: expanded, nothing is left in the chunk lists.
            assert_eq!(a.solid.is_empty(), expand);
            assert_eq!(a.cutout.is_empty(), expand);
            assert_eq!(a.sections.is_empty(), !expand);
            assert_eq!(a.solid, b.solid);
            assert_eq!(a.cutout, b.cutout);
            assert_eq!(a.translucent, b.translucent);
            assert_eq!(a.sections, b.sections);
            assert_eq!(
                (
                    a.count.total,
                    a.count.drawn,
                    a.count.vertices,
                    a.count.draws
                ),
                (
                    b.count.total,
                    b.count.drawn,
                    b.count.vertices,
                    b.count.draws
                )
            );
            assert_eq!(one.seen_translucent, many.seen_translucent);
            one.recycle((a.solid, a.cutout, a.translucent), a.sections);
            many.recycle((b.solid, b.cutout, b.translucent), b.sections);
        }
    }

    /// The layers handed to the GPU expand into the quad ranges the chunk
    /// draws name, layer by layer in the same order, with the same counts.
    #[test]
    fn expanded_sections_draw_what_the_chunk_draws_draw() {
        use melon_world::culling::frustum::Frustum;
        use melon_world::culling::{CullSettings, Culler};
        let mut meshes = LevelMeshes {
            cull_pool: CullPool::with_helpers(0),
            ..LevelMeshes::default()
        };
        random_columns(&mut meshes, 12, 0x5eed_0002);
        let by_slot: std::collections::HashMap<u32, SectionDraws> = meshes
            .columns
            .list
            .iter()
            .flat_map(|column| column.draws.iter().copied())
            .filter(|draws| draws.slot != NO_MESH)
            .map(|draws| (draws.slot, draws))
            .collect();
        /// Per layer, in order: each section's slot and the quad ranges drawn.
        type Layers = [Vec<(u32, Vec<(u32, u32)>)>; 2];
        // Above, inside and below the sections, so up, down and both show.
        for (turn, height) in [(0.0, 200.0), (1.7, 90.0), (3.1, -40.0), (4.4, 75.5)] {
            let eye = DVec3::new(3.25, height, -7.5);
            let view = glam::Mat4::from_rotation_y(turn);
            let frustum = Frustum::from_view_projection(
                melon_world::projection::world_projection(70.0, 16.0 / 9.0, 512.0) * view,
            );
            let culler = Culler::new(frustum, None, eye, (-64, 24), CullSettings::default());
            let chunks = meshes.visible_sections(&culler, false);
            let expanded = meshes.visible_sections(&culler, true);
            let mut from_chunks = Layers::default();
            for (layer, list) in [&chunks.solid, &chunks.cutout].into_iter().enumerate() {
                for draw in list.iter().filter_map(|d| d.cull.map(|c| (c.slot, d.range))) {
                    let out = &mut from_chunks[layer];
                    match out.last_mut() {
                        Some((slot, ranges)) if *slot == draw.0 => {
                            ranges.push((draw.1.start, draw.1.end));
                        }
                        _ => out.push((draw.0, vec![(draw.1.start, draw.1.end)])),
                    }
                }
            }
            let mut from_sections = Layers::default();
            for section in &expanded.sections {
                let draws = &by_slot[&section.slot()];
                let layer = if section.cutout() {
                    &draws.cutout
                } else {
                    &draws.solid
                };
                assert_eq!(section.block, layer.block);
                let mut ranges = Vec::new();
                melon_render::expand::layer_runs(&layer.ends, section.shown(), |a, b| {
                    ranges.push((a, b));
                });
                assert_eq!(usize::from(section.draws), ranges.len());
                from_sections[usize::from(section.cutout())].push((section.slot(), ranges));
            }
            assert!(!from_chunks[0].is_empty() && !from_chunks[1].is_empty());
            assert_eq!(from_chunks, from_sections);
            assert_eq!(chunks.count.vertices, expanded.count.vertices);
            assert_eq!(chunks.count.draws, expanded.count.draws);
            assert_eq!(chunks.translucent, expanded.translucent);
            meshes.recycle((chunks.solid, chunks.cutout, chunks.translucent), chunks.sections);
        }
    }

    #[test]
    fn sorted_draws_come_out_like_a_stable_sort_by_distance() {
        let draw = |id: usize| ChunkDraw {
            mesh: TerrainMesh::Quads(QuadMeshId(id)),
            origin: IVec3::new(id as i32, 0, 0),
            range: (0..id as u32).into(),
            cull: None,
        };
        // Runs of one distance in a row too, as a section's draws come.
        let distances = [9.0, 1.0, 1.0, 4.0, 1.0, 0.0, 9.0, 9.0, 9.0, 2.5, 1.0];
        let mut near = SortedDraws::default();
        let mut far = SortedDraws::default();
        for (id, &distance) in distances.iter().enumerate() {
            near.push(distance, draw(id), (0, 1));
            far.push_far_first(distance, draw(id), (0, 1));
        }
        let mut by_sort: Vec<(f32, usize)> = distances.iter().copied().zip(0..).collect();
        by_sort.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = Vec::new();
        near.drain_sorted_into(&mut out);
        let expected: Vec<ChunkDraw> = by_sort.iter().map(|&(_, id)| draw(id)).collect();
        assert_eq!(out, expected);
        by_sort.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut out = Vec::new();
        far.drain_sorted_into(&mut out);
        let expected: Vec<ChunkDraw> = by_sort.iter().map(|&(_, id)| draw(id)).collect();
        assert_eq!(out, expected);
        assert!(near.draws.is_empty() && near.keys.is_empty());
    }

    /// A section at `pos` with two flat translucent quads, at heights `low` and `high`.
    fn two_quads(pos: SectionPos, low: f32, high: f32, eye: DVec3) -> FinishedMesh {
        let quad = |y: f32| {
            [[0.0, y, 0.0], [0.0, y, 1.0], [1.0, y, 1.0], [1.0, y, 0.0]]
                .map(|p| WorldVertex::new(p, [0.0, 0.0], [1.0; 3]))
        };
        let mut faces = TranslucentMesh {
            vertices: [quad(low), quad(high)].concat(),
            ..TranslucentMesh::default()
        };
        faces.sort(terrain::sort_eye(eye, pos, Height::OVERWORLD.min_y));
        FinishedMesh {
            pos,
            mesh: SectionMesh {
                base_y: Height::OVERWORLD.min_y,
                translucent: faces,
                ..SectionMesh::default()
            },
            sync: true,
        }
    }

    /// The heights of a section's translucent quads, in drawing order.
    fn heights(meshes: &LevelMeshes, pos: SectionPos) -> Vec<f32> {
        let translucent = meshes.columns[&(pos.0, pos.2)].sections[pos.1 as usize]
            .translucent
            .as_ref()
            .unwrap();
        translucent
            .faces
            .sorted_quads()
            .iter()
            .map(|q| q[0].pos[1])
            .collect()
    }

    #[test]
    fn the_point_of_view_is_clamped_to_the_neighbours() {
        assert_eq!(
            point_of_view(IVec3::new(5, 4, -3), (0, 4, 0)),
            IVec3::new(1, 0, -1)
        );
        assert_eq!(
            section_of_eye(DVec3::new(-0.5, -64.0, 15.9), Height::OVERWORLD.min_y),
            IVec3::new(-1, 0, 0)
        );
    }

    #[test]
    fn translucent_faces_are_sorted_again_as_the_camera_moves() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (0, 4, 0);
        let above = DVec3::new(0.5, 40.0, 0.5);
        let mut meshes = LevelMeshes::default();
        meshes.upload_to(&mut sink, vec![two_quads(pos, 2.0, 12.0, above)]);
        assert_eq!(heights(&meshes, pos), [2.0, 12.0]);
        meshes.seen_translucent = vec![pos];
        meshes.resort_with(&mut sink, above);
        meshes.resort_with(&mut sink, DVec3::new(0.5, -30.0, 0.5));
        assert_eq!(heights(&meshes, pos), [12.0, 2.0]);
    }

    /// Faces count from their section's corner (y 32 here), as vanilla's
    /// do: from y 36 the quad at 12 (44 in the world) is the far one.
    #[test]
    fn faces_are_sorted_from_their_sections_corner() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (0, 6, 0);
        let above = DVec3::new(0.5, 60.0, 0.5);
        let mut meshes = LevelMeshes::default();
        meshes.upload_to(&mut sink, vec![two_quads(pos, 2.0, 12.0, above)]);
        assert_eq!(heights(&meshes, pos), [2.0, 12.0]);
        meshes.seen_translucent = vec![pos];
        meshes.resort_with(&mut sink, DVec3::new(0.5, 36.0, 0.5));
        assert_eq!(heights(&meshes, pos), [12.0, 2.0]);
    }

    #[test]
    fn an_unchanged_view_is_not_sorted_again() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (40, 4, 40);
        let eye = DVec3::new(0.5, 40.0, 0.5);
        let mut meshes = LevelMeshes::default();
        meshes.upload_to(&mut sink, vec![two_quads(pos, 2.0, 12.0, eye)]);
        meshes.seen_translucent = vec![pos];
        let mesh = |meshes: &LevelMeshes| {
            meshes.columns[&(40, 40)].sections[4]
                .translucent
                .as_ref()
                .unwrap()
                .mesh
        };
        let before = mesh(&meshes);
        meshes.resort_with(&mut sink, eye);
        meshes.resort_with(&mut sink, eye + DVec3::X);
        assert_eq!(mesh(&meshes), before);
    }

    #[test]
    fn diving_within_a_section_sorts_far_sections_again() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (10, 4, 0);
        let above = DVec3::new(0.5, 13.5, 8.5);
        let mut meshes = LevelMeshes::default();
        meshes.upload_to(&mut sink, vec![two_quads(pos, 2.0, 12.0, above)]);
        meshes.seen_translucent = vec![pos];
        meshes.resort_with(&mut sink, above);
        assert_eq!(heights(&meshes, pos), [2.0, 12.0]);
        meshes.resort_with(&mut sink, DVec3::new(0.5, 6.5, 8.5));
        assert_eq!(heights(&meshes, pos), [12.0, 2.0]);
    }

    #[test]
    fn a_mesh_sorted_before_the_camera_moved_is_sorted_again() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (10, 4, 0);
        let below = DVec3::new(0.5, 6.5, 8.5);
        let mut meshes = LevelMeshes::default();
        meshes.resort_with(&mut sink, below);
        meshes.upload_to(
            &mut sink,
            vec![two_quads(pos, 2.0, 12.0, DVec3::new(0.5, 13.5, 8.5))],
        );
        meshes.seen_translucent = vec![pos];
        meshes.resort_with(&mut sink, below);
        assert_eq!(heights(&meshes, pos), [12.0, 2.0]);
    }

    #[test]
    fn diving_past_the_water_plane_in_the_same_block_sorts_again() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (0, 4, 0);
        let above = DVec3::new(0.5, 8.95, 0.5);
        let mut meshes = LevelMeshes::default();
        meshes.upload_to(&mut sink, vec![two_quads(pos, 8.888889, 9.0, above)]);
        meshes.seen_translucent = vec![pos];
        meshes.resort_with(&mut sink, above);
        assert_eq!(heights(&meshes, pos), [8.888889, 9.0]);
        meshes.resort_with(&mut sink, DVec3::new(0.5, 8.5, 0.5));
        assert_eq!(heights(&meshes, pos), [9.0, 8.888889]);
    }

    #[test]
    fn looking_up_after_a_dive_sorts_ice_that_was_not_seen() {
        let mut next = 0;
        let mut sink = Headless(|| {
            next += 1;
            next
        });
        let pos = (0, 4, 0);
        let above = DVec3::new(0.5, 8.95, 0.5);
        let mut meshes = LevelMeshes::default();
        meshes.upload_to(&mut sink, vec![two_quads(pos, 8.888889, 9.0, above)]);
        assert_eq!(heights(&meshes, pos), [8.888889, 9.0]);
        meshes.resort_with(&mut sink, DVec3::new(0.5, 8.5, 0.5));
        assert_eq!(heights(&meshes, pos), [9.0, 8.888889]);
    }

    #[test]
    fn a_newer_mesh_replaces_the_one_waiting() {
        let mut sink = Headless(|| 0);
        let mut meshes = LevelMeshes::default();
        let eye = DVec3::ZERO;
        let mut waiting = two_quads((0, 4, 0), 2.0, 12.0, eye);
        waiting.sync = false;
        meshes.waiting.push_back(waiting);
        let newer = FinishedMesh {
            sync: false,
            ..two_quads((0, 4, 0), 3.0, 13.0, eye)
        };
        meshes.upload_to(&mut sink, vec![newer]);
        assert!(meshes.waiting.is_empty());
        assert!(heights(&meshes, (0, 4, 0)).contains(&3.0));
    }

    #[test]
    fn faces_are_seen_only_from_their_front_side() {
        let origin = DVec3::new(16.0, 0.0, 32.0);
        let up = Direction::Up.index();
        let down = Direction::Down.index();
        let east = Direction::East.index();
        assert!(faces_camera(
            up,
            origin,
            5,
            DVec3::new(20.0, 40.0, 40.0),
            Height::OVERWORLD.min_y
        ));
        assert!(!faces_camera(
            up,
            origin,
            5,
            DVec3::new(20.0, 10.0, 40.0),
            Height::OVERWORLD.min_y
        ));
        assert!(faces_camera(
            down,
            origin,
            5,
            DVec3::new(20.0, 10.0, 40.0),
            Height::OVERWORLD.min_y
        ));
        assert!(faces_camera(
            up,
            origin,
            5,
            DVec3::new(20.0, 20.0, 40.0),
            Height::OVERWORLD.min_y
        ));
        assert!(faces_camera(
            down,
            origin,
            5,
            DVec3::new(20.0, 20.0, 40.0),
            Height::OVERWORLD.min_y
        ));
        assert!(!faces_camera(
            east,
            origin,
            5,
            DVec3::new(10.0, 20.0, 40.0),
            Height::OVERWORLD.min_y
        ));
        assert!(faces_camera(
            terrain::SLANTED,
            origin,
            5,
            DVec3::ZERO,
            Height::OVERWORLD.min_y
        ));
    }
}
