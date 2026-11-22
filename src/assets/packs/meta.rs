//! `pack.mcmeta`.

use regex::Regex;
use serde_json::Value;

/// A pack format: major and minor version (`PackFormat`).
pub type Format = (u32, u32);

/// The resource pack format of the version Melon plays.
pub const GAME_FORMAT: Format = (97, 1);

/// `PackCompatibility`: how a pack's formats compare to [`GAME_FORMAT`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Compatibility {
    TooOld,
    TooNew,
    Unknown,
    Compatible,
}

impl Compatibility {
    /// `PackCompatibility.forVersion`.
    pub fn for_range(range: Option<(Format, Format)>) -> Self {
        match range {
            None => Self::Unknown,
            Some((_, max)) if max < GAME_FORMAT => Self::TooOld,
            Some((min, _)) if GAME_FORMAT < min => Self::TooNew,
            Some(_) => Self::Compatible,
        }
    }

    /// `pack.incompatible.<key>`, and `pack.incompatible.confirm.<key>`.
    pub fn key(self) -> &'static str {
        match self {
            Self::TooOld => "old",
            Self::TooNew => "new",
            Self::Unknown => "unknown",
            Self::Compatible => "compatible",
        }
    }
}

/// One `filter.block` entry (`IdentifierPattern`); a missing pattern matches everything.
pub struct FilterRule {
    namespace: Option<Regex>,
    path: Option<Regex>,
}

pub struct PackMeta {
    /// A text component.
    pub description: Value,
    /// The formats the pack supports, if it says.
    pub formats: Option<(Format, Format)>,
    /// `overlays.entries`: a folder and the formats it applies to.
    pub overlays: Vec<(String, (Format, Format))>,
    pub filter: Vec<FilterRule>,
}

impl PackMeta {
    /// Compatible meta for an extracted zip-mod (`assets/` at its root).
    #[must_use]
    pub fn for_mod() -> Self {
        Self {
            description: Value::String(String::new()),
            formats: Some((GAME_FORMAT, GAME_FORMAT)),
            overlays: Vec::new(),
            filter: Vec::new(),
        }
    }

