//! Small helpers shared across the crate.

pub use melon_base::util::{base64, log};

pub mod intern;
#[cfg(test)]
pub mod xorshift;
