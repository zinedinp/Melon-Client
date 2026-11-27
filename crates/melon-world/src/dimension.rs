//! Dimension types (vanilla `DimensionType`).

use crate::math::argb;
use anyhow::Result;
use serde_json::Value;

/// Blocks along Y in a chunk section.
pub const SECTION_HEIGHT: i32 = 16;

/// Where a level's blocks are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Height {
    pub min_y: i32,
    pub height: i32,
}

impl Height {
    /// `dimension_type/overworld.json`.
    pub const OVERWORLD: Self = Self {
        min_y: -64,
        height: 384,
    };
    /// `dimension_type/the_nether.json` and `the_end.json`.
    pub const NETHER: Self = Self {
        min_y: 0,
        height: 256,
    };

    /// Block sections in a chunk column.
    pub const fn sections(self) -> usize {
        (self.height / SECTION_HEIGHT) as usize
    }

    /// One past the highest block.
    pub const fn max_y(self) -> i32 {
        self.min_y + self.height
    }

    /// World Y of light section 0.
    pub const fn light_min_y(self) -> i32 {
        self.min_y - SECTION_HEIGHT
    }

    /// Light sections in a column.
    pub const fn light_sections(self) -> usize {
        self.sections() + 2
    }

    /// World Y above the column's top light section.
    pub const fn light_max_y(self) -> i32 {
        self.max_y() + SECTION_HEIGHT
    }
}

/// `DimensionType.skybox`: what is drawn behind the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skybox {
    /// The sky dome, sun, moon and stars.
    Overworld,
    /// The end's dark cubemap.
    End,
    /// Nothing but fog (the nether).
    None,
}

/// `DimensionType.cardinalLightType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardinalLight {
    Default,
    Nether,
}

impl CardinalLight {
    /// `CardinalLighting.byFace` as vanilla stores it.
    pub const fn shades(self) -> [f32; 6] {
        match self {
            Self::Default => [0.5, 1.0, 0.8, 0.8, 0.6, 0.6],
            Self::Nether => [0.9, 0.9, 0.8, 0.8, 0.6, 0.6],
        }
    }

    /// The factor on a face baked with the default shade that gives this dimension's shade.
    pub fn relative(self) -> [f32; 6] {
        let default = Self::Default.shades();
        let mine = self.shades();
        std::array::from_fn(|i| mine[i] / default[i])
    }
}

/// A colour attribute, linear `0..1` RGB.
pub type Rgb = [f32; 3];

/// The environment attributes a dimension type sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attributes {
    pub ambient_sounds: Option<crate::biome::AmbientSounds>,
    pub fog_color: Option<Rgb>,
    pub sky_color: Option<Rgb>,
    pub fog_start_distance: Option<f32>,
    pub fog_end_distance: Option<f32>,
    pub sky_fog_end_distance: Option<f32>,
    pub cloud_fog_end_distance: Option<f32>,
    pub water_fog_color: Option<Rgb>,
    pub water_fog_start_distance: Option<f32>,
    pub water_fog_end_distance: Option<f32>,
    pub ambient_light_color: Option<Rgb>,
    pub block_light_tint: Option<Rgb>,
    pub sky_light_color: Option<Rgb>,
    pub sky_light_factor: Option<f32>,
    pub sky_light_level: Option<f32>,
    pub night_vision_color: Option<Rgb>,
    pub cloud_color: Option<[f32; 4]>,
    pub cloud_height: Option<f32>,
    pub piglins_zombify: Option<bool>,
}

/// One dimension type.
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionType {
    pub height: Height,
    pub has_skylight: bool,
    pub has_ceiling: bool,
    /// `DimensionType.ambientLight`, `0..1`: brightens light 0.
    pub ambient_light: f32,
    /// `DimensionType.coordinateScale`: 8 in the nether.
    pub coordinate_scale: f64,
    /// A fixed time of day (`hasFixedTime`): no sun and moon cycle.
    pub fixed_time: bool,
    pub skybox: Skybox,
    pub cardinal_light: CardinalLight,
    pub attributes: Attributes,
}

