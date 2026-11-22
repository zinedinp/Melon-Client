//! Vanilla `AbstractSelectionList` (`ObjectSelectionList`).

use std::time::{Duration, Instant};

use crate::gui::screen::CursorKind;
use crate::gui::widgets::{Rect, Ui};

/// `AbstractScrollArea.SCROLLBAR_WIDTH`.
const SCROLLBAR_W: f32 = 6.0;
/// `AbstractSelectionList.extractItem`: `isFocused() ? -1 : -8355712`.
const OUTLINE_FOCUSED: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const OUTLINE_UNFOCUSED: [f32; 4] = [128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0];
/// A second click on the same row within this long is a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(250);

/// What a click on the list did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListClick {
    /// Missed the list.
    None,
    /// Grabbed the scrollbar.
    Scrollbar,
    /// Selected a row; `double` when it was the second click on it.
    Row { index: usize, double: bool },
}

pub struct ScrollList {
    /// `getRowWidth()` and `itemHeight`.
    row_w: f32,
    row_h: f32,
    /// The viewport, top to bottom, across a screen this wide.
    top: f32,
    bottom: f32,
    /// The area the rows are centred in: `left` to `left + width`.
    left: f32,
    width: f32,
    len: usize,
    scroll: f32,
    /// `AbstractScrollArea.scrolling`: last mouse y while dragging the thumb.
    scrollbar_drag: Option<f32>,
    selected: Option<usize>,
    /// `AbstractSelectionList.isFocused()`.
    focused: bool,
    last_click: Option<(usize, Instant)>,
    mouse: (f32, f32),
}

impl ScrollList {
    pub fn new(row_w: f32, row_h: f32) -> Self {
        Self {
            row_w,
            row_h,
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            width: 0.0,
            len: 0,
            scroll: 0.0,
            scrollbar_drag: None,
            selected: None,
            focused: false,
            last_click: None,
            mouse: (-1.0, -1.0),
        }
    }

    /// `updateSizeAndPosition`: the viewport for the screen's new size.
    pub fn set_bounds(&mut self, top: f32, bottom: f32, width: f32) {
        self.set_area(0.0, top, bottom, width);
    }

    /// [`Self::set_bounds`] for a list that takes only part of the screen.
    pub fn set_area(&mut self, left: f32, top: f32, bottom: f32, width: f32) {
        self.top = top;
        self.bottom = bottom.max(top);
        self.left = left;
        self.width = width;
        self.clamp_scroll();
    }

    pub fn set_row_width(&mut self, w: f32) {
        self.row_w = w.max(1.0);
    }

