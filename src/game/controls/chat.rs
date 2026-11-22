//! Sending what was typed in the chat.

use crate::game::InWorld;

impl InWorld {
    /// `ChatScreen.handleChatInput`.
    pub fn send_chat(&mut self, message: &str) {
        if message.is_empty() {
            return;
        }
        match message.strip_prefix('/') {
            Some(command) => self.joined.session.send_command(command),
            None => {
                let offset = self.chat.take_offset();
                self.joined.session.send_chat(message, offset);
            }
        }
    }
}
