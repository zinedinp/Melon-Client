//! Vanilla `CreativeModeInventoryScreen`.

use crate::world::item_model::KeyedStack;
use crate::world::item_model::StackKey;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use crate::assets::Assets;
use crate::gui::hud::item_slot::{self};
use crate::gui::overlay::container_input::{self, ContainerMouse, MouseButtonKind, SlotClick};
use crate::gui::overlay::{
    ChatExit, Overlay, OverlayAction, OverlayContext, OverlayDraw, OverlayKind,
};
use crate::gui::screen::TextEvent;
use crate::gui::sprite::GuiSprite;
use crate::gui::widgets::text_input::{Edit, TextInput};
use crate::i18n::Lang;
use crate::renderer::{Renderer, TextureId};
use crate::settings::input::InputBind;
use crate::world::creative_tabs::{CreativeTab, TabType, tabs};
use crate::world::hotbars::{HOTBARS, Hotbars};
use crate::world::inventory::{Inventory, SLOT_COUNT, empty};

/// `searchBox`: `EditBox(font, leftPos + 82, topPos + 6, 80, 9)`, borderless.
pub(super) const SEARCH_BOX: (f32, f32, f32, f32) = (82.0, 6.0, 80.0, 9.0);

const IMAGE_WIDTH: f32 = 195.0;
const IMAGE_HEIGHT: f32 = 136.0;
const TAB_WIDTH: f32 = 26.0;
const TAB_HEIGHT: f32 = 32.0;
const ROWS: usize = 5;
const COLUMNS: usize = 9;
/// The inventory tab's destroy-item slot, after the player's 46.
const DESTROY_SLOT: usize = SLOT_COUNT;
/// `searchBox.setMaxLength(50)`.
const SEARCH_MAX_LEN: usize = 50;
mod draw;
mod rules;
mod slots;
mod view;
pub use view::CreativeView;

pub struct CreativeTextures {
    /// `tab_items`, `tab_item_search`, `tab_inventory`.
    items: TextureId,
    search: TextureId,
    inventory: TextureId,
    top: [[GuiSprite; 7]; 2],
    bottom: [[GuiSprite; 7]; 2],
    scroller: GuiSprite,
    scroller_disabled: GuiSprite,
}

impl CreativeTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let background = "minecraft:gui/container/creative_inventory";
        let sprites = "minecraft:gui/sprites/container/creative_inventory";
        let mut tabs = |row: &str, selected: &str| -> anyhow::Result<[GuiSprite; 7]> {
            let mut out = [GuiSprite::from(TextureId(0)); 7];
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = GuiSprite::load(
                    renderer,
                    assets,
                    &format!("{sprites}/tab_{row}_{selected}_{}.png", i + 1),
                )?;
            }
            Ok(out)
        };
        let top = [tabs("top", "unselected")?, tabs("top", "selected")?];
        let bottom = [tabs("bottom", "unselected")?, tabs("bottom", "selected")?];
        let mut load = |id: &str| assets.texture(renderer, id);
        let items = load(&format!("{background}/tab_items.png"))?;
        let search = load(&format!("{background}/tab_item_search.png"))?;
        let inventory = load(&format!("{background}/tab_inventory.png"))?;
        let mut sprite =
            |name: &str| GuiSprite::load(renderer, assets, &format!("{sprites}/{name}.png"));
        Ok(Self {
            items,
            search,
            inventory,
            top,
            bottom,
            scroller: sprite("scroller")?,
            scroller_disabled: sprite("scroller_disabled")?,
        })
    }
}

/// What a slot of the screen shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// One of the 45 item picker slots.
    Picker(usize),
    /// A slot of the player's `InventoryMenu`.
    Player(usize),
    Destroy,
}

/// What a click asks the server for: slots to set and stacks to drop.
#[derive(Default)]
pub struct CreativeChanges {
    /// `(menu slot, stack)`; slot `-1` drops the stack.
    pub sets: Vec<(i16, ItemStack)>,
}

pub struct CreativeOverlay {
    left: f32,
    top: f32,
    /// Indices into [`tabs`] of the tabs shown (the operator tab hidden).
    tabs: Vec<usize>,
    /// Index into `tabs`.
    selected: usize,
    /// `menu.items`: what the picker slots scroll over.
    items: Vec<KeyedStack>,
    /// Picker items that can't be taken (the saved hotbars' placeholders).
    locked: Vec<bool>,
    scroll: f32,
    scrolling: bool,
    /// The left button went down in the search box and is still held.
    selecting: bool,
    /// `CreativeModeInventoryScreen.searchBox`: at most 50 characters.
    search: TextInput,
    mouse: ContainerMouse,
    cursor: (f32, f32),
    has_clicked_outside: bool,
    /// The chat key switched to the search tab; its character is not typed.
    ignore_text_input: bool,
    empty: KeyedStack,
}

