//! Vanilla `Mth` and `SectionPos` helpers.

pub mod angle;
pub mod bytes;
pub mod euler;
pub mod lanes;
pub mod radix;
pub mod simd;
pub mod sin_cos;
pub mod slices;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "x86_64")]
pub mod x86;

pub use std::f32::consts::FRAC_1_SQRT_2;
pub use std::f32::consts::SQRT_2;

pub use angle::{DEG_TO_RAD, RAD_TO_DEG, cos, sin, sin_cos_deg as sin_cos_degrees, wrap_degrees};

/// Vanilla's packed `0xAARRGGBB` colours (`ARGB`, the ints the decomp
/// spells colours as) as `0..1` channels, red first.
pub const fn argb(color: u32) -> [f32; 4] {
    const fn channel(color: u32, shift: u32) -> f32 {
        ((color >> shift) & 0xFF) as f32 / 255.0
    }
    [
        channel(color, 16),
        channel(color, 8),
        channel(color, 0),
        channel(color, 24),
    ]
}

/// Blocks along each side of a section (`SectionPos.SECTION_SIZE`).
pub const SECTION_SIZE: i32 = 16;

/// `SectionPos.blockToSectionCoord`.
#[inline]
pub const fn section_coord(block: i32) -> i32 {
    block >> 4
}

/// `SectionPos.sectionRelative`.
#[inline]
pub const fn section_relative(block: i32) -> i32 {
    block & 15
}

/// `SectionPos.sectionToBlockCoord`: a section's lowest block coordinate.
#[inline]
pub const fn section_to_block(section: i32) -> i32 {
    section << 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_unpacks_vanillas_colours() {
        assert_eq!(argb(0xFF00_00FF), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(argb(0xFFFF_0000), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(argb(0x8000_0000)[3], 128.0 / 255.0);
    }

    #[test]
    fn wraps_and_splits_coordinates() {
        assert_eq!(section_coord(-1), -1);
        assert_eq!(section_relative(-1), 15);
        assert_eq!(section_coord(31), 1);
        assert_eq!(section_to_block(-2), -32);
    }
}

/// Hashes chunk coordinates with one rotate, xor and multiply per `i32`
#[derive(Default, Clone, Copy)]
pub struct ChunkHasher(u64);

impl ChunkHasher {
    const K: u64 = 0xf135_7aea_2e62_a9c5;

    #[inline]
    fn add(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(Self::K);
    }
}

impl std::hash::Hasher for ChunkHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.add(u64::from(byte));
        }
    }

    #[inline]
    fn write_i32(&mut self, n: i32) {
        self.add(u64::from(n as u32));
    }
}

pub type ChunkHash = std::hash::BuildHasherDefault<ChunkHasher>;
/// A map keyed by chunk `(x, z)`.
pub type ChunkMap<V> = std::collections::HashMap<(i32, i32), V, ChunkHash>;
/// A set of chunk `(x, z)`.
pub type ChunkSet = std::collections::HashSet<(i32, i32), ChunkHash>;
/// A map keyed by section `(x, y, z)`, `y` counted from the world's bottom.
pub type SectionMap<V> = std::collections::HashMap<(i32, i32, i32), V, ChunkHash>;
/// A set of sections `(x, y, z)`.
pub type SectionSet = std::collections::HashSet<(i32, i32, i32), ChunkHash>;
