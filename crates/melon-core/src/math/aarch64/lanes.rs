//! [`Lanes`] on AArch64 Neon, which every AArch64 CPU has: four floats as
//! two pairs of doubles. The arithmetic is `miraculix`'s, widening and
//! narrowing too (`fcvtl`, `fcvtn`).

use miraculix::aarch64::FeatureSet;
use miraculix::aarch64::ops::neon::neon::Neon;

use crate::math::lanes::Lanes;

#[derive(Clone, Copy)]
pub struct NeonLanes {
    neon: Neon,
}

impl NeonLanes {
    pub fn new(features: FeatureSet) -> Option<Self> {
        Some(Self {
            neon: Neon::from_features(features)?,
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

impl Lanes for NeonLanes {
    const N: usize = 4;
    type Half = Self;
    #[inline(always)]
    fn half(self) -> Self {
        self
    }
    type F64 = [[f64; 2]; 2];
    type M64 = [[u64; 2]; 2];
    type F32 = [f32; 4];

    #[inline(always)]
    fn splat(self, v: f64) -> Self::F64 {
        [[v; 2]; 2]
    }
    #[inline(always)]
    fn add(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        pairs!(|a, b| self.neon.add_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn sub(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        pairs!(|a, b| self.neon.sub_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn mul(self, a: Self::F64, b: Self::F64) -> Self::F64 {
        pairs!(|a, b| self.neon.mul_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn eq(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        pairs!(|a, b| self.neon.cmpeq_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn ge(self, a: Self::F64, b: Self::F64) -> Self::M64 {
        pairs!(|a, b| self.neon.cmpge_f64x2(a, b), a, b)
    }
    #[inline(always)]
    fn mask_or(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        pairs!(|a, b| self.neon.or_u64x2(a, b), a, b)
    }
    #[inline(always)]
    fn mask_xor(self, a: Self::M64, b: Self::M64) -> Self::M64 {
        pairs!(|a, b| self.neon.xor_u64x2(a, b), a, b)
    }
    #[inline(always)]
    fn select(self, mask: Self::M64, yes: Self::F64, no: Self::F64) -> Self::F64 {
        let n = self.neon;
        [
            n.select_f64x2(mask[0], yes[0], no[0]),
            n.select_f64x2(mask[1], yes[1], no[1]),
        ]
    }
    #[inline(always)]
    fn negate_where(self, mask: Self::M64, v: Self::F64) -> Self::F64 {
        let n = self.neon;
        let flip = |m, v| n.select_f64x2(m, n.neg_f64x2(v), v);
        [flip(mask[0], v[0]), flip(mask[1], v[1])]
    }
    #[inline(always)]
    fn all_within(self, v: Self::F64, limit: f64) -> bool {
        let n = self.neon;
        let inside = |v| n.cmple_f64x2(n.abs_f64x2(v), [limit; 2]);
        let [a, b] = [inside(v[0]), inside(v[1])];
        a[0] & a[1] & b[0] & b[1] == u64::MAX
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
        let n = self.neon;
        [n.cvt_f32x4_to_f64x2(v), n.cvt_high_f32x4_to_f64x2(v)]
    }
    #[inline(always)]
    fn narrow(self, v: Self::F64) -> Self::F32 {
        let n = self.neon;
        let (low, high) = (n.cvt_f64x2_to_f32x2(v[0]), n.cvt_f64x2_to_f32x2(v[1]));
        [low[0], low[1], high[0], high[1]]
    }

    #[inline(always)]
    fn fadd(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.neon.add_f32x4(a, b)
    }
    #[inline(always)]
    fn fsub(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.neon.sub_f32x4(a, b)
    }
    #[inline(always)]
    fn fmul(self, a: Self::F32, b: Self::F32) -> Self::F32 {
        self.neon.mul_f32x4(a, b)
    }
    #[inline(always)]
    fn fneg(self, a: Self::F32) -> Self::F32 {
        self.neon.neg_f32x4(a)
    }
}
