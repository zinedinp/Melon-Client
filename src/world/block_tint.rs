//! Block tints sampled while meshing (vanilla `ClientLevel.calculateBlockTint`,

use std::sync::Arc;

use crate::world::biome::{Biome, Biomes, GrassModifier};
use crate::world::biome_zoom;
use crate::world::colormap::{self, ColorMaps};

/// Which colour map a tinted face reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TintKind {
    None,
    Grass,
    /// Double plants' upper half: vanilla samples the block below.
    GrassBelow,
    Foliage,
    DryFoliage,
    Water,
}

impl TintKind {
    fn source(self) -> Option<Source> {
        Some(match self {
            Self::None => return None,
            Self::Grass | Self::GrassBelow => Source::Grass,
            Self::Foliage => Source::Foliage,
            Self::DryFoliage => Source::DryFoliage,
            Self::Water => Source::Water,
        })
    }

    fn sample_pos(self, [x, y, z]: [i32; 3]) -> [i32; 3] {
        if self == Self::GrassBelow {
            [x, y - 1, z]
        } else {
            [x, y, z]
        }
    }
}

/// `BiomeColors`' four resolvers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Source {
    Grass,
    Foliage,
    DryFoliage,
    Water,
}

/// One biome's colours, packed `0xAARRGGBB`, by [`Source`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct BiomeTint {
    colors: [u32; 4],
    /// Swamp grass is noise by position, not one colour.
    swamp_grass: bool,
}

impl BiomeTint {
    fn of(biome: &Biome, maps: &ColorMaps) -> Self {
        Self {
            colors: [
                biome.grass_color_at(maps, 0.0, 0.0),
                biome.foliage_color_at(maps),
                biome.dry_foliage_color_at(maps),
                biome.water_color_packed(),
            ],
            swamp_grass: biome.grass_modifier == GrassModifier::Swamp,
        }
    }

    /// `ColorResolver.getColor` at block column `(x, z)`.
    #[inline]
    fn color(&self, source: Source, x: i32, z: i32) -> u32 {
        if source == Source::Grass && self.swamp_grass {
            return colormap::swamp_grass(f64::from(x), f64::from(z));
        }
        self.colors[source as usize]
    }
}

/// What the mesher needs to colour a section.
#[derive(Clone)]
pub struct TintSettings {
    /// Each biome's colours, by registry index.
    biomes: Arc<[BiomeTint]>,
    /// An index past the registry: plains, as an unknown biome looks.
    fallback: BiomeTint,
    /// `Options.biomeBlendRadius`.
    pub blend: u8,
    /// `BiomeManager.biomeZoomSeed` (the login's hashed seed): biome
    zoom_seed: Option<i64>,
}

impl TintSettings {
    /// Works the registry's colours out of the colour maps.
    pub fn new(biomes: &Biomes, maps: &ColorMaps, blend: u8, zoom_seed: Option<i64>) -> Self {
        let tints = (0..biomes.len())
            .map(|id| BiomeTint::of(&biomes.get(id as u16), maps))
            .collect();
        Self {
            biomes: tints,
            fallback: BiomeTint::of(&Biome::default(), maps),
            blend,
            zoom_seed,
        }
    }

    /// Plains, the default blend radius, the crate's colormap.
    pub fn plains() -> Self {
        Self::new(
            &Biomes::default(),
            ColorMaps::cached(),
            crate::settings::options::BIOME_BLEND_DEFAULT,
            None,
        )
    }

