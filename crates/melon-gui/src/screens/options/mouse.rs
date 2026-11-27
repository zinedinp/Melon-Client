//! Vanilla `MouseSettingsScreen`, cut to the options that change looking.

use crate::gui::screen::{ConfigChange, ScreenContext};
use crate::gui::screens::options::{on_off_label, value_label};
use crate::gui::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};
use crate::i18n::Lang;
use crate::settings::input;
use crate::settings::options::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseOption {
    Sensitivity,
    InvertY,
}

static SECTIONS: [Section<MouseOption>; 1] = [Section {
    header: None,
    options: &[MouseOption::Sensitivity, MouseOption::InvertY],
}];

impl MouseOption {
    fn steps(self, config: &Config) -> Option<(u32, u32)> {
        match self {
            Self::Sensitivity => Some((
                input::SENSITIVITY_STEPS,
                (config.input.sensitivity * f64::from(input::SENSITIVITY_STEPS)).round() as u32,
            )),
            Self::InvertY => None,
        }
    }

    fn apply_step(self, config: &mut Config, step: u32) {
        if self == Self::Sensitivity {
            config.input.sensitivity = f64::from(step) / f64::from(input::SENSITIVITY_STEPS);
        }
    }

    fn caption(self, config: &Config, lang: &Lang) -> String {
        match self {
            Self::Sensitivity => sensitivity_label(lang, config.input.sensitivity),
            Self::InvertY => on_off_label(lang, "options.invertMouseY", config.input.invert_y),
        }
    }
}

/// `Options.sensitivity`'s caption: "Sensitivity: *YOURS NOT TO REASON WHY*"
fn sensitivity_label(lang: &Lang, value: f64) -> String {
    let text = if value <= 0.0 {
        lang.get("options.sensitivity.min").to_owned()
    } else if value >= 1.0 {
        lang.get("options.sensitivity.max").to_owned()
    } else {
        let percent = ((value * 2.0 * 100.0).round() as i32).to_string();
        return lang.format(
            "options.percent_value",
            &[lang.get("options.sensitivity"), &percent],
        );
    };
    value_label(lang, "options.sensitivity", &text)
}

impl OptionEntry for MouseOption {
    fn slider(self, config: &Config) -> Option<(u32, u32)> {
        self.steps(config)
    }

    fn set_step(self, step: u32) -> ConfigChange {
        Box::new(move |config| self.apply_step(config, step))
    }

    fn press(self, _ctx: &ScreenContext) -> ConfigChange {
        Box::new(|config| config.input.invert_y = !config.input.invert_y)
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        self.caption(config, lang)
    }
}

/// `MouseSettingsScreen`.
pub fn mouse_settings_screen(ctx: &ScreenContext) -> OptionsSubScreen<MouseOption> {
    OptionsSubScreen::new("options.mouse_settings.title", &SECTIONS, ctx)
}
