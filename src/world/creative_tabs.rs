//! Vanilla `CreativeModeTabs`.

use std::borrow::Cow;
use std::path::Path;
use std::sync::atomic::{AtomicPtr, AtomicU16, Ordering};
use std::sync::{LazyLock, Mutex};

use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{
    BlockStateImpl, DataComponentImpl, FireworksImpl, InstrumentImpl, ItemModelImpl, ItemNameImpl,
    MaxStackSizeImpl, OminousBottleAmplifierImpl, PaintingVariantImpl, PotionContentsImpl,
    StoredEnchantmentsImpl,
};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use serde::Deserialize;

/// `CreativeModeTab.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabType {
    #[default]
    Category,
    Hotbar,
    Search,
    Inventory,
}

/// `CreativeModeTab.Row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Row {
    Top,
    Bottom,
}

#[derive(Deserialize)]
struct TabFile {
    tabs: Vec<TabEntry>,
}

#[derive(Deserialize)]
struct TabEntry {
    id: String,
    row: Row,
    column: u8,
    title: String,
    icon: String,
    #[serde(rename = "type")]
    kind: TabType,
    aligned_right: bool,
    show_title: bool,
    scrollbar: bool,
    background: String,
    #[serde(default)]
    operator_only: bool,
    items: Vec<ItemEntry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ItemEntry {
    Plain(String),
    Detailed {
        #[serde(default)]
        item: Option<String>,
        #[serde(default)]
        generator: Option<String>,
        #[serde(default)]
        search_only: bool,
        #[serde(default)]
        parent_only: bool,
    },
}

/// One tab, its contents built.
#[derive(Clone)]
pub struct CreativeTab {
    pub id: String,
    pub row: Row,
    pub column: u8,
    /// `itemGroup.*` translation key.
    pub title: String,
    pub icon: ItemStack,
    pub kind: TabType,
    pub aligned_right: bool,
    pub show_title: bool,
    pub scrollbar: bool,
    /// `textures/gui/container/creative_inventory/<background>.png`.
    pub background: String,
    /// The operator utilities tab, hidden unless enabled and permitted.
    pub operator_only: bool,
    /// `getDisplayItems`: what the tab itself shows.
    pub items: Vec<ItemStack>,
    /// `getSearchTabDisplayItems`: what it adds to the search tab.
    pub search_items: Vec<ItemStack>,
}

impl CreativeTab {
    /// `CreativeModeTab.contains`: for the tooltip's list of tabs.
    pub fn contains(&self, stack: &ItemStack) -> bool {
        self.items.iter().any(|s| s.item.id == stack.item.id)
    }
}

/// Vanilla tabs only. Prefer [`tabs`] once zip-mods may have appended theirs.
pub static TABS: LazyLock<Vec<CreativeTab>> = LazyLock::new(load);

static TABS_PTR: AtomicPtr<Vec<CreativeTab>> = AtomicPtr::new(std::ptr::null_mut());
static MOD_ITEMS: Mutex<Vec<&'static Item>> = Mutex::new(Vec::new());
static NEXT_MOD_ITEM_ID: AtomicU16 = AtomicU16::new(50_000);

/// Vanilla tabs, then tabs from enabled zip-mods (`data/*/creative_tab.json`).
pub fn tabs() -> &'static [CreativeTab] {
    let ptr = TABS_PTR.load(Ordering::Acquire);
    if ptr.is_null() {
        TABS.as_slice()
    } else {
        unsafe { (*ptr).as_slice() }
    }
}

/// Reads `data/<ns>/creative_tab.json` from extracted zip-mods and appends their tabs.
pub fn apply_extracted_mods(extracted: impl IntoIterator<Item = impl AsRef<Path>>) {
    let extra: Vec<CreativeTab> = extracted
        .into_iter()
        .flat_map(|dir| collect_mod_tabs(dir.as_ref()))
        .collect();
    if extra.is_empty() {
        TABS_PTR.store(std::ptr::null_mut(), Ordering::Release);
        return;
    }
    let mut tabs = load();
    let existing: std::collections::HashSet<String> = tabs.iter().map(|t| t.id.clone()).collect();
    tabs.extend(extra.into_iter().filter(|tab| !existing.contains(&tab.id)));
    fill_search(&mut tabs);
    publish(tabs);
}

