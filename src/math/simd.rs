//! Which instruction sets the kernels run on, decided once.
//!
//! [`init`] probes the CPU at Melon's start (`miraculix`'s feature cache)
//! and settles the widest tier of each family: [`Tier`] for the kernels on
//! doubles and floats ([`super::lanes::Lanes`]), [`ByteTier`] for those on
//! bytes ([`super::bytes::Bytes`]). Every kernel asks [`tier`] or
//! [`byte_tier`] after that, a load of a cached value. A kernel sits beside
//! the scalar code it speeds up (`sin_cos.rs` beside `angle.rs`,
//! `section_decode.rs` beside `section.rs`), written once against its
//! family's trait; its x86_64 `target_feature` bodies are in the `x86/`
//! folder beside it, each architecture's lanes in `x86/` and `aarch64/`
//! here.
//!
//! x86_64 and AArch64 have kernels; other CPUs (RISC-V, PowerPC, wasm)
//! run the scalar code for now.

use std::sync::OnceLock;

/// The instruction set the float kernels run on.
#[derive(Clone, Copy)]
pub enum Tier {
    /// No vector unit Melon has kernels for.
    Scalar,
    /// Every x86_64: two doubles a register.
    #[cfg(target_arch = "x86_64")]
    Sse2(super::x86::lanes::Sse2Lanes),
    #[cfg(target_arch = "x86_64")]
    Avx(super::x86::lanes::AvxLanes),
    #[cfg(target_arch = "x86_64")]
    Avx512(super::x86::lanes::Avx512Lanes),
    /// Every AArch64: two doubles a register.
    #[cfg(target_arch = "aarch64")]
    Neon(super::aarch64::lanes::NeonLanes),
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Scalar => "scalar",
            #[cfg(target_arch = "x86_64")]
            Tier::Sse2(_) => "SSE2",
            #[cfg(target_arch = "x86_64")]
            Tier::Avx(_) => "AVX",
            #[cfg(target_arch = "x86_64")]
            Tier::Avx512(_) => "AVX-512F",
            #[cfg(target_arch = "aarch64")]
            Tier::Neon(_) => "Neon",
        }
    }

    /// Every tier this CPU can run, widest first, [`Tier::Scalar`] last.
    pub fn available() -> Vec<Tier> {
        let mut tiers = Self::vector();
        tiers.push(Tier::Scalar);
        tiers
    }

    #[cfg(target_arch = "x86_64")]
    fn vector() -> Vec<Tier> {
        use super::x86::lanes::{Avx512Lanes, AvxLanes, Sse2Lanes};
        let features = miraculix::x86::detect_features();
        let mut tiers = Vec::new();
        if os_saves_zmm() {
            tiers.extend(Avx512Lanes::new(features).map(Tier::Avx512));
        }
        if os_saves_ymm() {
            tiers.extend(AvxLanes::new(features).map(Tier::Avx));
        }
        tiers.extend(Sse2Lanes::new(features).map(Tier::Sse2));
        tiers
    }

    #[cfg(target_arch = "aarch64")]
    fn vector() -> Vec<Tier> {
        let features = miraculix::aarch64::detect_features();
        super::aarch64::lanes::NeonLanes::new(features)
            .map(Tier::Neon)
            .into_iter()
            .collect()
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn vector() -> Vec<Tier> {
        Vec::new()
    }
}

/// The instruction set the byte kernels (shuffles, table lookups) run on.
#[derive(Clone, Copy)]
pub enum ByteTier {
    Scalar,
    /// Every x86_64: unpacks and compares, no `pshufb` (K8, K10, Pentium 4).
    #[cfg(target_arch = "x86_64")]
    Sse2(super::x86::bytes::Sse2Bytes),
    /// `pshufb`: nearly every x86_64 since 2006.
    #[cfg(target_arch = "x86_64")]
    Ssse3(super::x86::bytes::Ssse3Bytes),
    #[cfg(target_arch = "x86_64")]
    Avx2(super::x86::bytes::Avx2Bytes),
    /// AVX-512 with VBMI's 128-entry byte permutes (Ice Lake, Zen 4 on).
    #[cfg(target_arch = "x86_64")]
    Avx512Vbmi(super::x86::bytes::VbmiBytes),
    /// Every little-endian AArch64.
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    Neon(super::aarch64::bytes::NeonBytes),
}

