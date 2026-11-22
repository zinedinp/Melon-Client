//! One 16³ section of block states, stored as small as its contents allow.

use std::cmp::Ordering;

use crate::world::section_decode::{self, ROWS};

/// Blocks in one section.
pub const BLOCKS_PER_SECTION: usize = 16 * 16 * 16;

/// The widest palette index; more states than `1 << 8` go flat.
const MAX_BITS: u8 = 8;

#[derive(Clone)]
pub struct Section {
    store: Store,
}

#[derive(Clone)]
enum Store {
    Uniform(u16),
    Packed(Box<Packed>),
    Flat(Box<[u16; BLOCKS_PER_SECTION]>),
}

#[derive(Clone)]
struct Packed {
    bits: u8,
    /// Index to block state. At most `1 << bits` entries.
    palette: Vec<u16>,
    /// `64 * bits` words: block `i` sits at bit `i * bits`.
    words: Box<[u64]>,
}

impl Packed {
    fn new(bits: u8, palette: Vec<u16>) -> Self {
        Self {
            bits,
            palette,
            words: vec![0; 64 * bits as usize].into_boxed_slice(),
        }
    }

    #[inline]
    fn index(&self, i: usize) -> usize {
        let offset = i * self.bits as usize;
        ((self.words[offset >> 6] >> (offset & 63)) & ((1 << self.bits) - 1)) as usize
    }

    #[inline]
    fn get(&self, i: usize) -> u16 {
        self.palette[self.index(i)]
    }

    /// Palette indices of blocks `16 * row ..` along x.
    #[inline]
    fn row_indices(&self, row: usize) -> [u8; 16] {
        section_decode::row_indices(self.bits, &self.words, row)
    }

    fn put(&mut self, i: usize, index: usize) {
        let offset = i * self.bits as usize;
        let mask = (1u64 << self.bits) - 1;
        let word = &mut self.words[offset >> 6];
        *word = *word & !(mask << (offset & 63)) | (index as u64) << (offset & 63);
    }

    /// Whether `id` fit: false when the palette is full.
    fn set(&mut self, i: usize, id: u16) -> bool {
        let index = match self.palette.iter().position(|&p| p == id) {
            Some(index) => index,
            None if self.palette.len() < 1 << self.bits => {
                self.palette.push(id);
                self.palette.len() - 1
            }
            None => return false,
        };
        self.put(i, index);
        true
    }

    /// The same blocks at twice the width, or flat past the widest.
    fn widened(&self) -> Store {
        if self.bits >= MAX_BITS {
            return Store::Flat(self.to_flat());
        }
        let mut wider = Packed::new(self.bits * 2, self.palette.clone());
        for i in 0..BLOCKS_PER_SECTION {
            wider.put(i, self.index(i));
        }
        Store::Packed(Box::new(wider))
    }

    fn to_flat(&self) -> Box<[u16; BLOCKS_PER_SECTION]> {
        let mut flat = Box::new([0u16; BLOCKS_PER_SECTION]);
        for (i, slot) in flat.iter_mut().enumerate() {
            *slot = self.get(i);
        }
        flat
    }
}

impl Section {
    /// Index into the section for section-relative `0..16` coordinates:
    #[inline]
    pub const fn index(x: usize, y: usize, z: usize) -> usize {
        y * 256 + z * 16 + x
    }

    /// [`Section::index`] for any block coordinates, taking their low four bits.
    #[inline]
    pub const fn index_of_block(x: i32, y: i32, z: i32) -> usize {
        Self::index((x & 15) as usize, (y & 15) as usize, (z & 15) as usize)
    }

    /// All air.
    pub const fn empty() -> Self {
        Self::filled(0)
    }

    /// Every block `id`.
    pub const fn filled(id: u16) -> Self {
        Self {
            store: Store::Uniform(id),
        }
    }

    /// Narrows `blocks` (index order, see [`Section::index`]) to the smallest store that holds
    /// them.
    pub fn from_array(blocks: &[u16; BLOCKS_PER_SECTION]) -> Self {
        let first = blocks[0];
        if blocks.iter().all(|&id| id == first) {
            return Self::filled(first);
        }
        let mut palette: Vec<u16> = Vec::new();
        let mut slots = [0u16; 1024];
        let mut indices = Box::new([0u8; BLOCKS_PER_SECTION]);
        let mut previous = (first, 0u8);
        let mut fits = true;
        palette.push(first);
        slots[hash(first)] = 1;
        for (i, &id) in blocks.iter().enumerate() {
            if id == previous.0 {
                indices[i] = previous.1;
                continue;
            }
            let mut slot = hash(id);
            let index = loop {
                match slots[slot] {
                    0 => {
                        if palette.len() == 1 << MAX_BITS {
                            fits = false;
                            break 0;
                        }
                        palette.push(id);
                        slots[slot] = palette.len() as u16;
                        break palette.len() - 1;
                    }
                    n if palette[n as usize - 1] == id => break n as usize - 1,
                    _ => slot = (slot + 1) & 1023,
                }
            };
            if !fits {
                return Self {
                    store: Store::Flat(Box::new(*blocks)),
                };
            }
            indices[i] = index as u8;
            previous = (id, index as u8);
        }
        let bits = match palette.len() {
            0..=2 => 1,
            3..=4 => 2,
            5..=16 => 4,
            _ => 8,
        };
        let mut packed = Packed::new(bits, palette);
        for (i, &index) in indices.iter().enumerate() {
            packed.put(i, index as usize);
        }
        Self {
            store: Store::Packed(Box::new(packed)),
        }
    }

