//! What a SIMD kernel needs of a CPU, as one trait (each architecture's in
//! its folder beside this file, `x86/lanes.rs`, `aarch64/lanes.rs`): `N` lanes of doubles
//! and of floats, loads and stores, widening and narrowing. A kernel is
//! written once against [`Lanes`] and runs on every tier that implements
//! it; every method is `#[inline(always)]`, so inside a kernel's
//! `target_feature` body the whole loop compiles to that tier's vector
//! instructions and nothing goes back to a scalar between load and store.

/// `N` lanes. Implemented only by tiers built from `miraculix` tokens, so
/// holding one proves the CPU has the instructions.
pub trait Lanes: Copy {
    /// Lanes per step.
    const N: usize;
    /// The tier half as wide (or this one, at four lanes), for a short
    /// last step: fewer lanes wasted on padding.
    type Half: Lanes;
    fn half(self) -> Self::Half;
    /// `N` doubles.
    type F64: Copy;
    /// `N` per-lane masks: all ones or all zeros.
    type M64: Copy;
    /// `N` floats.
    type F32: Copy;

    fn splat(self, v: f64) -> Self::F64;
    fn add(self, a: Self::F64, b: Self::F64) -> Self::F64;
    fn sub(self, a: Self::F64, b: Self::F64) -> Self::F64;
    fn mul(self, a: Self::F64, b: Self::F64) -> Self::F64;
    fn eq(self, a: Self::F64, b: Self::F64) -> Self::M64;
    fn ge(self, a: Self::F64, b: Self::F64) -> Self::M64;
    fn mask_or(self, a: Self::M64, b: Self::M64) -> Self::M64;
    fn mask_xor(self, a: Self::M64, b: Self::M64) -> Self::M64;
    /// `mask ? yes : no` per lane.
    fn select(self, mask: Self::M64, yes: Self::F64, no: Self::F64) -> Self::F64;
    /// `-v` where `mask` is set: the sign bit flipped, `-0.0` included.
    fn negate_where(self, mask: Self::M64, v: Self::F64) -> Self::F64;
    /// Whether every lane is within `-limit..=limit` (NaN is not).
    fn all_within(self, v: Self::F64, limit: f64) -> bool;

    /// `src[..N]`.
    fn load(self, src: &[f32]) -> Self::F32;
    /// Into `dst[..N]`.
    fn store(self, v: Self::F32, dst: &mut [f32]);
    fn widen(self, v: Self::F32) -> Self::F64;
    /// Rounded to nearest, as `as f32`.
    fn narrow(self, v: Self::F64) -> Self::F32;

    fn fadd(self, a: Self::F32, b: Self::F32) -> Self::F32;
    fn fsub(self, a: Self::F32, b: Self::F32) -> Self::F32;
    fn fmul(self, a: Self::F32, b: Self::F32) -> Self::F32;
    /// The sign bit flipped, as `-x`.
    fn fneg(self, a: Self::F32) -> Self::F32;
}
