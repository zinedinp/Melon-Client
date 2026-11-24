//! Vanilla `PackSelectionScreen` for resource packs.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use image::RgbaImage;

use crate::assets::packs::server::PromptRequest;
use crate::assets::packs::{Compatibility, repository};
use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::text::font::LINE_HEIGHT;
use crate::gui::text::{StyledText, colors};
use crate::gui::widgets::button::Button;
use crate::gui::widgets::dialog::DialogScreen;
use crate::gui::widgets::edit_box::EditBox;
use crate::gui::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, SPACING};
use crate::gui::widgets::row_icon::{self, Moves};
use crate::gui::widgets::scroll_list::{ListClick, ScrollList};
use crate::gui::widgets::set::{KeyResult, WidgetSet};
use crate::gui::widgets::{Rect, Ui};
use crate::i18n::Lang;
use crate::renderer::TextureId;

/// `LIST_WIDTH`.
const LIST_W: f32 = 200.0;
/// `TransferableSelectionList`'s row height.
const ROW_H: f32 = 36.0;
/// `setHeaderHeight(4 + 9 + 4 + 9 + 4 + 15 + 4)`; the footer is the default.
const LAYOUT: HeaderFooter = HeaderFooter::new(49.0, 33.0);
/// The list's own title row above its entries.
const LIST_TITLE_H: f32 = 13.0;
/// `SEARCH_BOX_HEIGHT`.
const SEARCH_H: f32 = 15.0;
/// `MAX_DESCRIPTION_WIDTH_PIXELS`, two lines at most.
const DESCRIPTION_W: f32 = 157.0;
/// `-8355712`: the description's grey.
const DESCRIPTION_COLOR: [f32; 3] = [128.0 / 255.0; 3];
/// `-8978432`: the frame behind an incompatible pack's icon.
const INCOMPATIBLE_FRAME: [f32; 4] = [119.0 / 255.0, 0.0, 0.0, 1.0];
/// How often (in frames) the folder is checked for new or removed packs.
const RESCAN_TICKS: u32 = 40;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Search,
    OpenFolder,
    Done,
}

/// One pack row.
struct Entry {
    /// `vanilla` or `file/<name>`.
    id: String,
    /// The file name; `vanilla`'s is translated when drawn.
    title: String,
    /// `pack.mcmeta`'s text component.
    description: serde_json::Value,
    compatibility: Compatibility,
    /// `vanilla`: always selected, always at the bottom.
    required: bool,
    /// `pack.png`, to upload; `None` once uploaded or when missing.
    icon_image: Option<RgbaImage>,
    icon: Option<TextureId>,
}

impl Entry {
    fn matches(&self, needle: &str) -> bool {
        needle.is_empty() || self.title.to_lowercase().contains(needle)
    }
}

/// Which list a row is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Available,
    Selected,
}

pub struct PackSelectionScreen {
    pack_dir: PathBuf,
    /// Set for a world's data packs (the Create World screen's Data Packs)
    data: Option<Rc<RefCell<Vec<String>>>>,
    available: Vec<Entry>,
    /// Top pack first, like the screen shows it; `vanilla` last.
    selected: Vec<Entry>,
    /// The rows each list shows after the search, as indices.
    shown_available: Vec<usize>,
    shown_selected: Vec<usize>,
    available_list: ScrollList,
    selected_list: ScrollList,
    widgets: WidgetSet<Id>,
    mouse: (f32, f32),
    /// The folder's contents when last read.
    fingerprint: Vec<(String, u64)>,
    ticks: u32,
    /// `textures/misc/unknown_pack.png`, for packs without an icon.
    unknown_icon: Option<RgbaImage>,
    unknown_texture: Option<TextureId>,
    width: f32,
}

