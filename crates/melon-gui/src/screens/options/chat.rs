//! Vanilla `ChatOptionsScreen`.

use crate::cycle::Cycle;
use crate::gui::screen::{ConfigChange, ScreenContext};
use crate::gui::screens::options::{on_off_label, value_label};
use crate::gui::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};
use crate::i18n::Lang;
use crate::settings::options::{ChatSettings, Config};

/// Unit sliders move in hundredths (their labels show whole percent).
const UNIT_STEPS: u32 = 100;

/// One option of the list, in vanilla's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatOption {
    Visibility,
    Colors,
    Links,
    LinksPrompt,
    Opacity,
    BackgroundOpacity,
    BackgroundWhere,
    Scale,
    LineSpacing,
    Delay,
    Width,
    HeightFocused,
    HeightUnfocused,
    Narrator,
    AutoSuggestions,
    HideMatchedNames,
    ReducedDebugInfo,
    OnlyShowSecureChat,
    SaveDrafts,
}

const OPTIONS: &[ChatOption] = &[
    ChatOption::Visibility,
    ChatOption::Colors,
    ChatOption::Links,
    ChatOption::LinksPrompt,
    ChatOption::Opacity,
    ChatOption::BackgroundOpacity,
    ChatOption::BackgroundWhere,
    ChatOption::Scale,
    ChatOption::LineSpacing,
    ChatOption::Delay,
    ChatOption::Width,
    ChatOption::HeightFocused,
    ChatOption::HeightUnfocused,
    ChatOption::Narrator,
    ChatOption::AutoSuggestions,
    ChatOption::HideMatchedNames,
    ChatOption::ReducedDebugInfo,
    ChatOption::OnlyShowSecureChat,
    ChatOption::SaveDrafts,
];

/// One section without a header.
static SECTIONS: [Section<ChatOption>; 1] = [Section {
    header: None,
    options: OPTIONS,
}];

impl ChatOption {
    /// The unit value a slider option holds, `None` for buttons.
    fn unit(self, chat: &ChatSettings) -> Option<f64> {
        Some(match self {
            Self::Opacity => chat.opacity,
            Self::BackgroundOpacity => chat.background_opacity,
            Self::Scale => chat.scale,
            Self::LineSpacing => chat.line_spacing,
            Self::Delay => chat.delay / 6.0,
            Self::Width => chat.width,
            Self::HeightFocused => chat.height_focused,
            Self::HeightUnfocused => chat.height_unfocused,
            _ => return None,
        })
    }

    fn steps(self) -> u32 {
        if self == Self::Delay { 60 } else { UNIT_STEPS }
    }

    fn apply_step(self, chat: &mut ChatSettings, step: u32) {
        let unit = f64::from(step) / f64::from(self.steps());
        match self {
            Self::Opacity => chat.opacity = unit,
            Self::BackgroundOpacity => chat.background_opacity = unit,
            Self::Scale => chat.scale = unit,
            Self::LineSpacing => chat.line_spacing = unit,
            Self::Delay => chat.delay = f64::from(step) / 10.0,
            Self::Width => chat.width = unit,
            Self::HeightFocused => chat.height_focused = unit,
            Self::HeightUnfocused => chat.height_unfocused = unit,
            _ => {}
        }
    }

    /// Clicks a button option: toggles or cycles it.
    fn apply_press(self, chat: &mut ChatSettings) {
        let flag = match self {
            Self::Visibility => {
                chat.visibility = chat.visibility.next();
                return;
            }
            Self::Colors => &mut chat.colors,
            Self::Links => &mut chat.links,
            Self::BackgroundWhere => &mut chat.background_for_chat_only,
            Self::LinksPrompt => &mut chat.links_prompt,
            Self::AutoSuggestions => &mut chat.auto_suggestions,
            Self::HideMatchedNames => &mut chat.hide_matched_names,
            Self::ReducedDebugInfo => &mut chat.reduced_debug_info,
            Self::OnlyShowSecureChat => &mut chat.only_show_secure_chat,
            Self::SaveDrafts => &mut chat.save_drafts,
            _ => return,
        };
        *flag = !*flag;
    }

