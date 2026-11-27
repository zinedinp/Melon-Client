//! `SoundPreviewHandler`: moving a volume slider outside a world plays a
//! sound of that source at the slider's volume, so it can be heard; a new
//! one only once the last has ended, and moving another slider stops it.

use super::source::SoundSource;
use super::stats::Playing;
use super::{AudioEngine, SoundInstance};
use std::sync::Arc;

/// The sound each source's slider plays; none for master, music and voice.
pub fn preview_event(source: SoundSource) -> Option<&'static str> {
    Some(match source {
        SoundSource::Records => "block.note_block.guitar",
        SoundSource::Weather => "entity.lightning_bolt.thunder",
        SoundSource::Blocks => "block.grass.place",
        SoundSource::Hostile => "entity.zombie.ambient",
        SoundSource::Neutral => "entity.cow.ambient",
        SoundSource::Players => "entity.generic.eat",
        SoundSource::Ambient => "ambient.cave",
        SoundSource::Ui => "ui.button.click",
        SoundSource::Master | SoundSource::Music | SoundSource::Voice => return None,
    })
}

#[derive(Default)]
pub struct SoundPreview {
    active: Option<Arc<Playing>>,
    previous: Option<SoundSource>,
}

impl SoundPreview {
    /// `SoundPreviewHandler.preview`.
    pub fn preview(&mut self, audio: &AudioEngine, source: SoundSource, volume: f32) {
        // `stopOtherCategoryPreview`.
        if self.previous != Some(source) {
            self.previous = Some(source);
            if let Some(active) = &self.active {
                active.stop();
            }
        }
        if self.active.as_ref().is_some_and(|a| a.is_active()) {
            return;
        }
        if let Some(event) = preview_event(source) {
            // `SimpleSoundInstance.forUI(sound, 1, volume)`.
            let sound = SoundInstance::of(event).on(SoundSource::Ui).volume(volume);
            self.active = audio.play_watched(&sound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_silent_sliders_play_nothing() {
        assert_eq!(preview_event(SoundSource::Master), None);
        assert_eq!(preview_event(SoundSource::Music), None);
        assert_eq!(preview_event(SoundSource::Voice), None);
        assert_eq!(
            preview_event(SoundSource::Blocks),
            Some("block.grass.place")
        );
    }
}
