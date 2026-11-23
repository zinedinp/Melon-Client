//! Float slice operations, through `miraculix`'s own dispatch: AVX-512F,
//! AVX2(+FMA), AVX or SSE2 on x86_64, NEON on AArch64, RVV on RISC-V,
//! scalar elsewhere.

/// `out[i] = sum_k(signal[i + k] * taps[k])`: an FIR filter with its taps in
/// reverse order (the newest sample's tap last), one dispatch for the slice.
///
/// # Panics
/// If `out` is not empty and `signal.len() + 1 < out.len() + taps.len()`.
#[inline]
pub fn sliding_dot(signal: &[f32], taps: &[f32], out: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::reduce::sliding_dot_f32(signal, taps, out);
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::reduce::sliding_dot_f32(signal, taps, out);
    #[cfg(target_arch = "riscv64")]
    miraculix::riscv::auto::sliding_dot_f32(signal, taps, out);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
    {
        assert!(out.is_empty() || signal.len() + 1 >= out.len() + taps.len());
        for (i, o) in out.iter_mut().enumerate() {
            *o = signal[i..][..taps.len()].iter().zip(taps).map(|(x, t)| x * t).sum();
        }
    }
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
    #[cfg(target_arch = "riscv64")]
    miraculix::riscv::auto::fmadd_f32(a, b, c, out);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
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
    #[cfg(target_arch = "riscv64")]
    miraculix::riscv::auto::sub_f32(a, b, out);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
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
        let taps = [0.5, -0.25, 2.0];
        let mut out = vec![0.0; 68];
        sliding_dot(&a, &taps, &mut out);
        assert!((0..68).all(|i| out[i] == a[i] * 0.5 - a[i + 1] * 0.25 + a[i + 2] * 2.0));
        let mut out = vec![0.0; 70];
        fmadd(&a, &b, &a, &mut out);
        assert!((0..70).all(|i| (out[i] - (a[i] * b[i] + a[i])).abs() < 1e-5));
        sub(&a, &b, &mut out);
        assert!((0..70).all(|i| out[i] == a[i] - b[i]));
    }
}
