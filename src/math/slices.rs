//! Float slice operations through `miraculix`. [`sliding_dot`] and
//! [`lerp_u8`] run on the `reduce` tier [`MELON_SIMD`](super::simd) caps
//! like the kernels. [`fmadd`] and [`sub`] take `miraculix`'s own dispatch,
//! which only `MELON_SIMD=scalar` caps.

use std::sync::OnceLock;

use super::simd;

/// `miraculix`'s `reduce` rungs in `available_tiers` order (scalar, SSE2,
/// AVX, AVX2, AVX2 with FMA, AVX-512F) as `MELON_SIMD` ranks.
#[cfg(target_arch = "x86_64")]
const REDUCE_RANKS: [u32; 6] = [0, 1, 3, 4, 4, 5];

#[cfg(target_arch = "x86_64")]
type ReduceTier = miraculix::x86::reduce::Tier;
#[cfg(target_arch = "aarch64")]
type ReduceTier = Option<miraculix::aarch64::ops::neon::neon::Neon>;

/// Every `reduce` tier this CPU runs, slowest first, with its rank.
#[cfg(target_arch = "x86_64")]
fn reduce_tiers() -> Vec<(ReduceTier, u32)> {
    miraculix::x86::reduce::available_tiers()
        .into_iter()
        .zip(REDUCE_RANKS)
        .filter_map(|(tier, rank)| Some((tier?, rank)))
        .collect()
}

#[cfg(target_arch = "aarch64")]
fn reduce_tiers() -> Vec<(ReduceTier, u32)> {
    use miraculix::aarch64::ops::neon::neon::Neon;
    let neon = Neon::from_features(miraculix::aarch64::detect_features());
    std::iter::once((None, 0))
        .chain(neon.map(|neon| (Some(neon), 1)))
        .collect()
}

/// The newest `reduce` tier `MELON_SIMD` allows.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn reduce_tier() -> ReduceTier {
    static TIER: OnceLock<ReduceTier> = OnceLock::new();
    *TIER.get_or_init(|| {
        let cap = simd::rank_cap();
        let tiers = reduce_tiers();
        tiers
            .iter()
            .rev()
            .find(|(_, rank)| *rank <= cap)
            .unwrap_or(&tiers[0])
            .0
    })
}

/// Whether `MELON_SIMD=scalar` keeps [`fmadd`] and [`sub`] off the vector unit.
fn scalar_only() -> bool {
    static SCALAR: OnceLock<bool> = OnceLock::new();
    *SCALAR.get_or_init(|| simd::rank_cap() == 0)
}