impl DimensionType {
    /// `EnvironmentAttributes.PIGLINS_ZOMBIFY`.
    pub fn piglins_zombify(&self) -> bool {
        self.attributes.piglins_zombify.unwrap_or(true)
    }

    /// `dimension_type/overworld.json`.
    pub fn overworld() -> Self {
        Self {
            height: Height::OVERWORLD,
            has_skylight: true,
            has_ceiling: false,
            ambient_light: 0.0,
            coordinate_scale: 1.0,
            fixed_time: false,
            skybox: Skybox::Overworld,
            cardinal_light: CardinalLight::Default,
            attributes: Attributes {
                ambient_light_color: Some(rgb(0x0a0a0a)),
                fog_color: Some(rgb(0xc0d8ff)),
                sky_color: Some(rgb(0x78a7ff)),
                cloud_color: Some(argb(0xccffffff)),
                cloud_height: Some(192.33),
                ambient_sounds: Some(crate::biome::AmbientSounds::cave()),
                ..Attributes::default()
            },
        }
    }

    /// `dimension_type/the_nether.json`.
    pub fn nether() -> Self {
        Self {
            height: Height::NETHER,
            has_skylight: false,
            has_ceiling: true,
            ambient_light: 0.1,
            coordinate_scale: 8.0,
            fixed_time: true,
            skybox: Skybox::None,
            cardinal_light: CardinalLight::Nether,
            attributes: Attributes {
                ambient_light_color: Some(rgb(0x302821)),
                fog_start_distance: Some(10.0),
                fog_end_distance: Some(96.0),
                sky_light_color: Some(rgb(0x7a7aff)),
                sky_light_factor: Some(0.0),
                sky_light_level: Some(4.0),
                piglins_zombify: Some(false),
                ..Attributes::default()
            },
        }
    }

    /// `dimension_type/the_end.json`.
    pub fn end() -> Self {
        Self {
            height: Height::NETHER,
            has_skylight: true,
            has_ceiling: false,
            ambient_light: 0.25,
            coordinate_scale: 1.0,
            fixed_time: true,
            skybox: Skybox::End,
            cardinal_light: CardinalLight::Default,
            attributes: Attributes {
                ambient_light_color: Some(rgb(0x3f473f)),
                fog_color: Some(rgb(0x181318)),
                sky_color: Some(rgb(0x000000)),
                sky_light_color: Some(rgb(0xac60cd)),
                sky_light_factor: Some(0.0),
                ambient_sounds: Some(crate::biome::AmbientSounds::cave()),
                ..Attributes::default()
            },
        }
    }

    /// The built-in type of a registry id such as `minecraft:the_nether`.
    pub fn built_in(name: &str) -> Option<Self> {
        match name.trim_start_matches("minecraft:") {
            "overworld" | "overworld_caves" => Some(Self::overworld()),
            "the_nether" => Some(Self::nether()),
            "the_end" => Some(Self::end()),
            _ => None,
        }
    }

    /// Reads one registry entry as the datapack JSON has it.
    pub fn from_json(json: &Value, base: &Self) -> Result<Self> {
        let int = |key: &str| json.get(key).and_then(Value::as_i64);
        let flag = |key: &str, default: bool| json.get(key).and_then(as_bool).unwrap_or(default);
        let min_y = int("min_y").map_or(base.height.min_y, |v| v as i32);
        let height = int("height").map_or(base.height.height, |v| v as i32);
        anyhow::ensure!(
            height > 0 && height % SECTION_HEIGHT == 0 && min_y % SECTION_HEIGHT == 0,
            "dimension of {height} blocks from {min_y} doesn't fill whole sections"
        );
        let attributes = json.get("attributes").map_or_else(
            || base.attributes.clone(),
            |a| Attributes::from_json(a, base),
        );
        Ok(Self {
            height: Height { min_y, height },
            has_skylight: flag("has_skylight", base.has_skylight),
            has_ceiling: flag("has_ceiling", base.has_ceiling),
            ambient_light: json
                .get("ambient_light")
                .and_then(Value::as_f64)
                .map_or(base.ambient_light, |v| v as f32),
            coordinate_scale: json
                .get("coordinate_scale")
                .and_then(Value::as_f64)
                .unwrap_or(base.coordinate_scale),
            fixed_time: flag("has_fixed_time", false),
            skybox: match json.get("skybox").and_then(Value::as_str) {
                Some("none") => Skybox::None,
                Some("end") => Skybox::End,
                Some(_) => Skybox::Overworld,
                None => Skybox::Overworld,
            },
            cardinal_light: match json.get("cardinal_light").and_then(Value::as_str) {
                Some("nether") => CardinalLight::Nether,
                _ => CardinalLight::Default,
            },
            attributes,
        })
    }
}

