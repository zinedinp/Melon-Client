//! `SlotDisplay` and `RecipeDisplay`.

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{RegistryKey, get_tag_ids};

/// An item stack with the comparison and printing a recipe display needs:
#[derive(Clone)]
pub struct Shown(pub ItemStack);

impl PartialEq for Shown {
    fn eq(&self, other: &Self) -> bool {
        self.0.item.id == other.0.item.id && self.0.item_count == other.0.item_count
    }
}

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x {}", self.0.item_count, self.0.item.registry_key)
    }
}

/// A `HolderSet<Item>`: a tag the receiver knows, or the items themselves.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemSet {
    Tag(String),
    Items(Vec<&'static Item>),
}

impl ItemSet {
    /// The items in the set, in the registry's order.
    pub fn items(&self) -> Vec<&'static Item> {
        match self {
            Self::Items(items) => items.clone(),
            Self::Tag(name) => {
                let name = if name.contains(':') {
                    name.clone()
                } else {
                    format!("minecraft:{name}")
                };
                get_tag_ids(RegistryKey::Item, &name)
                    .into_iter()
                    .flatten()
                    .filter_map(|&id| Item::from_id(id))
                    .collect()
            }
        }
    }

    pub fn contains(&self, item: &Item) -> bool {
        match self {
            Self::Items(items) => items.iter().any(|i| i.id == item.id),
            Self::Tag(_) => self.items().iter().any(|i| i.id == item.id),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Items(items) => items.is_empty(),
            Self::Tag(_) => self.items().is_empty(),
        }
    }
}

/// One slot of a recipe.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotDisplay {
    Empty,
    /// TODO(fuel): the client has no fuel list (`minecraft:cooking_fuel`
    AnyFuel,
    /// TODO(potions): list one stack per potion (`WithAnyPotion`).
    WithAnyPotion(Box<SlotDisplay>),
    /// TODO(components): filter by the component once stacks carry theirs.
    OnlyWithComponent(Box<SlotDisplay>, i32),
    Item(&'static Item),
    Stack(Shown),
    Set(ItemSet),
    /// A dye over a target; shown as the target.
    Dyed {
        dye: Box<SlotDisplay>,
        target: Box<SlotDisplay>,
    },
    /// An armor trim demo; shown as the base.
    SmithingTrim {
        base: Box<SlotDisplay>,
        material: Box<SlotDisplay>,
        pattern: i32,
    },
    /// `input`, which leaves `remainder` behind (a bucket, a bottle).
    WithRemainder {
        input: Box<SlotDisplay>,
        remainder: Box<SlotDisplay>,
    },
    Composite(Vec<SlotDisplay>),
}

impl SlotDisplay {
    /// The stacks the slot cycles through (`resolveForStacks`).
    pub fn stacks(&self) -> Vec<ItemStack> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    /// The first of them, if any (`resolveForFirstStack`).
    pub fn first(&self) -> Option<ItemStack> {
        self.stacks().into_iter().next()
    }

    fn collect(&self, out: &mut Vec<ItemStack>) {
        match self {
            Self::Empty | Self::AnyFuel => {}
            Self::WithAnyPotion(inner) | Self::OnlyWithComponent(inner, _) => inner.collect(out),
            Self::Item(item) => out.push(ItemStack::new(1, item)),
            Self::Stack(stack) => out.push(stack.0.clone()),
            Self::Set(set) => out.extend(set.items().into_iter().map(|i| ItemStack::new(1, i))),
            Self::Dyed { target, .. } => target.collect(out),
            Self::SmithingTrim { base, .. } => base.collect(out),
            Self::WithRemainder { input, .. } => input.collect(out),
            Self::Composite(parts) => parts.iter().for_each(|p| p.collect(out)),
        }
    }

