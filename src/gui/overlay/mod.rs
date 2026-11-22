//! What opens over a world and keeps it running.

pub mod chat;
pub mod container;
pub mod container_input;
pub mod creative;
pub mod game_mode_switcher;
pub mod inventory;
pub mod menu_layout;
pub mod menus;
pub mod recipe_book;
pub mod spectator_menu;

use pumpkin_data::item_stack::ItemStack;

use crate::gui::hud::item_slot::ItemPainter;
use crate::gui::screen::TextEvent;
use crate::gui::text::font::Font;
use crate::i18n::Lang;
use crate::network::play::chat::Suggestions;
use crate::network::play::chat::commands::CommandTree;
use crate::renderer::quad::DrawList;
use crate::settings::input::InputBind;
use crate::settings::options::{ChatSettings, Config};
use crate::world::game_mode::GameMode;
use crate::world::hotbars::Hotbars;
use crate::world::inventory::{Inventory, OpenContainer};
use chat::Chat;
use container::ContainerTextures;
use container_input::{MouseButtonKind, SlotClick};
use creative::CreativeTextures;
use game_mode_switcher::SwitcherTextures;
use inventory::InventoryTextures;

/// Which overlay it is, for what the world treats differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    Inventory,
    Creative,
    Chat,
    Switcher,
    /// A server-opened menu (`OPEN_SCREEN`).
    Container,
}

/// `ChatScreen.ExitReason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatExit {
    /// Escape: kept only with "Save Chat Drafts" on.
    Intentional,
    /// Interrupted by something else (death): kept.
    Interrupted,
    /// Sent: gone.
    Done,
}

/// What an overlay asks the world to do.
pub enum OverlayAction {
    None,
    /// A chat link's `show_dialog` or `custom`: for the app's dialogs.
    Dialog(crate::dialog::DialogClick),
    /// Close the overlay.
    Close(ChatExit),
    /// Inventory clicks.
    Clicks {
        container: i32,
        clicks: Vec<SlotClick>,
    },
    /// Several actions, in order.
    Sequence(Vec<OverlayAction>),
    /// A button of the open menu (a stonecutter recipe, a loom pattern...).
    MenuButton {
        container: i32,
        button: i32,
    },
    /// The anvil's name field changed.
    RenameItem(String),
    /// The villager's offer to trade, by index.
    SelectTrade(i32),
    /// The beacon's chosen effects, by `MobEffect` registry id.
    SetBeacon {
        primary: Option<i32>,
        secondary: Option<i32>,
    },
    /// Creative inventory slots the client set (`ServerboundSetCreativeModeSlotPacket`;
    CreativeSlots(Vec<(i16, ItemStack)>),
    /// Send this chat message or `/command`, and close.
    Send(String),
    /// Ask the server for completions of this text.
    RequestSuggestions {
        transaction: i32,
        text: String,
    },
    /// Run a command from a clicked text.
    RunCommand(String),
    /// A link was clicked.
    OpenUrl(String),
    /// A file of the client's own was clicked: opened at once, as
    /// `ClickEvent.OpenFile` is (`Util.getPlatform().openFile`).
    OpenFile(String),
    Copy(String),
    /// Close and switch to this game mode.
    SwitchGameMode(GameMode),
    /// A sound of the GUI (`SimpleSoundInstance.forUI`), by event name.
    Sound(&'static str),
    /// The recipe book: place a recipe, a setting changed, a recipe seen.
    Recipe(recipe_book::RecipeAction),
}

/// The sound of any button (`AbstractButton.playDownSound`).
pub const CLICK: &str = "ui.button.click";

impl OverlayAction {
    /// This action after the sound `event`.
    pub fn with_sound(self, event: &'static str) -> Self {
        Self::Sequence(vec![Self::Sound(event), self])
    }
}

/// What an overlay may read and change of the world it covers.
pub struct OverlayContext<'a> {
    pub inventory: &'a mut Inventory,
    pub container: Option<&'a mut OpenContainer>,
    pub chat: &'a mut Chat,
    /// The recipe book the server filled.
    pub recipes: &'a mut crate::world::recipes::RecipeBook,
    /// What the server's machines accept and the stonecutter cuts.
    pub recipe_sets: &'a crate::world::recipes::RecipeSets,
    pub hotbars: &'a Hotbars,
    pub commands: &'a CommandTree,
    /// The registries' ids, for completing command arguments.
    pub registries: &'a crate::network::play::chat::command_suggestions::Registries,
    /// `EntityEvent.PERMISSION_LEVEL_*`: whether `@` selectors complete.
    pub permission_level: u8,
    /// The listed players' names, for completing chat; only built when asked.
    pub players: &'a dyn Fn() -> Vec<String>,
    /// `CreativeModeInventoryScreen.selectedTab`, static in vanilla.
    pub creative_tab: &'a mut usize,
    pub game_mode: GameMode,
    /// The player's experience level, which the anvil and the enchanting table show.
    pub experience_level: i32,
    pub lang: &'a Lang,
    pub font: &'a Font,
    pub chat_settings: &'a ChatSettings,
    pub config: &'a Config,
    /// The GUI's size in GUI pixels.
    pub size: (f32, f32),
    /// Shift (sneak) and Control (sprint) as held.
    pub shift: bool,
    pub control: bool,
}

