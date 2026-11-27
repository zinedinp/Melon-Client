//! [`crate::section_decode`]'s loops under SSE2, SSSE3, AVX2 and
//! AVX-512 VBMI: one body each, the whole loop inside, so every byte op
//! inlines to vector instructions.

use miraculix::x86::bundles::V4Vbmi;
use miraculix::x86::ops::avx::avx2::Avx2;
use miraculix::x86::ops::sse::ssse3::Ssse3;

use melon_core::math::x86::bytes::{Avx2Bytes, Sse2Bytes, Ssse3Bytes, VbmiBytes};
use crate::section::BLOCKS_PER_SECTION;
use crate::section_decode::{ROWS, decode_with, flag_rows_with};

// SSE2 is every x86_64's baseline, so its bodies need no `target_feature`.
// Out of line like the others: inlined, the loop made the dispatch around
// it 5 µs slower a chunk on every tier (`section_visibility`).

#[inline(never)]
pub fn decode_sse2(
    b: Sse2Bytes,
    bits: u8,
    bytes: &[u8],
    palette: &[u16],
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    decode_with(b, bits, bytes, palette, out)
}

#[inline(never)]
pub fn flag_rows_sse2(
    b: Sse2Bytes,
    bits: u8,
    bytes: &[u8],
    flags: &[u8; 256],
    len: usize,
    out: &mut [[u16; ROWS]],
) {
    flag_rows_with(b, bits, bytes, flags, len, out)
}

pub fn decode_ssse3(
    b: Ssse3Bytes,
    bits: u8,
    bytes: &[u8],
    palette: &[u16],
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    decode_ssse3_body(b.proof(), b, bits, bytes, palette, out);
}

miraculix::ssse3_fn! {
    fn decode_ssse3_body(
        _proof: Ssse3,
        b: Ssse3Bytes,
        bits: u8,
        bytes: &[u8],
        palette: &[u16],
        out: &mut [u16; BLOCKS_PER_SECTION],
    ) {
        decode_with(b, bits, bytes, palette, out)
    }
}

pub fn flag_rows_ssse3(
    b: Ssse3Bytes,
    bits: u8,
    bytes: &[u8],
    flags: &[u8; 256],
    len: usize,
    out: &mut [[u16; ROWS]],
) {
    flag_rows_ssse3_body(b.proof(), b, bits, bytes, flags, len, out);
}

miraculix::ssse3_fn! {
    fn flag_rows_ssse3_body(
        _proof: Ssse3,
        b: Ssse3Bytes,
        bits: u8,
        bytes: &[u8],
        flags: &[u8; 256],
        len: usize,
        out: &mut [[u16; ROWS]],
    ) {
        flag_rows_with(b, bits, bytes, flags, len, out)
    }
}

pub fn decode_avx2(
    b: Avx2Bytes,
    bits: u8,
    bytes: &[u8],
    palette: &[u16],
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    decode_avx2_body(b.proof(), b, bits, bytes, palette, out);
}

miraculix::avx2_fn! {
    fn decode_avx2_body(
        _proof: Avx2,
        b: Avx2Bytes,
        bits: u8,
        bytes: &[u8],
        palette: &[u16],
        out: &mut [u16; BLOCKS_PER_SECTION],
    ) {
        decode_with(b, bits, bytes, palette, out)
    }
}

pub fn flag_rows_avx2(
    b: Avx2Bytes,
    bits: u8,
    bytes: &[u8],
    flags: &[u8; 256],
    len: usize,
    out: &mut [[u16; ROWS]],
) {
    flag_rows_avx2_body(b.proof(), b, bits, bytes, flags, len, out);
}

miraculix::avx2_fn! {
    fn flag_rows_avx2_body(
        _proof: Avx2,
        b: Avx2Bytes,
        bits: u8,
        bytes: &[u8],
        flags: &[u8; 256],
        len: usize,
        out: &mut [[u16; ROWS]],
    ) {
        flag_rows_with(b, bits, bytes, flags, len, out)
    }
}

pub fn decode_vbmi(
    b: VbmiBytes,
    bits: u8,
    bytes: &[u8],
    palette: &[u16],
    out: &mut [u16; BLOCKS_PER_SECTION],
) {
    decode_vbmi_body(b.proof(), b, bits, bytes, palette, out);
}

miraculix::avx512_v4_vbmi_fn! {
    fn decode_vbmi_body(
        _proof: V4Vbmi,
        b: VbmiBytes,
        bits: u8,
        bytes: &[u8],
        palette: &[u16],
        out: &mut [u16; BLOCKS_PER_SECTION],
    ) {
        decode_with(b, bits, bytes, palette, out)
    }
}

pub fn flag_rows_vbmi(
    b: VbmiBytes,
    bits: u8,
    bytes: &[u8],
    flags: &[u8; 256],
    len: usize,
    out: &mut [[u16; ROWS]],
) {
    flag_rows_vbmi_body(b.proof(), b, bits, bytes, flags, len, out);
}

miraculix::avx512_v4_vbmi_fn! {
    fn flag_rows_vbmi_body(
        _proof: V4Vbmi,
        b: VbmiBytes,
        bits: u8,
        bytes: &[u8],
        flags: &[u8; 256],
        len: usize,
        out: &mut [[u16; ROWS]],
    ) {
        flag_rows_with(b, bits, bytes, flags, len, out)
    }
}
