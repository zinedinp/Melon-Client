//! Recipes that don't come from the server.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use serde_json::Value;

use super::book::{Category, Entry, RecipeId};
use super::display::{ItemSet, RecipeDisplay, Shown, SlotDisplay};

/// The recipes added and removed by mods.
#[derive(Debug, Default)]
pub struct Layer {
    entries: Vec<Entry>,
    removed: HashSet<u16>,
}

static CURRENT: Mutex<Option<Arc<Layer>>> = Mutex::new(None);

/// The layer new recipe books start with.
pub fn current() -> Arc<Layer> {
    CURRENT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

/// Replaces the layer (the mods changed).
pub fn install(layer: Layer) {
    *CURRENT.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(layer));
}

/// Reads the enabled mods' recipes, as [`install`] wants them.
pub fn load_extracted(extracted: impl IntoIterator<Item = impl AsRef<Path>>) -> Layer {
    let mut layer = Layer::default();
    for dir in extracted {
        layer.read_dir(dir.as_ref());
    }
    layer
}

impl Layer {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether the layer hides a recipe.
    pub fn removes(&self, display: &RecipeDisplay) -> bool {
        if self.removed.is_empty() {
            return false;
        }
        let results = display.result().stacks();
        !results.is_empty() && results.iter().all(|s| self.removed.contains(&s.item.id))
    }

    pub fn remove_result(&mut self, item: &'static Item) {
        self.removed.insert(item.id);
    }

    /// Adds a recipe by its display; `requirements` are what it takes.
    pub fn add(
        &mut self,
        display: RecipeDisplay,
        category: Category,
        group: Option<i32>,
        requirements: Option<Vec<ItemSet>>,
    ) {
        let id = RecipeId(-1 - self.entries.len() as i32);
        self.entries.push(Entry {
            id,
            display,
            group,
            category,
            requirements,
        });
    }

