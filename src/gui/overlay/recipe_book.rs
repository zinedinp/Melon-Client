//! The recipe book beside a crafting grid or a furnace.

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use crate::assets::Assets;
use crate::gui::hud::item_slot;
use crate::gui::overlay::container_input::MouseButtonKind;
use crate::gui::overlay::menu_layout::MenuLayout;
use crate::gui::overlay::{OverlayAction, OverlayDraw};
use crate::gui::screen::TextEvent;
use crate::gui::sprite::GuiSprite;
use crate::gui::text::StyledText;
use crate::gui::widgets::text_input::TextInput;
use crate::math::argb;
use crate::renderer::{Renderer, TextureId};
use crate::world::recipes::{BookType, Category, RecipeBook, RecipeDisplay, RecipeId, Stock, Tab};

const WIDTH: f32 = 147.0;
const HEIGHT: f32 = 166.0;
/// `RecipeBookComponent.OFFSET_X_POSITION`.
const OFFSET_X: f32 = 86.0;
/// Recipe buttons on a page, and their pitch.
const PER_PAGE: usize = 20;
const BUTTON: f32 = 25.0;
const TAB_W: f32 = 35.0;
const TAB_H: f32 = 27.0;
/// Ticks one recipe of a button is shown before the next (`TICKS_TO_SWAP_SLOT`).
const SWAP_TICKS: f32 = 30.0;

/// TODO(book): vanilla's squeeze animation of tabs and buttons with new recipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeAction {
    /// `ServerboundPlaceRecipePacket`.
    Place {
        container: i32,
        recipe: i32,
        max: bool,
    },
    /// `ServerboundRecipeBookChangeSettingsPacket`.
    Settings {
        book: i32,
        open: bool,
        filtering: bool,
    },
    /// `ServerboundRecipeBookSeenRecipePacket`.
    Seen(i32),
}

/// A button of the book went down: its sound first.
fn click(mut actions: Vec<OverlayAction>) -> Vec<OverlayAction> {
    actions.insert(0, OverlayAction::Sound(super::CLICK));
    actions
}

/// Which menu the book belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A crafting grid `grid` wide and high.
    Crafting {
        grid: usize,
    },
    Furnace(BookType),
}

impl Mode {
    pub fn for_kind(kind: crate::world::inventory::ContainerKind) -> Option<Self> {
        use crate::world::inventory::ContainerKind as K;
        Some(match kind {
            K::Crafting => Self::Crafting { grid: 3 },
            K::Furnace => Self::Furnace(BookType::Furnace),
            K::BlastFurnace => Self::Furnace(BookType::BlastFurnace),
            K::Smoker => Self::Furnace(BookType::Smoker),
            _ => return None,
        })
    }

    fn book(self) -> BookType {
        match self {
            Self::Crafting { .. } => BookType::Crafting,
            Self::Furnace(book) => book,
        }
    }

    /// The menu slots the ghost recipe fills: result and the grid's first.
    fn result_slot(self) -> usize {
        match self {
            Self::Crafting { .. } => 0,
            Self::Furnace(_) => 2,
        }
    }

    /// Where a recipe's input `index` (as [`RecipeDisplay::ghost_slots`]
    fn input_slot(self, index: usize) -> usize {
        match self {
            Self::Crafting { .. } => 1 + index,
            Self::Furnace(_) => index,
        }
    }

    /// The button that opens the book, relative to the menu panel.
    pub fn toggle_at(self, inventory: bool, height: f32) -> (f32, f32) {
        match (self, inventory) {
            (_, true) => (104.0, height / 2.0 - 22.0),
            (Self::Crafting { .. }, false) => (5.0, height / 2.0 - 49.0),
            (Self::Furnace(_), false) => (20.0, height / 2.0 - 49.0),
        }
    }

    fn can_display(self, display: &RecipeDisplay) -> bool {
        match self {
            Self::Crafting { grid } => {
                matches!(
                    display,
                    RecipeDisplay::Shaped { .. } | RecipeDisplay::Shapeless { .. }
                ) && display.fits(grid)
            }
            Self::Furnace(_) => matches!(display, RecipeDisplay::Furnace { .. }),
        }
    }
}

/// The sprites of the book.
#[derive(Clone, Copy)]
pub struct RecipeBookSprites {
    background: TextureId,
    button: GuiSprite,
    button_highlighted: GuiSprite,
    tab: GuiSprite,
    tab_selected: GuiSprite,
    /// Enabled, disabled, enabled highlighted, disabled highlighted.
    filter: [[GuiSprite; 4]; 2],
    page_forward: [GuiSprite; 2],
    page_backward: [GuiSprite; 2],
    /// Craftable, many craftable, uncraftable, many uncraftable.
    slot: [GuiSprite; 4],
}

