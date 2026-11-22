//! Vanilla's smooth lighting and ambient occlusion for block faces.
//!
//! [`LightCache`] flat 20³ array: one cell per block of the
//! section and a two-block shell, filled once a section slab by slab (the
//! 27 sections it overlaps, each looked up once), so the quad loop only
//! indexes. The centre's layers go through the byte kernels
//! ([`crate::world::nibble_decode`]); the shell reads its few nibbles.

use crate::renderer::CornerLight;
use crate::world::block::ao_flags::{self, AoFlags};
use crate::world::block::direction::Direction;
use crate::world::block_models::ao_shape::AoShape;
use crate::world::chunk::{LIGHT_NIBBLES, LightSection};
use crate::world::nibble_decode;
use crate::world::section::{BLOCKS_PER_SECTION, Section};
use crate::world::terrain::Neighborhood;

/// `LightCoordsUtil.pack`.
fn pack(block: u32, sky: u32) -> u32 {
    block << 4 | sky << 20
}

/// `LightCoordsUtil.block` / `sky` of packed coordinates.
fn block_of(packed: u32) -> u32 {
    packed >> 4 & 15
}
fn sky_of(packed: u32) -> u32 {
    packed >> 20 & 15
}

/// `LightCoordsUtil.FULL_BRIGHT`, what `emissiveRendering` blocks get.
const FULL_BRIGHT: u32 = 0xF0_00F0;

/// `LightCoordsUtil.smoothBlend`: three neighbours and the centre averaged;
fn smooth_blend(mut n1: u32, mut n2: u32, mut n3: u32, center: u32) -> u32 {
    if sky_of(center) > 2 || block_of(center) > 2 {
        for n in [&mut n1, &mut n2, &mut n3] {
            if *n == 0 {
                *n = center;
            } else if sky_of(*n) == 0 {
                *n |= center & 0xFF_0000;
            }
        }
    }
    (n1 + n2 + n3 + center) >> 2 & 0xFF_00FF
}

/// Java `(int)float` toward zero, then `& 0xFF` as `smoothPack` does.
fn java_u8(v: f32) -> u32 {
    (v as i32 as u32) & 0xFF
}

/// `LightCoordsUtil.smoothWeightedBlend` of four smooth coordinates.
fn smooth_weighted_blend(coords: [u32; 4], weights: [f32; 4]) -> u32 {
    let mut sky = 0.0f32;
    let mut block = 0.0f32;
    for (c, w) in coords.into_iter().zip(weights) {
        sky += (c >> 16 & 0xFF) as f32 * w;
        block += (c & 0xFF) as f32 * w;
    }
    java_u8(block) | java_u8(sky) << 16
}

/// One cell of the cache, 4 bytes so the 20³ stays in L1.
#[derive(Clone, Copy)]
#[repr(C)]
struct Cell {
    flags: AoFlags,
    /// Block and sky (`block | sky << 4`) as the face reads them: the
    /// block's own emission in, both 15 for `emissiveRendering`.
    lit: u8,
    /// The stored levels, `block | sky << 4`, for flat light.
    levels: u8,
}

impl Cell {
    const EMPTY: Self = Self {
        flags: AoFlags::AIR,
        lit: 0,
        levels: 0,
    };

    #[inline]
    fn new(flags: AoFlags, block: u8, sky: u8) -> Self {
        // Branch free, for the combine to vectorize.
        let full = 0u8.wrapping_sub(u8::from(flags.emissive_rendering()));
        let lit = (block.max(flags.emission()) | sky << 4) | full;
        Self {
            flags,
            lit,
            levels: block | sky << 4,
        }
    }

    /// Vanilla's packed light coordinates.
    #[inline]
    fn light(self) -> u32 {
        let lit = u32::from(self.lit);
        pack(lit & 15, lit >> 4)
    }

    fn block(self) -> u8 {
        self.levels & 15
    }

    fn sky(self) -> u8 {
        self.levels >> 4
    }

    /// `getShadeBrightness`.
    fn shade(self) -> f32 {
        if self.flags.shade_dark() { 0.2 } else { 1.0 }
    }
}