    /// A sampler reading biomes through `noise_biome`: the registry index
    pub fn sampler<'a>(
        &'a self,
        noise_biome: impl Fn(i32, i32, i32) -> u16 + 'a,
    ) -> TintSampler<'a> {
        TintSampler {
            settings: self,
            noise_biome: Box::new(noise_biome),
            layers: Vec::new(),
            last: 0,
            ids: Vec::new(),
            corners: Vec::new(),
            origin: None,
        }
    }

    /// A new level's `BiomeManager` seed (`ClientboundRespawnPacket`).
    pub fn set_zoom_seed(&mut self, seed: i64) {
        self.zoom_seed = Some(seed);
    }

    /// `BiomeManager`'s zoom seed, if blocks jitter between biome cells.
    pub fn zoom_seed(&self) -> Option<i64> {
        self.zoom_seed
    }

    fn tint(&self, id: u16) -> &BiomeTint {
        self.biomes.get(usize::from(id)).unwrap_or(&self.fallback)
    }

    /// The colour of the biome at block `(x, y, z)`, unblended.
    fn unblended(
        &self,
        noise: &dyn Fn(i32, i32, i32) -> u16,
        source: Source,
        x: i32,
        y: i32,
        z: i32,
    ) -> u32 {
        let id = match self.zoom_seed {
            Some(seed) => biome_zoom::biome(seed, x, y, z, noise),
            None => noise(x >> 2, y >> 2, z >> 2),
        };
        self.tint(id).color(source, x, z)
    }
}

/// Not a biome index: not looked up yet.
const NO_BIOME: u16 = u16::MAX;

/// The biomes at one block height, over the layers' columns: every colour
struct BiomeRow {
    y: i32,
    /// The [`CornerRow`]s of the parent cells' height and the one above.
    corners: [usize; 2],
    /// `side × side`, z-major, like [`Layer::columns`].
    ids: Box<[u16]>,
}

/// The quart corners at one height (see [`biome_zoom`]): each is shared by
struct CornerRow {
    qy: i32,
    /// `quart_side × quart_side`, z-major.
    corners: Box<[Corner]>,
    /// For the cell at each corner, as the parent `getBiome` looks from:
    parents: Box<[u16]>,
}

/// A parent cell whose corners hold more than one biome: its blocks pick
const MIXED: u16 = u16::MAX - 1;

#[derive(Clone, Copy)]
struct Corner {
    /// [`NO_BIOME`] until looked up.
    biome: u16,
    /// [`biome_zoom::corner_fiddle`]; NaN until worked out.
    fiddle: [f64; 3],
}

impl Corner {
    const UNKNOWN: Self = Self {
        biome: NO_BIOME,
        fiddle: [f64::NAN; 3],
    };
}

/// The grid geometry the rows share: block columns from the chunk less
#[derive(Clone, Copy)]
struct Grid {
    /// Block coordinates of column `(0, 0)`.
    min: [i32; 2],
    side: i32,
    /// Quart coordinates of corner `(0, 0)`.
    quart_min: [i32; 2],
    quart_side: i32,
}

impl Grid {
    fn new([ox, oz]: [i32; 2], radius: i32) -> Self {
        let side = 16 + 2 * radius;
        let min = [ox - radius, oz - radius];
        Self {
            min,
            side,
            quart_min: min.map(|v| (v - 2) >> 2),
            quart_side: side / 4 + 3,
        }
    }
}

