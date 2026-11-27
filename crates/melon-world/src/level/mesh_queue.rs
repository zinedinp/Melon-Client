//! Which sections need a new mesh, and getting them built, as vanilla's
//! `SectionRenderDispatcher` does.

use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::ChunkBuilder;
use crate::block_models::BlockModels;
use crate::block_tint::TintSettings;
use crate::chunk::Chunk;
use crate::culling::occlusion::SectionPos;
use crate::dimension::Height;
use crate::fluid::FluidSprites;
use melon_core::math::{ChunkMap, SECTION_SIZE, SectionMap, section_coord};
use crate::terrain::{self, SectionMesh};

use super::mesher::{Job, Mesher};
use super::{AROUND, LoadedChunk, MeshStats};

/// Chunks this close to the camera (squared, in chunks) are meshed without waiting for their
/// neighbours.
const NEAR_CHUNKS_SQ: i32 = 2;
/// `LevelRenderer.compileSections`.
const NEARBY_SQ: i64 = 768;
/// `SectionTaskDynamicQueue.MAX_RECOMPILE_QUOTA`.
const MAX_RECOMPILE_QUOTA: u8 = 2;
/// Sections per column at most: the masks the renderer keeps are `u32`.
const MAX_SECTIONS: i32 = 32;

type Chunks = ChunkMap<LoadedChunk>;

/// A loaded section's meshing state.
#[derive(Default)]
struct Versions {
    /// Bumped (from [`MeshQueue::next_version`]) whenever the blocks this section's mesh shows
    /// change.
    current: u64,
    /// The version the current mesh was built from.
    meshed: u64,
    /// The version of the mesh job in flight, if any.
    job: Option<u64>,
}

/// A section waiting for its mesh.
#[derive(Clone, Copy, Default)]
struct Dirty {
    /// Changed by the player (`SectionUpdateRenderState.playerChanged`).
    player: bool,
    /// Found to hold blocks since it was queued: not looked at again.
    filled: bool,
}

impl Dirty {
    /// Both waits as one: by the player if either was, known filled only
    /// if both were.
    fn merged(self, other: Self) -> Self {
        Self {
            player: self.player || other.player,
            filled: self.filled && other.filled,
        }
    }
}

/// A section's new mesh.
pub(super) struct Built {
    pub pos: SectionPos,
    pub mesh: SectionMesh,
    /// Built on this thread for the frame now.
    pub sync: bool,
}

#[derive(Default)]
pub(super) struct MeshQueue {
    /// Every loaded section's versions.
    versions: SectionMap<Versions>,
    /// Sections waiting to be (re)meshed.
    dirty: SectionMap<Dirty>,
    /// Waiting sections of columns not [`ready`] yet, by column: put aside
    /// so a frame doesn't look at them again, and back in `dirty` once a
    /// chunk arrives or the camera moves to another chunk.
    parked: ChunkMap<Vec<(i32, Dirty)>>,
    /// Bumped whenever a chunk arrives: then a parked column may be ready.
    arrivals: u64,
    /// The camera's chunk and `arrivals` when `parked` was last looked at.
    parked_at: Option<((i32, i32), u64)>,
    /// Started on the first [`MeshQueue::run`].
    mesher: Option<Mesher>,
    /// The block models `mesher`'s workers build with.
    mesher_models: Option<Arc<BlockModels>>,
    /// Source of versions, never reused.
    next_version: u64,
    /// Dirty sections that could be meshed, as of the last [`MeshQueue::run`].
    ready_count: usize,
    /// The "Smooth Lighting" option is off: faces are lit flat.
    flat_lighting: bool,
    /// The "See-Through Leaves" option is off: leaves are opaque.
    opaque_leaves: bool,
    /// The "Chunk Builder" option.
    chunk_builder: ChunkBuilder,
    /// Rebuilds handed out in a row (`SectionTaskDynamicQueue`'s quota).
    recompiles: u8,
    /// `Options.biomeBlendRadius` last applied: a change remeshes.
    biome_blend: u8,
}

