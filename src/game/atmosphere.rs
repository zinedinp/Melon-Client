//! Cloud and weather meshes. The weather's vertices are camera-relative; the

use super::InWorld;
use super::camera::Camera;
use super::environment::FrameEnvironment;
use super::world_resources::WorldResources;
use crate::renderer::{CloudDraw, EntityDraw, MeshId, Renderer, WeatherDraw, WorldVertex};
use crate::settings::options::Config;
use crate::world::clouds::{self, CloudFrame};
use crate::world::weather::{self, WeatherQuery};
use glam::Mat4;

/// What the clouds and the weather keep between frames.
#[derive(Default)]
pub(super) struct SkyMeshes {
    /// The weather columns around the camera, for what they were made for.
    weather_columns: Option<(weather::PrepareKey, Vec<weather::Prepared>)>,
    cloud_mesh: Option<MeshId>,
    /// What `cloud_mesh` was built for.
    cloud_key: Option<clouds::CloudKey>,
    rain_mesh: Option<MeshId>,
    snow_mesh: Option<MeshId>,
}

impl SkyMeshes {
    /// Frees the meshes; the next frame builds them again.
    pub(super) fn clear(&mut self, renderer: &mut Renderer) {
        for mesh in [
            &mut self.cloud_mesh,
            &mut self.rain_mesh,
            &mut self.snow_mesh,
        ] {
            if let Some(mesh) = mesh.take() {
                renderer.free_mesh(mesh);
            }
        }
        *self = Self::default();
    }
}

impl InWorld {
    /// Cloud boxes, then rain, then snow, matching vanilla's atmosphere pass.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn atmosphere_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        camera: &Camera,
        config: &Config,
        look: &FrameEnvironment,
        lightmap_params: &crate::world::lightmap::LightmapParams,
        partial: f32,
        aspect: f32,
    ) -> (Option<CloudDraw>, Vec<WeatherDraw>) {
        // The weather's quads are relative to the camera: placed from its
        // block, so the floats stay small however far out it is.
        let camera_block = camera.position.floor();
        let model = Mat4::from_translation((camera.position - camera_block).as_vec3());
        let camera_origin = camera_block.as_ivec3();
        let cloud_frame = CloudFrame {
            status: config.clouds,
            color: look.atmosphere.cloud_color,
            bottom_y: look.atmosphere.cloud_height,
            range_chunks: config.cloud_range,
            camera: camera.position,
            game_time: self.clocks.game_time(),
            partial_tick: partial,
        };
        let cloud_key = clouds::key(&assets.weather.clouds, &cloud_frame);
        if cloud_key != self.sky.cloud_key || (cloud_key.is_some() && self.sky.cloud_mesh.is_none())
        {
            let verts = clouds::build_anchored(&assets.weather.clouds, cloud_frame);
            replace_mesh(renderer, &mut self.sky.cloud_mesh, &verts);
            self.sky.cloud_key = cloud_key;
        }
        let cloud_anchor = {
            let at = clouds::anchor(&assets.weather.clouds, &cloud_frame);
            let block = at.floor();
            (block.as_ivec3(), (at - block).as_vec3())
        };
        let clouds = self.sky.cloud_mesh.map(|mesh| CloudDraw {
            color: cloud_frame.color,
            draw: EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas: renderer.white_texture(),
                model: Mat4::from_translation(cloud_anchor.1),
                origin: cloud_anchor.0,
                layer: crate::renderer::RenderLayer::Atmosphere,
                blend: crate::renderer::Blending::Translucent,
                light: None,
                overlay: 0.0,
                glint: crate::renderer::Glint::None,
            },
        });

        let mut weather = Vec::new();
        let intensity = self.weather.rain;
        let radius = i32::from(config.weather_radius);
        if intensity > 0.0 && radius > 0 {
            let level = &self.level;
            let biomes = &self.level_info.biomes;
            let at = camera.position.floor().as_ivec3();
            let key = weather::PrepareKey {
                block: at.to_array(),
                revision: level.revision(),
                radius,
            };
            if self
                .sky
                .weather_columns
                .as_ref()
                .is_none_or(|(k, _)| *k != key)
            {
                let height = |x, z| level.motion_blocking_height(x, z);
                let precipitation = |x, y, z| {
                    level
                        .biome(x, y, z)
                        .and_then(|id| biomes.biome(id))
                        .map(|biome| biome.precipitation())
                        .unwrap_or(weather::Precipitation::None)
                };
                let light = |x, y, z| level.light_at(x, y, z);
                let prepared = weather::prepare(&WeatherQuery {
                    intensity,
                    radius,
                    camera: camera.position,
                    game_time: 0,
                    partial_tick: 0.0,
                    height: &height,
                    precipitation: &precipitation,
                    light: &light,
                });
                self.sky.weather_columns = Some((key, prepared));
            }
            let frustum = camera.frustum(aspect);
            let game_time = self.clocks.game_time();
            let columns: Vec<_> = self
                .sky
                .weather_columns
                .iter()
                .flat_map(|(_, prepared)| prepared)
                .map(|p| p.column(game_time, partial))
                .collect();
            let (rain, snow) = weather::pieces(
                &columns,
                camera.position,
                radius,
                intensity,
                lightmap_params,
                |min, max| frustum.sees_box(min, max),
            );
            if let Some(pieces) = replace_mesh(renderer, &mut self.sky.rain_mesh, &rain.vertices) {
                weather.push(WeatherDraw {
                    template: assets.weather.rain_template,
                    pieces,
                    wraps: rain.wraps,
                    atlas: assets.weather.rain,
                    model,
                    origin: camera_origin,
                });
            }
            if let Some(pieces) = replace_mesh(renderer, &mut self.sky.snow_mesh, &snow.vertices) {
                weather.push(WeatherDraw {
                    template: assets.weather.snow_template,
                    pieces,
                    wraps: snow.wraps,
                    atlas: assets.weather.snow,
                    model,
                    origin: camera_origin,
                });
            }
        } else {
            replace_mesh(renderer, &mut self.sky.rain_mesh, &[]);
            replace_mesh(renderer, &mut self.sky.snow_mesh, &[]);
        }
        (clouds, weather)
    }
}

fn replace_mesh(
    renderer: &mut Renderer,
    slot: &mut Option<MeshId>,
    verts: &[WorldVertex],
) -> Option<MeshId> {
    if let Some(old) = slot.take() {
        renderer.free_mesh(old);
    }
    if verts.is_empty() {
        return None;
    }
    match renderer.upload_mesh(verts) {
        Ok(mesh) => {
            *slot = Some(mesh);
            Some(mesh)
        }
        Err(e) => {
            eprintln!("Atmosphere mesh failed: {e:#}");
            None
        }
    }
}
