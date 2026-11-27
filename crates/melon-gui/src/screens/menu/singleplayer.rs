//! Vanilla `SelectWorldScreen`.

use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::screens::menu::confirm_delete::confirm_delete_world;
use crate::screens::menu::connecting::ConnectingScreen;
use crate::screens::menu::create_world::CreateWorldScreen;
use crate::screens::menu::edit_world::EditWorldScreen;
use crate::text::font::LINE_HEIGHT;
use crate::text::{StyledText, TextStyle};
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, SPACING, wide_column_left};
use crate::widgets::row_icon::{self, Layout, Moves};
use crate::widgets::scroll_list::{ListClick, ScrollList};
use crate::widgets::set::{KeyResult, WidgetSet};
use crate::widgets::{Rect, Ui};
use image::RgbaImage;
use melon_base::gfx::TextureId;
use melon_net::join::Target;
use melon_net::singleplayer::{self, WorldCreationMode, WorldEntry};
use std::collections::HashMap;

const ROW_W: f32 = 270.0;
const ROW_H: f32 = 36.0;
/// `SelectWorldScreen`'s `HeaderAndFooterLayout(this, 8 + 9 + 8 + 20 + 4, 60)`.
const LAYOUT: HeaderFooter = HeaderFooter::new(49.0, 60.0);
/// Footer grid: 4 columns of 71 with `GridLayout.columnSpacing(8)`.
const COL_W: f32 = 71.0;
const GRID_W: f32 = COL_W * 4.0 + SPACING * 3.0;
/// `-8355712`: the grey of a world's folder line and its info line.
const DETAIL_RGB: [f32; 3] = [128.0 / 255.0; 3];
/// `Component.withColor(-65536)`: Hardcore in pure red.
const HARDCORE_RGB: [f32; 3] = [1.0, 0.0, 0.0];
const DETAIL_COLOR: [f32; 4] = [128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0];
/// `WorldListEntry`.
const TEXT_GAP: f32 = 3.0;
const TEXT_MARGIN: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Search,
    Play,
    Create,
    Edit,
    Delete,
    Recreate,
    Back,
}

/// The buttons that act on the selected world.
const NEEDS_SELECTION: [Id; 4] = [Id::Play, Id::Edit, Id::Delete, Id::Recreate];

pub struct SingleplayerScreen {
    entries: Vec<WorldEntry>,
    /// Visible `entries` indices after the search filter; the list's rows.
    filtered: Vec<usize>,
    list: ScrollList,
    widgets: WidgetSet<Id>,
    mouse: (f32, f32),
    /// `saves/<folder>/icon.png`s read but not yet on the GPU, by folder.
    pending_icons: Vec<(String, RgbaImage)>,
    /// The worlds' icon textures by folder; a world without one shows the default icon.
    icons: HashMap<String, TextureId>,
}

