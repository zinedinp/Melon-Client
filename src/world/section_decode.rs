//! A packed section's blocks, unpacked many at a time: the palette indices
//! (1, 2, 4 or 8 bits a block) to block states ([`decode`]), or to rows of
//! flag bits ([`flag_rows`], what the mesher and the visibility graph
//! read). Each step unpacks a register of indices, looks them up in the
//! palette (or the palette's flags) with byte shuffles and stores words or
//! bit masks, without leaving the vector registers; written once against
//! [`Bytes`], the `target_feature` bodies for x86_64 in `x86/` beside this
//! file. The scalar code is the reference every tier must match exactly.

use crate::math::bytes::Bytes;
use crate::math::simd::{self, ByteTier};
use crate::world::section::BLOCKS_PER_SECTION;

/// Rows of 16 blocks along x in a section, index `y * 16 + z`.
pub const ROWS: usize = 256;

/// Block states of a packed section, in index order: `words` holds block
/// `i`'s palette index at bit `i * bits` (`bits` 1, 2, 4 or 8).
pub fn decode(bits: u8, words: &[u64], palette: &[u16], out: &mut [u16; BLOCKS_PER_SECTION]) {
    decode_on(simd::byte_tier(), bits, words, palette, out);
}

/// [`decode`] on `tier`, for the tests and benchmarks to try each.
pub fn decode_on(
    tier: ByteTier,
    bits: u8,
    words: &[u64],
    palette: &[u16],
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    check(bits, words, palette.len());
    match tier {
        #[cfg(target_arch = "x86_64")]
        ByteTier::Avx512Vbmi(b) => {
            super::x86::section_decode::decode_vbmi(b, bits, as_bytes(words), palette, out)
        }
        #[cfg(target_arch = "x86_64")]
        ByteTier::Avx2(b) => {
            super::x86::section_decode::decode_avx2(b, bits, as_bytes(words), palette, out)
        }
        #[cfg(target_arch = "x86_64")]
        ByteTier::Ssse3(b) => {
            super::x86::section_decode::decode_ssse3(b, bits, as_bytes(words), palette, out)
        }
        #[cfg(target_arch = "x86_64")]
        ByteTier::Sse2(b) => {
            super::x86::section_decode::decode_sse2(b, bits, as_bytes(words), palette, out)
        }
        // Neon is every AArch64's baseline: no `target_feature` body needed.
        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        ByteTier::Neon(b) => decode_with(b, bits, as_bytes(words), palette, out),
        ByteTier::Scalar => decode_scalar(bits, words, palette, out),
    }
}

/// Rows of flag bits of a packed section: bit `x` of `out[k][row]` is bit
/// `k` of `flags[index]` for the block at `x` of `row`. `flags` holds an
/// entry for each of the `len` palette indices; `out` is at most 8 long.
pub fn flag_rows(bits: u8, words: &[u64], flags: &[u8; 256], len: usize, out: &mut [[u16; ROWS]]) {
    flag_rows_on(simd::byte_tier(), bits, words, flags, len, out);
}

/// [`flag_rows`] on `tier`.
pub fn flag_rows_on(
    tier: ByteTier,
    bits: u8,
    words: &[u64],
    flags: &[u8; 256],
    len: usize,
    out: &mut [[u16; ROWS]],
) {
    check(bits, words, len);
    assert!(out.len() <= 8);
    match tier {
        #[cfg(target_arch = "x86_64")]
        ByteTier::Avx512Vbmi(b) => {
            super::x86::section_decode::flag_rows_vbmi(b, bits, as_bytes(words), flags, len, out)
        }
        #[cfg(target_arch = "x86_64")]
        ByteTier::Avx2(b) => {
            super::x86::section_decode::flag_rows_avx2(b, bits, as_bytes(words), flags, len, out)
        }
        #[cfg(target_arch = "x86_64")]
        ByteTier::Ssse3(b) => {
            super::x86::section_decode::flag_rows_ssse3(b, bits, as_bytes(words), flags, len, out)
        }
        #[cfg(target_arch = "x86_64")]
        ByteTier::Sse2(b) => {
            super::x86::section_decode::flag_rows_sse2(b, bits, as_bytes(words), flags, len, out)
        }
        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        ByteTier::Neon(b) => flag_rows_with(b, bits, as_bytes(words), flags, len, out),
        ByteTier::Scalar => flag_rows_scalar(bits, words, flags, out),
    }
}

/// The packing the kernels rely on.
fn check(bits: u8, words: &[u64], len: usize) {
    assert!(matches!(bits, 1 | 2 | 4 | 8), "{bits} bits");
    assert_eq!(words.len(), 64 * usize::from(bits));
    assert!(
        (1..=1 << bits).contains(&len),
        "{len} entries at {bits} bits"
    );
}

