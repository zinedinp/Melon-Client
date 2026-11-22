//! Chat, titles and the messages the game itself sends.

use crate::component::Component;
use crate::game::InWorld;
use crate::gui::text::{StyledText, font::Font};
use crate::i18n::Lang;
use crate::network::play::chat::ChatEvent;

impl InWorld {
    pub(in crate::game) fn receive_chat(&mut self, event: ChatEvent, lang: &Lang, font: &Font) {
        match event {
            ChatEvent::Suggestions(suggestions) => {
                if let Some(overlay) = &mut self.overlay {
                    overlay.receive_suggestions(suggestions, lang);
                }
            }
            ChatEvent::Title(c) => self.hud.set_title(StyledText::from_component(&c, lang)),
            ChatEvent::Subtitle(c) => {
                self.hud.set_subtitle(StyledText::from_component(&c, lang));
            }
            ChatEvent::TitleTimes {
                fade_in,
                stay,
                fade_out,
            } => self.hud.set_title_times(fade_in, stay, fade_out),
            ChatEvent::ClearTitles { reset } => self.hud.clear_titles(reset),
            event => {
                if let Some(message) = self.chat.receive(event, lang, font) {
                    self.hud.set_overlay_message(message);
                }
            }
        }
    }

    /// `NO_RESPAWN_BLOCK_AVAILABLE`: `LocalPlayer.sendSystemMessage`, a
    /// system message in the chat.
    pub(super) fn no_respawn_block(&mut self, lang: &Lang, font: &Font) {
        let content = Component::translatable("block.minecraft.spawn.not_valid", Vec::new());
        self.receive_chat(
            ChatEvent::System {
                content,
                overlay: false,
            },
            lang,
            font,
        );
    }
}
