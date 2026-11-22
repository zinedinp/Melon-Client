//! `Options.processOptions`.

use super::*;

/// A JSON list of strings, as `options.txt` keeps the pack lists.
pub(super) fn string_list(value: &str) -> Option<Vec<String>> {
    serde_json::from_str(value).ok()
}

pub(super) fn write_string_list(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".to_owned())
}

/// `OptionInstance.UnitDouble`'s codec: a double clamped to `0..=1`.
pub(super) fn unit(value: &str) -> Option<f64> {
    clamped(value, 0.0, 1.0)
}

/// A number, clamped into `min..=max` as vanilla's codecs do.
pub(super) fn clamped<T: FromStr + PartialOrd>(value: &str, min: T, max: T) -> Option<T> {
    let v = value.parse::<T>().ok()?;
    Some(if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    })
}

pub(super) fn flag(value: &str) -> bool {
    value == "true"
}

/// One `options.txt` key.
pub(super) struct OptionKey {
    pub(super) name: &'static str,
    pub(super) read: fn(&mut Config, &str),
    pub(super) write: fn(&Config) -> String,
}

/// Every key Melon reads and writes, in the order it writes them.
pub(super) const KEYS: &[OptionKey] = &[
    OptionKey {
        name: "resourcePacks",
        read: |c, v| {
            if let Some(list) = string_list(v) {
                c.resource_packs = list;
            }
        },
        write: |c| write_string_list(&c.resource_packs),
    },
    OptionKey {
        name: "incompatibleResourcePacks",
        read: |c, v| {
            if let Some(list) = string_list(v) {
                c.incompatible_resource_packs = list;
            }
        },
        write: |c| write_string_list(&c.incompatible_resource_packs),
    },
    OptionKey {
        name: "lang",
        read: |c, v| {
            if let Some(language) = Language::from_code(v) {
                c.lang = language;
            }
        },
        write: |c| c.lang.code().to_owned(),
    },
    OptionKey {
        name: "fov",
        read: |c, v| {
            if let Ok(stored) = v.parse::<f64>() {
                let degrees = (stored * 40.0 + 70.0).round();
                let (min, max) = FOV_RANGE;
                c.fov = degrees.clamp(min.into(), max.into()) as u8;
            }
        },
        write: |c| ((f64::from(c.fov) - 70.0) / 40.0).to_string(),
    },
    OptionKey {
        name: "renderDistance",
        read: |c, v| {
            let (min, max) = RENDER_DISTANCE_RANGE;
            c.render_distance = clamped(v, min, max).unwrap_or(c.render_distance);
        },
        write: |c| c.render_distance.to_string(),
    },
    OptionKey {
        name: "simulationDistance",
        read: |c, v| {
            let (min, max) = SIMULATION_DISTANCE_RANGE;
            c.simulation_distance = clamped(v, min, max).unwrap_or(c.simulation_distance);
        },
        write: |c| c.simulation_distance.to_string(),
    },
    OptionKey {
        name: "entityDistanceScaling",
        read: |c, v| {
            let (min, max) = ENTITY_DISTANCE_RANGE;
            c.cull.entity_distance = clamped(v, min, max).unwrap_or(c.cull.entity_distance);
        },
        write: |c| c.cull.entity_distance.to_string(),
    },
    OptionKey {
        name: "ao",
        read: |c, v| c.smooth_lighting = flag(v),
        write: |c| c.smooth_lighting.to_string(),
    },
    OptionKey {
        name: "soundDevice",
        read: |c, v| {
            if let Ok(name) = serde_json::from_str::<String>(v) {
                c.sound_device = name;
            }
        },
        write: |c| serde_json::to_string(&c.sound_device).unwrap_or_else(|_| "\"\"".to_owned()),
    },
    OptionKey {
        name: "showSubtitles",
        read: |c, v| c.show_subtitles = flag(v),
        write: |c| c.show_subtitles.to_string(),
    },
    OptionKey {
        name: "directionalAudio",
        read: |c, v| c.directional_audio = flag(v),
        write: |c| c.directional_audio.to_string(),
    },
    OptionKey {
        name: "musicFrequency",
        read: |c, v| {
            if let Some(frequency) = MusicFrequency::from_id(v.trim_matches('"')) {
                c.music_frequency = frequency;
            }
        },
        write: |c| format!("\"{}\"", c.music_frequency.id()),
    },
    OptionKey {
        name: "musicToast",
        read: |c, v| {
            if let Some(toast) = MusicToast::from_id(v.trim_matches('"')) {
                c.music_toast = toast;
            }
        },
        write: |c| format!("\"{}\"", c.music_toast.id()),
    },
    OptionKey {
        name: "graphicsPreset",
        read: |c, v| {
            if let Some(preset) = GraphicsPreset::from_id(v.trim_matches('"')) {
                c.graphics_preset = preset;
            }
        },
        write: |c| format!("\"{}\"", c.graphics_preset.id()),
    },
    OptionKey {
        name: "prioritizeChunkUpdates",
        read: |c, v| {
            if let Ok(id) = v.parse::<i64>() {
                c.chunk_builder = ChunkBuilder::from_id(id);
            }
        },
        write: |c| (c.chunk_builder as u8).to_string(),
    },
    OptionKey {
        name: "biomeBlendRadius",
        read: |c, v| {
            let (min, max) = BIOME_BLEND_RANGE;
            c.biome_blend = clamped(v, min, max).unwrap_or(c.biome_blend);
        },
        write: |c| c.biome_blend.to_string(),
    },
    OptionKey {
        name: "renderClouds",
        read: |c, v| {
            if let Some(status) = CloudStatus::from_options(v) {
                c.clouds = status;
            }
        },
        write: |c| format!("\"{}\"", c.clouds.options_id()),
    },
    OptionKey {
        name: "cloudRange",
        read: |c, v| {
            let (min, max) = CLOUD_RANGE_RANGE;
            c.cloud_range = clamped(v, min, max).unwrap_or(c.cloud_range);
        },
        write: |c| c.cloud_range.to_string(),
    },
    OptionKey {
        name: "weatherRadius",
        read: |c, v| {
            let (min, max) = WEATHER_RADIUS_RANGE;
            c.weather_radius = clamped(v, min, max).unwrap_or(c.weather_radius);
        },
        write: |c| c.weather_radius.to_string(),
    },
    OptionKey {
        name: "vignette",
        read: |c, v| c.vignette = flag(v),
        write: |c| c.vignette.to_string(),
    },
    OptionKey {
        name: "screenEffectScale",
        read: |c, v| c.screen_effect_scale = unit(v).unwrap_or(c.screen_effect_scale),
        write: |c| c.screen_effect_scale.to_string(),
    },
    OptionKey {
        name: "fovEffectScale",
        read: |c, v| c.fov_effect_scale = unit(v).unwrap_or(c.fov_effect_scale),
        write: |c| c.fov_effect_scale.to_string(),
    },
    OptionKey {
        name: "damageTiltStrength",
        read: |c, v| c.damage_tilt_strength = unit(v).unwrap_or(c.damage_tilt_strength),
        write: |c| c.damage_tilt_strength.to_string(),
    },
    OptionKey {
        name: "guiScale",
        read: |c, v| c.gui_scale = clamped(v, 0, 255).unwrap_or(c.gui_scale),
        write: |c| c.gui_scale.to_string(),
    },
    OptionKey {
        name: "fullscreen",
        read: |c, v| c.fullscreen = flag(v),
        write: |c| c.fullscreen.to_string(),
    },
    OptionKey {
        name: "debugGuiScale",
        read: |c, v| c.debug_gui_scale = v.parse::<i32>().unwrap_or(0).max(-1),
        write: |c| c.debug_gui_scale.to_string(),
    },
    OptionKey {
        name: "pauseOnLostFocus",
        read: |c, v| c.pause_on_lost_focus = flag(v),
        write: |c| c.pause_on_lost_focus.to_string(),
    },
    OptionKey {
        name: "enableVsync",
        read: |c, v| c.vsync = flag(v),
        write: |c| c.vsync.to_string(),
    },
    OptionKey {
        name: "speakerLayout",
        read: |c, v| c.speaker_layout = SpeakerLayout::from_id(v.trim_matches('"')),
        write: |c| c.speaker_layout.id().to_owned(),
    },
    OptionKey {
        name: "presentMode",
        read: |c, v| c.present_mode = PresentMode::from_id(v.trim_matches('"')),
        write: |c| c.present_mode.id().to_owned(),
    },
    OptionKey {
        name: "maxFps",
        read: |c, v| {
            let (min, max) = MAX_FPS_RANGE;
            c.max_fps = clamped(v, min, max).unwrap_or(c.max_fps);
        },
        write: |c| c.max_fps.to_string(),
    },
    OptionKey {
        name: "inactivityFpsLimit",
        read: |c, v| {
            c.inactivity_fps_limit = match v.trim_matches('"') {
                "minimized" => InactivityFpsLimit::Minimized,
                _ => InactivityFpsLimit::Afk,
            };
        },
        write: |c| c.inactivity_fps_limit.id().to_owned(),
    },
    OptionKey {
        name: "gamma",
        read: |c, v| c.gamma = unit(v).unwrap_or(c.gamma),
        write: |c| c.gamma.to_string(),
    },
    OptionKey {
        name: "chatVisibility",
        read: |c, v| {
            if let Ok(id) = v.parse::<i64>() {
                c.chat.visibility = ChatVisibility::from_id(id);
            }
        },
        write: |c| (c.chat.visibility as u8).to_string(),
    },
    OptionKey {
        name: "chatColors",
        read: |c, v| c.chat.colors = flag(v),
        write: |c| c.chat.colors.to_string(),
    },
    OptionKey {
        name: "chatLinks",
        read: |c, v| c.chat.links = flag(v),
        write: |c| c.chat.links.to_string(),
    },
    OptionKey {
        name: "chatLinksPrompt",
        read: |c, v| c.chat.links_prompt = flag(v),
        write: |c| c.chat.links_prompt.to_string(),
    },
    OptionKey {
        name: "chatOpacity",
        read: |c, v| c.chat.opacity = unit(v).unwrap_or(c.chat.opacity),
        write: |c| c.chat.opacity.to_string(),
    },
    OptionKey {
        name: "textBackgroundOpacity",
        read: |c, v| c.chat.background_opacity = unit(v).unwrap_or(c.chat.background_opacity),
        write: |c| c.chat.background_opacity.to_string(),
    },
    OptionKey {
        name: "backgroundForChatOnly",
        read: |c, v| c.chat.background_for_chat_only = flag(v),
        write: |c| c.chat.background_for_chat_only.to_string(),
    },
    OptionKey {
        name: "chatScale",
        read: |c, v| c.chat.scale = unit(v).unwrap_or(c.chat.scale),
        write: |c| c.chat.scale.to_string(),
    },
    OptionKey {
        name: "chatLineSpacing",
        read: |c, v| c.chat.line_spacing = unit(v).unwrap_or(c.chat.line_spacing),
        write: |c| c.chat.line_spacing.to_string(),
    },
    OptionKey {
        name: "chatDelay",
        read: |c, v| c.chat.delay = clamped(v, 0.0, 6.0).unwrap_or(c.chat.delay),
        write: |c| c.chat.delay.to_string(),
    },
    OptionKey {
        name: "chatWidth",
        read: |c, v| c.chat.width = unit(v).unwrap_or(c.chat.width),
        write: |c| c.chat.width.to_string(),
    },
    OptionKey {
        name: "chatHeightFocused",
        read: |c, v| c.chat.height_focused = unit(v).unwrap_or(c.chat.height_focused),
        write: |c| c.chat.height_focused.to_string(),
    },
    OptionKey {
        name: "chatHeightUnfocused",
        read: |c, v| c.chat.height_unfocused = unit(v).unwrap_or(c.chat.height_unfocused),
        write: |c| c.chat.height_unfocused.to_string(),
    },
    OptionKey {
        name: "autoSuggestions",
        read: |c, v| c.chat.auto_suggestions = flag(v),
        write: |c| c.chat.auto_suggestions.to_string(),
    },
    OptionKey {
        name: "hideMatchedNames",
        read: |c, v| c.chat.hide_matched_names = flag(v),
        write: |c| c.chat.hide_matched_names.to_string(),
    },
    OptionKey {
        name: "reducedDebugInfo",
        read: |c, v| c.chat.reduced_debug_info = flag(v),
        write: |c| c.chat.reduced_debug_info.to_string(),
    },
    OptionKey {
        name: "onlyShowSecureChat",
        read: |c, v| c.chat.only_show_secure_chat = flag(v),
        write: |c| c.chat.only_show_secure_chat.to_string(),
    },
    OptionKey {
        name: "saveChatDrafts",
        read: |c, v| c.chat.save_drafts = flag(v),
        write: |c| c.chat.save_drafts.to_string(),
    },
    OptionKey {
        name: "mouseSensitivity",
        read: |c, v| c.input.sensitivity = unit(v).unwrap_or(c.input.sensitivity),
        write: |c| c.input.sensitivity.to_string(),
    },
    OptionKey {
        name: "invertYMouse",
        read: |c, v| c.input.invert_y = flag(v),
        write: |c| c.input.invert_y.to_string(),
    },
    OptionKey {
        name: "autoJump",
        read: |c, v| c.input.auto_jump = flag(v),
        write: |c| c.input.auto_jump.to_string(),
    },
    OptionKey {
        name: "toggleCrouch",
        read: |c, v| c.input.toggle_crouch = flag(v),
        write: |c| c.input.toggle_crouch.to_string(),
    },
    OptionKey {
        name: "toggleSprint",
        read: |c, v| c.input.toggle_sprint = flag(v),
        write: |c| c.input.toggle_sprint.to_string(),
    },
    OptionKey {
        name: "sprintWindow",
        read: |c, v| {
            c.input.sprint_window =
                clamped(v, 0, input::SPRINT_WINDOW_MAX).unwrap_or(c.input.sprint_window);
        },
        write: |c| c.input.sprint_window.to_string(),
    },
    OptionKey {
        name: "attackIndicator",
        read: |c, v| {
            if let Ok(id) = v.parse::<i64>() {
                c.attack_indicator = AttackIndicator::from_id(id);
            }
        },
        write: |c| c.attack_indicator.id().to_string(),
    },
    OptionKey {
        name: "particles",
        read: |c, v| {
            if let Ok(id) = v.parse::<i64>() {
                c.particles = ParticleStatus::from_id(id);
            }
        },
        write: |c| c.particles.id().to_string(),
    },
    OptionKey {
        name: "bobView",
        read: |c, v| c.bob_view = flag(v),
        write: |c| c.bob_view.to_string(),
    },
    OptionKey {
        name: "highContrastBlockOutline",
        read: |c, v| c.high_contrast_block_outline = flag(v),
        write: |c| c.high_contrast_block_outline.to_string(),
    },
    OptionKey {
        name: "mainHand",
        read: |c, v| {
            if let Some(arm) = Arm::from_id(v.trim_matches('"')) {
                c.main_hand = arm;
            }
        },
        write: |c| format!("\"{}\"", c.main_hand.id()),
    },
    OptionKey {
        name: "melon.frustumCulling",
        read: |c, v| c.cull.frustum = v != "false",
        write: |c| c.cull.frustum.to_string(),
    },
    OptionKey {
        name: "melon.caveCulling",
        read: |c, v| c.cull.occlusion = v != "false",
        write: |c| c.cull.occlusion.to_string(),
    },
];
