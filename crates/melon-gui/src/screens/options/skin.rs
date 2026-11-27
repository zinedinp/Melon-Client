//! Vanilla `SkinCustomizationScreen`.

use crate::i18n::Lang;
use crate::screen::{ConfigChange, ScreenContext};
use crate::screens::options::{on_off_label, value_label};
use crate::settings::options::Config;
use crate::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};
use crate::world::entity::player::PlayerModelPart;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinOption {
    Part(PlayerModelPart),
    MainHand,
}

static SECTIONS: [Section<SkinOption>; 1] = [Section {
    header: None,
    options: &[
        SkinOption::Part(PlayerModelPart::Cape),
        SkinOption::Part(PlayerModelPart::Jacket),
        SkinOption::Part(PlayerModelPart::LeftSleeve),
        SkinOption::Part(PlayerModelPart::RightSleeve),
        SkinOption::Part(PlayerModelPart::LeftPantsLeg),
        SkinOption::Part(PlayerModelPart::RightPantsLeg),
        SkinOption::Part(PlayerModelPart::Hat),
        SkinOption::MainHand,
    ],
}];

impl OptionEntry for SkinOption {
    fn press(self, _ctx: &ScreenContext) -> ConfigChange {
        Box::new(move |config| match self {
            Self::Part(part) => {
                let shown = config.model_parts.shows(part);
                config.model_parts.set(part, !shown);
            }
            Self::MainHand => config.main_hand = config.main_hand.opposite(),
        })
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        match self {
            Self::Part(part) => {
                on_off_label(lang, part.caption_key(), config.model_parts.shows(part))
            }
            Self::MainHand => value_label(
                lang,
                "options.mainHand",
                lang.get(config.main_hand.caption_key()),
            ),
        }
    }
}

/// `SkinCustomizationScreen`.
pub fn skin_customization_screen(ctx: &ScreenContext) -> OptionsSubScreen<SkinOption> {
    OptionsSubScreen::new("options.skinCustomisation.title", &SECTIONS, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_has_its_button_then_main_hand() {
        let options = SECTIONS[0].options;
        assert_eq!(options.len(), PlayerModelPart::ALL.len() + 1);
        for (option, part) in options.iter().zip(PlayerModelPart::ALL) {
            assert_eq!(*option, SkinOption::Part(part));
        }
        assert_eq!(options.last(), Some(&SkinOption::MainHand));
    }
}
