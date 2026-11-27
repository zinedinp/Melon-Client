//! What vanilla asks `Minecraft.hasShiftDown()` and `MouseHandler` for in screens.

use std::cell::Cell;

thread_local! {
    static SHIFT: Cell<bool> = const { Cell::new(false) };
    static LEFT_HELD: Cell<bool> = const { Cell::new(false) };
}

pub fn set_shift(down: bool) {
    SHIFT.with(|s| s.set(down));
}

/// `Minecraft.hasShiftDown`.
pub fn shift_down() -> bool {
    SHIFT.with(Cell::get)
}

pub fn set_left_held(held: bool) {
    LEFT_HELD.with(|s| s.set(held));
}

/// The left button is down: a mouse move is a drag.
pub fn left_held() -> bool {
    LEFT_HELD.with(Cell::get)
}
