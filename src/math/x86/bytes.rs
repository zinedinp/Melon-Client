//! [`Bytes`] on x86_64: SSE2 (16 lanes, compare lookups), SSSE3 (16,
//! `pshufb` lookups), AVX2 (32, two
//! 16-lane halves) and AVX-512 VBMI (64, `vpermi2b` over 128 entries at a
//! time). Every op is a `miraculix` token method, so the tier holding the
//! tokens is all the proof the kernels need: no `unsafe` here.
//!
//! A table of 256 entries is 16 chunks of 16 for `pshufb`: chunk `c` takes
//! the lanes whose index, less `16 * c`, is below 16. Saturating `+ 0x70`
//! keeps those under `0x80` with their low four bits, and pushes every
//! other lane (wrapped below zero or past 15) to `0x80` and up, which
//! `pshufb` turns into zero.

use miraculix::x86::FeatureSet;
use miraculix::x86::bundles::V4Vbmi;
use miraculix::x86::ops::avx::avx2::Avx2;
use miraculix::x86::ops::sse::sse2::Sse2;
use miraculix::x86::ops::sse::ssse3::Ssse3;

use crate::math::bytes::{Bytes, chunks};

/// Each byte four times: lane `i` reads byte `i / 4` (two-bit fields).
const SPREAD_2: [u8; 16] = [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3];
/// Each byte eight times (one-bit fields).
const SPREAD_1: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1];
/// A two-bit field's low and high bit, lane `i`'s at `2 * (i % 4)`.
const LOW_2: [u8; 16] = [1, 4, 16, 64, 1, 4, 16, 64, 1, 4, 16, 64, 1, 4, 16, 64];
const HIGH_2: [u8; 16] = [2, 8, 32, 128, 2, 8, 32, 128, 2, 8, 32, 128, 2, 8, 32, 128];
/// Lane `i`'s bit, `i % 8`.
const BIT_1: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];

/// `pattern` (16 bytes) once a 128-bit lane, each lane's bytes moved on by
/// `step` from the last's: the in-lane shuffles of the wide tiers.
const fn lanes<const N: usize>(pattern: [u8; 16], step: u8) -> [u8; N] {
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = pattern[i % 16] + step * (i / 16) as u8;
        i += 1;
    }
    out
}

/// The first `N` bytes of `bytes`.
#[inline(always)]
fn load<const N: usize>(bytes: &[u8]) -> [u8; N] {
    bytes[..N].try_into().unwrap()
}

/// The first `K` bytes of `bytes`, zero padded to `N` (`movd`/`movq`).
#[inline(always)]
fn load_low<const K: usize, const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    out[..K].copy_from_slice(&bytes[..K]);
    out
}

/// `a` then `b`.
#[inline(always)]
fn concat<const N: usize, const M: usize>(a: [u8; N], b: [u8; N]) -> [u8; M] {
    let mut out = [0; M];
    out[..N].copy_from_slice(&a);
    out[N..].copy_from_slice(&b);
    out
}

/// The `k`th 16 bytes of `v`.
#[inline(always)]
fn quarter<const N: usize>(v: [u8; N], k: usize) -> [u8; 16] {
    load(&v[16 * k..])
}

/// For the byte shuffles, which take signed lanes. A plain loop: `map`
/// is not inlined at 32 lanes and above, and then costs a call per op.
#[inline(always)]
fn signed<const N: usize>(v: [u8; N]) -> [i8; N] {
    let mut out = [0; N];
    for i in 0..N {
        out[i] = v[i] as i8;
    }
    out
}

#[inline(always)]
fn unsigned<const N: usize>(v: [i8; N]) -> [u8; N] {
    let mut out = [0; N];
    for i in 0..N {
        out[i] = v[i] as u8;
    }
    out
}

/// Byte pairs as words and back, for the word shifts.
#[inline(always)]
fn words<const N: usize, const M: usize>(v: [u8; N]) -> [u16; M] {
    let mut out = [0; M];
    store_pairs(&v, &mut out);
    out
}

#[inline(always)]
fn bytes<const N: usize, const M: usize>(v: [u16; M]) -> [u8; N] {
    let mut out = [0; N];
    for (pair, word) in out.as_chunks_mut::<2>().0.iter_mut().zip(v) {
        *pair = word.to_le_bytes();
    }
    out
}

/// Little-endian byte pairs as words: `out[i] = v[2i] | v[2i + 1] << 8`.
#[inline(always)]
fn store_pairs(v: &[u8], out: &mut [u16]) {
    for (word, pair) in out.iter_mut().zip(v.as_chunks::<2>().0) {
        *word = u16::from_le_bytes(*pair);
    }
}

