//! Vanilla `JoinMultiplayerScreen`.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Instant;

use crate::component::Component;
use image::RgbaImage;

use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::screens::menu::confirm_delete::confirm_delete_server;
use crate::gui::screens::menu::connecting::ConnectingScreen;
use crate::gui::screens::menu::direct_connect::DirectConnectScreen;
use crate::gui::screens::menu::edit_server::EditServerScreen;
use crate::gui::text::colors;
use crate::gui::text::font::{Font, LINE_HEIGHT, sprite_index};
use crate::gui::text::{StyledText, TextStyle};
use crate::gui::widgets::button::Button;
use crate::gui::widgets::layout::{BUTTON_H, HeaderFooter};
use crate::gui::widgets::row_icon::{self, Layout, Moves, Zone};
use crate::gui::widgets::scroll_list::{ListClick, ScrollList};
use crate::gui::widgets::set::{KeyResult, WidgetSet};
use crate::gui::widgets::{Rect, Ui};
use crate::join::Target;
use crate::network::status::{self, CannotResolve, ServerStatus};
use crate::renderer::TextureId;
use crate::settings::server_list::{self, ServerEntry};

/// `HeaderAndFooterLayout`: the default header, and the two-row footer.
const LAYOUT: HeaderFooter = HeaderFooter::new(33.0, 60.0);
/// `ServerSelectionList`: `getRowWidth() = 305`, rows 36 high.
const ROW_W: f32 = 305.0;
const ROW_H: f32 = 36.0;
/// `AbstractSelectionList.Entry.CONTENT_PADDING`.
const PADDING: f32 = 2.0;
/// `OnlineServerEntry.ICON_SIZE` and `SPACING`.
const ICON: f32 = row_icon::SIZE;
const SPACING_X: f32 = 5.0;
/// `STATUS_ICON_WIDTH` and `STATUS_ICON_HEIGHT`.
const STATUS_W: f32 = 10.0;
const STATUS_H: f32 = 8.0;
const SPACING: f32 = 4.0;
/// Top row: three 100-wide buttons. Bottom row: four 74-wide.
const TOP_W: f32 = 100.0;
const BOTTOM_W: f32 = 74.0;
/// `-8355712`: the MOTD's and the player count's colour.
const GREY: [f32; 3] = [0.5, 0.5, 0.5];
/// `-65536`: the "Can't connect" line is pure red, unlike `ChatFormatting.RED`.
const PURE_RED: [f32; 3] = [1.0, 0.0, 0.0];

mod draw;
mod rows;

use rows::*;

pub struct MultiplayerScreen {
    servers: Vec<ServerEntry>,
    rows: Vec<Row>,
    next_id: u64,
    /// Bumped on every refresh, so a slow answer from before it is dropped.
    generation: u64,
    ping_tx: Sender<PingResult>,
    ping_rx: Receiver<PingResult>,
    list: ScrollList,
    widgets: WidgetSet<Id>,
    mouse: (f32, f32),
    assets_root: PathBuf,
    /// Drives the "pinging" animation.
    opened: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Join,
    Direct,
    Add,
    Edit,
    Delete,
    Refresh,
    Back,
}

