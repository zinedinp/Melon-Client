//! A 16³ light layer's packed nibbles, unpacked many at a time: each byte
//! holds two 4-bit levels (even index low, odd high), the same order
//! [`crate::world::chunk::LightSection::get`] reads. Written once against
//! [`Bytes`]

use crate::math::bytes::Bytes;
use crate::math::simd::{self, ByteTier};
use crate::world::chunk::LIGHT_NIBBLES;
use crate::world::section::BLOCKS_PER_SECTION;

/// Packed nibbles (`LIGHT_NIBBLES` bytes) to one level per block.
pub fn unpack(packed: &[u8; LIGHT_NIBBLES], out: &mut [u8; BLOCKS_PER_SECTION]) {
    unpack_on(simd::byte_tier(), packed, out);
}

/// [`unpack`] on `tier`, for the tests to try each.
pub fn unpack_on(tier: ByteTier, packed: &[u8; LIGHT_NIBBLES], out: &mut [u8; BLOCKS_PER_SECTION]) {
    match tier {
        #[cfg(target_arch = "x86_64")]
        ByteTier::Avx512Vbmi(b) => super::x86::nibble_decode::unpack_vbmi(b, packed, out),
        #[cfg(target_arch = "x86_64")]
        ByteTier::Avx2(b) => super::x86::nibble_decode::unpack_avx2(b, packed, out),
        #[cfg(target_arch = "x86_64")]
        ByteTier::Ssse3(b) => super::x86::nibble_decode::unpack_ssse3(b, packed, out),
        #[cfg(target_arch = "x86_64")]
        ByteTier::Sse2(b) => super::x86::nibble_decode::unpack_sse2(b, packed, out),
        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        ByteTier::Neon(b) => unpack_with(b, packed, out),
        ByteTier::Scalar => unpack_scalar(packed, out),
    }
}

fn unpack_scalar(packed: &[u8; LIGHT_NIBBLES], out: &mut [u8; BLOCKS_PER_SECTION]) {
    for (i, slot) in out.iter_mut().enumerate() {
        let byte = packed[i >> 1];
        *slot = if i & 1 == 0 { byte & 15 } else { byte >> 4 };
    }
}

/// [`unpack`]'s loop: a register of 4-bit fields a step.
#[inline(always)]
pub(super) fn unpack_with<B: Bytes>(
    b: B,
    packed: &[u8; LIGHT_NIBBLES],
    out: &mut [u8; BLOCKS_PER_SECTION],
) {
    let step = B::LEN / 2;
    for (bytes, out) in packed.chunks_exact(step).zip(out.chunks_exact_mut(B::LEN)) {
        b.store_bytes(b.unpack::<4>(bytes), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn every_tier_unpacks_as_the_scalar_code() {
        let mut noise = Noise(0x2545_F491_4F6C_DD1D);
        for _ in 0..8 {
            let mut packed = [0u8; LIGHT_NIBBLES];
            for byte in &mut packed {
                *byte = noise.next() as u8;
            }
            let mut expected = [0u8; BLOCKS_PER_SECTION];
            unpack_on(ByteTier::Scalar, &packed, &mut expected);
            for (i, &level) in expected.iter().enumerate() {
                let byte = packed[i >> 1];
                let want = if i & 1 == 0 { byte & 15 } else { byte >> 4 };
                assert_eq!(level, want);
            }
            for tier in ByteTier::available() {
                let mut out = [0xAAu8; BLOCKS_PER_SECTION];
                unpack_on(tier, &packed, &mut out);
                assert!(out == expected, "{}", tier.name());
            }
        }
    }
}
