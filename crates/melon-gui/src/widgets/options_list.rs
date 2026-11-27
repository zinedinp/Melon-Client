//! Vanilla `OptionsList` inside an `OptionsSubScreen`.

use crate::screen::{ConfigChange, CursorKind, Screen, ScreenAction, ScreenContext};
use crate::widgets::button::Button;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, WIDE_BUTTON_W};
use crate::widgets::slider::Slider;
use crate::widgets::{Rect, Ui, Widget};
use melon_assets::i18n::Lang;
use melon_settings::options::Config;

/// `OptionsList`'s `itemHeight`.
const ROW_H: f32 = 25.0;
/// `AbstractSelectionList`.
const ENTRY_PAD: f32 = 2.0;
/// `OptionsList.addHeader`.
const HEADER_H: f32 = 18.0;
const HEADER_GAP: f32 = 18.0;
/// `FocusableTextWidget.DEFAULT_PADDING` around a header's text.
const HEADER_TEXT_PAD: f32 = 4.0;
/// `OptionsList.Entry`: the right widget starts 160 right of the left one.
const COLUMN_GAP: f32 = 10.0;
/// Pixels per wheel notch.
const SCROLL_RATE: f32 = 10.0;
/// `OptionsList.BIG_BUTTON_WIDTH`: a wide option spans both columns.
const WIDE_W: f32 = BUTTON_W * 2.0 + COLUMN_GAP;

/// One option of a list.
pub trait OptionEntry: Copy + 'static {
    /// A slider's step count and current step; `None` for a button.
    fn slider(self, _config: &Config) -> Option<(u32, u32)> {
        None
    }

    /// A slider moved to `step`.
    fn set_step(self, _step: u32) -> ConfigChange {
        Box::new(|_| {})
    }

    /// A button was clicked: toggles or cycles the option.
    fn press(self, _ctx: &ScreenContext) -> ConfigChange {
        Box::new(|_| {})
    }

    /// The caption: `OptionInstance`'s caption function.
    fn label(self, config: &Config, lang: &Lang) -> String;

    /// Whether it can be used at all (the narrator can't).
    fn enabled(self) -> bool {
        true
    }

    /// Opens a sub-screen instead of changing a setting (Mouse Settings).
    fn open(self, _ctx: &ScreenContext) -> Option<ScreenAction> {
        None
    }

    /// `OptionsList.addBig`: a row of its own, both columns wide.
    fn wide(self) -> bool {
        false
    }

    /// `OptionInstance`'s tooltip for the current value, a translation key.
    fn tooltip(self, _config: &Config) -> Option<&'static str> {
        None
    }
}

/// Each option's row and column (0 or 1; wide ones take column 0) in a section, and the row
/// count.
fn section_layout<O: OptionEntry>(options: &[O]) -> (Vec<(usize, usize)>, usize) {
    let mut cells = Vec::with_capacity(options.len());
    let (mut row, mut column) = (0, 0);
    for option in options {
        if option.wide() {
            if column != 0 {
                row += 1;
            }
            cells.push((row, 0));
            row += 1;
            column = 0;
        } else {
            cells.push((row, column));
            column ^= 1;
            if column == 0 {
                row += 1;
            }
        }
    }
    (cells, row + usize::from(column != 0))
}

/// `OptionsList.addHeader`'s space above a header.
fn header_gap(height: f32) -> f32 {
    if height > 0.0 { HEADER_GAP } else { 0.0 }
}

/// A run of options under an optional header.
pub struct Section<O: 'static> {
    pub header: Option<&'static str>,
    pub options: &'static [O],
}

enum Control {
    Slider(Slider),
    Button(Button),
}

impl Control {
    fn widget(&self) -> &dyn Widget {
        match self {
            Control::Slider(s) => s,
            Control::Button(b) => b,
        }
    }

    fn widget_mut(&mut self) -> &mut dyn Widget {
        match self {
            Control::Slider(s) => s,
            Control::Button(b) => b,
        }
    }
}

/// The scrolled list of options, without the title and Done.
pub struct OptionsList<O: OptionEntry> {
    sections: &'static [Section<O>],
    /// Each section header's text top, `None` for sections without one.
    header_y: Vec<Option<f32>>,
    /// Every option's widget, in section order.
    options: Vec<(O, Control)>,
    /// The visible band: top, bottom, screen width.
    top: f32,
    bottom: f32,
    width: f32,
    scroll: f32,
    /// Changes not yet handed out through [`OptionsList::take_changes`].
    pending: Vec<ConfigChange>,
    /// The last mouse position, for tooltips.
    mouse: (f32, f32),
}