impl MeshQueue {
    /// A chunk of `sections` sections arrived: each needs its first mesh.
    pub fn added(&mut self, key: (i32, i32), sections: usize) {
        self.arrivals += 1;
        for s in 0..sections as i32 {
            self.versions.insert((key.0, s, key.1), Versions::default());
        }
        self.queue_column(key);
    }

    /// A chunk was unloaded: nothing of it is meshed any more.
    pub fn removed(&mut self, key: (i32, i32)) {
        for s in 0..MAX_SECTIONS {
            self.versions.remove(&(key.0, s, key.1));
            self.dirty.remove(&(key.0, s, key.1));
        }
        self.parked.remove(&key);
    }

    /// The "Smooth Lighting" option.
    pub fn set_smooth_lighting(&mut self, on: bool) {
        if self.flat_lighting != on {
            return;
        }
        self.flat_lighting = !on;
        self.queue_all();
    }

    /// The "See-Through Leaves" option.
    pub fn set_cutout_leaves(&mut self, on: bool) {
        if self.opaque_leaves != on {
            return;
        }
        self.opaque_leaves = !on;
        self.queue_all();
    }

    /// Meshes every known section again.
    fn queue_all(&mut self) {
        let all: Vec<_> = self.versions.keys().copied().collect();
        for pos in all {
            self.queue(pos, false);
        }
    }

    /// The "Chunk Builder" option.
    pub fn set_chunk_builder(&mut self, builder: ChunkBuilder) {
        self.chunk_builder = builder;
    }

    /// `Options.biomeBlendRadius`.
    pub fn set_biome_blend(&mut self, radius: u8) {
        if self.biome_blend == radius {
            return;
        }
        self.biome_blend = radius;
        let all: Vec<_> = self.versions.keys().copied().collect();
        for pos in all {
            self.queue(pos, false);
        }
    }

    pub fn clear(&mut self) {
        self.versions.clear();
        self.dirty.clear();
        self.parked.clear();
    }

    /// Queues every loaded section: the block models changed.
    pub fn remesh_all(&mut self) {
        let all: Vec<_> = self.versions.keys().copied().collect();
        for pos in all {
            self.queue(pos, false);
        }
    }

    /// Marks a loaded section for a remesh; `player` if the player's own edit caused it.
    pub fn queue(&mut self, pos: SectionPos, player: bool) {
        let Some(versions) = self.versions.get_mut(&pos) else {
            return;
        };
        self.next_version += 1;
        versions.current = self.next_version;
        let dirty = self.dirty.entry(pos).or_default();
        dirty.player |= player;
        dirty.filled = false;
    }

    /// Marks every section of a loaded chunk.
    pub fn queue_column(&mut self, key: (i32, i32)) {
        for s in 0..MAX_SECTIONS {
            self.queue((key.0, s, key.1), false);
        }
    }

    /// Queues every section of the loaded chunks at `offsets` from `key`.
    pub fn queue_around(&mut self, key: (i32, i32), offsets: &[(i32, i32)]) {
        for &(dx, dz) in offsets {
            self.queue_column((key.0 + dx, key.1 + dz));
        }
    }

