//! Vanilla `KeyMapping` / `InputConstants`.

use std::borrow::Cow;

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use melon_assets::i18n::Lang;

/// `InputConstants.UNKNOWN.getName`.
pub const UNKNOWN: &str = "key.keyboard.unknown";

/// `Options.mouseSensitivity`'s default (`UnitDouble` 0.5 → 100%).
pub const SENSITIVITY_DEFAULT: f64 = 0.5;
/// Slider steps for sensitivity (`UnitDouble` shown as 0..=200%).
pub const SENSITIVITY_STEPS: u32 = 200;
/// `Options.sprintWindow`'s `IntRange(0, 10)` default.
pub const SPRINT_WINDOW_DEFAULT: u8 = 7;
pub const SPRINT_WINDOW_MAX: u8 = 10;

/// One physical key or mouse button, as vanilla names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InputBind {
    name: String,
}

impl InputBind {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: if name.is_empty() {
                UNKNOWN.to_owned()
            } else {
                name
            },
        }
    }

    pub fn unknown() -> Self {
        Self::new(UNKNOWN)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_unbound(&self) -> bool {
        self.name == UNKNOWN
    }

    /// `InputConstants.getKey(name)`.
    pub fn parse(name: &str) -> Self {
        Self::new(name.trim())
    }

    pub fn from_key_code(code: KeyCode) -> Option<Self> {
        key_code_name(code).map(Self::new)
    }

    pub fn from_mouse(button: MouseButton) -> Option<Self> {
        mouse_name(button).map(Self::new)
    }

    /// `KeyMapping.getTranslatedKeyMessage` (by `InputConstants.Type`).
    pub fn label<'a>(&'a self, lang: &'a Lang) -> Cow<'a, str> {
        if let Some(number) = self.name.strip_prefix("key.mouse.")
            && number.parse::<u8>().is_ok()
        {
            return Cow::Owned(lang.format("key.mouse", &[number]));
        }
        if let Some(key) = self.name.strip_prefix("key.keyboard.")
            && key.chars().count() == 1
        {
            return Cow::Owned(key.to_uppercase());
        }
        Cow::Borrowed(lang.get_dynamic(&self.name))
    }
}

/// `MouseHandler` then `LocalPlayer.turn`: `(sens * 0.6 + 0.2)³ * 8 * 0.15`.
pub fn look_scale(sensitivity: f64) -> f32 {
    let f = (sensitivity as f32).mul_add(0.6, 0.2);
    f * f * f * 8.0 * 0.15
}

/// An action Melon reads from a bind. Only what the client actually uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Attack,
    Use,
    PickItem,
    Forward,
    Left,
    Back,
    Right,
    Jump,
    Sneak,
    Sprint,
    Inventory,
    SwapOffhand,
    Drop,
    Hotbar1,
    Hotbar2,
    Hotbar3,
    Hotbar4,
    Hotbar5,
    Hotbar6,
    Hotbar7,
    Hotbar8,
    Hotbar9,
    Chat,
    PlayerList,
    Command,
    ToggleGui,
    Screenshot,
    TogglePerspective,
    SaveHotbar,
    LoadHotbar,
    /// `keyQuickActions`: the server's quick actions dialog.
    QuickActions,
}

