//! Vanilla `options.txt`: `key:value` lines next to `assets/`

use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::audio::music::{MusicFrequency, MusicToast};
use crate::audio::source::{SoundSource, Volumes};
use crate::cycle::Cycle;
use crate::i18n::languages::Language;
use crate::input::{self, Action, InputBind, InputSettings};
use crate::world::clouds::CloudStatus;
use crate::world::culling::CullSettings;
use crate::world::entity::player::{Arm, PlayerModelPart, ShownParts};

mod chat;
mod choices;
mod keys;
#[cfg(test)]
mod tests;

pub use crate::particle::ParticleStatus;
pub use crate::world::level::ChunkBuilder;
pub use chat::*;
pub use choices::*;
use keys::*;

/// `Options.fov`'s range and default, in degrees.
pub const FOV_RANGE: (u8, u8) = (30, 110);
pub const FOV_DEFAULT: u8 = 70;
/// `Options.renderDistance` (the 64-bit Java range) and default, in chunks.
pub const RENDER_DISTANCE_RANGE: (u8, u8) = (2, 32);
pub const RENDER_DISTANCE_DEFAULT: u8 = 12;
/// `Options.simulationDistance` (the 64-bit Java range) and default.
pub const SIMULATION_DISTANCE_RANGE: (u8, u8) = (5, 32);
pub const SIMULATION_DISTANCE_DEFAULT: u8 = 12;
/// `Options.entityDistanceScaling`: `0.5..=5.0` in quarter steps.
pub const ENTITY_DISTANCE_RANGE: (f32, f32) = (0.5, 5.0);
/// `Options.framerateLimit`.
pub const MAX_FPS_RANGE: (u32, u32) = (10, 260);
pub const MAX_FPS_STEP: u32 = 10;
pub const MAX_FPS_DEFAULT: u32 = 120;
/// `Options.gamma`: Moody `0`, Default `0.5`, Bright `1`.
pub const GAMMA_DEFAULT: f64 = 0.5;
/// `Options.biomeBlendRadius`: `0` is off, `7` blends 15x15 blocks.
pub const BIOME_BLEND_RANGE: (u8, u8) = (0, 7);
pub use crate::world::block_tint::BIOME_BLEND_DEFAULT;
/// `Options.cloudRange` in chunks.
pub const CLOUD_RANGE_RANGE: (u8, u8) = (2, 128);
pub const CLOUD_RANGE_DEFAULT: u8 = 128;
/// `Options.weatherRadius` in blocks.
pub const WEATHER_RADIUS_RANGE: (u8, u8) = (3, 10);
pub const WEATHER_RADIUS_DEFAULT: u8 = 10;
/// Slider steps for `Options.gamma` (`UnitDouble` shown as 0..=100).
pub const GAMMA_STEPS: u32 = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub lang: Language,
    /// Vertical field of view, degrees.
    pub fov: u8,
    /// Chunks: how far terrain is asked for and drawn.
    pub render_distance: u8,
    /// Chunks: how far the integrated server ticks the world around the player.
    pub simulation_distance: u8,
    /// Which culling runs, and how far entities are drawn.
    pub cull: CullSettings,
    /// `Options.graphicsPreset`: what last set the quality options.
    pub graphics_preset: GraphicsPreset,
    /// `Options.ambientOcclusion`, "Smooth Lighting".
    pub smooth_lighting: bool,
    /// `Options.cutoutLeaves`, "See-Through Leaves".
    pub cutout_leaves: bool,
    /// `Options.prioritizeChunkUpdates`.
    pub chunk_builder: ChunkBuilder,
    /// `Options.biomeBlendRadius`.
    pub biome_blend: u8,
    /// `Options.renderClouds`.
    pub clouds: CloudStatus,
    /// `Options.cloudRange`, in chunks.
    pub cloud_range: u8,
    /// `Options.weatherRadius`, in blocks.
    pub weather_radius: u8,
    /// `Options.vignette`: the darkened screen edges.
    pub vignette: bool,
    /// `Options.screenEffectScale` ("Distortion Effects").
    pub screen_effect_scale: f64,
    /// `Options.fovEffectScale` ("FOV Effects").
    pub fov_effect_scale: f64,
    /// `Options.damageTiltStrength` ("Damage Tilt").
    pub damage_tilt_strength: f64,
    /// `Options.guiScale`: `0` is Auto, the largest scale that fits.
    pub gui_scale: u8,
    /// `Options.fullscreen` (borderless, on the current monitor).
    pub fullscreen: bool,
    /// `Options.enableVsync`: present at the display's refresh rate.
    pub vsync: bool,
    /// `Options.pauseOnLostFocus` (F3 + P): the pause menu opens when the
    /// window loses the focus.
    pub pause_on_lost_focus: bool,
    /// `Options.debugGuiScale`: the F3 overlay's own GUI scale, `-1` the
    /// game's, `0` half the largest.
    pub debug_gui_scale: i32,
    /// How frames are presented; anything but `Auto` overrides `vsync`.
    pub present_mode: PresentMode,
    /// `Options.framerateLimit`; [`MAX_FPS_RANGE`]'s top is unlimited.
    pub max_fps: u32,
    pub inactivity_fps_limit: InactivityFpsLimit,
    /// `Options.gamma`: world brightness, `0` Moody .. `0.5` Default .. `1` Bright.
    pub gamma: f64,
    pub chat: ChatSettings,
    /// `Options.soundSourceVolumes`, the sliders of Music & Sound Options.
    pub sound_volumes: Volumes,
    /// `Options.soundDevice`.
    pub sound_device: String,
    /// `Options.showSubtitles` ("Closed Captions").
    pub show_subtitles: bool,
    /// `Options.directionalAudio`.
    pub directional_audio: bool,
    /// The output's speakers (Melon's own).
    pub speaker_layout: SpeakerLayout,
    pub music_frequency: MusicFrequency,
    pub music_toast: MusicToast,
    /// Mouse, toggles, sprint window and key binds.
    pub input: InputSettings,
    /// `Options.modelParts` (Skin Customization).
    pub model_parts: ShownParts,
    /// `Options.mainHand`.
    pub main_hand: Arm,
    /// `Options.bobView` ("View Bobbing"): the view sways while walking.
    pub bob_view: bool,
    /// `Options.highContrastBlockOutline`: the block outline in cyan over a
    /// wide black one.
    pub high_contrast_block_outline: bool,
    /// `Options.attackIndicator`.
    pub attack_indicator: AttackIndicator,
    /// `Options.particles`: how many particles show.
    pub particles: ParticleStatus,
    /// `Options.resourcePacks`.
    pub resource_packs: Vec<String>,
    /// `Options.incompatibleResourcePacks`.
    pub incompatible_resource_packs: Vec<String>,
}

