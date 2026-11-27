//! What the player does to the inventory and the hotbar, and the F3 key combinations.

use super::InWorld;
use melon_gui::text::font::Font;
use melon_gui::text::{StyledText, TextStyle, colors};
use melon_assets::i18n::Lang;
use melon_net::play::game::PlayerAction;
use melon_world::inventory::ClickType;

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
            .map(|&i| (i, &*self.inventory.slots()[i as usize]))
            .collect();
        self.joined.session.send_container_click(
            0,
            self.inventory.state_id(),
            &click,
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

    /// What World Options starts from; `None` outside a singleplayer world.
    pub fn world_options_state(
        &self,
    ) -> Option<melon_gui::screens::ingame::world_options::WorldOptionsState> {
        let server = self.integrated_server()?;
        let mode =
            |mode: pumpkin_util::GameMode| melon_world::game_mode::GameMode::from_id(mode as i32);
        Some(
            melon_gui::screens::ingame::world_options::WorldOptionsState {
                default_game_mode: server
                    .default_game_mode()
                    .map_or(self.game_mode.current, mode),
                personal_game_mode: server.personal_game_mode().map(mode),
                allow_commands: server.allow_commands(),
                hardcore: self.hardcore(),
                permission_level: self.permission_level,
                difficulty: self.level_settings.difficulty,
                difficulty_locked: self.level_settings.difficulty_locked,
                published: server.published(),
                guest_command_access: server.guest_command_access(),
                force_game_mode: server.force_game_mode(),
            },
        )
    }

    /// World Options' Apply Changes (`WorldOptionsScreen.applyChanges`).
    pub fn apply_world_options(
        &mut self,
        change: melon_gui::screens::ingame::world_options::WorldOptionsChange,
        lang: &melon_assets::i18n::Lang,
        font: &Font,
    ) {
        if let Some(difficulty) = change.difficulty {
            self.joined.session.send_change_difficulty(difficulty);
        }
        if change.lock_difficulty {
            self.joined.session.send_lock_difficulty(true);
        }
        let mode = |mode: melon_world::game_mode::GameMode| {
            pumpkin_util::GameMode::try_from(i32::from(mode.id())).ok()
        };
        let mut messages = Vec::new();
        if let Some(server) = self.joined.server.as_mut() {
            if let Some(allow) = change.allow_commands {
                server.set_allow_commands(allow);
            }
            if let Some(force) = change.force_game_mode {
                server.set_force_game_mode(force);
            }
            if let Some(game_mode) = change.default_game_mode.and_then(mode) {
                server.set_default_game_mode(game_mode);
            }
            if let Some(game_mode) = change.personal_game_mode.and_then(mode) {
                server.set_personal_game_mode(game_mode);
            }
            if let Some(access) = change.guest_command_access {
                server.set_guest_command_access(access);
            }
            // `changeMultiplayerScope`.
            if let Some(publish) = change.publish {
                if server.unpublish() {
                    messages.push(
                        lang.get("menu.multiplayerOptions.publish.stopped")
                            .to_owned(),
                    );
                }
                if let Some(publish) = publish {
                    messages.push(match server.publish(publish) {
                        Ok(()) => lang.format(
                            "menu.multiplayerOptions.publish.started.lan",
                            &[&publish.port.to_string()],
                        ),
                        Err(error) => {
                            melon_base::warn!("Not published: {error:#}");
                            lang.get("commands.publish.failed").to_owned()
                        }
                    });
                }
            }
        }
        for message in messages {
            self.show_client_message(StyledText::plain(&message), font);
        }
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
