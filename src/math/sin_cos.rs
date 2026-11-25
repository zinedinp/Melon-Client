//! Sines and cosines of many angles at once, each bit for bit
//! [`sin_cos_exact`]'s (`(float) Math.sin`): fdlibm's kernel on every
//! lane, the same operations in the same order, no fused multiply-add.
//!
//! A step loads the floats, widens them, runs the kernel, narrows and
//! stores, all in vector registers; only a lane past
//! [`REDUCE_LIMIT`](crate::math::angle::REDUCE_LIMIT) (or NaN) sends its
//! step to the scalar code.

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
use super::lanes::Lanes;
use super::simd::Tier;
use crate::math::angle::sin_cos_exact;
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
use crate::math::angle::{
    COS_COEFFS, FRAC_2_PI, PIO2_HI, PIO2_LO, REDUCE_LIMIT, ROUND_MAGIC, SIN_COEFFS,
};

/// The widest step any tier takes, for the padded last one.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const MAX_LANES: usize = 8;

/// `sin[i], cos[i]` of `angles[i]`, on the tier [`super::simd::init`]
/// settled.
pub fn sin_cos(angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    sin_cos_on(super::simd::tier(), angles, sin, cos);
}

/// [`sin_cos`] on `tier`, for the tests to try each.
pub fn sin_cos_on(tier: Tier, angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    assert_eq!(angles.len(), sin.len());
    assert_eq!(angles.len(), cos.len());
    match tier {
        #[cfg(target_arch = "x86_64")]
        Tier::Avx512(lanes) => super::x86::sin_cos::avx512(lanes, angles, sin, cos),
        #[cfg(target_arch = "x86_64")]
        Tier::Avx(lanes) => super::x86::sin_cos::avx(lanes, angles, sin, cos),
        // SSE2 and Neon are every x86_64's and AArch64's baseline: no
        // `target_feature` body needed.
        #[cfg(target_arch = "x86_64")]
        Tier::Sse2(lanes) => run(lanes, angles, sin, cos),
        #[cfg(target_arch = "aarch64")]
        Tier::Neon(lanes) => run(lanes, angles, sin, cos),
        Tier::Scalar => {
            for ((&x, s), c) in angles.iter().zip(sin).zip(cos) {
                (*s, *c) = sin_cos_exact(x);
            }
        }
    }
}

/// The loop: whole steps, the rest in half-wide ones, the last of those
/// padded.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
pub(super) fn run<L: Lanes>(lanes: L, angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    let done = steps(lanes, angles, sin, cos, 0);
    let half = lanes.half();
    let done = steps(half, angles, sin, cos, done);
    padded(half, angles, sin, cos, done);
}

/// Whole steps of `L::N` from `from` on; how far they got.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
fn steps<L: Lanes>(l: L, angles: &[f32], sin: &mut [f32], cos: &mut [f32], from: usize) -> usize {
    let n = L::N;
    let mut i = from;
    while i + n <= angles.len() {
        step(l, &angles[i..i + n], &mut sin[i..i + n], &mut cos[i..i + n]);
        i += n;
    }
    i
}

/// The angles from `whole` on, fewer than a step, padded to one.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
fn padded<L: Lanes>(lanes: L, angles: &[f32], sin: &mut [f32], cos: &mut [f32], whole: usize) {
    let n = L::N;
    debug_assert!(n <= MAX_LANES);
    let rest = angles.len() - whole;
    if rest > 0 {
        let mut x = [0.0f32; MAX_LANES];
        x[..rest].copy_from_slice(&angles[whole..]);
        let (mut s, mut c) = ([0.0f32; MAX_LANES], [0.0f32; MAX_LANES]);
        step(lanes, &x[..n], &mut s[..n], &mut c[..n]);
        sin[whole..].copy_from_slice(&s[..rest]);
        cos[whole..].copy_from_slice(&c[..rest]);
    }
}

/// One step of `L::N` angles.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
fn step<L: Lanes>(lanes: L, x: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    let wide = lanes.widen(lanes.load(x));
    if lanes.all_within(wide, f64::from(REDUCE_LIMIT)) {
        let (s, c) = kernel(lanes, wide);
        lanes.store(lanes.narrow(s), sin);
        lanes.store(lanes.narrow(c), cos);
    } else {
        far_step(&x[..L::N], sin, cos);
    }
}

