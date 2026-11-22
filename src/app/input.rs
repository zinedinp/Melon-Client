//! The window's keys and mouse: to the world while playing, else to the top

use std::time::Instant;

use winit::dpi::PhysicalPosition;
use winit::event::{
    DeviceEvent, ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKey, PhysicalKey};
use winit::window::{CursorGrabMode, CursorIcon, Window};

use super::RunningState;
use crate::gui::overlay::container_input::MouseButtonKind;
use crate::gui::screen::{CursorKind, TextEvent};
use crate::platform::clipboard_text;
use crate::settings::input::InputBind;

/// What the window last said about input, and the cursor grab.
pub(super) struct InputState {
    /// The cursor in physical pixels.
    cursor: PhysicalPosition<f64>,
    /// Whether the window has the keyboard: the cursor is only grabbed then.
    focused: bool,
    /// `Minecraft.lastActiveTime`: when the window last had the focus.
    last_active: Instant,
    /// Shift, Control and friends.
    modifiers: ModifiersState,
    /// Hidden by other windows or minimized.
    pub occluded: bool,
    /// The last key, click, scroll or mouse move, for the AFK framerate.
    pub last_input: Instant,
    /// Whether the cursor is grabbed for looking around.
    grabbed: bool,
    /// Whether the cursor was last asked to be grabbed, so a failing grab
    grab_wanted: bool,
    /// The cursor was let go and sent to the centre, but is still locked
    release_pending: bool,
    /// Whether the window takes composed text (an input method editor:
    ime_allowed: bool,
    /// Where the input method's candidate window was last put (physical
    ime_area: Option<(i32, i32)>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            cursor: PhysicalPosition::new(0.0, 0.0),
            focused: true,
            last_active: Instant::now(),
            modifiers: ModifiersState::empty(),
            occluded: false,
            last_input: Instant::now(),
            grabbed: false,
            grab_wanted: false,
            release_pending: false,
            ime_allowed: false,
            ime_area: None,
        }
    }

    /// Grabs or frees the cursor, asked once per change so a refused grab
    pub fn want_grab(&mut self, window: &Window, want: bool) -> bool {
        if self.release_pending && !want {
            self.release_cursor(window);
            self.center_cursor(window);
        }
        if want == self.grab_wanted {
            return false;
        }
        self.grab_wanted = want;
        if want {
            self.release_pending = false;
            self.grabbed = grab_cursor(window);
            return false;
        }
        if !self.grabbed {
            self.release_cursor(window);
            return false;
        }
        self.grabbed = false;
        self.center_cursor(window);
        self.release_pending = true;
        true
    }

    /// `MouseHandler.releaseMouse`: the cursor goes to the window's centre,
    fn center_cursor(&mut self, window: &Window) {
        let size = window.inner_size();
        let center =
            PhysicalPosition::new(f64::from(size.width) / 2.0, f64::from(size.height) / 2.0);
        let _ = window.set_cursor_position(center);
        self.cursor = center;
    }

    pub fn release_cursor(&mut self, window: &Window) {
        self.grabbed = false;
        self.grab_wanted = false;
        self.release_pending = false;
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
    }
}

/// Best-effort pointer lock: `Locked` (relative motion, no OS cursor) falls
fn grab_cursor(window: &Window) -> bool {
    let grabbed = window
        .set_cursor_grab(CursorGrabMode::Locked)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
        .is_ok();
    window.set_cursor_visible(!grabbed);
    if !grabbed {
        crate::warn!("Cursor not grabbed: mouse look may be limited");
    }
    grabbed
}