impl Attributes {
    fn from_json(json: &Value, base: &DimensionType) -> Self {
        let color = |key: &str| json.get(key).and_then(parse_color);
        let float = |key: &str| json.get(key).and_then(Value::as_f64).map(|v| v as f32);
        let _ = base;
        Self {
            ambient_sounds: json
                .get("minecraft:audio/ambient_sounds")
                .and_then(crate::biome::AmbientSounds::from_json),
            fog_color: color("minecraft:visual/fog_color"),
            sky_color: color("minecraft:visual/sky_color"),
            fog_start_distance: float("minecraft:visual/fog_start_distance"),
            fog_end_distance: float("minecraft:visual/fog_end_distance"),
            sky_fog_end_distance: float("minecraft:visual/sky_fog_end_distance"),
            cloud_fog_end_distance: float("minecraft:visual/cloud_fog_end_distance"),
            water_fog_color: color("minecraft:visual/water_fog_color"),
            water_fog_start_distance: float("minecraft:visual/water_fog_start_distance"),
            water_fog_end_distance: float("minecraft:visual/water_fog_end_distance"),
            ambient_light_color: color("minecraft:visual/ambient_light_color"),
            block_light_tint: color("minecraft:visual/block_light_tint"),
            sky_light_color: color("minecraft:visual/sky_light_color"),
            sky_light_factor: float("minecraft:visual/sky_light_factor"),
            sky_light_level: float("minecraft:gameplay/sky_light_level"),
            night_vision_color: color("minecraft:visual/night_vision_color"),
            cloud_color: json
                .get("minecraft:visual/cloud_color")
                .and_then(parse_argb),
            cloud_height: float("minecraft:visual/cloud_height"),
            piglins_zombify: json
                .get("minecraft:gameplay/piglins_zombify")
                .and_then(as_bool),
        }
    }
}

/// The dimension types the server registered, by registry index.
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionTypes(Vec<(String, DimensionType)>);

impl Default for DimensionTypes {
    /// Overworld, nether, end: vanilla's registry order when none was sent.
    fn default() -> Self {
        Self(
            ["overworld", "the_nether", "the_end"]
                .into_iter()
                .filter_map(|name| {
                    DimensionType::built_in(name).map(|t| (format!("minecraft:{name}"), t))
                })
                .collect(),
        )
    }
}

impl DimensionTypes {
    /// The registry's entries in index order.
    pub fn new(entries: Vec<(String, DimensionType)>) -> Self {
        Self(entries)
    }

    /// The type at registry index `id`; an unknown index is the overworld.
    pub fn get(&self, id: i32) -> DimensionType {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.0.get(i))
            .map_or_else(DimensionType::overworld, |(_, t)| t.clone())
    }

    /// The registry id of the type at `id`.
    pub fn name(&self, id: i32) -> Option<&str> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.0.get(i))
            .map(|(name, _)| name.as_str())
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| value.as_i64().map(|v| v != 0))
}

/// `#rrggbb` (also `#aarrggbb`, the alpha dropped) or a signed ARGB int.
fn parse_color(value: &Value) -> Option<Rgb> {
    let packed = parse_packed(value)?;
    Some(rgb(packed & 0x00ff_ffff))
}

fn parse_argb(value: &Value) -> Option<[f32; 4]> {
    let packed = parse_packed(value)?;
    let packed = match value.as_str() {
        Some(s) if s.trim_start_matches('#').len() <= 6 => packed | 0xff00_0000,
        _ => packed,
    };
    Some(argb(packed))
}

