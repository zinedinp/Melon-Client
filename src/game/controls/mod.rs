//! Input while in a world and not paused (vanilla `Minecraft.handleKeybinds`).

use crate::gui::text::font::Font;
use crate::i18n::Lang;
use crate::settings::options::Config;

mod chat;
mod keyboard;
mod mouse;

/// What input handling reads from the rest of the client.
pub struct WorldContext<'a> {
    pub lang: &'a Lang,
    pub config: &'a Config,
    pub font: &'a Font,
    /// The GUI screen's size.
    pub size: (f32, f32),
}