/// The words as the bytes they are in memory: block `i`'s field at bit
/// `i * bits` of the whole, as on little-endian CPUs only.
#[cfg(any(
    target_arch = "x86_64",
    all(target_arch = "aarch64", target_endian = "little")
))]
fn as_bytes(words: &[u64]) -> &[u8] {
    // SAFETY: any `u64` is eight valid bytes; same allocation and lifetime.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), size_of_val(words)) }
}

/// The palette indices of `row`'s 16 blocks along x. A row is `16 * bits`
/// bits, never across two words.
#[inline]
pub fn row_indices(bits: u8, words: &[u64], row: usize) -> [u8; 16] {
    match bits {
        1 => row_in_word::<1>(words, row),
        2 => row_in_word::<2>(words, row),
        4 => row_in_word::<4>(words, row),
        _ => {
            let [lo, hi] = [words[row * 2], words[row * 2 + 1]];
            std::array::from_fn(|x| ((if x < 8 { lo } else { hi }) >> (x % 8 * 8) & 0xFF) as u8)
        }
    }
}

#[inline(always)]
fn row_in_word<const BITS: usize>(words: &[u64], row: usize) -> [u8; 16] {
    let offset = row * 16 * BITS;
    let word = words[offset >> 6] >> (offset & 63);
    std::array::from_fn(|x| (word >> (x * BITS) & ((1 << BITS) - 1)) as u8)
}

fn decode_scalar(bits: u8, words: &[u64], palette: &[u16], out: &mut [u16; BLOCKS_PER_SECTION]) {
    let mut table = [0u16; 256];
    table[..palette.len()].copy_from_slice(palette);
    for (row, out) in out.as_chunks_mut::<16>().0.iter_mut().enumerate() {
        for (slot, index) in out.iter_mut().zip(row_indices(bits, words, row)) {
            *slot = table[usize::from(index)];
        }
    }
}

fn flag_rows_scalar(bits: u8, words: &[u64], flags: &[u8; 256], out: &mut [[u16; ROWS]]) {
    match out {
        [masks] => flag_rows_one(bits, words, flags, masks),
        _ => flag_rows_many(bits, words, flags, out),
    }
}

/// One flag: each block's bit straight into the mask (measured faster
/// than gathering it for one, slower for two and more).
fn flag_rows_one(bits: u8, words: &[u64], flags: &[u8; 256], out: &mut [u16; ROWS]) {
    for (row, out) in out.iter_mut().enumerate() {
        *out = row_indices(bits, words, row)
            .into_iter()
            .enumerate()
            .fold(0, |mask, (x, index)| {
                mask | u16::from(flags[usize::from(index)] & 1) << x
            });
    }
}

/// Several flags: a row's 16 flag bytes as two words, each flag's bits
/// gathered out of them.
fn flag_rows_many(bits: u8, words: &[u64], flags: &[u8; 256], out: &mut [[u16; ROWS]]) {
    for row in 0..ROWS {
        let indices = row_indices(bits, words, row);
        let [lo, hi] = [0, 8].map(|start| {
            u64::from_le_bytes(std::array::from_fn(|x| {
                flags[usize::from(indices[start + x])]
            }))
        });
        for (k, masks) in out.iter_mut().enumerate() {
            masks[row] = u16::from(gather_bits(lo >> k)) | u16::from(gather_bits(hi >> k)) << 8;
        }
    }
}

/// The low bit of each of `bytes`' eight bytes, byte `i`'s as bit `i`: a
/// `movemask` in a multiply. Each byte's bit lands on its own bit of the
/// top byte (`8i + 8(7 - i) + i`), and no two products overlap, so
/// nothing carries.
#[inline(always)]
fn gather_bits(bytes: u64) -> u8 {
    ((bytes & 0x0101_0101_0101_0101).wrapping_mul(0x0102_0408_1020_4080) >> 56) as u8
}

/// [`decode`]'s loop: the palette's low and high bytes as two tables, then
/// a register of blocks a step, looked up in both and stored as words.
#[inline(always)]
pub(super) fn decode_with<B: Bytes>(
    b: B,
    bits: u8,
    bytes: &[u8],
    palette: &[u16],
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    let (mut lo, mut hi) = ([0u8; 256], [0u8; 256]);
    for (i, &id) in palette.iter().enumerate() {
        [lo[i], hi[i]] = id.to_le_bytes();
    }
    let tables = (b.table(&lo, palette.len()), b.table(&hi, palette.len()));
    match bits {
        1 => decode_steps::<B, 1>(b, bytes, &tables, out),
        2 => decode_steps::<B, 2>(b, bytes, &tables, out),
        4 => decode_steps::<B, 4>(b, bytes, &tables, out),
        _ => decode_steps::<B, 8>(b, bytes, &tables, out),
    }
}