    /// The number of rows changed. A selection past the end is dropped.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        if self.selected.is_some_and(|i| i >= len) {
            self.selected = None;
        }
        self.clamp_scroll();
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|&i| i < self.len);
    }

    /// The screen's focus moved elsewhere (a text box, a button).
    pub fn blur(&mut self) {
        self.focused = false;
    }

    /// The screen's focus moves onto the list (an arrow key from a text box).
    pub fn focus(&mut self) {
        self.focused = true;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// `AbstractSelectionList.keyPressed`'s Up/Down and Page Up/Down.
    pub fn move_selection(&mut self, delta: isize) -> bool {
        if self.len == 0 {
            return false;
        }
        let last = self.len - 1;
        let next = match self.selected {
            None if delta > 0 => 0,
            None => last,
            Some(i) => i.saturating_add_signed(delta).min(last),
        };
        self.selected = Some(next);
        self.focused = true;
        self.scroll_to_row(next);
        true
    }

    /// Rows a Page Up/Down moves: a viewport's worth, at least one.
    pub fn page_rows(&self) -> isize {
        ((self.viewport_height() / self.row_h) as isize - 1).max(1)
    }

    /// `scrollToEntry`: the least scroll that shows the whole row.
    fn scroll_to_row(&mut self, index: usize) {
        let top = 2.0 + index as f32 * self.row_h;
        if top - 2.0 < self.scroll {
            self.scroll = top - 2.0;
        } else if top + self.row_h + 2.0 > self.scroll + self.viewport_height() {
            self.scroll = top + self.row_h + 2.0 - self.viewport_height();
        }
        self.clamp_scroll();
    }

    pub fn viewport_height(&self) -> f32 {
        self.bottom - self.top
    }

    fn content_height(&self) -> f32 {
        self.len as f32 * self.row_h + 4.0
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_height() - self.viewport_height()).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// `centerScrollOn`.
    pub fn center_on(&mut self, index: usize, viewport_h: f32) {
        let y = index as f32 * self.row_h + self.row_h / 2.0;
        self.scroll = y - viewport_h / 2.0;
        self.clamp_scroll();
    }

    /// `getRowLeft()`.
    pub fn row_left(&self) -> f32 {
        self.left + (self.width / 2.0).floor() - (self.row_w / 2.0).floor()
    }

    fn scrollbar_x(&self) -> f32 {
        self.row_left() + self.row_w + SCROLLBAR_W + 2.0
    }

    /// `AbstractScrollArea.scrollerHeight`: `(int)` before the clamp.
    fn scroller_height(&self) -> f32 {
        let viewport = self.viewport_height();
        ((viewport * viewport) / self.content_height())
            .trunc()
            .clamp(32.0, viewport - 8.0)
    }

    /// `AbstractScrollArea.scrollBarY`, entirely in `int` arithmetic.
    fn scroller_y(&self) -> f32 {
        let max_scroll = self.max_scroll();
        if max_scroll <= 0.0 {
            return self.top;
        }
        let travel = self.viewport_height() - self.scroller_height();
        self.top + (self.scroll.trunc() * travel / max_scroll).trunc().max(0.0)
    }

    /// Row `index`'s area. `repositionEntries` lays entries out from
    /// The top of the list's band.
    pub fn top(&self) -> f32 {
        self.top
    }

    /// The bottom of the list's band.
    pub fn bottom(&self) -> f32 {
        self.bottom
    }

    pub fn row_rect(&self, index: usize) -> Rect {
        let y = self.top + 2.0 - self.scroll.trunc() + index as f32 * self.row_h;
        Rect::new(self.row_left(), y, self.row_w, self.row_h)
    }

    fn row_visible(&self, rect: &Rect) -> bool {
        rect.bottom() >= self.top && rect.y <= self.bottom
    }

    /// The row under `(x, y)`, if any.
    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        if y < self.top || y >= self.bottom || self.len == 0 {
            return None;
        }
        let first = self.row_rect(0);
        if x < first.x || x >= first.right() || y < first.y {
            return None;
        }
        let index = ((y - first.y) / self.row_h) as usize;
        (index < self.len).then_some(index)
    }

    /// `AbstractScrollArea.isOverScrollbar`.
    fn over_scrollbar(&self, x: f32, y: f32) -> bool {
        if self.max_scroll() <= 0.0 {
            return false;
        }
        let scrollbar_x = self.scrollbar_x();
        x >= scrollbar_x && x <= scrollbar_x + SCROLLBAR_W && y >= self.top && y < self.bottom
    }

    /// The mouse moved: follows a scrollbar drag (`mouseDragged`).
    pub fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        let Some(last_y) = self.scrollbar_drag else {
            return;
        };
        let viewport_h = self.viewport_height();
        let max_scroll = self.max_scroll();
        if max_scroll > 0.0 {
            if y < self.top {
                self.scroll = 0.0;
            } else if y > self.bottom {
                self.scroll = max_scroll;
            } else {
                let drag_scale =
                    (max_scroll / (viewport_h - self.scroller_height()).max(1.0)).max(1.0);
                self.scroll = (self.scroll + (y - last_y) * drag_scale).clamp(0.0, max_scroll);
            }
        }
        self.scrollbar_drag = Some(y);
    }

    /// `onRelease`: ends a scrollbar drag.
    pub fn on_mouse_up(&mut self) {
        self.scrollbar_drag = None;
    }

    /// `AbstractScrollArea.defaultSettings(defaultEntryHeight / 2)`.
    pub fn on_scroll(&mut self, delta: f32) {
        self.scroll = (self.scroll - delta * self.row_h / 2.0).clamp(0.0, self.max_scroll());
    }

    /// A click.
    pub fn on_click(&mut self, x: f32, y: f32) -> ListClick {
        if self.over_scrollbar(x, y) {
            self.scrollbar_drag = Some(y);
            return ListClick::Scrollbar;
        }
        let Some(index) = self.row_at(x, y) else {
            self.focused = false;
            return ListClick::None;
        };
        let now = Instant::now();
        let double = self
            .last_click
            .is_some_and(|(row, at)| row == index && now.duration_since(at) < DOUBLE_CLICK);
        self.last_click = Some((index, now));
        self.selected = Some(index);
        self.focused = true;
        ListClick::Row { index, double }
    }

    /// `extractScrollbar`.
    pub fn cursor(&self) -> Option<CursorKind> {
        if self.scrollbar_drag.is_some() {
            Some(CursorKind::ResizeNs)
        } else if self.over_scrollbar(self.mouse.0, self.mouse.1) {
            Some(CursorKind::Pointer)
        } else {
            None
        }
    }

    /// The tiled background, the rows clipped to the viewport, and the scroll bar.
    pub fn draw(&self, ui: &mut Ui, draw_row: impl FnMut(&mut Ui, usize, Rect)) {
        ui.list_background(self.top, self.bottom);
        self.draw_rows(ui, draw_row);
    }

    /// [`Self::draw`] without the background, for lists sharing one.
    pub fn draw_rows(&self, ui: &mut Ui, mut draw_row: impl FnMut(&mut Ui, usize, Rect)) {
        ui.clip(Rect::new(0.0, self.top, ui.width, self.viewport_height()));
        for index in 0..self.len {
            let rect = self.row_rect(index);
            if !self.row_visible(&rect) {
                continue;
            }
            if self.selected == Some(index) {
                let outline = if self.focused {
                    OUTLINE_FOCUSED
                } else {
                    OUTLINE_UNFOCUSED
                };
                ui.fill(rect, outline);
                ui.fill(
                    Rect::new(rect.x + 1.0, rect.y + 1.0, rect.w - 2.0, rect.h - 2.0),
                    [0.0, 0.0, 0.0, 1.0],
                );
            }
            draw_row(ui, index, rect);
        }
        ui.unclip();
        self.draw_scrollbar(ui);
    }

    /// Only while scrollable (vanilla; no disabled sprite).
    fn draw_scrollbar(&self, ui: &mut Ui) {
        if self.max_scroll() <= 0.0 {
            return;
        }
        let x = self.scrollbar_x();
        let assets = ui.assets;
        assets.scroller_background.draw(
            ui.draw_list,
            x,
            self.top,
            SCROLLBAR_W,
            self.viewport_height(),
        );
        assets.scroller.draw(
            ui.draw_list,
            x,
            self.scroller_y(),
            SCROLLBAR_W,
            self.scroller_height(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(len: usize) -> ScrollList {
        let mut list = ScrollList::new(200.0, 20.0);
        list.set_bounds(30.0, 130.0, 400.0);
        list.set_len(len);
        list
    }

    #[test]
    fn arrow_keys_step_the_selection_and_keep_it_in_view() {
        let mut list = list(10);
        assert!(list.move_selection(1));
        assert_eq!(list.selected(), Some(0));
        for _ in 0..8 {
            list.move_selection(1);
        }
        assert_eq!(list.selected(), Some(8));
        assert_eq!(list.row_rect(8).y + 20.0 + 2.0, 130.0);
        list.move_selection(100);
        assert_eq!(list.selected(), Some(9));
        list.move_selection(-100);
        assert_eq!(list.selected(), Some(0));
        assert_eq!(list.row_rect(0).y, 32.0);
        list.select(None);
        list.move_selection(-1);
        assert_eq!(list.selected(), Some(9));
        assert!(!ScrollList::new(200.0, 20.0).move_selection(1));
    }

    #[test]
    fn rows_scroll_and_clamp() {
        let mut list = list(10);
        assert_eq!(list.max_scroll(), 104.0);
        assert_eq!(list.row_at(150.0, 33.0), Some(0));
        assert_eq!(list.row_at(150.0, 53.0), Some(1));
        assert_eq!(list.row_at(50.0, 53.0), None);
        list.on_scroll(-100.0);
        assert_eq!(list.row_rect(0).y, 30.0 + 2.0 - 104.0);
        list.set_len(2);
        assert_eq!(list.max_scroll(), 0.0);
        assert_eq!(list.row_rect(0).y, 32.0);
    }

    #[test]
    fn a_second_click_on_a_row_is_a_double_click() {
        let mut list = list(3);
        assert_eq!(
            list.on_click(150.0, 33.0),
            ListClick::Row {
                index: 0,
                double: false
            }
        );
        assert_eq!(
            list.on_click(150.0, 33.0),
            ListClick::Row {
                index: 0,
                double: true
            }
        );
        assert_eq!(list.selected(), Some(0));
        assert_eq!(list.on_click(150.0, 200.0), ListClick::None);
        list.set_len(0);
        assert_eq!(list.selected(), None);
    }
}