/// Scan instance `mods/.cache` and apply enabled mods' creative tabs.
pub fn apply_from_game_dir(game_dir: &Path) {
    let extracted = pumpkin_mod::extracted_mods(&game_dir.join("mods").join(".cache"));
    pumpkin_mod::register_data(&extracted);
    super::recipes::layers::install(super::recipes::layers::load_extracted(&extracted));
    apply_extracted_mods(extracted);
}

fn publish(tabs: Vec<CreativeTab>) {
    let boxed = Box::into_raw(Box::new(tabs));
    TABS_PTR.store(boxed, Ordering::Release);
}

fn load() -> Vec<CreativeTab> {
    let file: TabFile =
        serde_json::from_str(include_str!("creative_tabs.json")).expect("creative_tabs.json");
    let mut tabs: Vec<CreativeTab> = file.tabs.into_iter().map(tab_from_entry).collect();
    fill_search(&mut tabs);
    tabs
}

fn tab_from_entry(entry: TabEntry) -> CreativeTab {
    let mut items = Vec::new();
    let mut search_items = Vec::new();
    for item in entry.items {
        let (stacks, search_only, parent_only) = match item {
            ItemEntry::Plain(name) => (stack(&name).into_iter().collect(), false, false),
            ItemEntry::Detailed {
                item,
                generator,
                search_only,
                parent_only,
            } => {
                let stacks = match generator.as_deref() {
                    Some(generator) => generate(generator, item.as_deref()),
                    None => item.as_deref().and_then(stack).into_iter().collect(),
                };
                (stacks, search_only, parent_only)
            }
        };
        if !search_only {
            items.extend(stacks.iter().cloned());
        }
        if !parent_only {
            search_items.extend(stacks);
        }
    }
    CreativeTab {
        icon: stack(&entry.icon).unwrap_or_else(crate::world::inventory::empty),
        id: entry.id,
        row: entry.row,
        column: entry.column,
        title: entry.title,
        kind: entry.kind,
        aligned_right: entry.aligned_right,
        show_title: entry.show_title,
        scrollbar: entry.scrollbar,
        background: entry.background,
        operator_only: entry.operator_only,
        items,
        search_items,
    }
}

fn fill_search(tabs: &mut [CreativeTab]) {
    let mut all: Vec<ItemStack> = Vec::new();
    for tab in tabs.iter().filter(|t| t.kind != TabType::Search) {
        for stack in &tab.search_items {
            if !all.iter().any(|s| s.are_items_and_components_equal(stack)) {
                all.push(stack.clone());
            }
        }
    }
    if let Some(search) = tabs.iter_mut().find(|t| t.kind == TabType::Search) {
        search.items = all;
    }
}

#[derive(Deserialize)]
struct ModTabFile {
    id: String,
    title: String,
    icon: String,
    row: Row,
    column: u8,
    #[serde(default)]
    aligned_right: bool,
    #[serde(default = "default_true")]
    show_title: bool,
    #[serde(default = "default_true")]
    scrollbar: bool,
    #[serde(default = "default_background")]
    background: String,
    #[serde(rename = "type", default)]
    kind: TabType,
    #[serde(default)]
    operator_only: bool,
    #[serde(default)]
    items: Vec<ItemEntry>,
}

const fn default_true() -> bool {
    true
}

fn default_background() -> String {
    "tab_items".to_owned()
}

fn collect_mod_tabs(extracted: &Path) -> Vec<CreativeTab> {
    let data = extracted.join("data");
    let Ok(namespaces) = std::fs::read_dir(&data) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = namespaces
        .filter_map(Result::ok)
        .map(|e| e.path().join("creative_tab.json"))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            serde_json::from_str::<ModTabFile>(&text)
                .ok()
                .map(tab_from_mod)
        })
        .collect()
}

fn tab_from_mod(entry: ModTabFile) -> CreativeTab {
    tab_from_entry(TabEntry {
        id: entry.id,
        row: entry.row,
        column: entry.column,
        title: entry.title,
        icon: entry.icon,
        kind: entry.kind,
        aligned_right: entry.aligned_right,
        show_title: entry.show_title,
        scrollbar: entry.scrollbar,
        background: entry.background,
        operator_only: entry.operator_only,
        items: entry.items,
    })
}

fn stack(name: &str) -> Option<ItemStack> {
    Some(ItemStack::new(1, intern_item(name)?))
}

