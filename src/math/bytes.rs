//! What a byte kernel needs of a CPU, as one trait (each architecture's in
//! its folder beside this file, `x86/bytes.rs`, `aarch64/bytes.rs`):
//! unpacking narrow bit fields to bytes, looking bytes up in a table of up
//! to 256, and turning bytes back into words or bit masks. Like
//! [`super::lanes::Lanes`], every method is `#[inline(always)]`, so inside
//! a kernel's `target_feature` body a whole loop stays in vector registers
//! from the packed load to the store.
//!
//! Lanes are in memory order: lane `i` is the `i`-th byte loaded, the
//! `i`-th word stored, bit `i` of a mask, whatever the tier's registers
//! do inside.

/// [`Self::LEN`] byte lanes. Implemented only by tiers built from
/// `miraculix` tokens, so holding one proves the CPU has the instructions.
pub trait Bytes: Copy {
    /// Lanes per step: 16, 32 or 64.
    const LEN: usize;
    /// [`Self::LEN`] bytes.
    type V: Copy;
    /// A table of up to 256 bytes, laid out for [`Self::lookup`].
    type Table;

    /// The first `len` of `entries` (the rest read as zero) as a table.
    fn table(self, entries: &[u8; 256], len: usize) -> Self::Table;
    /// `BITS`-bit fields (1, 2, 4 or 8), lowest bits first, one a lane:
    /// `packed` is `LEN * BITS / 8` bytes.
    fn unpack<const BITS: usize>(self, packed: &[u8]) -> Self::V;
    /// Each lane's entry. A lane may only hold an index below the table's
    /// `len`.
    fn lookup(self, table: &Self::Table, index: Self::V) -> Self::V;
    /// `out[i] = lo[i] | hi[i] << 8`, `out` being `LEN` words.
    fn store_words(self, lo: Self::V, hi: Self::V, out: &mut [u16]);
    /// `out[i] = v[i]`, `out` being `LEN` bytes.
    fn store_bytes(self, v: Self::V, out: &mut [u8]);
    /// Bit `i` set where lane `i` has bit `bit` (`0..8`) set.
    fn bit_mask(self, v: Self::V, bit: u32) -> u64;
}

/// Table chunks needed for `len` entries, `per` to a chunk.
#[inline(always)]
pub fn chunks(len: usize, per: usize) -> usize {
    len.div_ceil(per).max(1)
}

/// Each byte four times: lane `i` reads byte `i / 4` (two-bit fields).
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub(super) const SPREAD_2: [u8; 16] = [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3];
/// Each byte eight times (one-bit fields).
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub(super) const SPREAD_1: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1];
/// A two-bit field's low and high bit, lane `i`'s at `2 * (i % 4)`.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub(super) const LOW_2: [u8; 16] = [1, 4, 16, 64, 1, 4, 16, 64, 1, 4, 16, 64, 1, 4, 16, 64];
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub(super) const HIGH_2: [u8; 16] = [2, 8, 32, 128, 2, 8, 32, 128, 2, 8, 32, 128, 2, 8, 32, 128];
/// Lane `i`'s bit, `i % 8`.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub(super) const BIT_1: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];

/// The first `N` bytes of `bytes`.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
pub(super) fn load<const N: usize>(bytes: &[u8]) -> [u8; N] {
    bytes[..N].try_into().unwrap()
}

/// The first `K` bytes of `bytes`, zero padded to `N` (`movd`/`movq`).
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
pub(super) fn load_low<const K: usize, const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    out[..K].copy_from_slice(&bytes[..K]);
    out
}
