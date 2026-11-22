//! `ClientRecipeBook`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;

use super::display::{ItemSet, RecipeDisplay};
use super::layers::Layer;

/// `RecipeBookType`, in the order the protocol numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BookType {
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
}

impl BookType {
    pub const ALL: [Self; 4] = [
        Self::Crafting,
        Self::Furnace,
        Self::BlastFurnace,
        Self::Smoker,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Whether a book is open and whether it lists only what can be crafted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeSettings {
    pub open: bool,
    pub filtering: bool,
}

/// `RecipeBookSettings`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookSettings([TypeSettings; 4]);

impl BookSettings {
    pub fn new(settings: [TypeSettings; 4]) -> Self {
        Self(settings)
    }

    pub fn get(&self, book: BookType) -> TypeSettings {
        self.0[book.index()]
    }

    pub fn set(&mut self, book: BookType, settings: TypeSettings) {
        self.0[book.index()] = settings;
    }
}

/// `RecipeBookCategories`, in registry order (what the packet numbers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    CraftingBuildingBlocks,
    CraftingRedstone,
    CraftingEquipment,
    CraftingMisc,
    FurnaceFood,
    FurnaceBlocks,
    FurnaceMisc,
    BlastFurnaceBlocks,
    BlastFurnaceMisc,
    SmokerFood,
    Stonecutter,
    Smithing,
    Campfire,
}

impl Category {
    const ALL: [Self; 13] = [
        Self::CraftingBuildingBlocks,
        Self::CraftingRedstone,
        Self::CraftingEquipment,
        Self::CraftingMisc,
        Self::FurnaceFood,
        Self::FurnaceBlocks,
        Self::FurnaceMisc,
        Self::BlastFurnaceBlocks,
        Self::BlastFurnaceMisc,
        Self::SmokerFood,
        Self::Stonecutter,
        Self::Smithing,
        Self::Campfire,
    ];

    pub fn from_wire(id: i32) -> Option<Self> {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }

    /// The book that lists this category, if one does.
    pub fn book(self) -> Option<BookType> {
        Some(match self {
            Self::CraftingBuildingBlocks
            | Self::CraftingRedstone
            | Self::CraftingEquipment
            | Self::CraftingMisc => BookType::Crafting,
            Self::FurnaceFood | Self::FurnaceBlocks | Self::FurnaceMisc => BookType::Furnace,
            Self::BlastFurnaceBlocks | Self::BlastFurnaceMisc => BookType::BlastFurnace,
            Self::SmokerFood => BookType::Smoker,
            Self::Stonecutter | Self::Smithing | Self::Campfire => return None,
        })
    }

    /// The tab's icon sprite, `recipe_book/<name>` in the GUI atlas.
    pub fn name(self) -> &'static str {
        match self {
            Self::CraftingBuildingBlocks => "crafting_building_blocks",
            Self::CraftingRedstone => "crafting_redstone",
            Self::CraftingEquipment => "crafting_equipment",
            Self::CraftingMisc => "crafting_misc",
            Self::FurnaceFood => "furnace_food",
            Self::FurnaceBlocks => "furnace_blocks",
            Self::FurnaceMisc => "furnace_misc",
            Self::BlastFurnaceBlocks => "blast_furnace_blocks",
            Self::BlastFurnaceMisc => "blast_furnace_misc",
            Self::SmokerFood => "smoker_food",
            Self::Stonecutter => "stonecutter",
            Self::Smithing => "smithing",
            Self::Campfire => "campfire",
        }
    }
}

/// A tab of a book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Search(BookType),
    Category(Category),
}

impl Tab {
    /// The tabs a book shows, search first.
    pub fn of(book: BookType) -> Vec<Self> {
        std::iter::once(Self::Search(book))
            .chain(
                Category::ALL
                    .into_iter()
                    .filter(|c| c.book() == Some(book))
                    .map(Self::Category),
            )
            .collect()
    }