    /// The caption functions of `Options`.
    fn caption(self, chat: &ChatSettings, lang: &Lang) -> String {
        let percent = |caption: &'static str, value: f64| {
            lang.format(
                "options.percent_value",
                &[
                    lang.get(caption),
                    &((value * 100.0).round() as i64).to_string(),
                ],
            )
        };
        let pixels = |caption: &'static str, pixels: i64| {
            lang.format(
                "options.pixel_value",
                &[lang.get(caption), &pixels.to_string()],
            )
        };
        let height = |pct: f64| (pct * 160.0 + 20.0).floor() as i64;
        match self {
            Self::Visibility => value_label(
                lang,
                "options.chat.visibility",
                lang.get(chat.visibility.key()),
            ),
            Self::Colors => on_off_label(lang, "options.chat.color", chat.colors),
            Self::Links => on_off_label(lang, "options.chat.links", chat.links),
            Self::LinksPrompt => on_off_label(lang, "options.chat.links.prompt", chat.links_prompt),
            Self::Opacity => percent("options.chat.opacity", chat.opacity * 0.9 + 0.1),
            Self::BackgroundOpacity => percent(
                "options.accessibility.text_background_opacity",
                chat.background_opacity,
            ),
            Self::BackgroundWhere => value_label(
                lang,
                "options.accessibility.text_background",
                lang.get(if chat.background_for_chat_only {
                    "options.accessibility.text_background.chat"
                } else {
                    "options.accessibility.text_background.everywhere"
                }),
            ),
            Self::Scale => {
                if chat.scale == 0.0 {
                    on_off_label(lang, "options.chat.scale", false)
                } else {
                    percent("options.chat.scale", chat.scale)
                }
            }
            Self::LineSpacing => percent("options.chat.line_spacing", chat.line_spacing),
            Self::Delay => {
                if chat.delay <= 0.0 {
                    lang.get("options.chat.delay_none").to_owned()
                } else {
                    lang.format("options.chat.delay", &[&format!("{:.1}", chat.delay)])
                }
            }
            Self::Width => pixels(
                "options.chat.width",
                (chat.width * 280.0 + 40.0).floor() as i64,
            ),
            Self::HeightFocused => {
                pixels("options.chat.height.focused", height(chat.height_focused))
            }
            Self::HeightUnfocused => pixels(
                "options.chat.height.unfocused",
                height(chat.height_unfocused),
            ),
            Self::Narrator => value_label(
                lang,
                "options.narrator",
                lang.get("options.narrator.notavailable"),
            ),
            Self::AutoSuggestions => {
                on_off_label(lang, "options.autoSuggestCommands", chat.auto_suggestions)
            }
            Self::HideMatchedNames => {
                on_off_label(lang, "options.hideMatchedNames", chat.hide_matched_names)
            }
            Self::ReducedDebugInfo => {
                on_off_label(lang, "options.reducedDebugInfo", chat.reduced_debug_info)
            }
            Self::OnlyShowSecureChat => on_off_label(
                lang,
                "options.onlyShowSecureChat",
                chat.only_show_secure_chat,
            ),
            Self::SaveDrafts => on_off_label(lang, "options.chat.drafts", chat.save_drafts),
        }
    }
}

impl OptionEntry for ChatOption {
    fn slider(self, config: &Config) -> Option<(u32, u32)> {
        let unit = self.unit(&config.chat)?;
        Some((
            self.steps(),
            (unit * f64::from(self.steps())).round() as u32,
        ))
    }

    fn set_step(self, step: u32) -> ConfigChange {
        Box::new(move |config| self.apply_step(&mut config.chat, step))
    }

    fn press(self, _ctx: &ScreenContext) -> ConfigChange {
        Box::new(move |config| self.apply_press(&mut config.chat))
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        self.caption(&config.chat, lang)
    }

    /// The narrator isn't available in Melon.
    fn enabled(self) -> bool {
        self != ChatOption::Narrator
    }

    fn tooltip(self, _config: &Config) -> Option<&'static str> {
        match self {
            Self::HideMatchedNames => Some("options.hideMatchedNames.tooltip"),
            Self::OnlyShowSecureChat => Some("options.onlyShowSecureChat.tooltip"),
            Self::SaveDrafts => Some("options.chat.drafts.tooltip"),
            _ => None,
        }
    }
}

/// `ChatOptionsScreen`.
pub fn chat_options_screen(ctx: &ScreenContext) -> OptionsSubScreen<ChatOption> {
    OptionsSubScreen::new("options.chat.title", &SECTIONS, ctx)
}