/// 16 lanes on SSE2 alone, every x86_64's baseline: no `pshufb`, so the
/// spreads are byte unpacks and the lookups compares or loads.
#[derive(Clone, Copy)]
pub struct Sse2Bytes(Sse2);

/// Nonzero entries up to which [`Sse2Bytes::lookup`] compares each lane's
/// index against each entry's rather than loading a lane at a time.
const COMPARES: usize = 16;

impl Sse2Bytes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Sse2::from_features(features).map(Self)
    }

    /// Each of the low 8 bytes twice.
    #[inline(always)]
    fn twice(self, x: [u8; 16]) -> [u8; 16] {
        self.0.unpacklo_u8x16(x, x)
    }

    /// Lanes where every bit of `bits` is set: `0xFF`, else `0`.
    #[inline(always)]
    fn has(self, v: [u8; 16], bits: [u8; 16]) -> [u8; 16] {
        let s = self.0;
        s.cmpeq_u8x16(s.and_u8x16(v, bits), bits)
    }

    /// Two-bit fields out of 4 bytes spread as [`SPREAD_2`].
    #[inline(always)]
    fn fields_2(self, spread: [u8; 16]) -> [u8; 16] {
        let s = self.0;
        s.or_u8x16(
            s.and_u8x16(self.has(spread, LOW_2), [1; 16]),
            s.and_u8x16(self.has(spread, HIGH_2), [2; 16]),
        )
    }

    /// One-bit fields out of 2 bytes spread as [`SPREAD_1`].
    #[inline(always)]
    fn fields_1(self, spread: [u8; 16]) -> [u8; 16] {
        self.0.and_u8x16(self.has(spread, BIT_1), [1; 16])
    }

    /// Low and high nibbles of 8 bytes, interleaved: 16 four-bit fields.
    #[inline(always)]
    fn nibbles(self, x: [u8; 16]) -> ([u8; 16], [u8; 16]) {
        let s = self.0;
        let (low, high) = (s.and_u8x16(x, [0x0F; 16]), s.shr_u8x16::<4>(x));
        (s.unpacklo_u8x16(low, high), s.unpackhi_u8x16(low, high))
    }
}

/// [`Sse2Bytes`]' table.
pub struct Picks {
    /// Up to [`COMPARES`] nonzero entries, each as its index and value in
    /// every lane; the rest are zero.
    few: [([u8; 16], [u8; 16]); COMPARES],
    count: usize,
    /// With more nonzero: every entry, loaded a lane at a time.
    many: Option<[u8; 256]>,
}

impl Bytes for Sse2Bytes {
    const LEN: usize = 16;
    type V = [u8; 16];
    type Table = Picks;

    #[inline(always)]
    fn table(self, entries: &[u8; 256], len: usize) -> Picks {
        let entries = &entries[..len];
        let mut table = Picks {
            few: [([0; 16], [0; 16]); COMPARES],
            count: 0,
            many: None,
        };
        if entries.iter().filter(|&&e| e != 0).count() > COMPARES {
            let mut all = [0; 256];
            all[..len].copy_from_slice(entries);
            table.many = Some(all);
            return table;
        }
        for (i, &entry) in entries.iter().enumerate().filter(|(_, e)| **e != 0) {
            table.few[table.count] = ([i as u8; 16], [entry; 16]);
            table.count += 1;
        }
        table
    }

    #[inline(always)]
    fn unpack<const BITS: usize>(self, packed: &[u8]) -> [u8; 16] {
        match BITS {
            8 => load(packed),
            4 => self.nibbles(load_low::<8, 16>(packed)).0,
            2 => self.fields_2(self.twice(self.twice(load_low::<4, 16>(packed)))),
            _ => self.fields_1(self.twice(self.twice(self.twice(load_low::<2, 16>(packed))))),
        }
    }

    #[inline(always)]
    fn lookup(self, table: &Picks, index: [u8; 16]) -> [u8; 16] {
        if let Some(entries) = &table.many {
            return std::array::from_fn(|i| entries[usize::from(index[i])]);
        }
        let s = self.0;
        table.few[..table.count]
            .iter()
            .fold([0; 16], |out, &(at, entry)| {
                s.or_u8x16(out, s.and_u8x16(s.cmpeq_u8x16(index, at), entry))
            })
    }

    #[inline(always)]
    fn store_words(self, lo: [u8; 16], hi: [u8; 16], out: &mut [u16]) {
        let s = self.0;
        store_pairs(&s.unpacklo_u8x16(lo, hi), &mut out[..8]);
        store_pairs(&s.unpackhi_u8x16(lo, hi), &mut out[8..16]);
    }