    fn includes(self, category: Category) -> bool {
        match self {
            Self::Search(book) => category.book() == Some(book),
            Self::Category(c) => c == category,
        }
    }
}

/// `RecipeDisplayId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecipeId(pub i32);

impl RecipeId {
    pub fn is_local(self) -> bool {
        self.0 < 0
    }
}

/// `RecipeDisplayEntry`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: RecipeId,
    pub display: RecipeDisplay,
    /// Recipes of one group in one category share a button.
    pub group: Option<i32>,
    pub category: Category,
    /// What the recipe takes, one set per slot; `None` when it can't be told.
    pub requirements: Option<Vec<ItemSet>>,
}

/// What the inventory holds, by item, for [`Stock::can_craft`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stock(HashMap<u16, u32>);

impl Stock {
    pub fn from_stacks<'a>(stacks: impl IntoIterator<Item = &'a ItemStack>) -> Self {
        let mut counts = HashMap::new();
        for stack in stacks.into_iter().filter(|s| !s.is_empty()) {
            *counts.entry(stack.item.id).or_insert(0) += u32::from(stack.item_count);
        }
        Self(counts)
    }

    /// Whether every set in `requirements` can be given a distinct item.
    pub fn can_craft(&self, requirements: &[ItemSet]) -> bool {
        let options: Vec<Vec<u16>> = requirements
            .iter()
            .map(|set| {
                set.items()
                    .iter()
                    .map(|i| i.id)
                    .filter(|id| self.0.contains_key(id))
                    .collect()
            })
            .collect();
        if options.iter().any(Vec::is_empty) {
            return false;
        }
        let mut assigned = vec![None; options.len()];
        let mut load: HashMap<u16, usize> = HashMap::new();
        (0..options.len()).all(|req| {
            let mut seen = HashSet::new();
            self.assign(req, &options, &mut assigned, &mut load, &mut seen)
        })
    }

    /// Kuhn's augmenting path.
    fn assign(
        &self,
        req: usize,
        options: &[Vec<u16>],
        assigned: &mut Vec<Option<u16>>,
        load: &mut HashMap<u16, usize>,
        seen: &mut HashSet<u16>,
    ) -> bool {
        for &item in &options[req] {
            if !seen.insert(item) {
                continue;
            }
            if load.get(&item).copied().unwrap_or(0) < self.0[&item] as usize {
                assigned[req] = Some(item);
                *load.entry(item).or_default() += 1;
                return true;
            }
            let holders: Vec<usize> = (0..assigned.len())
                .filter(|&other| assigned[other] == Some(item))
                .collect();
            for other in holders {
                assigned[other] = None;
                *load.entry(item).or_default() -= 1;
                if self.assign(other, options, assigned, load, seen) {
                    assigned[req] = Some(item);
                    *load.entry(item).or_default() += 1;
                    return true;
                }
                assigned[other] = Some(item);
                *load.entry(item).or_default() += 1;
            }
        }
        false
    }
}

/// `RecipeCollection`: the recipes sharing one button of the book.
#[derive(Clone, Debug)]
pub struct Collection {
    pub category: Category,
    pub entries: Vec<RecipeId>,
}

/// The book itself.
#[derive(Clone, Debug)]
pub struct RecipeBook {
    known: BTreeMap<RecipeId, Entry>,
    highlight: HashSet<RecipeId>,
    settings: BookSettings,
    layer: Arc<Layer>,
    /// The layer's recipes that survive, rebuilt with the collections.
    local: Vec<Entry>,
    collections: Vec<Collection>,
    revision: u64,
}

impl Default for RecipeBook {
    fn default() -> Self {
        Self::with_layer(Arc::default())
    }
}

impl RecipeBook {
    pub fn with_layer(layer: Arc<Layer>) -> Self {
        let mut book = Self {
            known: BTreeMap::new(),
            highlight: HashSet::new(),
            settings: BookSettings::default(),
            layer,
            local: Vec::new(),
            collections: Vec::new(),
            revision: 0,
        };
        book.rebuild();
        book
    }

