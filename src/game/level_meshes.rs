//! The level's terrain on the GPU (vanilla `LevelRenderer`'s section meshes).

use std::collections::{HashSet, VecDeque};
use std::ops::Range;

use glam::{DVec3, IVec3, Vec3};

use crate::math::{ChunkMap, SECTION_SIZE, section_coord};
use crate::renderer::{
    ChunkDraw, MeshId, QuadMeshId, Renderer, SectionCull, TerrainMesh, TerrainQuad, WorldVertex,
};
use crate::world::block::direction::Direction;
use crate::world::block_entity::BlockEntities;
use crate::world::block_models::BlockModels;
use crate::world::culling::Culler;
use crate::world::culling::occlusion::SectionPos;
use crate::world::dimension::Height;
use crate::world::level::{FinishedMesh, Level};
use crate::world::terrain::{self, LayerMesh, SectionMesh, TranslucentMesh};

/// How many mesh bytes go to the GPU per frame at most.
const UPLOAD_BUDGET: usize = 16 << 20;
/// `Octree.isClose`'s 32 blocks.
const NEARBY_SECTIONS: i32 = 2;
/// Bytes of sorted-again faces uploaded per frame at most, nearest sections first.
const RESORT_BUDGET: usize = 4 << 20;

/// Where section meshes go.
trait MeshSink {
    fn quads(&mut self, quads: &[TerrainQuad]) -> anyhow::Result<QuadMeshId>;
    fn vertices(&mut self, vertices: &[WorldVertex]) -> anyhow::Result<MeshId>;
    fn free(&mut self, mesh: TerrainMesh);
}

impl MeshSink for Renderer {
    fn quads(&mut self, quads: &[TerrainQuad]) -> anyhow::Result<QuadMeshId> {
        self.upload_quads(quads)
    }

    fn vertices(&mut self, vertices: &[WorldVertex]) -> anyhow::Result<MeshId> {
        self.upload_mesh(vertices)
    }