impl<O: OptionEntry> OptionsList<O> {
    pub fn new(sections: &'static [Section<O>], config: &Config) -> Self {
        let options = sections
            .iter()
            .flat_map(|section| section.options.iter().copied())
            .map(|option| {
                let width = if option.wide() { WIDE_W } else { BUTTON_W };
                let control = match option.slider(config) {
                    Some((steps, step)) => {
                        Control::Slider(Slider::new(steps, step, width, BUTTON_H))
                    }
                    None => {
                        let mut button = Button::sized("", width, BUTTON_H);
                        button.enabled = option.enabled();
                        Control::Button(button)
                    }
                };
                (option, control)
            })
            .collect();
        Self {
            sections,
            header_y: vec![None; sections.len()],
            options,
            top: 0.0,
            bottom: 0.0,
            width: 0.0,
            scroll: 0.0,
            pending: Vec::new(),
            mouse: (-1.0, -1.0),
        }
    }

    /// `AbstractSelectionList.contentHeight`: every entry's height, and 4.
    fn content_height(&self) -> f32 {
        let mut height = 0.0;
        for section in self.sections {
            if section.header.is_some() {
                height += header_gap(height) + HEADER_H;
            }
            height += section_layout(section.options).1 as f32 * ROW_H;
        }
        height + ENTRY_PAD * 2.0
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - (self.bottom - self.top)).max(0.0)
    }

    /// The list fills the band from `top` to `bottom`.
    pub fn set_bounds(&mut self, top: f32, bottom: f32, width: f32) {
        self.top = top;
        self.bottom = bottom;
        self.width = width;
        self.scroll = self.scroll.min(self.max_scroll());
        self.layout();
    }

    fn layout(&mut self) {
        let left = (self.width / 2.0 - BUTTON_W - COLUMN_GAP / 2.0).floor();
        let right = left + BUTTON_W + COLUMN_GAP;
        let start = self.top + ENTRY_PAD - self.scroll;
        let mut y = 0.0;
        let mut controls = self.options.iter_mut();
        for (section, header_y) in self.sections.iter().zip(&mut self.header_y) {
            *header_y = section.header.map(|_| {
                let gap = header_gap(y);
                let text_y = start + y + ENTRY_PAD + gap + HEADER_TEXT_PAD;
                y += gap + HEADER_H;
                text_y
            });
            let (cells, rows) = section_layout(section.options);
            for (r, column) in cells {
                let (_, control) = controls.next().expect("one control per option");
                let x = if column == 0 { left } else { right };
                let row_y = start + y + r as f32 * ROW_H + ENTRY_PAD;
                control.widget_mut().set_position(x, row_y);
            }
            y += rows as f32 * ROW_H;
        }
    }

    /// Whether `y` is inside the visible band.
    fn in_list(&self, y: f32) -> bool {
        y >= self.top && y < self.bottom
    }

    pub fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        let in_list = self.in_list(y);
        for (option, control) in &mut self.options {
            control.widget_mut().update_hover(x, y);
            match control {
                Control::Slider(s) => {
                    s.hovered &= in_list;
                    if s.drag(x) {
                        self.pending.push(option.set_step(s.step()));
                    }
                }
                Control::Button(b) => b.hovered &= in_list,
            }
        }
    }

    /// Whether a slider was let go, which clicks.
    pub fn on_mouse_up(&mut self) -> bool {
        let mut released = false;
        for (_, control) in &mut self.options {
            if let Control::Slider(s) = control {
                released |= s.release();
            }
        }
        released
    }

    pub fn on_scroll(&mut self, delta: f32) {
        self.scroll = (self.scroll - delta * SCROLL_RATE).clamp(0.0, self.max_scroll());
        self.layout();
    }

    /// A click on an option.
    pub fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if !self.in_list(y) {
            return ScreenAction::None;
        }
        for (option, control) in &mut self.options {
            match control {
                Control::Slider(s) => {
                    if let Some(changed) = s.press(x, y) {
                        if changed {
                            self.pending.push(option.set_step(s.step()));
                        }
                        return ScreenAction::None;
                    }
                }
                Control::Button(b) => {
                    if b.clicked(x, y) {
                        ctx.click();
                        if let Some(action) = option.open(ctx) {
                            return action;
                        }
                        self.pending.push(option.press(ctx));
                        return ScreenAction::None;
                    }
                }
            }
        }
        ScreenAction::None
    }

    /// The changes made since the last call, as one action.
    pub fn take_changes(&mut self) -> ScreenAction {
        ScreenAction::change_config(std::mem::take(&mut self.pending))
    }

    /// The options changed: sliders follow.
    pub fn refresh_config(&mut self, config: &Config) {
        for (option, control) in &mut self.options {
            if let (Control::Slider(s), Some((_, step))) = (control, option.slider(config)) {
                s.set_step(step);
            }
        }
    }

    pub fn cursor(&self) -> Option<CursorKind> {
        self.options.iter().find_map(|(_, c)| c.widget().cursor())
    }

    pub fn draw(&self, ui: &mut Ui, config: &Config) {
        ui.list_background(self.top, self.bottom);
        ui.clip(Rect::new(0.0, self.top, ui.width, self.bottom - self.top));
        for (section, y) in self.sections.iter().zip(&self.header_y) {
            if let (Some(key), Some(y)) = (section.header, y) {
                let text = ui.tr(key);
                ui.section_header(text, *y);
            }
        }
        let mut tooltip = None;
        for (option, control) in &self.options {
            let label = option.label(config, ui.lang);
            let hovered = match control {
                Control::Slider(s) => {
                    s.draw_labeled(ui, &label);
                    s.hovered
                }
                Control::Button(b) => {
                    b.draw_labeled(ui, &label);
                    b.hovered
                }
            };
            if hovered && let Some(key) = option.tooltip(config) {
                tooltip = Some((key, control.widget().rect()));
            }
        }
        ui.unclip();
        if let Some((key, rect)) = tooltip {
            let text = ui.tr(key);
            ui.widget_tooltip(text, self.mouse, rect);
        }
    }
}

