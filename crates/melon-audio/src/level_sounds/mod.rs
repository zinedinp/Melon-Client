//! What each level event sounds like (`LevelEventHandler.levelEvent`).

mod data;

use super::SoundInstance;
use super::source::SoundSource;

/// How a pitch is rolled; `r` is a fresh `random.nextFloat()` each time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pitch {
    Fixed(f32),
    /// `base + (r - r) * spread`.
    Spread(f32, f32),
    /// `base + r * range`.
    Up(f32, f32),
    /// `numerator / (base + r * range)`.
    Recip(f32, f32, f32),
}

impl Pitch {
    pub fn roll(self, mut r: impl FnMut() -> f32) -> f32 {
        match self {
            Pitch::Fixed(pitch) => pitch,
            Pitch::Spread(base, spread) => base + (r() - r()) * spread,
            Pitch::Up(base, range) => base + r() * range,
            Pitch::Recip(numerator, base, range) => numerator / (base + r() * range),
        }
    }
}

/// One level event's sound.
#[derive(Clone, Copy, Debug)]
pub struct LevelSound {
    pub event: i32,
    pub sound: &'static str,
    pub source: SoundSource,
    pub volume: f32,
    pub pitch: Pitch,
    /// Heard as it is, without a place (`forLocalAmbience`).
    pub local: bool,
}

impl LevelSound {
    pub fn of(event: i32) -> Option<&'static LevelSound> {
        let table = data::LEVEL_SOUNDS;
        table
            .binary_search_by_key(&event, |sound| sound.event)
            .ok()
            .map(|at| &table[at])
    }

    /// The sound with its pitch rolled, heard from `at` unless it is local.
    pub fn instance(&self, at: [f64; 3]) -> SoundInstance<'static> {
        let sound = SoundInstance::of(self.sound)
            .on(self.source)
            .volume(self.volume)
            .pitch(self.pitch.roll(rand::random::<f32>));
        if self.local { sound } else { sound.at(at) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_for_the_search() {
        assert!(
            data::LEVEL_SOUNDS
                .windows(2)
                .all(|p| p[0].event < p[1].event)
        );
    }

    #[test]
    fn events_have_their_vanilla_sounds() {
        let anvil = LevelSound::of(1030).unwrap();
        assert_eq!(anvil.sound, "block.anvil.use");
        assert_eq!(anvil.source, SoundSource::Blocks);
        assert_eq!(anvil.pitch, Pitch::Up(0.9, 0.1));
        let ghast = LevelSound::of(1016).unwrap();
        assert_eq!((ghast.sound, ghast.volume), ("entity.ghast.shoot", 10.0));
        assert!(LevelSound::of(1032).unwrap().local);
        assert!(LevelSound::of(2001).is_none());
    }

    #[test]
    fn pitches_are_rolled_like_vanilla() {
        let mut values = [0.75, 0.25].into_iter();
        assert!((Pitch::Spread(1.0, 0.2).roll(|| values.next().unwrap()) - 1.1).abs() < 1e-6);
        assert_eq!(Pitch::Up(0.9, 0.1).roll(|| 0.5), 0.95);
        assert_eq!(Pitch::Recip(0.4, 0.8, 0.4).roll(|| 0.0), 0.5);
    }

    #[test]
    fn every_event_exists_in_sounds_json() {
        let Ok(json) =
            std::fs::read_to_string(melon_assets::assets::default_root().join("minecraft/sounds.json"))
        else {
            return;
        };
        let events: serde_json::Value = serde_json::from_str(&json).unwrap();
        for sound in data::LEVEL_SOUNDS {
            assert!(events.get(sound.sound).is_some(), "{}", sound.sound);
        }
    }
}