impl PackSelectionScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        let mut widgets = WidgetSet::new()
            .with_field(
                Id::Search,
                EditBox::new_search("gui.packSelection.search", LIST_W, SEARCH_H),
            )
            .with_button(
                Id::OpenFolder,
                Button::sized("pack.openFolder", BUTTON_W, BUTTON_H),
            )
            .with_button(Id::Done, Button::sized("gui.done", BUTTON_W, BUTTON_H));
        widgets.focus(Id::Search);
        let mut screen = Self {
            pack_dir: repository::folder(ctx.game_dir),
            data: None,
            available: Vec::new(),
            selected: Vec::new(),
            shown_available: Vec::new(),
            shown_selected: Vec::new(),
            available_list: ScrollList::new(LIST_W, ROW_H),
            selected_list: ScrollList::new(LIST_W, ROW_H),
            widgets,
            mouse: (-1.0, -1.0),
            fingerprint: Vec::new(),
            ticks: 0,
            unknown_icon: crate::assets::texture_image(ctx.assets_root, "misc/unknown_pack"),
            unknown_texture: None,
            width: 0.0,
        };
        let selected = ctx.config.resource_packs.clone();
        screen.rescan(&selected);
        screen
    }

    /// The Data Packs screen of Create World.
    pub fn new_data(ctx: &ScreenContext, dir: PathBuf, chosen: Rc<RefCell<Vec<String>>>) -> Self {
        let mut screen = Self::new(ctx);
        screen.pack_dir = dir;
        let selected: Vec<String> = chosen.borrow().clone();
        screen.data = Some(chosen);
        screen.rescan(&selected);
        screen
    }

    /// The key a pack's icon is cached under.
    fn icon_key(&self, id: &str) -> String {
        if self.data.is_some() {
            format!("data/{id}")
        } else {
            id.to_owned()
        }
    }

    /// Data packs: the folder's packs in the lists' entries.
    fn data_entries(&self) -> (HashMap<String, Entry>, Entry) {
        let entries = crate::singleplayer::data_packs::discover(&self.pack_dir)
            .into_iter()
            .map(|pack| {
                let key = self.icon_key(&pack.id);
                let cached = cached_icon(&key);
                let entry = Entry {
                    id: pack.id.clone(),
                    title: pack.name,
                    description: pack.description,
                    compatibility: Compatibility::Compatible,
                    required: false,
                    icon_image: cached
                        .is_none()
                        .then(|| pack.icon.and_then(|png| crate::assets::decode_png(&png)))
                        .flatten(),
                    icon: cached,
                };
                (pack.id, entry)
            })
            .collect();
        let vanilla = Entry {
            id: crate::singleplayer::data_packs::VANILLA.to_owned(),
            title: String::new(),
            description: serde_json::json!({"translate": "dataPack.vanilla.description"}),
            compatibility: Compatibility::Compatible,
            required: true,
            icon_image: None,
            icon: None,
        };
        (entries, vanilla)
    }

    /// Reads the folder again; `selected` (ids, lowest first) stay selected.
    fn rescan(&mut self, selected: &[String]) {
        self.fingerprint = fingerprint(&self.pack_dir);
        if self.data.is_some() {
            let (mut entries, vanilla) = self.data_entries();
            self.selected = selected
                .iter()
                .rev()
                .filter_map(|id| entries.remove(id))
                .collect();
            self.selected.push(vanilla);
            let mut available: Vec<Entry> = entries.into_values().collect();
            available.sort_by(|a, b| a.id.cmp(&b.id));
            self.available = available;
            self.apply_filter();
            return;
        }
        let packs = repository::discover(self.pack_dir.parent().unwrap_or(Path::new(".")));
        let mut entries: HashMap<String, Entry> = packs
            .iter()
            .map(|pack| {
                let entry = Entry {
                    id: pack.id.clone(),
                    title: pack.title.clone(),
                    description: pack.meta.description.clone(),
                    compatibility: pack.meta.compatibility(),
                    required: false,
                    icon_image: cached_icon(&pack.id)
                        .is_none()
                        .then(|| pack.icon().and_then(|png| crate::assets::decode_png(&png)))
                        .flatten(),
                    icon: cached_icon(&pack.id),
                };
                (pack.id.clone(), entry)
            })
            .collect();
        let vanilla = Entry {
            id: repository::VANILLA_ID.to_owned(),
            title: String::new(),
            description: serde_json::json!({"translate": "resourcePack.vanilla.description"}),
            compatibility: Compatibility::Compatible,
            required: true,
            icon_image: None,
            icon: None,
        };
        self.selected = selected
            .iter()
            .rev()
            .filter_map(|id| entries.remove(id))
            .collect();
        self.selected.push(vanilla);
        let mut available: Vec<Entry> = entries.into_values().collect();
        available.sort_by(|a, b| a.id.cmp(&b.id));
        self.available = available;
        self.apply_filter();
    }

    /// The selected ids, lowest first, as `options.txt` keeps them.
    fn selected_ids(&self) -> Vec<String> {
        self.selected.iter().rev().map(|e| e.id.clone()).collect()
    }

    fn apply_filter(&mut self) {
        let needle = self.widgets.field(Id::Search).value().to_lowercase();
        let shown = |entries: &[Entry]| -> Vec<usize> {
            entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.matches(&needle))
                .map(|(i, _)| i)
                .collect()
        };
        self.shown_available = shown(&self.available);
        self.shown_selected = shown(&self.selected);
        self.available_list.set_len(self.shown_available.len());
        self.selected_list.set_len(self.shown_selected.len());
    }

    fn list(&self, side: Side) -> &ScrollList {
        match side {
            Side::Available => &self.available_list,
            Side::Selected => &self.selected_list,
        }
    }

    /// The list the keyboard acts on: the one with the screen's focus.
    fn keyboard_side(&self) -> Option<Side> {
        if self.available_list.is_focused() {
            Some(Side::Available)
        } else if self.selected_list.is_focused() {
            Some(Side::Selected)
        } else {
            None
        }
    }

    fn entry(&self, side: Side, row: usize) -> &Entry {
        match side {
            Side::Available => &self.available[self.shown_available[row]],
            Side::Selected => &self.selected[self.shown_selected[row]],
        }
    }

    /// The row under the mouse, in either list.
    fn hovered(&self) -> Option<(Side, usize)> {
        let (x, y) = self.mouse;
        [Side::Available, Side::Selected]
            .into_iter()
            .find_map(|side| self.list(side).row_at(x, y).map(|row| (side, row)))
    }

    fn icon_rect(&self, side: Side, row: usize) -> Rect {
        let rect = self.list(side).row_rect(row);
        Rect::new(rect.x + 2.0, rect.y + 2.0, row_icon::SIZE, row_icon::SIZE)
    }

    /// The arrow zone a row's icon has under the mouse.
    fn icon_zone(&self, side: Side, row: usize) -> Option<row_icon::Zone> {
        row_icon::zone(
            self.icon_rect(side, row),
            self.mouse,
            self.icon_layout(side),
            self.icon_moves(side, row),
        )
    }

    /// `TransferableSelectionList.PackEntry`.
    fn icon_layout(&self, side: Side) -> row_icon::Layout {
        match side {
            Side::Available => row_icon::Layout::Whole,
            Side::Selected => row_icon::Layout::Selected,
        }
    }

    fn icon_moves(&self, side: Side, row: usize) -> Moves {
        match side {
            Side::Available => Moves::NONE,
            Side::Selected => {
                let index = self.shown_selected[row];
                Moves {
                    up: self.can_move(index, -1),
                    down: self.can_move(index, 1),
                }
            }
        }
    }

    /// What clicking the icon at the mouse would do.
    fn icon_action(&self, side: Side, row: usize) -> Option<IconAction> {
        if side == Side::Selected && self.entry(side, row).required {
            return None;
        }
        Some(match (side, self.icon_zone(side, row)?) {
            (Side::Available, _) => IconAction::Select,
            (Side::Selected, row_icon::Zone::Primary) => IconAction::Unselect,
            (Side::Selected, row_icon::Zone::MoveUp) => IconAction::MoveUp,
            (Side::Selected, row_icon::Zone::MoveDown) => IconAction::MoveDown,
        })
    }

    /// Whether selected pack `index` can swap with its neighbour `by`
    fn can_move(&self, index: usize, by: isize) -> bool {
        index
            .checked_add_signed(by)
            .and_then(|other| self.selected.get(other))
            .is_some_and(|other| !other.required)
    }

    fn act(&mut self, side: Side, row: usize, action: IconAction) {
        match (side, action) {
            (Side::Available, IconAction::Select) => {
                let entry = self.available.remove(self.shown_available[row]);
                self.selected.insert(0, entry);
            }
            (Side::Selected, IconAction::Unselect) => {
                let entry = self.selected.remove(self.shown_selected[row]);
                let at = self
                    .available
                    .binary_search_by(|e| e.id.cmp(&entry.id))
                    .unwrap_or_else(|i| i);
                self.available.insert(at, entry);
            }
            (Side::Selected, IconAction::MoveUp) => {
                let i = self.shown_selected[row];
                self.selected.swap(i, i - 1);
            }
            (Side::Selected, IconAction::MoveDown) => {
                let i = self.shown_selected[row];
                self.selected.swap(i, i + 1);
            }
            _ => {}
        }
        self.apply_filter();
    }

    /// Done or Escape (`PackSelectionScreen.onClose` commits too).
    fn done(&self) -> ScreenAction {
        if let Some(chosen) = &self.data {
            let mut ids = chosen.borrow_mut();
            ids.retain(|id| !id.starts_with("file/"));
            ids.extend(
                self.selected_ids()
                    .into_iter()
                    .filter(|id| id.starts_with("file/")),
            );
            return ScreenAction::Back;
        }
        let selected = self.selected_ids();
        let incompatible = self
            .selected
            .iter()
            .filter(|e| e.compatibility != Compatibility::Compatible && !e.required)
            .map(|e| e.id.clone())
            .collect();
        ScreenAction::SetResourcePacks {
            selected,
            incompatible,
        }
    }

    fn press(&mut self, id: Id) -> ScreenAction {
        match id {
            Id::Done => self.done(),
            Id::OpenFolder => {
                let _ = std::fs::create_dir_all(&self.pack_dir);
                if let Err(e) = open::that(&self.pack_dir) {
                    crate::warn!("Pack folder not opened: {e}");
                }
                ScreenAction::None
            }
            Id::Search => ScreenAction::None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IconAction {
    Select,
    Unselect,
    MoveUp,
    MoveDown,
}

impl Screen for PackSelectionScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.width = width;
        let left = ((width - LIST_W) / 2.0).floor();
        self.widgets
            .set_position(Id::Search, left, 4.0 + LINE_HEIGHT * 2.0 + 8.0 + 4.0);
        let top = LAYOUT.top() + LIST_TITLE_H;
        let bottom = LAYOUT.bottom(height);
        let half = (width / 2.0).floor();
        self.available_list
            .set_area(half - 15.0 - LIST_W, top, bottom, LIST_W);
        self.selected_list
            .set_area(half + 15.0, top, bottom, LIST_W);
        let y = LAYOUT.footer_y(height, BUTTON_H);
        self.widgets
            .centered_row(&[Id::OpenFolder, Id::Done], width, y, SPACING);
    }

    fn tick(&mut self) {
        self.ticks += 1;
    }

    fn poll(&mut self) -> ScreenAction {
        if self.ticks >= RESCAN_TICKS {
            self.ticks = 0;
            if fingerprint(&self.pack_dir) != self.fingerprint {
                let selected = self.selected_ids();
                self.rescan(&selected);
            }
        }
        ScreenAction::None
    }

    fn upload_images(&mut self, upload: &mut dyn FnMut(&RgbaImage) -> Option<TextureId>) {
        if let Some(image) = self.unknown_icon.take() {
            self.unknown_texture = row_icon::upload_cached(&image, upload);
        }
        let data = self.data.is_some();
        let key = |id: &str| {
            if data {
                format!("data/{id}")
            } else {
                id.to_owned()
            }
        };
        for entry in self.available.iter_mut().chain(&mut self.selected) {
            if let Some(image) = entry.icon_image.take() {
                entry.icon = row_icon::upload_cached(&image, upload);
                if let Some(texture) = entry.icon {
                    remember_icon(&key(&entry.id), texture);
                }
            }
        }
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.widgets.hover(x, y);
        self.available_list.on_mouse_move(x, y);
        self.selected_list.on_mouse_move(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets
            .cursor()
            .or(self.available_list.cursor())
            .or(self.selected_list.cursor())
            .or_else(|| {
                let (side, row) = self.hovered()?;
                self.icon_action(side, row).map(|_| CursorKind::Pointer)
            })
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.available_list.blur();
        self.selected_list.blur();
        match self.widgets.click(x, y) {
            Some(Id::Search) => return ScreenAction::None,
            Some(id) => {
                ctx.click();
                return self.press(id);
            }
            None => {}
        }
        let Some((side, row)) = self.hovered() else {
            self.available_list.on_click(x, y);
            self.selected_list.on_click(x, y);
            return ScreenAction::None;
        };
        let on_icon = self.icon_action(side, row);
        let required = self.entry(side, row).required;
        let list = match side {
            Side::Available => &mut self.available_list,
            Side::Selected => &mut self.selected_list,
        };
        if let ListClick::Row { double, .. } = list.on_click(x, y) {
            let action = on_icon.or_else(|| {
                (double && !required).then_some(match side {
                    Side::Available => IconAction::Select,
                    Side::Selected => IconAction::Unselect,
                })
            });
            if let Some(action) = action {
                ctx.click();
                self.act(side, row, action);
                self.available_list.select(None);
                self.selected_list.select(None);
            }
        }
        ScreenAction::None
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.available_list.on_mouse_up();
        self.selected_list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        match self.hovered().map(|(side, _)| side) {
            Some(Side::Selected) => self.selected_list.on_scroll(delta),
            Some(Side::Available) => self.available_list.on_scroll(delta),
            None if self.mouse.0 >= self.width / 2.0 => self.selected_list.on_scroll(delta),
            None => self.available_list.on_scroll(delta),
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        if let Some(side) = self.keyboard_side() {
            let list = match side {
                Side::Available => &mut self.available_list,
                Side::Selected => &mut self.selected_list,
            };
            let step = match event {
                TextEvent::Up => Some(-1),
                TextEvent::Down => Some(1),
                TextEvent::PageUp => Some(-list.page_rows()),
                TextEvent::PageDown => Some(list.page_rows()),
                _ => None,
            };
            if let Some(step) = step {
                list.move_selection(step);
                return ScreenAction::None;
            }
            if matches!(event, TextEvent::Enter)
                && let Some(row) = list.selected()
            {
                let entry = self.entry(side, row);
                let action = match side {
                    Side::Available => Some(IconAction::Select),
                    Side::Selected => (!entry.required).then_some(IconAction::Unselect),
                };
                if let Some(action) = action {
                    ctx.click();
                    self.act(side, row, action);
                    self.available_list.select(None);
                    self.selected_list.select(None);
                }
                return ScreenAction::None;
            }
        } else if matches!(event, TextEvent::Down | TextEvent::Up)
            && !self.available_list.is_empty()
        {
            self.widgets.blur();
            self.available_list
                .move_selection(if matches!(event, TextEvent::Down) {
                    1
                } else {
                    -1
                });
            return ScreenAction::None;
        }
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => {
                ctx.click();
                self.press(id)
            }
            KeyResult::Edited(_) => {
                self.apply_filter();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn on_escape(&mut self, _ctx: &ScreenContext) -> Option<ScreenAction> {
        Some(self.done())
    }

    fn on_files_dropped(&mut self, paths: Vec<PathBuf>, _ctx: &ScreenContext) -> ScreenAction {
        drop_confirm(paths, self.pack_dir.clone())
    }

    fn on_resume(&mut self, _ctx: &ScreenContext) {
        let selected = self.selected_ids();
        self.rescan(&selected);
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let center = (self.width / 2.0).floor();
        let title = ui.tr(if self.data.is_some() {
            "dataPack.title"
        } else {
            "resourcePack.title"
        });
        ui.centered(title, center, 4.0, [1.0; 4]);
        let drop_info = ui.tr("pack.dropInfo");
        let gray = [colors::GRAY[0], colors::GRAY[1], colors::GRAY[2], 1.0];
        ui.centered(drop_info, center, 4.0 + LINE_HEIGHT + 4.0, gray);

        ui.list_background(LAYOUT.top(), LAYOUT.bottom(ui.height));
        let hovered = self.hovered();
        for side in [Side::Available, Side::Selected] {
            let list = self.list(side);
            let key = match side {
                Side::Available => "pack.available.title",
                Side::Selected => "pack.selected.title",
            };
            let style = crate::gui::text::TextStyle {
                bold: true,
                underlined: true,
                ..Default::default()
            };
            let mut header = StyledText::default();
            header.push(ui.tr(key), &style);
            let x = (list.row_left() + LIST_W / 2.0 - ui.styled_width(&header) / 2.0).floor();
            ui.styled(&header, x, LAYOUT.top() + 3.0, colors::WHITE);
            list.draw_rows(ui, |ui, row, rect| {
                let keyboard = list.selected() == Some(row) && list.is_focused();
                self.draw_row(
                    ui,
                    side,
                    row,
                    rect,
                    hovered == Some((side, row)) || keyboard,
                );
            });
        }
        self.widgets.draw(ui);
    }
}

impl PackSelectionScreen {
    fn draw_row(&self, ui: &mut Ui, side: Side, row: usize, rect: Rect, hovered: bool) {
        let entry = self.entry(side, row);
        let icon = self.icon_rect(side, row);
        let incompatible = entry.compatibility != Compatibility::Compatible;
        if incompatible {
            ui.fill(
                Rect::new(rect.x + 1.0, rect.y + 1.0, rect.w - 2.0, rect.h - 2.0),
                INCOMPATIBLE_FRAME,
            );
        }
        if let Some(texture) = entry.icon.or(self.unknown_texture) {
            ui.sprite(texture, icon);
        }
        let text_x = icon.x + row_icon::SIZE + 2.0;
        let title = if hovered && incompatible {
            StyledText::colored(ui.tr("pack.incompatible"), colors::RED)
        } else if entry.required {
            StyledText::plain(ui.tr(if self.data.is_some() {
                "dataPack.vanilla.name"
            } else {
                "resourcePack.vanilla.name"
            }))
        } else {
            StyledText::plain(&entry.title)
        };
        ui.styled(&title, text_x, icon.y + 1.0, colors::WHITE);
        let lines = description_lines(ui, &entry.description, entry.compatibility);
        for (i, line) in lines.iter().enumerate() {
            ui.styled(
                line,
                text_x,
                icon.y + 12.0 + i as f32 * LINE_HEIGHT,
                DESCRIPTION_COLOR,
            );
        }
        if hovered {
            if side == Side::Selected && entry.required {
                row_icon::veil(ui, icon);
            } else {
                let sprites = match side {
                    Side::Available => &ui.assets.pack_row,
                    Side::Selected => &ui.assets.pack_selected_row,
                };
                let moves = self.icon_moves(side, row);
                row_icon::draw(ui, icon, sprites, self.icon_zone(side, row), moves);
            }
        }
    }
}

/// A pack's description as at most two lines of [`DESCRIPTION_W`].
fn description_lines(
    ui: &Ui,
    description: &serde_json::Value,
    compatibility: Compatibility,
) -> Vec<StyledText> {
    let lang = ui.lang;
    let text = match description {
        serde_json::Value::String(s) => StyledText::formatted(s),
        other => StyledText::from_component(&crate::component::decode::component(other), lang),
    };
    let text = text.with_default_color(DESCRIPTION_COLOR);
    let mut lines = ui.wrap_styled(&text, DESCRIPTION_W);
    if compatibility != Compatibility::Compatible {
        lines.truncate(1);
        let key = format!("pack.incompatible.{}", compatibility.key());
        lines.push(StyledText::colored(lang.get_dynamic(&key), colors::RED));
    }
    lines.truncate(2);
    lines
}

/// The folder's packs by name and modification time.
fn fingerprint(dir: &Path) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let modified = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            (e.file_name().to_string_lossy().into_owned(), modified)
        })
        .collect();
    out.sort();
    out
}

/// Pack icons already on the GPU, by pack id, so reopening the screen doesn't upload them
/// again.
fn icon_cache() -> &'static Mutex<HashMap<String, TextureId>> {
    static ICONS: OnceLock<Mutex<HashMap<String, TextureId>>> = OnceLock::new();
    ICONS.get_or_init(Default::default)
}