    pub fn add(&mut self, entry: Entry) {
        self.known.insert(entry.id, entry);
    }

    pub fn remove(&mut self, id: RecipeId) {
        self.known.remove(&id);
        self.highlight.remove(&id);
    }

    pub fn clear(&mut self) {
        self.known.clear();
        self.highlight.clear();
    }

    pub fn add_highlight(&mut self, id: RecipeId) {
        self.highlight.insert(id);
    }

    pub fn remove_highlight(&mut self, id: RecipeId) {
        self.revision += 1;
        self.highlight.remove(&id);
    }

    pub fn will_highlight(&self, id: RecipeId) -> bool {
        self.highlight.contains(&id)
    }

    /// Changes whenever the recipes or the settings do, for whoever caches what they show.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn settings(&self) -> &BookSettings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: BookSettings) {
        self.revision += 1;
        self.settings = settings;
    }

    pub fn set_book(&mut self, book: BookType, open: bool, filtering: bool) {
        self.revision += 1;
        self.settings.set(book, TypeSettings { open, filtering });
    }

    pub fn get(&self, id: RecipeId) -> Option<&Entry> {
        self.known
            .get(&id)
            .or_else(|| self.local.iter().find(|e| e.id == id))
    }

    /// Every recipe the book shows: the server's, then the layer's.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.known.values().chain(self.local.iter())
    }

    /// Regroups after a change (`rebuildCollections`).
    pub fn rebuild(&mut self) {
        self.revision += 1;
        let server_results: HashSet<(Category, Vec<u16>)> =
            self.known.values().map(result_key).collect();
        self.local = self
            .layer
            .entries()
            .iter()
            .filter(|e| !self.layer.removes(&e.display))
            .filter(|e| !server_results.contains(&result_key(e)))
            .cloned()
            .collect();
        let hidden: HashSet<RecipeId> = self
            .known
            .values()
            .filter(|e| self.layer.removes(&e.display))
            .map(|e| e.id)
            .collect();
        let mut by_group: HashMap<(Category, i32), usize> = HashMap::new();
        let mut collections: Vec<Collection> = Vec::new();
        for entry in self.known.values().chain(self.local.iter()) {
            if hidden.contains(&entry.id) {
                continue;
            }
            match entry.group {
                Some(group) => match by_group.get(&(entry.category, group)) {
                    Some(&at) => collections[at].entries.push(entry.id),
                    None => {
                        by_group.insert((entry.category, group), collections.len());
                        collections.push(Collection {
                            category: entry.category,
                            entries: vec![entry.id],
                        });
                    }
                },
                None => collections.push(Collection {
                    category: entry.category,
                    entries: vec![entry.id],
                }),
            }
        }
        self.collections = collections;
    }

    /// The collections of a tab.
    pub fn collections(&self, tab: Tab) -> impl Iterator<Item = &Collection> {
        self.collections
            .iter()
            .filter(move |c| tab.includes(c.category))
    }

    pub fn len(&self) -> usize {
        self.collections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.collections.is_empty()
    }

    /// Whether any recipe of the collection is highlighted (new).
    pub fn highlights(&self, collection: &Collection) -> bool {
        collection.entries.iter().any(|id| self.will_highlight(*id))
    }

    /// The recipes of `collection` the stock can craft (empty requirements:
    pub fn craftable<'a>(&'a self, collection: &'a Collection, stock: &Stock) -> Vec<&'a Entry> {
        collection
            .entries
            .iter()
            .filter_map(|id| self.get(*id))
            .filter(|e| e.requirements.as_ref().is_some_and(|r| stock.can_craft(r)))
            .collect()
    }
}

