//! Changed options, applied and saved at once (`OptionInstance.set`).

use std::time::Duration;

use ash::vk;
use winit::window::Fullscreen;

use super::RunningState;
use crate::i18n::Lang;
use crate::i18n::languages::Language;
use crate::network::ClientInfo;
use crate::renderer::FIFO_LATEST_READY;
use crate::settings::options::{self, Config, PresentMode};

impl RunningState {
    /// Puts VSync and fullscreen as the options say.
    pub(super) fn apply_display_options(&mut self) {
        let requested = match self.env.config.present_mode {
            PresentMode::Auto => None,
            PresentMode::Fifo => Some(vk::PresentModeKHR::FIFO),
            PresentMode::Mailbox => Some(vk::PresentModeKHR::MAILBOX),
            PresentMode::Immediate => Some(vk::PresentModeKHR::IMMEDIATE),
            PresentMode::FifoLatestReady => Some(FIFO_LATEST_READY),
        };
        if let Err(e) = self.renderer.set_present(self.env.config.vsync, requested) {
            crate::warn!("Present mode not changed: {e:#}");
        }
        let fullscreen = self
            .env
            .config
            .fullscreen
            .then_some(Fullscreen::Borderless(None));
        if self.window.fullscreen().is_some() != fullscreen.is_some() {
            self.window.set_fullscreen(fullscreen);
        }
    }

    /// Lays every screen and overlay out again for the window's GUI size.
    pub(super) fn resize_gui(&mut self) {
        let size = self.gui_size();
        self.screens.resize(&self.env.ctx(self.scale()));
        if let Some(world) = &mut self.world {
            world.resize_overlay(size.0, size.1);
        }
    }

    /// Applies, saves and shows changed options.
    pub(super) fn set_config(&mut self, config: Config) {
        let old = &self.env.config;
        if config == *old {
            return;
        }
        let info = ClientInfo::from_config(&config);
        if info != ClientInfo::from_config(old)
            && let Some(world) = &self.world
        {
            world.send_client_info(&info);
        }
        config.save(self.env.assets.game_dir());
        let rescale = config.gui_scale != old.gui_scale;
        let display = (config.vsync, config.present_mode, config.fullscreen)
            != (old.vsync, old.present_mode, old.fullscreen);
        if config.sound_device != old.sound_device {
            self.env.audio.set_device(&config.sound_device);
            self.env.audio.play_click();
        }
        if config.directional_audio != old.directional_audio {
            // Vanilla reloads the sound engine, then clicks.
            self.env.audio.set_directional(config.directional_audio);
            self.env.audio.play_click();
        }
        // `Options.createSoundSliderOptionInstance`: outside a world the
        // slider's source is heard (`SoundPreviewHandler.preview`).
        if self.world.is_none()
            && let Some(source) = crate::audio::source::SoundSource::ALL
                .into_iter()
                .find(|&s| config.sound_volumes.get(s) != old.sound_volumes.get(s))
        {
            let volume = config.sound_volumes.get(source) as f32;
            self.sound_preview.preview(&self.env.audio, source, volume);
        }
        if config.speaker_layout != old.speaker_layout {
            self.env
                .audio
                .set_channels(config.speaker_layout.channels());
            self.env.audio.play_click();
        }
        if config.sound_volumes != old.sound_volumes {
            self.env.audio.set_volumes(&config.sound_volumes);
        }
        self.env.config = config;
        if display {
            self.apply_display_options();
        }
        if rescale {
            self.resize_gui();
        }
        if let Some(world) = &mut self.world {
            world.set_chat_settings(&self.env.config.chat, &self.env.gui_assets.font);
            world.set_smooth_lighting(self.env.config.smooth_lighting);
            world.set_cutout_leaves(self.env.config.cutout_leaves);
            world.set_chunk_builder(self.env.config.chunk_builder);
            world.set_biome_blend(self.env.config.biome_blend);
        }
        self.screens.refresh_config(&self.env.config);
    }

    /// The language screen's Done with a new language.
    pub(super) fn set_language(&mut self, language: Language) {
        self.env.lang = Lang::load(&self.env.assets, language);
        self.set_config(Config {
            lang: language,
            ..self.env.config.clone()
        });
        self.screens.refresh_lang(&self.env.lang);
        if let Some(world) = &mut self.world {
            world.refresh_lang(&self.env.lang);
        }
        self.resize_gui();
        self.screens.pop_unless_last();
    }

    /// `Options.keyFullscreen`, anywhere.
    pub(super) fn toggle_fullscreen(&mut self) {
        self.set_config(Config {
            fullscreen: !self.env.config.fullscreen,
            ..self.env.config.clone()
        });
    }

    /// `FramerateLimitTracker.getFramerateLimit`.
    pub(super) fn framerate_limit(&self) -> Option<u32> {
        if let Some(run) = &self.bench {
            return run.options.fps_cap;
        }
        if self.input.occluded || self.window.is_minimized() == Some(true) {
            return Some(10);
        }
        let config = &self.env.config;
        let limit = config.framerate_limit();
        if config.inactivity_fps_limit == options::InactivityFpsLimit::Afk {
            let idle = self.input.last_input.elapsed();
            if idle > Duration::from_secs(600) {
                return Some(10);
            }
            if idle > Duration::from_secs(60) {
                return Some(limit.map_or(30, |l| l.min(30)));
            }
        }
        if self.world.is_none() {
            return Some(limit.map_or(60, |l| l.min(60)));
        }
        limit
    }
}
