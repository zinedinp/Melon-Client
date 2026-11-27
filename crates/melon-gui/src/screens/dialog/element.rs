//! What a dialog screen is made of: each body part, input and button is an
//! [`Element`] the screen lays out in a column and hands the mouse and keys
//! to. A new kind of body or input is a new element, made by its handler
//! ([`super::body`], [`super::input`]); the screen needs no change.

use melon_world::item_model::StackKey;
use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;

use crate::screen::{CursorKind, ScreenContext, TextEvent};
use crate::text::{self, Hover, StyledText};
use crate::widgets::button::Button;
use crate::widgets::tooltip::MAX_WIDTH as TOOLTIP_WIDTH;
use crate::widgets::{Rect, Ui, Widget};
use melon_net::dialog::{Action, Dialog, InputValue};
use melon_protocol::component::{ClickEvent, Component};

/// What a click or key on an element asks of the dialog.
pub enum Press {
    /// Not on this element.
    Missed,
    /// On it, handled there.
    Taken,
    /// A link in text: the dialog runs it as a button would.
    Click(ClickEvent),
    /// A button: its action, with the inputs' values.
    Action(Option<Action>),
    /// A `dialog_list`'s button: that dialog.
    Show(Arc<Dialog>),
    /// The warning button.
    Warning,
}

/// One part of a dialog screen.
pub trait Element {
    /// Measures the element for a screen as `ctx` says (text wraps once
    /// here): its width and height.
    fn layout(&mut self, ctx: &ScreenContext) -> (f32, f32);
    fn place(&mut self, x: f32, y: f32);
    fn rect(&self) -> Rect;
    fn draw(&self, ui: &mut Ui, mouse: (f32, f32));

    fn hover(&mut self, _x: f32, _y: f32) {}
    /// The left button went down at `(x, y)`.
    fn press(&mut self, x: f32, y: f32, _ctx: &ScreenContext) -> Press {
        if self.rect().contains(x, y) {
            Press::Taken
        } else {
            Press::Missed
        }
    }
    /// The mouse moved with the button held since a press here.
    fn drag(&mut self, _x: f32, _y: f32) {}
    fn release(&mut self) {}
    /// The wheel over the element; whether it took it.
    fn scroll(&mut self, _delta: f32) -> bool {
        false
    }
    /// A key while focused.
    fn key(&mut self, _event: &TextEvent, _ctx: &ScreenContext) -> Press {
        Press::Missed
    }
    fn focusable(&self) -> bool {
        false
    }
    fn set_focused(&mut self, _focused: bool) {}
    fn cursor(&self) -> Option<CursorKind> {
        None
    }
    /// The tooltip for the mouse at `mouse`, if it shows one.
    fn tooltip(&self, _mouse: (f32, f32), _ui: &Ui) -> Option<Vec<StyledText>> {
        None
    }
    fn visit_stacks(&self, _visit: &mut dyn FnMut(&dyn StackKey) -> bool) {}
    /// An input's value (`Action.ValueGetter`); `None` for the rest.
    fn value(&self) -> Option<InputValue> {
        None
    }
}

/// `Tooltip.create`: the text wrapped as option tooltips are.
pub fn tooltip_lines(component: &Component, ctx: &ScreenContext) -> Vec<StyledText> {
    let text = StyledText::from_component(component, ctx.lang);
    text::wrap(&ctx.assets.font, &text, TOOLTIP_WIDTH)
}

/// `FocusableTextWidget` as dialogs use it (`PlainMessageHandler`): text
/// wrapped to `width` less the padding, centred, its links clickable.
pub struct TextElement {
    text: StyledText,
    width: f32,
    lines: Vec<StyledText>,
    rect: Rect,
}

/// `FocusableTextWidget.DEFAULT_PADDING`.
const TEXT_PADDING: f32 = 4.0;

impl TextElement {
    pub fn new(component: &Component, width: f32, ctx: &ScreenContext) -> Self {
        Self {
            text: StyledText::from_component(component, ctx.lang),
            width,
            lines: Vec::new(),
            rect: Rect::default(),
        }
    }

    fn line_x(&self, ui_font: &crate::text::font::Font, line: &StyledText) -> f32 {
        let line_w = text::width(ui_font, line);
        self.rect.x + ((self.rect.w - line_w) / 2.0).floor()
    }

    /// The run under `(x, y)`, if any.
    fn style_at(&self, font: &crate::text::font::Font, x: f32, y: f32) -> Option<&text::TextStyle> {
        if !self.rect.contains(x, y) {
            return None;
        }
        let row = ((y - self.rect.y - TEXT_PADDING) / 9.0).floor();
        let line = self.lines.get(usize::try_from(row as i64).ok()?)?;
        text::style_at(font, line, x - self.line_x(font, line))
    }
}

