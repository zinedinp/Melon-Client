//! Vanilla `MultiLineEditBox`: text over several wrapped lines in a text
//! field's frame, scrolled to the cursor, with a character and a line limit.

use std::time::Instant;

use crate::gui::screen::{CursorKind, TextEvent};
use crate::gui::text::font::Font;
use crate::gui::widgets::text_input::cursor_visible;
use crate::gui::widgets::{Rect, Ui, Widget};

/// `AbstractTextAreaWidget.INNER_PADDING`.
const PADDING: f32 = 4.0;
/// `Font.lineHeight`.
const LINE_HEIGHT: f32 = 9.0;
/// `MultiLineEditBox.TEXT_COLOR` (`0xFFE0E0E0`) and `CURSOR_COLOR`.
const TEXT_COLOR: [f32; 4] = [224.0 / 255.0, 224.0 / 255.0, 224.0 / 255.0, 1.0];
const CURSOR_COLOR: [f32; 4] = [208.0 / 255.0, 208.0 / 255.0, 208.0 / 255.0, 1.0];

pub struct MultiLineEditBox {
    chars: Vec<char>,
    /// Character index, `0..=chars.len()`.
    cursor: usize,
    max_chars: usize,
    max_lines: Option<usize>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Pixels scrolled down.
    scroll: f32,
    focused: bool,
    hovered: bool,
    blink_start: Instant,
    /// The displayed lines as of the last edit (the width never changes).
    line_count: usize,
}

impl MultiLineEditBox {
    pub fn new(w: f32, h: f32, max_chars: usize, max_lines: Option<usize>) -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
            max_chars,
            max_lines,
            x: 0.0,
            y: 0.0,
            w,
            h,
            scroll: 0.0,
            focused: false,
            hovered: false,
            blink_start: Instant::now(),
            line_count: 1,
        }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    /// `setValue`: cut to the limits.
    pub fn set_value(&mut self, value: &str, font: &Font) {
        self.chars.clear();
        self.cursor = 0;
        self.insert(value, font);
        self.scroll_to_cursor(font);
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    /// The width text wraps at.
    fn text_width(&self) -> f32 {
        (self.w - PADDING * 2.0).max(1.0)
    }

    /// The displayed lines as character ranges: split at line breaks, then
    /// wrapped at the last space that fits (mid-word when none does).
    fn lines(&self, font: &Font) -> Vec<(usize, usize)> {
        wrap(&self.chars, self.text_width(), |c| font.advance(c))
    }

    /// `MultiLineEditBox.insertText`: what fits under both limits.
    fn insert(&mut self, text: &str, font: &Font) -> bool {
        let mut changed = false;
        for c in text.chars().filter(|&c| c == '\n' || !c.is_control()) {
            if self.chars.len() >= self.max_chars {
                break;
            }
            self.chars.insert(self.cursor, c);
            if self
                .max_lines
                .is_some_and(|max| self.lines(font).len() > max)
            {
                self.chars.remove(self.cursor);
                break;
            }
            self.cursor += 1;
            changed = true;
        }
        changed
    }

    /// The line the cursor is on, and its range.
    fn cursor_line(&self, lines: &[(usize, usize)]) -> usize {
        lines
            .iter()
            .rposition(|&(start, _)| start <= self.cursor)
            .unwrap_or(0)
    }

    /// One key or typed text; whether the value changed.
    pub fn handle(&mut self, event: &TextEvent, font: &Font) -> bool {
        self.blink_start = Instant::now();
        let changed = match event {
            TextEvent::Char(c) => self.insert(&c.to_string(), font),
            TextEvent::Paste(text) => self.insert(text, font),
            TextEvent::Enter => self.insert("\n", font),
            TextEvent::Backspace { .. } if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
                true
            }
            TextEvent::Delete { .. } if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
                true
            }
            TextEvent::Left { .. } => {
                self.cursor = self.cursor.saturating_sub(1);
                false
            }
            TextEvent::Right { .. } => {
                self.cursor = (self.cursor + 1).min(self.chars.len());
                false
            }
            TextEvent::Up | TextEvent::Down => {
                let lines = self.lines(font);
                let line = self.cursor_line(&lines);
                let target = if matches!(event, TextEvent::Up) {
                    line.checked_sub(1)
                } else {
                    Some(line + 1).filter(|&l| l < lines.len())
                };
                if let Some(target) = target {
                    let (start, _) = lines[line];
                    let x = self.width_of(start, self.cursor, font);
                    self.cursor = self.index_at(lines[target], x, font);
                }
                false
            }
            TextEvent::Home { .. } => {
                let lines = self.lines(font);
                self.cursor = lines[self.cursor_line(&lines)].0;
                false
            }
            TextEvent::End { .. } => {
                let lines = self.lines(font);
                self.cursor = lines[self.cursor_line(&lines)].1;
                false
            }
            _ => false,
        };
        self.scroll_to_cursor(font);
        changed
    }

    fn width_of(&self, start: usize, end: usize, font: &Font) -> f32 {
        self.chars[start..end]
            .iter()
            .map(|&c| font.advance(c))
            .sum()
    }

    /// The character on `line` nearest `x` pixels from its start.
    fn index_at(&self, (start, end): (usize, usize), x: f32, font: &Font) -> usize {
        let mut pen = 0.0;
        for (i, &c) in self.chars.iter().enumerate().take(end).skip(start) {
            let advance = font.advance(c);
            if pen + advance / 2.0 > x {
                return i;
            }
            pen += advance;
        }
        end
    }

    /// Scrolls so the cursor's line is inside the box.
    fn scroll_to_cursor(&mut self, font: &Font) {
        let lines = self.lines(font);
        let top = self.cursor_line(&lines) as f32 * LINE_HEIGHT;
        let view = self.h - PADDING * 2.0;
        if top < self.scroll {
            self.scroll = top;
        } else if top + LINE_HEIGHT > self.scroll + view {
            self.scroll = top + LINE_HEIGHT - view;
        }
        self.line_count = lines.len();
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let content = self.line_count as f32 * LINE_HEIGHT;
        let max = (content - (self.h - PADDING * 2.0)).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// The wheel over the box.
    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll -= delta * LINE_HEIGHT * 2.0;
        self.clamp_scroll();
    }

    /// A click inside the box: the cursor to the character under it.
    pub fn click(&mut self, x: f32, y: f32, font: &Font) {
        let lines = self.lines(font);
        let row = ((y - self.y - PADDING + self.scroll) / LINE_HEIGHT)
            .floor()
            .max(0.0) as usize;
        let line = lines[row.min(lines.len() - 1)];
        self.cursor = self.index_at(line, x - self.x - PADDING, font);
        self.blink_start = Instant::now();
    }

    pub fn draw_with(&self, ui: &mut Ui) {
        let assets = ui.assets;
        let frame = if self.focused {
            assets.text_field_highlighted
        } else {
            assets.text_field
        };
        frame.draw(ui.draw_list, self.x, self.y, self.w, self.h);
        let font = &assets.font;
        let lines = self.lines(font);
        let inner = Rect::new(self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0);
        ui.clip(inner);
        let cursor_line = self.cursor_line(&lines);
        for (i, &(start, end)) in lines.iter().enumerate() {
            let y = self.y + PADDING + i as f32 * LINE_HEIGHT - self.scroll;
            if y + LINE_HEIGHT < self.y || y > self.y + self.h {
                continue;
            }
            let text: String = self.chars[start..end].iter().collect();
            ui.text(&text, self.x + PADDING, y, TEXT_COLOR);
            if self.focused && i == cursor_line && cursor_visible(self.blink_start) {
                let x = self.x + PADDING + self.width_of(start, self.cursor, font);
                ui.fill(
                    Rect::new(x.round(), y - 1.0, 1.0, LINE_HEIGHT + 1.0),
                    CURSOR_COLOR,
                );
            }
        }
        ui.unclip();
    }
}

