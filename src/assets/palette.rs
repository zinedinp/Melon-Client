//! Vanilla `PalettedPermutations` / `PaletteMapping`.

use image::{Rgba, RgbaImage};
use std::path::Path;

/// A palette mapping: each base colour (RGB) and what it becomes.
pub type ColorMapping = rustc_hash::FxHashMap<[u8; 3], [u8; 4]>;

/// `PaletteMapping.create`: `None` when the palettes differ in size (vanilla
/// throws, and the sprite is not made); a colour listed twice takes its last entry.
pub fn mapping(base: &RgbaImage, target: &RgbaImage) -> Option<ColorMapping> {
    if base.pixels().len() != target.pixels().len() {
        return None;
    }
    let mut out = ColorMapping::default();
    for (key, value) in base.pixels().zip(target.pixels()) {
        if key[3] != 0 {
            out.insert([key[0], key[1], key[2]], value.0);
        }
    }
    Some(out)
}

/// `PaletteMapping.apply` over every pixel.
pub fn apply(image: &RgbaImage, map: &ColorMapping) -> RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        if a == 0 {
            continue;
        }
        if let Some(&[tr, tg, tb, ta]) = map.get(&[r, g, b]) {
            let alpha = (u16::from(a) * u16::from(ta) / 255) as u8;
            *pixel = Rgba([tr, tg, tb, alpha]);
        }
    }
    out
}

/// `textures/palettes/<id>.png` (`Palette.ID_CONVERTER`).
fn palette_image(assets_root: &Path, id: &str) -> Option<RgbaImage> {
    crate::assets::decode_png(&crate::assets::resource(
        assets_root,
        "textures/palettes",
        id,
        "png",
    )?)
}

/// Recolor `base` (`textures/<id>.png`) with `palette` (`minecraft:trim/gold`).
pub fn recolor(assets_root: &Path, base_id: &str, palette_id: &str) -> Option<RgbaImage> {
    permute(assets_root, base_id, "trim_base", palette_id)
}

/// `PalettedPermutations`: `base` (`textures/<id>.png`) with the colours of
/// palette `key` swapped for those of `palette` (both `textures/palettes/`).
pub fn permute(assets_root: &Path, base_id: &str, key: &str, palette: &str) -> Option<RgbaImage> {
    let (png, _) = crate::assets::texture_with_meta(assets_root, base_id)?;
    let base = crate::assets::decode_png(&png)?;
    let key = palette_image(assets_root, key)?;
    let target = palette_image(assets_root, palette)?;
    Some(apply(&base, &mapping(&key, &target)?))
}

/// Atlas `paletted_permutations` for item trims: `trims/items/chestplate_trim_gold`
pub fn resolve(assets_root: &Path, identifier: &str) -> Option<RgbaImage> {
    let (_, path) = crate::assets::split_id(identifier);
    if let Some((base, palette)) = path.split_once('#') {
        return recolor(assets_root, base, palette.trim_start_matches("minecraft:"));
    }
    item_trim_permutation(assets_root, path)
}

fn item_trim_permutation(assets_root: &Path, path: &str) -> Option<RgbaImage> {
    const PIECES: [&str; 4] = [
        "trims/items/helmet_trim",
        "trims/items/chestplate_trim",
        "trims/items/leggings_trim",
        "trims/items/boots_trim",
    ];
    const MATERIALS: [&str; 16] = [
        "copper_darker",
        "diamond_darker",
        "gold_darker",
        "iron_darker",
        "netherite_darker",
        "amethyst",
        "copper",
        "diamond",
        "emerald",
        "gold",
        "iron",
        "lapis",
        "netherite",
        "quartz",
        "redstone",
        "resin",
    ];
    for piece in PIECES {
        let prefix = format!("{piece}_");
        let Some(rest) = path.strip_prefix(&prefix) else {
            continue;
        };
        if MATERIALS.contains(&rest) {
            return recolor(assets_root, piece, &format!("trim/{rest}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_skips_clear_base_entries_and_apply_keeps_source_alpha() {
        let mut base = RgbaImage::new(2, 1);
        base.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        base.put_pixel(1, 0, Rgba([1, 2, 3, 0]));
        let mut target = RgbaImage::new(2, 1);
        target.put_pixel(0, 0, Rgba([200, 100, 50, 128]));
        target.put_pixel(1, 0, Rgba([9, 9, 9, 255]));
        let map = mapping(&base, &target).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&[10, 20, 30]), Some(&[200, 100, 50, 128]));
        let mut src = RgbaImage::new(1, 1);
        src.put_pixel(0, 0, Rgba([10, 20, 30, 128]));
        let out = apply(&src, &map);
        assert_eq!(out.get_pixel(0, 0).0, [200, 100, 50, 64]);
    }

    #[test]
    fn a_repeated_colour_takes_its_last_entry_and_sizes_must_match() {
        let base = RgbaImage::from_pixel(2, 1, Rgba([10, 20, 30, 255]));
        let mut target = RgbaImage::new(2, 1);
        target.put_pixel(0, 0, Rgba([1, 1, 1, 255]));
        target.put_pixel(1, 0, Rgba([2, 2, 2, 255]));
        let map = mapping(&base, &target).unwrap();
        assert_eq!(map.get(&[10, 20, 30]), Some(&[2, 2, 2, 255]));
        assert!(mapping(&base, &RgbaImage::new(3, 1)).is_none());
    }
}
