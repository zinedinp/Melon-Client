//! Post effect chains (`assets/minecraft/post_effect/<id>.json`).

pub mod config;
pub(crate) mod gpu;
pub mod load;
pub(super) mod record;
pub use gpu::ChainSource;
pub mod shader;
pub mod uniform;
