//! A chunk column's blocks.

use std::sync::Arc;

use crate::world::biome::ChunkBiomes;
use crate::world::dimension::Height;

pub use crate::world::section::{BLOCKS_PER_SECTION, Section};
/// `DataLayer.SIZE`: 4096 nibbles packed into 2048 bytes.
pub const LIGHT_NIBBLES: usize = 2048;

/// One 16³ light layer: empty (all 0) or packed nibbles (`DataLayer`).
#[derive(Clone, Debug)]
pub enum LightSection {
    Empty,
    Data(Arc<[u8; LIGHT_NIBBLES]>),
}

impl LightSection {
    /// `DataLayer.get` at section-local `0..16`.
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Data(data) => {
                let index = y * 256 + z * 16 + x;
                let byte = data[index >> 1];
                if index & 1 == 0 { byte & 15 } else { byte >> 4 }
            }
        }
    }

    /// A `DataLayer` from the wire.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() != LIGHT_NIBBLES || bytes.iter().all(|&b| b == 0) {
            return Self::Empty;
        }
        match Arc::<[u8]>::from(bytes).try_into() {
            Ok(data) => Self::Data(data),
            Err(_) => Self::Empty,
        }
    }

    /// `DataLayer.set` at section-local `0..16`.
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: u8) {
        let value = value & 15;
        let index = y * 256 + z * 16 + x;
        let i = index >> 1;
        let even = index & 1 == 0;
        match self {
            Self::Empty if value == 0 => {}
            Self::Empty => {
                let mut data = [0u8; LIGHT_NIBBLES];
                data[i] = if even { value } else { value << 4 };
                *self = Self::Data(Arc::new(data));
            }
            Self::Data(data) => {
                let bytes = Arc::make_mut(data);
                if even {
                    bytes[i] = (bytes[i] & 0xF0) | value;
                } else {
                    bytes[i] = (bytes[i] & 0x0F) | (value << 4);
                }
            }
        }
    }
}

/// Sky and block light for one chunk column, including the extra sections below and above.
#[derive(Clone, Debug)]
pub struct ChunkLight {
    height: Height,
    has_skylight: bool,
    pub sky: Vec<LightSection>,
    pub block: Vec<LightSection>,
}

/// An overworld column without light, for tests.
#[cfg(test)]
impl Default for ChunkLight {
    fn default() -> Self {
        Self::new(Height::OVERWORLD)
    }
}

impl ChunkLight {
    /// A column of `height` without any light.
    pub fn new(height: Height) -> Self {
        let empty = vec![LightSection::Empty; height.light_sections()];
        Self {
            height,
            has_skylight: true,
            sky: empty.clone(),
            block: empty,
        }
    }

    /// Whether this column has a sky light engine (overworld / end).
    pub fn has_skylight(&self) -> bool {
        self.has_skylight
    }

    /// Nether: stored sky is empty and reads as 0, including above the world.
    pub fn disable_skylight(&mut self) {
        self.has_skylight = false;
        self.sky.fill(LightSection::Empty);
    }

    /// Sky light `level` in every world section of an overworld column.
    #[cfg(test)]
    pub fn filled_sky(level: u8) -> Self {
        Self::filled_sky_in(Height::OVERWORLD, level)
    }

    /// Sky light `level` in every world section of a `height` column.
    #[cfg(test)]
    pub fn filled_sky_in(height: Height, level: u8) -> Self {
        let mut light = Self::new(height);
        let nibble = level & 15;
        let packed = nibble | (nibble << 4);
        let mut data = [0u8; LIGHT_NIBBLES];
        data.fill(packed);
        let section = LightSection::Data(Arc::new(data));
        for slot in light.sky.iter_mut().skip(1).take(height.sections()) {
            *slot = section.clone();
        }
        light
    }

    /// World Y above the column's top light section: open sky.
    pub fn max_y(&self) -> i32 {
        self.height.light_max_y()
    }

