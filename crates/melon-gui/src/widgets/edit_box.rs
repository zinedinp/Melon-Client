//! Vanilla `EditBox`.

use crate::screen::{CursorKind, TextEvent};
use crate::widgets::text_input::{Edit, TextInput};
use crate::widgets::{Label, Rect, Ui, Widget};

/// `EditBox.DEFAULT_TEXT_COLOR` (`-2039584` = `0xFFE0E0E0`).
const TEXT_COLOR: [f32; 4] = [
    0xE0 as f32 / 255.0,
    0xE0 as f32 / 255.0,
    0xE0 as f32 / 255.0,
    1.0,
];
/// `ChatFormatting.GRAY` + `ITALIC`, `EditBox.SEARCH_HINT_STYLE`.
const SEARCH_HINT_COLOR: [f32; 4] = [170.0 / 255.0, 170.0 / 255.0, 170.0 / 255.0, 1.0];
/// `ChatFormatting.DARK_GRAY`, `EditBox.DEFAULT_HINT_STYLE`.
const DEFAULT_HINT_COLOR: [f32; 4] = [85.0 / 255.0, 85.0 / 255.0, 85.0 / 255.0, 1.0];

pub struct EditBox {
    input: TextInput,
    pub hint: Label,
    /// `EditBox.SEARCH_HINT_STYLE` (gray+italic) vs `DEFAULT_HINT_STYLE` (dark gray).
    search_style: bool,
    /// Screen's single focused widget (`AbstractContainerEventHandler.setFocused`).
    focused: bool,
    hovered: bool,
    /// `EditBox.setTextColor`; the default colour when unset.
    text_color: Option<[f32; 4]>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl EditBox {
    pub fn new(hint: impl Into<Label>, w: f32, h: f32) -> Self {
        Self::build(hint, w, h, false)
    }

    /// A search box (`EditBox.SEARCH_HINT_STYLE`).
    pub fn new_search(hint: impl Into<Label>, w: f32, h: f32) -> Self {
        Self::build(hint, w, h, true)
    }

    fn build(hint: impl Into<Label>, w: f32, h: f32, search_style: bool) -> Self {
        Self {
            input: TextInput::new(TextInput::UNLIMITED),
            hint: hint.into(),
            search_style,
            focused: false,
            hovered: false,
            text_color: None,
            x: 0.0,
            y: 0.0,
            w,
            h,
        }
    }

    /// `AbstractContainerEventHandler.setFocused`.
    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        if focused && !self.focused {
            self.input.restart_blink();
        }
        self.focused = focused;
    }

    pub fn value(&self) -> String {
        self.input.value()
    }

    /// `EditBox.setMaxLength`, in characters.
    pub fn set_max_length(&mut self, max: usize) {
        self.input.set_max_len(max);
    }

    /// Preset an initial value (e.g. an existing world's name when editing).
    pub fn set_value(&mut self, value: &str) {
        self.input.set_value(value);
    }

    pub fn contains(&self, mx: f32, my: f32) -> bool {
        Widget::contains(self, mx, my)
    }

    /// `EditBox.onClick`.
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        let hit = self.contains(x, y);
        self.set_focused(hit);
        if hit {
            let shift = crate::input_state::shift_down();
            self.input.click(x - self.text_x(), shift);
        }
        hit
    }

    /// `EditBox.onDrag`: the left button held after a click on the box.
    pub fn drag(&mut self, x: f32) {
        self.input.drag(x - self.text_x());
    }

    /// `EditBox.textX`: where the text starts.
    fn text_x(&self) -> f32 {
        self.x + 4.0
    }

    /// One `TextEvent`. `true` if the value changed (re-run the search filter).
    pub fn set_text_color(&mut self, color: Option<[f32; 4]>) {
        self.text_color = color;
    }

    pub fn handle(&mut self, event: &TextEvent) -> bool {
        self.input.handle(event) == Edit::Changed
    }

    pub fn draw(&self, ui: &mut Ui) {
        Widget::draw(self, ui);
    }
}

impl Widget for EditBox {
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

    /// `EditBox`: an I-beam over the box.
    fn cursor(&self) -> Option<CursorKind> {
        self.hovered.then_some(CursorKind::Text)
    }

    fn set_focused(&mut self, focused: bool) {
        EditBox::set_focused(self, focused);
    }

    fn draw(&self, ui: &mut Ui) {
        let assets = ui.assets;
        let sprite = if self.focused {
            assets.text_field_highlighted
        } else {
            assets.text_field
        };
        sprite.draw(ui.draw_list, self.x, self.y, self.w, self.h);

        let text_x = self.text_x();
        let text_y = (self.y + (self.h - 8.0) / 2.0).round();
        if self.input.is_empty() && !self.focused {
            let hint = self.hint.resolve(ui.lang);
            if self.search_style {
                assets.font.push_text_italic(
                    ui.draw_list,
                    &assets.font_textures,
                    hint,
                    text_x,
                    text_y,
                    SEARCH_HINT_COLOR,
                );
            } else {
                ui.text(hint, text_x, text_y, DEFAULT_HINT_COLOR);
            }
        } else {
            self.input.draw(
                ui.draw_list,
                assets.glyphs(),
                (text_x, text_y),
                self.w,
                self.text_color.unwrap_or(TEXT_COLOR),
                self.focused,
            );
        }
    }
}
