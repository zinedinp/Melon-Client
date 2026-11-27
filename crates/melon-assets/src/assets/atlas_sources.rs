//! Vanilla `SpriteSourceList`: what an atlas's `atlases/<name>.json` from
//! every pack names its sprites and which file each comes from.
//!
//! A sprite is usually named after its file, but a `directory` source with
//! its own prefix, a `single` source with its own sprite name or a
//! `paletted_permutations` source names sprites no file carries. Models use
//! those names, so [`resolve`] turns them back into files for every texture
//! read ([`super::texture_with_meta`], [`super::texture_image`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use super::packs::{self, ResourceStack};

/// Where one sprite's pixels come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpriteFile {
    /// `textures/<id>.png`.
    Texture(String),
    /// `texture` recoloured from palette `key` to `palette`
    /// (`PalettedPermutations`).
    Paletted {
        texture: String,
        key: String,
        palette: String,
    },
}

/// The sprites of atlas `minecraft:<atlas>`, by `namespace:path` name: the
/// sources of every copy of its JSON, vanilla's first, applied in order
/// (a later source replaces an earlier one's sprite of the same name).
/// `directory` sources over a folder in `skip_dirs` are left out.
pub fn sprites(
    stack: &ResourceStack,
    assets_root: &Path,
    atlas: &str,
    skip_dirs: &[&str],
) -> BTreeMap<String, SpriteFile> {
    let namespaces = stack.namespaces(assets_root);
    let mut out = BTreeMap::new();
    for bytes in stack.find_all(assets_root, "minecraft", &format!("atlases/{atlas}.json")) {
        let Ok(json) = serde_json::from_slice::<Value>(packs::meta::strip_bom(&bytes)) else {
            continue;
        };
        for source in json["sources"].as_array().into_iter().flatten() {
            apply(stack, assets_root, &namespaces, skip_dirs, source, &mut out);
        }
    }
    out
}

/// One `sources` entry onto `out`.
fn apply(
    stack: &ResourceStack,
    assets_root: &Path,
    namespaces: &BTreeSet<String>,
    skip_dirs: &[&str],
    source: &Value,
    out: &mut BTreeMap<String, SpriteFile>,
) {
    let kind = source["type"].as_str().unwrap_or("");
    match kind.trim_start_matches("minecraft:") {
        "directory" => {
            let Some(dir) = source["source"].as_str() else {
                return;
            };
            let dir = dir.trim_end_matches('/');
            if skip_dirs.contains(&dir) {
                return;
            }
            let prefix = source["prefix"].as_str().unwrap_or("");
            let base = format!("textures/{dir}/");
            for namespace in namespaces {
                for path in stack.list(assets_root, namespace, &format!("textures/{dir}")) {
                    let Some(rel) = path
                        .strip_prefix(&base)
                        .and_then(|p| p.strip_suffix(".png"))
                    else {
                        continue;
                    };
                    out.insert(
                        format!("{namespace}:{prefix}{rel}"),
                        SpriteFile::Texture(format!("{namespace}:{dir}/{rel}")),
                    );
                }
            }
        }
        "single" => {
            let Some(resource) = source["resource"].as_str() else {
                return;
            };
            let name = source["sprite"].as_str().unwrap_or(resource);
            out.insert(canonical(name), SpriteFile::Texture(canonical(resource)));
        }
        "paletted_permutations" => {
            let Some(key) = source["palette_key"].as_str() else {
                return;
            };
            let separator = source["separator"].as_str().unwrap_or("_");
            let permutations = source["permutations"].as_object();
            for texture in source["textures"].as_array().into_iter().flatten() {
                let Some(texture) = texture.as_str() else {
                    continue;
                };
                for (suffix, palette) in permutations.into_iter().flatten() {
                    let Some(palette) = palette.as_str() else {
                        continue;
                    };
                    out.insert(
                        format!("{}{separator}{suffix}", canonical(texture)),
                        SpriteFile::Paletted {
                            texture: canonical(texture),
                            key: canonical(key),
                            palette: canonical(palette),
                        },
                    );
                }
            }
        }
        "filter" => {
            let pattern = &source["pattern"];
            let compile = |key: &str| match pattern[key].as_str() {
                Some(re) => regex::Regex::new(re).ok().map(Some),
                None => Some(None),
            };
            let (Some(namespace), Some(path)) = (compile("namespace"), compile("path")) else {
                return;
            };
            out.retain(|name, _| {
                let (ns, p) = super::split_id(name);
                let hit = namespace.as_ref().is_none_or(|re| re.is_match(ns))
                    && path.as_ref().is_none_or(|re| re.is_match(p));
                !hit
            });
        }
        // `unstitch` cuts legacy sheets: such sprites keep a texture of
        // their own.
        _ => {}
    }
}