    /// `None` without a `pack` section, as vanilla skips such a pack.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let json: Value = serde_json::from_slice(strip_bom(bytes)).ok()?;
        let pack = json.get("pack")?.as_object()?;
        let description = pack
            .get("description")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let overlays = json["overlays"]["entries"]
            .as_array()
            .map(|entries| entries.iter().filter_map(overlay).collect())
            .unwrap_or_default();
        let filter = json["filter"]["block"]
            .as_array()
            .map(|rules| {
                rules
                    .iter()
                    .map(|rule| FilterRule {
                        namespace: rule["namespace"].as_str().and_then(|p| Regex::new(p).ok()),
                        path: rule["path"].as_str().and_then(|p| Regex::new(p).ok()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            description,
            formats: pack_formats(pack),
            overlays,
            filter,
        })
    }

    pub fn compatibility(&self) -> Compatibility {
        Compatibility::for_range(self.formats)
    }

    /// The overlay folders for [`GAME_FORMAT`], in the order listed (later ones win).
    pub fn active_overlays(&self) -> Vec<String> {
        self.overlays
            .iter()
            .filter(|(_, (min, max))| (*min..=*max).contains(&GAME_FORMAT))
            .map(|(dir, _)| dir.clone())
            .collect()
    }

    /// Whether this pack hides `namespace:path` of the packs below it.
    pub fn filters(&self, namespace: &str, path: &str) -> bool {
        self.filters_namespace(namespace)
            && self
                .filter
                .iter()
                .any(|rule| rule.path.as_ref().is_none_or(|p| p.is_match(path)))
    }

    pub fn filters_namespace(&self, namespace: &str) -> bool {
        self.filter.iter().any(|rule| {
            rule.namespace
                .as_ref()
                .is_none_or(|p| p.is_match(namespace))
        })
    }
}

/// UTF-8 files saved by some editors start with a byte order mark.
pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// The pack's format range from `min_format`/`max_format`, else the old `pack_format`.
fn pack_formats(pack: &serde_json::Map<String, Value>) -> Option<(Format, Format)> {
    if let (Some(min), Some(max)) = (
        pack.get("min_format").and_then(|v| format(v, 0)),
        pack.get("max_format").and_then(|v| format(v, u32::MAX)),
    ) {
        return Some((min, max));
    }
    if let Some((min, max)) = pack.get("supported_formats").and_then(int_range) {
        return Some(((min, 0), (max, u32::MAX)));
    }
    let single = pack.get("pack_format")?.as_u64()? as u32;
    Some(((single, 0), (single, u32::MAX)))
}

fn overlay(entry: &Value) -> Option<(String, (Format, Format))> {
    let directory = entry["directory"].as_str()?;
    let valid = !directory.is_empty()
        && directory
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    if !valid {
        return None;
    }
    let range = match (
        entry.get("min_format").and_then(|v| format(v, 0)),
        entry.get("max_format").and_then(|v| format(v, u32::MAX)),
    ) {
        (Some(min), Some(max)) => (min, max),
        _ => {
            let (min, max) = entry.get("formats").and_then(int_range)?;
            ((min, 0), (max, u32::MAX))
        }
    };
    Some((directory.to_owned(), range))
}

/// `PackFormat`: `n` or `[major]` (minor `default_minor`) or `[major, minor]`.
fn format(value: &Value, default_minor: u32) -> Option<Format> {
    let int = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    match value {
        Value::Array(parts) => match parts.as_slice() {
            [major] => Some((int(major)?, default_minor)),
            [major, minor, ..] => Some((int(major)?, int(minor)?)),
            [] => None,
        },
        _ => Some((int(value)?, default_minor)),
    }
}

/// `InclusiveRange<Integer>`.
fn int_range(value: &Value) -> Option<(u32, u32)> {
    let int = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    match value {
        Value::Array(parts) => Some((int(parts.first()?)?, int(parts.get(1)?)?)),
        Value::Object(map) => Some((
            int(map.get("min_inclusive")?)?,
            int(map.get("max_inclusive")?)?,
        )),
        _ => int(value).map(|n| (n, n)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_format_style() {
        let parse = |s: &str| PackMeta::parse(s.as_bytes()).unwrap();
        let new = parse(r#"{"pack":{"description":"x","min_format":[97,0],"max_format":98}}"#);
        assert_eq!(new.formats, Some(((97, 0), (98, u32::MAX))));
        assert_eq!(new.compatibility(), Compatibility::Compatible);
        let old = parse(r#"{"pack":{"description":"x","pack_format":34}}"#);
        assert_eq!(old.compatibility(), Compatibility::TooOld);
        let range = parse(
            r#"{"pack":{"description":"x","pack_format":15,"supported_formats":{"min_inclusive":15,"max_inclusive":200}}}"#,
        );
        assert_eq!(range.compatibility(), Compatibility::Compatible);
        let future = parse(r#"{"pack":{"description":"x","min_format":120,"max_format":130}}"#);
        assert_eq!(future.compatibility(), Compatibility::TooNew);
        let unknown = parse(r#"{"pack":{"description":"x"}}"#);
        assert_eq!(unknown.compatibility(), Compatibility::Unknown);
        assert!(PackMeta::parse(b"{}").is_none());
    }

    #[test]
    fn picks_overlays_and_filters() {
        let meta = PackMeta::parse(
            br#"{"pack":{"description":"x","pack_format":34},
                "overlays":{"entries":[
                    {"directory":"old","formats":[18,34]},
                    {"directory":"new","min_format":[97,0],"max_format":[97,9]},
                    {"directory":"../bad","min_format":1,"max_format":200}]},
                "filter":{"block":[{"namespace":"minecraft","path":"^textures/block/"}]}}"#,
        )
        .unwrap();
        assert_eq!(meta.active_overlays(), ["new"]);
        assert!(meta.filters("minecraft", "textures/block/stone.png"));
        assert!(!meta.filters("minecraft", "textures/item/stick.png"));
        assert!(!meta.filters("other", "textures/block/stone.png"));
    }
}
