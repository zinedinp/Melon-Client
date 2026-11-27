//! The open in-world overlay (inventory, creative inventory, chat, game mode switcher...).

use super::InWorld;
use crate::game::controls::WorldContext;
use melon_gui::overlay::chat::input::ChatInput;
use melon_gui::overlay::creative::CreativeOverlay;
use melon_gui::overlay::game_mode_switcher::GameModeSwitcher;
use melon_gui::overlay::inventory::InventoryOverlay;
use melon_gui::overlay::recipe_book::RecipeAction;
use melon_gui::overlay::{ChatExit, Overlay, OverlayAction, OverlayContext, OverlayKind};
use melon_gui::screen::ScreenAction;
use melon_assets::i18n::Lang;

/// The menus the server opens and what the overlays keep of them.
#[derive(Default)]
pub(super) struct Menus {
    /// Server-opened menu (`OPEN_SCREEN`).
    pub(super) open_container: Option<melon_world::inventory::OpenContainer>,
    /// Open the container overlay on the next tick, once `WorldContext` exists.
    pub(super) pending_container_overlay: bool,
    /// Close the container/inventory overlay on the next tick without telling
    /// the server.
    pub(super) pending_container_close: bool,
    /// The recipe the server asked the open menu to show (ghost items).
    pub(super) ghost_recipe: Option<(i32, melon_world::recipes::RecipeDisplay)>,
    /// The creative inventory's last tab, to reopen on.
    pub(super) creative_tab: usize,
}

impl InWorld {
    pub fn overlay_open(&self) -> bool {
        self.overlay.is_some()
    }

    /// Which overlay is open, if any.
    pub(super) fn overlay_kind(&self) -> Option<OverlayKind> {
        self.overlay.as_ref().map(|overlay| overlay.kind())
    }

    /// Runs `f` on the open overlay with what it may read and change of the world.
    pub(super) fn with_overlay<R>(
        &mut self,
        ui: &WorldContext,
        f: impl FnOnce(&mut dyn Overlay, &mut OverlayContext) -> R,
    ) -> Option<R> {
        let overlay = self.overlay.as_deref_mut()?;
        let players = &self.players;
        let names = || players.values().map(|p| p.name.clone()).collect();
        let mut ctx = OverlayContext {
            inventory: &mut self.inventory,
            container: self.menus.open_container.as_mut(),
            chat: &mut self.chat,
            recipes: &mut self.recipes,
            recipe_sets: &self.recipe_sets,
            hotbars: &self.hotbars,
            commands: &self.commands,
            registries: &self.registries,
            permission_level: self.permission_level,
            players: &names,
            creative_tab: &mut self.menus.creative_tab,
            game_mode: self.game_mode.current,
            experience_level: self.status.experience_level,
            lang: ui.lang,
            font: ui.font,
            chat_settings: &ui.config.chat,
            config: ui.config,
            size: ui.size,
            shift: self.keys.sneak,
            control: self.keys.sprint,
        };
        Some(f(overlay, &mut ctx))
    }

    /// Opens `overlay`.
    pub(super) fn open_overlay(&mut self, overlay: Box<dyn Overlay>, ui: &WorldContext) {
        let recenter = self.overlay.is_none();
        self.open_overlay_with(overlay, recenter, ui);
    }

    /// Opens `overlay`, the mouse at the centre if `recenter`.
    pub(super) fn open_overlay_with(
        &mut self,
        overlay: Box<dyn Overlay>,
        recenter: bool,
        ui: &WorldContext,
    ) {
        self.keys.clear_clicks();
        self.abort_destroy();
        self.overlay = Some(overlay);
        if recenter {
            self.keys.mouse = (ui.size.0 / 2.0, ui.size.1 / 2.0);
        }
        let (x, y) = self.keys.mouse;
        self.with_overlay(ui, |overlay, ctx| overlay.mouse_moved(x, y, false, ctx));
    }

    /// Closes whatever overlay is open, as if something else interrupted it.
    pub fn close_overlay(&mut self, ui: &WorldContext) {
        self.close_overlay_with(ChatExit::Interrupted, ui);
    }

    pub(super) fn close_overlay_with(&mut self, exit: ChatExit, ui: &WorldContext) {
        self.finish_overlay(exit, ui, true);
    }

    /// Drops the overlay. `notify_server` is `AbstractContainerScreen.onClose`
    pub(super) fn finish_overlay(
        &mut self,
        exit: ChatExit,
        ui: &WorldContext,
        notify_server: bool,
    ) {
        self.with_overlay(ui, |overlay, ctx| overlay.closed(exit, ctx));
        self.menus.ghost_recipe = None;
        if let Some(overlay) = self.overlay.take() {
            if notify_server {
                let id = match overlay.kind() {
                    OverlayKind::Inventory | OverlayKind::Creative => Some(0),
                    OverlayKind::Container => {
                        self.menus.open_container.as_ref().map(|menu| menu.id)
                    }
                    OverlayKind::Chat | OverlayKind::Switcher => None,
                };
                if let Some(id) = id {
                    self.joined.session.send_container_close(id);
                }
            }
            if overlay.kind() == OverlayKind::Container {
                self.menus.open_container = None;
            }
        }
    }

    /// Escape over an overlay.
    pub fn escape(&mut self, ui: &WorldContext) -> bool {
        let Some(action) = self.overlay.as_mut().map(|overlay| overlay.escape()) else {
            return false;
        };
        self.apply_overlay_action(action, ui);
        true
    }

