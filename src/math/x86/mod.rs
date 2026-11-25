//! The x86_64 halves of the math kernels: [`lanes`] (SSE2, AVX, AVX-512F),
//! [`bytes`] (SSE2, SSSE3, AVX2, AVX-512 VBMI) and each kernel's
//! `target_feature` bodies, one file per kernel.

pub mod bytes;
pub mod lanes;
pub mod sin_cos;
