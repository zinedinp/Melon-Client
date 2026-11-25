//! What the camera sees of the environment each frame.

use glam::{Mat4, Vec3};

use super::InWorld;
use super::camera::Camera;
use super::world_resources::WorldResources;
use crate::renderer::{CelestialDraw, SkyKind};
use crate::world::atmosphere::{Atmosphere, BiomeMix};
use crate::world::dimension::Skybox;
use crate::world::fluid::FluidKind;
use crate::world::fog::{self, FogContext, FogData, FogEffects, FogType};
use crate::world::level::Level;
use crate::world::physics::{eye_in_water, fluid_height};
use crate::world::sky;

/// `LocalPlayer.waterVisionTime`'s end.
const WATER_VISION_TICKS: i32 = 600;

/// What the fog keeps between frames.
pub(super) struct FogState {
    /// `AtmosphericFogEnvironment.rainFogMultiplier`: how far the rain's fog
    /// has closed in around the camera.
    rain_fog: f32,
    /// When `rain_fog` last moved, for the ticks since.
    rain_fog_at: Option<std::time::Instant>,
    /// What this frame is cleared to: the fog's colour.
    pub(super) clear_color: [f32; 4],
}

impl Default for FogState {
    fn default() -> Self {
        Self {
            rain_fog: 0.0,
            rain_fog_at: None,
            clear_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// One frame's environment.
pub struct FrameEnvironment {
    pub atmosphere: Atmosphere,
    pub fog: FogData,
    pub fog_type: FogType,
}

impl InWorld {
    /// Blindness and night vision, as the fog reads them.
    fn fog_effects(&self, partial: f32) -> FogEffects {
        use crate::world::player::status_effects::{BLINDNESS_ID, NIGHT_VISION_ID};
        let blindness = self
            .status_effects
            .get(BLINDNESS_ID)
            .map(|effect| (effect.duration, effect.infinite()));
        let night_vision = self
            .status_effects
            .get(NIGHT_VISION_ID)
            .map_or(0.0, |effect| {
                if effect.ends_within(200) {
                    0.7 + crate::math::angle::sin(
                        (effect.duration as f32 - partial) * crate::math::angle::PI * 0.2,
                    ) * 0.3
                } else {
                    1.0
                }
            });
        FogEffects {
            blindness,
            darkness: None,
            night_vision,
        }
    }

    /// This frame's atmosphere at the camera, its fog, and what the camera sees of the sky.
    pub(super) fn environment(
        &self,
        camera: &Camera,
        aspect: f32,
        partial: f32,
        render_distance: u8,
        cloud_range: u8,
    ) -> FrameEnvironment {
        let day = self.clocks.environment(partial);
        let mix = BiomeMix::gaussian(camera.position, |x, y, z| self.level.biome_at(x, y, z));
        let atmosphere = Atmosphere::resolve(
            &self.level_info.dimension_type,
            &self.level_info.biomes,
            &mix,
            &day,
        )
        .with_weather(self.weather.rain, self.weather.thunder);
        let fog_type = fog_type(&self.level, camera, aspect);
        let spectator = self.game_mode.current.is_spectator();
        let min_y = self.level_info.dimension_type.height.min_y;
        let context = FogContext {
            fog_type,
            spectator,
            render_distance_chunks: render_distance,
            camera_y: camera.position.y as f32,
            min_y,
            void_darkness_range: 32.0,
            water_vision: self.water_vision(fog_type == FogType::Water),
            fire_resistance: false,
            forward: camera.forward(),
            effects: self.fog_effects(partial),
            cloud_range_chunks: cloud_range,
            rain_level: self.weather.rain,
            thunder_level: self.weather.thunder,
            rain_fog: self.fog.rain_fog,
        };
        let fog = fog::setup(&atmosphere, &context);
        FrameEnvironment {
            atmosphere,
            fog,
            fog_type,
        }
    }

    /// `AtmosphericFogEnvironment.updateRainFogState`: the rain's fog moves a
    ///
    /// `FogRenderer.setupFog` runs only the first environment that applies:
    /// lava, powder snow, blindness, darkness and water come before this
    /// one, and the multiplier stands still under them. The frame's time
    /// still moves on, as `DeltaTracker.getGameTimeDeltaTicks` does.
    ///
    /// The step is held at 1: vanilla's `deltaTicks * 0.2` overshoots the
    /// target on frames over 250 ms (a hitch while loading), Melon's stops
    /// at it. Below that both are the same.
    pub(super) fn update_rain_fog(
        &mut self,
        camera: &Camera,
        aspect: f32,
        partial: f32,
        now: std::time::Instant,
    ) {
        let ticks = self.fog.rain_fog_at.replace(now).map_or(0.0, |last| {
            now.duration_since(last).as_secs_f32() / self.level_settings.tick_seconds()
        });
        let effects = self.fog_effects(partial);
        let atmospheric = fog_type(&self.level, camera, aspect) == FogType::Atmospheric
            && effects.blindness.is_none()
            && effects.darkness.is_none();
        if !atmospheric {
            return;
        }
        let at = camera.position.floor().as_ivec3();
        let sky = f32::from(self.level.light_at(at.x, at.y, at.z).1);
        let open_sky = ((sky - 8.0) / 7.0).clamp(0.0, 1.0);
        let rains = self
            .level
            .biome_at(at.x, at.y, at.z)
            .and_then(|id| self.level_info.biomes.biome(id))
            .is_none_or(|biome| biome.has_precipitation);
        let target = self.weather.rain * open_sky * if rains { 1.0 } else { 0.5 };
        self.fog.rain_fog += (target - self.fog.rain_fog) * (ticks * 0.2).min(1.0);
    }

    /// `LocalPlayer.getWaterVision`.
    fn water_vision(&self, in_water: bool) -> f32 {
        if !in_water {
            return 0.0;
        }
        let time = self.body.water_vision_time as f32;
        if self.body.water_vision_time >= WATER_VISION_TICKS {
            return 1.0;
        }
        let a = (time / 100.0).clamp(0.0, 1.0);
        let b = if time < 100.0 {
            0.0
        } else {
            ((time - 100.0) / 500.0).clamp(0.0, 1.0)
        };
        a * 0.6 + b * 0.399_999_98
    }

    /// One client tick of the eyes getting used to the water.
    pub(super) fn tick_water_vision(&mut self) {
        let eye = self.player.tick_eye();
        let in_water = eye_in_water(&self.level, eye);
        if in_water {
            let speed = if self.game_mode.current.is_spectator() {
                10
            } else {
                1
            };
            self.body.water_vision_time =
                (self.body.water_vision_time + speed).clamp(0, WATER_VISION_TICKS);
        } else if self.body.water_vision_time > 0 {
            self.body.water_vision_time =
                (self.body.water_vision_time - 10).clamp(0, WATER_VISION_TICKS);
        }
    }

    /// The sky's meshes in drawing order for this frame, as `SkyRenderer`
    pub(super) fn sky_draws(
        &self,
        assets: &WorldResources,
        look: &FrameEnvironment,
        camera: &Camera,
        white: crate::renderer::TextureId,
    ) -> Vec<CelestialDraw> {
        let atmosphere = &look.atmosphere;
        let mut out = Vec::new();
        match atmosphere.skybox {
            Skybox::None => {}
            Skybox::End => out.push(CelestialDraw {
                kind: SkyKind::Glow,
                mesh: assets.sky.end_sky_mesh,
                atlas: assets.sky.end_sky,
                model: Mat4::IDENTITY,
                tint: [1.0; 4],
            }),
            Skybox::Overworld => {
                let [r, g, b] = atmosphere.sky_color;
                out.push(CelestialDraw {
                    kind: SkyKind::Disc,
                    mesh: assets.sky.disc_mesh,
                    atlas: white,
                    model: Mat4::from_translation(Vec3::ZERO),
                    tint: [r, g, b, 1.0],
                });
                let day = &atmosphere.day;
                let glow = atmosphere.sunrise_sunset_color;
                if glow[3] > 0.001 {
                    out.push(CelestialDraw {
                        kind: SkyKind::Glow,
                        mesh: assets.sky.sunrise_mesh,
                        atlas: white,
                        model: sky::sunrise_model(day.sun_angle, glow[3]),
                        tint: glow,
                    });
                }
                for (mesh, atlas, model) in [
                    (assets.sky.sun_mesh, assets.sky.sun, sky::sun_model(day)),
                    (
                        assets.sky.moon_mesh,
                        assets.sky.moon[day.moon_phase as usize],
                        sky::moon_model(day),
                    ),
                ] {
                    out.push(CelestialDraw {
                        kind: SkyKind::Celestial,
                        mesh,
                        atlas,
                        model,
                        tint: [1.0; 4],
                    });
                }
                if day.star_brightness > 0.001 {
                    let b = day.star_brightness;
                    out.push(CelestialDraw {
                        kind: SkyKind::Celestial,
                        mesh: assets.sky.star_mesh,
                        atlas: white,
                        model: sky::star_model(day),
                        tint: [b, b, b, b],
                    });
                }
                if camera.position.y < f64::from(HORIZON_HEIGHT) && look.fog_type != FogType::Water
                {
                    out.push(CelestialDraw {
                        kind: SkyKind::Disc,
                        mesh: assets.sky.dark_disc_mesh,
                        atlas: white,
                        model: Mat4::from_translation(Vec3::new(0.0, 12.0, 0.0)),
                        tint: [0.0, 0.0, 0.0, 1.0],
                    });
                }
            }
        }
        out
    }
}

/// `ClientLevelData.getHorizonHeight`.
const HORIZON_HEIGHT: f32 = 63.0;

/// `Camera.getFluidInCamera`, `ATMOSPHERIC` for none.
pub fn fog_type(level: &Level, camera: &Camera, aspect: f32) -> FogType {
    if eye_in_water(level, camera.position) {
        return FogType::Water;
    }
    for offset in camera.near_plane(aspect) {
        let point = camera.position + offset.as_dvec3();
        let block = point.floor().as_ivec3();
        if let Some(height) = fluid_height(level, block, FluidKind::Lava) {
            if point.y <= f64::from(block.y) + f64::from(height) {
                return FogType::Lava;
            }
        } else if level
            .block_state(block.x, block.y, block.z)
            .is_some_and(is_powder_snow)
        {
            return FogType::PowderSnow;
        }
    }
    FogType::Atmospheric
}

/// Whether a block state is powder snow.
fn is_powder_snow(state: u16) -> bool {
    pumpkin_data::BlockStateId::new(state).is_some_and(|id| {
        pumpkin_data::Block::from_state_id(id) == &pumpkin_data::Block::POWDER_SNOW
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{Chunk, Section};
    use pumpkin_data::Block;

    /// Looking north, so the view is the identity.
    fn camera_at(position: glam::DVec3) -> Camera {
        Camera::new(
            position,
            crate::math::euler::Look::new(180.0, 0.0),
            70.0,
            256.0,
        )
    }

    /// A level of air with `states` at the given positions.
    fn level_with(states: &[([i32; 3], u16)]) -> Level {
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        for &(pos, state) in states {
            level.set_block(pos, state, false);
        }
        level
    }

    fn water() -> u16 {
        Block::WATER.default_state.id.as_u16()
    }

    #[test]
    fn the_air_is_atmospheric_and_water_fogs_below_its_surface() {
        let level = level_with(&[([4, 70, 4], water()), ([4, 69, 4], water())]);
        let aspect = 16.0 / 9.0;
        assert_eq!(
            fog_type(&level, &camera_at(glam::DVec3::new(8.5, 70.5, 8.5)), aspect),
            FogType::Atmospheric
        );
        assert_eq!(
            fog_type(&level, &camera_at(glam::DVec3::new(4.5, 70.5, 4.5)), aspect),
            FogType::Water
        );
        assert_eq!(
            fog_type(
                &level,
                &camera_at(glam::DVec3::new(4.5, 70.95, 4.5)),
                aspect
            ),
            FogType::Atmospheric
        );
    }

    #[test]
    fn lava_and_powder_snow_fog_the_near_plane() {
        let lava = Block::LAVA.default_state.id.as_u16();
        let snow = Block::POWDER_SNOW.default_state.id.as_u16();
        let level = level_with(&[([4, 70, 4], lava), ([9, 70, 4], snow)]);
        let aspect = 1.0;
        assert_eq!(
            fog_type(&level, &camera_at(glam::DVec3::new(4.5, 70.5, 4.5)), aspect),
            FogType::Lava
        );
        assert_eq!(
            fog_type(&level, &camera_at(glam::DVec3::new(9.5, 70.5, 4.5)), aspect),
            FogType::PowderSnow
        );
    }

    #[test]
    fn a_covered_source_is_full_to_the_top() {
        let stone = Block::STONE.default_state.id.as_u16();
        let level = level_with(&[([4, 70, 4], water()), ([4, 71, 4], stone)]);
        assert_eq!(
            fog_type(&level, &camera_at(glam::DVec3::new(4.5, 70.97, 4.5)), 1.0),
            FogType::Water
        );
    }
}
