//! Vanilla `WorldOptionsScreen` (26.3): the world's game modes, commands and
//! difficulty, and opening it to the LAN, which replaced "Open to LAN".
//! Melon adds two LAN switches: whether guests are verified, and Bedrock.

use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::text::{StyledText, TextStyle, colors};
use crate::gui::widgets::button::Button;
use crate::gui::widgets::edit_box::EditBox;
use crate::gui::widgets::layout::{BUTTON_H, HeaderFooter};
use crate::gui::widgets::set::{KeyResult, WidgetSet};
use crate::gui::widgets::{Rect, Ui, Widget};
use crate::singleplayer::server::Publish;
use crate::world::game_mode::GameMode;

/// `Button.builder`'s and `CycleButton`'s default width, and the grids' gap.
const W: f32 = 150.0;
const GAP: f32 = 8.0;
/// A button spanning both columns of a grid.
const WIDE: f32 = W * 2.0 + GAP;
const ROW_GAP: f32 = 4.0;
/// `FocusableTextWidget`: a line and its padding.
const HEADER_H: f32 = 17.0;
/// `LinearLayout.vertical().spacing(8)` with `padding(8)` per cell.
const SECTION_PAD: f32 = 8.0;
/// `LockIconButton`.
const LOCK: f32 = 20.0;
/// `WorldOptionsScreen.PORT_LOWER_BOUND` / `PORT_HIGHER_BOUND`.
const PORTS: std::ops::RangeInclusive<u16> = 1024..=65535;
/// `HttpUtil.getAvailablePort`'s port when none is free.
const FALLBACK_PORT: u16 = 25564;
/// `EditBox.setTextColor`: `-2039584` and `-2142128`.
const PORT_OK: [f32; 4] = [0.878, 0.878, 0.878, 1.0];
const PORT_BAD: [f32; 4] = [0.871, 0.314, 0.314, 1.0];
const SCROLL_RATE: f32 = 10.0;

/// What the screen starts from: the world as it is now.
#[derive(Clone, Debug)]
pub struct WorldOptionsState {
    pub default_game_mode: GameMode,
    /// The host's own; `None` without its player on the server.
    pub personal_game_mode: Option<GameMode>,
    pub allow_commands: bool,
    pub hardcore: bool,
    /// The player's permission level (`COMMANDS_OWNER` is 4).
    pub permission_level: u8,
    pub difficulty: u8,
    pub difficulty_locked: bool,
    pub published: Option<Publish>,
    pub guest_command_access: bool,
    pub force_game_mode: bool,
}

/// What Apply Changes asks the world to do; `None` leaves a value as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldOptionsChange {
    pub difficulty: Option<u8>,
    pub lock_difficulty: bool,
    pub allow_commands: Option<bool>,
    pub force_game_mode: Option<bool>,
    pub default_game_mode: Option<GameMode>,
    pub personal_game_mode: Option<GameMode>,
    pub guest_command_access: Option<bool>,
    /// `changeMultiplayerScope`: unpublish, then publish this when `Some`.
    pub publish: Option<Option<Publish>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    DefaultGameMode,
    PersonalGameMode,
    AllowCommands,
    Difficulty,
    Lock,
    GameRules,
    Restrictions,
    Lan,
    Port,
    GuestCommands,
    ForceGameMode,
    OnlineMode,
    Bedrock,
    Apply,
    Cancel,
    /// The difficulty lock's confirmation.
    Yes,
    No,
}

pub struct WorldOptionsScreen {
    initial: WorldOptionsState,
    difficulty: u8,
    locked: bool,
    allow_commands: bool,
    default_game_mode: GameMode,
    personal_game_mode: Option<GameMode>,
    lan: bool,
    /// The port a publish uses: the field's, or a free one while it is blank.
    port: u16,
    port_error: Option<&'static str>,
    online_mode: bool,
    bedrock: bool,
    guest_command_access: bool,
    force_game_mode: bool,
    widgets: WidgetSet<Id>,
    scroll: f32,
    content_h: f32,
    /// The GUI size of the last layout.
    size: (f32, f32),
    mouse: (f32, f32),
    /// Asking whether to lock the difficulty for good.
    confirming: bool,
}