    pub fn resize_overlay(&mut self, width: f32, height: f32) {
        if let Some(overlay) = &mut self.overlay {
            overlay.resize(width, height);
        }
    }

    pub fn refresh_lang(&mut self, lang: &Lang) {
        if let Some(overlay) = &mut self.overlay {
            overlay.refresh_lang(lang);
        }
        self.entities.rename_all();
        self.entities.resolve_names(&|name| {
            melon_gui::text::StyledText::from_component(name, lang).to_plain()
        });
    }

    /// `keyInventory`.
    pub(super) fn open_inventory(&mut self, ui: &WorldContext) {
        let mode = self.game_mode.current;
        if mode.is_spectator() {
            return;
        }
        let overlay: Box<dyn Overlay> = if mode.has_infinite_materials() {
            Box::new(self.creative_overlay(ui))
        } else {
            Box::new(InventoryOverlay::new(ui.lang, ui.size.0, ui.size.1))
        };
        self.open_overlay(overlay, ui);
    }

    pub(super) fn creative_overlay(&self, ui: &WorldContext) -> CreativeOverlay {
        let (width, height) = ui.size;
        CreativeOverlay::new(
            width,
            height,
            &self.hotbars,
            ui.lang,
            self.menus.creative_tab,
        )
    }

    /// `Gui.openChatScreen`: a restorable draft, or the prefix.
    pub(super) fn open_chat(&mut self, command: bool, ui: &WorldContext) {
        let draft = self
            .chat
            .draft
            .take()
            .filter(|draft| !command || draft.starts_with('/'));
        let history = self.chat.history.len();
        let screen = match draft {
            Some(draft) => ChatInput::new(&draft, true, history),
            None => ChatInput::new(if command { "/" } else { "" }, false, history),
        };
        self.open_overlay(Box::new(screen), ui);
    }

    /// F3+F4: opens the game mode switcher.
    pub(super) fn open_switcher(&mut self, ui: &WorldContext) {
        self.open_overlay(Box::new(GameModeSwitcher::new(self.game_mode)), ui);
    }

    /// Does what the overlay asked for. What the app has to do on top.
    pub(super) fn apply_overlay_action(
        &mut self,
        action: OverlayAction,
        ui: &WorldContext,
    ) -> ScreenAction {
        match action {
            OverlayAction::None => {}
            OverlayAction::Close(exit) => self.close_overlay_with(exit, ui),
            OverlayAction::Clicks { container, clicks } => {
                for click in clicks {
                    self.click_menu(container, click.slot_id, click.button, click.kind);
                }
            }
            OverlayAction::Sequence(actions) => return self.apply_overlay_actions(actions, ui),
            OverlayAction::MenuButton { container, button } => {
                self.joined.session.send_container_button(container, button);
            }
            OverlayAction::Recipe(action) => match action {
                RecipeAction::Place {
                    container,
                    recipe,
                    max,
                } => {
                    self.menus.ghost_recipe = None;
                    self.joined
                        .session
                        .send_place_recipe(container, recipe, max);
                }
                RecipeAction::Settings {
                    book,
                    open,
                    filtering,
                } => self
                    .joined
                    .session
                    .send_recipe_book_settings(book, open, filtering),
                RecipeAction::Seen(recipe) => self.joined.session.send_recipe_seen(recipe),
            },
            OverlayAction::Sound(event) => self.audio.push(melon_audio::SoundInstance::of(event)),
            OverlayAction::RenameItem(name) => self.joined.session.send_rename_item(&name),
            OverlayAction::SelectTrade(index) => self.joined.session.send_select_trade(index),
            OverlayAction::SetBeacon { primary, secondary } => {
                self.joined.session.send_set_beacon(primary, secondary);
            }
            OverlayAction::CreativeSlots(sets) => {
                for (slot, stack) in sets {
                    self.joined.session.send_creative_slot(slot, &stack);
                }
            }
            OverlayAction::Send(message) => {
                self.close_overlay_with(ChatExit::Done, ui);
                self.send_chat(&message);
            }
            OverlayAction::RequestSuggestions { transaction, text } => {
                self.joined
                    .session
                    .send_command_suggestion(transaction, &text);
            }
            OverlayAction::RunCommand(command) => {
                let command = command.trim_start_matches('/');
                self.joined.session.send_command(command);
            }
            OverlayAction::OpenUrl(url) => {
                return if ui.config.chat.links_prompt {
                    ScreenAction::open(
                        melon_gui::screens::menu::confirm_link::confirm_link_screen(url),
                    )
                } else {
                    ScreenAction::OpenUrl(url)
                };
            }
            OverlayAction::OpenFile(path) => return ScreenAction::OpenUrl(path),
            OverlayAction::Dialog(click) => {
                return ScreenAction::Dialog(melon_gui::screens::dialog::DialogRequest::Run {
                    click: Some(click),
                    then: melon_net::dialog::AfterAction::None,
                });
            }
            OverlayAction::Copy(text) => return ScreenAction::Copy(text),
            OverlayAction::SwitchGameMode(mode) => {
                self.close_overlay_with(ChatExit::Interrupted, ui);
                self.switch_game_mode(mode);
            }
        }
        ScreenAction::None
    }

    /// Several actions in order; the first one the app has to act on wins.
    pub(super) fn apply_overlay_actions(
        &mut self,
        actions: Vec<OverlayAction>,
        ui: &WorldContext,
    ) -> ScreenAction {
        for action in actions {
            match self.apply_overlay_action(action, ui) {
                ScreenAction::None => {}
                other => return other,
            }
        }
        ScreenAction::None
    }
}