impl CreativeOverlay {
    /// Opens on `remembered_tab` (an index into [`tabs`]).
    pub fn new(
        width: f32,
        height: f32,
        hotbars: &Hotbars,
        lang: &Lang,
        remembered_tab: usize,
    ) -> Self {
        let shown: Vec<usize> = tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| !tab.operator_only)
            .map(|(i, _)| i)
            .collect();
        let selected = shown.iter().position(|&i| i == remembered_tab).unwrap_or(0);
        let mut screen = Self {
            left: 0.0,
            top: 0.0,
            tabs: shown,
            selected: 0,
            items: Vec::new(),
            locked: Vec::new(),
            scroll: 0.0,
            scrolling: false,
            search: TextInput::new(SEARCH_MAX_LEN),
            selecting: false,
            mouse: ContainerMouse::default(),
            cursor: (0.0, 0.0),
            has_clicked_outside: false,
            ignore_text_input: false,
            empty: KeyedStack::default(),
        };
        screen.resize(width, height);
        screen.select_tab(selected, hotbars, lang);
        screen
    }

    /// Runs the shared container mouse logic over this screen.
    fn with_mouse<R>(
        &mut self,
        inventory: &Inventory,
        f: impl FnOnce(&mut ContainerMouse, &CreativeView) -> R,
    ) -> R {
        let mut mouse = std::mem::take(&mut self.mouse);
        let result = f(&mut mouse, &self.view(inventory));
        self.mouse = mouse;
        result
    }

    /// Runs clicks through `slotClicked`: the slots they set, to send.
    fn apply(&mut self, clicks: Vec<SlotClick>, inventory: &mut Inventory) -> OverlayAction {
        let mut sets = Vec::new();
        for click in clicks {
            sets.extend(self.slot_clicked(inventory, click).sets);
        }
        if sets.is_empty() {
            OverlayAction::None
        } else {
            OverlayAction::CreativeSlots(sets)
        }
    }

    /// The selected tab, as an index into [`tabs`]: what to reopen on.
    pub fn tab_index(&self) -> usize {
        self.tabs[self.selected]
    }

    fn tab(&self) -> &'static CreativeTab {
        &tabs()[self.tabs[self.selected]]
    }

    fn tab_type(&self) -> TabType {
        self.tab().kind
    }

    /// `selectTab`: fills the picker with the tab's items.
    fn select_tab(&mut self, index: usize, hotbars: &Hotbars, lang: &Lang) {
        let old = self.tab_type();
        self.selected = index;
        self.mouse.cancel();
        self.items.clear();
        self.locked.clear();
        let tab = self.tab();
        match tab.kind {
            TabType::Hotbar => {
                for i in 0..HOTBARS {
                    if hotbars.is_empty(i) {
                        for column in 0..COLUMNS {
                            if column == i {
                                self.items.push(ItemStack::new(1, &Item::PAPER).into());
                                self.locked.push(true);
                            } else {
                                self.items.push(KeyedStack::default());
                                self.locked.push(false);
                            }
                        }
                    } else {
                        self.items
                            .extend(hotbars.hotbars[i].iter().cloned().map(KeyedStack::from));
                        self.locked.extend([false; COLUMNS]);
                    }
                }
            }
            TabType::Category => self
                .items
                .extend(tab.items.iter().cloned().map(KeyedStack::from)),
            TabType::Search => {
                if old != TabType::Search {
                    self.search.clear();
                }
                self.refresh_search(lang);
            }
            TabType::Inventory => {}
        }
        if tab.kind != TabType::Search {
            self.search.clear();
        }
        self.locked.resize(self.items.len(), false);
        self.scroll = 0.0;
    }

    /// `refreshSearchResults`.
    fn refresh_search(&mut self, lang: &Lang) {
        self.items.clear();
        let tab = self.tab();
        let term = self.search.value().to_lowercase();
        let term = term.trim_start_matches('#');
        self.items.extend(
            tab.items
                .iter()
                .filter(|stack| {
                    term.is_empty()
                        || stack.item.registry_key.contains(term)
                        || item_slot::display_name(lang, stack)
                            .to_lowercase()
                            .contains(term)
                })
                .cloned()
                .map(KeyedStack::from),
        );
        self.locked = vec![false; self.items.len()];
        self.scroll = 0.0;
    }

    /// `ItemPickerMenu.calculateRowCount`: rows past the five shown.
    fn extra_rows(&self) -> usize {
        self.items.len().div_ceil(COLUMNS).saturating_sub(ROWS)
    }

    fn can_scroll(&self) -> bool {
        self.tab().scrollbar && self.items.len() > ROWS * COLUMNS
    }

    /// `getRowIndexForScroll`.
    fn first_row(&self) -> usize {
        (self.scroll * self.extra_rows() as f32 + 0.5).max(0.0) as usize
    }

    /// The scroll wheel over the screen.
    fn scroll_picker(&mut self, lines: f32) {
        if self.can_scroll() {
            let rows = self.extra_rows().max(1) as f32;
            self.scroll = (self.scroll - lines / rows).clamp(0.0, 1.0);
        }
    }

    /// The item a picker slot shows, and whether it can be taken.
    fn picker(&self, index: usize) -> (&KeyedStack, bool) {
        let at = index + self.first_row() * COLUMNS;
        match self.items.get(at) {
            Some(stack) => (stack, self.locked.get(at).copied().unwrap_or(false)),
            None => (&self.empty, false),
        }
    }

    /// `mouseClicked`'s own part: tabs and the scrollbar.
    fn press_tabs(&mut self, left_button: bool) -> bool {
        if !left_button {
            return false;
        }
        if self.tab_at(self.cursor).is_some() {
            return true;
        }
        if self.tab_type() != TabType::Inventory && self.inside_scrollbar(self.cursor) {
            self.scrolling = self.can_scroll();
            return true;
        }
        false
    }

    /// `mouseReleased`'s own part: a click on a tab selects it.
    fn release_tabs(&mut self, left_button: bool, hotbars: &Hotbars, lang: &Lang) -> bool {
        if !left_button {
            return false;
        }
        self.scrolling = false;
        if let Some(tab) = self.tab_at(self.cursor) {
            self.select_tab(tab, hotbars, lang);
            return true;
        }
        false
    }

    /// Typed text goes to the search box on the search tab.
    fn on_text(&mut self, event: &TextEvent, lang: &Lang) -> bool {
        if self.tab_type() != TabType::Search {
            return false;
        }
        if self.ignore_text_input && matches!(event, TextEvent::Char(_)) {
            return true;
        }
        let changed = self.search.handle(event) == Edit::Changed;
        if changed {
            self.refresh_search(lang);
        }
        changed
    }

    /// `keyPressed`.
    fn key_takes_text(&mut self, key_char: Option<&str>, hotbars: &Hotbars, lang: &Lang) -> bool {
        self.ignore_text_input = false;
        if self.tab_type() != TabType::Search {
            if matches!(key_char, Some("t" | "T")) {
                self.ignore_text_input = true;
                if let Some(search) = self
                    .tabs
                    .iter()
                    .position(|&i| tabs()[i].kind == TabType::Search)
                {
                    self.select_tab(search, hotbars, lang);
                }
                return true;
            }
            return false;
        }
        let number = key_char.is_some_and(|c| c.len() == 1 && c.as_bytes()[0].is_ascii_digit());
        let hovered = self
            .hovered_slot(self.cursor)
            .and_then(|i| self.slot_kind(i));
        let swap_ok = match hovered {
            Some(Slot::Picker(i)) => !self.picker(i).0.is_empty(),
            Some(_) => true,
            None => false,
        };
        !(number && swap_ok)
    }

    /// The slot under the mouse, as a menu click target for the keys.
    fn hovered(&self) -> Option<usize> {
        self.hovered_slot(self.cursor)
    }

    /// Every stack on show, for baking their icons.
    fn visible_stacks(&self) -> impl Iterator<Item = &dyn StackKey> {
        (0..ROWS * COLUMNS)
            .map(|i| self.picker(i).0 as &dyn StackKey)
            .chain(self.tabs.iter().map(|&t| &tabs()[t].icon as &dyn StackKey))
    }
}

