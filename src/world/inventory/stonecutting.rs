//! The stonecutter's recipes.

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::RECIPES_STONECUTTING;

/// What `input` cuts into.
pub fn options_from(sets: &crate::world::recipes::RecipeSets, input: &ItemStack) -> Vec<ItemStack> {
    if !sets.received() {
        return options(input);
    }
    sets.stonecutter_options(input)
        .into_iter()
        .filter_map(|display| display.first())
        .collect()
}

/// What `input` cuts into, in the recipes' order; nothing for an empty stack.
pub fn options(input: &ItemStack) -> Vec<ItemStack> {
    if input.is_empty() {
        return Vec::new();
    }
    RECIPES_STONECUTTING
        .iter()
        .filter(|recipe| recipe.ingredient.match_item(input.item))
        .filter_map(|recipe| {
            let id = recipe.result.id;
            let item = Item::from_registry_key(id.strip_prefix("minecraft:").unwrap_or(id))?;
            Some(ItemStack::new(recipe.result.count, item))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_cuts_into_slabs_stairs_and_bricks_in_the_recipes_order() {
        let cut = options(&ItemStack::new(1, &Item::STONE));
        assert!(!cut.is_empty());
        let names: Vec<_> = cut.iter().map(|s| s.item.registry_key).collect();
        assert!(names.contains(&"stone_slab"));
        assert!(names.contains(&"stone_stairs"));
        assert_eq!(
            cut[names.iter().position(|n| *n == "stone_slab").unwrap()].item_count,
            2
        );
    }

    #[test]
    fn an_empty_input_and_an_item_without_recipes_cut_into_nothing() {
        assert!(options(&crate::world::inventory::empty()).is_empty());
        assert!(options(&ItemStack::new(1, &Item::DIAMOND)).is_empty());
    }
}