    fn read_dir(&mut self, extracted: &Path) {
        let Ok(namespaces) = std::fs::read_dir(extracted.join("data")) else {
            return;
        };
        let mut dirs: Vec<_> = namespaces
            .filter_map(Result::ok)
            .flat_map(|ns| [ns.path().join("recipe"), ns.path().join("recipes")])
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let mut files: Vec<_> = walk(&dir);
            files.sort();
            for file in files {
                match std::fs::read_to_string(&file)
                    .map_err(|e| e.to_string())
                    .and_then(|text| {
                        serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())
                    }) {
                    Ok(json) => {
                        self.read(&json);
                    }
                    Err(e) => crate::warn!("Recipes: {}: {e}", file.display()),
                }
            }
        }
    }

    /// One recipe file; whether it was understood.
    pub fn read(&mut self, json: &Value) -> bool {
        let kind = json["type"].as_str().unwrap_or("");
        let kind = kind.strip_prefix("minecraft:").unwrap_or(kind);
        let group = json["group"]
            .as_str()
            .filter(|g| !g.is_empty())
            .map(group_number);
        let category = json["category"].as_str().unwrap_or("");
        match kind {
            "melon:remove" => json["result"]
                .as_str()
                .and_then(item)
                .map(|item| self.remove_result(item))
                .is_some(),
            "crafting_shapeless" => {
                let Some(list) = json["ingredients"].as_array() else {
                    return false;
                };
                let sets: Option<Vec<ItemSet>> = list.iter().map(ingredient).collect();
                let (Some(sets), Some(result)) = (sets, result(&json["result"])) else {
                    return false;
                };
                let display = RecipeDisplay::Shapeless {
                    ingredients: sets.iter().cloned().map(SlotDisplay::Set).collect(),
                    result,
                    station: SlotDisplay::Item(&Item::CRAFTING_TABLE),
                };
                self.add(display, crafting_category(category), group, Some(sets));
                true
            }
            "crafting_shaped" => self.read_shaped(json, crafting_category(category), group),
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                let (Some(set), Some(result)) =
                    (ingredient(&json["ingredient"]), result(&json["result"]))
                else {
                    return false;
                };
                let (station, default_time, category) = match kind {
                    "smelting" => (
                        &Item::FURNACE,
                        200,
                        if category == "food" {
                            Category::FurnaceFood
                        } else if category == "blocks" {
                            Category::FurnaceBlocks
                        } else {
                            Category::FurnaceMisc
                        },
                    ),
                    "blasting" => (
                        &Item::BLAST_FURNACE,
                        100,
                        if category == "blocks" {
                            Category::BlastFurnaceBlocks
                        } else {
                            Category::BlastFurnaceMisc
                        },
                    ),
                    "smoking" => (&Item::SMOKER, 100, Category::SmokerFood),
                    _ => (&Item::CAMPFIRE, 600, Category::Campfire),
                };
                let display = RecipeDisplay::Furnace {
                    ingredient: SlotDisplay::Set(set.clone()),
                    fuel: SlotDisplay::AnyFuel,
                    result,
                    station: SlotDisplay::Item(station),
                    duration: json["cookingtime"].as_i64().unwrap_or(default_time) as i32,
                    experience: json["experience"].as_f64().unwrap_or(0.0) as f32,
                };
                self.add(display, category, group, Some(vec![set]));
                true
            }
            "stonecutting" => {
                let (Some(set), Some(result)) =
                    (ingredient(&json["ingredient"]), result(&json["result"]))
                else {
                    return false;
                };
                let display = RecipeDisplay::Stonecutter {
                    input: SlotDisplay::Set(set.clone()),
                    result,
                    station: SlotDisplay::Item(&Item::STONECUTTER),
                };
                self.add(display, Category::Stonecutter, group, Some(vec![set]));
                true
            }
            "smithing_transform" => {
                let parts = ["template", "base", "addition"].map(|k| ingredient(&json[k]));
                let (Some(result), [Some(template), Some(base), Some(addition)]) =
                    (result(&json["result"]), parts)
                else {
                    return false;
                };
                let display = RecipeDisplay::Smithing {
                    template: SlotDisplay::Set(template),
                    base: SlotDisplay::Set(base),
                    addition: SlotDisplay::Set(addition),
                    result,
                    station: SlotDisplay::Item(&Item::SMITHING_TABLE),
                };
                self.add(display, Category::Smithing, group, None);
                true
            }
            _ => false,
        }
    }

    fn read_shaped(&mut self, json: &Value, category: Category, group: Option<i32>) -> bool {
        let rows: Option<Vec<&str>> = json["pattern"]
            .as_array()
            .map(|rows| rows.iter().filter_map(Value::as_str).collect());
        let (Some(rows), Some(key), Some(result)) =
            (rows, json["key"].as_object(), result(&json["result"]))
        else {
            return false;
        };
        let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        if rows.is_empty() || width == 0 || rows.len() > 3 || width > 3 {
            return false;
        }
        let mut slots = Vec::new();
        let mut needs = Vec::new();
        for row in &rows {
            let mut cells: Vec<char> = row.chars().collect();
            cells.resize(width, ' ');
            for c in cells {
                if c == ' ' {
                    slots.push(SlotDisplay::Empty);
                    continue;
                }
                let Some(set) = key.get(&c.to_string()).and_then(ingredient) else {
                    return false;
                };
                needs.push(set.clone());
                slots.push(SlotDisplay::Set(set));
            }
        }
        let display = RecipeDisplay::Shaped {
            width,
            height: rows.len(),
            ingredients: slots,
            result,
            station: SlotDisplay::Item(&Item::CRAFTING_TABLE),
        };
        self.add(display, category, group, Some(needs));
        true
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "json") {
            files.push(path);
        }
    }
    files
}

/// Recipe groups are names in the data; the packets number them.
fn group_number(name: &str) -> i32 {
    let mut hash: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        hash = (hash ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    (hash >> 1) as i32
}

fn crafting_category(name: &str) -> Category {
    match name {
        "building" => Category::CraftingBuildingBlocks,
        "redstone" => Category::CraftingRedstone,
        "equipment" => Category::CraftingEquipment,
        _ => Category::CraftingMisc,
    }
}

fn item(name: &str) -> Option<&'static Item> {
    super::super::creative_tabs::intern_item(name.strip_prefix("minecraft:").unwrap_or(name))
}

/// `"ns:item"`, `"#ns:tag"`, or a list of those.
fn ingredient(json: &Value) -> Option<ItemSet> {
    match json {
        Value::String(name) => match name.strip_prefix('#') {
            Some(tag) => Some(ItemSet::Tag(tag.to_owned())),
            None => Some(ItemSet::Items(vec![item(name)?])),
        },
        Value::Array(list) => {
            let mut items = Vec::new();
            for entry in list {
                items.extend(ingredient(entry)?.items());
            }
            Some(ItemSet::Items(items))
        }
        _ => None,
    }
}

