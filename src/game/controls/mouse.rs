//! Mouse buttons, movement and the wheel in a world.

use super::WorldContext;
use crate::game::InWorld;
use melon_gui::overlay::container_input::MouseButtonKind;
use melon_gui::screen::ScreenAction;

impl InWorld {
    /// A mouse button went down or up.
    pub fn mouse_button(
        &mut self,
        button: MouseButtonKind,
        pressed: bool,
        bind: Option<&melon_settings::input::InputBind>,
        ui: &WorldContext,
    ) -> ScreenAction {
        let index = match button {
            MouseButtonKind::Left => 0,
            MouseButtonKind::Right => 1,
            MouseButtonKind::Middle => 2,
        };
        self.keys.buttons[index] = pressed;
        if self.overlay.is_none() {
            if let Some(bind) = bind {
                self.keys.apply_bind(bind, pressed, &ui.config.input);
            }
        } else if !pressed && let Some(bind) = bind {
            self.keys.apply_bind(bind, false, &ui.config.input);
        }
        if let Some(action) = self.with_overlay(ui, |overlay, ctx| {
            overlay.mouse_button(button, pressed, ctx)
        }) {
            return self.apply_overlay_action(action, ui);
        }
        if pressed && button == MouseButtonKind::Left && self.game_mode.current.is_spectator() {
            let targets = self.teleport_targets();
            let action = self.spectator_menu.action_key(targets);
            self.spectator_action(action);
        }
        ScreenAction::None
    }

    /// The mouse moved to `(x, y)` GUI pixels.
    pub fn mouse_moved(&mut self, x: f32, y: f32, ui: &WorldContext) {
        self.keys.mouse = (x, y);
        let dragging = self.keys.buttons.iter().any(|&b| b);
        self.with_overlay(ui, |overlay, ctx| overlay.mouse_moved(x, y, dragging, ctx));
    }

    /// The wheel: the open overlay, else the hotbar.
    pub fn scroll(&mut self, lines: f32, ui: &WorldContext) {
        if self
            .with_overlay(ui, |overlay, ctx| overlay.scroll(lines, ctx))
            .is_none()
        {
            self.scroll_hotbar(lines);
        }
    }
}