/// `HttpUtil.getAvailablePort`: one the OS hands out.
fn available_port() -> u16 {
    std::net::TcpListener::bind(("0.0.0.0", 0))
        .and_then(|listener| listener.local_addr())
        .map_or(FALLBACK_PORT, |address| address.port())
}

/// `HttpUtil.isPortAvailable`.
fn port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// `tryParsePort`: the port and no error, or the error. `None` for a blank field.
fn parse_port(
    value: &str,
    initial: Option<u16>,
    available: impl Fn(u16) -> bool,
) -> Result<Option<u16>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let Ok(port) = value.parse::<u32>() else {
        return Err("lanServer.port.invalid");
    };
    let Some(port) = u16::try_from(port).ok().filter(|p| PORTS.contains(p)) else {
        return Err("lanServer.port.invalid");
    };
    if Some(port) != initial && !available(port) {
        return Err("lanServer.port.unavailable");
    }
    Ok(Some(port))
}

impl WorldOptionsScreen {
    pub fn new(state: WorldOptionsState) -> Self {
        let cycle = |w| Button::sized(String::new(), w, BUTTON_H);
        let mut port = EditBox::new("lanServer.port", W, BUTTON_H);
        port.set_max_length(5);
        let initial_port = state.published.map(|p| p.port);
        if let Some(value) = initial_port {
            port.set_value(&value.to_string());
        }
        let widgets = WidgetSet::new()
            .with_button(Id::DefaultGameMode, cycle(WIDE))
            .with_button(Id::PersonalGameMode, cycle(WIDE))
            .with_button(Id::AllowCommands, cycle(W))
            .with_button(Id::Difficulty, cycle(W - LOCK))
            .with_button(Id::Lock, Button::sized(String::new(), LOCK, LOCK))
            .with_button(
                Id::GameRules,
                Button::sized("editGamerule.inGame.button", W, BUTTON_H),
            )
            .with_button(
                Id::Restrictions,
                Button::sized("restrictions_screen.button", W, BUTTON_H),
            )
            .with_button(Id::Lan, cycle(W))
            .with_field(Id::Port, port)
            .with_button(Id::GuestCommands, cycle(W))
            .with_button(Id::ForceGameMode, cycle(W))
            .with_button(Id::OnlineMode, cycle(W))
            .with_button(Id::Bedrock, cycle(W))
            .with_button(
                Id::Apply,
                Button::sized("menu.multiplayerOptions.applyChanges", W, BUTTON_H),
            )
            .with_button(Id::Cancel, Button::sized("gui.cancel", W, BUTTON_H))
            .with_button(Id::Yes, Button::sized("gui.yes", W, BUTTON_H))
            .with_button(Id::No, Button::sized("gui.no", W, BUTTON_H));
        let mut screen = Self {
            difficulty: state.difficulty,
            locked: state.difficulty_locked || state.hardcore,
            allow_commands: state.allow_commands,
            default_game_mode: state.default_game_mode,
            personal_game_mode: state.personal_game_mode,
            lan: state.published.is_some(),
            port: initial_port.unwrap_or_else(available_port),
            port_error: None,
            online_mode: state.published.is_none_or(|p| p.online_mode),
            bedrock: state.published.is_some_and(|p| p.bedrock),
            guest_command_access: state.guest_command_access,
            force_game_mode: state.force_game_mode,
            initial: state,
            widgets,
            scroll: 0.0,
            content_h: 0.0,
            size: (0.0, 0.0),
            mouse: (0.0, 0.0),
            confirming: false,
        };
        screen.widgets.set_visible(Id::Yes, false);
        screen.widgets.set_visible(Id::No, false);
        screen
            .widgets
            .set_visible(Id::PersonalGameMode, screen.personal_game_mode.is_some());
        screen.refresh_guest_access();
        screen.update();
        screen
    }

