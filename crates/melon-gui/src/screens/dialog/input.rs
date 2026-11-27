//! `InputControlHandlers`: an element for each kind of dialog input, each
//! giving its value as the actions take it.

use pumpkin_nbt::tag::NbtTag;

use super::element::{Element, Press};
use crate::component::Component;
use crate::dialog::action::escape_without_quotes;
use crate::dialog::input::{
    BooleanInput, NumberRangeInput, SingleOptionInput, TextInput, value_to_string,
};
use crate::dialog::{Input, InputControl, InputValue};
use crate::gui::screen::{CursorKind, ScreenContext, TextEvent};
use crate::gui::text::{self, StyledText};
use crate::gui::widgets::button::Button;
use crate::gui::widgets::checkbox::Checkbox;
use crate::gui::widgets::edit_box::EditBox;
use crate::gui::widgets::multiline_edit::MultiLineEditBox;
use crate::gui::widgets::slider::Slider;
use crate::gui::widgets::{Rect, Ui, Widget};

/// `InputControlHandlers.createHandler`: the element for `input`.
pub fn element(input: &Input, ctx: &ScreenContext) -> Box<dyn Element> {
    match &input.control {
        InputControl::Text(text) => Box::new(TextField::new(text, ctx)),
        InputControl::Boolean(boolean) => Box::new(BooleanBox::new(boolean, ctx)),
        InputControl::SingleOption(options) => Box::new(OptionCycle::new(options, ctx)),
        InputControl::NumberRange(range) => Box::new(RangeSlider::new(range, ctx)),
    }
}

/// `CommonLayouts.LABEL_SPACING`.
const LABEL_SPACING: f32 = 4.0;

/// One line or several.
enum TextBox {
    Line(EditBox),
    Lines(MultiLineEditBox),
}

impl TextBox {
    fn widget(&self) -> &dyn Widget {
        match self {
            Self::Line(field) => field,
            Self::Lines(field) => field,
        }
    }

    fn widget_mut(&mut self) -> &mut dyn Widget {
        match self {
            Self::Line(field) => field,
            Self::Lines(field) => field,
        }
    }
}

/// `TextInputHandler`: an `EditBox` or a `MultiLineEditBox`, under its label
/// when that is visible (`CommonLayouts.labeledElement`).
pub struct TextField {
    label: Option<StyledText>,
    field: TextBox,
    rect: Rect,
    dragging: bool,
}

impl TextField {
    fn new(input: &TextInput, ctx: &ScreenContext) -> Self {
        let width = input.width as f32;
        let max = input.max_length as usize;
        let field = match input.multiline {
            Some(multiline) => {
                let mut field = MultiLineEditBox::new(
                    width,
                    multiline.box_height() as f32,
                    max,
                    multiline.max_lines.map(|n| n as usize),
                );
                field.set_value(&input.initial, &ctx.assets.font);
                TextBox::Lines(field)
            }
            None => {
                let mut field = EditBox::new(String::new(), width, 20.0);
                field.set_max_length(max);
                field.set_value(&input.initial);
                TextBox::Line(field)
            }
        };
        Self {
            label: input
                .label_visible
                .then(|| StyledText::from_component(&input.label, ctx.lang)),
            field,
            rect: Rect::default(),
            dragging: false,
        }
    }

    fn label_height(&self) -> f32 {
        if self.label.is_some() {
            9.0 + LABEL_SPACING
        } else {
            0.0
        }
    }

    fn text(&self) -> String {
        match &self.field {
            TextBox::Line(field) => field.value(),
            TextBox::Lines(field) => field.value(),
        }
    }
}

impl Element for TextField {
    fn layout(&mut self, ctx: &ScreenContext) -> (f32, f32) {
        let field = self.field.widget().rect();
        let label_w = self
            .label
            .as_ref()
            .map_or(0.0, |label| text::width(&ctx.assets.font, label));
        self.rect.w = field.w.max(label_w);
        self.rect.h = self.label_height() + field.h;
        (self.rect.w, self.rect.h)
    }

