//! The text and cursor behind every text field.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use crate::screen::TextEvent;
use crate::text::font::{Font, Glyphs};
use melon_render::quad::{BlendMode, DrawList};

/// `TextCursorUtils.isCursorVisible`.
pub fn cursor_visible(since: Instant) -> bool {
    (since.elapsed().as_millis() / 300).is_multiple_of(2)
}

/// `MouseHandler.DOUBLE_CLICK_THRESHOLD_MS`.
const DOUBLE_CLICK: Duration = Duration::from_millis(250);

/// What one [`TextEvent`] did to a [`TextInput`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edit {
    /// The text changed (re-run a search filter, ask for completions).
    Changed,
    /// Only the cursor moved.
    Moved,
    /// The event isn't a text edit (Enter, Tab, Up, ...).
    Ignored,
}

pub struct TextInput {
    chars: Vec<char>,
    /// Character index, `0..=chars.len()`.
    cursor: usize,
    /// `EditBox.highlightPos`.
    highlight: usize,
    /// `EditBox.setMaxLength`, in characters.
    max_len: usize,
    /// When the cursor last restarted its blink: the last edit or focus.
    blink_start: Instant,
    /// `EditBox.displayPos`.
    display_start: Cell<usize>,
    /// The text an input method is composing at the cursor.
    preedit: String,
    /// The advance of each character shown, from `display_start`, as last drawn.
    advances: RefCell<Vec<f32>>,
    /// Insert was pressed: typing replaces the character after the cursor.
    overwrite: bool,
    /// The last click, for double clicks.
    last_click: Option<Instant>,
}

impl TextInput {
    /// No length limit.
    pub const UNLIMITED: usize = usize::MAX;

