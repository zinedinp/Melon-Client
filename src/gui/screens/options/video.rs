//! Vanilla `VideoSettingsScreen`, cut to the options Melon has.

use crate::cycle::Cycle;
use crate::gui::screen::{ConfigChange, ScreenContext};
use crate::gui::screens::options::{
    biome_blend_label, entity_distance_label, gamma_label, on_off_label, percent_label,
    render_distance_label, simulation_distance_label, value_label,
};
use crate::gui::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};
use crate::i18n::Lang;
use crate::settings::options::{self, Config, GraphicsPreset, InactivityFpsLimit};

/// The accessibility sliders' steps: 1% each, as `UnitDouble` shows them.
const EFFECT_STEPS: u32 = 100;

/// `entityDistanceScaling`'s `IntRange(2, 20)` of quarter steps.
const ENTITY_QUARTERS: (u32, u32) = (2, 20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoOption {
    Fullscreen,
    Vsync,
    MaxFps,
    InactivityFpsLimit,
    Brightness,
    GraphicsPreset,
    RenderDistance,
    SimulationDistance,
    BiomeBlend,
    SmoothLighting,
    CutoutLeaves,
    Clouds,
    Particles,
    CloudRange,
    WeatherRadius,
    EntityDistance,
    FrustumCulling,
    CaveCulling,
    GuiScale,
    Vignette,
    AttackIndicator,
    DistortionEffects,
    FovEffects,
    DamageTilt,
}

/// The sections, each a header and its options, two to a row.
static SECTIONS: [Section<VideoOption>; 3] = [
    Section {
        header: Some("options.video.display.header"),
        options: &[
            VideoOption::Fullscreen,
            VideoOption::Vsync,
            VideoOption::MaxFps,
            VideoOption::InactivityFpsLimit,
            VideoOption::Brightness,
        ],
    },
    Section {
        header: Some("options.video.quality.header"),
        options: &[
            VideoOption::GraphicsPreset,
            VideoOption::BiomeBlend,
            VideoOption::RenderDistance,
            VideoOption::SimulationDistance,
            VideoOption::SmoothLighting,
            VideoOption::Clouds,
            VideoOption::Particles,
            VideoOption::CloudRange,
            VideoOption::CutoutLeaves,
            VideoOption::WeatherRadius,
            VideoOption::EntityDistance,
            VideoOption::FrustumCulling,
            VideoOption::CaveCulling,
        ],
    },
    Section {
        header: Some("options.video.interface.header"),
        options: &[
            VideoOption::GuiScale,
            VideoOption::Vignette,
            VideoOption::AttackIndicator,
            VideoOption::DistortionEffects,
            VideoOption::FovEffects,
            VideoOption::DamageTilt,
        ],
    },
];

impl VideoOption {
    /// A slider's step count and current step, `None` for buttons.
    fn steps(self, config: &Config) -> Option<(u32, u32)> {
        Some(match self {
            Self::GraphicsPreset => (
                GraphicsPreset::ALL.len() as u32 - 1,
                GraphicsPreset::ALL
                    .iter()
                    .position(|&p| p == config.graphics_preset)
                    .unwrap_or(0) as u32,
            ),
            Self::BiomeBlend => {
                let (min, max) = options::BIOME_BLEND_RANGE;
                (
                    u32::from(max - min),
                    u32::from(config.biome_blend.clamp(min, max) - min),
                )
            }
            Self::DistortionEffects => (
                EFFECT_STEPS,
                (config.screen_effect_scale * f64::from(EFFECT_STEPS)).round() as u32,
            ),
            Self::FovEffects => (
                EFFECT_STEPS,
                (config.fov_effect_scale * f64::from(EFFECT_STEPS)).round() as u32,
            ),
            Self::DamageTilt => (
                EFFECT_STEPS,
                (config.damage_tilt_strength * f64::from(EFFECT_STEPS)).round() as u32,
            ),
            Self::RenderDistance => {
                let (min, max) = options::RENDER_DISTANCE_RANGE;
                (
                    u32::from(max - min),
                    u32::from(config.render_distance - min),
                )
            }
            Self::SimulationDistance => {
                let (min, max) = options::SIMULATION_DISTANCE_RANGE;
                (
                    u32::from(max - min),
                    u32::from(config.simulation_distance - min),
                )
            }
            Self::EntityDistance => (
                ENTITY_QUARTERS.1 - ENTITY_QUARTERS.0,
                ((config.cull.entity_distance * 4.0).round() as u32)
                    .saturating_sub(ENTITY_QUARTERS.0),
            ),
            Self::MaxFps => {
                let (min, max) = options::MAX_FPS_RANGE;
                (
                    (max - min) / options::MAX_FPS_STEP,
                    (config.max_fps - min) / options::MAX_FPS_STEP,
                )
            }
            Self::Brightness => (
                options::GAMMA_STEPS,
                (config.gamma * f64::from(options::GAMMA_STEPS)).round() as u32,
            ),
            Self::CloudRange => {
                let (min, max) = options::CLOUD_RANGE_RANGE;
                (
                    u32::from(max - min),
                    u32::from(config.cloud_range.clamp(min, max) - min),
                )
            }
            Self::WeatherRadius => {
                let (min, max) = options::WEATHER_RADIUS_RANGE;
                (
                    u32::from(max - min),
                    u32::from(config.weather_radius.clamp(min, max) - min),
                )
            }
            _ => return None,
        })
    }

    fn apply_step(self, config: &mut Config, step: u32) {
        if self.in_preset() {
            config.graphics_preset = GraphicsPreset::Custom;
        }
        match self {
            Self::GraphicsPreset => {
                let preset =
                    GraphicsPreset::ALL[(step as usize).min(GraphicsPreset::ALL.len() - 1)];
                preset.apply(config);
                config.graphics_preset = preset;
            }
            Self::BiomeBlend => {
                config.biome_blend = options::BIOME_BLEND_RANGE.0 + step as u8;
            }
            Self::DistortionEffects => {
                config.screen_effect_scale = f64::from(step) / f64::from(EFFECT_STEPS);
            }
            Self::FovEffects => {
                config.fov_effect_scale = f64::from(step) / f64::from(EFFECT_STEPS);
            }
            Self::DamageTilt => {
                config.damage_tilt_strength = f64::from(step) / f64::from(EFFECT_STEPS);
            }
            Self::RenderDistance => {
                config.render_distance = options::RENDER_DISTANCE_RANGE.0 + step as u8;
            }
            Self::SimulationDistance => {
                config.simulation_distance = options::SIMULATION_DISTANCE_RANGE.0 + step as u8;
            }
            Self::EntityDistance => {
                config.cull.entity_distance = (step + ENTITY_QUARTERS.0) as f32 / 4.0;
            }
            Self::MaxFps => {
                config.max_fps = options::MAX_FPS_RANGE.0 + step * options::MAX_FPS_STEP;
            }
            Self::Brightness => {
                config.gamma = f64::from(step) / f64::from(options::GAMMA_STEPS);
            }
            Self::CloudRange => {
                config.cloud_range = options::CLOUD_RANGE_RANGE.0 + step as u8;
            }
            Self::WeatherRadius => {
                config.weather_radius = options::WEATHER_RADIUS_RANGE.0 + step as u8;
            }
            _ => {}
        }
    }

    /// Clicks a button option: toggles or cycles it.
    fn apply_press(self, config: &mut Config, max_gui_scale: u8) {
        if self.in_preset() {
            config.graphics_preset = GraphicsPreset::Custom;
        }
        match self {
            Self::SmoothLighting => config.smooth_lighting = !config.smooth_lighting,
            Self::CutoutLeaves => config.cutout_leaves = !config.cutout_leaves,
            Self::Vignette => config.vignette = !config.vignette,
            Self::AttackIndicator => config.attack_indicator = config.attack_indicator.next(),
            Self::Fullscreen => config.fullscreen = !config.fullscreen,
            Self::Vsync => config.vsync = !config.vsync,
            Self::InactivityFpsLimit => {
                config.inactivity_fps_limit = config.inactivity_fps_limit.next();
            }
            Self::FrustumCulling => config.cull.frustum = !config.cull.frustum,
            Self::CaveCulling => config.cull.occlusion = !config.cull.occlusion,
            Self::Clouds => config.clouds = config.clouds.next(),
            Self::Particles => config.particles = config.particles.next(),
            Self::GuiScale => {
                config.gui_scale = if config.gui_scale >= max_gui_scale {
                    0
                } else {
                    config.gui_scale + 1
                };
            }
            _ => {}
        }
    }

    /// Whether changing it by hand makes the preset Custom.
    fn in_preset(self) -> bool {
        matches!(
            self,
            Self::RenderDistance
                | Self::SimulationDistance
                | Self::BiomeBlend
                | Self::SmoothLighting
                | Self::CutoutLeaves
                | Self::Clouds
                | Self::Particles
                | Self::CloudRange
                | Self::WeatherRadius
                | Self::EntityDistance
        )
    }

    /// The caption functions of `Options`.
    fn caption(self, config: &Config, lang: &Lang) -> String {
        match self {
            Self::Fullscreen => on_off_label(lang, "options.fullscreen", config.fullscreen),
            Self::Vsync => on_off_label(lang, "options.vsync", config.vsync),
            Self::MaxFps => {
                let value = match config.framerate_limit() {
                    Some(fps) => lang.format("options.framerate", &[&fps.to_string()]),
                    None => lang.get("options.framerateLimit.max").to_owned(),
                };
                value_label(lang, "options.framerateLimit", &value)
            }
            Self::InactivityFpsLimit => value_label(
                lang,
                "options.inactivityFpsLimit",
                lang.get(config.inactivity_fps_limit.key()),
            ),
            Self::Brightness => gamma_label(lang, config.gamma),
            Self::GraphicsPreset => value_label(
                lang,
                "options.graphics.preset",
                lang.get(config.graphics_preset.key()),
            ),
            Self::SmoothLighting => on_off_label(lang, "options.ao", config.smooth_lighting),
            Self::CutoutLeaves => on_off_label(lang, "options.cutoutLeaves", config.cutout_leaves),
            Self::Clouds => value_label(
                lang,
                "options.renderClouds",
                lang.get(config.clouds.caption_key()),
            ),
            Self::Particles => value_label(
                lang,
                "options.particles",
                lang.get(config.particles.caption_key()),
            ),
            Self::CloudRange => {
                let value = lang.format("options.chunks", &[&config.cloud_range.to_string()]);
                value_label(lang, "options.renderCloudsDistance", &value)
            }
            Self::WeatherRadius => {
                let value = lang.format("options.blocks", &[&config.weather_radius.to_string()]);
                value_label(lang, "options.weatherRadius", &value)
            }
            Self::BiomeBlend => biome_blend_label(lang, config.biome_blend),
            Self::Vignette => on_off_label(lang, "options.vignette", config.vignette),
            Self::AttackIndicator => value_label(
                lang,
                "options.attackIndicator",
                lang.get(config.attack_indicator.caption_key()),
            ),
            Self::DistortionEffects => percent_label(
                lang,
                "options.screenEffectScale",
                config.screen_effect_scale,
            ),
            Self::FovEffects => {
                percent_label(lang, "options.fovEffectScale", config.fov_effect_scale)
            }
            Self::DamageTilt => percent_label(
                lang,
                "options.damageTiltStrength",
                config.damage_tilt_strength,
            ),
            Self::RenderDistance => render_distance_label(lang, config.render_distance),
            Self::SimulationDistance => simulation_distance_label(lang, config.simulation_distance),
            Self::EntityDistance => entity_distance_label(lang, config.cull.entity_distance),
            Self::FrustumCulling => {
                on_off_label(lang, "melon.options.frustumCulling", config.cull.frustum)
            }
            Self::CaveCulling => {
                on_off_label(lang, "melon.options.caveCulling", config.cull.occlusion)
            }
            Self::GuiScale => {
                let scale = match config.gui_scale {
                    0 => lang.get("options.guiScale.auto").to_owned(),
                    n => n.to_string(),
                };
                value_label(lang, "options.guiScale", &scale)
            }
        }
    }
}

impl OptionEntry for VideoOption {
    fn slider(self, config: &Config) -> Option<(u32, u32)> {
        self.steps(config)
    }

    fn set_step(self, step: u32) -> ConfigChange {
        Box::new(move |config| self.apply_step(config, step))
    }

    fn press(self, ctx: &ScreenContext) -> ConfigChange {
        let max_gui_scale = ctx.scale.max_scale();
        Box::new(move |config| self.apply_press(config, max_gui_scale))
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        self.caption(config, lang)
    }

    fn wide(self) -> bool {
        self == Self::GraphicsPreset
    }

    fn tooltip(self, config: &Config) -> Option<&'static str> {
        match self {
            Self::GraphicsPreset => Some("options.graphics.preset.tooltip"),
            Self::Vsync => Some("melon.options.vsync.tooltip"),
            Self::Vignette => Some("options.vignette.tooltip"),
            Self::CutoutLeaves => Some("options.cutoutLeaves.tooltip"),
            Self::DistortionEffects => Some("options.screenEffectScale.tooltip"),
            Self::FovEffects => Some("options.fovEffectScale.tooltip"),
            Self::DamageTilt => Some("options.damageTiltStrength.tooltip"),
            Self::WeatherRadius => Some("options.weatherRadius.tooltip"),
            Self::InactivityFpsLimit => Some(match config.inactivity_fps_limit {
                InactivityFpsLimit::Minimized => "options.inactivityFpsLimit.minimized.tooltip",
                InactivityFpsLimit::Afk => "options.inactivityFpsLimit.afk.tooltip",
            }),
            _ => None,
        }
    }
}

/// `VideoSettingsScreen`.
pub fn video_settings_screen(ctx: &ScreenContext) -> OptionsSubScreen<VideoOption> {
    OptionsSubScreen::new("options.videoTitle", &SECTIONS, ctx)
}
