//! `EquipmentAssetManager`: `equipment/<asset>.json` through the active packs.
//!
//! Before [`install`] (in tests) it guesses vanilla's assets from their names.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use pumpkin_data::data_component_impl::{DyedColorImpl, EquippableImpl};
use pumpkin_data::item_stack::ItemStack;

/// `EquipmentClientInfo.Layer`.
#[derive(Clone, Debug, PartialEq)]
struct Layer {
    /// `namespace:name`, under `textures/entity/equipment/<layer type>/`.
    texture: String,
    /// `dyeable`: its `color_when_undyed`, if it says one.
    dyeable: Option<Option<u32>>,
}

/// One asset's layers by `LayerType`; `any` for the built-in guesses.
#[derive(Debug, Default)]
struct Info {
    by_type: HashMap<String, Vec<Layer>>,
    any: Option<Vec<Layer>>,
}

impl Info {
    fn layers(&self, layer_type: &str) -> &[Layer] {
        self.by_type
            .get(layer_type)
            .or(self.any.as_ref())
            .map_or(&[], Vec::as_slice)
    }
}

#[derive(Default)]
struct Store {
    root: Option<PathBuf>,
    cache: HashMap<String, Arc<Info>>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(Mutex::default);

/// Reads the assets from `assets_root` from now on, forgetting those read.
pub fn install(assets_root: &Path) {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.root = Some(assets_root.to_path_buf());
    store.cache.clear();
}

fn info(asset: &str) -> Arc<Info> {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(info) = store.cache.get(asset) {
        return Arc::clone(info);
    }
    // A missing file draws nothing, as vanilla's `EquipmentClientInfo` default.
    let info = Arc::new(match &store.root {
        Some(root) => crate::assets::json_resource(root, "equipment", asset)
            .map(|json| parse(&json))
            .unwrap_or_default(),
        None => built_in(asset),
    });
    store.cache.insert(asset.to_owned(), Arc::clone(&info));
    info
}

fn parse(json: &serde_json::Value) -> Info {
    let by_type = json
        .get("layers")
        .and_then(serde_json::Value::as_object)
        .map(|layers| {
            layers
                .iter()
                .map(|(ty, list)| {
                    let list = list.as_array().map_or(&[][..], Vec::as_slice);
                    (ty.clone(), list.iter().filter_map(parse_layer).collect())
                })
                .collect()
        })
        .unwrap_or_default();
    Info { by_type, any: None }
}

fn parse_layer(json: &serde_json::Value) -> Option<Layer> {
    let texture = json.get("texture")?.as_str()?;
    let dyeable = json.get("dyeable").map(|d| {
        d.get("color_when_undyed")
            .and_then(serde_json::Value::as_i64)
            .map(|c| c as u32 & 0xFF_FFFF)
    });
    Some(Layer {
        texture: with_namespace(texture),
        dyeable,
    })
}

fn with_namespace(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

/// Vanilla's `equipment/*.json`, by the asset's name.
fn built_in(asset: &str) -> Info {
    let name = asset.strip_prefix("minecraft:").unwrap_or(asset);
    let plain = |texture: &str| Layer {
        texture: format!("minecraft:{texture}"),
        dyeable: None,
    };
    let dyed = |texture: &str, undyed| Layer {
        texture: format!("minecraft:{texture}"),
        dyeable: Some(undyed),
    };
    let any = match name {
        "leather" => vec![dyed("leather", Some(0xA0_6540)), plain("leather_overlay")],
        "armadillo_scute" => vec![
            plain("armadillo_scute"),
            dyed("armadillo_scute_overlay", None),
        ],
        _ => match name.strip_suffix("_carpet") {
            Some(dye) => vec![plain(dye)],
            None => vec![plain(name)],
        },
    };
    Info {
        by_type: HashMap::new(),
        any: Some(any),
    }
}

/// The equipment asset `stack` wears as (`Equippable.assetId`).
pub fn asset_of(stack: &ItemStack) -> Option<String> {
    if stack.is_empty() {
        return None;
    }
    stack
        .get_data_component::<EquippableImpl>()?
        .asset_id
        .as_deref()
        .map(str::to_owned)
}

/// `EquipmentLayerRenderer.renderLayers`'s textures and colours.
pub fn textures(stack: &ItemStack, layer_type: &str) -> Vec<(String, [f32; 3])> {
    let Some(asset) = asset_of(stack) else {
        return Vec::new();
    };
    let dye = stack
        .get_data_component::<DyedColorImpl>()
        .map(|d| d.rgb as u32 & 0xFF_FFFF);
    asset_textures(&asset, layer_type, dye)
}

/// [`textures`] of an asset worn without an item (a trader llama's blanket).
pub fn asset_textures(asset: &str, layer_type: &str, dye: Option<u32>) -> Vec<(String, [f32; 3])> {
    info(&with_namespace(asset))
        .layers(layer_type)
        .iter()
        .filter_map(|layer| {
            let rgb = match layer.dyeable {
                None => 0xFF_FFFF,
                Some(undyed) => dye.or(undyed)?,
            };
            let (namespace, path) = crate::assets::split_id(&layer.texture);
            Some((
                format!("{namespace}:entity/equipment/{layer_type}/{path}"),
                [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_names_its_layers_and_their_dyes() {
        let info = parse(&serde_json::json!({
            "layers": {
                "wolf_body": [
                    {"texture": "minecraft:armadillo_scute"},
                    {"texture": "armadillo_scute_overlay", "dyeable": {}}
                ],
                "humanoid": [{"texture": "minecraft:leather", "dyeable": {"color_when_undyed": -6265536}}]
            }
        }));
        let wolf = info.layers("wolf_body");
        assert_eq!(wolf.len(), 2);
        assert_eq!(wolf[1].texture, "minecraft:armadillo_scute_overlay");
        assert_eq!(wolf[1].dyeable, Some(None));
        assert_eq!(info.layers("humanoid")[0].dyeable, Some(Some(0xA0_6540)));
        assert!(info.layers("horse_body").is_empty());
    }

    #[test]
    fn an_undyed_overlay_without_a_colour_is_left_out() {
        let undyed = asset_textures("minecraft:armadillo_scute", "wolf_body", None);
        assert_eq!(undyed.len(), 1);
        let dyed = asset_textures("minecraft:armadillo_scute", "wolf_body", Some(0xFF_0000));
        assert_eq!(dyed.len(), 2);
        assert_eq!(dyed[1].1, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_carpet_draws_its_dye_on_a_llama() {
        let carpet = asset_textures("minecraft:lime_carpet", "llama_body", None);
        assert_eq!(carpet[0].0, "minecraft:entity/equipment/llama_body/lime");
    }
}