    fn free(&mut self, mesh: TerrainMesh) {
        match mesh {
            TerrainMesh::Quads(id) => self.free_quads(id),
            TerrainMesh::Vertices(id) => self.free_mesh(id),
        }
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

    fn free(&mut self, _: TerrainMesh) {}
}

/// A section's solid or cutout quads on the GPU, by facing group.
struct QuadLayer {
    mesh: QuadMeshId,
    groups: [Range<u32>; terrain::FACINGS],
}

/// A section's translucent faces on the GPU, and the list they came from.
struct Translucent {
    mesh: MeshId,
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
            Ok(Some(QuadLayer {
                mesh: MeshSink::quads(sink, &layer.items)?,
                groups: layer.groups,
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
        let faces = built.translucent;
        if !faces.vertices.is_empty() {
            let eye = faces.sorted_from.as_dvec3() + corner_of(pos);
            self.translucent = Some(Translucent {
                mesh: sink.vertices(&faces.vertices)?,
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
            sink.free(TerrainMesh::Vertices(translucent.mesh));
        }
    }
}

/// A section layer's quads as the culling pass reads them: the mesh and
/// where each facing group ends (they follow one another from 0).
#[derive(Clone, Copy)]
struct QuadRuns {
    mesh: u32,
    ends: [u32; terrain::FACINGS],
}

/// No mesh: the layer has nothing to draw.
const NO_MESH: u32 = u32::MAX;

impl QuadRuns {
    const NONE: Self = Self {
        mesh: NO_MESH,
        ends: [0; terrain::FACINGS],
    };

    fn of(layer: Option<&QuadLayer>) -> Self {
        layer.map_or(Self::NONE, |layer| Self {
            mesh: mesh_index(layer.mesh.0),
            ends: layer.groups.clone().map(|group| group.end),
        })
    }
}

/// What drawing a section reads of its [`SectionMeshes`], packed in 80
/// bytes beside its neighbours: the culling pass touches a line or two
/// per section instead of the whole struct and its translucent faces.
#[derive(Clone, Copy)]
struct SectionDraws {
    solid: QuadRuns,
    cutout: QuadRuns,
    /// Mesh and vertex count, [`NO_MESH`] for none.
    lava: (u32, u32),
    translucent: (u32, u32),
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
            translucent: (NO_MESH, 0),
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
            translucent: meshes.translucent.as_ref().map_or((NO_MESH, 0), |t| {
                (mesh_index(t.mesh.0), t.faces.vertices.len() as u32)
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

/// The chunk-local origin of section `pos`'s vertices.
fn corner_of((x, _, z): SectionPos) -> DVec3 {
    DVec3::new(
        f64::from(x * SECTION_SIZE),
        0.0,
        f64::from(z * SECTION_SIZE),
    )
}

/// `eye` from a section's corner, in the floats its faces are sorted by.
fn from_corner(eye: DVec3, pos: SectionPos) -> Vec3 {
    (eye - corner_of(pos)).as_vec3()
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
    /// Draws with their distance, sorted before they go out.
    solid: SortedDraws,
    cutout: SortedDraws,
    translucent: SortedDraws,
    /// Last frame's draw lists, handed back by [`Self::recycle`] to be reused.
    spare_solid: Vec<ChunkDraw>,
    spare_cutout: Vec<ChunkDraw>,
    spare_translucent: Vec<ChunkDraw>,
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
            solid: SortedDraws::default(),
            cutout: SortedDraws::default(),
            translucent: SortedDraws::default(),
            spare_solid: Vec::new(),
            spare_cutout: Vec::new(),
            spare_translucent: Vec::new(),
            seen_translucent: Vec::new(),
            slots: Slots::default(),
        }
    }
}

impl LevelMeshes {
    /// The level's bottom: where its first section starts.
    pub fn set_min_y(&mut self, min_y: i32) {
        self.min_y = min_y;
    }

    /// Uploads finished meshes in place of the sections' old ones.
    pub fn upload(&mut self, renderer: &mut Renderer, finished: Vec<FinishedMesh>) {
        self.upload_to(renderer, finished);
    }

    fn upload_to(&mut self, sink: &mut impl MeshSink, finished: Vec<FinishedMesh>) {
        if !finished.is_empty() && !self.waiting.is_empty() {
            let newer: crate::math::SectionSet = finished.iter().map(|mesh| mesh.pos).collect();
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
            .filter(|&y| y < 32)
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
            .filter(|&y| y < 32)
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
        if y >= 32 {
            return;
        }
        let meshes = match SectionMeshes::upload(pos, mesh, sink) {
            Ok(meshes) => meshes,
            Err(e) => {
                crate::warn!("Section mesh not uploaded: {e:#}");
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
        let mut bytes = 0;
        for (_, pos, nearby) in due {
            if nearby || bytes < RESORT_BUDGET {
                bytes += self.resort_section(sink, pos, eye, camera);
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
            .camera_crossed_a_face(from_corner(eye, pos))
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

    /// Sorts one section's translucent faces from `eye` and uploads them.
    fn resort_section(
        &mut self,
        sink: &mut impl MeshSink,
        pos: SectionPos,
        eye: DVec3,
        camera: IVec3,
    ) -> usize {
        let Some(translucent) = self.translucent_mut(pos) else {
            return 0;
        };
        translucent.faces.sort(from_corner(eye, pos));
        translucent.view = point_of_view(camera, pos);
        translucent.block = eye.floor().as_ivec3();
        match sink.vertices(&translucent.faces.vertices) {
            Ok(mesh) => {
                sink.free(TerrainMesh::Vertices(std::mem::replace(
                    &mut translucent.mesh,
                    mesh,
                )));
            }
            Err(e) => crate::warn!("Translucent mesh not resorted: {e:#}"),
        }
        let (bytes, mesh) = (
            std::mem::size_of_val(translucent.faces.vertices.as_slice()),
            mesh_index(translucent.mesh.0),
        );
        if let Some(draws) = self
            .columns
            .get_mut(&(pos.0, pos.2))
            .and_then(|column| column.draws.get_mut(pos.1 as usize))
        {
            draws.translucent.0 = mesh;
        }
        bytes
    }

    /// The parts of meshes `culler` lets through: per section, its solid
    /// TODO(perf): the biggest per-frame cost left on the render thread
    /// (~0.1 ms at view distance 32): ~5.800 `ChunkDraw`s pushed, sorted and
    /// copied again every frame, though the cache misses are gone. Next step:
    /// cull on the GPU (a compute pass over the `SectionDraws`, writing the
    /// indirect commands itself), the CPU only uploading what changed.
    pub fn visible_sections(&mut self, culler: &Culler) -> VisibleChunks {
        let eye = culler.eye();
        let mut visible = VisibleChunks {
            solid: std::mem::take(&mut self.spare_solid),
            cutout: std::mem::take(&mut self.spare_cutout),
            translucent: std::mem::take(&mut self.spare_translucent),
            count: SectionCount::default(),
            slots: self.slots.next,
        };
        visible.solid.clear();
        visible.cutout.clear();
        visible.translucent.clear();
        self.solid.clear();
        self.cutout.clear();
        self.translucent.clear();
        self.seen_translucent.clear();
        let min_y = self.min_y;
        let vertical = |group: usize| {
            (0..32).fold(0u32, |mask, y| {
                mask | u32::from(faces_camera(group, DVec3::ZERO, y, eye, min_y)) << y
            })
        };
        let up = vertical(Direction::Up.index());
        let down = vertical(Direction::Down.index());
        let cull = culler.section_cull();
        for ((cx, cz), column) in self.columns.iter() {
            let filled = column.filled;
            if filled == 0 {
                continue;
            }
            visible.count.total += filled.count_ones() as usize;
            let in_view = culler.reached_of((cx, cz), cull.column((cx, cz)) & filled);
            if in_view == 0 {
                continue;
            }
            let (x, z) = (f64::from(cx * SECTION_SIZE), f64::from(cz * SECTION_SIZE));
            let bottom = |y: u32| f64::from(min_y + y as i32 * SECTION_SIZE);
            let origin = DVec3::new(x, 0.0, z);
            // Bit per facing group: whether some face of it can face the eye.
            let sides = (0..terrain::FACINGS).fold(0u32, |mask, group| {
                mask | u32::from(faces_camera(group, origin, 0, eye, min_y)) << group
            });
            let vertical = 1 << Direction::Up.index() | 1 << Direction::Down.index();
            let corner = (cx * SECTION_SIZE, cz * SECTION_SIZE);
            let quads_origin = IVec3::new(corner.0, min_y, corner.1);
            let vertices_origin = IVec3::new(corner.0, 0, corner.1);
            let mut rest = in_view;
            while rest != 0 {
                let y = rest.trailing_zeros();
                rest &= rest - 1;
                let Some(section) = column.draws.get(y as usize) else {
                    continue;
                };
                visible.count.drawn += 1;
                let center = DVec3::new(x + 8.0, bottom(y) + 8.0, z + 8.0);
                let distance = center.distance_squared(eye) as f32;
                let shown = sides & !vertical
                    | (up >> y & 1) << Direction::Up.index()
                    | (down >> y & 1) << Direction::Down.index();
                for (layer, list) in [
                    (&section.solid, &mut self.solid),
                    (&section.cutout, &mut self.cutout),
                ] {
                    if layer.mesh == NO_MESH {
                        continue;
                    }
                    let mesh = TerrainMesh::Quads(QuadMeshId(layer.mesh as usize));
                    let cull = Some(SectionCull {
                        slot: section.slot,
                        bottom: y as i32 * SECTION_SIZE,
                    });
                    let mut flush = |run: &mut Option<Range<u32>>| {
                        if let Some(range) = run.take() {
                            list.push(
                                distance,
                                ChunkDraw {
                                    mesh,
                                    origin: quads_origin,
                                    range: range.into(),
                                    cull,
                                },
                            );
                        }
                    };
                    let mut run: Option<Range<u32>> = None;
                    let mut start = 0;
                    for (group, &end) in layer.ends.iter().enumerate() {
                        let range = start..end;
                        start = end;
                        if range.is_empty() {
                            continue;
                        }
                        if shown >> group & 1 == 0 {
                            flush(&mut run);
                            continue;
                        }
                        match &mut run {
                            Some(open) => open.end = range.end,
                            None => run = Some(range),
                        }
                    }
                    flush(&mut run);
                }
                let (lava, count) = section.lava;
                if lava != NO_MESH {
                    self.solid.push(
                        distance,
                        ChunkDraw {
                            mesh: TerrainMesh::Vertices(MeshId(lava as usize)),
                            origin: vertices_origin,
                            range: (0..count).into(),
                            cull: None,
                        },
                    );
                }
                let (translucent, count) = section.translucent;
                if translucent != NO_MESH {
                    self.seen_translucent.push((cx, y as i32, cz));
                    self.translucent.push_far_first(
                        distance,
                        ChunkDraw {
                            mesh: TerrainMesh::Vertices(MeshId(translucent as usize)),
                            origin: vertices_origin,
                            range: (0..count).into(),
                            cull: None,
                        },
                    );
                }
            }
        }
        for list in [&self.solid, &self.cutout, &self.translucent] {
            visible.count.vertices += list.vertices;
            visible.count.draws += list.draws.len();
        }
        self.solid.drain_sorted_into(&mut visible.solid);
        self.cutout.drain_sorted_into(&mut visible.cutout);
        self.translucent.drain_sorted_into(&mut visible.translucent);
        visible
    }

    /// Takes back the draw lists [`Self::visible_sections`] handed out, to reuse them.
    pub fn recycle(
        &mut self,
        solid: Vec<ChunkDraw>,
        cutout: Vec<ChunkDraw>,
        translucent: Vec<ChunkDraw>,
    ) {
        self.spare_solid = solid;
        self.spare_cutout = cutout;
        self.spare_translucent = translucent;
    }
}

/// One layer's draws in the order culling found them, sorted by distance
/// as a stable sort would. Draws pushed one after another at the same
/// distance (a section's) are one run with one sort key ([`draw_key`]):
/// sorting the runs' 8-byte keys and copying each run whole moves far fewer
/// bytes than sorting the draws, and reads them in order.
#[derive(Default)]
struct SortedDraws {
    draws: Vec<ChunkDraw>,
    /// Per run, its key; the low half counts the runs.
    keys: Vec<u64>,
    /// Per run, where its draws start.
    starts: Vec<u32>,
    /// The draws' vertices, for F3.
    vertices: usize,
    /// [`radix_sort`]'s second buffer.
    scratch: Vec<u64>,
}

impl SortedDraws {
    /// Adds `draw`, `distance` (squared) from the eye, near ones first.
    fn push(&mut self, distance: f32, draw: ChunkDraw) {
        self.push_keyed(draw_key(distance, self.keys.len()), draw);
    }

    /// Adds `draw`, far ones first: translucent faces blend back to front.
    fn push_far_first(&mut self, distance: f32, draw: ChunkDraw) {
        self.push_keyed(draw_key(distance, self.keys.len()) ^ !0 << 32, draw);
    }

    /// Adds `draw` to the last run if `key` sorts it with it, else to a new run.
    fn push_keyed(&mut self, key: u64, draw: ChunkDraw) {
        if self.keys.last().is_none_or(|&last| last >> 32 != key >> 32) {
            self.keys.push(key);
            self.starts.push(self.draws.len() as u32);
        }
        self.vertices += draw.vertex_count();
        self.draws.push(draw);
    }

    fn clear(&mut self) {
        self.draws.clear();
        self.keys.clear();
        self.starts.clear();
        self.vertices = 0;
    }

    /// Appends the draws to `out` in key order and empties `self`.
    fn drain_sorted_into(&mut self, out: &mut Vec<ChunkDraw>) {
        radix_sort(&mut self.keys, &mut self.scratch);
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

/// Sorts [`draw_key`]s as `sort_unstable` does. Their low halves count up
/// in the order the draws were pushed, so a stable sort by the high half
/// alone is the same order: four passes of a byte each (`O(n)`, no
/// compares to mispredict), the passes whose byte is the same in every
/// key left out.
fn radix_sort(keys: &mut Vec<u64>, scratch: &mut Vec<u64>) {
    const SMALL: usize = 64;
    if keys.len() <= SMALL || keys.len() > u32::MAX as usize {
        keys.sort_unstable();
        return;
    }
    debug_assert!(keys.windows(2).all(|w| (w[0] as u32) < (w[1] as u32)));
    let mut counts = [[0u32; 256]; 4];
    for &key in keys.iter() {
        for (pass, count) in counts.iter_mut().enumerate() {
            count[(key >> (32 + 8 * pass)) as usize & 0xFF] += 1;
        }
    }
    scratch.clear();
    scratch.resize(keys.len(), 0);
    for (pass, count) in counts.iter().enumerate() {
        if count.iter().any(|&n| n as usize == keys.len()) {
            continue;
        }
        let mut at = [0u32; 256];
        let mut sum = 0;
        for (slot, &n) in at.iter_mut().zip(count) {
            *slot = sum;
            sum += n;
        }
        for &key in keys.iter() {
            let digit = (key >> (32 + 8 * pass)) as usize & 0xFF;
            scratch[at[digit] as usize] = key;
            at[digit] += 1;
        }
        std::mem::swap(keys, scratch);
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
                        crate::warn!("Block entity mesh failed: {e:#}");
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

/// The block entities' geometry (chests, bells, books...), in two meshes:
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

    /// The radix sort gives what a comparison sort does, near and far
    /// keys, short lists and long.
    #[test]
    fn radix_sort_matches_sort_unstable() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut scratch = Vec::new();
        for len in [0, 1, 63, 64, 65, 500, 6000] {
            for spread in [1u64, 16, 1 << 20, u64::MAX] {
                let mut keys: Vec<u64> = (0..len)
                    .map(|i| {
                        let distance = (next() % spread) as f32;
                        let key = draw_key(distance, i);
                        if next() % 3 == 0 { key ^ !0 << 32 } else { key }
                    })
                    .collect();
                let mut want = keys.clone();
                want.sort_unstable();
                radix_sort(&mut keys, &mut scratch);
                assert_eq!(keys, want, "{len} keys spread {spread}");
            }
        }
    }

    #[test]
    fn draw_keys_order_like_distance_then_index() {
        let mut seed = 0x0bad_5eed_1234_5678u64;
        let mut random = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
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
        use crate::world::culling::frustum::Frustum;
        use crate::world::culling::{CullSettings, Culler};
        let layer = || LayerMesh {
            items: vec![TerrainQuad([0; 5]); 70],
            groups: std::array::from_fn(|g| g as u32 * 10..g as u32 * 10 + 10),
        };
        let mut meshes = LevelMeshes::default();
        meshes.set_min_y(-64);
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
            crate::renderer::world_projection(70.0, 16.0 / 9.0, 512.0) * view,
        );
        let culler = Culler::new(frustum, None, eye, (-64, 24), CullSettings::default());
        let rounds = 300;
        let mut draws = (0, 0, 0);
        let started = std::time::Instant::now();
        for _ in 0..rounds {
            let visible = meshes.visible_sections(&culler);
            draws = (
                visible.count.draws,
                visible.count.drawn,
                visible.count.total,
            );
            meshes.recycle(visible.solid, visible.cutout, visible.translucent);
        }
        let each = started.elapsed() / rounds;
        eprintln!("visible_sections: {each:?} per frame, (draws, sections drawn, of) {draws:?}");
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
            near.push(distance, draw(id));
            far.push_far_first(distance, draw(id));
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
        faces.sort(from_corner(eye, pos));
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
            .vertices
            .chunks(4)
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
