//! Keys in a world: movement, hotbar, F3 combos, the game mode switcher.

use super::WorldContext;
use crate::cycle::Cycle;
use crate::game::minecraft_yaw;
use crate::game::{GAMEMASTER_LEVEL, InWorld};
use crate::gui::hud::item_slot;
use crate::gui::overlay::OverlayAction;
use crate::gui::screen::{ScreenAction, TextEvent};
use crate::gui::text::StyledText;
use crate::math::wrap_degrees;
use crate::settings::input::{Action, InputBind};
use crate::world::game_mode::GameMode;
use crate::world::inventory::HOTBAR_SLOTS;
use winit::keyboard::{Key, NamedKey};

use crate::gui::hud::debug_entries::Entry;

/// `VersionCommand.dumpVersion` for what Melon knows of its version: the
/// build time and the pack formats are not among it.
fn version_lines(lang: &crate::i18n::Lang) -> Vec<String> {
    let version = env!("CARGO_PKG_VERSION");
    let minecraft = version.split_once("+mc").map_or("?", |(_, mc)| mc);
    let protocol = crate::network::PROTOCOL_VERSION;
    vec![
        lang.format("commands.version.id", &[minecraft]),
        lang.format("commands.version.name", &[minecraft]),
        lang.format(
            "commands.version.data",
            &[&crate::world::hotbars::DATA_VERSION.to_string()],
        ),
        lang.format("commands.version.series", &["main"]),
        lang.format(
            "commands.version.protocol",
            &[&protocol.to_string(), &format!("0x{protocol:x}")],
        ),
        lang.get("commands.version.stable.yes").to_owned(),
    ]
}

impl InWorld {
    pub(super) fn can_switch_game_mode(&self) -> bool {
        self.permission_level >= GAMEMASTER_LEVEL
    }

    /// `GameModeSwitcherScreen.switchToHoveredGameMode`.
    pub(crate) fn switch_game_mode(&mut self, mode: GameMode) {
        if mode != self.game_mode.current && self.can_switch_game_mode() {
            self.joined.session.send_change_game_mode(mode);
        }
    }

    /// A key went down or up. `text` is what it typed, as text events.
    pub fn key(
        &mut self,
        key: &Key,
        bind: Option<&InputBind>,
        pressed: bool,
        text: &[TextEvent],
        ui: &WorldContext,
    ) -> ScreenAction {
        if let Some(bind) = bind {
            self.keys.apply_bind(bind, pressed, &ui.config.input);
        }

        if let Key::Named(NamedKey::F3) = key {
            if pressed {
                if !self.keys.f3_held {
                    self.keys.f3_used = false;
                }
                self.keys.f3_held = true;
            } else {
                self.keys.f3_held = false;
                let mode = self
                    .overlay
                    .as_ref()
                    .and_then(|overlay| overlay.released_debug_key());
                if let Some(mode) = mode {
                    return self.apply_overlay_action(OverlayAction::SwitchGameMode(mode), ui);
                } else if !self.keys.f3_used {
                    self.debug.toggle();
                    self.save_debug_settings();
                }
            }
            return ScreenAction::None;
        }
        if !pressed {
            return ScreenAction::None;
        }
        if self.keys.f3_held
            && let Some(action) = self.debug_key(key, ui)
        {
            self.keys.f3_used = true;
            return action;
        }

        if let Some(actions) = self.with_overlay(ui, |overlay, ctx| overlay.key(bind, text, ctx)) {
            return self.apply_overlay_actions(actions, ui);
        }

        let binds = &ui.config.input.binds;
        let mut quick_actions = false;
        if let Some(bind) = bind {
            for action in binds.actions_for(bind) {
                if action == Action::QuickActions {
                    quick_actions = true;
                } else {
                    self.play_action(action, ui);
                }
            }
        }
        if quick_actions {
            // `Minecraft.handleKeybinds`: `keyQuickActions` shows the dialog.
            return ScreenAction::Dialog(crate::gui::screens::dialog::DialogRequest::QuickActions);
        }
        ScreenAction::None
    }

    /// A matching bind pressed while playing, no overlay.
    fn play_action(&mut self, action: Action, ui: &WorldContext) {
        let mode = self.game_mode.current;
        match action {
            Action::TogglePerspective => self.camera_mode = self.camera_mode.next(),
            // `Gui.handleKeybinds`: `keyToggleGui` toggles `Hud.isHidden`.
            Action::ToggleGui => self.hud_hidden = !self.hud_hidden,
            Action::Inventory if self.rides_with_inventory() => self.open_mount_inventory(),
            Action::Inventory => self.open_inventory(ui),
            Action::Chat => self.open_chat(false, ui),
            Action::Command => self.open_chat(true, ui),
            Action::Drop if !mode.is_spectator() => {
                let whole = self.keys.sprint;
                self.drop_selected(whole);
            }
            Action::SwapOffhand if !mode.is_spectator() => self.swap_offhand(),
            other => {
                if let Some(n) = other.hotbar_index() {
                    self.hotbar_key(n, ui);
                }
            }
        }
    }

