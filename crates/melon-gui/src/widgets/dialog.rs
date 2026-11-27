//! Vanilla `ConfirmScreen`/`DisconnectedScreen`.

use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::text::font::LINE_HEIGHT;
use crate::gui::widgets::button::Button;
use crate::gui::widgets::layout::{BUTTON_H, SPACING};
use crate::gui::widgets::set::{KeyResult, WidgetSet};
use crate::gui::widgets::{Label, Ui};

/// The gap between the buttons of the row.
const BUTTON_GAP: f32 = 4.0;
/// `MultiLineLabel`'s width: the screen's less this.
const WRAP_MARGIN: f32 = 50.0;

/// What a button does when clicked.
pub type Choice = Box<dyn FnMut(&ScreenContext) -> ScreenAction>;

pub struct DialogScreen {
    paragraphs: Vec<(Label, [f32; 4])>,
    /// The paragraphs wrapped for the current width and language.
    lines: Vec<(String, [f32; 4], f32)>,
    /// The buttons, by their index in `choices`.
    widgets: WidgetSet<usize>,
    choices: Vec<Choice>,
    width: f32,
}

impl DialogScreen {
    pub fn new() -> Self {
        Self {
            paragraphs: Vec::new(),
            lines: Vec::new(),
            widgets: WidgetSet::new(),
            choices: Vec::new(),
            width: 0.0,
        }
    }

    /// Adds a paragraph of text.
    pub fn line(mut self, text: impl Into<Label>, color: [f32; 4]) -> Self {
        self.paragraphs.push((text.into(), color));
        self
    }

    /// Adds a button to the row.
    pub fn button(
        mut self,
        label: impl Into<Label>,
        width: f32,
        choice: impl FnMut(&ScreenContext) -> ScreenAction + 'static,
    ) -> Self {
        let index = self.choices.len();
        self.widgets = self
            .widgets
            .with_button(index, Button::sized(label, width, BUTTON_H));
        self.choices.push(Box::new(choice));
        self
    }

    fn choose(&mut self, index: usize, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        (self.choices[index])(ctx)
    }
}

impl Default for DialogScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for DialogScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.width = width;
        let font = &ctx.assets.font;
        let wrapped: Vec<Vec<String>> = self
            .paragraphs
            .iter()
            .map(|(text, _)| font.wrap(text.resolve(ctx.lang), width - WRAP_MARGIN))
            .collect();
        let text_h: f32 = wrapped
            .iter()
            .map(|lines| lines.len() as f32 * LINE_HEIGHT + SPACING)
            .sum();
        let mut y = ((height - text_h - BUTTON_H) / 2.0).floor();
        self.lines.clear();
        for (lines, &(_, color)) in wrapped.into_iter().zip(&self.paragraphs) {
            for line in lines {
                self.lines.push((line, color, y));
                y += LINE_HEIGHT;
            }
            y += SPACING;
        }
        let ids: Vec<usize> = (0..self.choices.len()).collect();
        self.widgets.centered_row(&ids, width, y, BUTTON_GAP);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.click(x, y) {
            Some(index) => self.choose(index, ctx),
            None => ScreenAction::None,
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(index) => self.choose(index, ctx),
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let center = self.width / 2.0;
        for (text, color, y) in &self.lines {
            ui.centered(text, center, *y, *color);
        }
        self.widgets.draw(ui);
    }
}