impl Config {
    /// The framerate limit, `None` for unlimited.
    pub fn framerate_limit(&self) -> Option<u32> {
        (self.max_fps < MAX_FPS_RANGE.1).then_some(self.max_fps)
    }
}

impl Default for Config {
    /// Vanilla's defaults (`Options.languageCode = "en_us"`).
    fn default() -> Self {
        Self {
            lang: Language::EnUs,
            fov: FOV_DEFAULT,
            render_distance: RENDER_DISTANCE_DEFAULT,
            simulation_distance: SIMULATION_DISTANCE_DEFAULT,
            cull: CullSettings::default(),
            graphics_preset: GraphicsPreset::Fancy,
            smooth_lighting: true,
            cutout_leaves: true,
            chunk_builder: ChunkBuilder::None,
            biome_blend: BIOME_BLEND_DEFAULT,
            clouds: CloudStatus::Fancy,
            cloud_range: CLOUD_RANGE_DEFAULT,
            weather_radius: WEATHER_RADIUS_DEFAULT,
            vignette: true,
            screen_effect_scale: 1.0,
            fov_effect_scale: 1.0,
            damage_tilt_strength: 1.0,
            gui_scale: 0,
            fullscreen: false,
            vsync: true,
            pause_on_lost_focus: true,
            debug_gui_scale: 0,
            present_mode: PresentMode::Auto,
            max_fps: MAX_FPS_DEFAULT,
            inactivity_fps_limit: InactivityFpsLimit::Afk,
            gamma: GAMMA_DEFAULT,
            chat: ChatSettings::default(),
            sound_volumes: Volumes::default(),
            sound_device: String::new(),
            show_subtitles: false,
            directional_audio: false,
            speaker_layout: SpeakerLayout::Auto,
            music_frequency: MusicFrequency::Default,
            music_toast: MusicToast::Never,
            input: InputSettings::default(),
            model_parts: ShownParts::default(),
            main_hand: Arm::Right,
            bob_view: true,
            high_contrast_block_outline: false,
            attack_indicator: AttackIndicator::Crosshair,
            particles: ParticleStatus::All,
            resource_packs: vec!["vanilla".to_owned()],
            incompatible_resource_packs: Vec::new(),
        }
    }
}

impl Config {
    /// Vanilla puts `options.txt` in the game folder, beside `assets/`.
    fn path(game_dir: &Path) -> PathBuf {
        game_dir.join("options.txt")
    }

    /// Missing file or keys: defaults.
    pub fn load(game_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(game_dir))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for (name, value) in text.lines().filter_map(|line| line.split_once(':')) {
            let value = value.trim();
            if let Some(key) = KEYS.iter().find(|key| key.name == name) {
                (key.read)(&mut config, value);
            } else if let Some(source) = name
                .strip_prefix("soundCategory_")
                .and_then(SoundSource::from_name)
            {
                if let Some(volume) = unit(value) {
                    config.sound_volumes.set(source, volume);
                }
            } else if let Some(part) = name
                .strip_prefix("modelPart_")
                .and_then(PlayerModelPart::from_id)
            {
                config.model_parts.set(part, flag(value));
            } else if let Some(action) = name
                .strip_prefix("key_")
                .and_then(Action::from_vanilla_name)
            {
                config.input.binds.set(action, InputBind::parse(value));
            }
        }
        config
    }

    fn serialize(&self) -> String {
        let mut out: String = KEYS
            .iter()
            .map(|key| format!("{}:{}\n", key.name, (key.write)(self)))
            .collect();
        for source in SoundSource::ALL {
            let volume = self.sound_volumes.get(source);
            out.push_str(&format!("soundCategory_{}:{volume:?}\n", source.name()));
        }
        for part in PlayerModelPart::ALL {
            let shown = self.model_parts.shows(part);
            out.push_str(&format!("modelPart_{}:{shown}\n", part.id()));
        }
        for &action in Action::ALL {
            out.push_str(&format!(
                "key_{}:{}\n",
                action.vanilla_name(),
                self.input.binds.get(action).name()
            ));
        }
        out
    }

    pub fn save(&self, game_dir: &Path) {
        if let Err(e) = std::fs::write(Self::path(game_dir), self.serialize()) {
            crate::warn!("options.txt not written: {e:#}");
        }
    }
}
