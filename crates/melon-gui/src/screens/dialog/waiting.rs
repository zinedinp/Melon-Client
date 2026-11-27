//! `WaitingForResponseScreen`: after a button with `wait_for_response`,
//! until the server shows the next dialog. Back appears after a second and
//! works after five.

use std::time::Instant;

use super::DialogLayer;
use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::widgets::button::Button;
use crate::widgets::{Ui, Widget};

/// `BUTTON_VISIBLE_AFTER` / `BUTTON_ACTIVE_AFTER`, in seconds.
const VISIBLE_AFTER: u64 = 1;
const ACTIVE_AFTER: u64 = 5;
/// `HeaderAndFooterLayout(this, 33, 0)`.
const HEADER_H: f32 = 33.0;

pub struct WaitingScreen {
    since: Instant,
    button: Button,
    width: f32,
}

impl WaitingScreen {
    pub fn new() -> Self {
        Self {
            since: Instant::now(),
            button: Button::sized("gui.back", 200.0, 20.0),
            width: 0.0,
        }
    }

    fn seconds(&self) -> u64 {
        self.since.elapsed().as_secs().min(ACTIVE_AFTER)
    }

    fn active(&self) -> bool {
        self.seconds() >= ACTIVE_AFTER
    }
}

impl Default for WaitingScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for WaitingScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.width = width;
        let x = ((width - self.button.w) / 2.0).floor();
        let y = HEADER_H + ((height - HEADER_H - self.button.h) / 2.0).floor();
        self.button.set_position(x, y);
    }

    fn tick(&mut self) {
        self.button.enabled = self.active();
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.button.update_hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.button.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if self.active() && self.button.clicked(x, y) {
            ctx.click();
            return ScreenAction::Back;
        }
        ScreenAction::None
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        if let TextEvent::Tab { .. } = event {
            self.button.focused = self.active();
        } else if matches!(event, TextEvent::Enter) && self.button.focused && self.active() {
            ctx.click();
            return ScreenAction::Back;
        }
        ScreenAction::None
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("gui.waitingForResponse.title");
        ui.title(title, ((HEADER_H - 9.0) / 2.0).floor());
        let seconds = self.seconds();
        if seconds < VISIBLE_AFTER {
            return;
        }
        if seconds >= ACTIVE_AFTER {
            self.button.draw(ui);
        } else {
            let label = ui.lang.format(
                "gui.waitingForResponse.button.inactive",
                &[&(ACTIVE_AFTER - seconds).to_string()],
            );
            self.button.draw_labeled(ui, &label);
        }
    }

    fn closes_on_escape(&self) -> bool {
        self.active()
    }

    /// `WaitingForResponseScreen.isPauseScreen`.
    fn is_pause_screen(&self) -> bool {
        false
    }

    fn dialog_layer(&self) -> Option<DialogLayer> {
        Some(DialogLayer::Waiting)
    }
}