    /// Builds the sections the Chunk Builder wants now and takes what the workers finished.
    pub fn run(
        &mut self,
        chunks: &Chunks,
        (models, fluids): (&Arc<BlockModels>, FluidSprites),
        camera: DVec3,
        height: Height,
        cardinal: [f32; 6],
        tints: &TintSettings,
    ) -> Vec<Built> {
        if self
            .mesher_models
            .as_ref()
            .is_some_and(|old| !Arc::ptr_eq(old, models))
        {
            self.mesher = None;
            for versions in self.versions.values_mut() {
                versions.job = None;
            }
            self.remesh_all();
        }
        self.mesher_models = Some(Arc::clone(models));
        let mut finished = Vec::new();
        let eye = camera.floor().as_ivec3();
        let center = (section_coord(eye.x), section_coord(eye.z));

        let mut empty = Vec::new();
        let mut sync = Vec::new();
        let mut initial = Vec::new();
        let mut rebuild = Vec::new();
        self.unpark(chunks, center);
        let mut ready_columns = ChunkMap::<bool>::default();
        let mut park = Vec::new();
        for (&pos, dirty) in &mut self.dirty {
            let Some(versions) = self.versions.get(&pos) else {
                continue;
            };
            if versions.job.is_some() {
                continue;
            }
            let column = (pos.0, pos.2);
            let Some(loaded) = chunks.get(&column) else {
                continue;
            };
            if !*ready_columns
                .entry(column)
                .or_insert_with(|| ready(chunks, column, center))
            {
                park.push(pos);
                continue;
            }
            if !dirty.filled {
                let section = usize::try_from(pos.1)
                    .ok()
                    .and_then(|s| loaded.chunk.sections.get(s));
                if section.is_none_or(|section| terrain::is_empty(section)) {
                    empty.push((pos, versions.current));
                    continue;
                }
                dirty.filled = true;
            }
            let distance = distance_sq(pos, eye, height.min_y);
            if builds_now(self.chunk_builder, dirty.player, distance) {
                sync.push(Job {
                    pos,
                    version: versions.current,
                    chunks: neighborhood_of(chunks, column),
                    smooth_lighting: !self.flat_lighting,
                    cutout_leaves: !self.opaque_leaves,
                    cardinal,
                    camera,
                    tints: tints.clone(),
                });
            } else if versions.meshed == 0 {
                initial.push((distance, pos));
            } else {
                rebuild.push((distance, pos));
            }
        }
        for pos in park {
            if let Some(dirty) = self.dirty.remove(&pos) {
                let sections = self.parked.entry((pos.0, pos.2)).or_default();
                match sections.iter_mut().find(|(s, _)| *s == pos.1) {
                    Some((_, parked)) => *parked = parked.merged(dirty),
                    None => sections.push((pos.1, dirty)),
                }
            }
        }
        for (pos, version) in empty {
            self.dirty.remove(&pos);
            let mesh = SectionMesh {
                base_y: height.min_y,
                ..SectionMesh::default()
            };
            self.accept(pos, version, mesh, true, &mut finished);
        }

        for job in &sync {
            self.dirty.remove(&job.pos);
        }
        let mesher = self
            .mesher
            .get_or_insert_with(|| Mesher::new(Arc::clone(models), fluids));
        for done in mesher.mesh_now(sync) {
            self.accept(done.pos, done.version, done.mesh, true, &mut finished);
        }
        let mesher = self.mesher.as_mut().expect("started above");
        for done in mesher.finished() {
            if let Some(versions) = self.versions.get_mut(&done.pos)
                && versions.job == Some(done.version)
            {
                versions.job = None;
                self.accept(done.pos, done.version, done.mesh, false, &mut finished);
            }
        }

        initial.sort_unstable();
        rebuild.sort_unstable();
        let mut room = self.mesher.as_ref().map_or(0, Mesher::room);
        let mut initial = initial.into_iter().peekable();
        let mut rebuild = rebuild.into_iter().peekable();
        while room > 0 {
            let Some(pos) = poll(&mut initial, &mut rebuild, &mut self.recompiles) else {
                break;
            };
            let versions = self.versions.get_mut(&pos).expect("listed above");
            let version = versions.current;
            let mesher = self.mesher.as_mut().expect("started above");
            if !mesher.send(Job {
                pos,
                version,
                chunks: neighborhood_of(chunks, (pos.0, pos.2)),
                smooth_lighting: !self.flat_lighting,
                cutout_leaves: !self.opaque_leaves,
                cardinal,
                camera,
                tints: tints.clone(),
            }) {
                break;
            }
            versions.job = Some(version);
            self.dirty.remove(&pos);
            room -= 1;
        }
        self.ready_count = initial.len() + rebuild.len();
        finished
    }

    /// Puts the parked sections of columns now [`ready`] back in `dirty`,
    /// when a chunk arrived or the camera's chunk changed since last time.
    fn unpark(&mut self, chunks: &Chunks, center: (i32, i32)) {
        let now = (center, self.arrivals);
        if self.parked_at == Some(now) {
            return;
        }
        self.parked_at = Some(now);
        self.parked.retain(|&column, sections| {
            if !ready(chunks, column, center) {
                return true;
            }
            for &(s, parked) in sections.iter() {
                let dirty = self.dirty.entry((column.0, s, column.1)).or_insert(parked);
                *dirty = dirty.merged(parked);
            }
            false
        });
    }

