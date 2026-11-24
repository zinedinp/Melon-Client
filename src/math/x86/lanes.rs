//! [`Lanes`] on x86_64: SSE2 (four floats as two pairs of doubles, every
//! x86_64 has it), AVX (four doubles) and AVX-512F (eight). The arithmetic
//! is `miraculix`'s, widening and narrowing too (`cvtps2pd`, `cvtpd2ps`).

use miraculix::x86::FeatureSet;
use miraculix::x86::ops::avx::avx::Avx;
use miraculix::x86::ops::avx512::avx512f::Avx512f;
use miraculix::x86::ops::sse::sse::Sse;
use miraculix::x86::ops::sse::sse2::Sse2;

use crate::math::lanes::Lanes;

/// The sign bit alone.
const SIGN: f64 = -0.0;

/// Four lanes on SSE2: doubles in two registers of two.
#[derive(Clone, Copy)]
pub struct Sse2Lanes {
    sse: Sse,
    sse2: Sse2,
}

impl Sse2Lanes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            sse: Sse::from_features(features)?,
            sse2: Sse2::from_features(features)?,
        })
    }
}

/// Both halves of a pair through `op`.
macro_rules! pairs {
    ($op:expr, $a:expr, $b:expr) => {{
        let (a, b) = ($a, $b);
        [$op(a[0], b[0]), $op(a[1], b[1])]
    }};
}

impl Lanes for Sse2Lanes {
    const N: usize = 4;
    type Half = Self;
    #[inline(always)]
    fn half(self) -> Self {
        self
    }
    type F64 = [[f64; 2]; 2];
    type M64 = [[f64; 2]; 2];
    type F32 = [f32; 4];

