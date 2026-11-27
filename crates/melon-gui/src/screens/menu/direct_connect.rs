//! Vanilla `DirectJoinServerScreen`.

use std::path::{Path, PathBuf};

use crate::join::Target;
use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::screens::menu::connecting::ConnectingScreen;
use crate::widgets::Ui;
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, WIDE_BUTTON_W, quarter, wide_column_left};
use crate::widgets::set::{KeyResult, WidgetSet};

/// Vanilla keeps the last address in `options.txt` (`lastMpIp`); Melon keeps it in its own
/// file.
const LAST_SERVER_FILE: &str = "melon_last_server.txt";
/// `0xA0A0A0`, the label above the field.
const LABEL_COLOR: [f32; 4] = [0.63, 0.63, 0.63, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Address,
    Join,
    Cancel,
}

pub struct DirectConnectScreen {
    widgets: WidgetSet<Id>,
}

/// Next to `options.txt`, in the game directory.
fn last_server_path(game_dir: &Path) -> PathBuf {
    game_dir.join(LAST_SERVER_FILE)
}

impl DirectConnectScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        let mut address = EditBox::new("", WIDE_BUTTON_W, BUTTON_H);
        if let Ok(last) = std::fs::read_to_string(last_server_path(ctx.game_dir)) {
            address.set_value(last.trim());
        }
        let mut widgets = WidgetSet::new()
            .with_field(Id::Address, address)
            .with_button(
                Id::Join,
                Button::sized("selectServer.select", WIDE_BUTTON_W, BUTTON_H),
            )
            .with_button(
                Id::Cancel,
                Button::sized("gui.cancel", WIDE_BUTTON_W, BUTTON_H),
            );
        widgets.focus(Id::Address);
        let mut screen = Self { widgets };
        screen.update_join_enabled();
        screen
    }

    fn address(&self) -> String {
        self.widgets.field(Id::Address).value().trim().to_owned()
    }

    /// `updateSelectButtonStatus`: only a plausible address can be joined.
    fn update_join_enabled(&mut self) {
        let ok = !self.address().is_empty();
        self.widgets.set_enabled(Id::Join, ok);
    }

    /// Remembers the address and connects to it.
    fn start_join(&self, ctx: &ScreenContext) -> ScreenAction {
        if !self.widgets.button(Id::Join).enabled {
            return ScreenAction::None;
        }
        let address = self.address();
        let _ = std::fs::write(last_server_path(ctx.game_dir), &address);
        ScreenAction::open(ConnectingScreen::new(Target::Server(address), ctx))
    }

    fn press(&self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        match id {
            Id::Join => self.start_join(ctx),
            Id::Cancel => ScreenAction::Back,
            Id::Address => ScreenAction::None,
        }
    }
}

impl Screen for DirectConnectScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let left = wide_column_left(width);
        self.widgets.set_position(Id::Address, left, 116.0);
        self.widgets
            .column(&[Id::Join, Id::Cancel], left, quarter(height) + 108.0, 24.0);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.click(x, y) {
            Some(id @ (Id::Join | Id::Cancel)) => {
                ctx.click();
                self.press(id, ctx)
            }
            _ => ScreenAction::None,
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => {
                ctx.click();
                self.press(id, ctx)
            }
            KeyResult::Edited(_) => {
                self.update_join_enabled();
                ScreenAction::None
            }
            KeyResult::Ignored if matches!(event, TextEvent::Enter) => self.start_join(ctx),
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("selectServer.direct");
        ui.title(title, 20.0);
        let label = ui.tr("addServer.enterIp");
        ui.text(label, wide_column_left(ui.width), 100.0, LABEL_COLOR);
        self.widgets.draw(ui);
    }
}