    fn initial_port(&self) -> Option<u16> {
        self.initial.published.map(|p| p.port)
    }

    /// `updateButton`'s rule: game modes and game rules need commands, and
    /// no hardcore.
    fn commands_usable(&self) -> bool {
        !self.initial.hardcore && self.allow_commands
    }

    /// The difficulty buttons' `updateDifficultyButtonsState`: the host may
    /// change it unless the level's is locked or hardcore.
    fn difficulty_usable(&self) -> bool {
        !self.initial.difficulty_locked && !self.initial.hardcore
    }

    /// `updateGuestCommandAccessButton`: guests get commands only with the
    /// LAN and commands on, and then start from the server's choice.
    fn refresh_guest_access(&mut self) {
        self.guest_command_access =
            self.lan && self.allow_commands && self.initial.guest_command_access;
        self.refresh_force_game_mode();
    }

    /// `updateForceGameModeButton`.
    fn refresh_force_game_mode(&mut self) {
        self.force_game_mode = if self.initial.hardcore || !self.lan {
            true
        } else if self.guest_command_access {
            false
        } else {
            self.initial.force_game_mode
        };
    }

    /// Which widgets are live, and whether Apply Changes is.
    fn update(&mut self) {
        let guests_free = self.lan && !self.guest_command_access && !self.initial.hardcore;
        let commands = self.commands_usable();
        let can_allow = !self.initial.hardcore || self.initial.permission_level >= 4;
        let difficulty = self.difficulty_usable();
        let apply = self.has_changes() && (!self.lan || self.port_error.is_none());
        let port_color = if self.port_error.is_some() {
            PORT_BAD
        } else {
            PORT_OK
        };
        let w = &mut self.widgets;
        w.set_enabled(Id::DefaultGameMode, commands);
        w.set_enabled(Id::PersonalGameMode, commands);
        w.set_enabled(Id::GameRules, false);
        w.set_enabled(Id::Restrictions, false);
        w.set_enabled(Id::AllowCommands, can_allow);
        w.set_enabled(Id::Difficulty, difficulty);
        w.set_enabled(Id::Lock, difficulty && !self.locked);
        w.set_enabled(Id::GuestCommands, self.lan && self.allow_commands);
        w.set_enabled(Id::ForceGameMode, guests_free);
        w.set_enabled(Id::OnlineMode, self.lan);
        w.set_enabled(Id::Bedrock, self.lan);
        w.set_enabled(Id::Apply, apply);
        if !self.lan {
            w.field_mut(Id::Port).set_value("");
            if w.focused() == Some(Id::Port) {
                w.blur();
            }
        }
        w.field_mut(Id::Port).set_text_color(Some(port_color));
    }

    /// The publish this screen asks for, if the LAN is on.
    fn wanted_publish(&self) -> Option<Publish> {
        self.lan.then_some(Publish {
            port: self.port,
            online_mode: self.online_mode,
            bedrock: self.bedrock,
        })
    }

    /// `applyChanges`, as a change for the world.
    fn change(&self) -> WorldOptionsChange {
        fn differs<T: PartialEq>(a: T, b: T) -> Option<T> {
            (a != b).then_some(a)
        }
        let s = &self.initial;
        WorldOptionsChange {
            difficulty: differs(self.difficulty, s.difficulty),
            lock_difficulty: self.locked && !(s.difficulty_locked || s.hardcore),
            allow_commands: differs(self.allow_commands, s.allow_commands),
            force_game_mode: differs(self.force_game_mode, s.force_game_mode),
            default_game_mode: differs(self.default_game_mode, s.default_game_mode),
            personal_game_mode: self
                .personal_game_mode
                .filter(|&mode| Some(mode) != s.personal_game_mode),
            guest_command_access: differs(self.guest_command_access, s.guest_command_access),
            publish: (self.wanted_publish() != s.published).then(|| self.wanted_publish()),
        }
    }

    fn has_changes(&self) -> bool {
        self.change() != WorldOptionsChange::default()
    }