    /// Whether `item` is one of the stacks shown.
    pub fn shows(&self, item: &Item) -> bool {
        self.stacks().iter().any(|s| s.item.id == item.id)
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

/// How a whole recipe is drawn (`RecipeDisplay`).
#[derive(Clone, Debug, PartialEq)]
pub enum RecipeDisplay {
    Shapeless {
        ingredients: Vec<SlotDisplay>,
        result: SlotDisplay,
        station: SlotDisplay,
    },
    /// `ingredients` is `width * height` slots, row by row.
    Shaped {
        width: usize,
        height: usize,
        ingredients: Vec<SlotDisplay>,
        result: SlotDisplay,
        station: SlotDisplay,
    },
    Furnace {
        ingredient: SlotDisplay,
        fuel: SlotDisplay,
        result: SlotDisplay,
        station: SlotDisplay,
        duration: i32,
        experience: f32,
    },
    Stonecutter {
        input: SlotDisplay,
        result: SlotDisplay,
        station: SlotDisplay,
    },
    Smithing {
        template: SlotDisplay,
        base: SlotDisplay,
        addition: SlotDisplay,
        result: SlotDisplay,
        station: SlotDisplay,
    },
}

impl RecipeDisplay {
    pub fn result(&self) -> &SlotDisplay {
        match self {
            Self::Shapeless { result, .. }
            | Self::Shaped { result, .. }
            | Self::Furnace { result, .. }
            | Self::Stonecutter { result, .. }
            | Self::Smithing { result, .. } => result,
        }
    }

    /// The block the recipe is made at (`craftingStation`).
    pub fn station(&self) -> &SlotDisplay {
        match self {
            Self::Shapeless { station, .. }
            | Self::Shaped { station, .. }
            | Self::Furnace { station, .. }
            | Self::Stonecutter { station, .. }
            | Self::Smithing { station, .. } => station,
        }
    }

    /// The input slots as the crafting grid (`grid_width` square) or the
    /// machine lays them out (`fillGhostRecipe`).
    pub fn ghost_slots(&self, grid_width: usize) -> Vec<(usize, &SlotDisplay)> {
        match self {
            Self::Shapeless { ingredients, .. } => ingredients
                .iter()
                .take(grid_width * grid_width)
                .enumerate()
                .filter(|(_, s)| !s.is_empty())
                .collect(),
            Self::Shaped {
                width,
                height,
                ingredients,
                ..
            } => place_recipe(grid_width, grid_width, *width, *height, ingredients.len())
                .into_iter()
                .zip(ingredients)
                .filter(|(_, s)| !s.is_empty())
                .map(|((index, ..), s)| (index, s))
                .collect(),
            Self::Furnace { ingredient, .. } => vec![(0, ingredient)],
            Self::Stonecutter { input, .. } => vec![(0, input)],
            Self::Smithing {
                template,
                base,
                addition,
                ..
            } => vec![(0, template), (1, base), (2, addition)],
        }
    }

    /// Each input of a crafting recipe on the recipe book's 3x3 mini grid
    /// (`OverlayCraftingRecipeButton.calculateIngredientsPositions`): `(x, y)`
    /// and what goes there; a furnace's one input in the middle.
    pub fn mini_grid(&self) -> Vec<((usize, usize), &SlotDisplay)> {
        let cells: Vec<((usize, usize), &SlotDisplay)> = match self {
            Self::Shaped {
                width,
                height,
                ingredients,
                ..
            } => place_recipe(3, 3, *width, *height, ingredients.len())
                .into_iter()
                .zip(ingredients)
                .map(|((_, x, y), s)| ((x, y), s))
                .collect(),
            Self::Shapeless { ingredients, .. } => ingredients
                .iter()
                .enumerate()
                .map(|(i, s)| ((i % 3, i / 3), s))
                .collect(),
            Self::Furnace { ingredient, .. } => vec![((1, 1), ingredient)],
            _ => Vec::new(),
        };
        cells.into_iter().filter(|(_, s)| !s.is_empty()).collect()
    }

    /// Whether the recipe needs more than `size` x `size` slots.
    pub fn fits(&self, size: usize) -> bool {
        match self {
            Self::Shaped { width, height, .. } => *width <= size && *height <= size,
            Self::Shapeless { ingredients, .. } => ingredients.len() <= size * size,
            _ => true,
        }
    }
}

/// `PlaceRecipeHelper.placeRecipe`: `(grid index, x, y)` for each of the
/// first `entries` cells of a `recipe_w x recipe_h` recipe, row by row, in a
/// `grid_w x grid_h` grid. A recipe under half the grid wide or high is
/// centred; the row skip and its index jumps are vanilla's, kept as they are.
pub fn place_recipe(
    grid_w: usize,
    grid_h: usize,
    recipe_w: usize,
    recipe_h: usize,
    entries: usize,
) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::with_capacity(entries);
    let mut index = 0;
    let mut y = 0;
    while y < grid_h {
        let center_y = (recipe_h as f32) < grid_h as f32 / 2.0;
        let start_y = (grid_h as f32 / 2.0 - recipe_h as f32 / 2.0).floor() as usize;
        if center_y && start_y > y {
            index += grid_w;
            y += 1;
        }
        for x in 0..grid_w {
            if out.len() == entries {
                return out;
            }
            let center_x = (recipe_w as f32) < grid_w as f32 / 2.0;
            let start_x = (grid_w as f32 / 2.0 - recipe_w as f32 / 2.0).floor() as usize;
            let (span, add) = if center_x {
                (start_x + recipe_w, start_x <= x && x < start_x + recipe_w)
            } else {
                (recipe_w, x < recipe_w)
            };
            if add {
                out.push((index, x, y));
            } else if span == x {
                index += grid_w - x;
                break;
            }
            index += 1;
        }
        y += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composite_lists_every_part_and_a_tag_lists_its_items() {
        let slot = SlotDisplay::Composite(vec![
            SlotDisplay::Item(&Item::STICK),
            SlotDisplay::Empty,
            SlotDisplay::Set(ItemSet::Tag("minecraft:planks".into())),
        ]);
        let stacks = slot.stacks();
        assert!(stacks.len() > 2, "planks are many items");
        assert_eq!(stacks[0].item.id, Item::STICK.id);
        assert!(slot.shows(&Item::OAK_PLANKS));
        assert!(!slot.shows(&Item::DIAMOND));
    }

    #[test]
    fn place_recipe_centres_what_is_under_half_the_grid() {
        let indices = |w, h| -> Vec<usize> {
            place_recipe(3, 3, w, h, w * h).iter().map(|p| p.0).collect()
        };
        assert_eq!(indices(1, 1), [4]);
        assert_eq!(indices(1, 3), [1, 4, 7]);
        assert_eq!(indices(3, 1), [3, 4, 5]);
        assert_eq!(indices(2, 2), [0, 1, 3, 4]);
        assert_eq!(indices(3, 3), (0..9).collect::<Vec<_>>());
        assert_eq!(place_recipe(2, 2, 1, 2, 2).iter().map(|p| p.0).collect::<Vec<_>>(), [0, 2]);
    }

    #[test]
    fn a_shaped_recipe_keeps_its_shape_in_a_wider_grid() {
        let stick = SlotDisplay::Item(&Item::STICK);
        let display = RecipeDisplay::Shaped {
            width: 1,
            height: 2,
            ingredients: vec![stick.clone(), stick],
            result: SlotDisplay::Item(&Item::LADDER),
            station: SlotDisplay::Empty,
        };
        // One wide: the middle column, as vanilla's `placeRecipe` centres it.
        let slots: Vec<usize> = display.ghost_slots(3).iter().map(|s| s.0).collect();
        assert_eq!(slots, [1, 4]);
        let slots: Vec<usize> = display.ghost_slots(2).iter().map(|s| s.0).collect();
        assert_eq!(slots, [0, 2]);
        assert!(
            display.fits(2)
                && !RecipeDisplay::Shapeless {
                    ingredients: vec![SlotDisplay::Empty; 5],
                    result: SlotDisplay::Empty,
                    station: SlotDisplay::Empty
                }
                .fits(2)
        );
    }
}