fn parse_packed(value: &Value) -> Option<u32> {
    match value {
        Value::String(text) => u32::from_str_radix(text.trim_start_matches('#'), 16).ok(),
        Value::Number(n) => n.as_i64().map(|v| v as u32),
        _ => None,
    }
}

/// `0xRRGGBB` as `0..1` channels.
pub const fn rgb(hex: u32) -> Rgb {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn piglins_zombify_everywhere_but_where_the_type_says_not() {
        assert!(DimensionType::overworld().piglins_zombify());
        assert!(!DimensionType::nether().piglins_zombify());
        let json = json!({
            "min_y": 0, "height": 256,
            "attributes": { "minecraft:gameplay/piglins_zombify": false }
        });
        let parsed = DimensionType::from_json(&json, &DimensionType::overworld()).unwrap();
        assert!(!parsed.piglins_zombify());
    }

    #[test]
    fn heights_count_sections_and_light() {
        let h = Height::OVERWORLD;
        assert_eq!(h.sections(), 24);
        assert_eq!(h.light_sections(), 26);
        assert_eq!(h.light_min_y(), -80);
        assert_eq!(h.light_max_y(), 336);
        let n = Height::NETHER;
        assert_eq!(n.sections(), 16);
        assert_eq!(n.light_min_y(), -16);
        assert_eq!(n.light_max_y(), 272);
    }

    #[test]
    fn a_nether_entry_reads_height_light_and_fog() {
        let json = json!({
            "min_y": 0, "height": 256, "has_skylight": false, "has_ceiling": true,
            "ambient_light": 0.1, "coordinate_scale": 8.0, "has_fixed_time": true,
            "skybox": "none", "cardinal_light": "nether",
            "attributes": {
                "minecraft:visual/fog_start_distance": 10.0,
                "minecraft:visual/fog_end_distance": 96.0,
                "minecraft:visual/ambient_light_color": "#302821",
                "minecraft:gameplay/sky_light_level": 4.0
            }
        });
        let t = DimensionType::from_json(&json, &DimensionType::overworld()).unwrap();
        assert_eq!(
            t,
            DimensionType {
                attributes: Attributes {
                    ambient_light_color: Some(rgb(0x302821)),
                    fog_start_distance: Some(10.0),
                    fog_end_distance: Some(96.0),
                    sky_light_level: Some(4.0),
                    ..Attributes::default()
                },
                ..DimensionType::nether()
            }
        );
    }

    #[test]
    fn the_nether_shades_faces_by_its_own_table() {
        assert_eq!(CardinalLight::Default.relative(), [1.0; 6]);
        let nether = CardinalLight::Nether.relative();
        assert!((nether[0] - 1.8).abs() < 1e-6);
        assert!((nether[1] - 0.9).abs() < 1e-6);
        assert_eq!(nether[2..], [1.0; 4]);
    }

    #[test]
    fn colours_come_as_text_or_ints() {
        assert_eq!(parse_color(&json!("#ff0000")), Some([1.0, 0.0, 0.0]));
        assert_eq!(parse_color(&json!(-16777216)), Some([0.0; 3]));
        let cloud = parse_argb(&json!("#ccffffff")).unwrap();
        assert!((cloud[3] - 0.8).abs() < 0.01);
        assert_eq!(parse_argb(&json!("#ffffff")).unwrap()[3], 1.0);
    }

    #[test]
    fn heights_must_fill_sections() {
        let json = json!({ "min_y": 0, "height": 250 });
        assert!(DimensionType::from_json(&json, &DimensionType::overworld()).is_err());
    }

    #[test]
    fn the_default_registry_is_vanillas_order() {
        let types = DimensionTypes::default();
        assert_eq!(types.get(0).height, Height::OVERWORLD);
        assert_eq!(types.get(1).height, Height::NETHER);
        assert!(!types.get(1).has_skylight);
        assert_eq!(types.get(99).height, Height::OVERWORLD);
        assert_eq!(types.name(2), Some("minecraft:the_end"));
    }
}
