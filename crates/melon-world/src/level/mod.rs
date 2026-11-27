//! The chunks of a joined world (vanilla `ClientChunkCache`) and their meshes.

mod light;
mod mesh_queue;
mod mesher;

use std::sync::Arc;

use glam::DVec3;

use crate::block_entity;
use crate::block_models::BlockModels;
use crate::block_tint::TintSettings;
use crate::chunk::{Chunk, LightUpdate, Section};
use crate::culling::occlusion::{
    self, Column, OcclusionMask, SectionGrid, SectionPos, SectionState,
};
use crate::culling::visgraph::VisibilitySet;
use crate::dimension::{DimensionType, Height};
use crate::fluid::FluidSprites;
use crate::head_profile::{HeadProfile, is_player_head};
use crate::terrain::SectionMesh;
use melon_core::math::{ChunkMap, SectionSet, section_coord, section_relative};
use mesh_queue::MeshQueue;

struct LoadedChunk {
    /// Shared with mesh jobs in flight; a block change copies it if one holds it.
    chunk: Arc<Chunk>,
    /// What can be seen through each section, bottom to top.
    visibility: Vec<VisibilitySet>,
    /// World positions of the blocks a block entity renderer draws.
    block_entities: Vec<[i32; 3]>,
    /// The player heads with a profile, by world position.
    heads: Vec<([i32; 3], HeadProfile)>,
}

impl LoadedChunk {
    /// Takes the chunk's [`ChunkScan`](crate::chunk::ChunkScan).
    fn new(mut chunk: Chunk) -> Self {
        let scan = std::mem::take(&mut chunk.scan);
        Self {
            visibility: vec![VisibilitySet::ALL; chunk.sections.len()],
            chunk: Arc::new(chunk),
            block_entities: scan.block_entities,
            heads: scan.heads,
        }
    }

    /// Whether it has block entities or heads to draw.
    fn furnished(&self) -> bool {
        !self.block_entities.is_empty() || !self.heads.is_empty()
    }

    /// Keeps the block entity positions up to date for one block that changed.
    fn block_changed(&mut self, pos: [i32; 3], old: u16, new: u16) {
        if block_entity::kind_of(old).is_some() {
            self.block_entities.retain(|&p| p != pos);
        }
        if is_player_head(old) {
            self.heads.retain(|&(p, _)| p != pos);
        }
        if block_entity::kind_of(new).is_some() {
            self.block_entities.push(pos);
        }
    }
}

/// A section's new terrain, for the game to upload in place of the old.
pub struct FinishedMesh {
    pub pos: SectionPos,
    pub mesh: SectionMesh,
    /// Built for the coming frame (the Chunk Builder's).
    pub sync: bool,
}

pub struct Level {
    chunks: ChunkMap<LoadedChunk>,
    /// The chunks that have had block entities or heads since they were
    /// loaded: what [`Self::block_entities`] and [`Self::heads`] walk,
    /// every frame, instead of every loaded chunk.
    furnished: Vec<(i32, i32)>,
    /// What needs meshing, and the workers meshing it.
    meshing: MeshQueue,
    /// Bumped whenever a mesh or a chunk changes, so the occlusion walk knows to run again.
    revision: u64,
    /// The dimension the chunks are of.
    height: Height,
    has_skylight: bool,
    /// How the dimension shades faces by direction, relative to the default.
    cardinal: [f32; 6],
    /// Biome colours and the blend radius, for the mesh workers.
    tints: TintSettings,
}

impl Default for Level {
    /// An overworld level with nothing in it.
    fn default() -> Self {
        Self {
            chunks: ChunkMap::default(),
            furnished: Vec::new(),
            meshing: MeshQueue::default(),
            revision: 0,
            height: Height::OVERWORLD,
            has_skylight: true,
            cardinal: [1.0; 6],
            tints: TintSettings::plains(),
        }
    }
}

