//! [`crate::culling::section_cull`]'s column test under AVX: eight
//! section heights a register, every plane a compare, the lanes left
//! standing as bits (`vmovmskps`).

use miraculix::x86::ops::avx::avx::Avx;

use crate::culling::section_cull::{LANES, SectionCull};
use melon_core::math::simd::{self, Tier};

#[derive(Clone, Copy)]
pub struct CullAvx(Avx);

impl CullAvx {
    pub fn new() -> Option<Self> {
        if !matches!(simd::tier(), Tier::Avx(_) | Tier::Avx512(_)) {
            return None;
        }
        Avx::from_features(miraculix::x86::detect_features()).map(Self)
    }
}

/// Eight lanes of a row.
#[inline(always)]
fn lanes(row: &[f32; LANES], at: usize) -> [f32; 8] {
    std::array::from_fn(|i| row[at + i])
}

/// [`SectionCull::column`] from its column parts: `least` per plane,
/// `fog_room` the squared distance left to the fog's end.
pub fn column(t: CullAvx, cull: &SectionCull, least: &[f32; 6], fog_room: Option<f32>) -> u32 {
    column_avx(t.0, cull, least, fog_room)
}

miraculix::avx_fn! {
    fn column_avx(avx: Avx, cull: &SectionCull, least: &[f32; 6], fog_room: Option<f32>) -> u32 {
        let mut mask = 0u32;
        for at in (0..LANES).step_by(8) {
            let mut lit = [f32::from_bits(!0); 8];
            if let Some(planes) = &cull.planes {
                for (row, &least) in planes.iter().zip(least) {
                    let front = avx.cmpge_f32x8(lanes(&row.0, at), [least; 8]);
                    lit = avx.and_f32x8(lit, front);
                }
            }
            if let (Some((near, _)), Some(room)) = (&cull.fog, fog_room) {
                lit = avx.and_f32x8(lit, avx.cmplt_f32x8(lanes(&near.0, at), [room; 8]));
            }
            mask |= u32::from(avx.movemask_f32x8(lit)) << at;
        }
        mask
    }
}
