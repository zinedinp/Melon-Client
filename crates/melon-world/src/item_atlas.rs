//! Which textures go into the item atlas, built once per resource load
//! (vanilla `AtlasProvider.itemsList`: the sources of `atlases/items.json`,
//! every `textures/item/` file and the trim permutations), plus what the
//! packs' `atlases/blocks.json` adds outside the block folder, since
//! custom-model packs list their own folders there.
//!
//! Held items and GUI icons sample this atlas or the block atlas; only
//! what neither holds (a downloaded skin, a composed banner, a texture no
//! atlas source names) gets a texture of its own.

use std::collections::BTreeSet;
use std::path::Path;

use melon_assets::assets::atlas_sources;
use melon_assets::assets::packs;

/// Every sprite name (`namespace:path`) the item atlas packs. Each is read
/// through [`melon_assets::assets::texture_image`], which finds the file behind a
/// prefixed or paletted name.
pub fn texture_ids(assets_root: &Path) -> Vec<String> {
    let stack = packs::active();
    let mut ids: BTreeSet<String> = atlas_sources::sprites(&stack, assets_root, "items", &[])
        .into_keys()
        .collect();
    // The whole block folder is the block atlas's.
    ids.extend(atlas_sources::sprites(&stack, assets_root, "blocks", &["block"]).into_keys());
    if ids.is_empty() {
        // No `items.json` at all (a broken asset tree): the item folders.
        for namespace in stack.namespaces(assets_root) {
            for path in stack.list(assets_root, &namespace, "textures/item") {
                if let Some(texture) = path
                    .strip_prefix("textures/")
                    .and_then(|p| p.strip_suffix(".png"))
                {
                    ids.insert(format!("{namespace}:{texture}"));
                }
            }
        }
    }
    ids.into_iter().collect()
}

/// `namespace:path`, `minecraft` when none is given.
pub fn canonical(id: &str) -> String {
    atlas_sources::canonical(id)
}