impl SingleplayerScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        let button = |label, w| Button::sized(label, w, BUTTON_H);
        let mut widgets = WidgetSet::new()
            .with_field(
                Id::Search,
                EditBox::new_search("gui.selectWorld.search", 200.0, BUTTON_H),
            )
            .with_button(Id::Play, button("selectWorld.select", BUTTON_W))
            .with_button(Id::Create, button("selectWorld.create", BUTTON_W))
            .with_button(Id::Edit, button("selectWorld.edit", COL_W))
            .with_button(Id::Delete, button("selectWorld.delete", COL_W))
            .with_button(Id::Recreate, button("selectWorld.recreate", COL_W))
            .with_button(Id::Back, button("gui.back", COL_W));
        widgets.focus(Id::Search);
        let mut screen = Self {
            entries: Vec::new(),
            filtered: Vec::new(),
            list: ScrollList::new(ROW_W, ROW_H),
            widgets,
            mouse: (-1.0, -1.0),
            pending_icons: Vec::new(),
            icons: HashMap::new(),
        };
        screen.reload(ctx);
        screen
    }

    /// Re-scans `saves/`.
    fn reload(&mut self, ctx: &ScreenContext) {
        self.entries = singleplayer::list_worlds(ctx.game_dir);
        self.icons.clear();
        self.pending_icons = self
            .entries
            .iter()
            .filter_map(|entry| {
                let path = singleplayer::world_dir(ctx.game_dir, &entry.folder).join("icon.png");
                let image = image::open(path).ok()?.to_rgba8();
                row_icon::is_favicon(&image).then(|| (entry.folder.clone(), image))
            })
            .collect();
        self.apply_filter();
    }

    fn selected(&self) -> Option<&WorldEntry> {
        self.list
            .selected()
            .map(|row| &self.entries[self.filtered[row]])
    }

    /// `SelectWorldScreen.joinWorld`.
    fn play_selected(&self, ctx: &ScreenContext) -> ScreenAction {
        let Some(entry) = self.selected() else {
            return ScreenAction::None;
        };
        let target = Target::World {
            save_path: singleplayer::world_dir(ctx.game_dir, &entry.folder),
            game_dir: ctx.game_dir.to_path_buf(),
            settings: entry.settings(),
        };
        ScreenAction::open(ConnectingScreen::new(target, ctx))
    }

    fn update_button_status(&mut self) {
        let has_selection = self.list.selected().is_some();
        for id in NEEDS_SELECTION {
            self.widgets.set_enabled(id, has_selection);
        }
    }

    /// Row `row`'s 32x32 icon, inside `WorldListEntry`'s 2 px padding.
    fn icon_rect(&self, row: usize) -> Rect {
        let rect = self.list.row_rect(row);
        Rect::new(rect.x + 2.0, rect.y + 2.0, row_icon::SIZE, row_icon::SIZE)
    }

    /// The row whose join arrow the mouse is on.
    fn hovered_join(&self) -> Option<usize> {
        let row = self.list.row_at(self.mouse.0, self.mouse.1)?;
        row_icon::zone(self.icon_rect(row), self.mouse, Layout::Whole, Moves::NONE)?;
        Some(row)
    }

    fn apply_filter(&mut self) {
        let needle = self.widgets.field(Id::Search).value().to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                needle.is_empty()
                    || e.name.to_lowercase().contains(&needle)
                    || e.folder.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        self.list.set_len(self.filtered.len());
        self.list.select(None);
        self.update_button_status();
    }

    fn press(&self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        if id == Id::Create {
            return ScreenAction::open(CreateWorldScreen::new_create(ctx));
        }
        if id == Id::Back {
            return ScreenAction::Back;
        }
        let Some(entry) = self.selected() else {
            return ScreenAction::None;
        };
        match id {
            Id::Play => self.play_selected(ctx),
            Id::Edit => ScreenAction::open(EditWorldScreen::new(entry, ctx)),
            Id::Delete => {
                let folder = entry.folder.clone();
                ScreenAction::open(confirm_delete_world(folder, &entry.name, ctx))
            }
            Id::Recreate => ScreenAction::open(CreateWorldScreen::new_recreate(entry, ctx)),
            Id::Search | Id::Create | Id::Back => ScreenAction::None,
        }
    }
}

/// `StringWidget.setMaxWidth`'s clip.
fn clip(ui: &Ui, text: &str, width: f32) -> String {
    let font = ui.font();
    if font.text_width(text) <= width {
        return text.to_owned();
    }
    let room = width - font.text_width("...");
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if font.text_width(&out) > room {
            out.pop();
            break;
        }
    }
    out.push_str("...");
    out
}

/// The row's second line: `levelId (last played)`.
fn id_and_date(entry: &WorldEntry) -> String {
    if entry.last_played == -1 {
        entry.folder.clone()
    } else {
        format!(
            "{} ({})",
            entry.folder,
            singleplayer::datetime::short(entry.last_played)
        )
    }
}

/// `LevelSummary.createInfo`.
fn info_text(ui: &Ui, entry: &WorldEntry) -> StyledText {
    let grey = TextStyle::colored(DETAIL_RGB);
    let mut text = StyledText::default();
    let mode = ui.tr(entry.game_mode.translation_key());
    if entry.game_mode == WorldCreationMode::Hardcore {
        text.push(mode, &TextStyle::colored(HARDCORE_RGB));
    } else {
        text.push(mode, &grey);
    }
    if entry.allow_cheats {
        text.push(", ", &grey);
        text.push(ui.tr("selectWorld.commands"), &grey);
    }
    if let Some(version) = &entry.version {
        text.push(", ", &grey);
        text.push(ui.tr("selectWorld.version"), &grey);
        text.push(" ", &grey);
        text.push(version, &grey);
    }
    text
}

