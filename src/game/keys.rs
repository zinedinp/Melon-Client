//! The keys that steer the player, with vanilla's default bindings.

use crate::network::play::game::PlayerInput;
use crate::settings::input::{Action, InputBind, InputSettings};
use crate::world::player::MoveInput;

/// WASD, Space, Left Shift (`keyShift`, sneak) and Left Control (`keySprint`).
#[derive(Default)]
pub struct Keys {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sprint: bool,
    pub sneak: bool,
    /// `keySaveHotbarActivator` (C) and `keyLoadHotbarActivator` (X).
    pub save_hotbar: bool,
    pub load_hotbar: bool,
    /// `keyPlayerList` (Tab): the player list shows while it is held.
    pub player_list: bool,
    /// F3 held (`keyDebugModifier`), and whether it was used for a combination.
    pub f3_held: bool,
    pub f3_used: bool,
    /// The mouse in GUI pixels, and which buttons are down (left, right, middle).
    pub mouse: (f32, f32),
    pub buttons: [bool; 3],
    pub attack: bool,
    pub use_item: bool,
    pub pick_item: bool,
    /// `KeyMapping.clickCount` waiting for `consumeClick`.
    pub attack_clicks: u32,
    pub use_clicks: u32,
    pub pick_clicks: u32,
}

impl Keys {
    /// `KeyboardInput.tick`: the held keys as this tick's movement input.
    pub fn movement(&self) -> MoveInput {
        MoveInput {
            forward: impulse(self.forward, self.back),
            strafe: impulse(self.right, self.left),
            jump: self.jump,
            sprint: self.sprint,
            sneak: self.sneak,
        }
    }

    /// `KeyMapping.set` / `click` for every action mapped to `bind`.
    pub fn apply_bind(&mut self, bind: &InputBind, pressed: bool, input: &InputSettings) {
        for action in input.binds.actions_for(bind) {
            self.set_action(action, pressed, input);
        }
    }

    fn set_action(&mut self, action: Action, pressed: bool, input: &InputSettings) {
        let toggle = action.is_toggle()
            && match action {
                Action::Sneak => input.toggle_crouch,
                Action::Sprint => input.toggle_sprint,
                _ => false,
            };
        if toggle {
            if pressed {
                match action {
                    Action::Sneak => self.sneak = !self.sneak,
                    Action::Sprint => self.sprint = !self.sprint,
                    _ => {}
                }
            }
            return;
        }
        let slot = match action {
            Action::Forward => &mut self.forward,
            Action::Back => &mut self.back,
            Action::Left => &mut self.left,
            Action::Right => &mut self.right,
            Action::Jump => &mut self.jump,
            Action::Sprint => &mut self.sprint,
            Action::Sneak => &mut self.sneak,
            Action::SaveHotbar => &mut self.save_hotbar,
            Action::LoadHotbar => &mut self.load_hotbar,
            Action::PlayerList => &mut self.player_list,
            Action::Attack => &mut self.attack,
            Action::Use => &mut self.use_item,
            Action::PickItem => &mut self.pick_item,
            _ => return,
        };
        *slot = pressed;
        if pressed {
            match action {
                Action::Attack => self.attack_clicks += 1,
                Action::Use => self.use_clicks += 1,
                Action::PickItem => self.pick_clicks += 1,
                _ => {}
            }
        }
    }

    /// `KeyMapping.consumeClick`.
    pub fn consume_attack(&mut self) -> bool {
        consume(&mut self.attack_clicks)
    }

    pub fn consume_use(&mut self) -> bool {
        consume(&mut self.use_clicks)
    }

    /// An attack click waits, without taking it.
    pub fn attack_clicked(&self) -> bool {
        self.attack_clicks > 0
    }

    pub fn use_clicked(&self) -> bool {
        self.use_clicks > 0
    }

    pub fn consume_pick(&mut self) -> bool {
        consume(&mut self.pick_clicks)
    }

    /// `MouseHandler.releaseMouse` while a screen opens.
    pub fn clear_clicks(&mut self) {
        self.attack_clicks = 0;
        self.use_clicks = 0;
        self.pick_clicks = 0;
        self.attack = false;
        self.use_item = false;
        self.pick_item = false;
        self.player_list = false;
    }
}

fn consume(clicks: &mut u32) -> bool {
    if *clicks == 0 {
        return false;
    }
    *clicks -= 1;
    true
}

/// `KeyboardInput.calculateImpulse`: `+1`, `-1`, or `0` for both or neither.
fn impulse(positive: bool, negative: bool) -> f32 {
    f32::from(i8::from(positive) - i8::from(negative))
}

/// `ServerboundPlayerInputPacket`'s flags for a movement input.
pub fn player_input(input: MoveInput) -> PlayerInput {
    PlayerInput {
        forward: input.forward > 0.0,
        backward: input.forward < 0.0,
        left: input.strafe < 0.0,
        right: input.strafe > 0.0,
        jump: input.jump,
        sneak: input.sneak,
        sprint: input.sprint,
    }
}

pub use crate::gui::overlay::container_input::hotbar_key;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::input::InputBind;

    #[test]
    fn clear_clicks_drops_leftover_attack() {
        let mut keys = Keys::default();
        let input = InputSettings::default();
        keys.apply_bind(&InputBind::new("key.mouse.left"), true, &input);
        assert!(keys.attack && keys.attack_clicks == 1);
        keys.clear_clicks();
        assert!(!keys.attack && keys.attack_clicks == 0);
        assert!(!keys.consume_attack());
    }
}