/// The sprites the overlays draw with.
#[derive(Clone, Copy)]
pub struct OverlayTextures<'a> {
    pub inventory: &'a InventoryTextures,
    pub creative: &'a CreativeTextures,
    pub switcher: &'a SwitcherTextures,
    pub container: &'a ContainerTextures,
}

/// What an overlay draws with and shows.
pub struct OverlayDraw<'a> {
    pub draw_list: &'a mut DrawList,
    pub textures: OverlayTextures<'a>,
    pub painter: ItemPainter<'a>,
    pub inventory: &'a Inventory,
    pub container: Option<&'a OpenContainer>,
    pub chat: &'a Chat,
    pub chat_settings: &'a ChatSettings,
    pub lang: &'a Lang,
    pub size: (f32, f32),
    pub recipes: &'a crate::world::recipes::RecipeBook,
    pub recipe_sets: &'a crate::world::recipes::RecipeSets,
    /// The recipe the server asked the menu to show as ghost items.
    pub ghost: Option<&'a (i32, crate::world::recipes::RecipeDisplay)>,
    /// The player's experience level and whether materials are free (creative).
    pub experience_level: i32,
    pub infinite_materials: bool,
    /// The texture of the entity the overlay asked to show.
    pub entity_preview: Option<crate::renderer::TextureId>,
}

/// A screen over the running world (vanilla's in-game `Screen`s that don't pause it).
pub trait Overlay {
    fn kind(&self) -> OverlayKind;

    /// The GUI was resized.
    fn resize(&mut self, _width: f32, _height: f32) {}

    /// The language changed.
    fn refresh_lang(&mut self, _lang: &Lang) {}

    /// The mouse moved to `(x, y)`, `dragging` with a button held.
    fn mouse_moved(&mut self, x: f32, y: f32, dragging: bool, ctx: &mut OverlayContext);

    fn mouse_button(
        &mut self,
        _button: MouseButtonKind,
        _pressed: bool,
        _ctx: &mut OverlayContext,
    ) -> OverlayAction {
        OverlayAction::None
    }

    /// The wheel, `lines` up.
    fn scroll(&mut self, _lines: f32, _ctx: &mut OverlayContext) {}

    /// A key went down.
    fn key(
        &mut self,
        _bind: Option<&InputBind>,
        _text: &[TextEvent],
        _ctx: &mut OverlayContext,
    ) -> Vec<OverlayAction> {
        Vec::new()
    }

    /// A client tick passed.
    fn tick(&mut self, _ctx: &mut OverlayContext) -> Vec<OverlayAction> {
        Vec::new()
    }

    /// Escape: closes the overlay, unless it has something of its own to close.
    fn escape(&mut self) -> OverlayAction {
        OverlayAction::Close(ChatExit::Intentional)
    }

    /// It is closing, for `exit`: hand back what it held.
    fn closed(&mut self, _exit: ChatExit, _ctx: &mut OverlayContext) {}

    /// F3+F4 while it is open (the game mode switcher picks the next mode).
    fn cycle_game_mode(&mut self) {}

    /// F3 let go: the game mode to switch to, if this overlay chooses one.
    fn released_debug_key(&self) -> Option<GameMode> {
        None
    }

    /// Command completions the server sent.
    fn receive_suggestions(&mut self, _suggestions: Suggestions, _lang: &Lang) {}

    fn draw(&mut self, ui: &mut OverlayDraw);

    /// Where to show the player turned toward the mouse, if anywhere.
    fn entity_preview(&self) -> Option<crate::gui::entity_preview::EntityPreview> {
        None
    }

    /// Where the panel is, for the list of status effects beside it.
    fn effects_panel(&self) -> Option<crate::gui::hud::effects::EffectsPanel> {
        None
    }

    /// Hands the stacks it shows to `visit`, for baking their icons; stops when `visit` says
    /// so.
    fn visit_stacks(
        &self,
        _inventory: &Inventory,
        _visit: &mut dyn FnMut(&ItemStack) -> bool,
    ) -> bool {
        true
    }
}
