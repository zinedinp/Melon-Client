//! Melon's own settings, the ones vanilla has no screen for.

use melon_core::cycle::Cycle;
use melon_assets::i18n::Lang;
use crate::screen::{ConfigChange, ScreenContext};
use crate::screens::options::value_label;
use melon_settings::options::Config;
use crate::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MelonOption {
    PresentMode,
    SpeakerLayout,
}

static SECTIONS: [Section<MelonOption>; 2] = [
    Section {
        header: Some("melon.options.presentation.header"),
        options: &[MelonOption::PresentMode],
    },
    Section {
        header: Some("melon.options.audio.header"),
        options: &[MelonOption::SpeakerLayout],
    },
];

impl OptionEntry for MelonOption {
    fn press(self, _ctx: &ScreenContext) -> ConfigChange {
        Box::new(move |config| match self {
            Self::PresentMode => config.present_mode = config.present_mode.next(),
            Self::SpeakerLayout => config.speaker_layout = config.speaker_layout.next(),
        })
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        match self {
            Self::PresentMode => value_label(
                lang,
                "melon.options.presentMode",
                lang.get(config.present_mode.key()),
            ),
            Self::SpeakerLayout => value_label(
                lang,
                "melon.options.speakerLayout",
                lang.get(config.speaker_layout.key()),
            ),
        }
    }

    fn tooltip(self, config: &Config) -> Option<&'static str> {
        match self {
            Self::PresentMode => Some(config.present_mode.tooltip_key()),
            Self::SpeakerLayout => Some("melon.options.speakerLayout.tooltip"),
        }
    }
}

/// Melon's settings screen.
pub fn melon_settings_screen(ctx: &ScreenContext) -> OptionsSubScreen<MelonOption> {
    OptionsSubScreen::new("melon.options.title", &SECTIONS, ctx)
}
