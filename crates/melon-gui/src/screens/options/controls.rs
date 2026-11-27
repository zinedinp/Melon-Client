//! Vanilla `ControlsScreen`, cut to the options Melon has.

use crate::screen::{ConfigChange, ScreenAction, ScreenContext};
use crate::screens::options::keybinds::KeyBindsScreen;
use crate::screens::options::mouse::mouse_settings_screen;
use crate::screens::options::{on_off_label, value_label};
use crate::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};
use melon_assets::i18n::Lang;
use melon_settings::input;
use melon_settings::options::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlOption {
    MouseSettings,
    KeyBinds,
    ToggleCrouch,
    ToggleSprint,
    AutoJump,
    SprintWindow,
}

static SECTIONS: [Section<ControlOption>; 1] = [Section {
    header: None,
    options: &[
        ControlOption::MouseSettings,
        ControlOption::KeyBinds,
        ControlOption::ToggleCrouch,
        ControlOption::ToggleSprint,
        ControlOption::AutoJump,
        ControlOption::SprintWindow,
    ],
}];

impl ControlOption {
    fn steps(self, config: &Config) -> Option<(u32, u32)> {
        match self {
            Self::SprintWindow => Some((
                u32::from(input::SPRINT_WINDOW_MAX),
                u32::from(config.input.sprint_window),
            )),
            _ => None,
        }
    }

    fn apply_step(self, config: &mut Config, step: u32) {
        if self == Self::SprintWindow {
            config.input.sprint_window = step as u8;
        }
    }

    fn apply_press(self, config: &mut Config) {
        match self {
            Self::ToggleCrouch => config.input.toggle_crouch = !config.input.toggle_crouch,
            Self::ToggleSprint => config.input.toggle_sprint = !config.input.toggle_sprint,
            Self::AutoJump => config.input.auto_jump = !config.input.auto_jump,
            _ => {}
        }
    }

    fn caption(self, config: &Config, lang: &Lang) -> String {
        match self {
            Self::MouseSettings => lang.get("options.mouse_settings").to_owned(),
            Self::KeyBinds => lang.get("controls.keybinds").to_owned(),
            Self::ToggleCrouch => hold_toggle_label(lang, "key.sneak", config.input.toggle_crouch),
            Self::ToggleSprint => hold_toggle_label(lang, "key.sprint", config.input.toggle_sprint),
            Self::AutoJump => on_off_label(lang, "options.autoJump", config.input.auto_jump),
            Self::SprintWindow => {
                let value = if config.input.sprint_window == 0 {
                    lang.get("options.off").to_owned()
                } else {
                    lang.format("options.value", &[&config.input.sprint_window.to_string()])
                };
                value_label(lang, "options.sprintWindow", &value)
            }
        }
    }
}

/// `Options.KEY_HOLD` / `KEY_TOGGLE`: "Sneak: Hold" / "Sneak: Toggle".
fn hold_toggle_label(lang: &Lang, caption: &'static str, toggle: bool) -> String {
    let state = lang.get(if toggle {
        "options.key.toggle"
    } else {
        "options.key.hold"
    });
    value_label(lang, caption, state)
}

impl OptionEntry for ControlOption {
    fn slider(self, config: &Config) -> Option<(u32, u32)> {
        self.steps(config)
    }

    fn set_step(self, step: u32) -> ConfigChange {
        Box::new(move |config| self.apply_step(config, step))
    }

    fn press(self, _ctx: &ScreenContext) -> ConfigChange {
        Box::new(move |config| self.apply_press(config))
    }

    fn open(self, ctx: &ScreenContext) -> Option<ScreenAction> {
        Some(match self {
            Self::MouseSettings => ScreenAction::open(mouse_settings_screen(ctx)),
            Self::KeyBinds => ScreenAction::open(KeyBindsScreen::new(ctx)),
            _ => return None,
        })
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        self.caption(config, lang)
    }

    fn tooltip(self, _config: &Config) -> Option<&'static str> {
        (self == Self::SprintWindow).then_some("options.sprintWindow.tooltip")
    }
}

/// `ControlsScreen`.
pub fn controls_screen(ctx: &ScreenContext) -> OptionsSubScreen<ControlOption> {
    OptionsSubScreen::new("controls.title", &SECTIONS, ctx)
}