    fn apply(&self) -> ScreenAction {
        ScreenAction::WorldOptions(Box::new(self.change()))
    }

    fn press(&mut self, id: Id) -> ScreenAction {
        let next_mode = |mode: GameMode| {
            let i = GameMode::ALL.iter().position(|&m| m == mode).unwrap_or(0);
            GameMode::ALL[(i + 1) % GameMode::ALL.len()]
        };
        match id {
            Id::DefaultGameMode => self.default_game_mode = next_mode(self.default_game_mode),
            Id::PersonalGameMode => {
                self.personal_game_mode = self.personal_game_mode.map(next_mode);
            }
            Id::AllowCommands => {
                self.allow_commands = !self.allow_commands;
                // `updatePermissionDependentButtons`: without commands, both
                // game modes go back to the world's.
                if !self.allow_commands {
                    self.default_game_mode = self.initial.default_game_mode;
                    if self.personal_game_mode.is_some() {
                        self.personal_game_mode = Some(self.default_game_mode);
                    }
                }
                self.refresh_guest_access();
            }
            Id::Difficulty => self.difficulty = (self.difficulty + 1) % 4,
            Id::Lock => self.locked = !self.locked,
            Id::Lan => {
                self.lan = !self.lan;
                self.port_error = None;
                if self.lan
                    && let Some(port) = self.initial_port()
                {
                    self.widgets
                        .field_mut(Id::Port)
                        .set_value(&port.to_string());
                    self.port = port;
                }
                self.refresh_guest_access();
            }
            Id::GuestCommands => {
                self.guest_command_access = !self.guest_command_access;
                self.refresh_force_game_mode();
            }
            Id::ForceGameMode => self.force_game_mode = !self.force_game_mode,
            Id::OnlineMode => self.online_mode = !self.online_mode,
            Id::Bedrock => self.bedrock = !self.bedrock,
            Id::Apply => {
                if self.locked != (self.initial.difficulty_locked || self.initial.hardcore) {
                    self.set_confirming(true);
                    return ScreenAction::None;
                }
                return self.apply();
            }
            Id::Cancel => return ScreenAction::Back,
            Id::Yes => return self.apply(),
            Id::No => self.set_confirming(false),
            Id::GameRules | Id::Restrictions | Id::Port => {}
        }
        self.update();
        ScreenAction::None
    }

    fn set_confirming(&mut self, on: bool) {
        self.confirming = on;
        self.widgets.set_visible(Id::Yes, on);
        self.widgets.set_visible(Id::No, on);
    }

    /// The port field changed: `setPortError(tryParsePort(value))`.
    fn port_edited(&mut self) {
        let value = self.widgets.field(Id::Port).value();
        match parse_port(&value, self.initial_port(), port_available) {
            Ok(port) => {
                self.port_error = None;
                self.port = port.unwrap_or_else(available_port);
            }
            Err(error) => self.port_error = Some(error),
        }
        self.update();
    }

    fn on_off(ui: &Ui, key: &'static str, on: bool) -> String {
        let value = ui.tr(if on { "options.on" } else { "options.off" });
        ui.lang.format("options.generic_value", &[ui.tr(key), value])
    }

    fn mode_label(ui: &Ui, key: &'static str, mode: GameMode) -> String {
        let value = ui.lang.get(mode.translation_key());
        ui.lang.format("options.generic_value", &[ui.tr(key), value])
    }