impl Overlay for CreativeOverlay {
    fn kind(&self) -> OverlayKind {
        OverlayKind::Creative
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.left = ((width - IMAGE_WIDTH) / 2.0).floor();
        self.top = ((height - IMAGE_HEIGHT) / 2.0).floor();
    }

    fn effects_panel(&self) -> Option<crate::gui::hud::effects::EffectsPanel> {
        Some(crate::gui::hud::effects::EffectsPanel {
            left: self.left,
            top: self.top,
            width: IMAGE_WIDTH,
            mouse: self.cursor,
        })
    }

    fn mouse_moved(&mut self, x: f32, y: f32, dragging: bool, ctx: &mut OverlayContext) {
        self.cursor = (x, y);
        self.selecting &= dragging;
        if self.selecting {
            self.search.drag(x - (self.left + SEARCH_BOX.0));
        }
        if self.scrolling {
            let top = self.top + 18.0;
            self.scroll = ((y - top - 7.5) / (112.0 - 15.0)).clamp(0.0, 1.0);
        }
        if dragging {
            self.with_mouse(ctx.inventory, |mouse, view| mouse.drag(view, (x, y)));
        }
    }

    fn mouse_button(
        &mut self,
        button: MouseButtonKind,
        pressed: bool,
        ctx: &mut OverlayContext,
    ) -> OverlayAction {
        let left = button == MouseButtonKind::Left;
        if !pressed {
            self.selecting = false;
        } else if self.tab_type() == TabType::Search {
            let (x, y) = (self.cursor.0 - self.left, self.cursor.1 - self.top);
            let (bx, by, bw, bh) = SEARCH_BOX;
            if (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y) {
                self.search.click(x - bx, ctx.shift);
                self.selecting = left;
                return OverlayAction::None;
            }
        }
        let taken = if pressed {
            self.press_tabs(left)
        } else {
            self.release_tabs(left, ctx.hotbars, ctx.lang)
        };
        if taken {
            return OverlayAction::None;
        }
        self.note_outside();
        let (cursor, shift) = (self.cursor, ctx.shift);
        let clicks = self.with_mouse(ctx.inventory, |mouse, view| {
            if pressed {
                mouse.press(view, button, cursor, shift, true)
            } else {
                mouse.release(view, button, cursor, shift)
            }
        });
        self.apply(clicks, ctx.inventory)
    }