    #[inline(always)]
    fn store_bytes(self, v: [u8; 16], out: &mut [u8]) {
        out[..16].copy_from_slice(&v);
    }

    #[inline(always)]
    fn bit_mask(self, v: [u8; 16], bit: u32) -> u64 {
        // The bit to the top of each byte, where `pmovmskb` reads it.
        let s = self.0;
        let up: [u8; 16] = bytes(s.sll_u16x8(words(v), 7 - bit));
        u64::from(s.movemask_i8x16(signed(up)))
    }
}

/// 16 lanes on SSSE3: [`Sse2Bytes`] with `pshufb` spreads and lookups.
#[derive(Clone, Copy)]
pub struct Ssse3Bytes {
    sse2: Sse2Bytes,
    ssse3: Ssse3,
}

impl Ssse3Bytes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            sse2: Sse2Bytes::new(features)?,
            ssse3: Ssse3::from_features(features)?,
        })
    }

    /// The token that proves this tier's features to a `*_fn!` kernel.
    #[inline(always)]
    pub fn proof(self) -> Ssse3 {
        self.ssse3
    }

    #[inline(always)]
    fn shuffle(self, table: [u8; 16], index: [u8; 16]) -> [u8; 16] {
        unsigned(self.ssse3.shuffle_i8x16(signed(table), signed(index)))
    }
}

/// 16-entry chunks, as many as the table needs.
pub struct Chunks<V> {
    chunks: [V; 16],
    count: usize,
}

impl Bytes for Ssse3Bytes {
    const LEN: usize = 16;
    type V = [u8; 16];
    type Table = Chunks<[u8; 16]>;

    #[inline(always)]
    fn table(self, entries: &[u8; 256], len: usize) -> Self::Table {
        let count = chunks(len, 16);
        let mut chunks = [[0; 16]; 16];
        for (c, chunk) in chunks.iter_mut().enumerate().take(count) {
            *chunk = load(&entries[c * 16..]);
        }
        Chunks { chunks, count }
    }

    #[inline(always)]
    fn unpack<const BITS: usize>(self, packed: &[u8]) -> [u8; 16] {
        match BITS {
            2 => self
                .sse2
                .fields_2(self.shuffle(load_low::<4, 16>(packed), SPREAD_2)),
            1 => self
                .sse2
                .fields_1(self.shuffle(load_low::<2, 16>(packed), SPREAD_1)),
            _ => self.sse2.unpack::<BITS>(packed),
        }
    }

    #[inline(always)]
    fn lookup(self, table: &Self::Table, index: [u8; 16]) -> [u8; 16] {
        if table.count == 1 {
            return self.shuffle(table.chunks[0], index);
        }
        let s = self.sse2.0;
        let mut out = [0; 16];
        let mut rest = index;
        for chunk in &table.chunks[..table.count] {
            out = s.or_u8x16(out, self.shuffle(*chunk, s.adds_u8x16(rest, [0x70; 16])));
            rest = s.sub_u8x16(rest, [16; 16]);
        }
        out
    }

    #[inline(always)]
    fn store_words(self, lo: [u8; 16], hi: [u8; 16], out: &mut [u16]) {
        self.sse2.store_words(lo, hi, out);
    }

    #[inline(always)]
    fn store_bytes(self, v: [u8; 16], out: &mut [u8]) {
        self.sse2.store_bytes(v, out);
    }

    #[inline(always)]
    fn bit_mask(self, v: [u8; 16], bit: u32) -> u64 {
        self.sse2.bit_mask(v, bit)
    }
}

/// 32 lanes on AVX2: two 16-lane halves, each shuffled on its own.
#[derive(Clone, Copy)]
pub struct Avx2Bytes {
    ssse3: Ssse3Bytes,
    avx2: Avx2,
}

impl Avx2Bytes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            ssse3: Ssse3Bytes::new(features)?,
            avx2: Avx2::from_features(features)?,
        })
    }

    /// The token that proves this tier's features to a `*_fn!` kernel.
    #[inline(always)]
    pub fn proof(self) -> Avx2 {
        self.avx2
    }

    #[inline(always)]
    fn shuffle(self, table: [u8; 32], index: [u8; 32]) -> [u8; 32] {
        unsigned(self.avx2.shuffle_i8x32(signed(table), signed(index)))
    }

    #[inline(always)]
    fn has(self, v: [u8; 32], bits: [u8; 32]) -> [u8; 32] {
        let a = self.avx2;
        a.cmpeq_u8x32(a.and_u8x32(v, bits), bits)
    }
}