/// `OptionsSubScreen`: a title over an [`OptionsList`], and Done.
pub struct OptionsSubScreen<O: OptionEntry> {
    title: &'static str,
    list: OptionsList<O>,
    done: Button,
}

impl<O: OptionEntry> OptionsSubScreen<O> {
    /// `OptionsSubScreen`'s header is just the title; its footer just Done.
    const LAYOUT: HeaderFooter = HeaderFooter::DEFAULT;

    pub fn new(title: &'static str, sections: &'static [Section<O>], ctx: &ScreenContext) -> Self {
        Self {
            title,
            list: OptionsList::new(sections, ctx.config),
            done: Button::sized("gui.done", WIDE_BUTTON_W, BUTTON_H),
        }
    }
}

impl<O: OptionEntry> Screen for OptionsSubScreen<O> {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let layout = Self::LAYOUT;
        self.list
            .set_bounds(layout.top(), layout.bottom(height), width);
        self.done.x = (width / 2.0 - WIDE_BUTTON_W / 2.0).floor();
        self.done.y = layout.footer_button_y(height);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.done.update_hover(x, y);
        self.list.on_mouse_move(x, y);
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, ctx: &ScreenContext) {
        if self.list.on_mouse_up() {
            ctx.click();
        }
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    fn cursor(&self) -> CursorKind {
        self.list
            .cursor()
            .or(self.done.cursor())
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if self.done.clicked(x, y) {
            ctx.click();
            return ScreenAction::Back;
        }
        let action = self.list.on_click(x, y, ctx);
        if !matches!(action, ScreenAction::None) {
            return action;
        }
        self.list.take_changes()
    }

    fn poll(&mut self) -> ScreenAction {
        self.list.take_changes()
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        ui.menu_background();
        self.list.draw(ui, ctx.config);
        let title = ui.tr(self.title);
        ui.title(title, Self::LAYOUT.title_y());
        self.done.draw(ui);
    }

    fn refresh_config(&mut self, config: &Config) {
        self.list.refresh_config(config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct Toggle;

    impl OptionEntry for Toggle {
        fn label(self, _config: &Config, _lang: &Lang) -> String {
            String::new()
        }
    }

    static SECTIONS: [Section<Toggle>; 2] = [
        Section {
            header: Some("a"),
            options: &[Toggle, Toggle, Toggle],
        },
        Section {
            header: Some("b"),
            options: &[Toggle],
        },
    ];

    #[test]
    fn headers_take_vanillas_heights() {
        let mut list = OptionsList::new(&SECTIONS, &Config::default());
        list.set_bounds(33.0, 1000.0, 400.0);
        assert_eq!(list.header_y[0], Some(33.0 + 2.0 + 2.0 + 4.0));
        let y = |i: usize| list.options[i].1.widget().rect().y;
        assert_eq!(y(0), 33.0 + 2.0 + 18.0 + 2.0);
        assert_eq!(y(2), y(0) + 25.0);
        let second_entry = 33.0 + 2.0 + 18.0 + 50.0;
        assert_eq!(list.header_y[1], Some(second_entry + 2.0 + 18.0 + 4.0));
        assert_eq!(y(3), second_entry + 36.0 + 2.0);
        assert_eq!(list.content_height(), 18.0 + 50.0 + 36.0 + 25.0 + 4.0);
    }
}