impl ByteTier {
    pub fn name(self) -> &'static str {
        match self {
            ByteTier::Scalar => "scalar",
            #[cfg(target_arch = "x86_64")]
            ByteTier::Sse2(_) => "SSE2",
            #[cfg(target_arch = "x86_64")]
            ByteTier::Ssse3(_) => "SSSE3",
            #[cfg(target_arch = "x86_64")]
            ByteTier::Avx2(_) => "AVX2",
            #[cfg(target_arch = "x86_64")]
            ByteTier::Avx512Vbmi(_) => "AVX-512VBMI",
            #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
            ByteTier::Neon(_) => "Neon",
        }
    }

    /// Every tier this CPU can run, widest first, [`ByteTier::Scalar`]
    /// last.
    pub fn available() -> Vec<ByteTier> {
        let mut tiers = Self::vector();
        tiers.push(ByteTier::Scalar);
        tiers
    }

    #[cfg(target_arch = "x86_64")]
    fn vector() -> Vec<ByteTier> {
        use super::x86::bytes::{Avx2Bytes, Sse2Bytes, Ssse3Bytes, VbmiBytes};
        let features = miraculix::x86::detect_features();
        let mut tiers = Vec::new();
        if os_saves_zmm() {
            tiers.extend(VbmiBytes::new(features).map(ByteTier::Avx512Vbmi));
        }
        if os_saves_ymm() {
            tiers.extend(Avx2Bytes::new(features).map(ByteTier::Avx2));
        }
        tiers.extend(Ssse3Bytes::new(features).map(ByteTier::Ssse3));
        tiers.extend(Sse2Bytes::new(features).map(ByteTier::Sse2));
        tiers
    }

    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    fn vector() -> Vec<ByteTier> {
        let features = miraculix::aarch64::detect_features();
        super::aarch64::bytes::NeonBytes::new(features)
            .map(ByteTier::Neon)
            .into_iter()
            .collect()
    }

    #[cfg(not(any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_endian = "little")
    )))]
    fn vector() -> Vec<ByteTier> {
        Vec::new()
    }
}

/// Whether the OS saves the 256-bit registers (XCR0, which `std` checks):
/// `miraculix` reads CPUID alone, and AVX with that state off faults
/// (`noxsave` kernels, some hypervisors).
#[cfg(target_arch = "x86_64")]
fn os_saves_ymm() -> bool {
    std::arch::is_x86_feature_detected!("avx")
}

/// [`os_saves_ymm`] for the 512-bit registers and mask registers.
#[cfg(target_arch = "x86_64")]
fn os_saves_zmm() -> bool {
    std::arch::is_x86_feature_detected!("avx512f")
}

/// Every tier name in the order CPUs gained them, for `MELON_SIMD`: a
/// cap admits the tiers up to its own, as on a CPU that stops there.
/// Neon is AArch64's baseline, as SSE2 is x86_64's.
const RANKS: [(&str, u32); 8] = [
    ("scalar", 0),
    ("SSE2", 1),
    ("Neon", 1),
    ("SSSE3", 2),
    ("AVX", 3),
    ("AVX2", 4),
    ("AVX-512F", 5),
    ("AVX-512VBMI", 6),
];

fn rank(name: &str) -> u32 {
    RANKS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(u32::MAX, |&(_, r)| r)
}