    /// Takes a mesh built from `version`, unless a newer one is there.
    fn accept(
        &mut self,
        pos: SectionPos,
        version: u64,
        mesh: SectionMesh,
        sync: bool,
        finished: &mut Vec<Built>,
    ) {
        let Some(versions) = self.versions.get_mut(&pos) else {
            return;
        };
        if version <= versions.meshed {
            return;
        }
        versions.meshed = version;
        finished.push(Built { pos, mesh, sync });
    }

    pub fn stats(&self) -> MeshStats {
        MeshStats {
            waiting: self.ready_count,
            in_flight: self.mesher.as_ref().map_or(0, |m| m.in_flight),
        }
    }
}

/// `LevelRenderer.compileSections`.
fn builds_now(builder: ChunkBuilder, player: bool, distance_sq: i64) -> bool {
    match builder {
        ChunkBuilder::Nearby => player || distance_sq < NEARBY_SQ,
        ChunkBuilder::PlayerAffected => player,
        ChunkBuilder::None => false,
    }
}

/// `SectionTaskDynamicQueue.poll` over two lists sorted nearest first.
fn poll<T>(
    initial: &mut std::iter::Peekable<impl Iterator<Item = (i64, T)>>,
    rebuild: &mut std::iter::Peekable<impl Iterator<Item = (i64, T)>>,
    recompiles: &mut u8,
) -> Option<T> {
    let take_rebuild = match (initial.peek(), rebuild.peek()) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(first), Some(again)) => *recompiles < MAX_RECOMPILE_QUOTA && again.0 < first.0,
    };
    let next = if take_rebuild {
        *recompiles = recompiles.saturating_add(1);
        rebuild.next()
    } else {
        *recompiles = 0;
        initial.next()
    };
    next.map(|(_, item)| item)
}

/// Squared distance from the camera's block to a section's centre.
fn distance_sq((x, s, z): SectionPos, eye: IVec3, min_y: i32) -> i64 {
    let centre = IVec3::new(
        x * SECTION_SIZE + 8,
        min_y + s * SECTION_SIZE + 8,
        z * SECTION_SIZE + 8,
    );
    let d = (centre - eye).as_i64vec3();
    d.dot(d)
}

/// The chunk at `key` and the eight around it.
fn neighborhood_of(chunks: &Chunks, key: (i32, i32)) -> [Option<Arc<Chunk>>; 9] {
    std::array::from_fn(|i| {
        let (dx, dz) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
        chunks
            .get(&(key.0 + dx, key.1 + dz))
            .map(|c| Arc::clone(&c.chunk))
    })
}