const SPREAD_2_256: [u8; 32] = lanes(SPREAD_2, 4);
const SPREAD_1_256: [u8; 32] = lanes(SPREAD_1, 2);
const LOW_2_256: [u8; 32] = lanes(LOW_2, 0);
const HIGH_2_256: [u8; 32] = lanes(HIGH_2, 0);
const BIT_1_256: [u8; 32] = lanes(BIT_1, 0);

impl Bytes for Avx2Bytes {
    const LEN: usize = 32;
    type V = [u8; 32];
    type Table = Chunks<[u8; 32]>;

    #[inline(always)]
    fn table(self, entries: &[u8; 256], len: usize) -> Self::Table {
        let count = chunks(len, 16);
        let mut chunks = [[0; 32]; 16];
        for (c, chunk) in chunks.iter_mut().enumerate().take(count) {
            *chunk = self.avx2.broadcast_u8x16_to_x32(load(&entries[c * 16..]));
        }
        Chunks { chunks, count }
    }

    #[inline(always)]
    fn unpack<const BITS: usize>(self, packed: &[u8]) -> [u8; 32] {
        let a = self.avx2;
        match BITS {
            8 => load(packed),
            4 => {
                let (low, high) = self.ssse3.sse2.nibbles(load(packed));
                concat(low, high)
            }
            2 => {
                // Each 128-bit lane shuffles its own copy of the 8 bytes.
                let x = a.broadcast_u8x16_to_x32(load_low::<8, 16>(packed));
                let spread = self.shuffle(x, SPREAD_2_256);
                a.or_u8x32(
                    a.and_u8x32(self.has(spread, LOW_2_256), [1; 32]),
                    a.and_u8x32(self.has(spread, HIGH_2_256), [2; 32]),
                )
            }
            _ => {
                let x = a.broadcast_u8x16_to_x32(load_low::<4, 16>(packed));
                let spread = self.shuffle(x, SPREAD_1_256);
                a.and_u8x32(self.has(spread, BIT_1_256), [1; 32])
            }
        }
    }

    #[inline(always)]
    fn lookup(self, table: &Self::Table, index: [u8; 32]) -> [u8; 32] {
        if table.count == 1 {
            return self.shuffle(table.chunks[0], index);
        }
        let a = self.avx2;
        let mut out = [0; 32];
        let mut rest = index;
        for chunk in &table.chunks[..table.count] {
            out = a.or_u8x32(out, self.shuffle(*chunk, a.adds_u8x32(rest, [0x70; 32])));
            rest = a.sub_u8x32(rest, [16; 32]);
        }
        out
    }

    #[inline(always)]
    fn store_words(self, lo: [u8; 32], hi: [u8; 32], out: &mut [u16]) {
        // Interleaving works within halves: put the halves back in order.
        let a = self.avx2;
        let (low, high) = (a.unpacklo_u8x32(lo, hi), a.unpackhi_u8x32(lo, hi));
        let first: [u8; 32] = concat(quarter(low, 0), quarter(high, 0));
        let second: [u8; 32] = concat(quarter(low, 1), quarter(high, 1));
        store_pairs(&first, &mut out[..16]);
        store_pairs(&second, &mut out[16..32]);
    }

    #[inline(always)]
    fn store_bytes(self, v: [u8; 32], out: &mut [u8]) {
        out[..32].copy_from_slice(&v);
    }

    #[inline(always)]
    fn bit_mask(self, v: [u8; 32], bit: u32) -> u64 {
        let a = self.avx2;
        let up: [u8; 32] = bytes(a.sll_u16x16(words(v), 7 - bit));
        u64::from(a.movemask_i8x32(signed(up)))
    }
}

/// 64 lanes on AVX-512 with VBMI: `vpermi2b` looks up 128 entries at once.
#[derive(Clone, Copy)]
pub struct VbmiBytes(V4Vbmi);

impl VbmiBytes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        V4Vbmi::from_features(features).map(Self)
    }

    /// The token that proves this tier's features to a `*_fn!` kernel.
    #[inline(always)]
    pub fn proof(self) -> V4Vbmi {
        self.0
    }

    /// Lanes of `mask` set: `value`, else `0`.
    #[inline(always)]
    fn where_set(self, mask: u64, value: u8) -> [u8; 64] {
        self.0
            .v4
            .avx512bw
            .mask_blend_u8x64(mask, [0; 64], [value; 64])
    }
}

/// Entries `0..128` and `128..256`, each as two 64-byte halves.
pub struct Wide {
    halves: [[[u8; 64]; 2]; 2],
    count: usize,
}