impl MultiplayerScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        let (ping_tx, ping_rx) = channel();
        let button = |label, w| Button::sized(label, w, BUTTON_H);
        let mut screen = Self {
            servers: Vec::new(),
            rows: Vec::new(),
            next_id: 0,
            generation: 0,
            ping_tx,
            ping_rx,
            list: ScrollList::new(ROW_W, ROW_H),
            widgets: WidgetSet::new()
                .with_button(Id::Join, button("selectServer.select", TOP_W))
                .with_button(Id::Direct, button("selectServer.direct", TOP_W))
                .with_button(Id::Add, button("selectServer.add", TOP_W))
                .with_button(Id::Edit, button("selectServer.edit", BOTTOM_W))
                .with_button(Id::Delete, button("selectServer.delete", BOTTOM_W))
                .with_button(Id::Refresh, button("selectServer.refresh", BOTTOM_W))
                .with_button(Id::Back, button("gui.back", BOTTOM_W)),
            mouse: (-1.0, -1.0),
            assets_root: ctx.assets_root.to_path_buf(),
            opened: Instant::now(),
        };
        screen.reload(ctx);
        screen
    }

    /// Re-reads `servers.json` and pings every server again.
    fn reload(&mut self, ctx: &ScreenContext) {
        let known_icons: HashMap<String, TextureId> = self
            .servers
            .iter()
            .zip(&self.rows)
            .filter_map(|(server, row)| Some((server.ip.clone(), row.icon?)))
            .collect();
        self.servers = server_list::load(ctx.game_dir);
        self.list.set_len(self.servers.len());
        self.generation += 1;
        self.rows.clear();
        for server in &self.servers {
            let id = self.next_id;
            self.next_id += 1;
            self.rows.push(Row {
                id,
                ping: Ping::Pinging,
                icon: known_icons.get(&server.ip).copied(),
                pending_icon: None,
                sprites: None,
            });
            let (tx, generation, ip) = (self.ping_tx.clone(), self.generation, server.ip.clone());
            let _ = thread::Builder::new()
                .name("server-ping".to_owned())
                .spawn(move || {
                    let result =
                        status::ping(&ip).map_err(|e| e.downcast_ref::<CannotResolve>().is_some());
                    let _ = tx.send((generation, id, result));
                });
        }
        self.update_buttons();
    }

    fn update_buttons(&mut self) {
        let selected = self.list.selected().is_some();
        for id in [Id::Join, Id::Edit, Id::Delete] {
            self.widgets.set_enabled(id, selected);
        }
    }

    fn join_server(&self, index: usize, ctx: &ScreenContext) -> ScreenAction {
        let Some(server) = self.servers.get(index) else {
            return ScreenAction::None;
        };
        ScreenAction::open(ConnectingScreen::new(
            Target::Server(server.ip.clone()),
            ctx,
        ))
    }

    fn join_selected(&self, ctx: &ScreenContext) -> ScreenAction {
        match self.list.selected() {
            Some(index) => self.join_server(index, ctx),
            None => ScreenAction::None,
        }
    }

    /// `ServerList.swap` for the two rows, which carry their answers along.
    fn swap(&mut self, a: usize, b: usize, ctx: &ScreenContext) {
        server_list::swap(ctx.game_dir, a, b);
        self.servers.swap(a, b);
        self.rows.swap(a, b);
        if self.list.selected() == Some(a) {
            self.list.select(Some(b));
        } else if self.list.selected() == Some(b) {
            self.list.select(Some(a));
        }
    }

    fn press(&mut self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        let selected = self.list.selected();
        match (id, selected) {
            (Id::Join, _) => self.join_selected(ctx),
            (Id::Direct, _) => ScreenAction::open(DirectConnectScreen::new(ctx)),
            (Id::Add, _) => ScreenAction::open(EditServerScreen::new_add(ctx)),
            (Id::Edit, Some(i)) => {
                ScreenAction::open(EditServerScreen::new_edit(i, &self.servers[i]))
            }
            (Id::Delete, Some(i)) => {
                ScreenAction::open(confirm_delete_server(i, &self.servers[i].name, ctx))
            }
            (Id::Refresh, _) => {
                self.reload(ctx);
                ScreenAction::None
            }
            (Id::Back, _) => ScreenAction::Back,
            (Id::Edit | Id::Delete, None) => ScreenAction::None,
        }
    }

    /// A row's content area, inside its padding.
    fn content(rect: Rect) -> Rect {
        Rect::new(
            rect.x + PADDING,
            rect.y + PADDING,
            rect.w - 2.0 * PADDING,
            rect.h - 2.0 * PADDING,
        )
    }

    fn icon_rect(&self, index: usize) -> Rect {
        let content = Self::content(self.list.row_rect(index));
        Rect::new(content.x, content.y, ICON, ICON)
    }

    fn moves(&self, index: usize) -> Moves {
        Moves::of(index, self.servers.len())
    }

    /// The zone under the mouse, in the row under the mouse.
    fn hovered_zone(&self) -> Option<(usize, Zone)> {
        let index = self.list.row_at(self.mouse.0, self.mouse.1)?;
        let zone = row_icon::zone(
            self.icon_rect(index),
            self.mouse,
            Layout::Server,
            self.moves(index),
        )?;
        Some((index, zone))
    }
}

