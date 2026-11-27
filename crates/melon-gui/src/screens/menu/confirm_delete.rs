//! Vanilla's delete `ConfirmScreen`s, for a world and for a saved server.

use crate::gui::screen::{ScreenAction, ScreenContext};
use crate::gui::widgets::dialog::DialogScreen;
use crate::gui::widgets::layout::BUTTON_W;
use crate::settings::server_list;
use crate::singleplayer;

/// The question, in red as Melon has always shown it.
const QUESTION_COLOR: [f32; 4] = [1.0, 0.5, 0.5, 1.0];
const WARNING_COLOR: [f32; 4] = [0.85, 0.85, 0.85, 1.0];

/// Deletes a world's whole folder.
pub fn confirm_delete_world(folder: String, world_name: &str, ctx: &ScreenContext) -> DialogScreen {
    let warning = ctx.lang.format("selectWorld.deleteWarning", &[world_name]);
    DialogScreen::new()
        .line("selectWorld.deleteQuestion", QUESTION_COLOR)
        .line(warning, WARNING_COLOR)
        .button("selectWorld.deleteButton", BUTTON_W, move |ctx| {
            if let Err(e) = singleplayer::delete_world(ctx.game_dir, &folder) {
                crate::warn!("World not deleted ({folder}): {e:#}");
            }
            ScreenAction::Back
        })
        .button("gui.cancel", BUTTON_W, |_| ScreenAction::Back)
}

/// Removes saved server `index` from the multiplayer list.
pub fn confirm_delete_server(index: usize, server_name: &str, ctx: &ScreenContext) -> DialogScreen {
    let warning = ctx
        .lang
        .format("selectServer.deleteWarning", &[server_name]);
    DialogScreen::new()
        .line("selectServer.deleteQuestion", QUESTION_COLOR)
        .line(warning, WARNING_COLOR)
        .button("selectServer.deleteButton", BUTTON_W, move |ctx| {
            server_list::remove(ctx.game_dir, index);
            ScreenAction::Back
        })
        .button("gui.cancel", BUTTON_W, |_| ScreenAction::Back)
}
