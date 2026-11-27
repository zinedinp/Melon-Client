//! Block entity models as items.

use super::DYES;
use super::models::{self, ChestHalf, SkullType};
use super::textures::{
    BANNER_BASE, CONDUIT_SHELL, POT_BASE, POT_SIDE, banner_flag, banner_texture, chest_texture,
};
use crate::math::angle::PI;
use crate::model_part::Part;
use glam::Vec2;
use image::RgbaImage;
use serde_json::Value;
use std::path::Path;

/// One part of a special item model and the texture it samples.
pub struct ItemPart {
    pub part: Part,
    /// A texture ID, or a composed one [`texture_image`] makes.
    pub texture: String,
    /// The model's texture size in texels.
    pub texture_size: Vec2,
}

/// The parts the special model `spec` (its `type` and options) draws, or `None`.
pub fn special_parts(spec: &Value) -> Option<Vec<ItemPart>> {
    let kind = spec["type"].as_str()?.trim_start_matches("minecraft:");
    let option = |key: &str| {
        spec[key]
            .as_str()
            .map(|v| v.trim_start_matches("minecraft:"))
    };
    let part = |part: Part, texture: String, texture_size: Vec2| ItemPart {
        part,
        texture,
        texture_size,
    };
    Some(match kind {
        "chest" => {
            let model = models::chest(ChestHalf::Single);
            let texture = chest_texture(option("texture")?, ChestHalf::Single);
            [model.bottom, model.lid, model.lock]
                .into_iter()
                .map(|p| part(p, texture.clone(), models::CHEST_TEXTURE))
                .collect()
        }
        "shulker_box" => {
            let model = models::shulker_box();
            let texture = format!("minecraft:entity/shulker/{}", option("texture")?);
            vec![
                part(model.base, texture.clone(), models::SHULKER_TEXTURE),
                part(model.lid, texture, models::SHULKER_TEXTURE),
            ]
        }
        "banner" => {
            let color = DYES
                .iter()
                .position(|&(dye, _)| Some(dye) == option("color"))?;
            let mut model = models::banner(true);
            model.flag.pose.rotation.x = (-0.0125 + 0.01) * PI;
            vec![
                part(model.frame, BANNER_BASE.to_owned(), models::BANNER_TEXTURE),
                part(model.flag, banner_texture(color), models::BANNER_TEXTURE),
            ]
        }
        "head" | "player_head" => {
            let skull = if kind == "player_head" {
                SkullType::Player
            } else {
                match option("kind")? {
                    "skeleton" => SkullType::Skeleton,
                    "wither_skeleton" => SkullType::WitherSkeleton,
                    "zombie" => SkullType::Zombie,
                    "creeper" => SkullType::Creeper,
                    "dragon" => SkullType::Dragon,
                    "piglin" => SkullType::Piglin,
                    _ => return None,
                }
            };
            vec![part(
                models::skull(skull),
                skull.texture().to_owned(),
                skull.texture_size(),
            )]
        }
        "decorated_pot" => {
            let base = models::pot_base()
                .into_iter()
                .map(|p| part(p, POT_BASE.to_owned(), models::POT_BASE_TEXTURE));
            let sides = models::pot_sides()
                .into_iter()
                .map(|p| part(p, POT_SIDE.to_owned(), models::POT_SIDE_TEXTURE));
            base.chain(sides).collect()
        }
        "conduit" => vec![part(
            models::conduit_shell(),
            CONDUIT_SHELL.to_owned(),
            models::CONDUIT_TEXTURE,
        )],
        "shield" => models::shield()
            .into_iter()
            .map(|p| {
                part(
                    p,
                    "minecraft:entity/shield/shield_base_nopattern".to_owned(),
                    models::SHIELD_TEXTURE,
                )
            })
            .collect(),
        "trident" => models::trident()
            .into_iter()
            .map(|p| {
                part(
                    p,
                    "minecraft:entity/trident/trident".to_owned(),
                    models::TRIDENT_TEXTURE,
                )
            })
            .collect(),
        _ => return None,
    })
}

/// The image of a texture [`special_parts`] composed rather than loaded.
pub fn texture_image(assets_root: &Path, id: &str) -> Option<RgbaImage> {
    let color = (0..DYES.len()).find(|&color| banner_texture(color) == id)?;
    banner_flag(assets_root, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trident_is_a_pole_and_a_three_spiked_head() {
        let parts = special_parts(&serde_json::json!({ "type": "minecraft:trident" })).unwrap();
        assert_eq!(parts.len(), 5);
        assert!(
            parts
                .iter()
                .all(|p| p.texture == "minecraft:entity/trident/trident")
        );
    }
}