    fn label(&self, id: Id, ui: &Ui) -> Option<String> {
        Some(match id {
            Id::DefaultGameMode => {
                Self::mode_label(ui, "options.worldOptions.game_mode", self.default_game_mode)
            }
            Id::PersonalGameMode => Self::mode_label(
                ui,
                "options.worldOptions.personal_game_mode",
                self.personal_game_mode?,
            ),
            Id::AllowCommands => Self::on_off(ui, "selectWorld.allowCommands", self.allow_commands),
            Id::Difficulty => {
                let value = ui.lang.get(DIFFICULTIES[usize::from(self.difficulty.min(3))]);
                ui.lang
                    .format("options.generic_value", &[ui.tr("options.difficulty"), value])
            }
            Id::Lan => Self::on_off(ui, "menu.multiplayerOptions.lan", self.lan),
            Id::GuestCommands => Self::on_off(
                ui,
                "options.worldOptions.guest.command_access",
                self.guest_command_access,
            ),
            Id::ForceGameMode => Self::on_off(
                ui,
                "options.worldOptions.guest.force_game_mode",
                self.force_game_mode,
            ),
            Id::OnlineMode => Self::on_off(ui, "melon.worldOptions.onlineMode", self.online_mode),
            Id::Bedrock => Self::on_off(ui, "melon.worldOptions.bedrock", self.bedrock),
            Id::Lock => String::new(),
            _ => return None,
        })
    }

    /// The tooltip of `id` as it is now.
    fn tooltip(&self, id: Id) -> Option<&'static str> {
        let hardcore = self.initial.hardcore;
        let mode_tooltip = |plain| {
            if hardcore {
                "options.worldOptions.game_mode.disabled.tooltip"
            } else if self.allow_commands {
                plain
            } else {
                "options.worldOptions.game_mode.disabled.operator.tooltip"
            }
        };
        Some(match id {
            Id::DefaultGameMode => mode_tooltip("options.worldOptions.game_mode.tooltip"),
            Id::PersonalGameMode => mode_tooltip("options.worldOptions.personal_game_mode.tooltip"),
            Id::AllowCommands if hardcore && self.initial.permission_level < 4 => {
                "options.worldOptions.allow_commands.disabled.tooltip"
            }
            Id::AllowCommands => "options.worldOptions.allow_commands.tooltip",
            Id::Difficulty if self.initial.difficulty_locked => {
                "options.worldOptions.difficulty.disabled.locked.tooltip"
            }
            Id::Difficulty if hardcore => {
                "options.worldOptions.difficulty.disabled.hardcore.tooltip"
            }
            Id::GameRules | Id::Restrictions => "melon.worldOptions.unavailable.tooltip",
            Id::Lan if self.lan => "menu.multiplayerOptions.lan.tooltip",
            Id::Lan => "menu.multiplayerOptions.off.tooltip",
            Id::Port => self.port_error.unwrap_or("lanServer.port"),
            Id::GuestCommands if !self.lan => {
                "options.worldOptions.guest.command_access.disabled.scope.tooltip"
            }
            Id::GuestCommands if !self.allow_commands => {
                "options.worldOptions.guest.command_access.disabled.commands.tooltip"
            }
            Id::GuestCommands => "options.worldOptions.guest.command_access.tooltip",
            Id::ForceGameMode if hardcore => {
                "options.worldOptions.guest.force_game_mode.off.hardcore.tooltip"
            }
            Id::ForceGameMode if !self.lan => {
                "options.worldOptions.guest.force_game_mode.off.scope.tooltip"
            }
            Id::ForceGameMode if self.guest_command_access => {
                "options.worldOptions.guest.force_game_mode.off.commands.tooltip"
            }
            Id::ForceGameMode if self.force_game_mode => {
                "options.worldOptions.guest.force_game_mode.on.tooltip"
            }
            Id::ForceGameMode => "options.worldOptions.guest.force_game_mode.off.tooltip",
            Id::OnlineMode => "melon.worldOptions.onlineMode.tooltip",
            Id::Bedrock => "melon.worldOptions.bedrock.tooltip",
            _ => return None,
        })
    }

    /// Lays the grids out from the scroll position.
    fn layout(&mut self, width: f32, height: f32) {
        self.size = (width, height);
        let frame = HeaderFooter::DEFAULT;
        let left = (width / 2.0 - WIDE / 2.0).floor();
        let right = left + W + GAP;
        let personal = self.personal_game_mode.is_some();
        let w = &mut self.widgets;
        let mut y = frame.top() + SECTION_PAD - self.scroll;
        // General.
        y += HEADER_H + ROW_GAP;
        w.set_position(Id::DefaultGameMode, left, y);
        y += BUTTON_H + ROW_GAP;
        if personal {
            w.set_position(Id::PersonalGameMode, left, y);
            y += BUTTON_H + ROW_GAP;
        }
        w.set_position(Id::AllowCommands, left, y);
        w.set_position(Id::Difficulty, right, y);
        w.set_position(Id::Lock, right + W - LOCK, y);
        y += BUTTON_H + ROW_GAP;
        w.row(&[Id::GameRules, Id::Restrictions], left, y, GAP);
        y += BUTTON_H + SECTION_PAD * 2.0 + GAP;
        // Multiplayer.
        y += HEADER_H + ROW_GAP;
        w.row(&[Id::Lan, Id::Port], left, y, GAP);
        y += BUTTON_H + ROW_GAP;
        w.row(&[Id::GuestCommands, Id::ForceGameMode], left, y, GAP);
        y += BUTTON_H + ROW_GAP;
        w.row(&[Id::OnlineMode, Id::Bedrock], left, y, GAP);
        y += BUTTON_H + SECTION_PAD;
        self.content_h = y + self.scroll - frame.top();
        let footer = frame.footer_y(height, BUTTON_H);
        w.row(&[Id::Apply, Id::Cancel], left, footer, GAP);
        let middle = (height / 2.0).floor();
        w.row(&[Id::Yes, Id::No], left, middle + 10.0, GAP);
    }

    /// The headers' places, as [`Self::layout`] puts them.
    fn header_ys(&self) -> (f32, f32) {
        let frame = HeaderFooter::DEFAULT;
        let general = frame.top() + SECTION_PAD - self.scroll + 4.0;
        let rows = if self.personal_game_mode.is_some() { 4.0 } else { 3.0 };
        let multiplayer = general
            + HEADER_H
            + ROW_GAP
            + (BUTTON_H + ROW_GAP) * rows
            + BUTTON_H
            + SECTION_PAD * 2.0
            + GAP;
        (general, multiplayer)
    }

    fn max_scroll(&self, height: f32) -> f32 {
        let frame = HeaderFooter::DEFAULT;
        (self.content_h - (frame.bottom(height) - frame.top())).max(0.0)
    }
}

