//! Biomes (vanilla `Biome`): what the server's `minecraft:worldgen/biome`

use serde_json::Value;

use crate::world::colormap::{self, ColorMaps};
use crate::world::dimension::{Rgb, rgb};

/// `Biome.TemperatureModifier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TemperatureModifier {
    #[default]
    None,
    /// Frozen oceans: patches of colder water.
    Frozen,
}

/// `BiomeSpecialEffects.GrassColorModifier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GrassModifier {
    #[default]
    None,
    DarkForest,
    Swamp,
}

/// What a biome sets of the environment attributes (`minecraft:visual/...`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visuals {
    pub sky_color: Option<Rgb>,
    pub fog_color: Option<Rgb>,
    pub water_fog_color: Option<Rgb>,
    /// The `multiply` modifier swamps put on the water fog's end.
    pub water_fog_end_scale: Option<f32>,
}

/// `minecraft:audio/ambient_sounds` (`AmbientSounds`): a loop, the mood
/// sound of dark places and random additions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbientSounds {
    pub loop_sound: Option<String>,
    pub mood: Option<AmbientMood>,
    pub additions: Vec<AmbientAddition>,
}

/// `AmbientMoodSettings`.
#[derive(Clone, Debug, PartialEq)]
pub struct AmbientMood {
    pub sound: String,
    pub tick_delay: i32,
    pub block_search_extent: i32,
    pub offset: f64,
}

/// `AmbientAdditionsSettings`.
#[derive(Clone, Debug, PartialEq)]
pub struct AmbientAddition {
    pub sound: String,
    pub tick_chance: f64,
}

/// A `SoundEvent` in a codec: its id, or an object with `sound_id`.
fn sound_event(value: &Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("sound_id").and_then(Value::as_str))
        .map(str::to_owned)
}

impl AmbientSounds {
    /// `AmbientMoodSettings.LEGACY_CAVE_SETTINGS`: the overworld's and the
    /// end's mood.
    pub fn cave() -> Self {
        Self {
            mood: Some(AmbientMood {
                sound: "minecraft:ambient.cave".to_owned(),
                tick_delay: 6000,
                block_search_extent: 8,
                offset: 2.0,
            }),
            ..Self::default()
        }
    }

    /// The attribute's value; fields a codec requires make a broken entry
    /// count as missing.
    pub fn from_json(value: &Value) -> Option<Self> {
        let mood = match value.get("mood") {
            None => None,
            Some(mood) => Some(AmbientMood {
                sound: sound_event(mood.get("sound")?)?,
                tick_delay: i32::try_from(mood.get("tick_delay")?.as_i64()?).ok()?,
                block_search_extent: i32::try_from(mood.get("block_search_extent")?.as_i64()?)
                    .ok()?,
                offset: mood.get("offset")?.as_f64()?,
            }),
        };
        // `ExtraCodecs.compactListCodec`: one entry, or a list of them.
        let additions = match value.get("additions") {
            None => Vec::new(),
            Some(Value::Array(list)) => list.iter().filter_map(addition).collect(),
            Some(one) => addition(one).into_iter().collect(),
        };
        Some(Self {
            loop_sound: value.get("loop").and_then(sound_event),
            mood,
            additions,
        })
    }
}