/// What F3 shows of the meshing (vanilla's `pC`/`pU`).
#[derive(Clone, Copy, Default)]
pub struct MeshStats {
    /// Sections waiting for a mesh whose chunk has its neighbours.
    pub waiting: usize,
    /// Section mesh jobs on the worker threads.
    pub in_flight: usize,
}

/// The chunks around `(x, z)`: the four sides, then the corners.
const AROUND: [(i32, i32); 8] = [
    (0, -1),
    (0, 1),
    (-1, 0),
    (1, 0),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

impl Level {
    /// A chunk arrived.
    pub fn insert(&mut self, mut chunk: Chunk) {
        if chunk.height != self.height {
            return;
        }
        if !self.has_skylight {
            Arc::make_mut(&mut chunk.light).disable_skylight();
        }
        let key = (chunk.x, chunk.z);
        self.remove(key.0, key.1);
        let sections = chunk.sections.len();
        let loaded = LoadedChunk::new(chunk);
        if loaded.furnished() {
            self.furnished.push(key);
        }
        self.chunks.insert(key, loaded);
        self.meshing.added(key, sections);
        self.meshing.queue_around(key, &AROUND);
        self.revision += 1;
    }

    /// A chunk was unloaded. Whether it was there.
    pub fn remove(&mut self, x: i32, z: i32) -> bool {
        let Some(loaded) = self.chunks.remove(&(x, z)) else {
            return false;
        };
        if loaded.furnished() || self.furnished.contains(&(x, z)) {
            self.furnished.retain(|&key| key != (x, z));
        }
        self.meshing.removed((x, z));
        self.meshing.queue_around((x, z), &AROUND[..4]);
        self.revision += 1;
        true
    }

    /// Sets one block, as `ClientboundBlockUpdatePacket` says or the player did.
    pub fn set_block(&mut self, [x, y, z]: [i32; 3], state: u16, from_player: bool) -> Option<u16> {
        let key = (section_coord(x), section_coord(z));
        let old = {
            let loaded = self.chunks.get_mut(&key)?;
            let s = usize::try_from(section_coord(y - self.height.min_y)).ok()?;
            let index = Section::index_of_block(x, y, z);
            let old = loaded.chunk.sections.get(s)?.get(index);
            if old == state {
                return Some(old);
            }
            Arc::make_mut(&mut Arc::make_mut(&mut loaded.chunk).sections[s]).set(index, state);
            loaded.block_changed([x, y, z], old, state);
            if loaded.furnished() && !self.furnished.contains(&key) {
                self.furnished.push(key);
            }
            old
        };
        for pos in self.update_light([x, y, z], old, state) {
            self.meshing.queue(pos, false);
        }
        let mut around = SectionSet::default();
        sections_around([x, y, z], self.height.min_y, &mut around);
        for pos in around {
            self.meshing.queue(pos, from_player);
        }
        self.revision += 1;
        Some(old)
    }

    /// `ClientboundLightUpdatePacket`.
    pub fn apply_light(&mut self, x: i32, z: i32, update: &LightUpdate) -> bool {
        let Some(loaded) = self.chunks.get_mut(&(x, z)) else {
            return false;
        };
        Arc::make_mut(&mut Arc::make_mut(&mut loaded.chunk).light).apply(update);
        self.meshing.queue_column((x, z));
        self.meshing.queue_around((x, z), &AROUND);
        self.revision += 1;
        true
    }

    /// `ClientChunkCache.replaceBiomes`.
    pub fn replace_biomes(&mut self, x: i32, z: i32, biomes: crate::biome::ChunkBiomes) -> bool {
        let Some(loaded) = self.chunks.get_mut(&(x, z)) else {
            return false;
        };
        Arc::make_mut(&mut loaded.chunk).biomes = Arc::new(biomes);
        self.meshing.queue_column((x, z));
        self.meshing.queue_around((x, z), &AROUND);
        self.revision += 1;
        true
    }

    /// Meshes every section again: the resource packs changed.
    pub fn remesh_all(&mut self) {
        self.meshing.remesh_all();
    }

    /// The "Smooth Lighting" option; changing it remeshes everything.
    pub fn set_smooth_lighting(&mut self, on: bool) {
        self.meshing.set_smooth_lighting(on);
    }

    /// The "See-Through Leaves" option; changing it remeshes everything.
    pub fn set_cutout_leaves(&mut self, on: bool) {
        self.meshing.set_cutout_leaves(on);
    }

    /// The "Chunk Builder" option.
    pub fn set_chunk_builder(&mut self, builder: ChunkBuilder) {
        self.meshing.set_chunk_builder(builder);
    }

    /// Colormaps, the biome registry and `Options.biomeBlendRadius`.
    pub fn set_tints(&mut self, tints: TintSettings) {
        self.meshing.set_biome_blend(tints.blend);
        self.tints = tints;
    }

    /// The biome colour `kind` faces get at a block, as the terrain's mesher picks it.
    pub fn tint_at(&self, kind: crate::block_tint::TintKind, pos: [i32; 3]) -> [u8; 3] {
        self.tints
            .sampler(|qx, qy, qz| self.biome_at(qx * 4, qy * 4, qz * 4).unwrap_or(0))
            .color(kind, pos)
    }

    /// The `BiomeManager` seed of a new level, before its chunks arrive.
    pub fn set_zoom_seed(&mut self, seed: i64) {
        self.tints.set_zoom_seed(seed);
    }

    /// `Options.biomeBlendRadius` only: remeshes when it changes.
    pub fn set_biome_blend(&mut self, radius: u8) {
        self.tints.blend = radius;
        self.meshing.set_biome_blend(radius);
    }

    /// Drops every chunk. Call when leaving the world.
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.furnished.clear();
        self.meshing.clear();
        self.revision += 1;
    }

    /// A level of another dimension, as vanilla's respawn makes a new one.
    pub fn set_dimension(&mut self, dimension: &DimensionType) {
        self.clear();
        self.height = dimension.height;
        self.has_skylight = dimension.has_skylight;
        self.cardinal = dimension.cardinal_light.relative();
    }

    /// Builds the sections the Chunk Builder wants now and takes what the workers finished.
    pub fn mesh_pending(
        &mut self,
        models: &Arc<BlockModels>,
        fluids: FluidSprites,
        camera: DVec3,
    ) -> Vec<FinishedMesh> {
        let built = self.meshing.run(
            &self.chunks,
            (models, fluids),
            camera,
            self.height,
            self.cardinal,
            &self.tints,
        );
        let mut finished = Vec::with_capacity(built.len());
        for built in built {
            let (x, s, z) = built.pos;
            let Some(slot) = self
                .chunks
                .get_mut(&(x, z))
                .and_then(|loaded| loaded.visibility.get_mut(s as usize))
            else {
                continue;
            };
            *slot = built.mesh.visibility;
            self.revision += 1;
            finished.push(FinishedMesh {
                pos: built.pos,
                mesh: built.mesh,
                sync: built.sync,
            });
        }
        finished
    }

    /// For F3.
    pub fn mesh_stats(&self) -> MeshStats {
        self.meshing.stats()
    }

    /// Changes whenever what the occlusion walk sees could have changed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Where the world starts and how many sections it has from there.
    pub fn height(&self) -> (i32, i32) {
        (self.height.min_y, self.height.sections() as i32)
    }

    /// The dimension's height.
    pub fn world_height(&self) -> Height {
        self.height
    }

    /// Whether the dimension has sky light (`DimensionType.hasSkyLight`).
    pub fn has_skylight(&self) -> bool {
        self.has_skylight
    }

    /// The occlusion walk ([`occlusion::walk`]) from section `camera` out to the view distance.
    pub fn occlusion_job(
        &self,
        camera: SectionPos,
        view_distance: i32,
        eye: Option<occlusion::Eye>,
    ) -> occlusion::WalkJob {
        let (_, height) = self.height();
        let grid = SectionGrid::new(
            (camera.0, camera.2),
            view_distance,
            height,
            |x, z| match self.chunks.get(&(x, z)) {
                None => Column::Missing,
                Some(loaded) => Column::Sets(&loaded.visibility),
            },
        );
        occlusion::WalkJob::new(camera, height, view_distance, eye, grid)
    }

    /// [`Self::occlusion_job`], run here and now.
    pub fn occlusion_walk(
        &self,
        camera: SectionPos,
        view_distance: i32,
        eye: Option<occlusion::Eye>,
    ) -> OcclusionMask {
        self.occlusion_job(camera, view_distance, eye).run()
    }

    pub fn section_state(&self, (x, y, z): SectionPos) -> SectionState {
        let Some(loaded) = self.chunks.get(&(x, z)) else {
            return SectionState::Missing;
        };
        let visibility = loaded
            .visibility
            .get(y as usize)
            .copied()
            .unwrap_or(VisibilitySet::ALL);
        SectionState::Loaded(visibility)
    }

    /// `ClientboundBlockEntityDataPacket` for a player head.
    pub fn set_head(&mut self, [x, y, z]: [i32; 3], profile: Option<HeadProfile>) {
        if !self.block_state(x, y, z).is_some_and(is_player_head) {
            return;
        }
        let Some(loaded) = self.chunks.get_mut(&(section_coord(x), section_coord(z))) else {
            return;
        };
        let pos = [x, y, z];
        let old = loaded.heads.iter().position(|&(p, _)| p == pos);
        if old.map(|i| &loaded.heads[i].1) == profile.as_ref() {
            return;
        }
        if let Some(i) = old {
            loaded.heads.swap_remove(i);
        }
        if let Some(profile) = profile {
            loaded.heads.push((pos, profile));
            let key = (section_coord(x), section_coord(z));
            if !self.furnished.contains(&key) {
                self.furnished.push(key);
            }
        }
        self.revision += 1;
    }

    /// The profile of the player head at `pos`, if it names one.
    pub fn head_profile(&self, [x, y, z]: [i32; 3]) -> Option<&HeadProfile> {
        let loaded = self.chunks.get(&(section_coord(x), section_coord(z)))?;
        loaded
            .heads
            .iter()
            .find(|(p, _)| *p == [x, y, z])
            .map(|(_, profile)| profile)
    }

    fn furnished_chunks(&self) -> impl Iterator<Item = &LoadedChunk> + '_ {
        self.furnished.iter().filter_map(|key| self.chunks.get(key))
    }

    /// Every loaded player head with a profile.
    pub fn heads(&self) -> impl Iterator<Item = ([i32; 3], &HeadProfile)> + '_ {
        self.furnished_chunks()
            .flat_map(|loaded| loaded.heads.iter().map(|(pos, profile)| (*pos, profile)))
    }

    /// The block entity positions of every loaded chunk.
    pub fn block_entities(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.furnished_chunks()
            .flat_map(|loaded| loaded.block_entities.iter().copied())
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Whether the chunk holding world column `(x, z)` has arrived.
    pub fn is_loaded_at(&self, x: i32, z: i32) -> bool {
        self.chunks
            .contains_key(&(section_coord(x), section_coord(z)))
    }

    /// The block-state ID at a world position.
    pub fn block_state(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let loaded = self.chunks.get(&(section_coord(x), section_coord(z)))?;
        let Some(section) = usize::try_from(section_coord(y - self.height.min_y))
            .ok()
            .and_then(|s| loaded.chunk.sections.get(s))
        else {
            return Some(0);
        };
        Some(section.get(Section::index_of_block(x, y, z)))
    }

    /// `ClientLevel.getBiome`.
    pub fn biome(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let own = self.biome_at(x, y, z)?;
        let Some(seed) = self.tints.zoom_seed() else {
            return Some(own);
        };
        let noise =
            |qx: i32, qy: i32, qz: i32| self.biome_at(qx * 4, qy * 4, qz * 4).unwrap_or(own);
        Some(crate::biome_zoom::biome(seed, x, y, z, &noise))
    }

    /// The registry index of the biome stored for a world position's 4×4×4 cell.
    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let loaded = self.chunks.get(&(section_coord(x), section_coord(z)))?;
        Some(loaded.chunk.biomes.at(x, y - self.height.min_y, z))
    }

    /// Approximate `Heightmap.Types.MOTION_BLOCKING`.
    pub fn motion_blocking_height(&self, x: i32, z: i32) -> i32 {
        let (min_y, sections) = self.height();
        let Some(loaded) = self.chunks.get(&(section_coord(x), section_coord(z))) else {
            return min_y;
        };
        for (s, section) in loaded
            .chunk
            .sections
            .iter()
            .enumerate()
            .take(sections.max(0) as usize)
            .rev()
        {
            let base = min_y + s as i32 * 16;
            for y in (base..base + 16).rev() {
                let id = section.get(Section::index_of_block(x, y, z));
                if crate::block::state_flags::is_drawn(id) {
                    return y + 1;
                }
            }
        }
        min_y
    }

    /// `LayerLightSectionStorage.SectionType.display` for the light section
    /// at section coordinates `(x, y, z)` (`y` in world sections): `"0"`
    /// light kept and blocks in it, `"1"` light kept only, `"2"` neither.
    /// Melon keeps no light for a section that is dark all through, so a
    /// dark section reads as empty where vanilla keeps its zeros.
    pub fn light_section_type(&self, sky: bool, (x, y, z): (i32, i32, i32)) -> &'static str {
        let Some(loaded) = self.chunks.get(&(x, z)) else {
            return "2";
        };
        let chunk = &loaded.chunk;
        let light = &chunk.light;
        let slot = y - (light.min_y() >> 4);
        let layer = if sky { &light.sky } else { &light.block };
        let has_light = usize::try_from(slot)
            .ok()
            .and_then(|i| layer.get(i))
            .is_some_and(|l| matches!(l, crate::chunk::LightSection::Data(_)));
        if !has_light {
            return "2";
        }
        let blocks = usize::try_from(y - (self.height.min_y >> 4))
            .ok()
            .and_then(|i| chunk.sections.get(i))
            .is_some_and(|section| !section.is_empty());
        if blocks { "0" } else { "1" }
    }

    /// Block and sky light at a world position, `(0, 0)` if the column isn't loaded.
    pub fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        self.chunks
            .get(&(section_coord(x), section_coord(z)))
            .map_or((0, 0), |loaded| loaded.chunk.light.at(x, y, z))
    }
}

