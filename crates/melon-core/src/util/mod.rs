//! Small helpers shared across the crates.

pub use melon_base::util::{base64, log};

pub mod intern;
/// Compiled outside tests too: other crates' tests use it.
pub mod xorshift;