/// `Difficulty.getDisplayName`, by id.
const DIFFICULTIES: [&str; 4] = [
    "options.difficulty.peaceful",
    "options.difficulty.easy",
    "options.difficulty.normal",
    "options.difficulty.hard",
];

impl Screen for WorldOptionsScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.layout(width, height);
        self.scroll = self.scroll.min(self.max_scroll(height));
        self.layout(width, height);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.widgets.hover(x, y);
    }

    fn on_scroll(&mut self, delta: f32) {
        if self.confirming {
            return;
        }
        let (width, height) = self.size;
        self.scroll = (self.scroll - delta * SCROLL_RATE).clamp(0.0, self.max_scroll(height));
        self.layout(width, height);
        self.widgets.hover(self.mouse.0, self.mouse.1);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        let (width, height) = ctx.size();
        self.scroll = self.scroll.min(self.max_scroll(height));
        self.layout(width, height);
        let frame = HeaderFooter::DEFAULT;
        let in_list = y >= frame.top() && y < frame.bottom(height);
        let Some(id) = self.widgets.click(x, y) else {
            return ScreenAction::None;
        };
        let confirm = matches!(id, Id::Yes | Id::No);
        if self.confirming != confirm {
            return ScreenAction::None;
        }
        if !confirm && !matches!(id, Id::Apply | Id::Cancel) && !in_list {
            return ScreenAction::None;
        }
        if id == Id::Port {
            if !self.lan {
                self.widgets.blur();
            }
            return ScreenAction::None;
        }
        ctx.click();
        let action = self.press(id);
        self.layout(width, height);
        action
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        if self.widgets.focused() == Some(Id::Port) && !self.lan {
            self.widgets.blur();
        }
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => {
                ctx.click();
                self.press(id)
            }
            KeyResult::Edited(Id::Port) => {
                self.port_edited();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let frame = HeaderFooter::DEFAULT;
        ui.list_background(frame.top(), frame.bottom(ui.height));
        let title = ui.tr("options.worldOptions.title");
        ui.title(title, frame.title_y());
        if self.confirming {
            let middle = (ui.height / 2.0).floor();
            let question_title = ui.tr("difficulty.lock.title");
            ui.title(question_title, middle - 30.0);
            let difficulty = ui
                .lang
                .get(DIFFICULTIES[usize::from(self.difficulty.min(3))]);
            let question = ui.lang.format("difficulty.lock.question", &[difficulty]);
            for (i, line) in ui.font().wrap(&question, 300.0).iter().enumerate() {
                ui.title_colored(line, middle - 14.0 + i as f32 * 10.0, [1.0; 4]);
            }
            self.widgets.button(Id::Yes).draw(ui);
            self.widgets.button(Id::No).draw(ui);
            return;
        }
        ui.clip(Rect::new(
            0.0,
            frame.top(),
            ui.width,
            frame.bottom(ui.height) - frame.top(),
        ));
        let (general, multiplayer) = self.header_ys();
        let header = |ui: &mut Ui, key: &'static str, y: f32| {
            let style = TextStyle {
                bold: true,
                underlined: true,
                ..Default::default()
            };
            let mut text = StyledText::default();
            text.push(ui.tr(key), &style);
            let x = (ui.width / 2.0 - ui.styled_width(&text) / 2.0).floor();
            ui.styled(&text, x, y, colors::WHITE);
        };
        header(ui, "options.worldOptions.general.title", general);
        header(ui, "options.worldOptions.multiplayer.title", multiplayer);
        let list_ids = [
            Id::DefaultGameMode,
            Id::PersonalGameMode,
            Id::AllowCommands,
            Id::Difficulty,
            Id::GameRules,
            Id::Restrictions,
            Id::Lan,
            Id::GuestCommands,
            Id::ForceGameMode,
            Id::OnlineMode,
            Id::Bedrock,
        ];
        for id in list_ids {
            if id == Id::PersonalGameMode && self.personal_game_mode.is_none() {
                continue;
            }
            let button = self.widgets.button(id);
            match self.label(id, ui) {
                Some(text) => button.draw_labeled(ui, &text),
                None => button.draw(ui),
            }
        }
        // `LockIconButton`.
        let lock = self.widgets.button(Id::Lock);
        let icons = &ui.assets.lock_icons[usize::from(!self.locked)];
        let sprite = if !lock.enabled {
            icons[2]
        } else if lock.hovered || lock.focused {
            icons[1]
        } else {
            icons[0]
        };
        ui.sprite(sprite, lock.rect());
        let port = self.widgets.field(Id::Port);
        port.draw(ui);
        if self.lan && port.value().is_empty() {
            let hint = self.port.to_string();
            ui.text(&hint, port.x + 4.0, port.y + 6.0, [0.4, 0.4, 0.4, 1.0]);
        }
        ui.unclip();
        self.widgets.button(Id::Apply).draw(ui);
        self.widgets.button(Id::Cancel).draw(ui);
        // The hovered widget's tooltip, inactive ones too.
        let hovered = [
            Id::DefaultGameMode,
            Id::PersonalGameMode,
            Id::AllowCommands,
            Id::Difficulty,
            Id::GameRules,
            Id::Restrictions,
            Id::Lan,
            Id::Port,
            Id::GuestCommands,
            Id::ForceGameMode,
            Id::OnlineMode,
            Id::Bedrock,
        ]
        .into_iter()
        .filter(|&id| id != Id::PersonalGameMode || self.personal_game_mode.is_some())
        .find(|&id| {
            let rect = self.widgets.widget(id).rect();
            rect.contains(self.mouse.0, self.mouse.1)
                && self.mouse.1 >= frame.top()
                && self.mouse.1 < frame.bottom(ui.height)
        });
        if let Some(id) = hovered
            && let Some(key) = self.tooltip(id)
        {
            let text = ui.lang.get(key);
            let rect = self.widgets.widget(id).rect();
            ui.widget_tooltip(text, self.mouse, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> WorldOptionsState {
        WorldOptionsState {
            default_game_mode: GameMode::Survival,
            personal_game_mode: Some(GameMode::Survival),
            allow_commands: true,
            hardcore: false,
            permission_level: 4,
            difficulty: 2,
            difficulty_locked: false,
            published: None,
            guest_command_access: false,
            force_game_mode: true,
        }
    }

    #[test]
    fn a_port_is_checked_like_try_parse_port() {
        let free = |_| true;
        assert_eq!(parse_port("", None, free), Ok(None));
        assert_eq!(parse_port(" 25565 ", None, free), Ok(Some(25565)));
        assert_eq!(parse_port("80", None, free), Err("lanServer.port.invalid"));
        assert_eq!(parse_port("70000", None, free), Err("lanServer.port.invalid"));
        assert_eq!(parse_port("abc", None, free), Err("lanServer.port.invalid"));
        let taken = |_| false;
        assert_eq!(parse_port("25565", None, taken), Err("lanServer.port.unavailable"));
        // The port the world is published on is its own.
        assert_eq!(parse_port("25565", Some(25565), taken), Ok(Some(25565)));
    }

    #[test]
    fn nothing_changed_applies_nothing() {
        let screen = WorldOptionsScreen::new(state());
        assert!(!screen.has_changes());
        assert!(!screen.widgets.button(Id::Apply).enabled);
    }

    #[test]
    fn guests_get_commands_only_with_the_lan_and_commands_on() {
        let mut screen = WorldOptionsScreen::new(state());
        assert!(!screen.widgets.button(Id::GuestCommands).enabled);
        screen.press(Id::Lan);
        assert!(screen.widgets.button(Id::GuestCommands).enabled);
        screen.press(Id::GuestCommands);
        assert!(screen.guest_command_access);
        // Guests with commands play the game mode they want.
        assert!(!screen.force_game_mode);
        screen.press(Id::AllowCommands);
        assert!(!screen.guest_command_access);
        assert!(!screen.widgets.button(Id::GuestCommands).enabled);
    }

    #[test]
    fn opening_to_the_lan_asks_for_a_publish() {
        let mut screen = WorldOptionsScreen::new(state());
        screen.press(Id::Lan);
        let change = screen.change();
        let publish = change.publish.flatten().expect("a publish");
        assert!(publish.online_mode && !publish.bedrock);
        assert!(screen.widgets.button(Id::Apply).enabled);
    }

    #[test]
    fn without_commands_the_game_modes_go_back_to_the_worlds() {
        let mut screen = WorldOptionsScreen::new(state());
        screen.press(Id::DefaultGameMode);
        assert_eq!(screen.default_game_mode, GameMode::Creative);
        screen.press(Id::AllowCommands);
        assert_eq!(screen.default_game_mode, GameMode::Survival);
        assert!(!screen.widgets.button(Id::DefaultGameMode).enabled);
    }

    #[test]
    fn locking_the_difficulty_asks_first() {
        let mut screen = WorldOptionsScreen::new(state());
        screen.press(Id::Lock);
        assert!(matches!(screen.press(Id::Apply), ScreenAction::None));
        assert!(screen.confirming);
        assert!(matches!(screen.press(Id::Yes), ScreenAction::WorldOptions(c) if c.lock_difficulty));
    }

    #[test]
    fn hardcore_keeps_the_difficulty_and_the_game_mode() {
        let mut hardcore = state();
        hardcore.hardcore = true;
        let screen = WorldOptionsScreen::new(hardcore);
        assert!(!screen.widgets.button(Id::Difficulty).enabled);
        assert!(!screen.widgets.button(Id::DefaultGameMode).enabled);
        assert!(screen.force_game_mode);
    }
}