impl Screen for MultiplayerScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.list
            .set_bounds(LAYOUT.top(), LAYOUT.bottom(height), width);
        let top_y = LAYOUT.footer_y(height, BUTTON_H * 2.0 + SPACING);
        let bottom_y = top_y + BUTTON_H + SPACING;
        self.widgets
            .centered_row(&[Id::Join, Id::Direct, Id::Add], width, top_y, SPACING);
        self.widgets.centered_row(
            &[Id::Edit, Id::Delete, Id::Refresh, Id::Back],
            width,
            bottom_y,
            SPACING,
        );
    }

    fn tick(&mut self) {
        while let Ok((generation, id, result)) = self.ping_rx.try_recv() {
            if generation != self.generation {
                continue;
            }
            let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
                continue;
            };
            row.ping = match result {
                Ok(mut status) => {
                    if let Some(favicon) = status.favicon.take() {
                        row.pending_icon = Some(favicon);
                    }
                    Ping::Done(Box::new(Answer {
                        motd: (!status.description.is_null())
                            .then(|| crate::component::decode::component(&status.description)),
                        motd_lines: OnceCell::new(),
                        players: player_count(status.online, status.max),
                        compatible: status.protocol == crate::network::PROTOCOL_VERSION,
                        status,
                    }))
                }
                Err(resolve) => Ping::Failed { resolve },
            };
        }
    }

    fn upload_images(&mut self, upload: &mut dyn FnMut(&RgbaImage) -> Option<TextureId>) {
        for row in &mut self.rows {
            if let (Ping::Done(answer), None) = (&row.ping, &row.sprites)
                && let Ok(mut cache) = uploaded_sprites().lock()
            {
                row.sprites = Some(
                    answer
                        .status
                        .sprites
                        .iter()
                        .map(|id| {
                            *cache.entry(id.clone()).or_insert_with(|| {
                                load_sprite(&self.assets_root, id).and_then(|image| upload(&image))
                            })
                        })
                        .collect(),
                );
            }
            let Some(image) = row.pending_icon.take() else {
                continue;
            };
            if row_icon::is_favicon(&image) {
                row.icon = row_icon::upload_cached(&image, upload);
            }
        }
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.widgets.hover(x, y);
        self.list.on_mouse_move(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.list
            .cursor()
            .or_else(|| self.hovered_zone().map(|_| CursorKind::Pointer))
            .or(self.widgets.cursor())
            .unwrap_or_default()
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => self.press(id, ctx),
            _ => ScreenAction::None,
        }
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.list.blur();
        if let Some(id) = self.widgets.click(x, y) {
            return self.press(id, ctx);
        }
        let zone = self.hovered_zone();
        if let ListClick::Row { index, double } = self.list.on_click(x, y) {
            self.update_buttons();
            match zone {
                Some((_, Zone::Primary)) => {
                    ctx.click();
                    return self.join_server(index, ctx);
                }
                Some((_, Zone::MoveUp)) => {
                    self.swap(index, index - 1, ctx);
                    return ScreenAction::None;
                }
                Some((_, Zone::MoveDown)) => {
                    self.swap(index, index + 1, ctx);
                    return ScreenAction::None;
                }
                None => {}
            }
            if double {
                ctx.click();
                return self.join_selected(ctx);
            }
        }
        ScreenAction::None
    }

    fn on_resume(&mut self, ctx: &ScreenContext) {
        self.reload(ctx);
    }

    fn is_world_exit(&self, integrated: bool) -> bool {
        !integrated
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("multiplayer.title");
        ui.title(title, ((LAYOUT.header - LINE_HEIGHT) / 2.0).round());
        self.list
            .draw(ui, |ui, index, rect| self.draw_row(ui, index, rect));
        self.widgets.draw(ui);
        self.draw_tooltip(ui);
    }
}
