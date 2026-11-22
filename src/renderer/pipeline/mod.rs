//! Renderer pipelines, kept separate: [`sky`] (3D cubemap), [`world`]

pub mod blur;
pub mod sky;
pub mod world;

pub use melon_base::gfx::pipeline::*;