impl RecipeBookSprites {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut sprite = |name: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/recipe_book/{name}.png"),
            )
        };
        let mut filter = |prefix: &str| -> anyhow::Result<[GuiSprite; 4]> {
            Ok([
                sprite(&format!("{prefix}filter_enabled"))?,
                sprite(&format!("{prefix}filter_disabled"))?,
                sprite(&format!("{prefix}filter_enabled_highlighted"))?,
                sprite(&format!("{prefix}filter_disabled_highlighted"))?,
            ])
        };
        let crafting = filter("")?;
        let furnace = filter("furnace_")?;
        let background = assets.texture(renderer, "minecraft:gui/recipe_book.png")?;
        let mut sprite = |name: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/recipe_book/{name}.png"),
            )
        };
        Ok(Self {
            background,
            button: sprite("button")?,
            button_highlighted: sprite("button_highlighted")?,
            tab: sprite("tab")?,
            tab_selected: sprite("tab_selected")?,
            filter: [crafting, furnace],
            page_forward: [sprite("page_forward")?, sprite("page_forward_highlighted")?],
            page_backward: [
                sprite("page_backward")?,
                sprite("page_backward_highlighted")?,
            ],
            slot: [
                sprite("slot_craftable")?,
                sprite("slot_many_craftable")?,
                sprite("slot_uncraftable")?,
                sprite("slot_many_uncraftable")?,
            ],
        })
    }
}

/// One button of the page: recipes that share it.
struct Cell {
    /// What it cycles through.
    recipes: Vec<(RecipeId, Vec<ItemStack>)>,
    craftable: bool,
    highlight: bool,
}

struct TabView {
    tab: Tab,
    icons: Vec<ItemStack>,
    visible: bool,
}

pub struct RecipeBookPanel {
    mode: Mode,
    /// The inventory's book sits at its own button.
    inventory: bool,
    visible: bool,
    tabs: Vec<TabView>,
    tab: usize,
    page: usize,
    search: TextInput,
    focused: bool,
    cells: Vec<Cell>,
    /// Ticks, for cycling what a button shows.
    time: f32,
    cursor: (f32, f32),
    size: (f32, f32),
    /// What the cells were built from.
    built: Option<(u64, Stock, usize, String)>,
    last_placed: Option<RecipeId>,
    /// The cell whose alternatives are listed.
    popup: Option<usize>,
    shown: Vec<crate::world::item_model::KeyedStack>,
}