    fn place(&mut self, x: f32, y: f32) {
        self.rect.x = x;
        self.rect.y = y;
        let below = y + self.label_height();
        self.field.widget_mut().set_position(x, below);
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&self, ui: &mut Ui, _mouse: (f32, f32)) {
        if let Some(label) = &self.label {
            ui.styled(label, self.rect.x, self.rect.y, text::colors::WHITE);
        }
        self.field.widget().draw(ui);
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.field.widget_mut().update_hover(x, y);
    }

    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        if !self.field.widget().rect().contains(x, y) {
            return if self.rect.contains(x, y) {
                Press::Taken
            } else {
                Press::Missed
            };
        }
        match &mut self.field {
            TextBox::Line(field) => {
                field.click(x, y);
            }
            TextBox::Lines(field) => field.click(x, y, &ctx.assets.font),
        }
        self.dragging = true;
        Press::Taken
    }

    fn drag(&mut self, x: f32, _y: f32) {
        if let (true, TextBox::Line(field)) = (self.dragging, &mut self.field) {
            field.drag(x);
        }
    }

    fn release(&mut self) {
        self.dragging = false;
    }

    fn scroll(&mut self, delta: f32) -> bool {
        match &mut self.field {
            TextBox::Lines(field) if field.focused() => {
                field.scroll_by(delta);
                true
            }
            _ => false,
        }
    }

    fn key(&mut self, event: &TextEvent, ctx: &ScreenContext) -> Press {
        match &mut self.field {
            TextBox::Line(field) => {
                if matches!(event, TextEvent::Enter) {
                    return Press::Missed;
                }
                field.handle(event);
            }
            TextBox::Lines(field) => {
                field.handle(event, &ctx.assets.font);
            }
        }
        Press::Taken
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focused(&mut self, focused: bool) {
        self.field.widget_mut().set_focused(focused);
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.field.widget().cursor()
    }

    /// `asTemplateSubstitution` escaped for SNBT, `asTag` as typed.
    fn value(&self) -> Option<InputValue> {
        let text = self.text();
        Some(InputValue {
            text: escape_without_quotes(&text),
            tag: NbtTag::String(text.into()),
        })
    }
}

/// `BooleanHandler`: a checkbox.
pub struct BooleanBox {
    checkbox: Checkbox,
    on_true: String,
    on_false: String,
}

impl BooleanBox {
    fn new(input: &BooleanInput, ctx: &ScreenContext) -> Self {
        let label = StyledText::from_component(&input.label, ctx.lang);
        Self {
            checkbox: Checkbox::new(&label, input.initial, None, &ctx.assets.font),
            on_true: input.on_true.clone(),
            on_false: input.on_false.clone(),
        }
    }
}

impl Element for BooleanBox {
    fn layout(&mut self, _ctx: &ScreenContext) -> (f32, f32) {
        let rect = self.checkbox.rect();
        (rect.w, rect.h)
    }

    fn place(&mut self, x: f32, y: f32) {
        self.checkbox.set_position(x, y);
    }

    fn rect(&self) -> Rect {
        self.checkbox.rect()
    }

    fn draw(&self, ui: &mut Ui, _mouse: (f32, f32)) {
        self.checkbox.draw(ui);
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.checkbox.update_hover(x, y);
    }

    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        if !self.checkbox.rect().contains(x, y) {
            return Press::Missed;
        }
        ctx.click();
        self.checkbox.toggle();
        Press::Taken
    }

    fn key(&mut self, event: &TextEvent, ctx: &ScreenContext) -> Press {
        if matches!(event, TextEvent::Enter | TextEvent::Char(' ')) {
            ctx.click();
            self.checkbox.toggle();
            return Press::Taken;
        }
        Press::Missed
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focused(&mut self, focused: bool) {
        self.checkbox.set_focused(focused);
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.checkbox.cursor()
    }

    /// `on_true`/`on_false` for a command, a byte for a payload.
    fn value(&self) -> Option<InputValue> {
        let selected = self.checkbox.selected;
        Some(InputValue {
            text: if selected {
                self.on_true.clone()
            } else {
                self.on_false.clone()
            },
            tag: NbtTag::Byte(i8::from(selected)),
        })
    }
}

/// `SingleOptionHandler`: a `CycleButton` through the options, showing
/// "Label: Value" or just the value.
pub struct OptionCycle {
    button: Button,
    ids: Vec<String>,
    /// The options' names, then the button's text for each.
    messages: Vec<StyledText>,
    selected: usize,
}

impl OptionCycle {
    fn new(input: &SingleOptionInput, ctx: &ScreenContext) -> Self {
        let messages = input
            .entries
            .iter()
            .map(|entry| {
                let value = entry.display();
                let message = if input.label_visible {
                    Component::translatable(
                        "options.generic_value",
                        vec![input.label.clone(), value],
                    )
                } else {
                    value
                };
                StyledText::from_component(&message, ctx.lang)
            })
            .collect();
        Self {
            button: Button::sized(String::new(), input.width as f32, 20.0),
            ids: input.entries.iter().map(|e| e.id.clone()).collect(),
            messages,
            selected: input.initial(),
        }
    }

    /// `CycleButton.onPress`: the next option, the one before with Shift.
    fn cycle(&mut self, back: bool) {
        let count = self.ids.len().max(1);
        self.selected = if back {
            (self.selected + count - 1) % count
        } else {
            (self.selected + 1) % count
        };
    }
}