/// How far around the section faces look.
const SHELL: i32 = 2;
/// Side of the cached cube: a section plus the shell on every side.
const SIDE: usize = 16 + 2 * SHELL as usize;
const CELLS: usize = SIDE * SIDE * SIDE;

/// Index of section-relative `(x, y, z)` in `-2..=17`.
#[inline]
fn cell_index(x: i32, y: i32, z: i32) -> usize {
    debug_assert!(
        (-SHELL..16 + SHELL).contains(&x)
            && (-SHELL..16 + SHELL).contains(&y)
            && (-SHELL..16 + SHELL).contains(&z),
        "{x},{y},{z} outside the cache"
    );
    ((y + SHELL) as usize * SIDE + (z + SHELL) as usize) * SIDE + (x + SHELL) as usize
}

/// The light and flags of a section's blocks and their neighbours, filled
/// densely once a section (Sodium's `ArrayLightDataCache` layout).
pub struct LightCache {
    cells: Vec<Cell>,
    section: usize,
    scratch: Box<Scratch>,
}

/// One slab's overlap, gathered first and combined into cells after:
/// whole passes keep the combine free of matches and let it vectorize,
/// and read back stores long retired (no store-forwarding stalls).
struct Scratch {
    flags: [AoFlags; BLOCKS_PER_SECTION],
    block: [u8; BLOCKS_PER_SECTION],
    sky: [u8; BLOCKS_PER_SECTION],
}

impl Default for LightCache {
    fn default() -> Self {
        Self {
            cells: vec![Cell::EMPTY; CELLS],
            section: 0,
            scratch: Box::new(Scratch {
                flags: [AoFlags::AIR; BLOCKS_PER_SECTION],
                block: [0; BLOCKS_PER_SECTION],
                sky: [0; BLOCKS_PER_SECTION],
            }),
        }
    }
}

/// A slab's blocks: one state, the unpacked centre, or a section.
enum States<'a> {
    One(u16),
    Flat(&'a [u16; BLOCKS_PER_SECTION]),
    Packed(&'a Section),
}

/// A slab's levels of one layer.
#[derive(Clone, Copy)]
enum Levels<'a> {
    One(u8),
    Nibbles(&'a [u8; LIGHT_NIBBLES]),
}

impl Levels<'_> {
    /// Layer `slot` (light sections start one below the world) of `layers`,
    /// vanilla's sky above the column and in its empty top padding.
    fn of(layers: &[LightSection], slot: i32, sky: bool, has_skylight: bool) -> Levels<'_> {
        let top = sky && has_skylight;
        match usize::try_from(slot).ok().and_then(|i| layers.get(i)) {
            Some(LightSection::Data(data)) => Levels::Nibbles(data),
            Some(LightSection::Empty) if top && slot as usize + 1 == layers.len() => {
                Levels::One(15)
            }
            None if top && slot >= layers.len() as i32 => Levels::One(15),
            _ => Levels::One(0),
        }
    }
}

/// A slab's overlap with the cache: `lo..hi` section-relative, `base` the
/// slab's corner.
#[derive(Clone, Copy)]
struct Overlap {
    lo: [i32; 3],
    hi: [i32; 3],
    base: [i32; 3],
}

impl Overlap {
    fn of(slab: [i32; 3]) -> Self {
        let base = slab.map(|d| d * 16);
        Self {
            lo: base.map(|b| b.max(-SHELL)),
            hi: base.map(|b| (b + 16).min(16 + SHELL)),
            base,
        }
    }

    fn width(self) -> usize {
        (self.hi[0] - self.lo[0]) as usize
    }

    /// Each row of x: its first section index and first cell, in order.
    fn rows(self) -> impl Iterator<Item = (usize, usize)> {
        let lx = (self.lo[0] - self.base[0]) as usize;
        (self.lo[1]..self.hi[1]).flat_map(move |y| {
            (self.lo[2]..self.hi[2]).map(move |z| {
                let row = ((y - self.base[1]) * 16 + (z - self.base[2])) as usize;
                (row * 16 + lx, cell_index(self.lo[0], y, z))
            })
        })
    }
}

