//! The music options: how often songs play and when the "now playing"

use melon_core::cycle::Cycle;

/// `MusicManager.MusicFrequency`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MusicFrequency {
    #[default]
    Default,
    Frequent,
    Constant,
}

impl MusicFrequency {
    /// The `options.txt` value (`StringRepresentable` names).
    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::Frequent => "FREQUENT",
            Self::Constant => "CONSTANT",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.id() == id)
    }

    pub fn caption(self) -> &'static str {
        match self {
            Self::Default => "options.music_frequency.default",
            Self::Frequent => "options.music_frequency.frequent",
            Self::Constant => "options.music_frequency.constant",
        }
    }

    /// `maxFrequency` in minutes: the longest wait between songs.
    pub fn max_minutes(self) -> u32 {
        match self {
            Self::Default => 20,
            Self::Frequent => 10,
            Self::Constant => 0,
        }
    }
}

impl Cycle for MusicFrequency {
    const ALL: &'static [Self] = &[Self::Default, Self::Frequent, Self::Constant];
}

/// `MusicToastDisplayState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MusicToast {
    #[default]
    Never,
    Pause,
    PauseAndToast,
}

impl MusicToast {
    pub fn id(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Pause => "pause",
            Self::PauseAndToast => "pause_and_toast",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.id() == id)
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Never => "options.musicToast.never",
            Self::Pause => "options.musicToast.pauseMenu",
            Self::PauseAndToast => "options.musicToast.pauseMenuAndToast",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::Never => "options.musicToast.never.tooltip",
            Self::Pause => "options.musicToast.pauseMenu.tooltip",
            Self::PauseAndToast => "options.musicToast.pauseMenuAndToast.tooltip",
        }
    }
}

impl Cycle for MusicToast {
    const ALL: &'static [Self] = &[Self::Never, Self::Pause, Self::PauseAndToast];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_cycle() {
        for v in MusicFrequency::ALL {
            assert_eq!(MusicFrequency::from_id(v.id()), Some(*v));
        }
        for v in MusicToast::ALL {
            assert_eq!(MusicToast::from_id(v.id()), Some(*v));
        }
        assert_eq!(MusicFrequency::from_id("nope"), None);
        assert_eq!(MusicFrequency::Constant.next(), MusicFrequency::Default);
        assert_eq!(MusicToast::Never.next(), MusicToast::Pause);
    }
}