fn cached_icon(id: &str) -> Option<TextureId> {
    icon_cache().lock().ok()?.get(id).copied()
}

fn remember_icon(id: &str, texture: TextureId) {
    if let Ok(mut icons) = icon_cache().lock() {
        icons.insert(id.to_owned(), texture);
    }
}

/// `PackSelectionScreen.onFilesDrop`.
fn drop_confirm(paths: Vec<PathBuf>, pack_dir: PathBuf) -> ScreenAction {
    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    if names.is_empty() {
        return ScreenAction::None;
    }
    let dialog = DialogScreen::new()
        .line("pack.dropConfirm", [1.0; 4])
        .line(names.join(", "), [1.0; 4])
        .button("gui.yes", BUTTON_W, move |_| {
            let _ = std::fs::create_dir_all(&pack_dir);
            for path in &paths {
                if let Err(e) = copy_pack(path, &pack_dir) {
                    crate::warn!("Pack {path:?} not copied: {e}");
                }
            }
            ScreenAction::Back
        })
        .button("gui.no", BUTTON_W, |_| ScreenAction::Back);
    ScreenAction::open(dialog)
}

/// Copies a zip or a pack folder into `dir`.
fn copy_pack(from: &Path, dir: &Path) -> std::io::Result<()> {
    let name = from
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;
    let to = dir.join(name);
    if from.is_dir() {
        copy_dir(from, &to)
    } else {
        std::fs::copy(from, &to).map(|_| ())
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// `ClientCommonPacketListenerImpl.PackConfirmScreen`.
pub struct PackConfirmScreen {
    dialog: DialogScreen,
}

impl PackConfirmScreen {
    pub fn new(request: PromptRequest, lang: &Lang) -> Self {
        let (line1, line2, yes, no) = if request.required {
            (
                "multiplayer.requiredTexturePrompt.line1",
                "multiplayer.requiredTexturePrompt.line2",
                "gui.proceed",
                "menu.disconnect",
            )
        } else {
            (
                "multiplayer.texturePrompt.line1",
                "multiplayer.texturePrompt.line2",
                "gui.yes",
                "gui.no",
            )
        };
        let header = lang.get(line2);
        let second = match &request.message {
            Some(message) => {
                let message = StyledText::from_component(message, lang).to_plain();
                lang.format(
                    "multiplayer.texturePrompt.serverPrompt",
                    &[header, &message],
                )
            }
            None => header.to_owned(),
        };
        let second_color = if request.required {
            [colors::YELLOW[0], colors::YELLOW[1], colors::YELLOW[2], 1.0]
        } else {
            [1.0; 4]
        };
        let dialog = DialogScreen::new()
            .line(line1, [1.0; 4])
            .line(second, second_color)
            .button(yes, BUTTON_W, |_| ScreenAction::AnswerServerPacks(true))
            .button(no, BUTTON_W, |_| ScreenAction::AnswerServerPacks(false));
        Self { dialog }
    }
}

impl Screen for PackConfirmScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        self.dialog.resize(ctx);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.dialog.on_mouse_move(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.dialog.cursor()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.dialog.on_click(x, y, ctx)
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        self.dialog.on_text_event(event, ctx)
    }

    fn on_escape(&mut self, _ctx: &ScreenContext) -> Option<ScreenAction> {
        Some(ScreenAction::AnswerServerPacks(false))
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        self.dialog.draw(ui, ctx);
    }
}
