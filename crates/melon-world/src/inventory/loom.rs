//! What the loom offers and shows.

use pumpkin_data::banner_pattern::BannerPattern;
use pumpkin_data::data_component_impl::{BannerPatternsImpl, DyeImpl, ProvidesBannerPatternsImpl};
use pumpkin_data::dye_color::DyeColor;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, RegistryKey};

/// `BannerPatternLayers.MAX_PATTERNS`: a banner has no room past six.
pub const MAX_LAYERS: usize = 6;

/// The patterns the loom offers with `pattern_item` in the pattern slot.
pub fn selectable_patterns(pattern_item: &ItemStack) -> Vec<BannerPattern> {
    let names: &[&str] = if pattern_item.is_empty() {
        tag::BannerPattern::MINECRAFT_NO_ITEM_REQUIRED.0
    } else {
        let Some(provides) = pattern_item.get_data_component::<ProvidesBannerPatternsImpl>() else {
            return Vec::new();
        };
        let key = provides.key.strip_prefix('#').unwrap_or(&provides.key);
        tag::get_tag_values(RegistryKey::BannerPattern, key).unwrap_or(&[])
    };
    names
        .iter()
        .filter_map(|name| BannerPattern::from_name(name))
        .collect()
}

/// The dye of a dye item; white for anything else (`getOrDefault(DYE, WHITE)`).
pub fn dye_color(stack: &ItemStack) -> DyeColor {
    stack
        .get_data_component::<DyeImpl>()
        .map_or(DyeColor::White, |dye| dye.color)
}

/// The base colour of a banner item (`red_banner` is red).
pub fn banner_color(banner: &ItemStack) -> Option<DyeColor> {
    banner
        .item
        .registry_key
        .strip_suffix("_banner")
        .and_then(DyeColor::by_name)
}

/// The layers a banner carries on top of its base colour.
pub fn layers(banner: &ItemStack) -> Vec<(BannerPattern, DyeColor)> {
    banner
        .get_data_component::<BannerPatternsImpl>()
        .map_or_else(Vec::new, |patterns| {
            patterns
                .layers
                .iter()
                .filter_map(|layer| Some((BannerPattern::from_name(&layer.pattern)?, layer.color)))
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::empty;
    use pumpkin_data::item::Item;

    #[test]
    fn an_empty_pattern_slot_offers_the_patterns_needing_no_item() {
        let patterns = selectable_patterns(&empty());
        assert!(patterns.contains(&BannerPattern::StripeCenter));
        assert!(patterns.contains(&BannerPattern::SquareTopLeft));
        assert!(!patterns.contains(&BannerPattern::Creeper));
        assert!(!patterns.contains(&BannerPattern::Base));
    }

    #[test]
    fn a_pattern_item_with_no_pattern_data_offers_nothing() {
        assert!(selectable_patterns(&ItemStack::new(1, &Item::STONE)).is_empty());
    }

    #[test]
    fn a_banner_item_names_its_colour() {
        assert_eq!(
            banner_color(&ItemStack::new(1, &Item::RED_BANNER)),
            Some(DyeColor::Red)
        );
        assert_eq!(
            banner_color(&ItemStack::new(1, &Item::LIGHT_BLUE_BANNER)),
            Some(DyeColor::LightBlue)
        );
        assert_eq!(banner_color(&ItemStack::new(1, &Item::STONE)), None);
    }

    #[test]
    fn a_dye_item_gives_its_colour_and_anything_else_white() {
        assert_eq!(dye_color(&ItemStack::new(1, &Item::RED_DYE)), DyeColor::Red);
        assert_eq!(dye_color(&ItemStack::new(1, &Item::STONE)), DyeColor::White);
    }
}
