//! `ClientboundUpdateRecipesPacket`: `ClientRecipeContainer`.

use std::collections::HashMap;

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use super::display::{ItemSet, SlotDisplay};

/// `RecipePropertySet`: the items that fit one slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertySet {
    items: Vec<u16>,
}

impl PropertySet {
    pub fn new(items: impl IntoIterator<Item = &'static Item>) -> Self {
        let mut items: Vec<u16> = items.into_iter().map(|i| i.id).collect();
        items.sort_unstable();
        items.dedup();
        Self { items }
    }

    pub fn test(&self, stack: &ItemStack) -> bool {
        !stack.is_empty() && self.items.binary_search(&stack.item.id).is_ok()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Names of the sets vanilla defines (`RecipePropertySet`).
pub const SMITHING_BASE: &str = "minecraft:smithing_base";
pub const SMITHING_TEMPLATE: &str = "minecraft:smithing_template";
pub const SMITHING_ADDITION: &str = "minecraft:smithing_addition";
pub const FURNACE_INPUT: &str = "minecraft:furnace_input";
pub const BLAST_FURNACE_INPUT: &str = "minecraft:blast_furnace_input";
pub const SMOKER_INPUT: &str = "minecraft:smoker_input";
pub const CAMPFIRE_INPUT: &str = "minecraft:campfire_input";
pub const BREWING_INPUT: &str = "minecraft:brewing_input";
pub const BREWING_REAGENT: &str = "minecraft:brewing_reagent";

/// One stonecutter option: for inputs in `input`, show `display`.
#[derive(Clone, Debug, PartialEq)]
pub struct StonecutterOption {
    pub input: ItemSet,
    pub display: SlotDisplay,
}

/// What the last update said; empty until the server sends one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeSets {
    sets: HashMap<String, PropertySet>,
    stonecutter: Vec<StonecutterOption>,
    /// Whether an update came at all.
    received: bool,
}

impl RecipeSets {
    pub fn new(sets: HashMap<String, PropertySet>, stonecutter: Vec<StonecutterOption>) -> Self {
        Self {
            sets,
            stonecutter,
            received: true,
        }
    }

    pub fn received(&self) -> bool {
        self.received
    }

    /// `set`'s test of `stack`; with no update yet, every non-empty stack passes.
    pub fn accepts(&self, set: &str, stack: &ItemStack) -> bool {
        if !self.received {
            return !stack.is_empty();
        }
        self.sets.get(set).is_some_and(|s| s.test(stack))
    }

    pub fn set(&self, name: &str) -> Option<&PropertySet> {
        self.sets.get(name)
    }

    /// `SingleInputSet.selectByInput`.
    pub fn stonecutter_options(&self, input: &ItemStack) -> Vec<&SlotDisplay> {
        if input.is_empty() {
            return Vec::new();
        }
        self.stonecutter
            .iter()
            .filter(|o| o.input.contains(input.item))
            .map(|o| &o.display)
            .collect()
    }

    pub fn stonecutter(&self) -> &[StonecutterOption] {
        &self.stonecutter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_tests_by_item_and_an_unsent_update_lets_everything_in() {
        let mut sets = HashMap::new();
        sets.insert(
            FURNACE_INPUT.to_owned(),
            PropertySet::new([&Item::IRON_ORE, &Item::BEEF]),
        );
        let sets = RecipeSets::new(sets, Vec::new());
        assert!(sets.accepts(FURNACE_INPUT, &ItemStack::new(1, &Item::BEEF)));
        assert!(!sets.accepts(FURNACE_INPUT, &ItemStack::new(1, &Item::DIAMOND)));
        assert!(!sets.accepts("minecraft:unknown", &ItemStack::new(1, &Item::BEEF)));
        assert!(RecipeSets::default().accepts(SMOKER_INPUT, &ItemStack::new(1, &Item::BEEF)));
    }

    #[test]
    fn the_stonecutter_lists_options_in_the_servers_order() {
        let option = |item: &'static Item| StonecutterOption {
            input: ItemSet::Items(vec![&Item::STONE]),
            display: SlotDisplay::Item(item),
        };
        let sets = RecipeSets::new(
            HashMap::new(),
            vec![option(&Item::STONE_SLAB), option(&Item::STONE_STAIRS)],
        );
        let cut = sets.stonecutter_options(&ItemStack::new(1, &Item::STONE));
        assert_eq!(cut.len(), 2);
        assert!(cut[0].shows(&Item::STONE_SLAB));
        assert!(
            sets.stonecutter_options(&ItemStack::new(1, &Item::DIRT))
                .is_empty()
        );
    }
}
