//! `sounds.json`: which sound files a sound event can play, and how loud

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// How deep events may point at events before the chain is given up.
const MAX_DEPTH: usize = 8;

/// The sound the click on any button plays, when there is no `sounds.json`.
const CLICK_EVENT: &str = "ui.button.click";
const CLICK_FILE: &str = "random/click_stereo";

/// One file to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// The file under `sounds/`, without `.ogg`: `random/click_stereo`.
    pub name: Arc<str>,
    pub volume: f32,
    pub pitch: f32,
    /// Read from the file while playing instead of decoded at once: music.
    pub stream: bool,
    /// Decoded when the sounds load, not when first played (`Sound.preload`).
    pub preload: bool,
    /// Blocks it can be heard from at volume 1 (`Sound.attenuationDistance`).
    pub attenuation_distance: f32,
}

/// `Sound.DEFAULT_ATTENUATION_DISTANCE`.
pub const DEFAULT_ATTENUATION: f32 = 16.0;

#[derive(Debug)]
enum Kind {
    Sound(Sound),
    /// Another event, whose own pick is played.
    Event(Box<str>),
}

#[derive(Debug)]
struct Entry {
    kind: Kind,
    weight: u32,
    volume: f32,
    pitch: f32,
}

#[derive(Debug, Default)]
pub struct SoundEvents {
    events: HashMap<Box<str>, Vec<Entry>>,
    /// `WeighedSoundEvents.getSubtitle`: the translation key of each event
    /// that has one.
    subtitles: HashMap<Box<str>, Arc<str>>,
}

