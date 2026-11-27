//! Vanilla `SoundOptionsScreen`.

use melon_audio::source::SoundSource;
use melon_core::cycle::Cycle;
use melon_assets::i18n::Lang;
use crate::screen::{ConfigChange, ScreenContext};
use crate::screens::options::{on_off_label, value_label};
use melon_settings::options::Config;
use crate::widgets::options_list::{OptionEntry, OptionsSubScreen, Section};

/// One step per percent.
const STEPS: u32 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundOption {
    /// A volume slider.
    Volume(SoundSource),
    Device,
    Subtitles,
    DirectionalAudio,
    MusicFrequency,
    MusicToast,
}

use SoundOption::*;

static SECTIONS: [Section<SoundOption>; 1] = [Section {
    header: None,
    options: &[
        Volume(SoundSource::Master),
        Volume(SoundSource::Music),
        Volume(SoundSource::Records),
        Volume(SoundSource::Weather),
        Volume(SoundSource::Blocks),
        Volume(SoundSource::Hostile),
        Volume(SoundSource::Neutral),
        Volume(SoundSource::Players),
        Volume(SoundSource::Ambient),
        Volume(SoundSource::Voice),
        Volume(SoundSource::Ui),
        Device,
        Subtitles,
        DirectionalAudio,
        MusicFrequency,
        MusicToast,
    ],
}];

/// `Options.percentValueOrOffLabel`: "Music: 40%", or "Music: OFF" at 0.
fn volume_label(lang: &Lang, source: SoundSource, volume: f64) -> String {
    let percent = (volume * f64::from(STEPS)).round() as u32;
    if percent == 0 {
        return value_label(lang, source.lang_key(), lang.get("options.off"));
    }
    lang.format(
        "options.percent_value",
        &[lang.get(source.lang_key()), &percent.to_string()],
    )
}

/// The device after `current` in the system default followed by `devices`.
fn next_device(devices: &[String], current: &str) -> String {
    let choices: Vec<&str> = std::iter::once("")
        .chain(devices.iter().map(String::as_str))
        .collect();
    let at = choices.iter().position(|d| *d == current).unwrap_or(0);
    choices[(at + 1) % choices.len()].to_owned()
}

impl OptionEntry for SoundOption {
    fn slider(self, config: &Config) -> Option<(u32, u32)> {
        let Volume(source) = self else { return None };
        let step = (config.sound_volumes.get(source) * f64::from(STEPS)).round() as u32;
        Some((STEPS, step))
    }

    fn set_step(self, step: u32) -> ConfigChange {
        Box::new(move |config| {
            if let Volume(source) = self {
                config
                    .sound_volumes
                    .set(source, f64::from(step) / f64::from(STEPS));
            }
        })
    }

    fn press(self, ctx: &ScreenContext) -> ConfigChange {
        match self {
            Volume(_) => Box::new(|_| {}),
            Device => {
                let devices = ctx.audio.devices();
                Box::new(move |config| {
                    config.sound_device = next_device(&devices, &config.sound_device);
                })
            }
            Subtitles => Box::new(|c| c.show_subtitles = !c.show_subtitles),
            DirectionalAudio => Box::new(|c| c.directional_audio = !c.directional_audio),
            MusicFrequency => Box::new(|c| c.music_frequency = c.music_frequency.next()),
            MusicToast => Box::new(|c| c.music_toast = c.music_toast.next()),
        }
    }

    fn label(self, config: &Config, lang: &Lang) -> String {
        match self {
            Volume(source) => volume_label(lang, source, config.sound_volumes.get(source)),
            Device if config.sound_device.is_empty() => {
                lang.get("options.audioDevice.default").to_owned()
            }
            Device => config.sound_device.clone(),
            Subtitles => on_off_label(lang, "options.showSubtitles", config.show_subtitles),
            DirectionalAudio => {
                on_off_label(lang, "options.directionalAudio", config.directional_audio)
            }
            MusicFrequency => value_label(
                lang,
                "options.music_frequency",
                lang.get(config.music_frequency.caption()),
            ),
            MusicToast => value_label(
                lang,
                "options.musicToast",
                lang.get(config.music_toast.text()),
            ),
        }
    }

    /// `addBig`: the master volume and the device.
    fn wide(self) -> bool {
        matches!(self, Volume(SoundSource::Master) | Device)
    }

    fn tooltip(self, config: &Config) -> Option<&'static str> {
        match self {
            Subtitles => Some("options.showSubtitles.tooltip"),
            DirectionalAudio => Some(if config.directional_audio {
                "options.directionalAudio.on.tooltip"
            } else {
                "options.directionalAudio.off.tooltip"
            }),
            MusicFrequency => Some("options.music_frequency.tooltip"),
            MusicToast => Some(config.music_toast.tooltip()),
            _ => None,
        }
    }
}

/// `SoundOptionsScreen`.
pub fn sound_options_screen(ctx: &ScreenContext) -> OptionsSubScreen<SoundOption> {
    OptionsSubScreen::new("options.sounds.title", &SECTIONS, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use melon_audio::music::MusicToast;

    #[test]
    fn a_slider_moves_its_own_source() {
        let mut config = Config::default();
        Volume(SoundSource::Music).set_step(40)(&mut config);
        assert_eq!(config.sound_volumes.get(SoundSource::Music), 0.4);
        assert_eq!(config.sound_volumes.get(SoundSource::Ui), 1.0);
        assert_eq!(Volume(SoundSource::Music).slider(&config), Some((100, 40)));
        assert_eq!(Subtitles.slider(&config), None);
    }

    #[test]
    fn captions_are_percents_or_off() {
        let lang = Lang::from_pairs(&[
            ("soundCategory.music", "Music"),
            ("options.percent_value", "%s: %s%%"),
            ("options.generic_value", "%s: %s"),
            ("options.off", "OFF"),
        ]);
        assert_eq!(volume_label(&lang, SoundSource::Music, 0.4), "Music: 40%");
        assert_eq!(volume_label(&lang, SoundSource::Music, 0.0), "Music: OFF");
    }

    #[test]
    fn the_device_button_walks_the_list_from_the_default() {
        let devices = vec!["Speakers".to_owned(), "Headset".to_owned()];
        assert_eq!(next_device(&devices, ""), "Speakers");
        assert_eq!(next_device(&devices, "Speakers"), "Headset");
        assert_eq!(next_device(&devices, "Headset"), "");
        assert_eq!(next_device(&devices, "Gone"), "Speakers");
        assert_eq!(next_device(&[], ""), "");
    }

    #[test]
    fn the_wide_rows_are_the_master_and_the_device() {
        let wide: Vec<_> = SECTIONS[0].options.iter().filter(|o| o.wide()).collect();
        assert_eq!(wide, [&Volume(SoundSource::Master), &Device]);
        assert_eq!(MusicToast::Never.next(), MusicToast::Pause);
    }
}
