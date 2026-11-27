//! The colours tinted faces get (vanilla `BlockColors`).

use crate::block_tint::TintKind;

pub(super) const fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    ]
}

/// Colour baked into the quad (`1,1,1` when the mesher applies a biome colour).
pub(super) fn bake_tint(
    block: &str,
    tintindex: Option<usize>,
    props: &[(&str, &str)],
) -> ([f32; 3], TintKind) {
    let Some(index) = tintindex else {
        return ([1.0; 3], TintKind::None);
    };
    match (block.trim_start_matches("minecraft:"), index) {
        ("pink_petals" | "wildflowers", 0) => ([1.0; 3], TintKind::None),
        ("pink_petals" | "wildflowers", _) => ([1.0; 3], TintKind::Grass),
        ("grass_block" | "short_grass" | "fern" | "potted_fern" | "sugar_cane" | "bush", _) => {
            ([1.0; 3], TintKind::Grass)
        }
        ("tall_grass" | "large_fern", _) => {
            let kind = if props.iter().any(|&(k, v)| k == "half" && v == "upper") {
                TintKind::GrassBelow
            } else {
                TintKind::Grass
            };
            ([1.0; 3], kind)
        }
        (
            "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves"
            | "mangrove_leaves" | "vine",
            _,
        ) => ([1.0; 3], TintKind::Foliage),
        ("leaf_litter", _) => ([1.0; 3], TintKind::DryFoliage),
        ("water" | "bubble_column" | "water_cauldron", _) => ([1.0; 3], TintKind::Water),
        ("spruce_leaves", _) => (rgb(0x619961), TintKind::None),
        ("birch_leaves", _) => (rgb(0x80A755), TintKind::None),
        ("lily_pad", _) => (rgb(0x208030), TintKind::None),
        ("redstone_wire", _) => (rgb(0xFF0000), TintKind::None),
        ("attached_melon_stem" | "attached_pumpkin_stem", _) => (rgb(0xE0C71C), TintKind::None),
        ("melon_stem" | "pumpkin_stem", _) => (rgb(0xE0C71C), TintKind::None),
        _ => ([1.0; 3], TintKind::None),
    }
}

/// The tint a block's particles get (`BlockTintSource.colorAsTerrainParticle`).
pub(super) fn particle_tint(block: &str, props: &[(&str, &str)]) -> ([f32; 3], TintKind) {
    if block.trim_start_matches("minecraft:") == "grass_block" {
        return ([1.0; 3], TintKind::None);
    }
    bake_tint(block, Some(0), props)
}

/// Whether vanilla's `BlockColors` gives the block a tint source at all.
pub(super) fn has_tint_source(block: &str) -> bool {
    matches!(
        block.trim_start_matches("minecraft:"),
        "large_fern"
            | "tall_grass"
            | "fern"
            | "short_grass"
            | "potted_fern"
            | "bush"
            | "grass_block"
            | "pink_petals"
            | "wildflowers"
            | "spruce_leaves"
            | "birch_leaves"
            | "oak_leaves"
            | "jungle_leaves"
            | "acacia_leaves"
            | "dark_oak_leaves"
            | "vine"
            | "mangrove_leaves"
            | "leaf_litter"
            | "water_cauldron"
            | "water"
            | "bubble_column"
            | "redstone_wire"
            | "sugar_cane"
            | "attached_melon_stem"
            | "attached_pumpkin_stem"
            | "melon_stem"
            | "pumpkin_stem"
            | "lily_pad"
    )
}