/// Vanilla [`Item::from_registry_key`], else a leaked runtime item for a mod's name.
pub(crate) fn intern_item(name: &str) -> Option<&'static Item> {
    if name.is_empty() {
        return None;
    }
    if let Some(item) = Item::from_registry_key(name) {
        return Some(item);
    }
    let mut guard = MOD_ITEMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(item) = guard.iter().copied().find(|item| item.registry_key == name) {
        return Some(item);
    }
    let id = NEXT_MOD_ITEM_ID.fetch_add(1, Ordering::Relaxed);
    let registry_key: &'static str = Box::leak(name.to_owned().into_boxed_str());
    let (namespace, path) = name.split_once(':').unwrap_or(("minecraft", name));
    let path = path.replace('/', ".");
    let item_name: &'static str = Box::leak(format!("item.{namespace}.{path}").into_boxed_str());
    let max = Box::leak(Box::new(MaxStackSizeImpl { size: 64 }));
    let iname = Box::leak(Box::new(ItemNameImpl {
        name: Cow::Borrowed(item_name),
    }));
    let imodel = Box::leak(Box::new(ItemModelImpl {
        id: Cow::Borrowed(registry_key),
    }));
    let components: &'static [(DataComponent, &'static dyn DataComponentImpl)] =
        Box::leak(Box::new([
            (DataComponent::MaxStackSize, max as _),
            (DataComponent::ItemName, iname as _),
            (DataComponent::ItemModel, imodel as _),
        ]));
    let item = Box::leak(Box::new(Item {
        id,
        registry_key,
        components,
        crafting_remainder: None,
    }));
    guard.push(item);
    Some(item)
}

