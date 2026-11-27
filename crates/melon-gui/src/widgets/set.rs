//! A screen's buttons and text fields by id, handled in one place.

use crate::screen::{CursorKind, TextEvent};
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout;
use crate::widgets::{Rect, Ui, Widget};

/// One widget of a [`WidgetSet`].
pub enum Control {
    Button(Button),
    Field(EditBox),
}

impl Control {
    fn widget(&self) -> &dyn Widget {
        match self {
            Control::Button(button) => button,
            Control::Field(field) => field,
        }
    }

    fn widget_mut(&mut self) -> &mut dyn Widget {
        match self {
            Control::Button(button) => button,
            Control::Field(field) => field,
        }
    }

    /// Whether Tab may move the focus here.
    fn focusable(&self) -> bool {
        match self {
            Control::Button(button) => button.enabled,
            Control::Field(_) => true,
        }
    }
}

/// What a key did to a [`WidgetSet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyResult<Id> {
    /// The set took the key.
    Taken,
    /// Enter on the focused button: press it.
    Pressed(Id),
    /// The focused field's text changed.
    Edited(Id),
    /// Not the set's: Enter with no button focused.
    Ignored,
}

pub struct WidgetSet<Id> {
    items: Vec<(Id, Control)>,
    /// Per item: hidden widgets (another tab's) take no input and aren't drawn.
    hidden: Vec<bool>,
    /// Index into `items` of the focused widget.
    focus: Option<usize>,
    /// The field the left button went down on, while it is held.
    pressed: Option<usize>,
}

impl<Id> Default for WidgetSet<Id> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            hidden: Vec::new(),
            focus: None,
            pressed: None,
        }
    }
}