#[inline(always)]
fn decode_steps<B: Bytes, const BITS: usize>(
    b: B,
    bytes: &[u8],
    (lo, hi): &(B::Table, B::Table),
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    let packed = B::LEN * BITS / 8;
    for (bytes, out) in bytes.chunks_exact(packed).zip(out.chunks_exact_mut(B::LEN)) {
        let index = b.unpack::<BITS>(bytes);
        b.store_words(b.lookup(lo, index), b.lookup(hi, index), out);
    }
}

/// [`flag_rows`]' loop: a register of blocks a step, their flags looked
/// up, one mask a flag bit, split into rows.
#[inline(always)]
pub(super) fn flag_rows_with<B: Bytes>(
    b: B,
    bits: u8,
    bytes: &[u8],
    flags: &[u8; 256],
    len: usize,
    out: &mut [[u16; ROWS]],
) {
    let table = b.table(flags, len);
    match bits {
        1 => flag_steps::<B, 1>(b, bytes, &table, out),
        2 => flag_steps::<B, 2>(b, bytes, &table, out),
        4 => flag_steps::<B, 4>(b, bytes, &table, out),
        _ => flag_steps::<B, 8>(b, bytes, &table, out),
    }
}

#[inline(always)]
fn flag_steps<B: Bytes, const BITS: usize>(
    b: B,
    bytes: &[u8],
    table: &B::Table,
    out: &mut [[u16; ROWS]],
) {
    let packed = B::LEN * BITS / 8;
    let rows = B::LEN / 16;
    for (step, bytes) in bytes.chunks_exact(packed).enumerate() {
        let flags = b.lookup(table, b.unpack::<BITS>(bytes));
        for (k, masks) in out.iter_mut().enumerate() {
            let mask = b.bit_mask(flags, k as u32);
            for (r, row) in masks[step * rows..][..rows].iter_mut().enumerate() {
                *row = (mask >> (16 * r)) as u16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small xorshift: the same blocks on every run.
    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// Random indices below `len`, packed at `bits`.
    fn packed(bits: u8, len: usize, noise: &mut Noise) -> Vec<u64> {
        let mut words = vec![0u64; 64 * usize::from(bits)];
        for i in 0..BLOCKS_PER_SECTION {
            let index = noise.next() % len as u64;
            let offset = i * usize::from(bits);
            words[offset >> 6] |= index << (offset & 63);
        }
        words
    }

    fn cases() -> Vec<(u8, usize)> {
        let mut cases = vec![(1, 1), (1, 2), (2, 3), (2, 4), (4, 5), (4, 16)];
        for len in [1, 16, 17, 32, 33, 64, 65, 100, 128, 129, 200, 255, 256] {
            cases.push((8, len));
        }
        cases
    }

    #[test]
    fn every_tier_decodes_as_the_scalar_code() {
        let mut noise = Noise(0x2545_F491_4F6C_DD1D);
        for (bits, len) in cases() {
            let words = packed(bits, len, &mut noise);
            let palette: Vec<u16> = (0..len).map(|_| noise.next() as u16).collect();
            let mut expected = [0u16; BLOCKS_PER_SECTION];
            decode_on(ByteTier::Scalar, bits, &words, &palette, &mut expected);
            for (i, block) in expected.iter().enumerate() {
                let offset = i * usize::from(bits);
                let index = words[offset >> 6] >> (offset & 63) & ((1 << bits) - 1);
                assert_eq!(*block, palette[index as usize]);
            }
            for tier in ByteTier::available() {
                let mut out = [0u16; BLOCKS_PER_SECTION];
                decode_on(tier, bits, &words, &palette, &mut out);
                assert!(
                    out == expected,
                    "{} at {bits} bits, {len} entries",
                    tier.name()
                );
            }
        }
    }

    #[test]
    fn every_tier_builds_the_scalar_flag_rows() {
        let mut noise = Noise(0x9E37_79B9_7F4A_7C15);
        for (bits, len) in cases() {
            let words = packed(bits, len, &mut noise);
            let mut flags = [0u8; 256];
            for flag in &mut flags[..len] {
                *flag = noise.next() as u8;
            }
            for count in [1, 3, 8] {
                let mut expected = vec![[0u16; ROWS]; count];
                flag_rows_on(ByteTier::Scalar, bits, &words, &flags, len, &mut expected);
                for tier in ByteTier::available() {
                    let mut out = vec![[0xAAAA; ROWS]; count];
                    flag_rows_on(tier, bits, &words, &flags, len, &mut out);
                    assert!(
                        out == expected,
                        "{} at {bits} bits, {len} entries, {count} flags",
                        tier.name()
                    );
                }
            }
        }
    }
}