    #[inline(always)]
    fn splat(self, v: f64) -> Self::F64 {
        [[v; 2]; 2]
    }
    #[inline(always)]
    fn add(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        pairs!(|a, b| self.sse2.add_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn sub(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        pairs!(|a, b| self.sse2.sub_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn mul(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        pairs!(|a, b| self.sse2.mul_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn eq(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        pairs!(|a, b| self.sse2.cmpeq_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn ge(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        pairs!(|a, b| self.sse2.cmpge_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn mask_or(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        pairs!(|a, b| self.sse2.or_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn mask_xor(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        pairs!(|a, b| self.sse2.xor_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn select(self, mask: Self::M64, yes: Self::F64, no: Self::F64) -> Self::F64 {
        let s = self.sse2;
        let pick = |m, y, n| s.or_f64x2(s.and_f64x2(m, y), s.andnot_f64x2(m, n));
        [pick(mask[0], yes[0], no[0]), pick(mask[1], yes[1], no[1])]
    }
    #[inline(always)]
    fn negate_where(self, mask: Self::M64, v: Self::F64) -> Self::F64 {
        let s = self.sse2;
        let flip = |m, v| s.xor_f64x2(v, s.and_f64x2(m, [SIGN; 2]));
        [flip(mask[0], v[0]), flip(mask[1], v[1])]
    }
    #[inline(always)]
    fn all_within(self, v: Self::F64, limit: f64) -> bool {
        let s = self.sse2;
        let inside = |v| s.movemask_f64x2(s.cmpge_f64x2([limit; 2], s.andnot_f64x2([SIGN; 2], v)));
        inside(v[0]) & inside(v[1]) == 0b11
    }

    #[inline(always)]
    fn load(self, src: &[f32]) -> Self::F32 {
        [src[0], src[1], src[2], src[3]]
    }
    #[inline(always)]
    fn store(self, v: Self::F32, dst: &mut [f32]) {
        dst[..4].copy_from_slice(&v);
    }
    #[inline(always)]
    fn widen(self, v: Self::F32) -> Self::F64 {
        let s = self.sse2;
        [
            s.cvt_f32x4_to_f64x2(v),
            s.cvt_f32x4_to_f64x2([v[2], v[3], v[2], v[3]]),
        ]
    }
    #[inline(always)]
    fn narrow(self, v: Self::F64) -> Self::F32 {
        let s = self.sse2;
        let (low, high) = (s.cvt_f64x2_to_f32x4(v[0]), s.cvt_f64x2_to_f32x4(v[1]));
        [low[0], low[1], high[0], high[1]]
    }

    #[inline(always)]
    fn fadd(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.sse.add_f32x4(a, b)
    }
    #[inline(always)]
    fn fsub(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.sse.sub_f32x4(a, b)
    }
    #[inline(always)]
    fn fmul(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.sse.mul_f32x4(a, b)
    }
    #[inline(always)]
    fn fneg(self, a: Self::F32) -> Self::F32 {
        self.sse.xor_f32x4(a, [-0.0; 4])
    }
}

/// Four lanes on AVX: the doubles in one 256-bit register.
#[derive(Clone, Copy)]
pub struct AvxLanes {
    sse: Sse,
    avx: Avx,
}

impl AvxLanes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            sse: Sse::from_features(features)?,
            avx: Avx::from_features(features)?,
        })
    }

    /// The token that proves this tier's features to a `*_fn!` kernel.
    #[inline(always)]
    pub fn proof(self) -> Avx {
        self.avx
    }
}

impl Lanes for AvxLanes {
    const N: usize = 4;
    type Half = Self;
    #[inline(always)]
    fn half(self) -> Self {
        self
    }
    type F64 = [f64; 4];
    type M64 = [f64; 4];
    type F32 = [f32; 4];

    #[inline(always)]
    fn splat(self, v: f64) -> Self::F64 {
        [v; 4]
    }
    #[inline(always)]
    fn add(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        self.avx.add_f64x4(a, b)
    }
    #[inline(always)]
    fn sub(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        self.avx.sub_f64x4(a, b)
    }
    #[inline(always)]
    fn mul(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        self.avx.mul_f64x4(a, b)
    }
    #[inline(always)]
    fn eq(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        self.avx.cmpeq_f64x4(a, b)
    }
    #[inline(always)]
    fn ge(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        self.avx.cmpge_f64x4(a, b)
    }
    #[inline(always)]
    fn mask_or(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        self.avx.or_f64x4(a, b)
    }
    #[inline(always)]
    fn mask_xor(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        self.avx.xor_f64x4(a, b)
    }
    #[inline(always)]
    fn select(self, mask: Self::M64, yes: Self::F64, no: Self::F64) -> Self::F64 {
        let a = self.avx;
        a.or_f64x4(a.and_f64x4(mask, yes), a.andnot_f64x4(mask, no))
    }
    #[inline(always)]
    fn negate_where(self, mask: Self::M64, v: Self::F64) -> Self::F64 {
        let a = self.avx;
        a.xor_f64x4(v, a.and_f64x4(mask, [SIGN; 4]))
    }
    #[inline(always)]
    fn all_within(self, v: Self::F64, limit: f64) -> bool {
        let a = self.avx;
        a.movemask_f64x4(a.cmpge_f64x4([limit; 4], a.andnot_f64x4([SIGN; 4], v))) == 0b1111
    }

    #[inline(always)]
    fn load(self, src: &[f32]) -> Self::F32 {
        [src[0], src[1], src[2], src[3]]
    }
    #[inline(always)]
    fn store(self, v: Self::F32, dst: &mut [f32]) {
        dst[..4].copy_from_slice(&v);
    }
    #[inline(always)]
    fn widen(self, v: Self::F32) -> Self::F64 {
        self.avx.cvt_f32x4_to_f64x4(v)
    }
    #[inline(always)]
    fn narrow(self, v: Self::F64) -> Self::F32 {
        self.avx.cvt_f64x4_to_f32x4(v)
    }

    #[inline(always)]
    fn fadd(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.sse.add_f32x4(a, b)
    }
    #[inline(always)]
    fn fsub(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.sse.sub_f32x4(a, b)
    }
    #[inline(always)]
    fn fmul(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.sse.mul_f32x4(a, b)
    }
    #[inline(always)]
    fn fneg(self, a: Self::F32) -> Self::F32 {
        self.sse.xor_f32x4(a, [-0.0; 4])
    }
}

/// Eight lanes on AVX-512F: the doubles in one 512-bit register, the
/// floats in a 256-bit one.
#[derive(Clone, Copy)]
pub struct Avx512Lanes {
    sse: Sse,
    avx: Avx,
    avx512: Avx512f,
}

impl Avx512Lanes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            sse: Sse::from_features(features)?,
            avx: Avx::from_features(features)?,
            avx512: Avx512f::from_features(features)?,
        })
    }

    /// The token that proves this tier's features to a `*_fn!` kernel.
    #[inline(always)]
    pub fn proof(self) -> Avx512f {
        self.avx512
    }
}

impl Lanes for Avx512Lanes {
    const N: usize = 8;
    type Half = AvxLanes;
    #[inline(always)]
    fn half(self) -> AvxLanes {
        AvxLanes {
            sse: self.sse,
            avx: self.avx,
        }
    }
    type F64 = [f64; 8];
    type M64 = [f64; 8];
    type F32 = [f32; 8];

    #[inline(always)]
    fn splat(self, v: f64) -> Self::F64 {
        [v; 8]
    }
    #[inline(always)]
    fn add(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        self.avx512.add_f64x8(a, b)
    }
    #[inline(always)]
    fn sub(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        self.avx512.sub_f64x8(a, b)
    }
    #[inline(always)]
    fn mul(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        self.avx512.mul_f64x8(a, b)
    }
    #[inline(always)]
    fn eq(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        self.avx512.cmpeq_f64x8(a, b)
    }
    #[inline(always)]
    fn ge(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        self.avx512.cmpge_f64x8(a, b)
    }
    #[inline(always)]
    fn mask_or(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        self.avx512.or_f64x8(a, b)
    }
    #[inline(always)]
    fn mask_xor(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        self.avx512.xor_f64x8(a, b)
    }
    #[inline(always)]
    fn select(self, mask: Self::M64, yes: Self::F64, no: Self::F64) -> Self::F64 {
        let a = self.avx512;
        a.or_f64x8(a.and_f64x8(mask, yes), a.andnot_f64x8(mask, no))
    }
    #[inline(always)]
    fn negate_where(self, mask: Self::M64, v: Self::F64) -> Self::F64 {
        let a = self.avx512;
        a.xor_f64x8(v, a.and_f64x8(mask, [SIGN; 8]))
    }
    #[inline(always)]
    fn all_within(self, v: Self::F64, limit: f64) -> bool {
        let a = self.avx512;
        let m = a.cmpge_f64x8([limit; 8], a.andnot_f64x8([SIGN; 8], v));
        let half = |i: usize| {
            self.avx
                .movemask_f64x4([m[i], m[i + 1], m[i + 2], m[i + 3]])
        };
        half(0) & half(4) == 0b1111
    }

    #[inline(always)]
    fn load(self, src: &[f32]) -> Self::F32 {
        std::array::from_fn(|i| src[i])
    }
    #[inline(always)]
    fn store(self, v: Self::F32, dst: &mut [f32]) {
        dst[..8].copy_from_slice(&v);
    }
    #[inline(always)]
    fn widen(self, v: Self::F32) -> Self::F64 {
        self.avx512.cvt_f32x8_to_f64x8(v)
    }
    #[inline(always)]
    fn narrow(self, v: Self::F64) -> Self::F32 {
        self.avx512.cvt_f64x8_to_f32x8(v)
    }

    #[inline(always)]
    fn fadd(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.avx.add_f32x8(a, b)
    }
    #[inline(always)]
    fn fsub(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.avx.sub_f32x8(a, b)
    }
    #[inline(always)]
    fn fmul(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.avx.mul_f32x8(a, b)
    }
    #[inline(always)]
    fn fneg(self, a: Self::F32) -> Self::F32 {
        self.avx.xor_f32x8(a, [-0.0; 8])
    }
}