/// What a key typed, as text events: the editing keys by name with their
pub fn text_events(event: &KeyEvent, shift: bool, control: bool) -> Vec<TextEvent> {
    let word = control;
    let shortcut = match (&event.logical_key, event.physical_key) {
        (Key::Character(c), _) if c.len() == 1 && c.is_ascii() => {
            c.chars().next().map(|c| c.to_ascii_lowercase())
        }
        (_, PhysicalKey::Code(KeyCode::KeyA)) => Some('a'),
        (_, PhysicalKey::Code(KeyCode::KeyC)) => Some('c'),
        (_, PhysicalKey::Code(KeyCode::KeyV)) => Some('v'),
        (_, PhysicalKey::Code(KeyCode::KeyX)) => Some('x'),
        _ => None,
    };
    let named = match &event.logical_key {
        Key::Named(NamedKey::Backspace) => Some(TextEvent::Backspace { word }),
        Key::Named(NamedKey::Delete) => Some(TextEvent::Delete { word }),
        Key::Named(NamedKey::ArrowLeft) => Some(TextEvent::Left { shift, word }),
        Key::Named(NamedKey::ArrowRight) => Some(TextEvent::Right { shift, word }),
        Key::Named(NamedKey::ArrowUp) => Some(TextEvent::Up),
        Key::Named(NamedKey::ArrowDown) => Some(TextEvent::Down),
        Key::Named(NamedKey::PageUp) => Some(TextEvent::PageUp),
        Key::Named(NamedKey::PageDown) => Some(TextEvent::PageDown),
        Key::Named(NamedKey::Home) => Some(TextEvent::Home { shift }),
        Key::Named(NamedKey::End) => Some(TextEvent::End { shift }),
        Key::Named(NamedKey::Enter) => Some(TextEvent::Enter),
        Key::Named(NamedKey::Tab) => Some(TextEvent::Tab { shift }),
        Key::Named(NamedKey::Insert) if !control && !shift => Some(TextEvent::Insert),
        _ if control && !shift => match shortcut {
            Some('a') => Some(TextEvent::SelectAll),
            Some('c') => Some(TextEvent::Copy),
            Some('x') => Some(TextEvent::Cut),
            Some('v') => clipboard_text().map(TextEvent::Paste),
            _ => None,
        },
        _ => None,
    };
    if let Some(named) = named {
        return vec![named];
    }
    if control {
        return Vec::new();
    }
    event
        .text
        .as_ref()
        .map(|text| text.chars().map(TextEvent::Char).collect())
        .unwrap_or_default()
}

impl RunningState {
    /// In a world with no screen over it, input goes to the world.
    fn playing(&self) -> bool {
        self.world.is_some() && !self.paused()
    }

    /// The cursor in GUI pixels.
    fn gui_cursor(&self) -> (f32, f32) {
        self.scale()
            .to_virtual((self.input.cursor.x, self.input.cursor.y))
    }

    /// Grabs the cursor while playing with no screen or overlay, and frees
    pub(super) fn sync_cursor(&mut self) {
        if self.world.is_none() || self.bench.is_some() {
            return;
        }
        let want = self.input.focused && !self.paused() && !self.overlay_open();
        if self.input.want_grab(&self.window, want) {
            self.cursor_moved();
        }
    }

    /// `AbstractWidget.handleCursor`: pointer/text over an active widget,
    fn update_os_cursor(&self) {
        let Some(cursor) = self.screens.cursor() else {
            return;
        };
        self.window.set_cursor(match cursor {
            CursorKind::Default => CursorIcon::Default,
            CursorKind::Pointer => CursorIcon::Pointer,
            CursorKind::Text => CursorIcon::Text,
            CursorKind::ResizeNs => CursorIcon::NsResize,
            CursorKind::ResizeEw => CursorIcon::EwResize,
        });
    }

    /// An event handled since `started`, for F3's time between frames.
    pub(super) fn count_event(&mut self, started: Instant) {
        if let Some(world) = &mut self.world {
            world.profiler.event(started.elapsed());
        }
    }

    /// Mouse motion while the cursor is grabbed: the player looks around.
    pub(super) fn device_event(&mut self, event: DeviceEvent) {
        if !self.input.grabbed || self.bench.is_some() {
            return;
        }
        if let (Some(world), DeviceEvent::MouseMotion { delta: (dx, dy) }) =
            (&mut self.world, event)
        {
            let input = &self.env.config.input;
            world.look(dx as f32, dy as f32, input.sensitivity, input.invert_y);
            self.input.last_input = Instant::now();
        }
    }

