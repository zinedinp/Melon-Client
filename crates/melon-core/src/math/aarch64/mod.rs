//! The AArch64 halves of the math kernels: [`lanes`] and [`bytes`] (Neon).
//! Neon is every AArch64's baseline, so the kernels need no
//! `target_feature` bodies.

#[cfg(target_endian = "little")]
pub mod bytes;
pub mod lanes;