/// Adds the sections of the 3×3×3 blocks around `pos` to `out`.
pub(super) fn sections_around([x, y, z]: [i32; 3], min_y: i32, out: &mut SectionSet) {
    let span = |coord: i32| {
        let section = section_coord(coord);
        let local = section_relative(coord);
        let low = if local == 0 { section - 1 } else { section };
        let high = if local == 15 { section + 1 } else { section };
        low..=high
    };
    for sx in span(x) {
        for sy in span(y - min_y) {
            for sz in span(z) {
                out.insert((sx, sy, sz));
            }
        }
    }
}

/// `PrioritizeChunkUpdates`, the "Chunk Builder".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChunkBuilder {
    /// Threaded: never waited for.
    #[default]
    None = 0,
    /// Semi Blocking: the player's own edits.
    PlayerAffected = 1,
    /// Fully Blocking: also every change near the camera.
    Nearby = 2,
}

impl ChunkBuilder {
    /// `LEGACY_CODEC`'s `ByIdMap.continuous(..., WRAP)`.
    pub fn from_id(id: i64) -> Self {
        match id.rem_euclid(3) {
            1 => Self::PlayerAffected,
            2 => Self::Nearby,
            _ => Self::None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::None => "options.prioritizeChunkUpdates.none",
            Self::PlayerAffected => "options.prioritizeChunkUpdates.byPlayer",
            Self::Nearby => "options.prioritizeChunkUpdates.nearby",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::None => "options.prioritizeChunkUpdates.none.tooltip",
            Self::PlayerAffected => "options.prioritizeChunkUpdates.byPlayer.tooltip",
            Self::Nearby => "options.prioritizeChunkUpdates.nearby.tooltip",
        }
    }
}