    /// `KeyboardHandler.handleDebugKeys` for what Melon has: the action,
    pub(super) fn debug_key(&mut self, key: &Key, ui: &WorldContext) -> Option<ScreenAction> {
        let feedback =
            |this: &mut Self, key: &'static str| this.debug_feedback(ui.lang, ui.font, key);
        let on_off = |on: bool, on_key: &'static str, off_key: &'static str| {
            if on { on_key } else { off_key }
        };
        let mut action = ScreenAction::None;
        match key {
            Key::Named(NamedKey::F4) => {
                if let Some(overlay) = &mut self.overlay {
                    overlay.cycle_game_mode();
                } else if self.can_switch_game_mode() {
                    self.open_switcher(ui);
                } else {
                    feedback(self, "debug.gamemodes.error");
                }
            }
            // `keyDebugDebugOptions`: the screen choosing the entries.
            Key::Named(NamedKey::F6) => {
                action = ScreenAction::open(
                    crate::gui::screens::options::debug::debug_options_screen(self.debug.shared()),
                );
            }
            Key::Character(c) => match c.to_ascii_lowercase().as_str() {
                "a" => {
                    self.level.remesh_all();
                    feedback(self, "debug.reload_chunks.message");
                }
                "b" => {
                    let on = self
                        .debug
                        .entries_mut()
                        .toggle_status(Entry::EntityHitboxes);
                    let key = on_off(on, "debug.show_hitboxes.on", "debug.show_hitboxes.off");
                    feedback(self, key);
                }
                "c" => {
                    let feet = self.player.pos();
                    action = ScreenAction::Clipboard(format!(
                        "/execute in {} run tp @s {:.2} {:.2} {:.2} {:.2} {:.2}",
                        self.level_info.dimension,
                        feet.x,
                        feet.y,
                        feet.z,
                        wrap_degrees(minecraft_yaw(self.player.yaw())),
                        wrap_degrees(self.player.pitch()),
                    ));
                    feedback(self, "debug.copy_location.message");
                }
                "d" => self.chat.clear_messages(),
                "g" => {
                    let on = self.debug.entries_mut().toggle_status(Entry::ChunkBorders);
                    let key = on_off(
                        on,
                        "debug.chunk_boundaries.on",
                        "debug.chunk_boundaries.off",
                    );
                    feedback(self, key);
                }
                // `keyDebugFocusPause`.
                "p" => {
                    let on = !ui.config.pause_on_lost_focus;
                    action = ScreenAction::ChangeConfig(Box::new(move |config| {
                        config.pause_on_lost_focus = on;
                    }));
                    feedback(
                        self,
                        on_off(on, "debug.pause_focus.on", "debug.pause_focus.off"),
                    );
                }
                // `keyDebugReloadResourcePacks`.
                "t" => {
                    feedback(self, "debug.reload_resourcepacks.message");
                    action = ScreenAction::ReloadResources;
                }
                // `keyDebugDumpVersion`: `VersionCommand.dumpVersion`.
                "v" => {
                    feedback(self, "debug.version.header");
                    for line in version_lines(ui.lang) {
                        self.show_client_message(StyledText::plain(&line), ui.font);
                    }
                }
                "h" => {
                    let on = !item_slot::advanced_tooltips();
                    item_slot::set_advanced_tooltips(on);
                    feedback(
                        self,
                        on_off(
                            on,
                            "debug.advanced_tooltips.on",
                            "debug.advanced_tooltips.off",
                        ),
                    );
                }
                "i" => {
                    if let Some(hit) = self.hit_block() {
                        let [x, y, z] = hit.pos;
                        let state = self.level.block_state(x, y, z).unwrap_or(0);
                        action = ScreenAction::Clipboard(
                            super::super::debug::info::setblock_command(hit.pos, state),
                        );
                        feedback(self, "debug.inspect.client.block");
                    } else if let Some(entity) =
                        self.hit_entity().and_then(|hit| self.entities.get(hit.id))
                    {
                        let at = entity.tick_pos();
                        action = ScreenAction::Clipboard(format!(
                            "/summon minecraft:{} {:.2} {:.2} {:.2}",
                            entity.ty.resource_name, at.x, at.y, at.z
                        ));
                        feedback(self, "debug.inspect.client.entity");
                    }
                }
                "n" if self.overlay.is_none() => {
                    if !self.can_switch_game_mode() {
                        feedback(self, "debug.creative_spectator.error");
                    } else if self.game_mode.current.is_spectator() {
                        let back = self.game_mode.previous.unwrap_or(GameMode::Creative);
                        self.joined.session.send_change_game_mode(back);
                    } else {
                        self.joined
                            .session
                            .send_change_game_mode(GameMode::Spectator);
                    }
                }
                _ => return None,
            },
            _ => return None,
        }
        self.save_debug_settings();
        Some(action)
    }

    /// Keeps what F3 and its combos switched on for the next run.
    fn save_debug_settings(&self) {
        self.debug_view.save(self.debug.visible());
    }

    /// A number key while playing: the hotbar slot, or in creative with C
    pub(super) fn hotbar_key(&mut self, index: u8, ui: &WorldContext) {
        let mode = self.game_mode.current;
        if mode.is_spectator() {
            let targets = self.teleport_targets();
            let action = self.spectator_menu.select_slot(usize::from(index), targets);
            self.spectator_action(action);
            return;
        }
        let (load, save) = (self.keys.load_hotbar, self.keys.save_hotbar);
        if mode == GameMode::Creative && (load || save) {
            let index = usize::from(index);
            if load {
                let items = self.hotbars.hotbars[index].clone();
                for (i, stack) in items.into_iter().enumerate() {
                    let slot = HOTBAR_SLOTS.start + i;
                    self.inventory.slots_mut()[slot] = stack.clone();
                    self.joined.session.send_creative_slot(slot as i16, &stack);
                }
            } else {
                let items = std::array::from_fn(|i| self.inventory.hotbar(i).clone());
                self.hotbars.store(index, items);
                let message = ui
                    .lang
                    .format("inventory.hotbarSaved", &["X", &(index + 1).to_string()]);
                self.hud.set_overlay_message(StyledText::plain(&message));
            }
            return;
        }
        self.select_hotbar(index);
    }
}
