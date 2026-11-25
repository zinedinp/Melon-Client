//! Renderer pipelines, kept separate: [`sky`] (3D cubemap), [`world`] and [`blur`].

pub mod blur;
pub mod sky;
pub mod world;

pub use melon_base::gfx::pipeline::*;