/// Lines as character ranges, line breaks excluded.
fn wrap(chars: &[char], width: f32, advance: impl Fn(char) -> f32) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut pen = 0.0;
    let mut last_space = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            lines.push((start, i));
            start = i + 1;
            pen = 0.0;
            last_space = None;
            i += 1;
            continue;
        }
        let w = advance(c);
        if pen + w > width && i > start {
            let end = match last_space {
                Some(space) => space + 1,
                None => i,
            };
            lines.push((start, end));
            start = end;
            pen = chars[start..i].iter().map(|&c| advance(c)).sum();
            last_space = None;
        }
        if c == ' ' {
            last_space = Some(i);
        }
        pen += w;
        i += 1;
    }
    lines.push((start, chars.len()));
    lines
}

impl Widget for MultiLineEditBox {
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    fn set_position(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    fn update_hover(&mut self, x: f32, y: f32) {
        self.hovered = self.rect().contains(x, y);
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.hovered.then_some(CursorKind::Text)
    }

    fn set_focused(&mut self, focused: bool) {
        if focused && !self.focused {
            self.blink_start = Instant::now();
        }
        self.focused = focused;
    }

    fn draw(&self, ui: &mut Ui) {
        self.draw_with(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn lines_break_at_newlines_and_spaces() {
        let text = chars("ab cd\nefgh");
        assert_eq!(wrap(&text, 100.0, |_| 6.0), vec![(0, 5), (6, 10)]);
        assert_eq!(
            wrap(&text, 18.0, |_| 6.0),
            vec![(0, 3), (3, 5), (6, 9), (9, 10)]
        );
        assert_eq!(wrap(&chars(""), 10.0, |_| 6.0), vec![(0, 0)]);
        assert_eq!(wrap(&chars("a\n"), 10.0, |_| 6.0), vec![(0, 1), (2, 2)]);
    }
}
