//! Vanilla `Checkbox`: a 17px box (`widget/checkbox*`) and its label to the
//! right, wrapped when the width is capped.

use crate::gui::screen::CursorKind;
use crate::gui::text::font::Font;
use crate::gui::text::{self, StyledText};
use crate::gui::widgets::{Rect, Ui, Widget};

/// `Checkbox.getBoxSize`: a line and 8.
pub const BOX_SIZE: f32 = 17.0;
/// `Checkbox.SPACING`: between the box and the label.
const SPACING: f32 = 4.0;

pub struct Checkbox {
    pub x: f32,
    pub y: f32,
    w: f32,
    lines: Vec<StyledText>,
    pub selected: bool,
    focused: bool,
    hovered: bool,
}

impl Checkbox {
    /// `Checkbox.builder(label).maxWidth(max_width)`: as wide as the box
    /// and the label, at most `max_width`.
    pub fn new(label: &StyledText, selected: bool, max_width: Option<f32>, font: &Font) -> Self {
        let full = BOX_SIZE + SPACING + text::width(font, label);
        let w = max_width.map_or(full, |max| full.min(max));
        let lines = text::wrap(font, label, (w - BOX_SIZE - SPACING).max(1.0));
        Self {
            x: 0.0,
            y: 0.0,
            w,
            lines,
            selected,
            focused: false,
            hovered: false,
        }
    }

    /// `Checkbox.onPress`.
    pub fn toggle(&mut self) {
        self.selected = !self.selected;
    }

    fn text_height(&self) -> f32 {
        self.lines.len() as f32 * 9.0
    }
}

impl Widget for Checkbox {
    /// `getAdjustedHeight`: the box or the label, the taller.
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, BOX_SIZE.max(self.text_height()))
    }

    fn set_position(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    fn update_hover(&mut self, x: f32, y: f32) {
        self.hovered = self.rect().contains(x, y);
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.hovered.then_some(CursorKind::Pointer)
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// `extractContents`: the box highlighted while focused, the label
    /// beside its middle.
    fn draw(&self, ui: &mut Ui) {
        let sprite = ui.assets.checkbox.get(self.selected, self.focused);
        sprite.draw(ui.draw_list, self.x, self.y, BOX_SIZE, BOX_SIZE);
        let text_x = self.x + BOX_SIZE + SPACING;
        let text_y = self.y + (BOX_SIZE / 2.0).floor() - (self.text_height() / 2.0).floor();
        for (i, line) in self.lines.iter().enumerate() {
            ui.styled(line, text_x, text_y + i as f32 * 9.0, text::colors::WHITE);
        }
    }
}