impl melon_core::cycle::Cycle for ChunkBuilder {
    const ALL: &'static [Self] = &[Self::None, Self::PlayerAffected, Self::Nearby];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Section;

    #[test]
    fn set_block_changes_the_block_and_reports_the_old_one() {
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let revision = level.revision();
        assert_eq!(level.set_block([3, 70, 5], 1, false), Some(0));
        assert_eq!(level.block_state(3, 70, 5), Some(1));
        assert!(level.revision() > revision);
        assert_eq!(level.set_block([100, 70, 5], 1, false), None);
        let chest = pumpkin_data::Block::CHEST.default_state.id.as_u16();
        level.set_block([1, 64, 2], chest, false);
        assert_eq!(level.block_entities().collect::<Vec<_>>(), vec![[1, 64, 2]]);
    }

    /// The chunks walked for block entities follow loads, unloads and
    /// edits: each listed once, none of an unloaded chunk.
    #[test]
    fn block_entities_follow_their_chunks() {
        let column = |x, z| Chunk::new(x, z, (0..24).map(|_| Section::empty()).collect());
        let chest = pumpkin_data::Block::CHEST.default_state.id.as_u16();
        let mut level = Level::default();
        level.insert(column(0, 0));
        level.insert(column(1, 0));
        level.set_block([1, 64, 2], chest, false);
        level.set_block([1, 64, 2], 0, false);
        level.set_block([1, 64, 2], chest, false);
        level.set_block([20, 64, 2], chest, false);
        let mut all: Vec<_> = level.block_entities().collect();
        all.sort_unstable();
        assert_eq!(all, vec![[1, 64, 2], [20, 64, 2]]);
        level.remove(1, 0);
        assert_eq!(level.block_entities().collect::<Vec<_>>(), vec![[1, 64, 2]]);
        level.insert(column(0, 0));
        assert_eq!(level.block_entities().count(), 0);
        assert!(level.furnished.is_empty());
    }