/// `out[i] = sum_k(signal[i + k] * taps[k])`: an FIR filter with its taps in
/// reverse order (the newest sample's tap last), one dispatch for the slice.
///
/// # Panics
/// If `out` is not empty and `signal.len() + 1 < out.len() + taps.len()`.
#[inline]
pub fn sliding_dot(signal: &[f32], taps: &[f32], out: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::reduce::sliding_dot_f32_with(reduce_tier(), signal, taps, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::reduce::sliding_dot_f32_with(reduce_tier(), signal, taps, out);
    #[cfg(target_arch = "riscv64")]
    if scalar_only() {
        sliding_dot_scalar(signal, taps, out);
    } else {
        miraculix::riscv::auto::sliding_dot_f32(signal, taps, out);
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    sliding_dot_scalar(signal, taps, out);
}

#[cfg(any(test, not(any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn sliding_dot_scalar(signal: &[f32], taps: &[f32], out: &mut [f32]) {
    assert!(out.is_empty() || signal.len() + 1 >= out.len() + taps.len());
    for (i, o) in out.iter_mut().enumerate() {
        *o = signal[i..][..taps.len()]
            .iter()
            .zip(taps)
            .map(|(x, t)| x * t)
            .sum();
    }
}

/// `out[i] = (a[i] * (den - num) + b[i] * num) / den`, rounded to the
/// nearest (halves up): `a` blended into `b` by `num / den`.
///
/// # Panics
/// If the three lengths differ, `den` is 0 or over 32767, or `num > den`.
#[inline]
pub fn lerp_u8(a: &[u8], b: &[u8], num: u16, den: u16, out: &mut [u8]) {
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::reduce::lerp_u8_with(reduce_tier(), a, b, num, den, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::reduce::lerp_u8_with(reduce_tier(), a, b, num, den, out);
    #[cfg(target_arch = "riscv64")]
    if scalar_only() {
        lerp_u8_scalar(a, b, num, den, out);
    } else {
        miraculix::riscv::auto::lerp_u8(a, b, num, den, out);
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    lerp_u8_scalar(a, b, num, den, out);
}

#[cfg(any(test, not(any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn lerp_u8_scalar(a: &[u8], b: &[u8], num: u16, den: u16, out: &mut [u8]) {
    assert!(a.len() == b.len() && b.len() == out.len());
    assert!((1..=32767).contains(&den) && num <= den);
    let (num, den) = (u32::from(num), u32::from(den));
    for ((&x, &y), o) in a.iter().zip(b).zip(out) {
        let sum = u32::from(x) * (den - num) + u32::from(y) * num;
        *o = ((2 * sum + den) / (2 * den)) as u8;
    }
}

/// `out[i] = a[i] * b[i] + c[i]`.
///
/// # Panics
/// If the four lengths differ.
#[inline]
pub fn fmadd(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) {
    if scalar_only() {
        return fmadd_scalar(a, b, c, out);
    }
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::auto_up::fmadd_f32(a, b, c, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::auto_up::fmadd_f32(a, b, c, out);
    #[cfg(target_arch = "riscv64")]
    miraculix::riscv::auto::fmadd_f32(a, b, c, out);
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    fmadd_scalar(a, b, c, out);
}

/// Unfused, as the SSE2 and AVX rungs (no FMA) round.
fn fmadd_scalar(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) {
    assert!(a.len() == b.len() && b.len() == c.len() && c.len() == out.len());
    for (((&x, &y), &z), o) in a.iter().zip(b).zip(c).zip(out) {
        *o = x * y + z;
    }
}

/// `out[i] = a[i] - b[i]`.
///
/// # Panics
/// If the three lengths differ.
#[inline]
pub fn sub(a: &[f32], b: &[f32], out: &mut [f32]) {
    if scalar_only() {
        return sub_scalar(a, b, out);
    }
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::auto_up::sub_f32(a, b, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::auto_up::sub_f32(a, b, out);
    #[cfg(target_arch = "riscv64")]
    miraculix::riscv::auto::sub_f32(a, b, out);
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    sub_scalar(a, b, out);
}

fn sub_scalar(a: &[f32], b: &[f32], out: &mut [f32]) {
    assert!(a.len() == b.len() && b.len() == out.len());
    for ((&x, &y), o) in a.iter().zip(b).zip(out) {
        *o = x - y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_ops_match_the_scalar_definitions() {
        let a: Vec<f32> = (0..70).map(|i| i as f32 * 0.25 - 3.0).collect();
        let b: Vec<f32> = (0..70).map(|i| 1.0 - i as f32 / 64.0).collect();
        let taps = [0.5, -0.25, 2.0];
        let mut out = vec![0.0; 68];
        sliding_dot(&a, &taps, &mut out);
        assert!((0..68).all(|i| out[i] == a[i] * 0.5 - a[i + 1] * 0.25 + a[i + 2] * 2.0));
        let mut out = vec![0.0; 70];
        fmadd(&a, &b, &a, &mut out);
        assert!((0..70).all(|i| (out[i] - (a[i] * b[i] + a[i])).abs() < 1e-5));
        sub(&a, &b, &mut out);
        assert!((0..70).all(|i| out[i] == a[i] - b[i]));
        let x: Vec<u8> = (0..70).map(|i| (i * 37 % 256) as u8).collect();
        let y: Vec<u8> = (0..70).map(|i| (255 - i * 11 % 256) as u8).collect();
        let mut out = vec![0; 70];
        lerp_u8(&x, &y, 333, 1000, &mut out);
        let want =
            |i: usize| ((u32::from(x[i]) * 1334 + u32::from(y[i]) * 666 + 1000) / 2000) as u8;
        assert!((0..70).all(|i| out[i] == want(i)));
    }

    /// Every `reduce` tier blends as the scalar code, bit for bit, and
    /// filters to within rounding, at lengths around each register width.
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    #[test]
    fn every_reduce_tier_matches_the_scalar_code() {
        #[cfg(target_arch = "aarch64")]
        use miraculix::aarch64::reduce::{lerp_u8_with, sliding_dot_f32_with};
        #[cfg(target_arch = "x86_64")]
        use miraculix::x86::reduce::{lerp_u8_with, sliding_dot_f32_with};

        use crate::util::xorshift::XorShift;
        let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
        for len in [0, 1, 7, 15, 16, 17, 31, 33, 63, 65, 130] {
            let a: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
            let b: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
            let signal: Vec<f32> = (0..len + 7).map(|_| rng.next_f64() as f32 - 0.5).collect();
            let taps: Vec<f32> = (0..8).map(|_| rng.next_f64() as f32 - 0.5).collect();
            for (num, den) in [(0, 1), (1, 1), (333, 1000), (1, 3), (32766, 32767)] {
                let mut want = vec![0; len];
                lerp_u8_scalar(&a, &b, num, den, &mut want);
                for (tier, rank) in reduce_tiers() {
                    let mut got = vec![0; len];
                    lerp_u8_with(tier, &a, &b, num, den, &mut got);
                    assert_eq!(got, want, "rank {rank}, {len} bytes, {num}/{den}");
                }
            }
            let mut want = vec![0.0; len];
            sliding_dot_scalar(&signal, &taps, &mut want);
            for (tier, rank) in reduce_tiers() {
                let mut got = vec![0.0; len];
                sliding_dot_f32_with(tier, &signal, &taps, &mut got);
                for (g, w) in got.iter().zip(&want) {
                    assert!(
                        (g - w).abs() <= 1e-6,
                        "rank {rank}, {len} samples: {g} vs {w}"
                    );
                }
            }
        }
    }
}