impl Action {
    pub const ALL: &'static [Self] = &[
        Self::Attack,
        Self::Use,
        Self::PickItem,
        Self::Forward,
        Self::Left,
        Self::Back,
        Self::Right,
        Self::Jump,
        Self::Sneak,
        Self::Sprint,
        Self::Inventory,
        Self::SwapOffhand,
        Self::Drop,
        Self::Hotbar1,
        Self::Hotbar2,
        Self::Hotbar3,
        Self::Hotbar4,
        Self::Hotbar5,
        Self::Hotbar6,
        Self::Hotbar7,
        Self::Hotbar8,
        Self::Hotbar9,
        Self::Chat,
        Self::PlayerList,
        Self::Command,
        Self::ToggleGui,
        Self::Screenshot,
        Self::TogglePerspective,
        Self::SaveHotbar,
        Self::LoadHotbar,
        Self::QuickActions,
    ];

    /// `KeyMapping.getName`.
    pub fn vanilla_name(self) -> &'static str {
        match self {
            Self::Attack => "key.attack",
            Self::Use => "key.use",
            Self::PickItem => "key.pickItem",
            Self::Forward => "key.forward",
            Self::Left => "key.left",
            Self::Back => "key.back",
            Self::Right => "key.right",
            Self::Jump => "key.jump",
            Self::Sneak => "key.sneak",
            Self::Sprint => "key.sprint",
            Self::Inventory => "key.inventory",
            Self::SwapOffhand => "key.swapOffhand",
            Self::Drop => "key.drop",
            Self::Hotbar1 => "key.hotbar.1",
            Self::Hotbar2 => "key.hotbar.2",
            Self::Hotbar3 => "key.hotbar.3",
            Self::Hotbar4 => "key.hotbar.4",
            Self::Hotbar5 => "key.hotbar.5",
            Self::Hotbar6 => "key.hotbar.6",
            Self::Hotbar7 => "key.hotbar.7",
            Self::Hotbar8 => "key.hotbar.8",
            Self::Hotbar9 => "key.hotbar.9",
            Self::Chat => "key.chat",
            Self::PlayerList => "key.playerlist",
            Self::Command => "key.command",
            Self::ToggleGui => "key.toggleGui",
            Self::Screenshot => "key.screenshot",
            Self::TogglePerspective => "key.togglePerspective",
            Self::SaveHotbar => "key.saveToolbarActivator",
            Self::LoadHotbar => "key.loadToolbarActivator",
            Self::QuickActions => "key.quickActions",
        }
    }

    /// `KeyMapping.Category` translation key (`key.category.<id>`).
    pub fn category_key(self) -> &'static str {
        match self {
            Self::Forward
            | Self::Left
            | Self::Back
            | Self::Right
            | Self::Jump
            | Self::Sneak
            | Self::Sprint => "key.category.movement",
            Self::ToggleGui | Self::Screenshot | Self::TogglePerspective | Self::QuickActions => {
                "key.category.misc"
            }
            Self::Chat | Self::PlayerList | Self::Command => "key.category.multiplayer",
            Self::Attack | Self::Use | Self::PickItem => "key.category.gameplay",
            Self::Inventory
            | Self::SwapOffhand
            | Self::Drop
            | Self::Hotbar1
            | Self::Hotbar2
            | Self::Hotbar3
            | Self::Hotbar4
            | Self::Hotbar5
            | Self::Hotbar6
            | Self::Hotbar7
            | Self::Hotbar8
            | Self::Hotbar9 => "key.category.inventory",
            Self::SaveHotbar | Self::LoadHotbar => "key.category.creative",
        }
    }

    /// Vanilla `KeyMapping` defaults (`InputConstants` names).
    pub fn default_bind(self) -> InputBind {
        InputBind::new(self.default_name())
    }

    fn default_name(self) -> &'static str {
        match self {
            Self::Attack => "key.mouse.left",
            Self::Use => "key.mouse.right",
            Self::PickItem => "key.mouse.middle",
            Self::Forward => "key.keyboard.w",
            Self::Left => "key.keyboard.a",
            Self::Back => "key.keyboard.s",
            Self::Right => "key.keyboard.d",
            Self::Jump => "key.keyboard.space",
            Self::Sneak => "key.keyboard.left.shift",
            Self::Sprint => "key.keyboard.left.control",
            Self::Inventory => "key.keyboard.e",
            Self::SwapOffhand => "key.keyboard.f",
            Self::Drop => "key.keyboard.q",
            Self::Hotbar1 => "key.keyboard.1",
            Self::Hotbar2 => "key.keyboard.2",
            Self::Hotbar3 => "key.keyboard.3",
            Self::Hotbar4 => "key.keyboard.4",
            Self::Hotbar5 => "key.keyboard.5",
            Self::Hotbar6 => "key.keyboard.6",
            Self::Hotbar7 => "key.keyboard.7",
            Self::Hotbar8 => "key.keyboard.8",
            Self::Hotbar9 => "key.keyboard.9",
            Self::Chat => "key.keyboard.t",
            Self::PlayerList => "key.keyboard.tab",
            Self::Command => "key.keyboard.slash",
            Self::ToggleGui => "key.keyboard.f1",
            Self::Screenshot => "key.keyboard.f2",
            Self::TogglePerspective => "key.keyboard.f5",
            Self::SaveHotbar => "key.keyboard.c",
            Self::LoadHotbar => "key.keyboard.x",
            Self::QuickActions => "key.keyboard.g",
        }
    }

    pub fn from_vanilla_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.vanilla_name() == name)
    }

    /// Hotbar slot 0-8 for `key.hotbar.N`.
    pub fn hotbar_index(self) -> Option<u8> {
        Some(match self {
            Self::Hotbar1 => 0,
            Self::Hotbar2 => 1,
            Self::Hotbar3 => 2,
            Self::Hotbar4 => 3,
            Self::Hotbar5 => 4,
            Self::Hotbar6 => 5,
            Self::Hotbar7 => 6,
            Self::Hotbar8 => 7,
            Self::Hotbar9 => 8,
            _ => return None,
        })
    }

    /// `ToggleKeyMapping`: sneak and sprint follow `toggleCrouch`/`toggleSprint`.
    pub fn is_toggle(self) -> bool {
        matches!(self, Self::Sneak | Self::Sprint)
    }
}

