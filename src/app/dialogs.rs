//! Server dialogs between the connection and the screens (vanilla
//! `ClientCommonPacketListenerImpl.showDialog`/`clearDialog` and
//! `DialogScreen.runAction`): what the server asks for each frame, and what
//! a dialog's buttons ask of the game.

use std::sync::Arc;

use crate::component::ClickEvent;
use crate::dialog::connection::{self, Request};
use crate::dialog::{AfterAction, Dialog, DialogClick};
use crate::game::InWorld;
use crate::gui::screens::dialog::{DialogLayer, DialogRequest, DialogScreen, WaitingScreen};

use super::RunningState;

impl RunningState {
    /// Once a frame: the dialogs the server showed or cleared.
    pub(super) fn poll_dialogs(&mut self) {
        while let Some(request) = connection::poll() {
            match request {
                Request::Show(dialog) => self.show_dialog(dialog),
                Request::Clear => self.clear_dialog(),
            }
        }
    }

    /// What a dialog, the pause menu or a key asked for.
    pub(super) fn apply_dialog(&mut self, request: DialogRequest) {
        match request {
            DialogRequest::Run { click, then } => self.run_dialog_action(click, then),
            DialogRequest::Show(dialog) => self.show_dialog(dialog),
            DialogRequest::QuickActions => {
                if let Some(dialog) = connection::quick_actions() {
                    self.show_dialog(dialog);
                }
            }
            DialogRequest::Disconnect => self.reject_custom_screen(),
        }
    }

    /// `showDialog`: over the screens, in place of a dialog or the wait for
    /// one, or behind the warning about custom screens while it is open.
    fn show_dialog(&mut self, dialog: Arc<Dialog>) {
        let local_server = self.world.as_ref().is_some_and(InWorld::is_integrated);
        let ctx = self.env.ctx(self.scale());
        let screen = Box::new(DialogScreen::new(dialog, local_server, &ctx));
        match self.screens.top().and_then(|s| s.dialog_layer()) {
            Some(DialogLayer::Warning) => {
                let replace = self
                    .screens
                    .below_top()
                    .is_some_and(|s| s.dialog_layer() == Some(DialogLayer::Dialog));
                self.screens.set_below_top(screen, replace, &ctx);
            }
            Some(DialogLayer::Dialog | DialogLayer::Waiting) => {
                self.screens.replace_top(screen, &ctx);
            }
            None => self.screens.push(screen, &ctx),
        }
    }

    /// `clearDialog`: the dialog closes, or leaves from behind the warning.
    fn clear_dialog(&mut self) {
        match self.screens.top().and_then(|s| s.dialog_layer()) {
            Some(DialogLayer::Warning) => {
                if self
                    .screens
                    .below_top()
                    .is_some_and(|s| s.dialog_layer() == Some(DialogLayer::Dialog))
                {
                    self.screens.remove_below_top();
                }
            }
            Some(DialogLayer::Dialog) => self.close_top_screen(),
            _ => {}
        }
    }

    /// `DialogScreen.runAction`: the dialog goes as `then` says, then the
    /// click runs where that left the screens.
    fn run_dialog_action(&mut self, click: Option<DialogClick>, then: AfterAction) {
        // `clickUrlAction` with links off does nothing, the dialog included.
        if let Some(DialogClick::Event(ClickEvent::OpenUrl { .. })) = &click
            && !self.env.config.chat.links
        {
            return;
        }
        match then {
            AfterAction::None => {}
            AfterAction::Close => self.close_top_screen(),
            AfterAction::WaitForResponse => {
                let ctx = self.env.ctx(self.scale());
                self.screens
                    .replace_top(Box::new(WaitingScreen::new()), &ctx);
            }
        }
        match click {
            None => {}
            Some(DialogClick::Show(dialog)) => self.show_dialog(dialog),
            Some(DialogClick::Custom { id, payload }) => {
                connection::send_custom(&id, payload.as_ref());
            }
            Some(DialogClick::Event(event)) => self.run_click_event(event),
        }
    }

    /// `Screen.defaultHandleGameClickEvent` for a dialog's click.
    fn run_click_event(&mut self, event: ClickEvent) {
        match event {
            ClickEvent::RunCommand { command } => match &self.world {
                Some(world) => world.run_command(&command),
                // `ClientConfigurationPacketListenerImpl`'s dialog access.
                None => crate::warn!("No commands in the configuration phase: {command}"),
            },
            ClickEvent::ShowDialog { dialog } => {
                match Dialog::reference(&dialog).and_then(|r| connection::resolve(&r)) {
                    Some(dialog) => self.show_dialog(dialog),
                    None => crate::warn!("Dialog not found: {dialog}"),
                }
            }
            ClickEvent::OpenUrl { url } => {
                if !crate::dialog::links::is_trusted_url(&url) {
                    return;
                }
                if self.env.config.chat.links_prompt {
                    let ctx = self.env.ctx(self.scale());
                    let screen = crate::gui::screens::menu::confirm_link::confirm_link_screen(url);
                    self.screens.push(Box::new(screen), &ctx);
                } else {
                    crate::platform::open_url(&url);
                }
            }
            ClickEvent::CopyToClipboard { value } => crate::platform::copy_to_clipboard(&value),
            ClickEvent::Custom { id, payload } => {
                let payload = payload.as_ref().map(crate::dialog::action::json_to_nbt);
                connection::send_custom(&id, payload.as_ref());
            }
            // `SuggestCommand` goes into the screen's text, which a dialog
            // has none of; pages and files are not a dialog's.
            ClickEvent::SuggestCommand { .. }
            | ClickEvent::ChangePage { .. }
            | ClickEvent::OpenFile { .. } => {}
        }
    }

    /// The warning's Disconnect (`DialogConnectionAccess.disconnect`): the
    /// server is left with "Custom screen rejected".
    fn reject_custom_screen(&mut self) {
        let reason = self
            .env
            .lang
            .get("menu.custom_screen_info.disconnect")
            .to_owned();
        if self.world.is_some() {
            self.leave_world(Some(reason));
            return;
        }
        // Still configuring: the join fails behind the dialogs.
        crate::assets::packs::server::close_connection();
        connection::end();
        while self
            .screens
            .top()
            .is_some_and(|s| s.dialog_layer().is_some())
        {
            self.close_top_screen();
        }
    }
}
