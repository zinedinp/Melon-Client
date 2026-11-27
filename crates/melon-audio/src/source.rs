//! Vanilla's sound sources (`SoundSource`) and the volume of each.

/// What a sound is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundSource {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}

impl SoundSource {
    /// In the order of the enum, which is the order of the sliders.
    pub const ALL: [SoundSource; 11] = [
        SoundSource::Master,
        SoundSource::Music,
        SoundSource::Records,
        SoundSource::Weather,
        SoundSource::Blocks,
        SoundSource::Hostile,
        SoundSource::Neutral,
        SoundSource::Players,
        SoundSource::Ambient,
        SoundSource::Voice,
        SoundSource::Ui,
    ];

    /// `SoundSource.getName()`: the name in `options.txt`
    pub fn name(self) -> &'static str {
        match self {
            SoundSource::Master => "master",
            SoundSource::Music => "music",
            SoundSource::Records => "record",
            SoundSource::Weather => "weather",
            SoundSource::Blocks => "block",
            SoundSource::Hostile => "hostile",
            SoundSource::Neutral => "neutral",
            SoundSource::Players => "player",
            SoundSource::Ambient => "ambient",
            SoundSource::Voice => "voice",
            SoundSource::Ui => "ui",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.name() == name)
    }

    /// The slider's caption: `soundCategory.<name>`.
    pub fn lang_key(self) -> &'static str {
        match self {
            SoundSource::Master => "soundCategory.master",
            SoundSource::Music => "soundCategory.music",
            SoundSource::Records => "soundCategory.record",
            SoundSource::Weather => "soundCategory.weather",
            SoundSource::Blocks => "soundCategory.block",
            SoundSource::Hostile => "soundCategory.hostile",
            SoundSource::Neutral => "soundCategory.neutral",
            SoundSource::Players => "soundCategory.player",
            SoundSource::Ambient => "soundCategory.ambient",
            SoundSource::Voice => "soundCategory.voice",
            SoundSource::Ui => "soundCategory.ui",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The source a sound event belongs to by its name, for sounds the server names without
    /// one.
    pub fn for_event(event: &str) -> SoundSource {
        let event = event.strip_prefix("minecraft:").unwrap_or(event);
        let starts = |prefix: &str| event.starts_with(prefix);
        if starts("ui.") {
            SoundSource::Ui
        } else if starts("music_disc.") || starts("block.note_block.") || starts("block.jukebox.") {
            SoundSource::Records
        } else if starts("music.") {
            SoundSource::Music
        } else if starts("weather.") || starts("entity.lightning_bolt.") {
            SoundSource::Weather
        } else if starts("ambient.") {
            SoundSource::Ambient
        } else if starts("entity.player.") || starts("item.") {
            SoundSource::Players
        } else if starts("block.") {
            SoundSource::Blocks
        } else if starts("entity.") {
            SoundSource::Neutral
        } else {
            SoundSource::Master
        }
    }
}

/// The volume of every source, `0..=1` (`Options.soundSourceVolumes`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volumes([f64; 11]);

impl Default for Volumes {
    /// Everything at full volume, as vanilla starts.
    fn default() -> Self {
        Self([1.0; 11])
    }
}

impl Volumes {
    pub fn get(&self, source: SoundSource) -> f64 {
        self.0[source.index()]
    }

    pub fn set(&mut self, source: SoundSource, volume: f64) {
        self.0[source.index()] = volume.clamp(0.0, 1.0);
    }

    /// `Options.getFinalSoundSourceVolume`.
    pub fn final_volume(&self, source: SoundSource) -> f32 {
        let own = self.get(source);
        if source == SoundSource::Master {
            own as f32
        } else {
            (own * self.get(SoundSource::Master)) as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_options_file() {
        assert_eq!(SoundSource::Records.name(), "record");
        assert_eq!(SoundSource::Blocks.name(), "block");
        for source in SoundSource::ALL {
            assert_eq!(SoundSource::from_name(source.name()), Some(source));
            assert_eq!(
                source.lang_key(),
                format!("soundCategory.{}", source.name())
            );
        }
        assert_eq!(SoundSource::from_name("nope"), None);
    }

    #[test]
    fn a_source_is_as_loud_as_it_and_the_master_allow() {
        let mut volumes = Volumes::default();
        volumes.set(SoundSource::Master, 0.5);
        volumes.set(SoundSource::Music, 0.4);
        assert_eq!(volumes.final_volume(SoundSource::Master), 0.5);
        assert!((volumes.final_volume(SoundSource::Music) - 0.2).abs() < 1e-6);
        assert_eq!(volumes.final_volume(SoundSource::Ui), 0.5);
        volumes.set(SoundSource::Ui, 7.0);
        assert_eq!(volumes.get(SoundSource::Ui), 1.0);
    }

    #[test]
    fn events_default_to_the_source_vanilla_plays_them_on() {
        let source = SoundSource::for_event;
        assert_eq!(source("ui.button.click"), SoundSource::Ui);
        assert_eq!(source("minecraft:music.menu"), SoundSource::Music);
        assert_eq!(source("music_disc.cat"), SoundSource::Records);
        assert_eq!(source("block.note_block.harp"), SoundSource::Records);
        assert_eq!(source("block.stone.break"), SoundSource::Blocks);
        assert_eq!(source("weather.rain"), SoundSource::Weather);
        assert_eq!(source("ambient.cave"), SoundSource::Ambient);
        assert_eq!(source("entity.player.hurt"), SoundSource::Players);
        assert_eq!(source("entity.cow.ambient"), SoundSource::Neutral);
        assert_eq!(source("something.else"), SoundSource::Master);
    }
}