const SPREAD_2_512: [u8; 64] = lanes(SPREAD_2, 4);
const LOW_2_512: [u8; 64] = lanes(LOW_2, 0);
const HIGH_2_512: [u8; 64] = lanes(HIGH_2, 0);

/// `vpermt2b` indices: lane `i` from `a[i / 2]` when even, else `b[i / 2]`.
const ZIP: [u8; 64] = {
    let mut out = [0; 64];
    let mut i = 0;
    while i < 64 {
        out[i] = (i / 2) as u8 + if i % 2 == 0 { 0 } else { 64 };
        i += 1;
    }
    out
};

/// `vpermt2b` indices taking 64-bit words `words` of `a` (`0..8`) and `b`
/// (`8..16`).
const fn qwords(words: [u8; 8]) -> [u8; 64] {
    let mut out = [0; 64];
    let mut i = 0;
    while i < 64 {
        out[i] = words[i / 8] * 8 + (i % 8) as u8;
        i += 1;
    }
    out
}

/// Quarter `k` of `unpacklo` and of `unpackhi` are words `16k..16k+8` and
/// `16k+8..16k+16`: the first and second 64 words in order.
const FIRST: [u8; 64] = qwords([0, 1, 8, 9, 2, 3, 10, 11]);
const SECOND: [u8; 64] = qwords([4, 5, 12, 13, 6, 7, 14, 15]);

impl Bytes for VbmiBytes {
    const LEN: usize = 64;
    type V = [u8; 64];
    type Table = Wide;

    #[inline(always)]
    fn table(self, entries: &[u8; 256], len: usize) -> Wide {
        let at = |i: usize| load(&entries[i * 64..]);
        Wide {
            halves: [[at(0), at(1)], [at(2), at(3)]],
            count: chunks(len, 128),
        }
    }

    #[inline(always)]
    fn unpack<const BITS: usize>(self, packed: &[u8]) -> [u8; 64] {
        let (avx2, bw, vbmi) = (self.0.v4.v3.avx2, self.0.v4.avx512bw, self.0.vbmi);
        match BITS {
            8 => load(packed),
            4 => {
                let x: [u8; 32] = load(packed);
                let low = avx2.and_u8x32(x, [0x0F; 32]);
                let high = avx2.shr_u8x32::<4>(x);
                vbmi.permutex2var_u8x64(concat(low, [0; 32]), ZIP, concat(high, [0; 32]))
            }
            2 => {
                let x = self.0.v4.avx512f.broadcast_u8x16_to_x64(load(packed));
                let spread = unsigned(bw.shuffle_i8x64(signed(x), signed(SPREAD_2_512)));
                let low = bw.test_u8x64_mask(spread, LOW_2_512);
                let high = bw.test_u8x64_mask(spread, HIGH_2_512);
                self.0
                    .v4
                    .avx512f
                    .or_u8x64(self.where_set(low, 1), self.where_set(high, 2))
            }
            // 64 one-bit fields are a mask already.
            _ => self.where_set(u64::from_le_bytes(load(packed)), 1),
        }
    }

    #[inline(always)]
    fn lookup(self, table: &Wide, index: [u8; 64]) -> [u8; 64] {
        let vbmi = self.0.vbmi;
        let [[a, b], [c, d]] = table.halves;
        let low = vbmi.permutex2var_u8x64(a, index, b);
        if table.count == 1 {
            return low;
        }
        let high = vbmi.permutex2var_u8x64(c, index, d);
        let bw = self.0.v4.avx512bw;
        bw.mask_blend_u8x64(bw.test_u8x64_mask(index, [0x80; 64]), low, high)
    }

    #[inline(always)]
    fn store_words(self, lo: [u8; 64], hi: [u8; 64], out: &mut [u16]) {
        let (bw, vbmi) = (self.0.v4.avx512bw, self.0.vbmi);
        let (a, b) = (bw.unpacklo_u8x64(lo, hi), bw.unpackhi_u8x64(lo, hi));
        store_pairs(&vbmi.permutex2var_u8x64(a, FIRST, b), &mut out[..32]);
        store_pairs(&vbmi.permutex2var_u8x64(a, SECOND, b), &mut out[32..64]);
    }

    #[inline(always)]
    fn store_bytes(self, v: [u8; 64], out: &mut [u8]) {
        out[..64].copy_from_slice(&v);
    }

    #[inline(always)]
    fn bit_mask(self, v: [u8; 64], bit: u32) -> u64 {
        self.0.v4.avx512bw.test_u8x64_mask(v, [1 << bit; 64])
    }
}
