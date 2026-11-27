//! `blockstates/<block>.json`: which models a block state shows.

use std::path::Path;

use serde_json::Value;

/// One model a state shows, as `blockstates/*.json` names it.
#[derive(Clone)]
pub(super) struct ModelRef {
    pub(super) model: String,
    pub(super) x: i64,
    pub(super) y: i64,
    pub(super) uvlock: bool,
}

impl ModelRef {
    /// Tells apart the refs that bake differently.
    pub(super) fn key(&self) -> String {
        format!("{}:{}:{}:{}", self.model, self.x, self.y, self.uvlock)
    }
}

/// A weighted list of models (one entry for a plain model).
pub(super) type Options = Vec<(u32, ModelRef)>;

/// `blockstates/<block>.json`, parsed once per block.
pub(super) enum BlockState {
    /// `variants`: property conditions per entry, first match wins.
    Variants(Vec<(Vec<(String, String)>, Options)>),
    /// `multipart`: every part whose condition holds.
    Multipart(Vec<(Value, Options)>),
}

impl BlockState {
    /// Every model it names, whichever state shows it.
    pub(super) fn models(&self) -> impl Iterator<Item = &str> {
        let options: Vec<&Options> = match self {
            Self::Variants(entries) => entries.iter().map(|(_, options)| options).collect(),
            Self::Multipart(parts) => parts.iter().map(|(_, options)| options).collect(),
        };
        options
            .into_iter()
            .flatten()
            .map(|(_, model_ref)| model_ref.model.as_str())
    }
}

pub(super) fn parse_block_state(assets_root: &Path, block: &str) -> Option<BlockState> {
    let name = block.trim_start_matches("minecraft:");
    let json = melon_assets::assets::json_resource(assets_root, "blockstates", name)?;
    if let Some(variants) = json["variants"].as_object() {
        let entries = variants
            .iter()
            .filter_map(|(key, value)| {
                let conditions = key
                    .split(',')
                    .filter(|pair| !pair.is_empty())
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect();
                Some((conditions, options(value)?))
            })
            .collect();
        return Some(BlockState::Variants(entries));
    }
    let parts = json["multipart"]
        .as_array()?
        .iter()
        .filter_map(|part| Some((part["when"].clone(), options(&part["apply"])?)))
        .collect();
    Some(BlockState::Multipart(parts))
}

/// A variant's models: one, or a weighted list (`weight` defaults to 1).
fn options(value: &Value) -> Option<Options> {
    let list = match value.as_array() {
        Some(list) => list
            .iter()
            .filter_map(|entry| {
                let weight = entry["weight"]
                    .as_u64()
                    .unwrap_or(1)
                    .min(u64::from(u32::MAX));
                Some((weight as u32, model_ref(entry)?))
            })
            .filter(|&(weight, _)| weight > 0)
            .collect(),
        None => vec![(1, model_ref(value)?)],
    };
    (!list.is_empty()).then_some(list)
}

fn model_ref(value: &Value) -> Option<ModelRef> {
    Some(ModelRef {
        model: value["model"].as_str()?.to_owned(),
        x: value["x"].as_i64().unwrap_or(0),
        y: value["y"].as_i64().unwrap_or(0),
        uvlock: value["uvlock"].as_bool().unwrap_or(false),
    })
}

/// `Selector.condition`: `{"key": "a|b"}`, `{"OR": [...]}`, `{"AND": [...]}`
pub(super) fn condition_holds(when: &Value, props: &[(&str, &str)]) -> bool {
    let Some(map) = when.as_object() else {
        return true;
    };
    if let Some(any) = map.get("OR").and_then(Value::as_array) {
        return any.iter().any(|c| condition_holds(c, props));
    }
    if let Some(all) = map.get("AND").and_then(Value::as_array) {
        return all.iter().all(|c| condition_holds(c, props));
    }
    map.iter().all(|(key, wanted)| {
        let wanted = match wanted {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let actual = props.iter().find(|&&(k, _)| k == key).map(|&(_, v)| v);
        match wanted.strip_prefix('!') {
            Some(not) => actual.is_some_and(|v| !not.split('|').any(|w| w == v)),
            None => actual.is_some_and(|v| wanted.split('|').any(|w| w == v)),
        }
    })
}
