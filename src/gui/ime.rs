//! Where the text cursor was drawn this frame, for the input method.

use std::cell::Cell;

thread_local! {
    static CURSOR: Cell<Option<(f32, f32, f32)>> = const { Cell::new(None) };
}

/// A text field's cursor at `(x, y)` in GUI pixels, `height` tall.
pub fn note_cursor(x: f32, y: f32, height: f32) {
    CURSOR.with(|cursor| cursor.set(Some((x, y, height))));
}

/// The cursor noted since the last call, if any text field had one.
pub fn take_cursor() -> Option<(f32, f32, f32)> {
    CURSOR.with(Cell::take)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_cursor_noted_is_taken_once() {
        assert_eq!(take_cursor(), None);
        note_cursor(1.0, 2.0, 9.0);
        note_cursor(5.0, 6.0, 9.0);
        assert_eq!(take_cursor(), Some((5.0, 6.0, 9.0)));
        assert_eq!(take_cursor(), None);
    }
}