/// `KeyMapping.Category.SORT_ORDER` for the categories Melon lists.
pub const CATEGORY_ORDER: &[&str] = &[
    "key.category.movement",
    "key.category.misc",
    "key.category.multiplayer",
    "key.category.gameplay",
    "key.category.inventory",
    "key.category.creative",
];

/// Every action's current bind.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyBinds {
    binds: Vec<InputBind>,
}

impl Default for KeyBinds {
    fn default() -> Self {
        Self {
            binds: Action::ALL.iter().map(|a| a.default_bind()).collect(),
        }
    }
}

impl KeyBinds {
    fn index(action: Action) -> usize {
        Action::ALL
            .iter()
            .position(|&a| a == action)
            .expect("Action::ALL lists every action")
    }

    pub fn get(&self, action: Action) -> &InputBind {
        &self.binds[Self::index(action)]
    }

    pub fn set(&mut self, action: Action, bind: InputBind) {
        self.binds[Self::index(action)] = bind;
    }

    pub fn reset(&mut self, action: Action) {
        self.set(action, action.default_bind());
    }

    pub fn reset_all(&mut self) {
        *self = Self::default();
    }

    pub fn is_default(&self, action: Action) -> bool {
        self.get(action) == &action.default_bind()
    }

    pub fn any_changed(&self) -> bool {
        Action::ALL.iter().any(|&a| !self.is_default(a))
    }

    /// `KeyMapping.hasKeyModifierConflict` for a bind colliding with another.
    pub fn has_collision(&self, action: Action) -> bool {
        let bind = self.get(action);
        if bind.is_unbound() {
            return false;
        }
        Action::ALL
            .iter()
            .any(|&other| other != action && self.get(other) == bind)
    }

    /// Every action currently mapped to `bind`.
    pub fn actions_for<'a>(&'a self, bind: &'a InputBind) -> impl Iterator<Item = Action> + 'a {
        Action::ALL
            .iter()
            .copied()
            .filter(move |&action| self.get(action) == bind)
    }
}

/// Mouse, toggle, sprint-window and the key map.
#[derive(Clone, Debug, PartialEq)]
pub struct InputSettings {
    /// `Options.mouseSensitivity`, `0..=1`.
    pub sensitivity: f64,
    /// `Options.invertYMouse`.
    pub invert_y: bool,
    pub toggle_crouch: bool,
    pub toggle_sprint: bool,
    /// `Options.autoJump`.
    pub auto_jump: bool,
    /// `Options.sprintWindow`, ticks, `0` off.
    pub sprint_window: u8,
    pub binds: KeyBinds,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            sensitivity: SENSITIVITY_DEFAULT,
            invert_y: false,
            toggle_crouch: false,
            toggle_sprint: false,
            auto_jump: false,
            sprint_window: SPRINT_WINDOW_DEFAULT,
            binds: KeyBinds::default(),
        }
    }
}

fn mouse_name(button: MouseButton) -> Option<&'static str> {
    Some(match button {
        MouseButton::Left => "key.mouse.left",
        MouseButton::Right => "key.mouse.right",
        MouseButton::Middle => "key.mouse.middle",
        MouseButton::Back => "key.mouse.4",
        MouseButton::Forward => "key.mouse.5",
        MouseButton::Other(3) => "key.mouse.4",
        MouseButton::Other(4) => "key.mouse.5",
        MouseButton::Other(5) => "key.mouse.6",
        MouseButton::Other(6) => "key.mouse.7",
        MouseButton::Other(7) => "key.mouse.8",
        _ => return None,
    })
}