impl LightCache {
    /// Starts on section `s`: fills the 20³ from `neighborhood`.
    pub fn start(&mut self, neighborhood: &Neighborhood, s: usize) {
        self.section = s;
        for ds in -1..=1 {
            for cz in -1..=1 {
                for cx in -1..=1 {
                    self.fill_slab(neighborhood, [cx, ds, cz]);
                }
            }
        }
    }

    /// The overlap of the 16³ slab `slab` (in sections, centre `0`).
    fn fill_slab(&mut self, neighborhood: &Neighborhood, slab: [i32; 3]) {
        let overlap = Overlap::of(slab);
        let centre = slab == [0; 3];
        let chunk = neighborhood.chunks[((slab[0] + 1) + (slab[2] + 1) * 3) as usize];
        let index = self.section as i32 + slab[1];

        let states =
            match chunk.and_then(|c| usize::try_from(index).ok().and_then(|i| c.sections.get(i))) {
                None => States::One(0),
                Some(section) => match (section.uniform(), neighborhood.decoded_center()) {
                    (Some(id), _) => States::One(id),
                    (None, Some((s, flat))) if centre && s == self.section => States::Flat(flat),
                    (None, _) => States::Packed(section),
                },
            };
        let [block, sky] = match chunk {
            None => [Levels::One(0); 2],
            Some(c) => {
                let has_skylight = c.light.has_skylight();
                let slot = index + 1;
                [
                    Levels::of(&c.light.block, slot, false, has_skylight),
                    Levels::of(&c.light.sky, slot, true, has_skylight),
                ]
            }
        };

        let width = overlap.width();
        if let (States::One(id), Levels::One(b), Levels::One(s)) = (&states, block, sky) {
            let cell = Cell::new(ao_flags::of(*id), b, s);
            for (_, at) in overlap.rows() {
                self.cells[at..at + width].fill(cell);
            }
            return;
        }

        let scratch = &mut *self.scratch;
        let n = overlap.rows().count() * width;
        let table = ao_flags::table();
        let flags_of = |id: u16| {
            table
                .get(usize::from(id))
                .copied()
                .unwrap_or_else(|| ao_flags::of(id))
        };
        match states {
            States::One(id) => scratch.flags[..n].fill(flags_of(id)),
            States::Flat(flat) => {
                for (out, (i, _)) in scratch.flags.chunks_exact_mut(width).zip(overlap.rows()) {
                    for (o, &id) in out.iter_mut().zip(&flat[i..i + width]) {
                        *o = flags_of(id);
                    }
                }
            }
            States::Packed(section) => {
                for (out, (i, _)) in scratch.flags.chunks_exact_mut(width).zip(overlap.rows()) {
                    for (o, i) in out.iter_mut().zip(i..) {
                        *o = flags_of(section.get(i));
                    }
                }
            }
        }
        for (levels, out) in [(block, &mut scratch.block), (sky, &mut scratch.sky)] {
            match levels {
                Levels::One(level) => out[..n].fill(level),
                // The centre is whole and in index order: the byte kernels.
                Levels::Nibbles(data) if centre => nibble_decode::unpack(data, out),
                Levels::Nibbles(data) => {
                    for (out, (i, _)) in out.chunks_exact_mut(width).zip(overlap.rows()) {
                        for (o, i) in out.iter_mut().zip(i..) {
                            *o = data[i >> 1] >> ((i & 1) * 4) & 15;
                        }
                    }
                }
            }
        }

        let rows = scratch.flags[..n]
            .chunks_exact(width)
            .zip(scratch.block.chunks_exact(width))
            .zip(scratch.sky.chunks_exact(width));
        for (((flags, block), sky), (_, at)) in rows.zip(overlap.rows()) {
            let cells = &mut self.cells[at..at + width];
            for (((cell, &f), &b), &s) in cells.iter_mut().zip(flags).zip(block).zip(sky) {
                *cell = Cell::new(f, b, s);
            }
        }
    }

