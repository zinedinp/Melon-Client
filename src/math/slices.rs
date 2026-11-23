//! Float slice operations, through `miraculix`'s own dispatch: fused
//! multiply-adds on AVX-512F, AVX2+FMA and NEON, scalar on other CPUs.

/// `sum(a[i] * b[i])`.
///
/// # Panics
/// If `a.len() != b.len()`.
#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    let sum = miraculix::x86::reduce::dot_f32(a, b);
    #[cfg(target_arch = "aarch64")]
    let sum = miraculix::aarch64::reduce::dot_f32(a, b);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let sum = {
        assert_eq!(a.len(), b.len());
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    };
    sum
}

/// `out[i] = a[i] * b[i] + c[i]`.
///
/// # Panics
/// If the four lengths differ.
#[inline]
pub fn fmadd(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::auto_up::fmadd_f32(a, b, c, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::auto_up::fmadd_f32(a, b, c, out);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        assert!(a.len() == b.len() && b.len() == c.len() && c.len() == out.len());
        for (((&x, &y), &z), o) in a.iter().zip(b).zip(c).zip(out) {
            *o = x.mul_add(y, z);
        }
    }
}

/// `out[i] = a[i] - b[i]`.
///
/// # Panics
/// If the three lengths differ.
#[inline]
pub fn sub(a: &[f32], b: &[f32], out: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::auto_up::sub_f32(a, b, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::auto_up::sub_f32(a, b, out);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        assert!(a.len() == b.len() && b.len() == out.len());
        for ((&x, &y), o) in a.iter().zip(b).zip(out) {
            *o = x - y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_ops_match_the_scalar_definitions() {
        let a: Vec<f32> = (0..70).map(|i| i as f32 * 0.25 - 3.0).collect();
        let b: Vec<f32> = (0..70).map(|i| 1.0 - i as f32 / 64.0).collect();
        let want: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!((dot(&a, &b) - want).abs() < 1e-3, "{} {want}", dot(&a, &b));
        let mut out = vec![0.0; 70];
        fmadd(&a, &b, &a, &mut out);
        assert!((0..70).all(|i| (out[i] - (a[i] * b[i] + a[i])).abs() < 1e-5));
        sub(&a, &b, &mut out);
        assert!((0..70).all(|i| out[i] == a[i] - b[i]));
    }
}
