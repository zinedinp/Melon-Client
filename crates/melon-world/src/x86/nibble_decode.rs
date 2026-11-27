//! [`crate::nibble_decode`]'s loop under SSE2, SSSE3, AVX2 and
//! AVX-512 VBMI: one body each.

use miraculix::x86::bundles::V4Vbmi;
use miraculix::x86::ops::avx::avx2::Avx2;
use miraculix::x86::ops::sse::ssse3::Ssse3;

use crate::chunk::LIGHT_NIBBLES;
use crate::nibble_decode::unpack_with;
use crate::section::BLOCKS_PER_SECTION;
use melon_core::math::x86::bytes::{Avx2Bytes, Sse2Bytes, Ssse3Bytes, VbmiBytes};

/// The x86_64 baseline needs no `target_feature`; out of line as the others.
#[inline(never)]
pub fn unpack_sse2(b: Sse2Bytes, packed: &[u8; LIGHT_NIBBLES], out: &mut [u8; BLOCKS_PER_SECTION]) {
    unpack_with(b, packed, out)
}

pub fn unpack_ssse3(
    b: Ssse3Bytes,
    packed: &[u8; LIGHT_NIBBLES],
    out: &mut [u8; BLOCKS_PER_SECTION],
) {
    unpack_ssse3_body(b.proof(), b, packed, out);
}

miraculix::ssse3_fn! {
    fn unpack_ssse3_body(
        _proof: Ssse3,
        b: Ssse3Bytes,
        packed: &[u8; LIGHT_NIBBLES],
        out: &mut [u8; BLOCKS_PER_SECTION],
    ) {
        unpack_with(b, packed, out)
    }
}

pub fn unpack_avx2(b: Avx2Bytes, packed: &[u8; LIGHT_NIBBLES], out: &mut [u8; BLOCKS_PER_SECTION]) {
    unpack_avx2_body(b.proof(), b, packed, out);
}

miraculix::avx2_fn! {
    fn unpack_avx2_body(
        _proof: Avx2,
        b: Avx2Bytes,
        packed: &[u8; LIGHT_NIBBLES],
        out: &mut [u8; BLOCKS_PER_SECTION],
    ) {
        unpack_with(b, packed, out)
    }
}

pub fn unpack_vbmi(b: VbmiBytes, packed: &[u8; LIGHT_NIBBLES], out: &mut [u8; BLOCKS_PER_SECTION]) {
    unpack_vbmi_body(b.proof(), b, packed, out);
}

miraculix::avx512_v4_vbmi_fn! {
    fn unpack_vbmi_body(
        _proof: V4Vbmi,
        b: VbmiBytes,
        packed: &[u8; LIGHT_NIBBLES],
        out: &mut [u8; BLOCKS_PER_SECTION],
    ) {
        unpack_with(b, packed, out)
    }
}