    /// World Y of the column's lowest light section.
    pub fn min_y(&self) -> i32 {
        self.height.light_min_y()
    }

    /// The light section holding world `y`, if the column has one.
    fn slot(&self, y: i32) -> Option<usize> {
        let slot = (y - self.height.light_min_y()) >> 4;
        usize::try_from(slot)
            .ok()
            .filter(|&i| i < self.height.light_sections())
    }

    /// Block and sky at world `(x, y, z)`.
    ///
    /// Vanilla `SkyLightSectionStorage`: above the stored column, and the
    /// extra empty padding section the engine never wrote, sky is 15 when
    /// this dimension has a sky. Empty sections inside the world stay 0
    /// (`empty_sky_mask` is zeros, not a null walk-up). Below the column
    /// both channels are 0. No sky engine (nether) never yields 15.
    pub fn at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        match self.slot(y) {
            Some(i) => {
                let (lx, ly, lz) = ((x & 15) as usize, (y & 15) as usize, (z & 15) as usize);
                (
                    self.block[i].get(lx, ly, lz),
                    self.sky_at_slot(i, lx, ly, lz),
                )
            }
            None => (
                0,
                if y >= self.height.light_max_y() && self.has_skylight {
                    15
                } else {
                    0
                },
            ),
        }
    }

    /// One layer at world `(x, y, z)`.
    pub fn layer_at(&self, sky: bool, x: i32, y: i32, z: i32) -> Option<u8> {
        let i = self.slot(y)?;
        let (lx, ly, lz) = ((x & 15) as usize, (y & 15) as usize, (z & 15) as usize);
        Some(if sky {
            self.sky_at_slot(i, lx, ly, lz)
        } else {
            self.block[i].get(lx, ly, lz)
        })
    }

    fn sky_at_slot(&self, slot: usize, x: usize, y: usize, z: usize) -> u8 {
        match &self.sky[slot] {
            LightSection::Data(_) => self.sky[slot].get(x, y, z),
            LightSection::Empty if self.has_skylight && slot + 1 == self.sky.len() => 15,
            LightSection::Empty => 0,
        }
    }

    /// Writes one layer at world `(x, y, z)`. No-op outside the column.
    pub fn set_layer(&mut self, sky: bool, x: i32, y: i32, z: i32, value: u8) {
        let Some(i) = self.slot(y) else {
            return;
        };
        let layer = if sky { &mut self.sky } else { &mut self.block };
        layer[i].set(
            (x & 15) as usize,
            (y & 15) as usize,
            (z & 15) as usize,
            value,
        );
    }

    /// Writes block and sky at world `(x, y, z)`. No-op outside the column.
    #[cfg(test)]
    pub fn set(&mut self, x: i32, y: i32, z: i32, block: u8, sky: u8) {
        self.set_layer(false, x, y, z, block);
        self.set_layer(true, x, y, z, sky);
    }

    /// `ClientboundLightUpdatePacketData`: replace the sections the masks name.
    pub fn apply(&mut self, update: &LightUpdate) {
        apply_layer(
            &mut self.sky,
            &update.sky_mask,
            &update.empty_sky_mask,
            &update.sky,
        );
        apply_layer(
            &mut self.block,
            &update.block_mask,
            &update.empty_block_mask,
            &update.block,
        );
    }
}

fn apply_layer(dest: &mut [LightSection], mask: &[u64], empty: &[u64], arrays: &[LightSection]) {
    let mut next = 0;
    for (i, section) in dest.iter_mut().enumerate() {
        if bit(empty, i) {
            *section = LightSection::Empty;
        } else if bit(mask, i) {
            if let Some(array) = arrays.get(next) {
                *section = array.clone();
            }
            next += 1;
        }
    }
}

fn bit(words: &[u64], i: usize) -> bool {
    words
        .get(i / 64)
        .is_some_and(|w| w & (1u64 << (i % 64)) != 0)
}

