//! A block's sounds (`SoundType`).

mod data;

use super::SoundInstance;
use super::source::SoundSource;

/// One of vanilla's `SoundType`s: `1.0, 1.0` and five events.
pub struct SoundType {
    pub name: &'static str,
    pub volume: f32,
    pub pitch: f32,
    pub break_sound: &'static str,
    pub step_sound: &'static str,
    pub place_sound: &'static str,
    pub hit_sound: &'static str,
    pub fall_sound: &'static str,
}

/// What a block is doing to be heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockSound {
    /// The block was broken (`LevelEvent.PARTICLES_DESTROY_BLOCK`).
    Break,
    /// A block was placed (`BlockItem.place`).
    Place,
    /// Being mined, every fourth tick (`continueDestroyBlock`).
    Hit,
    /// A footstep (`Entity.playStepSound`).
    Step,
    /// The block under a carpet or snow layer, or under water
    /// (`Entity.playMuffledStepSound`).
    MuffledStep,
    /// Landing (`LivingEntity.playBlockFallSound`).
    Fall,
}

impl SoundType {
    /// The sound type of the block with this name (`stone`, `oak_planks`);
    pub fn of(block: &str) -> &'static SoundType {
        let index = data::BLOCKS
            .binary_search_by(|(name, _)| (*name).cmp(block))
            .map_or(data::DEFAULT, |at| usize::from(data::BLOCKS[at].1));
        &data::SOUND_TYPES[index]
    }

    /// The sound to play.
    pub fn sound(&self, kind: BlockSound) -> Option<SoundInstance<'static>> {
        let (event, volume, pitch, source) = match kind {
            BlockSound::Break => (
                self.break_sound,
                (self.volume + 1.0) / 2.0,
                self.pitch * 0.8,
                SoundSource::Blocks,
            ),
            BlockSound::Place => (
                self.place_sound,
                (self.volume + 1.0) / 2.0,
                self.pitch * 0.8,
                SoundSource::Blocks,
            ),
            BlockSound::Hit => (
                self.hit_sound,
                (self.volume + 1.0) / 8.0,
                self.pitch * 0.5,
                SoundSource::Blocks,
            ),
            BlockSound::Step => (
                self.step_sound,
                self.volume * 0.15,
                self.pitch,
                SoundSource::Players,
            ),
            BlockSound::MuffledStep => (
                self.step_sound,
                self.volume * 0.05,
                self.pitch * 0.8,
                SoundSource::Players,
            ),
            BlockSound::Fall => (
                self.fall_sound,
                self.volume * 0.5,
                self.pitch * 0.75,
                SoundSource::Players,
            ),
        };
        (!event.is_empty()).then(|| {
            SoundInstance::of(event)
                .on(source)
                .volume(volume)
                .pitch(pitch)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_for_the_search() {
        assert!(data::BLOCKS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(
            data::BLOCKS
                .iter()
                .all(|&(_, t)| usize::from(t) < data::SOUND_TYPES.len())
        );
    }

    #[test]
    fn blocks_have_their_vanilla_sound_types() {
        assert_eq!(SoundType::of("stone").name, "stone");
        assert_eq!(SoundType::of("grass_block").name, "grass");
        assert_eq!(SoundType::of("dirt").name, "gravel");
        assert_eq!(SoundType::of("oak_planks").name, "wood");
        assert_eq!(SoundType::of("oak_stairs").name, "wood");
        assert_eq!(SoundType::of("white_wool").name, "wool");
        assert_eq!(SoundType::of("glass").name, "glass");
        assert_eq!(SoundType::of("anvil").name, "anvil");
        assert_eq!(SoundType::of("cobblestone").name, "stone");
        assert_eq!(SoundType::of("not_a_block").name, "stone");
    }

    #[test]
    fn each_case_is_scaled_like_vanilla() {
        let stone = SoundType::of("stone");
        let broken = stone.sound(BlockSound::Break).unwrap();
        assert_eq!(broken.event, "block.stone.break");
        assert_eq!((broken.volume, broken.pitch), (1.0, 0.8));
        assert_eq!(broken.source, SoundSource::Blocks);
        let hit = stone.sound(BlockSound::Hit).unwrap();
        assert_eq!((hit.volume, hit.pitch), (0.25, 0.5));
        let step = stone.sound(BlockSound::Step).unwrap();
        assert_eq!(
            (step.volume, step.pitch, step.source),
            (0.15, 1.0, SoundSource::Players)
        );
        let muffled = stone.sound(BlockSound::MuffledStep).unwrap();
        assert_eq!((muffled.volume, muffled.pitch), (0.05, 0.8));
        let fall = stone.sound(BlockSound::Fall).unwrap();
        assert_eq!((fall.volume, fall.pitch), (0.5, 0.75));
    }

    #[test]
    fn a_block_without_sound_plays_nothing() {
        assert!(SoundType::of("water").sound(BlockSound::Step).is_none());
    }

    #[test]
    fn every_event_exists_in_sounds_json() {
        let Ok(json) =
            std::fs::read_to_string(melon_assets::assets::default_root().join("minecraft/sounds.json"))
        else {
            return;
        };
        let events: serde_json::Value = serde_json::from_str(&json).unwrap();
        for sound_type in data::SOUND_TYPES {
            for event in [
                sound_type.break_sound,
                sound_type.step_sound,
                sound_type.place_sound,
                sound_type.hit_sound,
                sound_type.fall_sound,
            ] {
                assert!(event.is_empty() || events.get(event).is_some(), "{event}");
            }
        }
    }
}