    /// The cell at section-relative `(x, y, z)`, each `-2..=17`.
    fn at(&self, [x, y, z]: [i32; 3]) -> Cell {
        self.cells[cell_index(x, y, z)]
    }

    /// The flags of the block at section-relative `pos`.
    pub fn flags(&self, pos: [i32; 3]) -> AoFlags {
        self.at(pos).flags
    }
}

/// `AdjacencyInfo.corners`: the four sides a face's corners lie towards.
const fn adjacent(direction: Direction) -> [Direction; 4] {
    use Direction::*;
    match direction {
        Down => [West, East, North, South],
        Up => [East, West, North, South],
        North => [Up, Down, East, West],
        South => [West, East, Down, Up],
        West => [Up, Down, North, South],
        East => [Down, Up, North, South],
    }
}

/// A step along `d` as an offset between cells.
const fn delta(d: Direction) -> isize {
    let [x, y, z] = d.step();
    (x + (z + y * SIDE as i32) * SIDE as i32) as isize
}

/// A face's neighbourhood as cell offsets: the side it faces, the four
/// corner sides, and the diagonals `c0+c2, c0+c3, c1+c2, c1+c3`.
struct Around {
    forward: isize,
    sides: [isize; 4],
    diagonals: [isize; 4],
}

const AROUND: [Around; 6] = {
    let mut out = [const {
        Around {
            forward: 0,
            sides: [0; 4],
            diagonals: [0; 4],
        }
    }; 6];
    let mut i = 0;
    while i < 6 {
        let d = Direction::ALL[i];
        let c = adjacent(d);
        let sides = [delta(c[0]), delta(c[1]), delta(c[2]), delta(c[3])];
        out[d.index()] = Around {
            forward: delta(d),
            sides,
            diagonals: [
                sides[0] + sides[2],
                sides[0] + sides[3],
                sides[1] + sides[2],
                sides[1] + sides[3],
            ],
        };
        i += 1;
    }
    out
};

/// Smooth coordinates as a corner.
fn corner(smooth: u32, ao: f32, emission: u8) -> CornerLight {
    let (mut block, mut sky) = (smooth & 0xFF, smooth >> 16 & 0xFF);
    if emission != 0 {
        let emission = u32::from(emission);
        block = (block >> 4).max(emission) << 4;
        sky = (sky >> 4).max(emission) << 4;
    }
    let quarter = |v: u32| ((v + 2) / 4).min(60) as u8;
    let gray = (ao * 255.0).floor() as u32;
    let ao = (gray * 60).div_ceil(255).min(60) as u8;
    CornerLight {
        block: quarter(block),
        sky: quarter(sky),
        ao,
    }
}

/// A face's four corners, smooth lit.
pub fn smooth_corners(
    cache: &LightCache,
    shape: &AoShape,
    pos: [i32; 3],
    center: AoFlags,
    emission: u8,
) -> [CornerLight; 4] {
    let groups = shape.groups;
    let around = &AROUND[shape.direction.index()];
    let cubic = shape.cubic(center.collision_full());
    let [x, y, z] = pos;
    let here = cell_index(x, y, z);
    let front = here.wrapping_add_signed(around.forward);
    let base = if cubic { front } else { here };
    let cell = |i: usize, d: isize| cache.cells[i.wrapping_add_signed(d)];

    let n = around.sides.map(|d| cell(base, d));
    // Past each corner's side, one more along the face: vanilla's
    // `corners[i]` then `move(direction)`, from `base`.
    let beyond = base.wrapping_add_signed(around.forward);
    let permeable = around
        .sides
        .map(|d| cell(beyond, d).flags.light_permeable());
    let (light, shade) = (n.map(Cell::light), n.map(Cell::shade));
    let diagonal = |k: usize, a: usize, b: usize| {
        if !permeable[a] && !permeable[b] {
            (shade[0], light[0])
        } else {
            let c = cell(base, around.diagonals[k]);
            (c.shade(), c.light())
        }
    };
    let (shade02, light02) = diagonal(0, 2, 0);
    let (shade03, light03) = diagonal(1, 3, 0);
    let (shade12, light12) = diagonal(2, 2, 1);
    let (shade13, light13) = diagonal(3, 3, 1);

    let next = cache.cells[front];
    let light_center = if cubic || !next.flags.solid_render() {
        next.light()
    } else {
        cache.cells[here].light()
    };
    let shade_center = cache.cells[base].shade();

    let ao = [
        (shade[3] + shade[0] + shade03 + shade_center) * 0.25,
        (shade[2] + shade[0] + shade02 + shade_center) * 0.25,
        (shade[2] + shade[1] + shade12 + shade_center) * 0.25,
        (shade[3] + shade[1] + shade13 + shade_center) * 0.25,
    ];
    let blended = [
        smooth_blend(light[3], light[0], light03, light_center),
        smooth_blend(light[2], light[0], light02, light_center),
        smooth_blend(light[2], light[1], light12, light_center),
        smooth_blend(light[3], light[1], light13, light_center),
    ];
    std::array::from_fn(|i| {
        if shape.partial {
            let w = shape.weights[i];
            let ao = (ao[0] * w[0] + ao[1] * w[1] + ao[2] * w[2] + ao[3] * w[3]).clamp(0.0, 1.0);
            corner(smooth_weighted_blend(blended, w), ao, emission)
        } else {
            let k = usize::from(groups[i]);
            corner(blended[k], ao[k], emission)
        }
    })
}