/// The biome of grid column `(gx, gz)` at `row.y`.
fn column_biome(
    settings: &TintSettings,
    noise: &dyn Fn(i32, i32, i32) -> u16,
    grid: Grid,
    row: &mut BiomeRow,
    corners: &mut [CornerRow],
    gx: i32,
    gz: i32,
) -> u16 {
    let slot = &mut row.ids[(gz * grid.side + gx) as usize];
    if *slot != NO_BIOME {
        return *slot;
    }
    let (x, y, z) = (grid.min[0] + gx, row.y, grid.min[1] + gz);
    let id = match settings.zoom_seed {
        None => noise(x >> 2, y >> 2, z >> 2),
        Some(seed) => {
            let parent = biome_zoom::parent(x, y, z);
            let (qx, qz) = (parent[0] - grid.quart_min[0], parent[2] - grid.quart_min[1]);
            let inside =
                (0..grid.quart_side - 1).contains(&qx) && (0..grid.quart_side - 1).contains(&qz);
            if inside {
                let [low, high] = row.corners;
                let at = |i: usize| {
                    let c = biome_zoom::corner(parent, i);
                    let cell = ((c[2] - grid.quart_min[1]) * grid.quart_side + c[0]
                        - grid.quart_min[0]) as usize;
                    (if i & 2 == 0 { low } else { high }, cell, c)
                };
                let mut ids = [0u16; 8];
                let mut corner_ids = |corners: &mut [CornerRow]| {
                    for (i, id) in ids.iter_mut().enumerate() {
                        let (r, cell, c) = at(i);
                        let corner = &mut corners[r].corners[cell];
                        if corner.biome == NO_BIOME {
                            corner.biome = noise(c[0], c[1], c[2]);
                        }
                        *id = corner.biome;
                    }
                };
                let (_, parent_cell, _) = at(0);
                let mut verdict = corners[low].parents[parent_cell];
                if verdict == NO_BIOME {
                    corner_ids(corners);
                    verdict = if ids.iter().all(|&id| id == ids[0]) {
                        ids[0]
                    } else {
                        MIXED
                    };
                    corners[low].parents[parent_cell] = verdict;
                } else if verdict == MIXED {
                    corner_ids(corners);
                }
                if verdict != MIXED {
                    verdict
                } else {
                    let mut i = 0;
                    let best = biome_zoom::nearest_corner(x, y, z, |c| {
                        let (r, cell, _) = at(i);
                        i += 1;
                        let corner = &mut corners[r].corners[cell];
                        if corner.fiddle[0].is_nan() {
                            corner.fiddle = biome_zoom::corner_fiddle(seed, c);
                        }
                        corner.fiddle
                    });
                    ids[best]
                }
            } else {
                biome_zoom::biome(seed, x, y, z, noise)
            }
        }
    };
    *slot = id;
    id
}

/// Not a colour (those are opaque): not worked out yet.
const UNSET: u32 = 0;

/// Not a [`spread`] colour: a column not worked out yet.
const NO_COLUMN: u64 = u64::MAX;

/// Bits per channel in a [`spread`] colour: room for the sum of a blend's
const CHANNEL_BITS: u32 = 21;

/// An RGB colour with each channel in its own [`CHANNEL_BITS`] field, so
#[inline]
fn spread(color: u32) -> u64 {
    let [r, g, b] = colormap::unpack(color).map(u64::from);
    r << (2 * CHANNEL_BITS) | g << CHANNEL_BITS | b
}

/// One source at one block height, over the first chunk asked about.
struct Layer {
    source: Source,
    y: i32,
    /// Unblended colours of the chunk's columns and the blend margin
    columns: Box<[u64]>,
    /// Blended colours of the chunk's 16×16 columns, z-major.
    blended: Box<[u32; 256]>,
}

/// Blended colours for one section mesh (`BlockTintCache`, for one chunk).
pub struct TintSampler<'a> {
    settings: &'a TintSettings,
    noise_biome: Box<dyn Fn(i32, i32, i32) -> u16 + 'a>,
    layers: Vec<Layer>,
    /// The layer the last lookup found: runs of blocks share one.
    last: usize,
    ids: Vec<BiomeRow>,
    corners: Vec<CornerRow>,
    /// The north-west block of the chunk the layers cover: the first one
    origin: Option<[i32; 2]>,
}