impl Screen for SingleplayerScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.widgets
            .set_position(Id::Search, wide_column_left(width), 8.0 + LINE_HEIGHT + 8.0);
        self.list
            .set_bounds(LAYOUT.top(), LAYOUT.bottom(height), width);

        let grid_x = ((width - GRID_W) / 2.0).floor();
        let row1_y = LAYOUT.footer_y(height, BUTTON_H * 2.0 + 4.0);
        let row2_y = row1_y + BUTTON_H + 4.0;
        self.widgets.set_position(Id::Play, grid_x, row1_y);
        self.widgets
            .set_position(Id::Create, grid_x + (COL_W + SPACING) * 2.0, row1_y);
        self.widgets.row(
            &[Id::Edit, Id::Delete, Id::Recreate, Id::Back],
            grid_x,
            row2_y,
            SPACING,
        );
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.widgets.hover(x, y);
        self.list.on_mouse_move(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets
            .cursor()
            .or(self.list.cursor())
            .or_else(|| self.hovered_join().map(|_| CursorKind::Pointer))
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.list.blur();
        match self.widgets.click(x, y) {
            Some(Id::Search) => return ScreenAction::None,
            Some(id) => return self.press(id, ctx),
            None => {}
        }
        let on_join_arrow = self.hovered_join().is_some();
        if let ListClick::Row { double, .. } = self.list.on_click(x, y) {
            self.update_button_status();
            if double || on_join_arrow {
                ctx.click();
                return self.play_selected(ctx);
            }
        }
        ScreenAction::None
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        let step = match event {
            TextEvent::Up => Some(-1),
            TextEvent::Down => Some(1),
            TextEvent::PageUp => Some(-self.list.page_rows()),
            TextEvent::PageDown => Some(self.list.page_rows()),
            _ => None,
        };
        if let Some(step) = step {
            if self.list.move_selection(step) {
                self.widgets.blur();
                self.update_button_status();
            }
            return ScreenAction::None;
        }
        if matches!(event, TextEvent::Enter)
            && self.list.is_focused()
            && self.list.selected().is_some()
        {
            ctx.click();
            return self.play_selected(ctx);
        }
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => self.press(id, ctx),
            KeyResult::Edited(_) => {
                self.apply_filter();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("selectWorld.title");
        ui.title(title, 8.0);

        let hovered = self.list.row_at(self.mouse.0, self.mouse.1);
        self.list.draw(ui, |ui, row, rect| {
            let entry = &self.entries[self.filtered[row]];
            let icon = self.icon_rect(row);
            let text_x = icon.x + row_icon::SIZE + TEXT_GAP;
            let max_width = ROW_W - (text_x - rect.x) - TEXT_MARGIN;
            let name = clip(ui, &entry.name, max_width);
            ui.text(&name, text_x, icon.y + 1.0, [1.0; 4]);
            let id_line = clip(ui, &id_and_date(entry), max_width);
            ui.text(&id_line, text_x, icon.y + 12.0, DETAIL_COLOR);
            let info = info_text(ui, entry);
            if ui.styled_width(&info) > max_width {
                let plain = clip(ui, &info.to_plain(), max_width);
                ui.text(&plain, text_x, icon.y + 21.0, DETAIL_COLOR);
            } else {
                ui.styled(&info, text_x, icon.y + 21.0, DETAIL_RGB);
            }
            let texture = self
                .icons
                .get(&entry.folder)
                .copied()
                .unwrap_or(ui.assets.server_list.unknown_server);
            ui.sprite(texture, icon);
            if hovered == Some(row) {
                let zone = row_icon::zone(icon, self.mouse, Layout::Whole, Moves::NONE);
                let sprites = &ui.assets.world_row;
                row_icon::draw(ui, icon, sprites, zone, Moves::NONE);
            }
        });

        self.widgets.draw(ui);
    }

    fn upload_images(&mut self, upload: &mut dyn FnMut(&RgbaImage) -> Option<TextureId>) {
        for (folder, image) in std::mem::take(&mut self.pending_icons) {
            if let Some(texture) = row_icon::upload_cached(&image, upload) {
                self.icons.insert(folder, texture);
            }
        }
    }

    fn on_resume(&mut self, ctx: &ScreenContext) {
        self.reload(ctx);
    }
}