impl Element for TextElement {
    fn layout(&mut self, ctx: &ScreenContext) -> (f32, f32) {
        let font = &ctx.assets.font;
        self.lines = text::wrap(font, &self.text, (self.width - TEXT_PADDING * 2.0).max(1.0));
        self.rect.w = self.width;
        self.rect.h = self.lines.len() as f32 * 9.0 + TEXT_PADDING * 2.0;
        (self.rect.w, self.rect.h)
    }

    fn place(&mut self, x: f32, y: f32) {
        self.rect.x = x;
        self.rect.y = y;
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&self, ui: &mut Ui, _mouse: (f32, f32)) {
        let font = ui.font();
        for (i, line) in self.lines.iter().enumerate() {
            let x = self.line_x(font, line);
            let y = self.rect.y + TEXT_PADDING + i as f32 * 9.0;
            ui.styled(line, x, y, text::colors::WHITE);
        }
    }

    /// `setComponentClickHandler`: a link's click event.
    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        if !self.rect.contains(x, y) {
            return Press::Missed;
        }
        match self
            .style_at(&ctx.assets.font, x, y)
            .and_then(|style| style.click.as_deref())
        {
            Some(click) => Press::Click(click.clone()),
            None => Press::Taken,
        }
    }

    fn cursor(&self) -> Option<CursorKind> {
        None
    }

    fn tooltip(&self, mouse: (f32, f32), ui: &Ui) -> Option<Vec<StyledText>> {
        let style = self.style_at(ui.font(), mouse.0, mouse.1)?;
        match style.hover.as_deref()? {
            Hover::Text(text) => Some(text::wrap(ui.font(), text, (ui.width / 2.0).max(200.0))),
            Hover::Entity(name) => Some(vec![name.clone()]),
            Hover::Item {
                id,
                count,
                components,
            } => Some(
                match crate::hud::item_slot::hover_stack(id, *count, components.as_ref()) {
                    Some(stack) => crate::hud::item_slot::tooltip_lines(ui.lang, &stack),
                    None => vec![StyledText::plain(id)],
                },
            ),
        }
    }
}

/// What a dialog button does when pressed.
#[derive(Clone)]
pub enum ButtonAction {
    /// `ActionButton.action`.
    Action(Option<Action>),
    /// A `dialog_list`'s entry.
    Show(Arc<Dialog>),
}

/// `DialogControlSet.createActionButton`: a button with its styled label,
/// its tooltip and its action.
pub struct ButtonElement {
    button: Button,
    label: StyledText,
    tooltip: Option<Vec<StyledText>>,
    action: ButtonAction,
}

impl ButtonElement {
    pub fn new(
        label: &Component,
        tooltip: Option<&Component>,
        width: u32,
        action: ButtonAction,
        ctx: &ScreenContext,
    ) -> Self {
        Self {
            button: Button::sized(String::new(), width as f32, 20.0),
            label: StyledText::from_component(label, ctx.lang),
            tooltip: tooltip.map(|tooltip| tooltip_lines(tooltip, ctx)),
            action,
        }
    }

    fn pressed(&self) -> Press {
        match &self.action {
            ButtonAction::Action(action) => Press::Action(action.clone()),
            ButtonAction::Show(dialog) => Press::Show(Arc::clone(dialog)),
        }
    }
}