/// A face's corners, flat lit (`prepareQuadFlat`): one light for all four.
pub fn flat_corners(
    cache: &LightCache,
    light_pos: [i32; 3],
    center: AoFlags,
    emission: u8,
) -> [CornerLight; 4] {
    let cell = cache.at(light_pos);
    let light = if center.emissive_rendering() {
        FULL_BRIGHT
    } else {
        pack(
            u32::from(cell.block().max(center.emission())),
            u32::from(cell.sky()),
        )
    };
    let smooth = (block_of(light) << 4) | (sky_of(light) << 4) << 16;
    [corner(smooth, 1.0, emission); 4]
}

/// What the old lazy fill stored for one cell, for the tests.
#[cfg(test)]
fn compute_cell(neighborhood: &Neighborhood, s: usize, [x, y, z]: [i32; 3]) -> (u32, AoFlags) {
    let state = neighborhood.block(s, x, y, z).unwrap_or(0);
    let flags = ao_flags::of(state);
    let light = if flags.emissive_rendering() {
        FULL_BRIGHT
    } else {
        let (block, sky) = neighborhood.light(s, x, y, z);
        pack(u32::from(block.max(flags.emission())), u32::from(sky))
    };
    (light, flags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{Chunk, ChunkLight};
    use crate::world::section::Section;

    #[test]
    fn blending_follows_vanilla() {
        let lit = pack(0, 15);
        assert_eq!(smooth_blend(lit, lit, lit, lit), 240 << 16);
        assert_eq!(smooth_blend(0, lit, lit, lit), 240 << 16);
        let dim = pack(0, 2);
        assert_eq!(smooth_blend(0, 0, 0, dim) >> 16, 8);
        let torch = pack(14, 0);
        let out = smooth_blend(torch, lit, lit, lit);
        assert_eq!(out & 0xFF, 14 * 16 / 4);
        assert_eq!(out >> 16, 240);
    }

    #[test]
    fn weighted_blend_wraps_negative_like_java() {
        assert_eq!(
            smooth_weighted_blend([16, 0, 0, 0], [-1.0, 0.0, 0.0, 0.0]),
            240
        );
        assert_eq!(
            smooth_weighted_blend([16 << 16, 0, 0, 0], [-1.0, 0.0, 0.0, 0.0]),
            240 << 16
        );
        assert_eq!(
            smooth_weighted_blend([240, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
            240
        );
    }

    #[test]
    fn corners_store_vanilla_gray() {
        for (ao, gray) in [
            (1.0f32, 255u32),
            (0.8, 204),
            (0.6, 153),
            (0.4, 102),
            (0.2, 51),
        ] {
            let c = corner(0, ao, 0);
            assert_eq!(u32::from(c.ao) * 255 / 60, gray, "ao {ao}");
        }
        let c = corner((7 * 16) | ((12 * 16) << 16), 1.0, 0);
        assert_eq!((c.block, c.sky), (28, 48));
        let c = corner(3 * 16, 1.0, 15);
        assert_eq!((c.block, c.sky), (60, 60));
    }

    #[test]
    fn a_filled_cache_matches_the_old_cell_by_cell_lookups() {
        let mut sections: Vec<Section> = (0..3).map(|_| Section::empty()).collect();
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    if (x + y + z) % 5 == 0 {
                        sections[1].set(Section::index(x, y, z), 1);
                    }
                }
            }
        }
        sections[0].set(Section::index(14, 15, 3), 1);
        sections[2].set(Section::index(1, 0, 8), 1);
        let mut light = ChunkLight::new(crate::world::dimension::Height {
            min_y: crate::world::dimension::Height::OVERWORLD.min_y,
            height: sections.len() as i32 * 16,
        });
        for y in -80i32..-80 + 3 * 16 + 32 {
            for z in 0..16i32 {
                for x in 0..16i32 {
                    let block = ((x + y.rem_euclid(16) + z) % 16) as u8;
                    let sky = 15 - block;
                    light.set(x, y, z, block, sky);
                }
            }
        }
        let center = Chunk::new(0, 0, sections).with_light(light);
        let mut west_sections: Vec<Section> = (0..3).map(|_| Section::empty()).collect();
        west_sections[1].set(Section::index(15, 8, 8), 1);
        let west = Chunk::new(-1, 0, west_sections);
        let mut chunks = [None; 9];
        chunks[4] = Some(&center);
        chunks[3] = Some(&west);
        let mut flat = Box::new([0u16; BLOCKS_PER_SECTION]);
        center.sections[1].copy_to(&mut flat);
        let neighborhood = Neighborhood::with_chunks(chunks, Some((1, &*flat)));

        let mut cache = LightCache::default();
        cache.start(&neighborhood, 1);
        for y in -SHELL..16 + SHELL {
            for z in -SHELL..16 + SHELL {
                for x in -SHELL..16 + SHELL {
                    let (light, flags) = compute_cell(&neighborhood, 1, [x, y, z]);
                    let cell = cache.at([x, y, z]);
                    assert_eq!(cell.light(), light, "light at {x},{y},{z}");
                    assert_eq!(cell.flags, flags, "flags at {x},{y},{z}");
                }
            }
        }
    }

    #[test]
    fn empty_sky_padding_fills_the_cache_like_vanilla() {
        let chunk = Chunk::new(0, 0, vec![Section::empty()]);
        let neighborhood = Neighborhood::alone(&chunk);
        let mut cache = LightCache::default();
        cache.start(&neighborhood, 0);
        let cell = cache.at([8, 16, 8]);
        assert_eq!(cell.sky(), 15);
        assert_eq!(sky_of(cell.light()), 15);
        let below = cache.at([8, 0, 8]);
        assert_eq!(below.sky(), 0);
    }

    /// `prepareQuadAmbientOcclusion` read off the neighbourhood by
    /// coordinates, for full faces: what the cache's offsets must match.
    fn corners_by_coordinates(
        n: &Neighborhood,
        shape: &AoShape,
        pos: [i32; 3],
        center: AoFlags,
    ) -> [CornerLight; 4] {
        let at = |p: [i32; 3]| compute_cell(n, 1, p);
        let shade = |f: AoFlags| if f.shade_dark() { 0.2 } else { 1.0 };
        let step = |a: [i32; 3], d: Direction| {
            let [x, y, z] = d.step();
            [a[0] + x, a[1] + y, a[2] + z]
        };
        let d = shape.direction;
        let cubic = shape.cubic(center.collision_full());
        let base = if cubic { step(pos, d) } else { pos };
        let c = adjacent(d);
        let side = c.map(|c| at(step(base, c)));
        let permeable = c.map(|c| at(step(step(base, c), d)).1.light_permeable());
        let diagonal = |i: usize, j: usize| {
            let (light, flags) = if !permeable[i] && !permeable[j] {
                side[0]
            } else {
                at(step(step(base, c[i]), c[j]))
            };
            (shade(flags), light)
        };
        let [d02, d03, d12, d13] = [
            diagonal(0, 2),
            diagonal(0, 3),
            diagonal(1, 2),
            diagonal(1, 3),
        ];
        let next = at(step(pos, d));
        let center_light = if cubic || !next.1.solid_render() {
            next.0
        } else {
            at(pos).0
        };
        let center_shade = shade(at(base).1);
        let (l, sh) = (side.map(|s| s.0), side.map(|s| shade(s.1)));
        let ao = [
            (sh[3] + sh[0] + d03.0 + center_shade) * 0.25,
            (sh[2] + sh[0] + d02.0 + center_shade) * 0.25,
            (sh[2] + sh[1] + d12.0 + center_shade) * 0.25,
            (sh[3] + sh[1] + d13.0 + center_shade) * 0.25,
        ];
        let blended = [
            smooth_blend(l[3], l[0], d03.1, center_light),
            smooth_blend(l[2], l[0], d02.1, center_light),
            smooth_blend(l[2], l[1], d12.1, center_light),
            smooth_blend(l[3], l[1], d13.1, center_light),
        ];
        std::array::from_fn(|i| {
            let k = usize::from(shape.groups[i]);
            corner(blended[k], ao[k], 0)
        })
    }

    /// A whole face towards `d`, on the block's side (`at` 1) or inside it.
    fn face(d: Direction, at: f32) -> AoShape {
        let a = d.axis();
        let (u, w) = ((a + 1) % 3, (a + 2) % 3);
        let mut corners = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)].map(|(cu, cw)| {
            let mut v = glam::Vec3::ZERO;
            v[a] = if d.is_positive() { at } else { 1.0 - at };
            v[u] = cu;
            v[w] = cw;
            v
        });
        if AoShape::of(corners).direction != d {
            corners.reverse();
        }
        let shape = AoShape::of(corners);
        assert_eq!(shape.direction, d);
        shape
    }

    #[test]
    fn smooth_corners_read_the_cells_vanilla_names() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut noise = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut chunks = Vec::new();
        for cz in -1..=1 {
            for cx in -1..=1 {
                let mut sections: Vec<Section> = (0..3).map(|_| Section::empty()).collect();
                for section in &mut sections {
                    for i in 0..BLOCKS_PER_SECTION {
                        if noise() % 3 == 0 {
                            section.set(i, 1);
                        }
                    }
                }
                let mut light = ChunkLight::new(crate::world::dimension::Height {
                    min_y: crate::world::dimension::Height::OVERWORLD.min_y,
                    height: 3 * 16,
                });
                for y in -80i32..0 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let r = noise();
                            light.set(x, y, z, (r & 15) as u8, (r >> 4 & 15) as u8);
                        }
                    }
                }
                chunks.push(Chunk::new(cx, cz, sections).with_light(light));
            }
        }
        let mut around = [None; 9];
        for (slot, chunk) in around.iter_mut().zip(&chunks) {
            *slot = Some(chunk);
        }
        let mut flat = Box::new([0u16; BLOCKS_PER_SECTION]);
        chunks[4].sections[1].copy_to(&mut flat);
        let n = Neighborhood::with_chunks(around, Some((1, &*flat)));
        let mut cache = LightCache::default();
        cache.start(&n, 1);

        let edge = [0, 1, 7, 14, 15];
        for d in Direction::ALL {
            for shape in [face(d, 1.0), face(d, 0.5)] {
                for &x in &edge {
                    for &y in &edge {
                        for &z in &edge {
                            let pos = [x, y, z];
                            let center = cache.flags(pos);
                            assert_eq!(
                                smooth_corners(&cache, &shape, pos, center, 0),
                                corners_by_coordinates(&n, &shape, pos, center),
                                "{d:?} at {pos:?}, cubic {}",
                                shape.cubic(center.collision_full())
                            );
                        }
                    }
                }
            }
        }
    }
}
