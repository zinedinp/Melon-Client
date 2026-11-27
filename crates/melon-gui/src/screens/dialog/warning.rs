//! `DialogScreen.WarningScreen`: what custom screens are, from the button by
//! a dialog's title, with a way to leave a server whose screen won't let go.

use super::{DialogLayer, DialogRequest};
use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::widgets::Ui;
use crate::widgets::dialog::DialogScreen as ConfirmScreen;

/// `ConfirmScreen`'s buttons.
const BUTTON_W: f32 = 150.0;

pub struct WarningScreen {
    confirm: ConfirmScreen,
}

impl WarningScreen {
    pub fn new(local_server: bool, _ctx: &ScreenContext) -> Self {
        // `CommonComponents.disconnectButtonLabel`.
        let disconnect = if local_server {
            "menu.returnToMenu"
        } else {
            "menu.disconnect"
        };
        Self {
            confirm: ConfirmScreen::new()
                .line("menu.custom_screen_info.title", [1.0; 4])
                .line("menu.custom_screen_info.contents", [1.0; 4])
                .button(disconnect, BUTTON_W, |_| {
                    ScreenAction::Dialog(DialogRequest::Disconnect)
                })
                .button("gui.back", BUTTON_W, |_| ScreenAction::Back),
        }
    }
}

impl Screen for WarningScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        self.confirm.resize(ctx);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.confirm.on_mouse_move(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.confirm.cursor()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.confirm.on_click(x, y, ctx)
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        self.confirm.on_text_event(event, ctx)
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        self.confirm.draw(ui, ctx);
    }

    fn dialog_layer(&self) -> Option<DialogLayer> {
        Some(DialogLayer::Warning)
    }
}
