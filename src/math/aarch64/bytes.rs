//! [`Bytes`] on AArch64 Neon: 16 lanes, `tbl` over 64 entries at a time.
//! A table of 256 is four chunks of 64: the first `tbl` zeroes the lanes
//! past it, each `tbx` after fills only the lanes whose index, less the
//! chunk's start, falls inside its chunk, and leaves the others as they
//! were. Neon is every AArch64's, so the kernels need no `target_feature`
//! body. Every op is a `miraculix` token method: no `unsafe` here.

use miraculix::aarch64::FeatureSet;
use miraculix::aarch64::ops::neon::neon::Neon;

use crate::math::bytes::{BIT_1, Bytes, HIGH_2, LOW_2, SPREAD_1, SPREAD_2, chunks, load, load_low};

/// 16 lanes on Neon.
#[derive(Clone, Copy)]
pub struct NeonBytes {
    neon: Neon,
}

impl NeonBytes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            neon: Neon::from_features(features)?,
        })
    }
}

/// 64-entry chunks, as many as the table needs.
pub struct Chunks {
    chunks: [[u8; 64]; 4],
    count: usize,
    /// All entries in the first 16: one `tbl` of one register.
    small: bool,
}

impl Bytes for NeonBytes {
    const LEN: usize = 16;
    type V = [u8; 16];
    type Table = Chunks;

    #[inline(always)]
    fn table(self, entries: &[u8; 256], len: usize) -> Chunks {
        Chunks {
            chunks: std::array::from_fn(|c| load(&entries[c * 64..])),
            count: chunks(len, 64),
            small: len <= 16,
        }
    }

    #[inline(always)]
    fn unpack<const BITS: usize>(self, packed: &[u8]) -> [u8; 16] {
        let n = self.neon;
        match BITS {
            8 => load(packed),
            4 => {
                let x: [u8; 8] = load(packed);
                let (low, high) = (n.and_u8x8(x, [0x0F; 8]), n.srli_u8x8::<4>(x));
                let mut out = [0; 16];
                out[..8].copy_from_slice(&n.zip1_u8x8(low, high));
                out[8..].copy_from_slice(&n.zip2_u8x8(low, high));
                out
            }
            2 => {
                let spread = n.tbl_u8x16(load_low::<4, 16>(packed), SPREAD_2);
                let low = n.and_u8x16(n.test_u8x16(spread, LOW_2), [1; 16]);
                let high = n.and_u8x16(n.test_u8x16(spread, HIGH_2), [2; 16]);
                n.or_u8x16(low, high)
            }
            _ => {
                let spread = n.tbl_u8x16(load_low::<2, 16>(packed), SPREAD_1);
                n.and_u8x16(n.test_u8x16(spread, BIT_1), [1; 16])
            }
        }
    }

    #[inline(always)]
    fn lookup(self, table: &Chunks, index: [u8; 16]) -> [u8; 16] {
        let n = self.neon;
        if table.small {
            return n.tbl_u8x16(load(&table.chunks[0]), index);
        }
        let mut out = n.tbl4_u8x64(table.chunks[0], index);
        for (c, chunk) in table.chunks[..table.count].iter().enumerate().skip(1) {
            out = n.tbx4_u8x64(out, *chunk, n.sub_u8x16(index, [64 * c as u8; 16]));
        }
        out
    }

    #[inline(always)]
    fn store_words(self, lo: [u8; 16], hi: [u8; 16], out: &mut [u16]) {
        let n = self.neon;
        let (first, second) = (n.zip1_u8x16(lo, hi), n.zip2_u8x16(lo, hi));
        for (words, pairs) in out[..16]
            .as_chunks_mut::<8>()
            .0
            .iter_mut()
            .zip([first, second])
        {
            for (word, pair) in words.iter_mut().zip(pairs.as_chunks::<2>().0) {
                *word = u16::from_le_bytes(*pair);
            }
        }
    }

    #[inline(always)]
    fn store_bytes(self, v: [u8; 16], out: &mut [u8]) {
        out[..16].copy_from_slice(&v);
    }

    #[inline(always)]
    fn bit_mask(self, v: [u8; 16], bit: u32) -> u64 {
        let n = self.neon;
        let set = n.test_u8x16(v, [1 << bit; 16]);
        let mut signed = [0i8; 16];
        for (s, b) in signed.iter_mut().zip(set) {
            *s = b as i8;
        }
        u64::from(n.movemask_i8x16(signed))
    }
}
