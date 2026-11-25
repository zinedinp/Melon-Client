//! The recipe packets.

use std::collections::HashMap;

use anyhow::{Result, bail};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;

use crate::network::protocol::packet::Reader;
use crate::network::protocol::reader::initial_capacity;
use crate::network::protocol::varint;
use crate::world::recipes::book::{BookSettings, Category, Entry, RecipeId, TypeSettings};
use crate::world::recipes::display::Shown;
use crate::world::recipes::sets::{PropertySet, StonecutterOption};
use crate::world::recipes::{ItemSet, RecipeDisplay, RecipeSets, SlotDisplay};

/// A recipe packet, decoded.
pub enum RecipeEvent {
    /// `ClientboundUpdateRecipesPacket`.
    Update(RecipeSets),
    /// `ClientboundRecipeBookAddPacket`.
    BookAdd {
        entries: Vec<(Entry, u8)>,
        replace: bool,
    },
    /// `ClientboundRecipeBookRemovePacket`.
    BookRemove(Vec<RecipeId>),
    /// `ClientboundRecipeBookSettingsPacket`.
    BookSettings(BookSettings),
    /// `ClientboundPlaceGhostRecipePacket`.
    Ghost {
        container: i32,
        display: RecipeDisplay,
    },
}

pub const FLAG_NOTIFICATION: u8 = 1;
pub const FLAG_HIGHLIGHT: u8 = 2;

/// Deepest nesting of slot displays read (a malformed packet's recursion).
const MAX_DEPTH: u32 = 16;

pub fn decode(pid: i32, r: &mut Reader) -> Result<Option<RecipeEvent>> {
    use pumpkin_data::packet::clientbound::play as cb;
    Ok(Some(match pid {
        p if p == cb::UPDATE_RECIPES.0 => RecipeEvent::Update(r.recipe_sets()?),
        p if p == cb::RECIPE_BOOK_ADD.0 => {
            let count = r.list_count()?;
            let mut entries = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                let entry = r.recipe_entry()?;
                entries.push((entry, r.u8()?));
            }
            RecipeEvent::BookAdd {
                entries,
                replace: r.bool()?,
            }
        }
        p if p == cb::RECIPE_BOOK_REMOVE.0 => {
            let count = r.list_count()?;
            let mut ids = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                ids.push(RecipeId(r.var_int()?));
            }
            RecipeEvent::BookRemove(ids)
        }
        p if p == cb::RECIPE_BOOK_SETTINGS.0 => {
            let mut settings = [TypeSettings::default(); 4];
            for slot in &mut settings {
                *slot = TypeSettings {
                    open: r.bool()?,
                    filtering: r.bool()?,
                };
            }
            RecipeEvent::BookSettings(BookSettings::new(settings))
        }
        p if p == cb::PLACE_GHOST_RECIPE.0 => RecipeEvent::Ghost {
            container: r.var_int()?,
            display: r.recipe_display()?,
        },
        _ => return Ok(None),
    }))
}