impl RecipeBookPanel {
    pub fn new(mode: Mode, inventory: bool) -> Self {
        let tabs = Self::tabs_of(mode)
            .into_iter()
            .map(|(tab, icons)| TabView {
                tab,
                icons,
                visible: true,
            })
            .collect();
        Self {
            mode,
            inventory,
            visible: false,
            tabs,
            tab: 0,
            page: 0,
            search: TextInput::new(50),
            focused: false,
            cells: Vec::new(),
            time: 0.0,
            cursor: (0.0, 0.0),
            size: (0.0, 0.0),
            built: None,
            last_placed: None,
            popup: None,
            shown: Vec::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The tabs and their icons (`RecipeBookComponent.TabInfo`).
    fn tabs_of(mode: Mode) -> Vec<(Tab, Vec<ItemStack>)> {
        let one = |item: &'static Item| ItemStack::new(1, item);
        let search = (Tab::Search(mode.book()), vec![one(&Item::COMPASS)]);
        let category = |c: Category, icons: &[&'static Item]| {
            (Tab::Category(c), icons.iter().map(|i| one(i)).collect())
        };
        let mut tabs = vec![search];
        match mode {
            Mode::Crafting { .. } => tabs.extend([
                category(
                    Category::CraftingEquipment,
                    &[&Item::IRON_AXE, &Item::GOLDEN_SWORD],
                ),
                category(Category::CraftingBuildingBlocks, &[&Item::BRICKS]),
                category(Category::CraftingMisc, &[&Item::LAVA_BUCKET, &Item::APPLE]),
                category(Category::CraftingRedstone, &[&Item::REDSTONE]),
            ]),
            Mode::Furnace(BookType::Furnace) => tabs.extend([
                category(Category::FurnaceFood, &[&Item::PORKCHOP]),
                category(Category::FurnaceBlocks, &[&Item::STONE]),
                category(Category::FurnaceMisc, &[&Item::LAVA_BUCKET, &Item::EMERALD]),
            ]),
            Mode::Furnace(BookType::BlastFurnace) => tabs.extend([
                category(Category::BlastFurnaceBlocks, &[&Item::REDSTONE_BLOCK]),
                category(
                    Category::BlastFurnaceMisc,
                    &[&Item::IRON_SHOVEL, &Item::GOLDEN_LEGGINGS],
                ),
            ]),
            Mode::Furnace(_) => tabs.push(category(Category::SmokerFood, &[&Item::PORKCHOP])),
        }
        tabs
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Where the menu's left edge is (`updateScreenPosition`).
    pub fn menu_left(&self, width: f32, image_width: f32) -> f32 {
        if self.visible && !self.too_narrow(width) {
            (177.0 + ((width - image_width - 200.0) / 2.0).floor()).floor()
        } else {
            ((width - image_width) / 2.0).floor()
        }
    }

    fn too_narrow(&self, width: f32) -> bool {
        width < 379.0
    }

    fn origin(&self) -> (f32, f32) {
        let offset = if self.too_narrow(self.size.0) {
            0.0
        } else {
            OFFSET_X
        };
        (
            ((self.size.0 - WIDTH) / 2.0).floor() - offset,
            ((self.size.1 - HEIGHT) / 2.0).floor(),
        )
    }

    fn toggle_rect(&self, layout: &MenuLayout) -> (f32, f32, f32, f32) {
        let (x, y) = self.mode.toggle_at(self.inventory, self.size.1);
        (layout.left() + x, y, 20.0, 18.0)
    }

    fn filtering(&self, book: &RecipeBook) -> bool {
        book.settings().get(self.mode.book()).filtering
    }

    fn selected_tab(&self) -> Tab {
        self.tabs[self.tab.min(self.tabs.len() - 1)].tab
    }

    /// Follows the server's settings and rebuilds the buttons when what they show changed.
    pub fn tick(
        &mut self,
        book: &mut RecipeBook,
        stock: Stock,
        size: (f32, f32),
        lang: &crate::i18n::Lang,
    ) -> Vec<OverlayAction> {
        self.size = size;
        self.time += 1.0;
        let open = book.settings().get(self.mode.book()).open;
        if open != self.visible {
            self.visible = open;
            if !open {
                self.popup = None;
                self.focused = false;
            }
        }
        if !self.visible {
            return Vec::new();
        }
        let key = (
            book.revision(),
            stock,
            self.tab * 2 + usize::from(self.filtering(book)),
            self.search.value(),
        );
        if self.built.as_ref() != Some(&key) {
            self.rebuild(book, &key.1, &key.3, lang);
            self.built = Some(key);
        }
        let mut seen = Vec::new();
        let first = self.page * PER_PAGE;
        for cell in self.cells.iter().skip(first).take(PER_PAGE) {
            for (id, _) in &cell.recipes {
                if book.will_highlight(*id) {
                    book.remove_highlight(*id);
                    if !id.is_local() {
                        seen.push(OverlayAction::Recipe(RecipeAction::Seen(id.0)));
                    }
                }
            }
        }
        seen
    }

    fn rebuild(
        &mut self,
        book: &RecipeBook,
        stock: &Stock,
        search: &str,
        lang: &crate::i18n::Lang,
    ) {
        let filtering = self.filtering(book);
        let mode = self.mode;
        for view in &mut self.tabs {
            view.visible = matches!(view.tab, Tab::Search(_))
                || book.collections(view.tab).any(|c| {
                    c.entries
                        .iter()
                        .filter_map(|id| book.get(*id))
                        .any(|e| mode.can_display(&e.display))
                });
        }
        if !self.tabs[self.tab.min(self.tabs.len() - 1)].visible {
            self.tab = 0;
        }
        let needle = search.to_lowercase();
        let mut cells = Vec::new();
        for collection in book.collections(self.selected_tab()) {
            let shown: Vec<_> = collection
                .entries
                .iter()
                .filter_map(|id| book.get(*id))
                .filter(|e| mode.can_display(&e.display))
                .collect();
            if shown.is_empty() {
                continue;
            }
            let craftable_ids: Vec<RecipeId> = shown
                .iter()
                .filter(|e| e.requirements.as_ref().is_some_and(|r| stock.can_craft(r)))
                .map(|e| e.id)
                .collect();
            if filtering && craftable_ids.is_empty() {
                continue;
            }
            let recipes: Vec<_> = shown
                .iter()
                .filter(|e| !filtering || craftable_ids.contains(&e.id))
                .map(|e| (e.id, e.display.result().stacks()))
                .collect();
            if !needle.is_empty()
                && !recipes.iter().any(|(_, stacks)| {
                    stacks.iter().any(|s| {
                        item_slot::hover_name(lang, s)
                            .to_lowercase()
                            .contains(&needle)
                    })
                })
            {
                continue;
            }
            cells.push(Cell {
                highlight: recipes.iter().any(|(id, _)| book.will_highlight(*id)),
                craftable: !craftable_ids.is_empty(),
                recipes,
            });
        }
        self.cells = cells;
        let pages = self.cells.len().div_ceil(PER_PAGE).max(1);
        self.page = self.page.min(pages - 1);
        self.refresh_shown();
    }

    /// The stacks the buttons and tabs show, for baking their icons.
    fn refresh_shown(&mut self) {
        self.shown.clear();
        self.shown.extend(
            self.tabs
                .iter()
                .flat_map(|t| t.icons.iter().cloned().map(Into::into)),
        );
        for cell in self.cells.iter().skip(self.page * PER_PAGE).take(PER_PAGE) {
            for (_, stacks) in &cell.recipes {
                self.shown.extend(stacks.iter().cloned().map(Into::into));
            }
        }
    }

    pub fn shown_stacks(&self) -> &[crate::world::item_model::KeyedStack] {
        &self.shown
    }

    /// The stack a cell shows now.
    fn current(&self, cell: &Cell) -> Option<(RecipeId, ItemStack)> {
        let step = (self.time / SWAP_TICKS) as usize;
        let n = cell.recipes.len();
        let (id, stacks) = cell.recipes.get(step % n.max(1))?;
        let stack = stacks.get((step / n.max(1)) % stacks.len().max(1))?;
        Some((*id, stack.clone()))
    }

    fn cell_pos(&self, i: usize) -> (f32, f32) {
        let (xo, yo) = self.origin();
        (
            xo + 11.0 + BUTTON * (i % 5) as f32,
            yo + 31.0 + BUTTON * (i / 5) as f32,
        )
    }

    fn tab_pos(&self, slot: usize) -> (f32, f32) {
        let (xo, yo) = self.origin();
        (xo - 30.0, yo + 3.0 + TAB_H * slot as f32)
    }

    fn pages(&self) -> usize {
        self.cells.len().div_ceil(PER_PAGE).max(1)
    }

    pub fn mouse_moved(&mut self, x: f32, y: f32) {
        self.cursor = (x, y);
    }

    /// A press: the toggle button, then (while visible) the book. `Some`
    pub fn press(
        &mut self,
        button: MouseButtonKind,
        book: &mut RecipeBook,
        layout: &MenuLayout,
        container: i32,
        shift: bool,
    ) -> Option<Vec<OverlayAction>> {
        let (mx, my) = self.cursor;
        let inside =
            |(x, y, w, h): (f32, f32, f32, f32)| mx >= x && my >= y && mx < x + w && my < y + h;
        let toggle = self.toggle_rect(layout);
        if inside(toggle) {
            if button != MouseButtonKind::Left {
                return Some(Vec::new());
            }
            return Some(click(vec![self.set_open(book, !self.visible)]));
        }
        if !self.visible {
            return None;
        }
        let (xo, yo) = self.origin();
        if let Some(cell) = self.popup.take() {
            if let Some(id) = self.popup_hit(cell) {
                return Some(click(self.place(id, container, shift)));
            }
            return Some(Vec::new());
        }
        let on_panel = mx >= xo && my >= yo && mx < xo + WIDTH && my < yo + HEIGHT;
        let mut slot = 0;
        for i in 0..self.tabs.len() {
            if !self.tabs[i].visible {
                continue;
            }
            let (x, y) = self.tab_pos(slot);
            slot += 1;
            if inside((x, y, TAB_W, TAB_H)) {
                if i != self.tab {
                    self.tab = i;
                    self.page = 0;
                    self.built = None;
                }
                return Some(click(Vec::new()));
            }
        }
        if !on_panel {
            self.focused = false;
            return None;
        }
        if inside((xo + 8.0, yo + 13.0, 98.0, 14.0)) {
            self.focused = true;
            self.search.click(mx - (xo + 28.0), shift);
            return Some(Vec::new());
        }
        self.focused = false;
        if inside((xo + 110.0, yo + 12.0, 26.0, 16.0)) {
            let filtering = !self.filtering(book);
            let open = self.visible;
            book.set_book(self.mode.book(), open, filtering);
            self.built = None;
            return Some(click(vec![OverlayAction::Recipe(RecipeAction::Settings {
                book: self.mode.book().index() as i32,
                open,
                filtering,
            })]));
        }
        let pages = self.pages();
        if self.page + 1 < pages && inside((xo + 93.0, yo + 137.0, 12.0, 17.0)) {
            self.page += 1;
            self.refresh_shown();
            return Some(click(Vec::new()));
        }
        if self.page > 0 && inside((xo + 38.0, yo + 137.0, 12.0, 17.0)) {
            self.page -= 1;
            self.refresh_shown();
            return Some(click(Vec::new()));
        }
        for i in 0..PER_PAGE {
            let Some(cell) = self.cells.get(self.page * PER_PAGE + i) else {
                break;
            };
            let (x, y) = self.cell_pos(i);
            if !inside((x, y, BUTTON, BUTTON)) {
                continue;
            }
            return Some(click(match button {
                MouseButtonKind::Left => match self.current(cell) {
                    Some((id, _)) => self.place(id, container, shift),
                    None => Vec::new(),
                },
                MouseButtonKind::Right if cell.recipes.len() > 1 => {
                    self.popup = Some(self.page * PER_PAGE + i);
                    Vec::new()
                }
                _ => Vec::new(),
            }));
        }
        Some(Vec::new())
    }

    fn set_open(&mut self, book: &mut RecipeBook, open: bool) -> OverlayAction {
        let filtering = self.filtering(book);
        book.set_book(self.mode.book(), open, filtering);
        self.visible = open;
        self.built = None;
        if !open {
            self.popup = None;
            self.focused = false;
        }
        OverlayAction::Recipe(RecipeAction::Settings {
            book: self.mode.book().index() as i32,
            open,
            filtering,
        })
    }

    /// `tryPlaceRecipe`: not the same uncraftable recipe twice in a row.
    fn place(&mut self, id: RecipeId, container: i32, max: bool) -> Vec<OverlayAction> {
        let craftable = self
            .cells
            .iter()
            .any(|c| c.craftable && c.recipes.iter().any(|(r, _)| *r == id));
        if id.is_local() || (!craftable && self.last_placed == Some(id)) {
            return Vec::new();
        }
        self.last_placed = Some(id);
        vec![OverlayAction::Recipe(RecipeAction::Place {
            container,
            recipe: id.0,
            max,
        })]
    }

    fn popup_rect(&self, cell: usize) -> (f32, f32, f32, f32) {
        let n = self.cells[cell].recipes.len();
        let i = cell - self.page * PER_PAGE;
        let (x, y) = self.cell_pos(i);
        let columns = n.min(4) as f32;
        let rows = n.div_ceil(4) as f32;
        (
            x - 4.0,
            y - rows * BUTTON - 4.0,
            columns * BUTTON + 8.0,
            rows * BUTTON + 4.0,
        )
    }

    fn popup_hit(&self, cell: usize) -> Option<RecipeId> {
        let (x, y, ..) = self.popup_rect(cell);
        let (mx, my) = self.cursor;
        let c = self.cells.get(cell)?;
        c.recipes.iter().enumerate().find_map(|(i, (id, _))| {
            let (cx, cy) = (
                x + 4.0 + BUTTON * (i % 4) as f32,
                y + 4.0 + BUTTON * (i / 4) as f32,
            );
            (mx >= cx && my >= cy && mx < cx + BUTTON && my < cy + BUTTON).then_some(*id)
        })
    }

    /// Typing into the search field; `true` when it took the keys.
    pub fn text(&mut self, events: &[TextEvent]) -> bool {
        if !self.visible || !self.focused {
            return false;
        }
        for event in events {
            self.search.handle(event);
        }
        true
    }

    pub fn wants_text(&self) -> bool {
        self.visible && self.focused
    }

    /// Escape: closes the popup, then leaves the field; `true` if it did.
    pub fn escape(&mut self) -> bool {
        if self.popup.take().is_some() {
            return true;
        }
        std::mem::take(&mut self.focused)
    }

    /// The panel, tabs, buttons and the toggle, under the slots.
    pub fn draw(
        &mut self,
        ui: &mut OverlayDraw,
        sprites: &RecipeBookSprites,
        book: &RecipeBook,
        layout: &MenuLayout,
    ) {
        self.size = ui.size;
        let toggle = self.toggle_rect(layout);
        let (mx, my) = self.cursor;
        let hovered =
            |(x, y, w, h): (f32, f32, f32, f32)| mx >= x && my >= y && mx < x + w && my < y + h;
        let sprite = if hovered(toggle) {
            &sprites.button_highlighted
        } else {
            &sprites.button
        };
        sprite.draw(ui.draw_list, toggle.0, toggle.1, toggle.2, toggle.3);
        if !self.visible {
            return;
        }
        let (xo, yo) = self.origin();
        ui.draw_list.push_rect(
            sprites.background,
            xo,
            yo,
            WIDTH,
            HEIGHT,
            [
                1.0 / 256.0,
                1.0 / 256.0,
                (1.0 + WIDTH) / 256.0,
                (1.0 + HEIGHT) / 256.0,
            ],
            [1.0; 4],
        );
        self.draw_search(ui, xo, yo);
        let filtering = self.filtering(book);
        let kind = usize::from(matches!(self.mode, Mode::Furnace(_)));
        let filter_hover = hovered((xo + 110.0, yo + 12.0, 26.0, 16.0));
        let filter = &sprites.filter[kind][usize::from(!filtering) + 2 * usize::from(filter_hover)];
        filter.draw(ui.draw_list, xo + 110.0, yo + 12.0, 26.0, 16.0);
        let mut slot = 0;
        for (i, view) in self.tabs.iter().enumerate() {
            if !view.visible {
                continue;
            }
            let (x, y) = self.tab_pos(slot);
            slot += 1;
            let selected = i == self.tab;
            let x = if selected { x - 2.0 } else { x };
            let tab = if selected {
                &sprites.tab_selected
            } else {
                &sprites.tab
            };
            tab.draw(ui.draw_list, x, y, TAB_W, TAB_H);
            let shift = if selected { -2.0 } else { 0.0 };
            match view.icons.as_slice() {
                [only] => ui.painter.stack(
                    ui.draw_list,
                    only,
                    x + 9.0 + 2.0 * (-shift) + shift,
                    y + 5.0,
                ),
                [a, b] => {
                    ui.painter
                        .stack(ui.draw_list, a, x + 3.0 + 2.0 * (-shift) + shift, y + 5.0);
                    ui.painter
                        .stack(ui.draw_list, b, x + 14.0 + 2.0 * (-shift) + shift, y + 5.0);
                }
                _ => {}
            }
        }
        for (i, cell) in self
            .cells
            .iter()
            .skip(self.page * PER_PAGE)
            .take(PER_PAGE)
            .enumerate()
        {
            let (x, y) = self.cell_pos(i);
            let many = cell.recipes.len() > 1;
            let frame = match (cell.craftable, many) {
                (true, false) => 0,
                (true, true) => 1,
                (false, false) => 2,
                (false, true) => 3,
            };
            sprites.slot[frame].draw(ui.draw_list, x, y, BUTTON, BUTTON);
            if let Some((_, stack)) = self.current(cell) {
                ui.painter.stack(ui.draw_list, &stack, x + 4.0, y + 4.0);
            }
            if cell.highlight {
                ui.draw_list.push_fill(
                    ui.painter.white,
                    x + 21.0,
                    y + 1.0,
                    3.0,
                    3.0,
                    argb(0xFFFF_FF55),
                );
            }
        }
        let pages = self.pages();
        if pages > 1 {
            let text = format!("{}/{pages}", self.page + 1);
            let width = ui.painter.font.text_width(&text);
            ui.painter.font.push_text_flat(
                ui.draw_list,
                ui.painter.font_textures,
                &text,
                xo - (width / 2.0).floor() + 73.0,
                yo + 141.0,
                argb(0xFFFF_FFFF),
            );
        }
        if self.page + 1 < pages {
            let hover = usize::from(hovered((xo + 93.0, yo + 137.0, 12.0, 17.0)));
            sprites.page_forward[hover].draw(ui.draw_list, xo + 93.0, yo + 137.0, 12.0, 17.0);
        }
        if self.page > 0 {
            let hover = usize::from(hovered((xo + 38.0, yo + 137.0, 12.0, 17.0)));
            sprites.page_backward[hover].draw(ui.draw_list, xo + 38.0, yo + 137.0, 12.0, 17.0);
        }
    }

    fn draw_search(&self, ui: &mut OverlayDraw, xo: f32, yo: f32) {
        let (x, y) = (xo + 28.0, yo + 16.0);
        let font = ui.painter.font;
        let value = self.search.value();
        if value.is_empty() && !self.focused {
            font.push_text_flat(
                ui.draw_list,
                ui.painter.font_textures,
                ui.lang.get("gui.recipebook.search_hint"),
                x,
                y,
                argb(0xFFAA_AAAA),
            );
            return;
        }
        font.push_text_flat(
            ui.draw_list,
            ui.painter.font_textures,
            &value,
            x,
            y,
            argb(0xFFFF_FFFF),
        );
        if self.focused && self.search.cursor_visible() {
            let at = x + font.text_width(&self.search.before_cursor());
            ui.draw_list
                .push_fill(ui.painter.white, at, y - 1.0, 1.0, 10.0, argb(0xFFD0_D0D0));
        }
    }

    /// The tooltip of the button under the mouse and the popup, over everything else.
    pub fn draw_overlay(&self, ui: &mut OverlayDraw, sprites: &RecipeBookSprites) {
        if !self.visible {
            return;
        }
        let (mx, my) = self.cursor;
        if let Some(cell) = self.popup {
            let (x, y, w, h) = self.popup_rect(cell);
            ui.draw_list
                .push_fill(ui.painter.white, x, y, w, h, argb(0xF010_1010));
            if let Some(c) = self.cells.get(cell) {
                for (i, (id, stacks)) in c.recipes.iter().enumerate() {
                    let (cx, cy) = (
                        x + 4.0 + BUTTON * (i % 4) as f32,
                        y + 4.0 + BUTTON * (i / 4) as f32,
                    );
                    let craftable = c.craftable;
                    let frame = if craftable { 0 } else { 2 };
                    sprites.slot[frame].draw(ui.draw_list, cx, cy, BUTTON, BUTTON);
                    if let Some(stack) = stacks.first() {
                        ui.painter.stack(ui.draw_list, stack, cx + 4.0, cy + 4.0);
                    }
                    let _ = id;
                }
            }
            return;
        }
        for (i, cell) in self
            .cells
            .iter()
            .skip(self.page * PER_PAGE)
            .take(PER_PAGE)
            .enumerate()
        {
            let (x, y) = self.cell_pos(i);
            if mx >= x && my >= y && mx < x + BUTTON && my < y + BUTTON {
                if let Some((_, stack)) = self.current(cell) {
                    let mut lines = item_slot::tooltip_lines(ui.lang, &stack);
                    if cell.recipes.len() > 1 {
                        lines.push(StyledText::plain(ui.lang.get("gui.recipebook.moreRecipes")));
                    }
                    ui.painter
                        .tooltip_lines(ui.draw_list, &lines, (mx, my), ui.size);
                }
                return;
            }
        }
        let (xo, yo) = self.origin();
        if mx >= xo + 110.0 && my >= yo + 12.0 && mx < xo + 136.0 && my < yo + 28.0 {
            let key = if self.built.as_ref().is_some_and(|b| b.2 % 2 == 1) {
                match self.mode {
                    Mode::Crafting { .. } => "gui.recipebook.toggleRecipes.craftable",
                    Mode::Furnace(BookType::BlastFurnace) => {
                        "gui.recipebook.toggleRecipes.blastable"
                    }
                    Mode::Furnace(BookType::Smoker) => "gui.recipebook.toggleRecipes.smokable",
                    Mode::Furnace(_) => "gui.recipebook.toggleRecipes.smeltable",
                }
            } else {
                "gui.recipebook.toggleRecipes.all"
            };
            ui.painter.tooltip_lines(
                ui.draw_list,
                &[StyledText::plain(ui.lang.get(key))],
                (mx, my),
                ui.size,
            );
        }
    }

    /// The ghost recipe in the menu's slots.
    pub fn draw_ghost(&self, ui: &mut OverlayDraw, layout: &MenuLayout, display: &RecipeDisplay) {
        let grid = match self.mode {
            Mode::Crafting { grid } => grid,
            Mode::Furnace(_) => 1,
        };
        let step = (self.time / SWAP_TICKS) as usize;
        let mut ghost = |slot: usize, stacks: Vec<ItemStack>, big: bool| {
            let Some((x, y)) = layout.slot_origin(slot) else {
                return;
            };
            let Some(stack) = stacks.get(step % stacks.len().max(1)) else {
                return;
            };
            ui.painter.stack(ui.draw_list, stack, x, y);
            let size = if big { 24.0 } else { 16.0 };
            let (x, y) = if big { (x - 4.0, y - 4.0) } else { (x, y) };
            ui.draw_list
                .push_fill(ui.painter.white, x, y, size, size, argb(0x30FF_FFFF));
        };
        ghost(self.mode.result_slot(), display.result().stacks(), false);
        for (index, slot) in display.ghost_slots(grid) {
            ghost(self.mode.input_slot(index), slot.stacks(), false);
        }
    }
}

/// What the inventory holds that can go into a recipe.
pub fn stock_of(
    inventory: &crate::world::inventory::Inventory,
    menu: Option<&crate::world::inventory::OpenContainer>,
    mode: Mode,
) -> Stock {
    let player = inventory.slots().get(9..45).unwrap_or(&[]);
    let grid: &[crate::world::item_model::KeyedStack] = match (menu, mode) {
        (Some(menu), Mode::Crafting { grid }) => menu.slots.get(1..1 + grid * grid).unwrap_or(&[]),
        (None, Mode::Crafting { .. }) => inventory.slots().get(1..5).unwrap_or(&[]),
        _ => &[],
    };
    Stock::from_stacks(player.iter().chain(grid).map(|s| &**s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::world::recipes::{Entry, ItemSet, SlotDisplay};

    fn recipe(id: i32, result: &'static Item, needs: &'static Item) -> Entry {
        Entry {
            id: RecipeId(id),
            display: RecipeDisplay::Shapeless {
                ingredients: vec![SlotDisplay::Item(needs)],
                result: SlotDisplay::Item(result),
                station: SlotDisplay::Empty,
            },
            group: None,
            category: Category::CraftingMisc,
            requirements: Some(vec![ItemSet::Items(vec![needs])]),
        }
    }

    fn open_book() -> RecipeBook {
        let mut book = RecipeBook::default();
        book.add(recipe(0, &Item::STICK, &Item::OAK_PLANKS));
        book.add(recipe(1, &Item::LADDER, &Item::STICK));
        book.rebuild();
        book.set_book(BookType::Crafting, true, false);
        book
    }

    #[test]
    fn an_open_book_shifts_the_menu_right_unless_the_screen_is_narrow() {
        let mut panel = RecipeBookPanel::new(Mode::Crafting { grid: 3 }, false);
        assert_eq!(panel.menu_left(427.0, 176.0), 125.0);
        panel.visible = true;
        assert_eq!(panel.menu_left(427.0, 176.0), 177.0 + 25.0);
        assert_eq!(panel.menu_left(300.0, 176.0), 62.0, "too narrow: centred");
    }

    #[test]
    fn the_buttons_follow_the_settings_and_the_stock() {
        let lang = Lang::empty();
        let mut book = open_book();
        let mut panel = RecipeBookPanel::new(Mode::Crafting { grid: 3 }, false);
        let planks = Stock::from_stacks(&[ItemStack::new(4, &Item::OAK_PLANKS)]);
        panel.tick(&mut book, planks.clone(), (427.0, 240.0), &lang);
        assert!(panel.visible());
        assert_eq!(panel.cells.len(), 2);
        assert_eq!(
            panel.cells.iter().filter(|c| c.craftable).count(),
            1,
            "only the sticks can be made"
        );
        book.set_book(BookType::Crafting, true, true);
        panel.tick(&mut book, planks, (427.0, 240.0), &lang);
        assert_eq!(panel.cells.len(), 1);
        book.set_book(BookType::Crafting, false, true);
        panel.tick(&mut book, Stock::default(), (427.0, 240.0), &lang);
        assert!(!panel.visible());
    }

    #[test]
    fn a_click_on_a_button_places_its_recipe_and_a_new_one_is_seen_once() {
        let lang = Lang::empty();
        let mut book = open_book();
        book.add_highlight(RecipeId(0));
        let mut panel = RecipeBookPanel::new(Mode::Crafting { grid: 3 }, false);
        let planks = Stock::from_stacks(&[ItemStack::new(4, &Item::OAK_PLANKS)]);
        let seen = panel.tick(&mut book, planks, (427.0, 240.0), &lang);
        assert!(matches!(
            seen.as_slice(),
            [OverlayAction::Recipe(RecipeAction::Seen(0))]
        ));
        assert!(!book.will_highlight(RecipeId(0)));
        let mut layout = crate::gui::overlay::container::menu_layout(
            crate::world::inventory::ContainerKind::Crafting,
        );
        layout.place_at(panel.menu_left(427.0, 176.0), 240.0);
        let (x, y) = panel.cell_pos(0);
        panel.mouse_moved(x + 3.0, y + 3.0);
        let actions = panel
            .press(MouseButtonKind::Left, &mut book, &layout, 7, true)
            .expect("the book took the click");
        assert!(matches!(
            actions.as_slice(),
            [
                OverlayAction::Sound(_),
                OverlayAction::Recipe(RecipeAction::Place {
                    container: 7,
                    max: true,
                    ..
                })
            ]
        ));
        panel.mouse_moved(layout.left() + 100.0, 120.0);
        assert!(
            panel
                .press(MouseButtonKind::Left, &mut book, &layout, 7, false)
                .is_none()
        );
    }
}