/// The sprites of the item and block atlases whose name is not their
/// file: what [`resolve`] looks up.
type Aliases = HashMap<String, SpriteFile>;

static ALIASES: RwLock<Option<(Arc<ResourceStack>, Arc<Aliases>)>> = RwLock::new(None);

/// The file behind sprite `id` of the item or block atlas, when it is not
/// `textures/<id>.png` itself: a prefixed folder, a renamed single or a
/// paletted permutation. Worked out once per pack stack.
pub fn resolve(assets_root: &Path, stack: &Arc<ResourceStack>, id: &str) -> Option<SpriteFile> {
    let aliases = aliases(assets_root, stack);
    if aliases.is_empty() {
        return None;
    }
    aliases.get(&canonical(id)).cloned()
}

fn aliases(assets_root: &Path, stack: &Arc<ResourceStack>) -> Arc<Aliases> {
    if let Some((known, aliases)) = &*ALIASES.read().unwrap_or_else(|e| e.into_inner())
        && Arc::ptr_eq(known, stack)
    {
        return aliases.clone();
    }
    let mut built = Aliases::new();
    for atlas in ["blocks", "items"] {
        for (name, file) in sprites(stack, assets_root, atlas, &[]) {
            if file != SpriteFile::Texture(name.clone()) {
                built.insert(name, file);
            }
        }
    }
    let built = Arc::new(built);
    *ALIASES.write().unwrap_or_else(|e| e.into_inner()) = Some((stack.clone(), built.clone()));
    built
}

/// `namespace:path`, `minecraft` when none is given.
pub fn canonical(id: &str) -> String {
    let (namespace, path) = super::split_id(id);
    format!("{namespace}:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(sources: Value) -> BTreeMap<String, SpriteFile> {
        let stack = ResourceStack::default();
        let root = Path::new("/nonexistent");
        let mut out = BTreeMap::new();
        for source in sources.as_array().unwrap() {
            apply(&stack, root, &BTreeSet::new(), &[], source, &mut out);
        }
        out
    }

    #[test]
    fn singles_permutations_and_filters_name_their_sprites() {
        let out = run(json!([
            {"type": "single", "resource": "pack:custom/sword"},
            {"type": "minecraft:single", "resource": "pack:art/axe_v2", "sprite": "pack:item/axe"},
            {"type": "paletted_permutations", "palette_key": "trim_base",
             "permutations": {"gold": "minecraft:trim/gold"},
             "textures": ["minecraft:trims/items/boots_trim"]},
            {"type": "paletted_permutations", "palette_key": "pack:key", "separator": "-",
             "permutations": {"red": "pack:red"}, "textures": ["pack:gem"]},
            {"type": "single", "resource": "pack:drop/me"},
            {"type": "filter", "pattern": {"namespace": "pack", "path": "^drop/"}},
        ]));
        assert_eq!(
            out["pack:custom/sword"],
            SpriteFile::Texture("pack:custom/sword".into())
        );
        assert_eq!(
            out["pack:item/axe"],
            SpriteFile::Texture("pack:art/axe_v2".into())
        );
        assert_eq!(
            out["minecraft:trims/items/boots_trim_gold"],
            SpriteFile::Paletted {
                texture: "minecraft:trims/items/boots_trim".into(),
                key: "minecraft:trim_base".into(),
                palette: "minecraft:trim/gold".into(),
            }
        );
        assert!(out.contains_key("pack:gem-red"));
        assert!(!out.contains_key("pack:drop/me"));
    }
}