/// What a recipe makes, to tell a layer's recipe from the server's.
fn result_key(entry: &Entry) -> (Category, Vec<u16>) {
    let mut ids: Vec<u16> = entry
        .display
        .result()
        .stacks()
        .iter()
        .map(|s| s.item.id)
        .collect();
    ids.sort_unstable();
    (entry.category, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::recipes::display::SlotDisplay;
    use pumpkin_data::item::Item;

    fn entry(id: i32, category: Category, group: Option<i32>, result: &'static Item) -> Entry {
        Entry {
            id: RecipeId(id),
            display: RecipeDisplay::Stonecutter {
                input: SlotDisplay::Empty,
                result: SlotDisplay::Item(result),
                station: SlotDisplay::Empty,
            },
            group,
            category,
            requirements: Some(vec![ItemSet::Items(vec![&Item::OAK_PLANKS])]),
        }
    }

    #[test]
    fn recipes_of_one_group_share_a_collection_and_tabs_filter() {
        let mut book = RecipeBook::default();
        book.add(entry(0, Category::CraftingMisc, Some(1), &Item::OAK_BUTTON));
        book.add(entry(
            1,
            Category::CraftingMisc,
            Some(1),
            &Item::BIRCH_BUTTON,
        ));
        book.add(entry(2, Category::CraftingMisc, None, &Item::STICK));
        book.add(entry(3, Category::FurnaceFood, None, &Item::COOKED_BEEF));
        book.rebuild();
        assert_eq!(book.collections(Tab::Search(BookType::Crafting)).count(), 2);
        assert_eq!(book.collections(Tab::Search(BookType::Furnace)).count(), 1);
        let grouped = book
            .collections(Tab::Category(Category::CraftingMisc))
            .next()
            .unwrap();
        assert_eq!(grouped.entries, [RecipeId(0), RecipeId(1)]);
        book.remove(RecipeId(0));
        book.rebuild();
        let grouped = book
            .collections(Tab::Category(Category::CraftingMisc))
            .next()
            .unwrap();
        assert_eq!(grouped.entries, [RecipeId(1)]);
    }

    #[test]
    fn a_stack_counts_once_per_item_it_holds() {
        let planks = ItemSet::Items(vec![&Item::OAK_PLANKS, &Item::BIRCH_PLANKS]);
        let stick = ItemSet::Items(vec![&Item::STICK]);
        let have = |stacks: &[ItemStack]| Stock::from_stacks(stacks);
        let oak = |n| ItemStack::new(n, &Item::OAK_PLANKS);
        assert!(!have(&[oak(1)]).can_craft(&[planks.clone(), planks.clone()]));
        assert!(have(&[oak(2)]).can_craft(&[planks.clone(), planks.clone()]));
        let birch = ItemStack::new(1, &Item::BIRCH_PLANKS);
        assert!(have(&[oak(1), birch]).can_craft(&[planks.clone(), planks.clone()]));
        assert!(!have(&[oak(2)]).can_craft(&[planks, stick]));
    }

    #[test]
    fn a_requirement_moves_off_an_item_another_needs() {
        let either = ItemSet::Items(vec![&Item::OAK_PLANKS, &Item::BIRCH_PLANKS]);
        let oak = ItemSet::Items(vec![&Item::OAK_PLANKS]);
        let stock = Stock::from_stacks(&[
            ItemStack::new(1, &Item::OAK_PLANKS),
            ItemStack::new(1, &Item::BIRCH_PLANKS),
        ]);
        assert!(stock.can_craft(&[either, oak]));
    }

    #[test]
    fn the_wire_numbers_name_the_categories() {
        assert_eq!(
            Category::from_wire(0),
            Some(Category::CraftingBuildingBlocks)
        );
        assert_eq!(Category::from_wire(12), Some(Category::Campfire));
        assert_eq!(Category::from_wire(13), None);
        assert_eq!(Category::FurnaceFood.book(), Some(BookType::Furnace));
        assert_eq!(Tab::of(BookType::Smoker).len(), 2);
    }
}