    /// The block state at `index`.
    #[inline]
    pub fn get(&self, index: usize) -> u16 {
        match &self.store {
            Store::Uniform(id) => *id,
            Store::Packed(packed) => packed.get(index),
            Store::Flat(flat) => flat[index],
        }
    }

    /// Sets the block at `index`, widening the section if it has to.
    pub fn set(&mut self, index: usize, id: u16) {
        match &mut self.store {
            Store::Uniform(current) => {
                if *current == id {
                    return;
                }
                let mut packed = Packed::new(1, vec![*current]);
                packed.set(index, id);
                self.store = Store::Packed(Box::new(packed));
            }
            Store::Packed(packed) => {
                if !packed.set(index, id) {
                    let mut widened = packed.widened();
                    match &mut widened {
                        Store::Packed(wider) => {
                            wider.set(index, id);
                        }
                        Store::Flat(flat) => flat[index] = id,
                        Store::Uniform(_) => unreachable!(),
                    }
                    self.store = widened;
                }
            }
            Store::Flat(flat) => flat[index] = id,
        }
    }

    /// The 16 blocks along x of row `row` (`y*16 + z`).
    #[inline]
    pub fn row(&self, row: usize) -> [u16; 16] {
        let start = row * 16;
        match &self.store {
            Store::Uniform(id) => [*id; 16],
            Store::Packed(packed) => {
                // A loop: `<[u8; 16]>::map` isn't inlined here.
                let mut ids = [0; 16];
                for (id, index) in ids.iter_mut().zip(packed.row_indices(row)) {
                    *id = packed.palette[usize::from(index)];
                }
                ids
            }
            Store::Flat(flat) => flat[start..start + 16].try_into().unwrap(),
        }
    }

    /// Every block, in index order.
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        (0..BLOCKS_PER_SECTION).map(|i| self.get(i))
    }

    /// The one state of a section that holds only one.
    pub fn uniform(&self) -> Option<u16> {
        match self.store {
            Store::Uniform(id) => Some(id),
            _ => None,
        }
    }

    /// Bits a block of a packed section (1, 2, 4 or 8); `None` for one
    /// state throughout or a flat section.
    pub fn packed_bits(&self) -> Option<u8> {
        match &self.store {
            Store::Packed(packed) => Some(packed.bits),
            _ => None,
        }
    }

    /// Plain air throughout. May say no for a section edited back to air.
    pub fn is_empty(&self) -> bool {
        self.uniform() == Some(0)
    }

    /// Whether some state `wanted` accepts may be in the section.
    pub fn may_hold(&self, wanted: impl Fn(u16) -> bool) -> bool {
        match &self.store {
            Store::Uniform(id) => wanted(*id),
            Store::Packed(packed) => packed.palette.iter().any(|&id| wanted(id)),
            Store::Flat(_) => true,
        }
    }

    /// Writes the blocks into `out`, in index order.
    pub fn copy_to(&self, out: &mut [u16; BLOCKS_PER_SECTION]) {
        match &self.store {
            Store::Uniform(id) => out.fill(*id),
            Store::Packed(packed) => {
                section_decode::decode(packed.bits, &packed.words, &packed.palette, out);
            }
            Store::Flat(flat) => out.copy_from_slice(&flat[..]),
        }
    }

    /// Rows of the blocks' flags, `K` of them (at most 8): bit `x` of
    /// `rows[k][y * 16 + z]` is bit `k` of `flags` of the block at
    /// `(x, y, z)`. `flags` is asked once a palette entry.
    pub fn flag_rows<const K: usize>(&self, flags: impl Fn(u16) -> u8) -> [[u16; ROWS]; K] {
        let mut rows = [[0u16; ROWS]; K];
        match &self.store {
            Store::Uniform(id) => {
                let bits = flags(*id);
                for (k, rows) in rows.iter_mut().enumerate() {
                    rows.fill(if bits >> k & 1 != 0 { u16::MAX } else { 0 });
                }
            }
            Store::Packed(packed) => {
                let mut table = [0u8; 256];
                for (entry, &id) in table.iter_mut().zip(&packed.palette) {
                    *entry = flags(id);
                }
                let len = packed.palette.len();
                section_decode::flag_rows(packed.bits, &packed.words, &table, len, &mut rows);
            }
            Store::Flat(flat) => {
                for (row, blocks) in flat.as_chunks::<16>().0.iter().enumerate() {
                    for (x, &id) in blocks.iter().enumerate() {
                        let bits = flags(id);
                        for (k, rows) in rows.iter_mut().enumerate() {
                            rows[row] |= u16::from(bits >> k & 1) << x;
                        }
                    }
                }
            }
        }
        rows
    }

    /// The blocks as a flat array.
    pub fn to_array(&self) -> Box<[u16; BLOCKS_PER_SECTION]> {
        match &self.store {
            Store::Uniform(id) => Box::new([*id; BLOCKS_PER_SECTION]),
            Store::Packed(packed) => packed.to_flat(),
            Store::Flat(flat) => flat.clone(),
        }
    }

    /// Bytes this section holds on the heap.
    pub fn heap_bytes(&self) -> usize {
        match &self.store {
            Store::Uniform(_) => 0,
            Store::Packed(packed) => {
                size_of::<Packed>() + packed.palette.capacity() * 2 + packed.words.len() * 8
            }
            Store::Flat(_) => size_of::<[u16; BLOCKS_PER_SECTION]>(),
        }
    }
}