/// The newest tier rank `MELON_SIMD` allows; `u32::MAX` when it names none.
pub(crate) fn rank_cap() -> u32 {
    static CAP: OnceLock<u32> = OnceLock::new();
    *CAP.get_or_init(|| {
        let Ok(wanted) = std::env::var("MELON_SIMD") else {
            return u32::MAX;
        };
        let cap = RANKS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&wanted))
            .map(|&(_, r)| r);
        if cap.is_none() {
            crate::warn!("MELON_SIMD={wanted} names no tier; taking the widest");
        }
        cap.unwrap_or(u32::MAX)
    })
}

/// Buffers this many lanes long (zero padded) only ever take whole steps
/// on every tier: no copied, padded last one.
pub const PAD: usize = 4;

static TIER: OnceLock<Tier> = OnceLock::new();
static BYTE_TIER: OnceLock<ByteTier> = OnceLock::new();

/// At Melon's start: fills `miraculix`'s feature cache and settles the
/// tiers every kernel runs on from then on.
pub fn init() -> (Tier, ByteTier) {
    #[cfg(target_arch = "x86_64")]
    miraculix::x86::warm_up();
    #[cfg(target_arch = "aarch64")]
    miraculix::aarch64::warm_up();
    let tiers = (tier(), byte_tier());
    crate::info!("SIMD: {} floats, {} bytes", tiers.0.name(), tiers.1.name());
    tiers
}

/// The float tier [`init`] settled (probed now if nothing called it, as in
/// tests): the widest, or the newest no newer than the tier `MELON_SIMD`
/// names (`AVX`, `SSSE3`, `SSE2`, `scalar`, ...; either family's names).
#[inline]
pub fn tier() -> Tier {
    *TIER.get_or_init(|| {
        let cap = rank_cap();
        Tier::available()
            .into_iter()
            .find(|tier| rank(tier.name()) <= cap)
            .unwrap_or(Tier::Scalar)
    })
}

/// The byte tier [`init`] settled, chosen like [`tier`].
#[inline]
pub fn byte_tier() -> ByteTier {
    *BYTE_TIER.get_or_init(|| {
        let cap = rank_cap();
        ByteTier::available()
            .into_iter()
            .find(|tier| rank(tier.name()) <= cap)
            .unwrap_or(ByteTier::Scalar)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The baseline every x86_64 and AArch64 has is found, so its kernels
    /// run (and their tests test them) everywhere.
    #[test]
    fn the_baseline_tier_is_there() {
        let names: Vec<&str> = Tier::available().into_iter().map(Tier::name).collect();
        #[cfg(target_arch = "x86_64")]
        assert!(names.contains(&"SSE2"), "{names:?}");
        #[cfg(target_arch = "aarch64")]
        assert!(names.contains(&"Neon"), "{names:?}");
        assert_eq!(names.last(), Some(&"scalar"));
        if std::env::var_os("MELON_SIMD").is_none() {
            assert_eq!(tier().name(), names[0]);
        }
        let bytes: Vec<&str> = ByteTier::available()
            .into_iter()
            .map(ByteTier::name)
            .collect();
        #[cfg(target_arch = "x86_64")]
        assert!(bytes.contains(&"SSE2"), "{bytes:?}");
        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        assert!(bytes.contains(&"Neon"), "{bytes:?}");
        assert_eq!(bytes.last(), Some(&"scalar"));
    }

    /// Every tier's name has a rank, so `MELON_SIMD` can cap by it, and
    /// each family lists its tiers newest first.
    #[test]
    fn every_tier_name_has_a_rank() {
        let floats: Vec<u32> = Tier::available()
            .into_iter()
            .map(|t| rank(t.name()))
            .collect();
        let bytes: Vec<u32> = ByteTier::available()
            .into_iter()
            .map(|t| rank(t.name()))
            .collect();
        for ranks in [floats, bytes] {
            assert!(!ranks.contains(&u32::MAX), "{ranks:?}");
            assert!(ranks.is_sorted_by(|a, b| a > b), "{ranks:?}");
        }
    }
}