    /// Every window event but closing, resizing and redrawing.
    pub(super) fn input_event(&mut self, event: WindowEvent, event_loop: &ActiveEventLoop) {
        if self.bench.is_some() {
            return;
        }
        if matches!(
            event,
            WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::CursorMoved { .. }
        ) {
            self.input.last_input = Instant::now();
        }
        match event {
            WindowEvent::Focused(focused) => self.input.focused = focused,
            WindowEvent::Occluded(occluded) => self.input.occluded = occluded,
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input.modifiers = modifiers.state();
                crate::gui::input_state::set_shift(self.input.modifiers.shift_key());
                self.env.modifiers = crate::gui::screen::Modifiers {
                    shift: self.input.modifiers.shift_key(),
                    alt: self.input.modifiers.alt_key(),
                    control: self.input.modifiers.control_key(),
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor = position;
                self.cursor_moved();
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => self.mouse_input(button, button_state == ElementState::Pressed, event_loop),
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as f32,
                };
                self.mouse_wheel(lines);
            }
            WindowEvent::KeyboardInput { event, .. } => self.keyboard(&event, event_loop),
            WindowEvent::Ime(Ime::Commit(text)) => self.ime_commit(&text, event_loop),
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.ime_events(vec![TextEvent::Preedit(text)], event_loop);
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime_events(vec![TextEvent::Preedit(String::new())], event_loop);
            }
            WindowEvent::DroppedFile(path) => {
                let ctx = self.env.ctx(self.scale());
                if let Some(screen) = self.screens.top_mut() {
                    let action = screen.on_files_dropped(vec![path], &ctx);
                    self.apply(action, event_loop);
                }
            }
            _ => {}
        }
    }

    /// Escape on a screen that closes on it: its own answer, else close.
    fn escape_top_screen(&mut self, event_loop: &ActiveEventLoop) {
        let ctx = self.env.ctx(self.scale());
        match self.screens.top_mut().and_then(|s| s.on_escape(&ctx)) {
            Some(action) => self.apply(action, event_loop),
            None => self.close_top_screen(),
        }
    }

    fn cursor_moved(&mut self) {
        let (x, y) = self.gui_cursor();
        if self.playing() {
            let ui = self.env.world_context(self.gui_size());
            if let Some(world) = &mut self.world {
                world.mouse_moved(x, y, &ui);
            }
            return;
        }
        if let Some(screen) = self.screens.top_mut() {
            screen.on_mouse_move(x, y);
        }
        self.update_os_cursor();
    }

    fn mouse_input(&mut self, button: MouseButton, pressed: bool, event_loop: &ActiveEventLoop) {
        if button == MouseButton::Left {
            crate::gui::input_state::set_left_held(pressed);
        }
        if self.playing() {
            let kind = match button {
                MouseButton::Left => MouseButtonKind::Left,
                MouseButton::Right => MouseButtonKind::Right,
                MouseButton::Middle => MouseButtonKind::Middle,
                _ => return,
            };
            let ui = self.env.world_context(self.gui_size());
            let Some(world) = &mut self.world else {
                return;
            };
            let bind = InputBind::from_mouse(button);
            let action = world.mouse_button(kind, pressed, bind.as_ref(), &ui);
            self.apply(action, event_loop);
            return;
        }
        if pressed && self.screens.capturing_bind() {
            if let Some(bind) = InputBind::from_mouse(button) {
                let scale = self.scale();
                let action = self
                    .screens
                    .top_mut()
                    .map(|screen| screen.on_bind(bind, &self.env.ctx(scale)))
                    .unwrap_or(crate::gui::screen::ScreenAction::None);
                self.apply(action, event_loop);
            }
            return;
        }
        if !matches!(button, MouseButton::Left | MouseButton::Right) {
            return;
        }
        let (x, y) = self.gui_cursor();
        let scale = self.scale();
        let Some(screen) = self.screens.top_mut() else {
            return;
        };
        if !pressed {
            if button == MouseButton::Left {
                screen.on_mouse_up(x, y, &self.env.ctx(scale));
            }
            return;
        }
        let action = screen.on_click(x, y, &self.env.ctx(scale));
        self.apply(action, event_loop);
    }

    fn mouse_wheel(&mut self, lines: f32) {
        if self.playing() {
            let ui = self.env.world_context(self.gui_size());
            if let Some(world) = &mut self.world {
                world.scroll(lines, &ui);
            }
            return;
        }
        if let Some(screen) = self.screens.top_mut() {
            screen.on_scroll(lines);
        }
    }

    /// Hands a key to the world (when no screen pauses it) and acts on what
    fn world_key(
        &mut self,
        key: &Key,
        bind: Option<&InputBind>,
        pressed: bool,
        text: &[TextEvent],
        event_loop: &ActiveEventLoop,
    ) {
        let ui = self.env.world_context(self.gui_size());
        let Some(world) = &mut self.world else {
            return;
        };
        let action = world.key(key, bind, pressed, text, &ui);
        self.apply(action, event_loop);
    }

    /// Lets the window take composed text where text can be typed (a screen
    pub(super) fn sync_ime(&mut self) {
        let want = !self.playing();
        if want != self.input.ime_allowed {
            self.input.ime_allowed = want;
            self.window.set_ime_allowed(want);
        }
    }

    /// `Ime::Commit`: composed text, typed like keys of those characters
    fn ime_commit(&mut self, text: &str, event_loop: &ActiveEventLoop) {
        let mut events = vec![TextEvent::Preedit(String::new())];
        events.extend(
            text.chars()
                .filter(|c| !c.is_control())
                .map(TextEvent::Char),
        );
        self.ime_events(events, event_loop);
    }

    /// What an input method typed, to the world's overlay (the chat) or
    fn ime_events(&mut self, events: Vec<TextEvent>, event_loop: &ActiveEventLoop) {
        if self.world.is_some() && !self.paused() {
            self.world_key(
                &Key::Unidentified(NativeKey::Unidentified),
                None,
                true,
                &events,
                event_loop,
            );
            return;
        }
        let scale = self.scale();
        for text_event in events {
            let Some(screen) = self.screens.top_mut() else {
                return;
            };
            let action = screen.on_text_event(text_event, &self.env.ctx(scale));
            if !matches!(action, crate::gui::screen::ScreenAction::None) {
                self.apply(action, event_loop);
                return;
            }
        }
    }

    /// After the frame: the input method's candidate window goes beside the
    pub(super) fn sync_ime_area(&mut self) {
        let Some((x, y, height)) = crate::gui::ime::take_cursor() else {
            return;
        };
        if !self.input.ime_allowed {
            return;
        }
        let scale = self.scale();
        let (px, py) = scale.to_physical((x, y));
        let (_, below) = scale.to_physical((0.0, y + height));
        let at = (px as i32, py as i32);
        if self.input.ime_area != Some(at) {
            self.input.ime_area = Some(at);
            self.window.set_ime_cursor_area(
                PhysicalPosition::new(px, py),
                winit::dpi::PhysicalSize::new(1.0, (below - py).max(1.0)),
            );
        }
    }

    /// `Minecraft.pauseIfInactive`: half a second without the focus pauses a
    /// world that nothing else covers.
    pub(super) fn pause_if_inactive(&mut self) {
        let now = Instant::now();
        if self.input.focused || !self.env.config.pause_on_lost_focus {
            self.input.last_active = now;
            return;
        }
        if self.world.is_some()
            && self.screens.top().is_none()
            && !self.overlay_open()
            && now.duration_since(self.input.last_active) > std::time::Duration::from_millis(500)
        {
            self.open_pause_menu();
        }
    }

    fn keyboard(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        let pressed = event.state == ElementState::Pressed;
        if pressed && !event.repeat && matches!(event.logical_key, Key::Named(NamedKey::F11)) {
            self.toggle_fullscreen();
            return;
        }
        let escape = matches!(event.logical_key, Key::Named(NamedKey::Escape));
        let bind = match event.physical_key {
            PhysicalKey::Code(code) => InputBind::from_key_code(code),
            PhysicalKey::Unidentified(_) => None,
        };
        // `Minecraft.handleGlobalKeyPress`: a screenshot from anywhere, but
        // not while a key is being bound.
        if pressed
            && !event.repeat
            && !self.screens.capturing_bind()
            && bind.as_ref().is_some_and(|bind| {
                self.env
                    .config
                    .input
                    .binds
                    .actions_for(bind)
                    .any(|action| action == crate::settings::input::Action::Screenshot)
            })
        {
            self.screenshots.request();
            return;
        }
        let text = if pressed {
            text_events(
                event,
                self.input.modifiers.shift_key(),
                self.input.modifiers.control_key(),
            )
        } else {
            Vec::new()
        };
        if self.world.is_some() {
            if pressed && escape {
                if self.paused() {
                    if self.screens.capturing_bind() {
                        let scale = self.scale();
                        let action = self
                            .screens
                            .top_mut()
                            .map(|screen| {
                                screen.on_bind(InputBind::unknown(), &self.env.ctx(scale))
                            })
                            .unwrap_or(crate::gui::screen::ScreenAction::None);
                        self.apply(action, event_loop);
                    } else if self.screens.top_closes_on_escape() {
                        self.escape_top_screen(event_loop);
                    }
                } else {
                    let ui = self.env.world_context(self.gui_size());
                    let taken = self.world.as_mut().is_some_and(|world| world.escape(&ui));
                    if !taken {
                        self.open_pause_menu();
                    }
                }
                return;
            }
            if !self.paused() {
                self.world_key(
                    &event.logical_key,
                    bind.as_ref(),
                    pressed,
                    &text,
                    event_loop,
                );
                return;
            }
            if !pressed {
                self.world_key(&event.logical_key, bind.as_ref(), false, &[], event_loop);
            }
        }
        if !pressed {
            return;
        }
        if self.screens.capturing_bind() {
            let captured = if escape {
                InputBind::unknown()
            } else {
                bind.unwrap_or_else(InputBind::unknown)
            };
            let scale = self.scale();
            let action = self
                .screens
                .top_mut()
                .map(|screen| screen.on_bind(captured, &self.env.ctx(scale)))
                .unwrap_or(crate::gui::screen::ScreenAction::None);
            self.apply(action, event_loop);
            return;
        }
        if escape && self.screens.top_closes_on_escape() {
            self.escape_top_screen(event_loop);
            return;
        }
        let scale = self.scale();
        for text_event in text {
            let Some(screen) = self.screens.top_mut() else {
                return;
            };
            let action = screen.on_text_event(text_event, &self.env.ctx(scale));
            if !matches!(action, crate::gui::screen::ScreenAction::None) {
                self.apply(action, event_loop);
                return;
            }
        }
    }
}