    #[test]
    fn breaking_an_opaque_block_lights_the_hole() {
        let mut level = Level::default();
        level.insert(
            Chunk::new(0, 0, (0..24).map(|_| Section::empty()).collect())
                .with_light(crate::chunk::ChunkLight::filled_sky(15)),
        );
        assert_eq!(level.set_block([3, 70, 5], 1, false), Some(0));
        assert_eq!(level.light_at(3, 70, 5), (0, 0));
        assert_eq!(level.set_block([3, 70, 5], 0, false), Some(1));
        assert_eq!(level.light_at(3, 70, 5), (0, 15));
    }

    #[test]
    fn apply_light_patches_a_loaded_column() {
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        assert_eq!(level.light_at(3, 70, 5), (0, 0));
        let mut sky = vec![0u8; crate::chunk::LIGHT_NIBBLES];
        sky.fill(0xFF);
        assert!(level.apply_light(
            0,
            0,
            &crate::chunk::LightUpdate {
                sky_mask: vec![1 << 9],
                block_mask: vec![],
                empty_sky_mask: vec![],
                empty_block_mask: vec![],
                sky: vec![crate::chunk::LightSection::from_bytes(&sky)],
                block: vec![],
            },
        ));
        assert_eq!(level.light_at(3, 70, 5), (0, 15));
        assert!(!level.apply_light(1, 0, &crate::chunk::LightUpdate::default()));
    }