fn addition(value: &Value) -> Option<AmbientAddition> {
    Some(AmbientAddition {
        sound: sound_event(value.get("sound")?)?,
        tick_chance: value.get("tick_chance")?.as_f64()?,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Biome {
    pub temperature: f32,
    pub downfall: f32,
    pub temperature_modifier: TemperatureModifier,
    pub has_precipitation: bool,
    /// `BiomeSpecialEffects.waterColor`.
    pub water_color: Rgb,
    /// Colours that replace the temperature-and-downfall colour map.
    pub grass_color: Option<Rgb>,
    pub foliage_color: Option<Rgb>,
    pub dry_foliage_color: Option<Rgb>,
    pub grass_modifier: GrassModifier,
    pub visuals: Visuals,
    /// `minecraft:visual/ambient_particles`.
    pub ambient_particles: Vec<(crate::particle::Kind, f32)>,
    /// `minecraft:audio/ambient_sounds`; `None` leaves the dimension's.
    pub ambient_sounds: Option<AmbientSounds>,
}

impl Default for Biome {
    /// Plains: what an unknown biome looks like.
    fn default() -> Self {
        Self {
            temperature: 0.8,
            downfall: 0.4,
            temperature_modifier: TemperatureModifier::None,
            has_precipitation: true,
            water_color: rgb(0x3f76e4),
            grass_color: None,
            foliage_color: None,
            dry_foliage_color: None,
            grass_modifier: GrassModifier::None,
            visuals: Visuals {
                sky_color: Some(rgb(0x78a7ff)),
                ..Visuals::default()
            },
            ambient_particles: Vec::new(),
            ambient_sounds: None,
        }
    }
}

impl Biome {
    /// Reads one registry entry as the datapack JSON has it.
    pub fn from_json(json: &Value) -> Self {
        let float = |key: &str, default: f32| {
            json.get(key)
                .and_then(Value::as_f64)
                .map_or(default, |v| v as f32)
        };
        let effects = json.get("effects");
        let color = |value: Option<&Value>| value.and_then(parse_rgb);
        let effect = |key: &str| color(effects.and_then(|e| e.get(key)));
        let attributes = json.get("attributes");
        let visual = |key: &str| color(attributes.and_then(|a| a.get(key)));
        Self {
            temperature: float("temperature", 0.8),
            downfall: float("downfall", 0.4),
            temperature_modifier: match json.get("temperature_modifier").and_then(Value::as_str) {
                Some("frozen") => TemperatureModifier::Frozen,
                _ => TemperatureModifier::None,
            },
            has_precipitation: json
                .get("has_precipitation")
                .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|n| n != 0)))
                .unwrap_or(true),
            water_color: effect("water_color").unwrap_or(rgb(0x3f76e4)),
            grass_color: effect("grass_color"),
            foliage_color: effect("foliage_color"),
            dry_foliage_color: effect("dry_foliage_color"),
            grass_modifier: match effects
                .and_then(|e| e.get("grass_color_modifier"))
                .and_then(Value::as_str)
            {
                Some("dark_forest") => GrassModifier::DarkForest,
                Some("swamp") => GrassModifier::Swamp,
                _ => GrassModifier::None,
            },
            visuals: Visuals {
                sky_color: visual("minecraft:visual/sky_color"),
                fog_color: visual("minecraft:visual/fog_color"),
                water_fog_color: visual("minecraft:visual/water_fog_color"),
                water_fog_end_scale: attributes
                    .and_then(|a| a.get("minecraft:visual/water_fog_end_distance"))
                    .and_then(|v| v.get("argument"))
                    .and_then(Value::as_f64)
                    .map(|v| v as f32),
            },
            ambient_particles: crate::particle::client::ambient::from_attributes(attributes),
            ambient_sounds: attributes
                .and_then(|a| a.get("minecraft:audio/ambient_sounds"))
                .and_then(AmbientSounds::from_json),
        }
    }

    /// `Biome.getGrassColor`: colormap or override, then the modifier.
    pub fn grass_color_at(&self, maps: &ColorMaps, x: f64, z: f64) -> u32 {
        let base = self.grass_color.map_or_else(
            || maps.grass.get(self.temperature, self.downfall),
            colormap::pack_f32,
        );
        match self.grass_modifier {
            GrassModifier::None => base,
            GrassModifier::DarkForest => colormap::dark_forest(base),
            GrassModifier::Swamp => colormap::swamp_grass(x, z),
        }
    }

    /// `Biome.getFoliageColor`.
    pub fn foliage_color_at(&self, maps: &ColorMaps) -> u32 {
        self.foliage_color.map_or_else(
            || maps.foliage.get(self.temperature, self.downfall),
            colormap::pack_f32,
        )
    }

    /// `Biome.getDryFoliageColor`.
    pub fn dry_foliage_color_at(&self, maps: &ColorMaps) -> u32 {
        self.dry_foliage_color.map_or_else(
            || maps.dry_foliage.get(self.temperature, self.downfall),
            colormap::pack_f32,
        )
    }

    /// `Biome.getPrecipitationAt` without the height curve.
    pub fn precipitation(&self) -> crate::world::weather::Precipitation {
        use crate::world::weather::Precipitation;
        if !self.has_precipitation {
            Precipitation::None
        } else if self.temperature < 0.15 {
            Precipitation::Snow
        } else {
            Precipitation::Rain
        }
    }

    /// `Biome.getWaterColor`.
    pub fn water_color_packed(&self) -> u32 {
        colormap::pack_f32(self.water_color)
    }
}

fn parse_rgb(value: &Value) -> Option<Rgb> {
    let packed = match value {
        Value::String(text) => u32::from_str_radix(text.trim_start_matches('#'), 16).ok()?,
        Value::Number(n) => n.as_i64()? as u32,
        _ => return None,
    };
    Some(rgb(packed & 0x00ff_ffff))
}

/// The biomes the server registered, by registry index.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Biomes(Vec<(String, Biome)>);

impl Biomes {
    pub fn new(entries: Vec<(String, Biome)>) -> Self {
        Self(entries)
    }

    /// The biome with registry index `id`; an unknown one is plains.
    pub fn get(&self, id: u16) -> Biome {
        self.0
            .get(usize::from(id))
            .map(|(_, biome)| biome.clone())
            .unwrap_or_default()
    }

    /// The registry id of biome `id`, e.g. `minecraft:plains`.
    pub fn name(&self, id: u16) -> Option<&str> {
        self.0.get(usize::from(id)).map(|(name, _)| name.as_str())
    }