impl TintSampler<'_> {
    /// Packed RGB the mesher multiplies onto a tinted face at `pos`.
    pub fn color(&mut self, kind: TintKind, pos: [i32; 3]) -> [u8; 3] {
        let Some(source) = kind.source() else {
            return [255; 3];
        };
        let [x, y, z] = kind.sample_pos(pos);
        let [ox, oz] = *self
            .origin
            .get_or_insert([x.div_euclid(16) * 16, z.div_euclid(16) * 16]);
        let (lx, lz) = (x - ox, z - oz);
        if !(0..16).contains(&lx) || !(0..16).contains(&lz) {
            return colormap::unpack(self.blend_uncached(source, [x, y, z]));
        }
        let layer = self.layer(source, y);
        let cell = (lz * 16 + lx) as usize;
        let cached = self.layers[layer].blended[cell];
        if cached != UNSET {
            return colormap::unpack(cached);
        }
        let color = self.blend_in_layer(layer, [ox, oz], lx, lz);
        self.layers[layer].blended[cell] = color;
        colormap::unpack(color)
    }

    /// The index of the layer of `source` at `y`, made if new.
    fn layer(&mut self, source: Source, y: i32) -> usize {
        let matches = |l: &Layer| l.source == source && l.y == y;
        if self.layers.get(self.last).is_some_and(matches) {
            return self.last;
        }
        self.last = match self.layers.iter().position(matches) {
            Some(i) => i,
            None => {
                let side = 16 + 2 * usize::from(self.settings.blend);
                self.layers.push(Layer {
                    source,
                    y,
                    columns: vec![NO_COLUMN; side * side].into_boxed_slice(),
                    blended: Box::new([UNSET; 256]),
                });
                self.layers.len() - 1
            }
        };
        self.last
    }

    /// `calculateBlockTint` for chunk column `(lx, lz)`, its columns read
    fn blend_in_layer(&mut self, layer: usize, origin: [i32; 2], lx: i32, lz: i32) -> u32 {
        let radius = i32::from(self.settings.blend);
        let grid = Grid::new(origin, radius);
        let y = self.layers[layer].y;
        let biomes = match self.ids.iter().position(|r| r.y == y) {
            Some(i) => i,
            None => {
                let qy = biome_zoom::parent(0, y, 0)[1];
                let corners = [qy, qy + 1].map(|qy| self.corner_row(grid, qy));
                self.ids.push(BiomeRow {
                    y,
                    corners,
                    ids: vec![NO_BIOME; (grid.side * grid.side) as usize].into_boxed_slice(),
                });
                self.ids.len() - 1
            }
        };
        let Self {
            settings,
            noise_biome,
            layers,
            ids,
            corners,
            ..
        } = self;
        let Layer {
            source, columns, ..
        } = &mut layers[layer];
        let biomes = &mut ids[biomes];
        let width = (2 * radius + 1) as usize;
        let mut sum = 0u64;
        for gz in lz..=lz + 2 * radius {
            let start = (gz * grid.side + lx) as usize;
            let row = &mut columns[start..start + width];
            if row.contains(&NO_COLUMN) {
                for (gx, slot) in (lx..).zip(row.iter_mut()) {
                    if *slot == NO_COLUMN {
                        let id =
                            column_biome(settings, &**noise_biome, grid, biomes, corners, gx, gz);
                        let (x, z) = (grid.min[0] + gx, grid.min[1] + gz);
                        *slot = spread(settings.tint(id).color(*source, x, z));
                    }
                }
            }
            sum += row.iter().sum::<u64>();
        }
        average_spread(sum, side_count(radius))
    }

    /// The index of the corner row at quart height `qy`, made if new.
    fn corner_row(&mut self, grid: Grid, qy: i32) -> usize {
        if let Some(i) = self.corners.iter().position(|r| r.qy == qy) {
            return i;
        }
        let cells = (grid.quart_side * grid.quart_side) as usize;
        self.corners.push(CornerRow {
            qy,
            corners: vec![Corner::UNKNOWN; cells].into_boxed_slice(),
            parents: vec![NO_BIOME; cells].into_boxed_slice(),
        });
        self.corners.len() - 1
    }

    /// `calculateBlockTint` outside the chunk the layers cover.
    fn blend_uncached(&self, source: Source, [x, y, z]: [i32; 3]) -> u32 {
        let radius = i32::from(self.settings.blend);
        let mut sum = [0u32; 3];
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let color = self
                    .settings
                    .unblended(&*self.noise_biome, source, x + dx, y, z + dz);
                add(&mut sum, color);
            }
        }
        average(sum, side_count(radius))
    }
}

fn side_count(radius: i32) -> u32 {
    let side = (2 * radius + 1) as u32;
    side * side
}

#[inline]
fn add(sum: &mut [u32; 3], color: u32) {
    sum[0] += (color >> 16) & 0xFF;
    sum[1] += (color >> 8) & 0xFF;
    sum[2] += color & 0xFF;
}

/// [`average`] of a sum of [`spread`] colours.
#[inline]
fn average_spread(sum: u64, count: u32) -> u32 {
    let mask = (1u64 << CHANNEL_BITS) - 1;
    let channel = |shift: u32| ((sum >> shift) & mask) as u32;
    average(
        [channel(2 * CHANNEL_BITS), channel(CHANNEL_BITS), channel(0)],
        count,
    )
}

