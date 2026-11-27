//! The sky layer: sun, moon and stars, clouds and weather.

use super::view::FrameView;
use crate::game::InWorld;
use crate::game::world_resources::WorldResources;
use melon_render::{CelestialDraw, CloudDraw, Renderer, WeatherDraw};
use melon_settings::options::Config;

/// The sky layer's draws.
pub(super) struct SkyLayer {
    pub celestials: Vec<CelestialDraw>,
    pub clouds: Option<CloudDraw>,
    pub weather: Vec<WeatherDraw>,
}

impl InWorld {
    pub(super) fn sky_layer(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        view: &FrameView,
        config: &Config,
        aspect: f32,
    ) -> SkyLayer {
        let camera = &view.camera;
        let celestials = self.sky_draws(assets, &view.look, camera, renderer.white_texture());
        let (clouds, weather) = self.atmosphere_draws(renderer, assets, view, config, aspect);
        SkyLayer {
            celestials,
            clouds,
            weather,
        }
    }
}