impl Reader<'_> {
    /// `Item.STREAM_CODEC`: the item's registry number.
    fn item_by_id(&mut self) -> Result<&'static Item> {
        let id = self.var_int()?;
        match u16::try_from(id).ok().and_then(Item::from_id) {
            Some(item) => Ok(item),
            None => bail!("unknown item {id}"),
        }
    }

    /// `ByteBufCodecs.holderSet(ITEM)`.
    pub fn item_set(&mut self) -> Result<ItemSet> {
        let n = self.var_int()?;
        if n == 0 {
            return Ok(ItemSet::Tag(self.string()?));
        }
        let Ok(count) = usize::try_from(n.wrapping_sub(1)) else {
            bail!("negative holder set size {}", n.wrapping_sub(1));
        };
        let mut items = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            items.push(self.item_by_id()?);
        }
        Ok(ItemSet::Items(items))
    }

    /// `ItemStackTemplate.STREAM_CODEC`.
    pub(crate) fn item_template(&mut self) -> Result<ItemStack> {
        let item = self.var_int()?;
        let count = self.var_int()?;
        let mut head = Vec::new();
        varint::write_var_int(&mut head, count)?;
        varint::write_var_int(&mut head, item)?;
        let rest = *self.remaining();
        let mut joined = head.clone();
        joined.extend_from_slice(rest);
        let mut slice = joined.as_slice();
        let stack = ItemStackSerializer::read(&mut slice)
            .map(ItemStackSerializer::to_stack)
            .map_err(|e| anyhow::anyhow!("item stack template: {e}"))?;
        let used = joined.len() - slice.len() - head.len();
        self.skip(used)?;
        Ok(stack)
    }

    pub fn slot_display(&mut self) -> Result<SlotDisplay> {
        self.slot_display_at(0)
    }

    fn slot_display_at(&mut self, depth: u32) -> Result<SlotDisplay> {
        if depth > MAX_DEPTH {
            bail!("slot display nested too deep");
        }
        let boxed = |r: &mut Self| r.slot_display_at(depth + 1).map(Box::new);
        Ok(match self.var_int()? {
            0 => SlotDisplay::Empty,
            1 => SlotDisplay::AnyFuel,
            2 => SlotDisplay::WithAnyPotion(boxed(self)?),
            3 => {
                let inner = boxed(self)?;
                SlotDisplay::OnlyWithComponent(inner, self.var_int()?)
            }
            4 => SlotDisplay::Item(self.item_by_id()?),
            5 => SlotDisplay::Stack(Shown(self.item_template()?)),
            6 => SlotDisplay::Set(self.item_set()?),
            7 => {
                let dye = boxed(self)?;
                SlotDisplay::Dyed {
                    dye,
                    target: boxed(self)?,
                }
            }
            8 => {
                let base = boxed(self)?;
                let material = boxed(self)?;
                let pattern = self.var_int()?;
                if pattern == 0 {
                    bail!("trim pattern written inline");
                }
                SlotDisplay::SmithingTrim {
                    base,
                    material,
                    pattern: pattern - 1,
                }
            }
            9 => {
                let input = boxed(self)?;
                SlotDisplay::WithRemainder {
                    input,
                    remainder: boxed(self)?,
                }
            }
            10 => {
                let count = self.list_count()?;
                let mut parts = Vec::with_capacity(count.min(64));
                for _ in 0..count {
                    parts.push(self.slot_display_at(depth + 1)?);
                }
                SlotDisplay::Composite(parts)
            }
            other => bail!("unknown slot display type {other}"),
        })
    }

    pub fn recipe_display(&mut self) -> Result<RecipeDisplay> {
        Ok(match self.var_int()? {
            0 => {
                let ingredients = self.slot_displays()?;
                RecipeDisplay::Shapeless {
                    ingredients,
                    result: self.slot_display()?,
                    station: self.slot_display()?,
                }
            }
            1 => {
                let width = self.var_int()?.clamp(0, 64) as usize;
                let height = self.var_int()?.clamp(0, 64) as usize;
                let ingredients = self.slot_displays()?;
                if ingredients.len() != width * height {
                    bail!(
                        "shaped recipe display of {width}x{height} has {} slots",
                        ingredients.len()
                    );
                }
                RecipeDisplay::Shaped {
                    width,
                    height,
                    ingredients,
                    result: self.slot_display()?,
                    station: self.slot_display()?,
                }
            }
            2 => RecipeDisplay::Furnace {
                ingredient: self.slot_display()?,
                fuel: self.slot_display()?,
                result: self.slot_display()?,
                station: self.slot_display()?,
                duration: self.var_int()?,
                experience: self.f32_be()?,
            },
            3 => RecipeDisplay::Stonecutter {
                input: self.slot_display()?,
                result: self.slot_display()?,
                station: self.slot_display()?,
            },
            4 => RecipeDisplay::Smithing {
                template: self.slot_display()?,
                base: self.slot_display()?,
                addition: self.slot_display()?,
                result: self.slot_display()?,
                station: self.slot_display()?,
            },
            other => bail!("unknown recipe display type {other}"),
        })
    }

    fn slot_displays(&mut self) -> Result<Vec<SlotDisplay>> {
        let count = self.list_count()?;
        let mut list = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            list.push(self.slot_display()?);
        }
        Ok(list)
    }

    /// `RecipeDisplayEntry.STREAM_CODEC`.
    pub fn recipe_entry(&mut self) -> Result<Entry> {
        let id = RecipeId(self.var_int()?);
        let display = self.recipe_display()?;
        let group = match self.var_int()? {
            0 => None,
            n => Some(n - 1),
        };
        let category = self.var_int()?;
        let Some(category) = Category::from_wire(category) else {
            bail!("unknown recipe book category {category}");
        };
        let requirements = if self.bool()? {
            let count = self.list_count()?;
            let mut sets = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                sets.push(self.item_set()?);
            }
            Some(sets)
        } else {
            None
        };
        Ok(Entry {
            id,
            display,
            group,
            category,
            requirements,
        })
    }

    /// `ClientboundUpdateRecipesPacket.STREAM_CODEC`.
    pub fn recipe_sets(&mut self) -> Result<RecipeSets> {
        let count = self.list_count()?;
        let mut sets = HashMap::with_capacity(initial_capacity(count));
        for _ in 0..count {
            let name = self.string()?;
            let n = self.list_count()?;
            let mut items = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                items.push(self.item_by_id()?);
            }
            sets.insert(name, PropertySet::new(items));
        }
        let count = self.list_count()?;
        let mut stonecutter = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            stonecutter.push(StonecutterOption {
                input: self.item_set()?,
                display: self.slot_display()?,
            });
        }
        Ok(RecipeSets::new(sets, stonecutter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::varint::write_var_int as var;

    fn bytes(f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut out = Vec::new();
        f(&mut out);
        out
    }

    #[test]
    fn a_shaped_entry_reads_with_tags_and_items() {
        let planks = "minecraft:planks";
        let body = bytes(|b| {
            var(b, 5).unwrap();
            var(b, 1).unwrap();
            var(b, 1).unwrap();
            var(b, 2).unwrap();
            var(b, 2).unwrap();
            var(b, 6).unwrap();
            var(b, 0).unwrap();
            crate::network::protocol::reader::write_string(b, planks).unwrap();
            var(b, 4).unwrap();
            var(b, i32::from(Item::STICK.id)).unwrap();
            var(b, 4).unwrap();
            var(b, i32::from(Item::LADDER.id)).unwrap();
            var(b, 0).unwrap();
            var(b, 3).unwrap();
            var(b, 3).unwrap();
            b.push(1);
            var(b, 1).unwrap();
            var(b, 2).unwrap();
            var(b, i32::from(Item::STICK.id)).unwrap();
        });
        let mut r = Reader::new(&body);
        let entry = r.recipe_entry().unwrap();
        assert!(r.remaining().is_empty());
        assert_eq!(entry.id, RecipeId(5));
        assert_eq!(entry.group, Some(2));
        assert_eq!(entry.category, Category::CraftingMisc);
        let RecipeDisplay::Shaped {
            width,
            height,
            ingredients,
            result,
            ..
        } = &entry.display
        else {
            panic!("not shaped");
        };
        assert_eq!((*width, *height), (1, 2));
        assert!(ingredients[0].shows(&Item::OAK_PLANKS));
        assert!(ingredients[1].shows(&Item::STICK));
        assert!(result.shows(&Item::LADDER));
        assert_eq!(entry.requirements.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn a_template_without_components_reads_as_a_stack() {
        let body = bytes(|b| {
            var(b, 5).unwrap();
            var(b, i32::from(Item::DIAMOND.id)).unwrap();
            var(b, 3).unwrap();
            var(b, 0).unwrap();
            var(b, 0).unwrap();
            var(b, 0).unwrap();
        });
        let mut r = Reader::new(&body);
        let SlotDisplay::Stack(stack) = r.slot_display().unwrap() else {
            panic!("not a stack");
        };
        assert_eq!((stack.0.item.id, stack.0.item_count), (Item::DIAMOND.id, 3));
        assert_eq!(r.slot_display().unwrap(), SlotDisplay::Empty);
    }

    #[test]
    fn an_update_reads_its_sets_and_stonecutter_options() {
        let body = bytes(|b| {
            var(b, 1).unwrap();
            crate::network::protocol::reader::write_string(b, "minecraft:furnace_input").unwrap();
            var(b, 2).unwrap();
            var(b, i32::from(Item::BEEF.id)).unwrap();
            var(b, i32::from(Item::IRON_ORE.id)).unwrap();
            var(b, 1).unwrap();
            var(b, 2).unwrap();
            var(b, i32::from(Item::STONE.id)).unwrap();
            var(b, 4).unwrap();
            var(b, i32::from(Item::STONE_SLAB.id)).unwrap();
        });
        let mut r = Reader::new(&body);
        let sets = r.recipe_sets().unwrap();
        assert!(r.remaining().is_empty());
        assert!(sets.accepts(
            crate::world::recipes::sets::FURNACE_INPUT,
            &ItemStack::new(1, &Item::BEEF)
        ));
        assert_eq!(
            sets.stonecutter_options(&ItemStack::new(1, &Item::STONE))
                .len(),
            1
        );
    }

    #[test]
    fn a_nested_display_that_never_ends_is_refused() {
        let body = bytes(|b| {
            for _ in 0..40 {
                var(b, 10).unwrap();
                var(b, 1).unwrap();
            }
        });
        assert!(Reader::new(&body).slot_display().is_err());
    }
}
