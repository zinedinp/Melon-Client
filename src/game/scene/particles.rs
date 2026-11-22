//! The particle layer.

use crate::game::InWorld;
use crate::game::camera::Camera;
use crate::game::world_resources::WorldResources;
use crate::renderer::{EntityDraw, Renderer};

/// What the particle layer draws from.
pub(super) struct ParticleInput<'a> {
    pub camera: &'a Camera,
    pub aspect: f32,
    /// The level's partial tick: `ParticleEngine` stands still with it.
    pub partial: f32,
}

impl InWorld {
    /// Adds the particles in view to `out`; returns the debug screen's
    /// (drawn, hidden) counts.
    pub(super) fn particle_layer(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        input: &ParticleInput<'_>,
        out: &mut Vec<EntityDraw>,
    ) -> (usize, usize) {
        let frustum = input.camera.frustum(input.aspect);
        self.particle_draws(renderer, assets, input.camera, &frustum, input.partial, out);
        let (quads, total) = self.particles.engine.counts();
        (quads, total - quads)
    }
}
