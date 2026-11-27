//! Vanilla `AccessibilityOptionsScreen`, cut to the options Melon has. Most
//! of them live on another screen too (Chat, Music & Sounds, Video) and are
//! the same entries here; View Bobbing and the high contrast block outline
//! are only here, as in vanilla.

use crate::gui::screen::{ConfigChange, ScreenAction, ScreenContext};
use crate::gui::screens::options::chat::ChatOption;
use crate::gui::screens::options::controls::controls_screen;
use crate::gui::screens::options::on_off_label;
use crate::gui::screens::options::sounds::SoundOption;
use crate::gui::screens::options::video::VideoOption;
use crate::gui::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};
use crate::i18n::Lang;
use crate::settings::options::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibilityOption {
    Chat(ChatOption),
    Sound(SoundOption),
    Video(VideoOption),
    /// The "Controls..." button beside the narrator.
    Controls,
    BobView,
    HighContrastBlockOutline,
}

use AccessibilityOption as A;

// TODO(accessibility): vanilla's options Melon doesn't have yet, in its order:
// `highContrast` (the resource pack), `menuBackgroundBlurriness`,
// `notificationDisplayTime`, `darknessEffectScale`, `glintSpeed`,
// `glintStrength`, `hideLightningFlash`, `darkMojangStudiosBackground`,
// `panoramaSpeed`, `hideSplashTexts`, `narratorHotkey`,
// `rotateWithMinecart`; and the footer's "Accessibility Guide" link
// (`options.accessibility.link`) beside Done.
static SECTIONS: [Section<AccessibilityOption>; 1] = [Section {
    header: None,
    options: &[
        A::Chat(ChatOption::Narrator),
        A::Controls,
        A::Sound(SoundOption::Subtitles),
        A::Chat(ChatOption::BackgroundOpacity),
        A::Chat(ChatOption::BackgroundWhere),
        A::Chat(ChatOption::Opacity),
        A::Chat(ChatOption::LineSpacing),
        A::Chat(ChatOption::Delay),
        A::BobView,
        A::Video(VideoOption::DistortionEffects),
        A::Video(VideoOption::FovEffects),
        A::Video(VideoOption::DamageTilt),
        A::HighContrastBlockOutline,
    ],
}];

impl OptionEntry for AccessibilityOption {
    fn slider(self, config: &Config) -> Option<(u32, u32)> {
        match self {
            A::Chat(o) => o.slider(config),
            A::Sound(o) => o.slider(config),
            A::Video(o) => o.slider(config),
            _ => None,
        }
    }

    fn set_step(self, step: u32) -> ConfigChange {
        match self {
            A::Chat(o) => o.set_step(step),
            A::Sound(o) => o.set_step(step),
            A::Video(o) => o.set_step(step),
            _ => Box::new(|_| {}),
        }
    }

    fn press(self, ctx: &ScreenContext) -> ConfigChange {
        match self {
            A::Chat(o) => o.press(ctx),
            A::Sound(o) => o.press(ctx),
            A::Video(o) => o.press(ctx),
            A::Controls => Box::new(|_| {}),
            A::BobView => Box::new(|config| config.bob_view = !config.bob_view),
            A::HighContrastBlockOutline => Box::new(|config| {
                config.high_contrast_block_outline = !config.high_contrast_block_outline;
            }),
        }
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        match self {
            A::Chat(o) => o.label(config, lang),
            A::Sound(o) => o.label(config, lang),
            A::Video(o) => o.label(config, lang),
            A::Controls => lang.get("options.controls").to_owned(),
            A::BobView => on_off_label(lang, "options.viewBobbing", config.bob_view),
            A::HighContrastBlockOutline => on_off_label(
                lang,
                "options.accessibility.high_contrast_block_outline",
                config.high_contrast_block_outline,
            ),
        }
    }

    fn enabled(self) -> bool {
        match self {
            A::Chat(o) => o.enabled(),
            A::Sound(o) => o.enabled(),
            A::Video(o) => o.enabled(),
            _ => true,
        }
    }

    fn open(self, ctx: &ScreenContext) -> Option<ScreenAction> {
        match self {
            A::Chat(o) => o.open(ctx),
            A::Sound(o) => o.open(ctx),
            A::Video(o) => o.open(ctx),
            A::Controls => Some(ScreenAction::open(controls_screen(ctx))),
            _ => None,
        }
    }

    fn tooltip(self, config: &Config) -> Option<&'static str> {
        match self {
            A::Chat(o) => o.tooltip(config),
            A::Sound(o) => o.tooltip(config),
            A::Video(o) => o.tooltip(config),
            A::HighContrastBlockOutline => {
                Some("options.accessibility.high_contrast_block_outline.tooltip")
            }
            _ => None,
        }
    }
}

/// `AccessibilityOptionsScreen`.
pub fn accessibility_options_screen(ctx: &ScreenContext) -> OptionsSubScreen<AccessibilityOption> {
    OptionsSubScreen::new("options.accessibility.title", &SECTIONS, ctx)
}
