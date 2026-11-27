//! Vanilla `ConfirmLinkScreen` for an untrusted link from the chat.

use crate::math::argb;
use crate::screen::ScreenAction;
use crate::widgets::dialog::DialogScreen;

/// `ConfirmLinkScreen`'s three buttons are narrower than the default.
const BUTTON_W: f32 = 100.0;

pub fn confirm_link_screen(url: String) -> DialogScreen {
    let open = url.clone();
    let copy = url.clone();
    DialogScreen::new()
        .line("chat.link.confirm", [1.0; 4])
        .line(url, [1.0; 4])
        .line("chat.link.warning", argb(0xFFFF_CCCC))
        .button("gui.yes", BUTTON_W, move |_| {
            ScreenAction::OpenUrl(open.clone())
        })
        .button("chat.copy", BUTTON_W, move |_| {
            ScreenAction::Copy(copy.clone())
        })
        .button("gui.no", BUTTON_W, |_| ScreenAction::Back)
}