/// The stacks of one of `CreativeModeTabs`' generator functions.
fn generate(generator: &str, item: Option<&str>) -> Vec<ItemStack> {
    let with = |name: &str, f: &dyn Fn(&mut ItemStack)| {
        stack(name).map(|mut s| {
            f(&mut s);
            s
        })
    };
    match generator {
        "potions" => {
            let Some(item) = item else { return Vec::new() };
            (0..=u8::MAX)
                .filter_map(pumpkin_data::potion::Potion::from_id)
                .filter_map(|potion| {
                    with(item, &|s| {
                        s.set_data_component(PotionContentsImpl {
                            potion_id: Some(i32::from(potion.id)),
                            custom_color: None,
                            custom_effects: Vec::new(),
                            custom_name: None,
                        });
                    })
                })
                .collect()
        }
        "enchanted_books_max" | "enchanted_books_all" => {
            let mut enchantments: Vec<_> = pumpkin_data::Enchantment::all().copied().collect();
            enchantments.sort_by_key(|e| e.registry_key);
            let all_levels = generator == "enchanted_books_all";
            enchantments
                .into_iter()
                .flat_map(|enchantment| {
                    let levels = if all_levels {
                        1..=enchantment.max_level
                    } else {
                        enchantment.max_level..=enchantment.max_level
                    };
                    levels.filter_map(move |level| {
                        stack("enchanted_book").map(|mut s| {
                            s.set_data_component(StoredEnchantmentsImpl {
                                enchantment: Cow::Owned(vec![(enchantment, level)]),
                            });
                            s
                        })
                    })
                })
                .collect()
        }
        "goat_horns" => [
            "ponder_goat_horn",
            "sing_goat_horn",
            "seek_goat_horn",
            "feel_goat_horn",
            "admire_goat_horn",
            "call_goat_horn",
            "yearn_goat_horn",
            "dream_goat_horn",
        ]
        .iter()
        .filter_map(|instrument| {
            with("goat_horn", &|s| {
                s.set_data_component(InstrumentImpl {
                    instrument: Cow::Owned(format!("minecraft:{instrument}")),
                });
            })
        })
        .collect(),
        "fireworks" => (1..=3)
            .filter_map(|duration| {
                with("firework_rocket", &|s| {
                    s.set_data_component(FireworksImpl::new(duration, Vec::new()));
                })
            })
            .collect(),
        "ominous_bottles" => (0..=4)
            .filter_map(|amplifier| {
                with("ominous_bottle", &|s| {
                    s.set_data_component(OminousBottleAmplifierImpl { amplifier });
                })
            })
            .collect(),
        "paintings" => {
            let mut paintings: Vec<_> =
                pumpkin_data::painting_variant::PaintingVariant::all_placeable().to_vec();
            paintings.sort_by_key(|p| (p.width() * p.height(), p.width()));
            paintings
                .into_iter()
                .filter_map(|painting| {
                    with("painting", &|s| {
                        s.set_data_component(PaintingVariantImpl {
                            value: Cow::Owned(format!("minecraft:{}", painting.to_name())),
                        });
                    })
                })
                .collect()
        }
        "light_blocks" => (0..=15)
            .filter_map(|level: u8| {
                with("light", &|s| {
                    s.set_data_component(BlockStateImpl {
                        properties: Cow::Owned(vec![(
                            Cow::Borrowed("level"),
                            Cow::Owned(level.to_string()),
                        )]),
                    });
                })
            })
            .collect(),
        "test_blocks" => ["start", "log", "fail", "accept"]
            .iter()
            .filter_map(|mode| {
                with("test_block", &|s| {
                    s.set_data_component(BlockStateImpl {
                        properties: Cow::Owned(vec![(Cow::Borrowed("mode"), Cow::Borrowed(*mode))]),
                    });
                })
            })
            .collect(),
        "suspicious_stews" => stack("suspicious_stew").into_iter().collect(),
        "ominous_banner" => Vec::new(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_load_with_their_items() {
        let tabs = &*TABS;
        assert_eq!(tabs.len(), 14);
        assert!(tabs.iter().all(|t| t.id != "industrial"));
        let building = &tabs[0];
        assert_eq!(building.title, "itemGroup.buildingBlocks");
        assert_eq!(building.items[0].item.registry_key, "oak_log");
        let food = tabs.iter().find(|t| t.id == "food_and_drinks").unwrap();
        assert!(
            food.items
                .iter()
                .filter(|s| s.item.registry_key == "potion")
                .count()
                > 30
        );
        let search = tabs.iter().find(|t| t.kind == TabType::Search).unwrap();
        assert!(search.items.len() > 1500);
        let ingredients = tabs.iter().find(|t| t.id == "ingredients").unwrap();
        let books = |items: &[ItemStack]| {
            items
                .iter()
                .filter(|s| s.item.registry_key == "enchanted_book")
                .count()
        };
        assert!(books(&ingredients.search_items) > books(&ingredients.items));
    }

    /// A zip-mod's `data/<ns>/creative_tab.json` orders a tab, and Melon
    /// makes it from what the file says. What the mod's blocks are like is
    /// the mod's business (`pumpkin-melon-mod-example` tests its own).
    #[test]
    fn a_mods_creative_tab_json_makes_a_tab() {
        let dir = std::env::temp_dir().join(format!(
            "melon-mod-tab-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ns = dir.join("data/example");
        std::fs::create_dir_all(ns.join("blocks")).unwrap();
        std::fs::write(
            ns.join("blocks/thing.json"),
            r#"{"id":"example:thing","name":"Thing","hardness":1.0,"blast_resistance":1.0}"#,
        )
        .unwrap();
        pumpkin_data::mod_registry::load_extracted_dir(&dir);
        std::fs::write(
            ns.join("creative_tab.json"),
            r#"{
              "id": "example_things",
              "title": "itemGroup.example_things",
              "icon": "example:thing",
              "row": "bottom",
              "column": 5,
              "aligned_right": true,
              "items": ["example:thing", "minecraft:stone"]
            }"#,
        )
        .unwrap();
        let tabs = collect_mod_tabs(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(tabs.len(), 1);
        let tab = &tabs[0];
        assert_eq!(tab.id, "example_things");
        assert_eq!(tab.title, "itemGroup.example_things");
        assert_eq!(
            (tab.row, tab.column, tab.aligned_right),
            (Row::Bottom, 5, true)
        );
        assert_eq!(tab.kind, TabType::Category);
        assert!(tab.show_title && tab.scrollbar);
        assert_eq!(tab.icon.item.registry_key, "example:thing");
        let items: Vec<_> = tab.items.iter().map(|s| s.item.registry_key).collect();
        assert_eq!(items, ["example:thing", "stone"]);
    }

    #[test]
    fn a_mod_without_creative_tab_json_adds_no_tab() {
        let dir = std::env::temp_dir().join(format!("melon-mod-notab-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("data/example")).unwrap();
        let tabs = collect_mod_tabs(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(tabs.is_empty());
    }
}