    /// Borrowed, for sampling many cells without cloning.
    pub fn biome(&self, id: u16) -> Option<&Biome> {
        self.0.get(usize::from(id)).map(|(_, biome)| biome)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Biome cells in a section, 4 per axis.
pub const CELLS_PER_SECTION: usize = 4 * 4 * 4;

/// A section's biomes: one for all of it, as most are, or per cell.
#[derive(Clone, Debug, PartialEq)]
pub enum SectionBiomes {
    Single(u16),
    /// `y*16 + z*4 + x`, `PalettedContainer`'s order.
    Cells(Box<[u16; CELLS_PER_SECTION]>),
}

impl SectionBiomes {
    /// From a decoded container's cells.
    pub fn from_cells(cells: [u16; CELLS_PER_SECTION]) -> Self {
        let first = cells[0];
        if cells.iter().all(|&c| c == first) {
            Self::Single(first)
        } else {
            Self::Cells(Box::new(cells))
        }
    }

    /// The biome in cell `(x, y, z)` of the section, each `0..4`.
    pub fn cell(&self, x: usize, y: usize, z: usize) -> u16 {
        match self {
            Self::Single(id) => *id,
            Self::Cells(cells) => cells[(y << 4) | (z << 2) | x],
        }
    }
}

/// A chunk column's biomes, bottom section to top.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkBiomes {
    pub sections: Vec<SectionBiomes>,
}

impl ChunkBiomes {
    /// A column of one biome, `sections` tall (tests, empty chunks).
    pub fn uniform(id: u16, sections: usize) -> Self {
        Self {
            sections: vec![SectionBiomes::Single(id); sections],
        }
    }

    /// The biome at section-local block coordinates.
    pub fn at(&self, x: i32, y: i32, z: i32) -> u16 {
        let Some(last) = self.sections.len().checked_sub(1) else {
            return 0;
        };
        let section = usize::try_from(y >> 4).unwrap_or(0).min(last);
        self.sections[section].cell(
            ((x & 15) >> 2) as usize,
            ((y & 15) >> 2) as usize,
            ((z & 15) >> 2) as usize,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_swamp_reads_its_colours_and_water_fog() {
        let swamp = Biome::from_json(&json!({
            "temperature": 0.8, "downfall": 0.9,
            "effects": { "water_color": "#617b64", "foliage_color": "#6a7039",
                         "grass_color_modifier": "swamp" },
            "attributes": {
                "minecraft:visual/sky_color": "#78a7ff",
                "minecraft:visual/water_fog_color": "#232317",
                "minecraft:visual/water_fog_end_distance": { "argument": 0.85, "modifier": "multiply" }
            },
            "has_precipitation": true
        }));
        assert_eq!(swamp.water_color, rgb(0x617b64));
        assert_eq!(swamp.foliage_color, Some(rgb(0x6a7039)));
        assert_eq!(swamp.grass_modifier, GrassModifier::Swamp);
        assert_eq!(swamp.visuals.water_fog_color, Some(rgb(0x232317)));
        assert_eq!(swamp.visuals.water_fog_end_scale, Some(0.85));
        assert_eq!(swamp.visuals.fog_color, None);
    }

    #[test]
    fn ambient_sounds_read_their_mood_loop_and_additions() {
        let crimson = json!({
            "attributes": {"minecraft:audio/ambient_sounds": {
                "loop": "minecraft:ambient.crimson_forest.loop",
                "mood": {"sound": "minecraft:ambient.crimson_forest.mood", "tick_delay": 6000,
                         "block_search_extent": 8, "offset": 2.0},
                "additions": {"sound": "minecraft:ambient.crimson_forest.additions", "tick_chance": 0.0111}
            }}
        });
        let sounds = Biome::from_json(&crimson).ambient_sounds.unwrap();
        assert_eq!(
            sounds.loop_sound.as_deref(),
            Some("minecraft:ambient.crimson_forest.loop")
        );
        assert_eq!(sounds.mood.as_ref().unwrap().block_search_extent, 8);
        assert_eq!(sounds.additions.len(), 1);
        assert_eq!(sounds.additions[0].tick_chance, 0.0111);
        // The overworld's, from its dimension type, is the cave mood.
        let overworld = json!({"mood": {"sound": "minecraft:ambient.cave", "tick_delay": 6000,
                                        "block_search_extent": 8, "offset": 2.0}});
        assert_eq!(
            AmbientSounds::from_json(&overworld),
            Some(AmbientSounds::cave())
        );
        // A mood without its required fields leaves the attribute out.
        assert_eq!(
            AmbientSounds::from_json(&json!({"mood": {"sound": "x"}})),
            None
        );
        assert!(Biome::from_json(&json!({})).ambient_sounds.is_none());
    }

    #[test]
    fn a_column_finds_the_cell_of_a_block() {
        let mut cells = [1u16; CELLS_PER_SECTION];
        cells[(2 << 4) | (1 << 2) | 3] = 7;
        let column = ChunkBiomes {
            sections: vec![SectionBiomes::Single(0), SectionBiomes::from_cells(cells)],
        };
        assert_eq!(column.at(0, 0, 0), 0);
        assert_eq!(column.at(13, 16 + 9, 6), 7);
        assert_eq!(column.at(12, 16 + 9, 4), 7);
        assert_eq!(column.at(11, 16 + 9, 4), 1);
        assert_eq!(column.at(13, 500, 6), 1);
        assert!(matches!(
            SectionBiomes::from_cells([4; 64]),
            SectionBiomes::Single(4)
        ));
    }
}