/// A step with a lane past [`REDUCE_LIMIT`] or NaN, the scalar way: out
/// of the vector loop, so it doesn't crowd it.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[cold]
#[inline(never)]
fn far_step(x: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    for ((&x, s), c) in x.iter().zip(sin).zip(cos) {
        (*s, *c) = sin_cos_exact(x);
    }
}

/// `sin_cos_exact`'s double precision part on every lane: the quarter
/// turns taken off, fdlibm's polynomials, the quadrant's swap and signs.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[inline(always)]
fn kernel<L: Lanes>(l: L, x: L::F64) -> (L::F64, L::F64) {
    let k = |v: f64| l.splat(v);
    let [s1, s2, s3, s4, s5, s6] = SIN_COEFFS;
    let [c1, c2, c3, c4, c5, c6] = COS_COEFFS;
    let quarters = l.sub(
        l.add(l.mul(x, k(FRAC_2_PI)), k(ROUND_MAGIC)),
        k(ROUND_MAGIC),
    );
    let r = l.sub(
        l.sub(x, l.mul(quarters, k(PIO2_HI))),
        l.mul(quarters, k(PIO2_LO)),
    );
    let z = l.mul(r, r);
    let s = {
        let t = l.add(k(s5), l.mul(z, k(s6)));
        let t = l.add(k(s4), l.mul(z, t));
        let t = l.add(k(s3), l.mul(z, t));
        let tail = l.add(k(s2), l.mul(z, t));
        let s = l.add(r, l.mul(l.mul(z, r), l.add(k(s1), l.mul(z, tail))));
        // `-0.0` keeps its sign, as Java's sine does.
        l.select(l.eq(r, k(0.0)), r, s)
    };
    let c = {
        let t = l.add(k(c5), l.mul(z, k(c6)));
        let t = l.add(k(c4), l.mul(z, t));
        let t = l.add(k(c3), l.mul(z, t));
        let t = l.add(k(c2), l.mul(z, t));
        let tail = l.mul(z, l.add(k(c1), l.mul(z, t)));
        let half = l.mul(k(0.5), z);
        let w = l.sub(k(1.0), half);
        l.add(w, l.add(l.sub(l.sub(k(1.0), w), half), l.mul(z, tail)))
    };
    // `quarters mod 4`, by comparing rather than from the low bits of a
    // double (a library that sets DAZ would zero those): `floor(q / 4)` is
    // `q / 4 - 0.375` rounded to nearest, `q / 4` being a whole quarter.
    let fours = l.sub(
        l.add(l.sub(l.mul(quarters, k(0.25)), k(0.375)), k(ROUND_MAGIC)),
        k(ROUND_MAGIC),
    );
    let quadrant = l.sub(quarters, l.mul(fours, k(4.0)));
    let odd = l.mask_or(l.eq(quadrant, k(1.0)), l.eq(quadrant, k(3.0)));
    let upper = l.ge(quadrant, k(2.0));
    // Quadrant 1: (c, -s); 2: (-s, -c); 3: (-c, s).
    let sin = l.negate_where(upper, l.select(odd, c, s));
    let cos = l.negate_where(l.mask_xor(upper, odd), l.select(odd, s, c));
    (sin, cos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::angle::{HALF_PI, PI};

    /// Every tier the CPU has gives every angle `sin_cos_exact`'s bits,
    /// odd lengths and the far and NaN ones included.
    #[test]
    fn every_tier_gives_sin_cos_exacts_bits() {
        let mut angles: Vec<f32> = (0..200_003u32)
            .map(|k| (k as f32 * 0.618_034 - 60_000.0) * 0.013)
            .collect();
        angles.extend([
            0.0,
            -0.0,
            HALF_PI,
            -PI,
            f32::NAN,
            5.0e5,
            -3.0e7,
            f32::INFINITY,
        ]);
        let tiers = Tier::available();
        #[cfg(target_arch = "x86_64")]
        assert!(tiers.len() >= 2, "x86_64 always has SSE2");
        for tier in tiers {
            for len in [angles.len(), 1, 3, 5, 7, 9] {
                let angles = &angles[angles.len() - len..];
                let (mut sin, mut cos) = (vec![0.0; len], vec![0.0; len]);
                sin_cos_on(tier, angles, &mut sin, &mut cos);
                for (i, &x) in angles.iter().enumerate() {
                    let (s, c) = sin_cos_exact(x);
                    assert_eq!(
                        (sin[i].to_bits(), cos[i].to_bits()),
                        (s.to_bits(), c.to_bits()),
                        "{}: {x}",
                        tier.name()
                    );
                }
            }
        }
    }
}