impl<Id: Copy + PartialEq + std::fmt::Debug> WidgetSet<Id> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a button; Tab visits widgets in the order they were added.
    pub fn with_button(mut self, id: Id, button: Button) -> Self {
        self.items.push((id, Control::Button(button)));
        self.hidden.push(false);
        self
    }

    /// Adds a text field.
    pub fn with_field(mut self, id: Id, field: EditBox) -> Self {
        self.items.push((id, Control::Field(field)));
        self.hidden.push(false);
        self
    }

    fn index(&self, id: Id) -> usize {
        self.items
            .iter()
            .position(|(i, _)| *i == id)
            .unwrap_or_else(|| panic!("no widget {id:?} in the set"))
    }

    pub fn widget(&self, id: Id) -> &dyn Widget {
        self.items[self.index(id)].1.widget()
    }

    pub fn widget_mut(&mut self, id: Id) -> &mut dyn Widget {
        let index = self.index(id);
        self.items[index].1.widget_mut()
    }

    pub fn button(&self, id: Id) -> &Button {
        match &self.items[self.index(id)].1 {
            Control::Button(button) => button,
            Control::Field(_) => panic!("{id:?} is a field, not a button"),
        }
    }

    pub fn button_mut(&mut self, id: Id) -> &mut Button {
        let index = self.index(id);
        match &mut self.items[index].1 {
            Control::Button(button) => button,
            Control::Field(_) => panic!("{id:?} is a field, not a button"),
        }
    }

    pub fn field(&self, id: Id) -> &EditBox {
        match &self.items[self.index(id)].1 {
            Control::Field(field) => field,
            Control::Button(_) => panic!("{id:?} is a button, not a field"),
        }
    }

    pub fn field_mut(&mut self, id: Id) -> &mut EditBox {
        let index = self.index(id);
        match &mut self.items[index].1 {
            Control::Field(field) => field,
            Control::Button(_) => panic!("{id:?} is a button, not a field"),
        }
    }

    /// Enables or disables a button.
    pub fn set_enabled(&mut self, id: Id, enabled: bool) {
        self.button_mut(id).enabled = enabled;
        if !enabled && self.focused() == Some(id) {
            self.blur();
        }
    }

    /// Shows or hides a widget.
    pub fn set_visible(&mut self, id: Id, visible: bool) {
        let index = self.index(id);
        self.hidden[index] = !visible;
        if !visible {
            self.items[index]
                .1
                .widget_mut()
                .update_hover(f32::NEG_INFINITY, f32::NEG_INFINITY);
            if self.focus == Some(index) {
                self.blur();
            }
        }
    }

    /// The items that are shown, with their index.
    fn shown(&self) -> impl Iterator<Item = (usize, &(Id, Control))> {
        self.items
            .iter()
            .enumerate()
            .filter(|&(i, _)| !self.hidden[i])
    }

    pub fn set_position(&mut self, id: Id, x: f32, y: f32) {
        self.widget_mut(id).set_position(x, y);
    }

    /// Places widgets left to right from `x`, each `gap` after the last.
    pub fn row(&mut self, ids: &[Id], x: f32, y: f32, gap: f32) {
        let mut x = x;
        for &id in ids {
            let widget = self.widget_mut(id);
            widget.set_position(x, y);
            x += widget.rect().w + gap;
        }
    }

    /// Places widgets in a row centred on a screen `width` wide.
    pub fn centered_row(&mut self, ids: &[Id], width: f32, y: f32, gap: f32) {
        let widths: f32 = ids.iter().map(|&id| self.widget(id).rect().w).sum();
        let row_w = widths + gap * ids.len().saturating_sub(1) as f32;
        self.row(ids, layout::centered_left(width, row_w), y, gap);
    }

    /// Places widgets top to bottom from `y`, each `step` below the last.
    pub fn column(&mut self, ids: &[Id], x: f32, y: f32, step: f32) {
        for (i, &id) in ids.iter().enumerate() {
            self.set_position(id, x, y + step * i as f32);
        }
    }

    /// The mouse is at `(x, y)`; with the button still held since a click, the pressed widget
    /// stays pressed.
    pub fn hover(&mut self, x: f32, y: f32) {
        if !crate::input_state::left_held() {
            self.pressed = None;
        }
        if let Some(i) = self.pressed
            && let Control::Field(field) = &mut self.items[i].1
        {
            field.drag(x);
        }
        for (i, (_, control)) in self.items.iter_mut().enumerate() {
            if !self.hidden[i] {
                control.widget_mut().update_hover(x, y);
            }
        }
    }

    /// The visible widget under `(x, y)`, enabled or not.
    pub fn at(&self, x: f32, y: f32) -> Option<(Id, Rect)> {
        self.shown().find_map(|(_, (id, control))| {
            let rect = control.widget().rect();
            rect.contains(x, y).then_some((*id, rect))
        })
    }

    /// The cursor the hovered widget asks for, if any.
    pub fn cursor(&self) -> Option<CursorKind> {
        self.shown()
            .find_map(|(_, (_, control))| control.widget().cursor())
    }

    /// `ContainerEventHandler.mouseClicked`: the widget a click at `(x, y)`
    pub fn click(&mut self, x: f32, y: f32) -> Option<Id> {
        let hit = self
            .shown()
            .find(|(_, (_, control))| match control {
                Control::Button(button) => button.clicked(x, y),
                Control::Field(field) => field.contains(x, y),
            })
            .map(|(i, _)| i);
        match hit.map(|i| &self.items[i].1) {
            Some(Control::Field(_)) => self.focus_index(hit),
            _ => self.focus_index(None),
        }
        self.pressed = None;
        if let Some(i) = hit
            && let Control::Field(field) = &mut self.items[i].1
        {
            field.click(x, y);
            self.pressed = Some(i);
        }
        hit.map(|i| self.items[i].0)
    }

    /// The focused widget, if any.
    pub fn focused(&self) -> Option<Id> {
        self.focus.map(|i| self.items[i].0)
    }

    /// `setInitialFocus` / `setFocused`.
    pub fn focus(&mut self, id: Id) {
        let index = self.index(id);
        self.focus_index(Some(index));
    }

    pub fn blur(&mut self) {
        self.focus_index(None);
    }

    fn focus_index(&mut self, index: Option<usize>) {
        if let Some(old) = self.focus {
            self.items[old].1.widget_mut().set_focused(false);
        }
        self.focus = index;
        if let Some(new) = index {
            self.items[new].1.widget_mut().set_focused(true);
        }
    }

    /// Tab: the focus moves to the next widget that takes it (Shift: the one before).
    pub fn tab(&mut self, backwards: bool) {
        let count = self.items.len();
        if count == 0 {
            return;
        }
        let start = match (self.focus, backwards) {
            (Some(i), false) => i + 1,
            (Some(i), true) => i + count - 1,
            (None, false) => 0,
            (None, true) => count - 1,
        };
        let next = (0..count)
            .map(|step| {
                if backwards {
                    (start + count - step) % count
                } else {
                    (start + step) % count
                }
            })
            .find(|&i| !self.hidden[i] && self.items[i].1.focusable());
        if next.is_some() {
            self.focus_index(next);
        }
    }

    /// A key or typed text.
    pub fn key(&mut self, event: &TextEvent) -> KeyResult<Id> {
        if let TextEvent::Tab { shift } = event {
            self.tab(*shift);
            return KeyResult::Taken;
        }
        let Some(index) = self.focus else {
            return KeyResult::Ignored;
        };
        let (id, control) = &mut self.items[index];
        match (control, event) {
            (Control::Button(button), TextEvent::Enter) if button.enabled => {
                KeyResult::Pressed(*id)
            }
            (_, TextEvent::Enter) => KeyResult::Ignored,
            (Control::Field(field), event) => {
                if field.handle(event) {
                    KeyResult::Edited(*id)
                } else {
                    KeyResult::Taken
                }
            }
            (Control::Button(_), _) => KeyResult::Ignored,
        }
    }

    /// Draws every widget.
    pub fn draw(&self, ui: &mut Ui) {
        self.draw_with(ui, |_| None);
    }

    /// Draws every widget, a button with the label `label` gives it, if any.
    pub fn draw_with(&self, ui: &mut Ui, label: impl Fn(Id) -> Option<String>) {
        for (_, (id, control)) in self.shown() {
            match (control, label(*id)) {
                (Control::Button(button), Some(text)) => button.draw_labeled(ui, &text),
                (control, _) => control.widget().draw(ui),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Id {
        Name,
        Ok,
        Off,
        Cancel,
    }

    fn set() -> WidgetSet<Id> {
        let mut off = Button::sized("off", 50.0, 20.0);
        off.enabled = false;
        let mut set = WidgetSet::new()
            .with_field(Id::Name, EditBox::new("", 100.0, 20.0))
            .with_button(Id::Ok, Button::sized("ok", 50.0, 20.0))
            .with_button(Id::Off, off)
            .with_button(Id::Cancel, Button::sized("cancel", 50.0, 20.0));
        set.row(&[Id::Name, Id::Ok, Id::Off, Id::Cancel], 0.0, 0.0, 10.0);
        set
    }

    #[test]
    fn tab_skips_disabled_buttons_and_wraps() {
        let mut set = set();
        set.tab(false);
        assert_eq!(set.focused(), Some(Id::Name));
        assert!(set.field(Id::Name).focused());
        set.tab(false);
        assert_eq!(set.focused(), Some(Id::Ok));
        assert!(!set.field(Id::Name).focused());
        set.tab(false);
        assert_eq!(set.focused(), Some(Id::Cancel));
        set.tab(false);
        assert_eq!(set.focused(), Some(Id::Name));
        set.tab(true);
        assert_eq!(set.focused(), Some(Id::Cancel));
    }

    #[test]
    fn keys_go_to_the_focused_widget() {
        let mut set = set();
        assert_eq!(set.key(&TextEvent::Char('a')), KeyResult::Ignored);
        set.focus(Id::Name);
        assert_eq!(set.key(&TextEvent::Char('a')), KeyResult::Edited(Id::Name));
        assert_eq!(set.field(Id::Name).value(), "a");
        assert_eq!(set.key(&TextEvent::Enter), KeyResult::Ignored);
        set.focus(Id::Ok);
        assert_eq!(set.key(&TextEvent::Enter), KeyResult::Pressed(Id::Ok));
    }

    #[test]
    fn clicks_hit_enabled_buttons_and_focus_fields() {
        let mut set = set();
        assert_eq!(set.click(5.0, 5.0), Some(Id::Name));
        assert_eq!(set.focused(), Some(Id::Name));
        assert_eq!(set.click(115.0, 5.0), Some(Id::Ok));
        assert_eq!(set.focused(), None);
        assert_eq!(set.click(175.0, 5.0), None, "disabled");
    }
}
