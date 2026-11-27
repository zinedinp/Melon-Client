//! Vanilla `ConnectScreen`.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

use anyhow::Result;

use crate::join::{self, Joined, Target};
use crate::network::{ClientInfo, Identity};
use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::widgets::button::Button;
use crate::widgets::dialog::DialogScreen;
use crate::widgets::layout::{BUTTON_H, WIDE_BUTTON_W, quarter, wide_column_left};
use crate::widgets::{Ui, Widget};

/// `DisconnectedScreen`'s title colour (`-6250336`, `0xA0A0A0`).
const FAILURE_TITLE_COLOR: [f32; 4] = [0.63, 0.63, 0.63, 1.0];

/// `DisconnectedScreen`.
pub fn disconnected_screen(
    title_key: &'static str,
    reason: String,
    button_key: &'static str,
) -> DialogScreen {
    DialogScreen::new()
        .line(title_key, FAILURE_TITLE_COLOR)
        .line(reason, [1.0; 4])
        .button(button_key, WIDE_BUTTON_W, |_| ScreenAction::Back)
}

pub struct ConnectingScreen {
    status_key: &'static str,
    result: Receiver<Result<Joined>>,
    cancel: Button,
}

impl ConnectingScreen {
    pub fn new(target: Target, ctx: &ScreenContext) -> Self {
        // Refreshed on the join's thread; the name and UUID stay the same.
        let identity = Identity::stored(ctx.account_dir);
        let account_dir = ctx.account_dir.to_path_buf();
        if let Target::Server(address) = &target {
            crate::instances::save_last_server(ctx.game_dir, address);
        }
        let saved = match &target {
            Target::Server(address) => crate::settings::server_list::find(ctx.game_dir, address),
            Target::World { .. } => None,
        };
        melon_assets::assets::packs::server::begin(
            ctx.game_dir,
            saved.as_ref().map(|s| s.ip.clone()),
            melon_assets::assets::packs::server::Policy::from_accept(
                saved.as_ref().and_then(|s| s.accept_textures),
            ),
            melon_assets::assets::packs::server::User {
                name: identity.username().to_owned(),
                uuid: identity.uuid(),
            },
        );
        let client = ClientInfo::from_config(ctx.config);
        let simulation_distance = ctx.config.simulation_distance;
        let status_key = target.status_key();
        let (tx, result) = channel();
        thread::Builder::new()
            .name("join".to_owned())
            .spawn(move || {
                let identity = Identity::saved(&account_dir);
                let _ = tx.send(join::join(target, &identity, &client, simulation_distance));
            })
            .expect("spawning the join thread should not fail");
        Self {
            status_key,
            result,
            cancel: Button::sized("gui.cancel", WIDE_BUTTON_W, BUTTON_H),
        }
    }

    fn failed(reason: String) -> ScreenAction {
        crate::warn!("Connection failed: {reason}");
        ScreenAction::Replace(Box::new(disconnected_screen(
            "connect.failed",
            reason,
            "gui.back",
        )))
    }
}

impl Screen for ConnectingScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.cancel
            .set_position(wide_column_left(width), quarter(height) + 132.0);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.cancel.update_hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        Widget::cursor(&self.cancel).unwrap_or_default()
    }

    fn poll(&mut self) -> ScreenAction {
        match self.result.try_recv() {
            Ok(Ok(joined)) => ScreenAction::EnterWorld(Box::new(joined)),
            Ok(Err(error)) => {
                melon_assets::assets::packs::server::end();
                Self::failed(format!("{error:#}"))
            }
            Err(TryRecvError::Disconnected) => {
                melon_assets::assets::packs::server::end();
                Self::failed("the join thread stopped".to_owned())
            }
            Err(TryRecvError::Empty) => ScreenAction::None,
        }
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if self.cancel.clicked(x, y) {
            ctx.click();
            melon_assets::assets::packs::server::end();
            ScreenAction::Back
        } else {
            ScreenAction::None
        }
    }

    /// Escape cancels like the button: the server's packs are dropped.
    fn on_escape(&mut self, _ctx: &ScreenContext) -> Option<ScreenAction> {
        melon_assets::assets::packs::server::end();
        None
    }

    /// Tab focuses Cancel, Enter presses it.
    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match event {
            TextEvent::Tab { .. } => {
                self.cancel.focused = true;
                ScreenAction::None
            }
            TextEvent::Enter if self.cancel.focused => {
                ctx.click();
                ScreenAction::Back
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let status = ui.tr(self.status_key);
        ui.title(status, (ui.height / 2.0 - 50.0).round());
        self.cancel.draw(ui);
    }
}