impl Element for ButtonElement {
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
        self.button.draw_styled(ui, &self.label);
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.button.update_hover(x, y);
    }

    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        if !self.button.clicked(x, y) {
            return Press::Missed;
        }
        ctx.click();
        self.pressed()
    }

    fn key(&mut self, event: &TextEvent, ctx: &ScreenContext) -> Press {
        if matches!(event, TextEvent::Enter) {
            ctx.click();
            return self.pressed();
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

    fn tooltip(&self, mouse: (f32, f32), _ui: &Ui) -> Option<Vec<StyledText>> {
        let shown = self.button.hovered || self.button.focused;
        let over = self.button.rect().contains(mouse.0, mouse.1);
        (shown && over).then(|| self.tooltip.clone()).flatten()
    }
}

/// `ItemDisplayWidget`, with the `ItemBody`'s description beside it.
pub struct ItemElement {
    stack: Option<ItemStack>,
    size: (f32, f32),
    decorations: bool,
    show_tooltip: bool,
    description: Option<TextElement>,
    /// The item's own area.
    item_rect: Rect,
    rect: Rect,
}

/// `ItemHandler`'s row: the item and the text 2 apart.
const ITEM_SPACING: f32 = 2.0;

impl ItemElement {
    pub fn new(body: &melon_net::dialog::ItemBody, ctx: &ScreenContext) -> Self {
        let stack =
            crate::hud::item_slot::hover_stack(&body.id, body.count, body.components.as_ref());
        Self {
            stack,
            size: (body.width as f32, body.height as f32),
            decorations: body.show_decorations,
            show_tooltip: body.show_tooltip,
            description: body
                .description
                .as_ref()
                .map(|d| TextElement::new(&d.contents, d.width as f32, ctx)),
            item_rect: Rect::default(),
            rect: Rect::default(),
        }
    }
}

impl Element for ItemElement {
    fn layout(&mut self, ctx: &ScreenContext) -> (f32, f32) {
        let (w, h) = self.size;
        self.item_rect.w = w;
        self.item_rect.h = h;
        let (dw, dh) = self
            .description
            .as_mut()
            .map_or((0.0, 0.0), |d| d.layout(ctx));
        let gap = if self.description.is_some() {
            ITEM_SPACING
        } else {
            0.0
        };
        self.rect.w = w + gap + dw;
        self.rect.h = h.max(dh);
        (self.rect.w, self.rect.h)
    }

    /// `alignVerticallyMiddle`: both centred on the row's height.
    fn place(&mut self, x: f32, y: f32) {
        self.rect.x = x;
        self.rect.y = y;
        self.item_rect.x = x;
        self.item_rect.y = y + ((self.rect.h - self.item_rect.h) / 2.0).floor();
        let item_w = self.item_rect.w;
        let row_h = self.rect.h;
        if let Some(description) = &mut self.description {
            let h = description.rect().h;
            description.place(x + item_w + ITEM_SPACING, y + ((row_h - h) / 2.0).floor());
        }
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&self, ui: &mut Ui, mouse: (f32, f32)) {
        if let (Some(stack), Some(painter)) = (&self.stack, ui.items) {
            let (x, y) = (self.item_rect.x, self.item_rect.y);
            if self.decorations {
                painter.stack(ui.draw_list, stack, x, y);
            } else {
                painter.icon(ui.draw_list, stack, x, y);
            }
        }
        if let Some(description) = &self.description {
            description.draw(ui, mouse);
        }
    }

    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        match &mut self.description {
            Some(description) if description.rect().contains(x, y) => description.press(x, y, ctx),
            _ if self.rect.contains(x, y) => Press::Taken,
            _ => Press::Missed,
        }
    }

    fn tooltip(&self, mouse: (f32, f32), ui: &Ui) -> Option<Vec<StyledText>> {
        if self.show_tooltip
            && self.item_rect.contains(mouse.0, mouse.1)
            && let Some(stack) = &self.stack
        {
            return Some(crate::hud::item_slot::tooltip_lines(ui.lang, stack));
        }
        self.description.as_ref()?.tooltip(mouse, ui)
    }

    fn visit_stacks(&self, visit: &mut dyn FnMut(&dyn StackKey) -> bool) {
        if let Some(stack) = &self.stack {
            visit(stack);
        }
    }
}

/// `DialogScreen.createWarningButton`: the 20px button by the title that
/// explains custom screens.
pub struct WarningElement {
    button: Button,
    tooltip: Vec<StyledText>,
}

impl WarningElement {
    pub fn new(ctx: &ScreenContext) -> Self {
        let tooltip = Component::translatable("menu.custom_screen_info.tooltip", Vec::new());
        Self {
            button: Button::sized("menu.custom_screen_info.button_narration", 20.0, 20.0),
            tooltip: tooltip_lines(&tooltip, ctx),
        }
    }
}

impl Element for WarningElement {
    fn layout(&mut self, _ctx: &ScreenContext) -> (f32, f32) {
        (20.0, 20.0)
    }

    fn place(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }

    fn rect(&self) -> Rect {
        self.button.rect()
    }

    fn draw(&self, ui: &mut Ui, _mouse: (f32, f32)) {
        let sprites = &ui.assets.dialog_warning;
        let sprite = if self.button.hovered || self.button.focused {
            sprites.highlighted
        } else {
            sprites.normal
        };
        sprite.draw(ui.draw_list, self.button.x, self.button.y, 20.0, 20.0);
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.button.update_hover(x, y);
    }

    fn press(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> Press {
        if !self.button.clicked(x, y) {
            return Press::Missed;
        }
        ctx.click();
        Press::Warning
    }

    fn key(&mut self, event: &TextEvent, ctx: &ScreenContext) -> Press {
        if matches!(event, TextEvent::Enter) {
            ctx.click();
            return Press::Warning;
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

    fn tooltip(&self, mouse: (f32, f32), _ui: &Ui) -> Option<Vec<StyledText>> {
        self.button
            .rect()
            .contains(mouse.0, mouse.1)
            .then(|| self.tooltip.clone())
    }
}
