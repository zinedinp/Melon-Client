//! Shared not-implemented stub: heading, a note, and Back.

use crate::screen::ScreenAction;
use crate::widgets::dialog::DialogScreen;
use crate::widgets::layout::WIDE_BUTTON_W;

pub fn placeholder_screen(title_key: &'static str) -> DialogScreen {
    DialogScreen::new()
        .line(title_key, [1.0; 4])
        .line("melon.not_implemented", [0.7, 0.7, 0.7, 1.0])
        .button("gui.back", WIDE_BUTTON_W, |_| ScreenAction::Back)
}