    fn scroll(&mut self, lines: f32, _ctx: &mut OverlayContext) {
        self.scroll_picker(lines);
    }

    fn key(
        &mut self,
        bind: Option<&InputBind>,
        text: &[TextEvent],
        ctx: &mut OverlayContext,
    ) -> Vec<OverlayAction> {
        let key_char = bind.and_then(|bind| {
            if ctx
                .config
                .input
                .binds
                .get(crate::settings::input::Action::Chat)
                == bind
            {
                return Some("t");
            }
            bind.name()
                .strip_prefix("key.keyboard.")
                .filter(|s| s.len() == 1)
        });
        if self.key_takes_text(key_char, ctx.hotbars, ctx.lang) {
            for event in text {
                self.on_text(event, ctx.lang);
            }
            return Vec::new();
        }
        let Some(bind) = bind else {
            return Vec::new();
        };
        if container_input::is_close_bind(bind, ctx.config) {
            return vec![OverlayAction::Close(ChatExit::Interrupted)];
        }
        let carried_empty = ctx.inventory.carried().is_empty();
        let Some(click) = self.hovered().and_then(|slot| {
            container_input::slot_bind(bind, ctx.config, slot, carried_empty, ctx.control)
        }) else {
            return Vec::new();
        };
        vec![self.apply(vec![click], ctx.inventory)]
    }

    /// The tab is remembered for the next opening, and the creative cursor is emptied.
    fn closed(&mut self, _exit: ChatExit, ctx: &mut OverlayContext) {
        *ctx.creative_tab = self.tab_index();
        ctx.inventory.set_carried(empty());
    }

    fn draw(&mut self, ui: &mut OverlayDraw) {
        self.draw_screen(
            ui.draw_list,
            ui.textures.creative,
            ui.textures.inventory,
            ui.painter,
            ui.inventory,
            ui.lang,
            ui.size,
            ui.entity_preview,
        );
    }

    /// `CreativeModeInventoryScreen.extractBackground`.
    fn entity_preview(&self) -> Option<crate::gui::entity_preview::EntityPreview> {
        (self.tab_type() == crate::world::creative_tabs::TabType::Inventory).then_some(
            crate::gui::entity_preview::EntityPreview {
                x0: self.left + 73.0,
                y0: self.top + 6.0,
                x1: self.left + 105.0,
                y1: self.top + 49.0,
                size: 20.0,
                offset_y: 0.0625,
                mouse: self.cursor,
                entity: None,
            },
        )
    }

    fn visit_stacks(
        &self,
        inventory: &Inventory,
        visit: &mut dyn FnMut(&dyn StackKey) -> bool,
    ) -> bool {
        let own = inventory.slots().iter().chain([inventory.carried()]);
        self.visible_stacks()
            .chain(own.map(|s| s as &dyn StackKey))
            .all(|stack| stack.stack().is_empty() || visit(stack))
    }
}