    /// A nether column: 16 sections from Y 0, lit by block light only.
    fn nether_chunk(x: i32, z: i32) -> Chunk {
        let height = Height::NETHER;
        let sections: Vec<Arc<Section>> = (0..height.sections())
            .map(|_| Arc::new(Section::empty()))
            .collect();
        Chunk::from_shared(
            x,
            z,
            height,
            sections,
            crate::biome::ChunkBiomes::uniform(0, height.sections()),
            crate::chunk::ChunkScan::default(),
        )
    }

    #[test]
    fn a_nether_level_counts_blocks_from_zero() {
        let mut level = Level::default();
        level.set_dimension(&DimensionType::nether());
        assert_eq!(level.height(), (0, 16));
        level.insert(nether_chunk(0, 0));
        assert_eq!(level.chunk_count(), 1);
        assert_eq!(level.set_block([3, 0, 5], 1, false), Some(0));
        assert_eq!(level.set_block([3, 255, 5], 1, false), Some(0));
        assert_eq!(level.block_state(3, 255, 5), Some(1));
        assert_eq!(level.block_state(3, 256, 5), Some(0));
        assert_eq!(level.set_block([3, 256, 5], 1, false), None);
        assert_eq!(level.set_block([3, -1, 5], 1, false), None);
    }