    pub fn new(max_len: usize) -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
            highlight: 0,
            max_len,
            blink_start: Instant::now(),
            display_start: Cell::new(0),
            preedit: String::new(),
            advances: RefCell::new(Vec::new()),
            overwrite: false,
            last_click: None,
        }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// `moveCursorTo(pos, false)`: the cursor there, nothing selected.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.move_to(cursor, false);
    }

    /// `EditBox.moveCursorTo`: with `extend` the selection grows to it.
    fn move_to(&mut self, cursor: usize, extend: bool) {
        self.cursor = cursor.min(self.chars.len());
        if !extend {
            self.highlight = self.cursor;
        }
    }

    /// The selection, as a character range; empty when nothing is selected.
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.highlight)..self.cursor.max(self.highlight)
    }

    /// `EditBox.getHighlighted`.
    pub fn selected(&self) -> String {
        let range = self.selection();
        self.slice(range.start, range.end)
    }

    /// `EditBox.setMaxLength`: the text is cut to fit.
    pub fn set_max_len(&mut self, max_len: usize) {
        self.max_len = max_len;
        if self.chars.len() > max_len {
            self.chars.truncate(max_len);
            self.cursor = self.cursor.min(max_len);
            self.highlight = self.highlight.min(max_len);
        }
    }

    /// Replaces the text, cursor at the end. Cut to the maximum length.
    pub fn set_value(&mut self, value: &str) {
        self.chars = value.chars().take(self.max_len).collect();
        self.cursor = self.chars.len();
        self.highlight = self.cursor;
        self.display_start.set(0);
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.highlight = 0;
        self.display_start.set(0);
    }

    /// The first character shown (see [`Self::window`]).
    pub fn display_start(&self) -> usize {
        self.display_start.get()
    }

    /// `EditBox.scrollTo`.
    pub fn window(&self, font: &Font, width: f32) -> (usize, usize) {
        let cursor = self.cursor;
        let mut start = self.display_start.get().min(cursor);
        let mut before: f32 = self.chars[start..cursor]
            .iter()
            .map(|&c| font.advance(c))
            .sum();
        while start < cursor && before > width - 8.0 {
            before -= font.advance(self.chars[start]);
            start += 1;
        }
        self.display_start.set(start);
        let mut advances = self.advances.borrow_mut();
        advances.clear();
        let mut end = start;
        let mut used = 0.0;
        while end < self.chars.len() {
            let w = font.advance(self.chars[end]);
            if used + w > width - 4.0 {
                break;
            }
            advances.push(w);
            used += w;
            end += 1;
        }
        (start, end)
    }

    /// `TextCursorUtils`.
    pub fn draw_cursor(
        &self,
        draw_list: &mut DrawList,
        glyphs: Glyphs<'_>,
        start: usize,
        (x, y): (f32, f32),
        color: [f32; 4],
    ) {
        let Glyphs { font, pages, white } = glyphs;
        let before = self.slice(start, self.cursor.max(start));
        let cursor_x = (x + font.text_width(&before)).round();
        crate::ime::note_cursor(cursor_x, y, 9.0);
        if !self.preedit.is_empty() {
            self.draw_preedit(draw_list, glyphs, (cursor_x, y), color);
            return;
        }
        if !self.cursor_visible() {
            return;
        }
        if self.overwrite && self.cursor < self.chars.len() {
            let width = font.advance(self.chars[self.cursor]) - 1.0;
            draw_list.push_fill(white, cursor_x, y + 8.0, width.max(1.0), 1.0, color);
        } else if self.cursor < self.chars.len() || self.chars.len() >= self.max_len {
            draw_list.push_fill(white, cursor_x, y - 1.0, 1.0, 10.0, color);
        } else {
            font.push_text(draw_list, pages, "_", cursor_x, y, color);
        }
    }

    /// The text being composed, over what follows the cursor.
    fn draw_preedit(
        &self,
        draw_list: &mut DrawList,
        glyphs: Glyphs<'_>,
        (x, y): (f32, f32),
        color: [f32; 4],
    ) {
        let Glyphs { font, pages, white } = glyphs;
        let width = font.text_width(&self.preedit);
        draw_list.push_fill(white, x, y - 1.0, width + 1.0, 10.0, [0.0, 0.0, 0.0, 0.85]);
        font.push_text(draw_list, pages, &self.preedit, x, y, color);
        draw_list.push_fill(white, x, y + 8.0, width, 1.0, color);
        if cursor_visible(self.blink_start) {
            draw_list.push_fill(white, x + width, y - 1.0, 1.0, 10.0, color);
        }
    }

    /// The text being composed (empty when none).
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    /// The text in a box `width` wide at `(x, y)`, scrolled to the cursor.
    pub fn draw(
        &self,
        draw_list: &mut DrawList,
        glyphs: Glyphs<'_>,
        (x, y): (f32, f32),
        width: f32,
        color: [f32; 4],
        focused: bool,
    ) {
        let Glyphs { font, pages, .. } = glyphs;
        let (start, end) = self.window(font, width);
        font.push_text(draw_list, pages, &self.slice(start, end), x, y, color);
        if focused {
            self.draw_cursor(draw_list, glyphs, start, (x, y), color);
        }
        self.draw_selection(draw_list, glyphs, (start, end), (x, y));
    }

    /// `GuiGraphicsExtractor.textHighlight` over the selected part of the text.
    pub fn draw_selection(
        &self,
        draw_list: &mut DrawList,
        glyphs: Glyphs<'_>,
        (start, end): (usize, usize),
        (x, y): (f32, f32),
    ) {
        let Glyphs { font, white, .. } = glyphs;
        let range = self.selection();
        let (from, to) = (range.start.clamp(start, end), range.end.clamp(start, end));
        if from == to {
            return;
        }
        let left = x + font.text_width(&self.slice(start, from));
        let right = x + font.text_width(&self.slice(start, to));
        let (x0, x1) = (left - 1.0, right - 1.0);
        let (top, bottom) = (y - 1.0, y + 10.0);
        draw_list.set_blend(BlendMode::Invert);
        draw_list.push_fill(white, x0, top, x1 - x0, bottom - top, [1.0; 4]);
        draw_list.set_blend(BlendMode::Additive);
        draw_list.push_fill(white, x0, top, x1 - x0, bottom - top, [0.0, 0.0, 1.0, 1.0]);
        draw_list.set_blend(BlendMode::Alpha);
    }

    /// The text between two character indices.
    pub fn slice(&self, start: usize, end: usize) -> String {
        let end = end.min(self.chars.len());
        self.chars[start.min(end)..end].iter().collect()
    }

    /// `EditBox.findClickedPositionInText`: the character a click `x`
    pub fn position_at(&self, x: f32) -> usize {
        let mut at = self.display_start.get();
        let mut used = 0.0;
        for &w in self.advances.borrow().iter() {
            used += w;
            if used > x.floor() {
                break;
            }
            at += 1;
        }
        at.min(self.chars.len())
    }

    /// `EditBox.onClick` at `x` pixels right of where the text starts.
    pub fn click(&mut self, x: f32, shift: bool) {
        let now = Instant::now();
        let double = self
            .last_click
            .is_some_and(|last| now.duration_since(last) < DOUBLE_CLICK);
        self.last_click = Some(now);
        let at = self.position_at(x);
        if double {
            self.move_to(self.word_position_from(at, false), false);
            self.move_to(self.word_position_from(at, true), true);
        } else {
            self.move_to(at, shift);
        }
        self.restart_blink();
    }

    /// `EditBox.onDrag`: the selection follows the mouse.
    pub fn drag(&mut self, x: f32) {
        self.move_to(self.position_at(x), true);
    }

    /// Insert is on: typing replaces what follows the cursor.
    pub fn overwrite(&self) -> bool {
        self.overwrite
    }

    /// The text left of the cursor.
    pub fn before_cursor(&self) -> String {
        self.slice(0, self.cursor)
    }

    pub fn blink_start(&self) -> Instant {
        self.blink_start
    }

    pub fn cursor_visible(&self) -> bool {
        cursor_visible(self.blink_start)
    }

    /// Restarts the blink, so the cursor shows right after a key or focus.
    pub fn restart_blink(&mut self) {
        self.blink_start = Instant::now();
    }

    /// `EditBox.insertText`.
    pub fn insert(&mut self, text: &str) -> bool {
        self.preedit.clear();
        let range = self.selection();
        let mut changed = !range.is_empty();
        let mut overwrite = self.overwrite && range.is_empty();
        self.chars.drain(range.clone());
        self.cursor = range.start;
        for c in text.chars().filter(|c| !c.is_control()) {
            overwrite &= self.cursor < self.chars.len();
            if overwrite {
                changed |= self.chars[self.cursor] != c;
                self.chars[self.cursor] = c;
            } else if self.chars.len() >= self.max_len {
                break;
            } else {
                self.chars.insert(self.cursor, c);
                changed = true;
            }
            self.cursor += 1;
        }
        self.highlight = self.cursor;
        changed
    }

    /// `EditBox.getWordPosition(dir)` from the cursor.
    fn word_position(&self, forward: bool) -> usize {
        self.word_position_from(self.cursor, forward)
    }

    /// `EditBox.getWordPosition(dir, from)`.
    fn word_position_from(&self, from: usize, forward: bool) -> usize {
        let mut at = from.min(self.chars.len());
        let len = self.chars.len();
        if forward {
            at = self.chars[at..]
                .iter()
                .position(|&c| c == ' ')
                .map_or(len, |i| at + i);
            while at < len && self.chars[at] == ' ' {
                at += 1;
            }
        } else {
            while at > 0 && self.chars[at - 1] == ' ' {
                at -= 1;
            }
            while at > 0 && self.chars[at - 1] != ' ' {
                at -= 1;
            }
        }
        at
    }

    /// `EditBox.deleteCharsToPos`.
    fn delete_to(&mut self, to: usize) -> Edit {
        if self.chars.is_empty() {
            return Edit::Moved;
        }
        if self.cursor != self.highlight {
            self.insert("");
            return Edit::Changed;
        }
        let (start, end) = (to.min(self.cursor), to.max(self.cursor));
        if start == end {
            return Edit::Moved;
        }
        self.chars.drain(start..end);
        self.move_to(start, false);
        Edit::Changed
    }

    /// One key or typed character: the editing and cursor keys.
    pub fn handle(&mut self, event: &TextEvent) -> Edit {
        self.restart_blink();
        let changed = |changed: bool| if changed { Edit::Changed } else { Edit::Moved };
        match *event {
            TextEvent::Preedit(ref text) => {
                self.preedit = text.chars().filter(|c| !c.is_control()).collect();
                Edit::Moved
            }
            TextEvent::Char(c) if !c.is_control() => {
                let mut buffer = [0; 4];
                changed(self.insert(c.encode_utf8(&mut buffer)))
            }
            TextEvent::Paste(ref text) => changed(self.insert(text)),
            TextEvent::Backspace { word } => {
                let to = if word {
                    self.word_position(false)
                } else {
                    self.cursor.saturating_sub(1)
                };
                self.delete_to(to)
            }
            TextEvent::Delete { word } => {
                let to = if word {
                    self.word_position(true)
                } else {
                    (self.cursor + 1).min(self.chars.len())
                };
                self.delete_to(to)
            }
            TextEvent::Left { shift, word } => {
                let to = if word {
                    self.word_position(false)
                } else {
                    self.cursor.saturating_sub(1)
                };
                self.move_to(to, shift);
                Edit::Moved
            }
            TextEvent::Right { shift, word } => {
                let to = if word {
                    self.word_position(true)
                } else {
                    self.cursor + 1
                };
                self.move_to(to, shift);
                Edit::Moved
            }
            TextEvent::Home { shift } => {
                self.move_to(0, shift);
                Edit::Moved
            }
            TextEvent::End { shift } => {
                self.move_to(self.chars.len(), shift);
                Edit::Moved
            }
            TextEvent::SelectAll => {
                self.move_to(self.chars.len(), false);
                self.highlight = 0;
                Edit::Moved
            }
            TextEvent::Copy => {
                melon_base::platform::copy_to_clipboard(&self.selected());
                Edit::Moved
            }
            TextEvent::Insert => {
                self.overwrite = !self.overwrite;
                Edit::Moved
            }
            TextEvent::Cut => {
                melon_base::platform::copy_to_clipboard(&self.selected());
                changed(self.insert(""))
            }
            _ => Edit::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_at_the_cursor_and_respects_the_maximum() {
        let mut input = TextInput::new(5);
        assert_eq!(input.handle(&TextEvent::Char('a')), Edit::Changed);
        assert_eq!(
            input.handle(&TextEvent::Paste("bc\nd".into())),
            Edit::Changed
        );
        assert_eq!(input.value(), "abcd");
        input.handle(&TextEvent::Home { shift: false });
        input.handle(&TextEvent::Right {
            shift: false,
            word: false,
        });
        assert_eq!(input.handle(&TextEvent::Char('x')), Edit::Changed);
        assert_eq!(input.value(), "axbcd");
        assert_eq!(input.handle(&TextEvent::Char('y')), Edit::Moved);
        assert_eq!(
            input.handle(&TextEvent::Backspace { word: false }),
            Edit::Changed
        );
        assert_eq!(input.value(), "abcd");
        assert_eq!(input.before_cursor(), "a");
        assert_eq!(input.handle(&TextEvent::Enter), Edit::Ignored);
    }

    #[test]
    fn composed_text_waits_beside_the_value_until_it_is_committed() {
        let mut input = TextInput::new(TextInput::UNLIMITED);
        input.handle(&TextEvent::Char('a'));
        input.handle(&TextEvent::Preedit("zhong".into()));
        assert_eq!(input.preedit(), "zhong");
        assert_eq!(input.value(), "a");
        input.handle(&TextEvent::Preedit("zh\u{7}".into()));
        assert_eq!(input.preedit(), "zh");
        input.handle(&TextEvent::Preedit(String::new()));
        for c in "中".chars() {
            input.handle(&TextEvent::Char(c));
        }
        assert_eq!(input.value(), "a中");
        assert_eq!(input.preedit(), "");
        input.handle(&TextEvent::Preedit("x".into()));
        input.handle(&TextEvent::Char('b'));
        assert_eq!(input.preedit(), "");
    }

    #[test]
    fn insert_overwrites_until_pressed_again() {
        let mut input = TextInput::new(4);
        input.set_value("abcd");
        input.set_cursor(1);
        input.handle(&TextEvent::Insert);
        assert_eq!(input.handle(&TextEvent::Paste("XY".into())), Edit::Changed);
        assert_eq!(input.value(), "aXYd");
        input.handle(&TextEvent::Paste("ZW".into()));
        assert_eq!(input.value(), "aXYZ");
        input.set_cursor(0);
        assert_eq!(input.handle(&TextEvent::Char('a')), Edit::Moved);
        input.handle(&TextEvent::Insert);
        input.set_value("ab");
        input.set_cursor(0);
        input.handle(&TextEvent::Char('x'));
        assert_eq!(input.value(), "xab");
    }

    #[test]
    fn clicks_place_the_cursor_and_double_clicks_take_the_word() {
        let mut input = TextInput::new(TextInput::UNLIMITED);
        input.set_value("say hello");
        *input.advances.borrow_mut() = vec![6.0; 9];
        input.click(13.0, false);
        assert_eq!(input.cursor(), 2);
        assert!(input.selection().is_empty());
        input.last_click = None;
        input.click(500.0, false);
        assert_eq!(input.cursor(), 9);
        input.last_click = None;
        input.click(-3.0, true);
        assert_eq!(input.selected(), "say hello");
        input.last_click = None;
        input.click(25.0, false);
        input.drag(54.0);
        assert_eq!(input.selected(), "hello");
        input.last_click = None;
        input.click(40.0, false);
        input.click(40.0, false);
        assert_eq!(input.selected(), "hello");
    }

    #[test]
    fn selections_words_and_replacing() {
        let left = |shift, word| TextEvent::Left { shift, word };
        let right = |shift, word| TextEvent::Right { shift, word };
        let mut input = TextInput::new(TextInput::UNLIMITED);
        input.set_value("/give  @a diamond");
        input.handle(&left(false, true));
        assert_eq!(input.cursor(), 10);
        input.handle(&left(false, true));
        assert_eq!(input.cursor(), 7);
        input.handle(&right(false, true));
        assert_eq!(input.cursor(), 10);
        input.handle(&right(true, true));
        assert_eq!(input.selected(), "diamond");
        input.handle(&TextEvent::Paste("stone".into()));
        assert_eq!(input.value(), "/give  @a stone");
        assert!(input.selection().is_empty());
        input.handle(&TextEvent::SelectAll);
        assert_eq!(input.selected(), "/give  @a stone");
        assert_eq!(
            input.handle(&TextEvent::Backspace { word: false }),
            Edit::Changed
        );
        assert!(input.is_empty());
        input.set_value("say hello world");
        input.handle(&TextEvent::Backspace { word: true });
        assert_eq!(input.value(), "say hello ");
        input.handle(&TextEvent::Home { shift: true });
        assert_eq!(input.selected(), "say hello ");
        input.handle(&right(false, false));
        assert!(input.selection().is_empty());
    }
}