impl Element for OptionCycle {
    fn layout(&mut self, _ctx: &ScreenContext) -> (f32, f32) {
        (self.button.w, self.button.h)
    }

    fn place(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }

    fn rect(&self) -> Rect {
        self.button.rect()
    }

    fn draw(&self, ui: &mut Ui, _mouse: (f32, f32)) {
        if let Some(message) = self.messages.get(self.selected) {
            self.button.draw_styled(ui, message);
        }
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.button.update_hover(x, y);
    }

    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        if !self.button.clicked(x, y) {
            return Press::Missed;
        }
        ctx.click();
        self.cycle(ctx.modifiers.shift);
        Press::Taken
    }

    fn key(&mut self, event: &TextEvent, ctx: &ScreenContext) -> Press {
        if matches!(event, TextEvent::Enter) {
            ctx.click();
            self.cycle(ctx.modifiers.shift);
            return Press::Taken;
        }
        Press::Missed
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focused(&mut self, focused: bool) {
        self.button.set_focused(focused);
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.button.cursor()
    }

    /// The option's id.
    fn value(&self) -> Option<InputValue> {
        Some(InputValue::string(
            self.ids.get(self.selected).cloned().unwrap_or_default(),
        ))
    }
}

/// Fine enough that the handle follows the mouse as vanilla's continuous
/// slider does.
const SLIDER_STEPS: u32 = 10_000;

/// `NumberRangeHandler.SliderImpl`: a slider labelled with its value.
pub struct RangeSlider {
    slider: Slider,
    input: NumberRangeInput,
}

impl RangeSlider {
    fn new(input: &NumberRangeInput, _ctx: &ScreenContext) -> Self {
        let initial = input.range.initial_slider().clamp(0.0, 1.0);
        let step = (initial * SLIDER_STEPS as f32).round() as u32;
        Self {
            slider: Slider::new(SLIDER_STEPS, step, input.width as f32, 20.0),
            input: input.clone(),
        }
    }

    fn fraction(&self) -> f32 {
        self.slider.step() as f32 / SLIDER_STEPS as f32
    }

    fn scaled(&self) -> f32 {
        self.input.range.scaled(self.fraction())
    }

    /// `computeMessage`: the label format with the value.
    fn message(&self, lang: &crate::i18n::Lang) -> StyledText {
        let label = self.input.label_for(&value_to_string(self.scaled()));
        StyledText::from_component(&label, lang)
    }
}

impl Element for RangeSlider {
    fn layout(&mut self, _ctx: &ScreenContext) -> (f32, f32) {
        (self.slider.w, self.slider.h)
    }

    fn place(&mut self, x: f32, y: f32) {
        self.slider.set_position(x, y);
    }

    fn rect(&self) -> Rect {
        self.slider.rect()
    }

    fn draw(&self, ui: &mut Ui, _mouse: (f32, f32)) {
        self.slider.draw_labeled(ui, "");
        let message = self.message(ui.lang);
        let r = self.slider.rect();
        let width = ui.styled_width(&message);
        let x = r.x + ((r.w - width) / 2.0).round();
        let y = r.y + ((r.h - 8.0) / 2.0).round();
        ui.styled(&message, x, y, text::colors::WHITE);
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.slider.update_hover(x, y);
    }

    fn press(&mut self, x: f32, y: f32, _ctx: &ScreenContext) -> Press {
        match self.slider.press(x, y) {
            None => Press::Missed,
            Some(_) => Press::Taken,
        }
    }

    fn drag(&mut self, x: f32, _y: f32) {
        self.slider.drag(x);
    }

    fn release(&mut self) {
        self.slider.release();
    }

    fn focusable(&self) -> bool {
        true
    }

    /// `AbstractSliderButton.keyPressed`: the arrows move it a pixel's worth.
    fn key(&mut self, event: &TextEvent, _ctx: &ScreenContext) -> Press {
        let per_pixel = SLIDER_STEPS as f32 / (self.slider.w - 8.0).max(1.0);
        let step = self.slider.step() as f32;
        let to = match event {
            TextEvent::Left { .. } => step - per_pixel,
            TextEvent::Right { .. } => step + per_pixel,
            _ => return Press::Missed,
        };
        self.slider
            .set_step(to.clamp(0.0, SLIDER_STEPS as f32).round() as u32);
        Press::Taken
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.slider.cursor()
    }

    /// The value as text (whole without a fraction) and as a float.
    fn value(&self) -> Option<InputValue> {
        let value = self.scaled();
        Some(InputValue {
            text: value_to_string(value),
            tag: NbtTag::Float(value),
        })
    }
}