/// `{"id": "ns:item", "count": 2}` or just `"ns:item"`.
fn result(json: &Value) -> Option<SlotDisplay> {
    let (name, count) = match json {
        Value::String(name) => (name.as_str(), 1),
        other => (other["id"].as_str()?, other["count"].as_u64().unwrap_or(1)),
    };
    Some(SlotDisplay::Stack(Shown(ItemStack::new(
        count.clamp(1, 99) as u8,
        item(name)?,
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::recipes::book::RecipeBook;
    use serde_json::json;

    #[test]
    fn a_shaped_recipe_is_read_with_its_shape_and_needs() {
        let mut layer = Layer::default();
        assert!(layer.read(&json!({
            "type": "minecraft:crafting_shaped",
            "category": "misc",
            "pattern": ["#", "#"],
            "key": {"#": "minecraft:stick"},
            "result": {"id": "minecraft:ladder", "count": 3}
        })));
        let entry = &layer.entries()[0];
        assert!(entry.id.is_local());
        let RecipeDisplay::Shaped { width, height, .. } = &entry.display else {
            panic!("not shaped");
        };
        assert_eq!((*width, *height), (1, 2));
        assert_eq!(entry.requirements.as_ref().unwrap().len(), 2);
        assert_eq!(entry.display.result().first().unwrap().item_count, 3);
    }

    #[test]
    fn cooking_and_stonecutting_get_their_categories() {
        let mut layer = Layer::default();
        assert!(layer.read(&json!({
            "type": "minecraft:smelting", "category": "food",
            "ingredient": "minecraft:beef", "result": {"id": "minecraft:cooked_beef"},
            "experience": 0.35, "cookingtime": 200
        })));
        assert!(layer.read(&json!({
            "type": "minecraft:stonecutting",
            "ingredient": "minecraft:stone", "result": {"id": "minecraft:stone_slab", "count": 2}
        })));
        assert!(!layer.read(&json!({"type": "minecraft:crafting_special_firework_star"})));
        assert_eq!(layer.entries()[0].category, Category::FurnaceFood);
        assert_eq!(layer.entries()[1].category, Category::Stonecutter);
        assert_eq!(layer.entries()[1].id, RecipeId(-2));
    }

    #[test]
    fn a_removal_hides_the_servers_recipe_and_the_layers_own() {
        let mut layer = Layer::default();
        layer.read(&json!({
            "type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:oak_planks"],
            "result": {"id": "minecraft:stick"}
        }));
        layer.read(&json!({
            "type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:oak_planks"],
            "result": {"id": "minecraft:ladder"}
        }));
        assert!(layer.read(&json!({"type": "melon:remove", "result": "minecraft:stick"})));
        let mut book = RecipeBook::with_layer(Arc::new(layer));
        assert_eq!(book.entries().count(), 1, "only the ladder is left");
        book.add(Entry {
            id: RecipeId(7),
            display: RecipeDisplay::Shapeless {
                ingredients: vec![],
                result: SlotDisplay::Item(&Item::STICK),
                station: SlotDisplay::Empty,
            },
            group: None,
            category: Category::CraftingMisc,
            requirements: None,
        });
        book.rebuild();
        let tab =
            crate::world::recipes::book::Tab::Search(crate::world::recipes::BookType::Crafting);
        assert_eq!(
            book.collections(tab).count(),
            1,
            "the server's stick is hidden"
        );
    }

    #[test]
    fn a_server_recipe_replaces_a_layers_with_the_same_result() {
        let mut layer = Layer::default();
        layer.read(&json!({
            "type": "minecraft:crafting_shapeless", "category": "misc",
            "ingredients": ["minecraft:oak_planks"], "result": {"id": "minecraft:stick"}
        }));
        let mut book = RecipeBook::with_layer(Arc::new(layer));
        assert_eq!(book.entries().count(), 1);
        book.add(Entry {
            id: RecipeId(0),
            display: RecipeDisplay::Shapeless {
                ingredients: vec![],
                result: SlotDisplay::Item(&Item::STICK),
                station: SlotDisplay::Empty,
            },
            group: None,
            category: Category::CraftingMisc,
            requirements: None,
        });
        book.rebuild();
        assert_eq!(book.entries().count(), 1);
        assert!(!book.entries().next().unwrap().id.is_local());
    }
}