/// `minecraft:` is the default namespace; names are kept without it.
fn plain(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn number(value: &Value, key: &str, default: f64) -> f32 {
    value.get(key).and_then(Value::as_f64).unwrap_or(default) as f32
}

impl SoundEvents {
    /// Reads `sounds.json`. An entry is a name, or an object with
    pub fn parse(json: &str) -> Option<Self> {
        let mut events = Self::default();
        events.merge("minecraft", json.as_bytes())?;
        Some(events)
    }

    /// Adds one `sounds.json` of `namespace` on top, as vanilla's
    pub fn merge(&mut self, namespace: &str, json: &[u8]) -> Option<()> {
        let root: Value =
            serde_json::from_slice(crate::assets::packs::meta::strip_bom(json)).ok()?;
        for (name, event) in root.as_object()? {
            let key: Box<str> = if namespace == "minecraft" {
                plain(name).into()
            } else {
                format!("{namespace}:{name}").into()
            };
            if event.get("replace").and_then(Value::as_bool) == Some(true) {
                self.events.remove(&key);
            }
            if let Some(subtitle) = event.get("subtitle").and_then(Value::as_str) {
                self.subtitles.insert(key.clone(), subtitle.into());
            }
            let Some(sounds) = event.get("sounds").and_then(Value::as_array) else {
                continue;
            };
            let entries: Vec<Entry> = sounds.iter().filter_map(parse_entry).collect();
            if !entries.is_empty() {
                self.events.entry(key).or_default().extend(entries);
            }
        }
        Some(())
    }

    /// Just the button click: what plays when the assets have no
    pub fn builtin() -> Self {
        let click = Entry {
            kind: Kind::Sound(Sound {
                name: CLICK_FILE.into(),
                volume: 1.0,
                pitch: 1.0,
                stream: false,
                preload: false,
                attenuation_distance: DEFAULT_ATTENUATION,
            }),
            weight: 1,
            volume: 1.0,
            pitch: 1.0,
        };
        Self {
            events: HashMap::from([(CLICK_EVENT.into(), vec![click])]),
            subtitles: HashMap::new(),
        }
    }

    /// Every namespace's `sounds.json`, vanilla's and each resource pack's,
    pub fn load(assets_root: &Path) -> Self {
        let mut events = Self::default();
        for namespace in crate::assets::namespaces(assets_root) {
            for json in crate::assets::resource_stack(assets_root, &namespace, "sounds.json") {
                if events.merge(&namespace, &json).is_none() {
                    crate::warn!("{namespace}:sounds.json unreadable");
                }
            }
        }
        if events.events.is_empty() {
            Self::builtin()
        } else {
            events.events.shrink_to_fit();
            events.subtitles.shrink_to_fit();
            events.events.values_mut().for_each(Vec::shrink_to_fit);
            events
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// The files to decode as the sounds load (`SoundEngine.requestPreload`).
    pub fn preloads(&self) -> impl Iterator<Item = &Sound> {
        self.events
            .values()
            .flatten()
            .filter_map(|entry| match &entry.kind {
                Kind::Sound(sound) if sound.preload && !sound.stream => Some(sound),
                _ => None,
            })
    }

    /// The subtitle's translation key of `event`, if it has one.
    pub fn subtitle(&self, event: &str) -> Option<&Arc<str>> {
        self.subtitles.get(plain(event))
    }

    /// The sound `event` plays this time: a weighted pick, `roll` in
    pub fn pick(&self, event: &str, roll: f32) -> Option<Sound> {
        if crate::probe::enabled() && !self.events.contains_key(plain(event)) {
            crate::probe::count("sound.unknown", event);
        }
        self.pick_at(plain(event), roll, 0)
    }

    fn pick_at(&self, event: &str, roll: f32, depth: usize) -> Option<Sound> {
        if depth > MAX_DEPTH {
            return None;
        }
        let entries = self.events.get(event)?;
        let total: u32 = entries.iter().map(|e| e.weight).sum();
        let mut target = (roll.clamp(0.0, 0.999_999) * total as f32) as u32;
        let entry = entries.iter().find(|e| {
            if target < e.weight {
                true
            } else {
                target -= e.weight;
                false
            }
        })?;
        let mut sound = match &entry.kind {
            Kind::Sound(sound) => sound.clone(),
            Kind::Event(inner) => self.pick_at(inner, (roll * 7.31).fract(), depth + 1)?,
        };
        sound.volume *= entry.volume;
        sound.pitch *= entry.pitch;
        Some(sound)
    }
}

fn parse_entry(value: &Value) -> Option<Entry> {
    if let Some(name) = value.as_str() {
        return Some(sound_entry(
            plain(name),
            1,
            (1.0, 1.0),
            (false, false, DEFAULT_ATTENUATION),
        ));
    }
    let name = plain(value.get("name")?.as_str()?);
    let weight = value
        .get("weight")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as u32;
    let (volume, pitch) = (number(value, "volume", 1.0), number(value, "pitch", 1.0));
    if value.get("type").and_then(Value::as_str) == Some("event") {
        return Some(Entry {
            kind: Kind::Event(name.into()),
            weight,
            volume,
            pitch,
        });
    }
    let stream = value
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let preload = value
        .get("preload")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let attenuation = number(
        value,
        "attenuation_distance",
        f64::from(DEFAULT_ATTENUATION),
    );
    Some(sound_entry(
        name,
        weight,
        (volume, pitch),
        (stream, preload, attenuation),
    ))
}

/// A file entry carries its volume and pitch on the entry itself.
fn sound_entry(
    name: &str,
    weight: u32,
    (volume, pitch): (f32, f32),
    (stream, preload, attenuation_distance): (bool, bool, f32),
) -> Entry {
    Entry {
        kind: Kind::Sound(Sound {
            name: name.into(),
            volume: 1.0,
            pitch: 1.0,
            stream,
            preload,
            attenuation_distance,
        }),
        weight,
        volume,
        pitch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
        "ui.button.click": {"sounds": ["random/click_stereo"]},
        "minecraft:block.stone.break": {"sounds": [
            {"name": "dig/stone1", "weight": 3},
            {"name": "minecraft:dig/stone2", "volume": 0.5, "pitch": 1.5}
        ], "subtitle": "subtitles.block.generic.break"},
        "music.menu": {"sounds": [{"name": "music/menu/a", "stream": true}]},
        "block.bell.use": {"sounds": [{"name": "bell", "attenuation_distance": 48, "preload": true}]},
        "alias": {"sounds": [{"name": "block.stone.break", "type": "event", "volume": 0.5}]},
        "loop": {"sounds": [{"name": "loop", "type": "event"}]},
        "empty": {"sounds": []},
        "broken": 7
    }"#;

    #[test]
    fn events_and_entries_are_read_in_every_form() {
        let events = SoundEvents::parse(JSON).unwrap();
        assert_eq!(events.len(), 6);
        let click = events.pick("ui.button.click", 0.3).unwrap();
        assert_eq!(&*click.name, "random/click_stereo");
        assert_eq!((click.volume, click.pitch, click.stream), (1.0, 1.0, false));
        let music = events.pick("minecraft:music.menu", 0.0).unwrap();
        assert!(music.stream);
        assert_eq!(music.attenuation_distance, 16.0);
        assert_eq!(
            events
                .pick("block.bell.use", 0.0)
                .unwrap()
                .attenuation_distance,
            48.0
        );
        assert!(events.pick("nothing", 0.0).is_none());
        let preloads: Vec<&str> = events.preloads().map(|s| &*s.name).collect();
        assert_eq!(preloads, ["bell"]);
        assert!(SoundEvents::parse("not json").is_none());
    }

    #[test]
    fn a_pick_follows_the_weights() {
        let events = SoundEvents::parse(JSON).unwrap();
        let first = events.pick("block.stone.break", 0.5).unwrap();
        assert_eq!(&*first.name, "dig/stone1");
        let second = events.pick("block.stone.break", 0.9).unwrap();
        assert_eq!(&*second.name, "dig/stone2");
        assert_eq!((second.volume, second.pitch), (0.5, 1.5));
    }

    #[test]
    fn events_pointing_at_events_multiply_and_cannot_loop() {
        let events = SoundEvents::parse(JSON).unwrap();
        assert_eq!(
            events.subtitle("minecraft:block.stone.break").map(|s| &**s),
            Some("subtitles.block.generic.break")
        );
        assert!(events.subtitle("ui.button.click").is_none());
        let alias = events.pick("alias", 0.0).unwrap();
        assert_eq!(&*alias.name, "dig/stone1");
        assert_eq!(alias.volume, 0.5);
        assert!(events.pick("loop", 0.0).is_none());
    }

    #[test]
    fn the_builtin_events_have_the_click() {
        let events = SoundEvents::builtin();
        assert_eq!(
            &*events.pick("ui.button.click", 0.9).unwrap().name,
            "random/click_stereo"
        );
    }
}
