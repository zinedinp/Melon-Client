//! [`crate::math::sin_cos`]'s loop under AVX and AVX-512F: one
//! `target_feature` body each, the whole loop inside, so every lane op
//! inlines to vector instructions. (SSE2 is every x86_64's baseline and
//! needs none.)

use miraculix::x86::ops::avx::avx::Avx;
use miraculix::x86::ops::avx512::avx512f::Avx512f;

use super::lanes::{Avx512Lanes, AvxLanes};
use crate::math::sin_cos::run;

pub fn avx(lanes: AvxLanes, angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    avx_body(lanes.proof(), lanes, angles, sin, cos);
}

miraculix::avx_fn! {
    fn avx_body(_proof: Avx, lanes: AvxLanes, angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
        run(lanes, angles, sin, cos)
    }
}

pub fn avx512(lanes: Avx512Lanes, angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    avx512_body(lanes.proof(), lanes, angles, sin, cos);
}

miraculix::avx512f_fn! {
    fn avx512_body(
        _proof: Avx512f,
        lanes: Avx512Lanes,
        angles: &[f32],
        sin: &mut [f32],
        cos: &mut [f32],
    ) {
        run(lanes, angles, sin, cos)
    }
}