/// One `ClientboundLightUpdatePacketData`.
#[derive(Clone, Debug, Default)]
pub struct LightUpdate {
    pub sky_mask: Vec<u64>,
    pub block_mask: Vec<u64>,
    pub empty_sky_mask: Vec<u64>,
    pub empty_block_mask: Vec<u64>,
    pub sky: Vec<LightSection>,
    pub block: Vec<LightSection>,
}

/// Cheap to clone.
#[derive(Clone)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    /// The dimension's height this column was built for.
    pub height: Height,
    pub sections: Vec<Arc<Section>>,
    pub light: Arc<ChunkLight>,
    /// The biome of each 4x4x4 cell.
    pub biomes: Arc<ChunkBiomes>,
    /// What the blocks held when the chunk was built.
    pub scan: ChunkScan,
}

/// What the level indexes when a chunk arrives, worked out where the chunk is decoded.
#[derive(Clone, Default, Debug)]
pub struct ChunkScan {
    /// World positions of the blocks a block entity renderer draws.
    pub block_entities: Vec<[i32; 3]>,
    /// The player heads among them that name whose head they are.
    pub heads: Vec<([i32; 3], crate::world::head_profile::HeadProfile)>,
}

impl ChunkScan {
    pub fn of(x: i32, z: i32, min_y: i32, sections: &[Section]) -> Self {
        let mut scan = Self::default();
        for (s, section) in sections.iter().enumerate() {
            scan.add_section(x, z, min_y, s, section);
        }
        scan
    }

    /// TODO(perf): small. A section whose palette may hold a block entity
    /// is then read block by block (`Section::iter`, one `get` each, on the
    /// connection thread). Mark the matching palette indices once and scan
    /// the decoded section (`Section::copy_to`) or its packed indices.
    pub fn add_section(&mut self, x: i32, z: i32, min_y: i32, s: usize, section: &Section) {
        let kinds = &*crate::world::block_entity::KIND_STATES;
        if !section.may_hold(|id| kinds.contains(id)) {
            return;
        }
        for (i, id) in section.iter().enumerate() {
            if kinds.contains(id) {
                let (lx, ly, lz) = (i as i32 & 15, i as i32 >> 8, i as i32 >> 4 & 15);
                self.block_entities
                    .push([x * 16 + lx, min_y + s as i32 * 16 + ly, z * 16 + lz]);
            }
        }
    }
}

impl Chunk {
    /// An overworld-based column of `sections`, for tests.
    #[cfg(test)]
    pub fn new(x: i32, z: i32, sections: Vec<Section>) -> Self {
        let height = Height {
            min_y: Height::OVERWORLD.min_y,
            height: sections.len() as i32 * 16,
        };
        Self {
            x,
            z,
            height,
            scan: ChunkScan::of(x, z, height.min_y, &sections),
            biomes: Arc::new(ChunkBiomes::uniform(0, sections.len())),
            sections: sections.into_iter().map(Arc::new).collect(),
            light: Arc::new(ChunkLight::new(height)),
        }
    }

    /// A chunk from sections already shared.
    pub fn from_shared(
        x: i32,
        z: i32,
        height: Height,
        sections: Vec<Arc<Section>>,
        biomes: ChunkBiomes,
        scan: ChunkScan,
    ) -> Self {
        Self {
            x,
            z,
            height,
            sections,
            light: Arc::new(ChunkLight::new(height)),
            biomes: Arc::new(biomes),
            scan,
        }
    }

    pub fn with_light(mut self, light: ChunkLight) -> Self {
        self.light = Arc::new(light);
        self
    }

    #[cfg(test)]
    pub fn with_biomes(mut self, biomes: crate::world::biome::ChunkBiomes) -> Self {
        self.biomes = Arc::new(biomes);
        self
    }