/// `ARGB.color(255, ...)` of the channel averages, so never [`UNSET`].
fn average(sum: [u32; 3], count: u32) -> u32 {
    colormap::pack(sum.map(|s| (s / count) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plains_and_desert() -> Biomes {
        Biomes::new(vec![
            ("minecraft:plains".into(), Biome::default()),
            (
                "minecraft:desert".into(),
                Biome {
                    temperature: 2.0,
                    downfall: 0.0,
                    ..Biome::default()
                },
            ),
        ])
    }

    fn grass(temperature: f32, downfall: f32) -> [u8; 3] {
        colormap::unpack(ColorMaps::cached().grass.get(temperature, downfall))
    }

    #[test]
    fn blend_zero_is_the_block_biome() {
        let settings = TintSettings::new(&plains_and_desert(), ColorMaps::cached(), 0, None);
        let mut sampler = settings.sampler(|_, _, _| 1);
        assert_eq!(sampler.color(TintKind::Grass, [8, 64, 8]), grass(2.0, 0.0));
    }

    #[test]
    fn blend_averages_a_square() {
        let settings = TintSettings::new(&plains_and_desert(), ColorMaps::cached(), 1, None);
        let mut sampler = settings.sampler(|qx, _, qz| u16::from(qx != 2 || qz != 2));
        let (plains, desert) = (grass(0.8, 0.4), grass(2.0, 0.0));
        let mixed = sampler.color(TintKind::Grass, [8, 64, 8]);
        for i in 0..3 {
            let expected = (u32::from(plains[i]) * 4 + u32::from(desert[i]) * 5) / 9;
            assert_eq!(u32::from(mixed[i]), expected, "channel {i}");
        }
        assert_eq!(sampler.color(TintKind::Grass, [9, 64, 9]), plains);
    }

    #[test]
    fn unknown_biomes_are_plains() {
        let settings = TintSettings::new(&Biomes::default(), ColorMaps::cached(), 0, None);
        let mut sampler = settings.sampler(|_, _, _| 77);
        assert_eq!(sampler.color(TintKind::Grass, [0, 0, 0]), grass(0.8, 0.4));
    }

    #[test]
    fn layers_give_what_blending_each_block_gives() {
        let biomes = plains_and_desert();
        for (seed, blend) in [
            (None, 3),
            (Some(-4_172_144_997_891_118_834), 3),
            (Some(7), 7),
        ] {
            let settings = TintSettings::new(&biomes, ColorMaps::cached(), blend, seed);
            let biome =
                |qx: i32, qy: i32, qz: i32| u16::from((qx * 3 + qz + qy).rem_euclid(5) == 0);
            let mut sampler = settings.sampler(biome);
            for _ in 0..2 {
                for [x, y, z] in [
                    [0, 64, 0],
                    [15, 64, 15],
                    [7, 70, 3],
                    [20, 64, -9],
                    [-30, 1, 40],
                    [8, 64, 8],
                ] {
                    for kind in [TintKind::Grass, TintKind::Foliage, TintKind::Water] {
                        let source = kind.source().unwrap();
                        let fresh = colormap::unpack(sampler.blend_uncached(source, [x, y, z]));
                        assert_eq!(
                            sampler.color(kind, [x, y, z]),
                            fresh,
                            "{kind:?} {x} {y} {z}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn swamp_grass_follows_the_noise_by_column() {
        let swamp = Biome {
            grass_modifier: GrassModifier::Swamp,
            ..Biome::default()
        };
        let biomes = Biomes::new(vec![("minecraft:swamp".into(), swamp)]);
        let settings = TintSettings::new(&biomes, ColorMaps::cached(), 0, None);
        let mut sampler = settings.sampler(|_, _, _| 0);
        let seen: std::collections::HashSet<_> = (0..64)
            .map(|x| sampler.color(TintKind::Grass, [x * 7, 64, x * 3]))
            .collect();
        assert_eq!(seen.len(), 2, "both swamp colours: {seen:?}");
    }
}
