//! What the player does to the inventory and the hotbar, and the F3 key combinations.

use super::InWorld;
use crate::gui::text::font::Font;
use crate::gui::text::{StyledText, TextStyle, colors};
use crate::i18n::Lang;
use crate::network::play::game::PlayerAction;
use crate::world::inventory::ClickType;

impl InWorld {
    /// Selects hotbar slot `slot` (`Inventory.setSelectedSlot`) and tells the server.
    pub fn select_hotbar(&mut self, slot: u8) {
        if slot < 9 && slot != self.inventory.selected() {
            self.inventory.select(slot);
            self.joined.session.send_carried_item(slot);
        }
    }

    /// The mouse wheel while playing: scrolling down moves right.
    pub fn scroll_hotbar(&mut self, lines: f32) {
        if lines == 0.0 {
            return;
        }
        if self.game_mode.current.is_spectator() {
            if self.spectator_menu.is_open() {
                self.spectator_menu.scroll(if lines > 0.0 { -1 } else { 1 });
                return;
            }
            self.player.scroll_flying_speed(lines);
            return;
        }
        let step = if lines > 0.0 { 8 } else { 1 };
        self.select_hotbar((self.inventory.selected() + step) % 9);
    }

    /// A click in the survival inventory: predicted locally, then sent.
    pub fn click_inventory(&mut self, slot: i16, button: i8, kind: ClickType) {
        let click = self.inventory.click(slot, button, kind, false);
        let changed: Vec<_> = click
            .changed
            .iter()
            .map(|&i| (i, &self.inventory.slots()[i as usize]))
            .collect();
        self.joined.session.send_container_click(
            0,
            self.inventory.state_id(),
            click.slot,
            click.button,
            click.kind,
            &changed,
            self.inventory.carried(),
        );
    }

    /// `Q` (with Control the whole stack): predicted, then sent.
    pub fn drop_selected(&mut self, whole_stack: bool) {
        if self.inventory.drop_selected(whole_stack) {
            self.joined.session.send_drop(whole_stack);
        }
    }

    /// `F`: swaps the selected hotbar stack with the offhand, predicted.
    pub fn swap_offhand(&mut self) {
        self.inventory.swap_offhand();
        self.joined
            .session
            .send_player_action(PlayerAction::SwapItemWithOffhand);
    }

    /// `KeyboardHandler.debugFeedbackTranslated`: a yellow bold "[Debug]:"
    pub fn debug_feedback(&mut self, lang: &Lang, font: &Font, key: &'static str) {
        self.debug_line(lang, font, lang.get(key));
    }

    /// `Minecraft.showDebugChat`: a client message in the chat, as it is.
    pub fn show_client_message(&mut self, text: StyledText, font: &Font) {
        self.chat.add_client_message(text, font);
    }

    /// [`Self::debug_feedback`] for a message that is already text.
    pub fn debug_line(&mut self, lang: &Lang, font: &Font, message: &str) {
        let mut text = StyledText::default();
        text.push(
            lang.get("debug.prefix"),
            &TextStyle {
                color: Some(colors::YELLOW),
                bold: true,
                ..TextStyle::default()
            },
        );
        text.push(" ", &TextStyle::default());
        text.push(message, &TextStyle::default());
        self.chat.add_client_message(text, font);
    }
}