/// Whether a chunk's sections may be meshed.
fn ready(chunks: &Chunks, key: (i32, i32), center: (i32, i32)) -> bool {
    let (dx, dz) = (key.0 - center.0, key.1 - center.1);
    dx * dx + dz * dz <= NEAR_CHUNKS_SQ
        || AROUND
            .iter()
            .all(|(ox, oz)| chunks.contains_key(&(key.0 + ox, key.1 + oz)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Section;

    fn chunks_around(blocks: u16) -> Chunks {
        let mut chunks = Chunks::default();
        for x in -1..=1 {
            for z in -1..=1 {
                let mut sections: Vec<Section> = (0..4).map(|_| Section::empty()).collect();
                sections[1].set(0, blocks);
                chunks.insert((x, z), LoadedChunk::new(Chunk::new(x, z, sections)));
            }
        }
        chunks
    }

    #[test]
    fn rebuilds_wait_their_turn_behind_first_meshes() {
        let mut initial = [(10, 'a'), (20, 'b'), (30, 'c')].into_iter().peekable();
        let mut rebuild = [(1, 'x'), (2, 'y'), (3, 'z')].into_iter().peekable();
        let mut recompiles = 0;
        let order: Vec<_> =
            std::iter::from_fn(|| poll(&mut initial, &mut rebuild, &mut recompiles)).collect();
        assert_eq!(order, ['x', 'y', 'a', 'z', 'b', 'c']);
    }

    #[test]
    fn the_chunk_builder_picks_what_is_built_now() {
        let (near, far) = (NEARBY_SQ - 1, NEARBY_SQ);
        assert!(!builds_now(ChunkBuilder::None, true, near));
        assert!(builds_now(ChunkBuilder::PlayerAffected, true, far));
        assert!(!builds_now(ChunkBuilder::PlayerAffected, false, near));
        assert!(builds_now(ChunkBuilder::Nearby, false, near));
        assert!(!builds_now(ChunkBuilder::Nearby, false, far));
        assert!(builds_now(ChunkBuilder::Nearby, true, far));
    }

    #[test]
    fn empty_sections_are_done_without_a_worker() {
        let chunks = chunks_around(0);
        let mut queue = MeshQueue::default();
        queue.added((0, 0), 4);
        let Some((models, fluids)) = crate::terrain::test_models() else {
            return;
        };
        let tints = crate::block_tint::TintSettings::plains();
        let built = queue.run(
            &chunks,
            (models, fluids),
            DVec3::new(8.0, -40.0, 8.0),
            Height::OVERWORLD,
            [1.0; 6],
            &tints,
        );
        assert_eq!(built.len(), 4);
        assert!(built.iter().all(|b| b.sync && b.mesh.is_empty()));
        assert!(queue.dirty.is_empty());
    }

    #[test]
    fn a_players_edit_is_built_for_the_next_frame() {
        let chunks = chunks_around(1);
        let Some((models, fluids)) = crate::terrain::test_models() else {
            return;
        };
        let mut queue = MeshQueue::default();
        queue.set_chunk_builder(ChunkBuilder::PlayerAffected);
        queue.added((0, 0), 4);
        queue.queue((0, 1, 0), true);
        let tints = crate::block_tint::TintSettings::plains();
        let built = queue.run(
            &chunks,
            (models, fluids),
            DVec3::new(8.0, 100.0, 8.0),
            Height::OVERWORLD,
            [1.0; 6],
            &tints,
        );
        assert!(
            built
                .iter()
                .any(|b| b.pos == (0, 1, 0) && b.sync && !b.mesh.is_empty())
        );
    }

    /// A column far out without all its neighbours waits aside, not in
    /// every frame's look at the queue, and is built once they came.
    #[test]
    fn a_column_waits_aside_for_its_neighbours() {
        let Some((models, fluids)) = crate::terrain::test_models() else {
            return;
        };
        let tints = crate::block_tint::TintSettings::plains();
        let column = |x, z| {
            let sections = (0..4).map(|_| Section::empty()).collect();
            LoadedChunk::new(Chunk::new(x, z, sections))
        };
        let mut chunks = Chunks::default();
        let mut queue = MeshQueue::default();
        chunks.insert((5, 0), column(5, 0));
        queue.added((5, 0), 4);
        let run = |queue: &mut MeshQueue, chunks: &Chunks| {
            queue.run(
                chunks,
                (models, fluids),
                DVec3::new(8.0, 0.0, 8.0),
                Height::OVERWORLD,
                [1.0; 6],
                &tints,
            )
        };
        assert!(run(&mut queue, &chunks).is_empty());
        assert!(queue.dirty.is_empty());
        assert_eq!(queue.parked[&(5, 0)].len(), 4);
        assert!(run(&mut queue, &chunks).is_empty());
        for (dx, dz) in AROUND {
            let key = (5 + dx, dz);
            chunks.insert(key, column(key.0, key.1));
            queue.added(key, 4);
        }
        let built = run(&mut queue, &chunks);
        assert_eq!(
            built
                .iter()
                .filter(|b| (b.pos.0, b.pos.2) == (5, 0))
                .count(),
            4
        );
        assert!(!queue.parked.contains_key(&(5, 0)));
    }

    #[test]
    fn an_older_mesh_never_replaces_a_newer_one() {
        let mut queue = MeshQueue::default();
        queue.added((0, 0), 1);
        let mut finished = Vec::new();
        queue.accept((0, 0, 0), 5, SectionMesh::default(), false, &mut finished);
        queue.accept((0, 0, 0), 3, SectionMesh::default(), false, &mut finished);
        assert_eq!(finished.len(), 1);
    }
}