fn key_code_name(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::KeyA => "key.keyboard.a",
        KeyCode::KeyB => "key.keyboard.b",
        KeyCode::KeyC => "key.keyboard.c",
        KeyCode::KeyD => "key.keyboard.d",
        KeyCode::KeyE => "key.keyboard.e",
        KeyCode::KeyF => "key.keyboard.f",
        KeyCode::KeyG => "key.keyboard.g",
        KeyCode::KeyH => "key.keyboard.h",
        KeyCode::KeyI => "key.keyboard.i",
        KeyCode::KeyJ => "key.keyboard.j",
        KeyCode::KeyK => "key.keyboard.k",
        KeyCode::KeyL => "key.keyboard.l",
        KeyCode::KeyM => "key.keyboard.m",
        KeyCode::KeyN => "key.keyboard.n",
        KeyCode::KeyO => "key.keyboard.o",
        KeyCode::KeyP => "key.keyboard.p",
        KeyCode::KeyQ => "key.keyboard.q",
        KeyCode::KeyR => "key.keyboard.r",
        KeyCode::KeyS => "key.keyboard.s",
        KeyCode::KeyT => "key.keyboard.t",
        KeyCode::KeyU => "key.keyboard.u",
        KeyCode::KeyV => "key.keyboard.v",
        KeyCode::KeyW => "key.keyboard.w",
        KeyCode::KeyX => "key.keyboard.x",
        KeyCode::KeyY => "key.keyboard.y",
        KeyCode::KeyZ => "key.keyboard.z",
        KeyCode::Digit1 => "key.keyboard.1",
        KeyCode::Digit2 => "key.keyboard.2",
        KeyCode::Digit3 => "key.keyboard.3",
        KeyCode::Digit4 => "key.keyboard.4",
        KeyCode::Digit5 => "key.keyboard.5",
        KeyCode::Digit6 => "key.keyboard.6",
        KeyCode::Digit7 => "key.keyboard.7",
        KeyCode::Digit8 => "key.keyboard.8",
        KeyCode::Digit9 => "key.keyboard.9",
        KeyCode::Digit0 => "key.keyboard.0",
        KeyCode::Enter => "key.keyboard.enter",
        KeyCode::Escape => "key.keyboard.escape",
        KeyCode::Backspace => "key.keyboard.backspace",
        KeyCode::Tab => "key.keyboard.tab",
        KeyCode::Space => "key.keyboard.space",
        KeyCode::Minus => "key.keyboard.minus",
        KeyCode::Equal => "key.keyboard.equal",
        KeyCode::BracketLeft => "key.keyboard.left.bracket",
        KeyCode::BracketRight => "key.keyboard.right.bracket",
        KeyCode::Backslash => "key.keyboard.backslash",
        KeyCode::Semicolon => "key.keyboard.semicolon",
        KeyCode::Quote => "key.keyboard.apostrophe",
        KeyCode::Backquote => "key.keyboard.grave.accent",
        KeyCode::Comma => "key.keyboard.comma",
        KeyCode::Period => "key.keyboard.period",
        KeyCode::Slash => "key.keyboard.slash",
        KeyCode::CapsLock => "key.keyboard.caps.lock",
        KeyCode::F1 => "key.keyboard.f1",
        KeyCode::F2 => "key.keyboard.f2",
        KeyCode::F3 => "key.keyboard.f3",
        KeyCode::F4 => "key.keyboard.f4",
        KeyCode::F5 => "key.keyboard.f5",
        KeyCode::F6 => "key.keyboard.f6",
        KeyCode::F7 => "key.keyboard.f7",
        KeyCode::F8 => "key.keyboard.f8",
        KeyCode::F9 => "key.keyboard.f9",
        KeyCode::F10 => "key.keyboard.f10",
        KeyCode::F11 => "key.keyboard.f11",
        KeyCode::F12 => "key.keyboard.f12",
        KeyCode::F13 => "key.keyboard.f13",
        KeyCode::F14 => "key.keyboard.f14",
        KeyCode::F15 => "key.keyboard.f15",
        KeyCode::F16 => "key.keyboard.f16",
        KeyCode::F17 => "key.keyboard.f17",
        KeyCode::F18 => "key.keyboard.f18",
        KeyCode::F19 => "key.keyboard.f19",
        KeyCode::F20 => "key.keyboard.f20",
        KeyCode::F21 => "key.keyboard.f21",
        KeyCode::F22 => "key.keyboard.f22",
        KeyCode::F23 => "key.keyboard.f23",
        KeyCode::F24 => "key.keyboard.f24",
        KeyCode::PrintScreen => "key.keyboard.print.screen",
        KeyCode::ScrollLock => "key.keyboard.scroll.lock",
        KeyCode::Pause => "key.keyboard.pause",
        KeyCode::Insert => "key.keyboard.insert",
        KeyCode::Home => "key.keyboard.home",
        KeyCode::PageUp => "key.keyboard.page.up",
        KeyCode::Delete => "key.keyboard.delete",
        KeyCode::End => "key.keyboard.end",
        KeyCode::PageDown => "key.keyboard.page.down",
        KeyCode::ArrowRight => "key.keyboard.right",
        KeyCode::ArrowLeft => "key.keyboard.left",
        KeyCode::ArrowDown => "key.keyboard.down",
        KeyCode::ArrowUp => "key.keyboard.up",
        KeyCode::NumLock => "key.keyboard.num.lock",
        KeyCode::NumpadDivide => "key.keyboard.keypad.divide",
        KeyCode::NumpadMultiply => "key.keyboard.keypad.multiply",
        KeyCode::NumpadSubtract => "key.keyboard.keypad.subtract",
        KeyCode::NumpadAdd => "key.keyboard.keypad.add",
        KeyCode::NumpadEnter => "key.keyboard.keypad.enter",
        KeyCode::Numpad1 => "key.keyboard.keypad.1",
        KeyCode::Numpad2 => "key.keyboard.keypad.2",
        KeyCode::Numpad3 => "key.keyboard.keypad.3",
        KeyCode::Numpad4 => "key.keyboard.keypad.4",
        KeyCode::Numpad5 => "key.keyboard.keypad.5",
        KeyCode::Numpad6 => "key.keyboard.keypad.6",
        KeyCode::Numpad7 => "key.keyboard.keypad.7",
        KeyCode::Numpad8 => "key.keyboard.keypad.8",
        KeyCode::Numpad9 => "key.keyboard.keypad.9",
        KeyCode::Numpad0 => "key.keyboard.keypad.0",
        KeyCode::NumpadDecimal => "key.keyboard.keypad.period",
        KeyCode::NumpadEqual => "key.keyboard.keypad.equal",
        KeyCode::NumpadComma => "key.keyboard.keypad.comma",
        KeyCode::ContextMenu => "key.keyboard.application",
        KeyCode::ShiftLeft => "key.keyboard.left.shift",
        KeyCode::ShiftRight => "key.keyboard.right.shift",
        KeyCode::ControlLeft => "key.keyboard.left.control",
        KeyCode::ControlRight => "key.keyboard.right.control",
        KeyCode::AltLeft => "key.keyboard.left.alt",
        KeyCode::AltRight => "key.keyboard.right.alt",
        KeyCode::SuperLeft => "key.keyboard.left.win",
        KeyCode::SuperRight => "key.keyboard.right.win",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sensitivity_matches_hardcoded_look_scale() {
        assert!((look_scale(SENSITIVITY_DEFAULT) - 0.15).abs() < 1e-6);
    }

    #[test]
    fn keys_show_vanillas_names() {
        let lang = Lang::from_pairs(&[
            ("key.mouse", "Button %1$s"),
            ("key.mouse.left", "Left Button"),
            ("key.keyboard.space", "Space"),
            ("key.keyboard.unknown", "Not Bound"),
        ]);
        let label = |name: &str| InputBind::new(name).label(&lang).into_owned();
        assert_eq!(label("key.keyboard.r"), "R");
        assert_eq!(label("key.keyboard.4"), "4");
        assert_eq!(label("key.keyboard.space"), "Space");
        assert_eq!(label("key.keyboard.unknown"), "Not Bound");
        assert_eq!(label("key.mouse.left"), "Left Button");
        assert_eq!(label("key.mouse.5"), "Button 5");
    }

    #[test]
    fn vanilla_names_round_trip() {
        for &action in Action::ALL {
            assert_eq!(
                Action::from_vanilla_name(action.vanilla_name()),
                Some(action)
            );
            assert_eq!(action.default_bind().name(), action.default_name());
        }
    }

    #[test]
    fn collision_ignores_unbound() {
        let mut binds = KeyBinds::default();
        binds.set(Action::Chat, InputBind::unknown());
        binds.set(Action::Command, InputBind::unknown());
        assert!(!binds.has_collision(Action::Chat));
        binds.set(Action::Chat, Action::Forward.default_bind());
        assert!(binds.has_collision(Action::Chat));
        assert!(binds.has_collision(Action::Forward));
    }
}
