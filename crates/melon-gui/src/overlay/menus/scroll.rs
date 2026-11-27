//! The scroll bar of the stonecutter, the loom and the villager's trade list.

/// A scroll bar over a list that shows `visible_rows` of its rows at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroller {
    offs: f32,
    dragging: bool,
}

impl Scroller {
    /// Rows past the ones shown: how far the list can scroll.
    pub fn offscreen_rows(entries: usize, columns: usize, visible_rows: usize) -> usize {
        entries.div_ceil(columns).saturating_sub(visible_rows)
    }

    pub fn offs(&self) -> f32 {
        self.offs
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Back to the top, nothing grabbed.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn grab(&mut self) {
        self.dragging = true;
    }

    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// The first entry shown, `(int)(offs * offscreen + 0.5) * columns`.
    pub fn start_index(&self, offscreen_rows: usize, columns: usize) -> usize {
        (self.offs * offscreen_rows as f32 + 0.5) as usize * columns
    }

    /// The thumb's offset from the top of its track of `travel` pixels.
    pub fn thumb(&self, travel: f32) -> f32 {
        (travel * self.offs).floor()
    }

    /// The wheel, `lines` up, over a list with `offscreen_rows` rows to scroll.
    pub fn wheel(&mut self, lines: f32, offscreen_rows: usize) {
        if offscreen_rows > 0 {
            self.offs = (self.offs - lines / offscreen_rows as f32).clamp(0.0, 1.0);
        }
    }

    /// The thumb dragged to the mouse at `y`, on a track from `top` that is `track` tall.
    pub fn drag_to(&mut self, y: f32, top: f32, track: f32, thumb: f32) {
        self.offs = ((y - top - thumb / 2.0) / (track - thumb)).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_past_the_visible_ones_round_up() {
        assert_eq!(Scroller::offscreen_rows(12, 4, 3), 0);
        assert_eq!(Scroller::offscreen_rows(13, 4, 3), 1);
        assert_eq!(Scroller::offscreen_rows(2, 4, 3), 0);
    }

    #[test]
    fn the_wheel_moves_a_row_at_a_time_and_stops_at_the_ends() {
        let mut bar = Scroller::default();
        bar.wheel(-1.0, 4);
        assert_eq!(bar.start_index(4, 4), 4);
        bar.wheel(-10.0, 4);
        assert_eq!(bar.start_index(4, 4), 16);
        bar.wheel(100.0, 4);
        assert_eq!(bar.start_index(4, 4), 0);
        bar.wheel(-1.0, 0);
        assert_eq!(bar.offs(), 0.0);
    }

    #[test]
    fn dragging_centres_the_thumb_on_the_mouse() {
        let mut bar = Scroller::default();
        bar.drag_to(14.0 + 7.5, 14.0, 54.0, 15.0);
        assert_eq!(bar.offs(), 0.0);
        bar.drag_to(14.0 + 54.0 - 7.5, 14.0, 54.0, 15.0);
        assert_eq!(bar.offs(), 1.0);
        bar.drag_to(500.0, 14.0, 54.0, 15.0);
        assert_eq!(bar.offs(), 1.0);
        assert_eq!(bar.thumb(41.0), 41.0);
    }
}