    #[test]
    fn chunks_of_another_dimension_are_not_taken() {
        let mut level = Level::default();
        level.insert(nether_chunk(0, 0));
        assert_eq!(level.chunk_count(), 0);
        level.set_dimension(&DimensionType::nether());
        level.insert(nether_chunk(0, 0));
        assert_eq!(level.chunk_count(), 1);
        level.set_dimension(&DimensionType::overworld());
        assert_eq!(level.chunk_count(), 0);
        assert_eq!(level.height(), (-64, 24));
    }

    #[test]
    fn a_nether_chunk_keeps_no_sky_light_even_if_the_server_sends_it() {
        let mut level = Level::default();
        level.set_dimension(&DimensionType::nether());
        let height = Height::NETHER;
        let chunk =
            nether_chunk(0, 0).with_light(crate::chunk::ChunkLight::filled_sky_in(height, 15));
        assert_eq!(chunk.light.at(3, 70, 5), (0, 15));
        level.insert(chunk);
        assert_eq!(level.light_at(3, 70, 5), (0, 0));
        let mut level = Level::default();
        level.insert(
            Chunk::new(0, 0, (0..24).map(|_| Section::empty()).collect())
                .with_light(crate::chunk::ChunkLight::filled_sky(15)),
        );
        assert_eq!(level.light_at(3, 70, 5), (0, 15));
    }

    #[test]
    fn a_torch_in_the_nether_leaves_the_sky_dark() {
        let mut level = Level::default();
        level.set_dimension(&DimensionType::nether());
        level.insert(nether_chunk(0, 0));
        let torch = pumpkin_data::Block::TORCH.default_state.id.as_u16();
        level.set_block([8, 64, 8], torch, false);
        assert_eq!(level.light_at(8, 64, 8), (14, 0));
        level.set_block([8, 64, 8], 0, false);
        assert_eq!(level.light_at(8, 64, 8), (0, 0));
        assert!(!level.has_skylight());
    }

    #[test]
    fn an_edit_marks_the_sections_it_borders() {
        let count = |pos: [i32; 3]| {
            let mut out = SectionSet::default();
            sections_around(pos, Height::OVERWORLD.min_y, &mut out);
            out.len()
        };
        assert_eq!(count([5, 70, 5]), 1);
        assert_eq!(count([0, 70, 5]), 2);
        assert_eq!(count([15, 70, 0]), 4);
        assert_eq!(count([0, 64, 15]), 8);
        let mut out = SectionSet::default();
        sections_around(
            [5, Height::OVERWORLD.min_y + 16, 5],
            Height::OVERWORLD.min_y,
            &mut out,
        );
        assert!(out.contains(&(0, 0, 0)) && out.contains(&(0, 1, 0)));
    }
}
