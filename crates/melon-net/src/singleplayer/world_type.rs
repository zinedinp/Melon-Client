//! `WorldCreationUiState.WorldTypeEntry`.

use crate::cycle::Cycle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorldType {
    #[default]
    Default,
    Superflat,
    LargeBiomes,
    Amplified,
    SingleBiome,
    /// Only in the `extended` list, which Alt shows.
    Debug,
}

impl WorldType {
    /// `WorldPresetTags.NORMAL`.
    pub const NORMAL: [WorldType; 5] = [
        WorldType::Default,
        WorldType::Superflat,
        WorldType::LargeBiomes,
        WorldType::Amplified,
        WorldType::SingleBiome,
    ];
    /// `WorldPresetTags.EXTENDED`: the normal ones and Debug Mode.
    pub const EXTENDED: [WorldType; 6] = [
        WorldType::Default,
        WorldType::Superflat,
        WorldType::LargeBiomes,
        WorldType::Amplified,
        WorldType::SingleBiome,
        WorldType::Debug,
    ];

    /// The world preset's id under `minecraft:worldgen/world_preset`.
    pub fn id(self) -> &'static str {
        match self {
            WorldType::Default => "normal",
            WorldType::Superflat => "flat",
            WorldType::LargeBiomes => "large_biomes",
            WorldType::Amplified => "amplified",
            WorldType::SingleBiome => "single_biome_surface",
            WorldType::Debug => "debug_all_block_states",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::EXTENDED
            .into_iter()
            .find(|t| t.id() == id)
            .unwrap_or_default()
    }

    /// `describePreset`: `generator.minecraft.<preset>`.
    pub fn translation_key(self) -> &'static str {
        match self {
            WorldType::Default => "generator.minecraft.normal",
            WorldType::Superflat => "generator.minecraft.flat",
            WorldType::LargeBiomes => "generator.minecraft.large_biomes",
            WorldType::Amplified => "generator.minecraft.amplified",
            WorldType::SingleBiome => "generator.minecraft.single_biome_surface",
            WorldType::Debug => "generator.minecraft.debug_all_block_states",
        }
    }

    /// `PresetEditor.EDITORS`: the types Customize opens something for.
    pub fn has_editor(self) -> bool {
        matches!(self, WorldType::Superflat | WorldType::SingleBiome)
    }

    /// Whether the integrated server can generate it.
    pub fn pumpkin_supported(self) -> bool {
        self == WorldType::Default
    }

    /// The list the World Type button steps through (`Alt` = extended).
    pub fn list(alt: bool) -> &'static [WorldType] {
        if alt { &Self::EXTENDED } else { &Self::NORMAL }
    }

    /// The type `delta` places on, wrapping, in the list [`Self::list`] gives.
    pub fn stepped(self, alt: bool, delta: isize) -> Self {
        let list = Self::list(alt);
        let at = list.iter().position(|&t| t == self).unwrap_or(0) as isize;
        list[(at + delta).rem_euclid(list.len() as isize) as usize]
    }
}

impl Cycle for WorldType {
    const ALL: &'static [Self] = &WorldType::NORMAL;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_adds_the_debug_world_and_wraps_both_ways() {
        assert_eq!(WorldType::Default.stepped(false, 1), WorldType::Superflat);
        assert_eq!(
            WorldType::Default.stepped(false, -1),
            WorldType::SingleBiome
        );
        assert_eq!(WorldType::SingleBiome.stepped(true, 1), WorldType::Debug);
        assert_eq!(WorldType::Debug.stepped(true, 1), WorldType::Default);
        assert_eq!(WorldType::from_id("flat"), WorldType::Superflat);
        assert_eq!(WorldType::from_id("nonsense"), WorldType::Default);
    }
}