    /// World Y of the column's lowest block.
    pub fn block_state(&self, [x, y, z]: [i32; 3]) -> Option<u16> {
        let s = usize::try_from((y - self.height.min_y) >> 4).ok()?;
        Some(self.sections.get(s)?.get(Section::index_of_block(x, y, z)))
    }

    pub fn min_y(&self) -> i32 {
        self.height.min_y
    }
}

/// `id` is a registry block-state ID, not a block ID.
fn block_name(id: u16) -> String {
    match id {
        0 => "air".to_string(),
        1 => "stone".to_string(),
        9 => "grass_block".to_string(),
        10 => "dirt".to_string(),
        other => format!("?{other}"),
    }
}

impl Chunk {
    /// One line per non-empty section.
    pub fn describe(&self) -> String {
        use std::collections::BTreeMap;
        use std::fmt::Write;

        let mut out = String::new();
        for (i, section) in self.sections.iter().enumerate() {
            let y0 = self.height.min_y + i as i32 * 16;
            let mut counts: BTreeMap<u16, u32> = BTreeMap::new();
            for id in section.iter() {
                if id != 0 {
                    *counts.entry(id).or_insert(0) += 1;
                }
            }
            if counts.is_empty() {
                continue;
            }
            let _ = write!(out, "  y {y0}..{}: ", y0 + 15);
            for (id, count) in &counts {
                let _ = write!(out, "{}={count} ", block_name(*id));
            }
            out.push('\n');
        }
        if out.is_empty() {
            out.push_str("  (all air)\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nibble_round_trip_and_filled_sky() {
        let mut data = [0u8; LIGHT_NIBBLES];
        data[0] = 0xA5;
        let section = LightSection::from_bytes(&data);
        assert_eq!(section.get(0, 0, 0), 5);
        assert_eq!(section.get(1, 0, 0), 10);

        let light = ChunkLight::filled_sky(15);
        assert_eq!(light.at(0, -64, 0), (0, 15));
        assert_eq!(light.at(8, 64, 8), (0, 15));
        assert_eq!(light.at(0, -80, 0), (0, 0));
        assert_eq!(light.at(0, 320, 0), (0, 15));
        assert_eq!(light.at(0, 336, 0), (0, 15));
    }

    #[test]
    fn empty_sky_padding_is_full_when_the_dimension_has_a_sky() {
        let light = ChunkLight::new(Height::OVERWORLD);
        assert_eq!(light.at(0, 64, 0), (0, 0));
        assert_eq!(light.at(0, 320, 0), (0, 15));
        assert_eq!(light.at(0, 336, 0), (0, 15));
        assert_eq!(light.at(0, -80, 0), (0, 0));
        let mut nether = ChunkLight::new(Height::NETHER);
        nether.disable_skylight();
        assert_eq!(nether.at(0, 64, 0), (0, 0));
        assert_eq!(nether.at(0, 256, 0), (0, 0));
        assert_eq!(nether.at(0, 272, 0), (0, 0));
    }

    #[test]
    fn apply_replaces_named_sections() {
        let mut light = ChunkLight::filled_sky(15);
        light.apply(&LightUpdate {
            sky_mask: vec![],
            block_mask: vec![],
            empty_sky_mask: vec![1 << 5],
            empty_block_mask: vec![],
            sky: vec![],
            block: vec![],
        });
        assert_eq!(light.at(0, 0, 0), (0, 0));
        assert_eq!(light.at(0, 16, 0), (0, 15));
    }

    #[test]
    fn set_writes_a_nibble_into_an_empty_section() {
        let mut light = ChunkLight::default();
        light.set(3, 70, 5, 7, 15);
        assert_eq!(light.at(3, 70, 5), (7, 15));
        assert_eq!(light.at(4, 70, 5), (0, 0));
        light.set(3, 70, 5, 0, 0);
        assert_eq!(light.at(3, 70, 5), (0, 0));
    }
}
