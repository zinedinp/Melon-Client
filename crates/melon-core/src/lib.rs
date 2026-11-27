//! The bottom of the client: what every other Melon crate may use and
//! which uses none of them.
//!
//! - [`math`]: vanilla's `Mth`, angles and the SIMD kernels.
//! - [`util`]: interning, a fast PRNG, base64 and logging.
//! - [`cycle`]: values an options button steps through.
//! - [`instances`]: which instance folder the game plays in.
//! - [`probe`]: the tester's report.

pub mod cycle;
pub mod instances;
pub mod math;
pub mod probe;
pub mod util;
