//! The entity textures block entities draw with, stitched into the block atlas.

use super::DYES;
use super::models::{ChestHalf, SkullType};
use crate::block_models::ExtraTexture;
use image::RgbaImage;
use std::path::Path;
use std::sync::LazyLock;

pub(super) fn chest_texture(texture: &str, half: ChestHalf) -> String {
    match half {
        ChestHalf::Single => format!("minecraft:entity/chest/{texture}"),
        ChestHalf::Left => format!("minecraft:entity/chest/{texture}_left"),
        ChestHalf::Right => format!("minecraft:entity/chest/{texture}_right"),
    }
}

pub(super) fn banner_texture(color: usize) -> String {
    format!("melon:banner/{}", DYES[color].0)
}

/// [`banner_texture`] of every dye, so drawing formats nothing.
pub(super) static BANNER_SPRITES: LazyLock<Vec<String>> =
    LazyLock::new(|| (0..DYES.len()).map(banner_texture).collect());

pub(super) const BANNER_BASE: &str = "minecraft:entity/banner/banner_base";
pub(super) const BELL_BODY: &str = "minecraft:entity/bell/bell_body";
pub(super) const BOOK: &str = "minecraft:entity/enchantment/enchanting_table_book";
pub(super) const POT_BASE: &str = "minecraft:entity/decorated_pot/decorated_pot_base";
pub(super) const POT_SIDE: &str = "minecraft:entity/decorated_pot/decorated_pot_side";
pub(super) const CONDUIT_SHELL: &str = "minecraft:entity/conduit/base";
pub(super) const END_PORTAL: &str = "minecraft:entity/end_portal/end_portal";

/// A banner's flag in dye `color`.
pub(super) fn banner_flag(assets_root: &Path, color: usize) -> Option<RgbaImage> {
    let load = |id: &str| -> Option<RgbaImage> { melon_assets::assets::texture_image(assets_root, id) };
    let mut flag = load(BANNER_BASE)?;
    let mut dyed = load("minecraft:entity/banner/base")?;
    let rgb = DYES[color].1;
    melon_assets::assets::tint(&mut dyed, [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
    image::imageops::overlay(&mut flag, &dyed, 0, 0);
    Some(flag)
}

/// Every texture the renderers here sample, for the block atlas.
pub fn atlas_textures(assets_root: &Path) -> Vec<ExtraTexture> {
    let file = |id: &str| ExtraTexture {
        id: id.to_owned(),
        image: None,
    };
    let mut textures = Vec::new();
    for texture in [
        "normal",
        "trapped",
        "copper",
        "copper_exposed",
        "copper_weathered",
        "copper_oxidized",
    ] {
        for half in [ChestHalf::Single, ChestHalf::Left, ChestHalf::Right] {
            textures.push(file(&chest_texture(texture, half)));
        }
    }
    textures.push(file(&chest_texture("ender", ChestHalf::Single)));
    textures.push(file("minecraft:entity/shulker/shulker"));
    for (dye, _) in DYES {
        textures.push(file(&format!("minecraft:entity/shulker/shulker_{dye}")));
    }
    for skull in [
        SkullType::Skeleton,
        SkullType::WitherSkeleton,
        SkullType::Zombie,
        SkullType::Creeper,
        SkullType::Dragon,
        SkullType::Piglin,
        SkullType::Player,
    ] {
        textures.push(file(skull.texture()));
    }
    for id in [
        BANNER_BASE,
        BELL_BODY,
        BOOK,
        POT_BASE,
        POT_SIDE,
        CONDUIT_SHELL,
        END_PORTAL,
    ] {
        textures.push(file(id));
    }
    for color in 0..DYES.len() {
        if let Some(flag) = banner_flag(assets_root, color) {
            textures.push(ExtraTexture {
                id: banner_texture(color),
                image: Some(flag),
            });
        }
    }
    textures
}