#[inline]
fn hash(id: u16) -> usize {
    (u32::from(id).wrapping_mul(0x9E37_79B1) >> 22) as usize
}

impl PartialEq for Section {
    fn eq(&self, other: &Self) -> bool {
        match (self.uniform(), other.uniform()) {
            (Some(a), Some(b)) => a == b,
            _ => self.iter().eq(other.iter()),
        }
    }
}

impl PartialOrd for Section {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl std::fmt::Debug for Section {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.store {
            Store::Uniform(id) => write!(f, "Section(all {id})"),
            Store::Packed(p) => write!(f, "Section({} states, {} bits)", p.palette.len(), p.bits),
            Store::Flat(_) => write!(f, "Section(flat)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(states: u16) -> Box<[u16; BLOCKS_PER_SECTION]> {
        let mut blocks = Box::new([0u16; BLOCKS_PER_SECTION]);
        for (i, b) in blocks.iter_mut().enumerate() {
            *b = (i as u16 * 7 + i as u16 / 13) % states * 3;
        }
        blocks
    }

    #[test]
    fn a_section_of_one_state_holds_no_heap() {
        let s = Section::filled(9);
        assert_eq!(s.heap_bytes(), 0);
        assert_eq!(s.uniform(), Some(9));
        assert!(Section::empty().is_empty());
        assert_eq!(
            Section::from_array(&[5; BLOCKS_PER_SECTION]).uniform(),
            Some(5)
        );
    }

    #[test]
    fn from_array_picks_the_narrowest_width_and_keeps_every_block() {
        for (states, bytes) in [
            (2, 512),
            (3, 1024),
            (4, 1024),
            (5, 2048),
            (16, 2048),
            (17, 4096),
            (256, 4096),
        ] {
            let blocks = sample(states);
            let s = Section::from_array(&blocks);
            assert!(s.iter().eq(blocks.iter().copied()), "{states}");
            let Store::Packed(p) = &s.store else {
                panic!("{states} not packed")
            };
            assert_eq!(p.words.len() * 8, bytes, "{states}");
            assert_eq!(*s.to_array(), *blocks);
        }
        let blocks = sample(257);
        let s = Section::from_array(&blocks);
        assert!(matches!(s.store, Store::Flat(_)));
        assert_eq!(*s.to_array(), *blocks);
    }

    #[test]
    fn setting_widens_through_every_form() {
        let mut s = Section::empty();
        let mut model = [0u16; BLOCKS_PER_SECTION];
        for n in 0..300usize {
            let i = n * 13 % BLOCKS_PER_SECTION;
            let id = 1000 + n as u16;
            s.set(i, id);
            model[i] = id;
        }
        assert!(matches!(s.store, Store::Flat(_)));
        assert!(s.iter().eq(model.iter().copied()));
        let mut s = Section::empty();
        let mut model = [0u16; BLOCKS_PER_SECTION];
        for n in 1..=40usize {
            let i = n * 101 % BLOCKS_PER_SECTION;
            s.set(i, n as u16);
            model[i] = n as u16;
            assert_eq!(s.get(i), n as u16);
        }
        assert!(s.iter().eq(model.iter().copied()));
        assert_eq!(s.row(0), std::array::from_fn(|x| model[x]));
    }

    #[test]
    fn setting_the_same_state_keeps_a_uniform_section_uniform() {
        let mut s = Section::filled(3);
        s.set(10, 3);
        assert_eq!(s.uniform(), Some(3));
        s.set(10, 4);
        assert_eq!((s.get(10), s.get(11)), (4, 3));
        assert_eq!(s, {
            let mut t = Section::filled(3);
            t.set(10, 4);
            t
        });
    }

    #[test]
    fn may_hold_goes_by_the_palette() {
        let mut s = Section::filled(1);
        assert!(!s.may_hold(|id| id == 2));
        s.set(0, 2);
        assert!(s.may_hold(|id| id == 2));
        assert!(!s.may_hold(|id| id == 3));
    }

    #[test]
    fn rows_come_out_in_x_order() {
        let blocks = sample(10);
        let s = Section::from_array(&blocks);
        for row in [0usize, 17, 255] {
            assert_eq!(s.row(row)[..], blocks[row * 16..row * 16 + 16]);
        }
    }
}
