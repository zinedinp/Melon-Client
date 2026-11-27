//! Server dialogs on screen (vanilla `client/gui/screens/dialog`): the title
//! with the warning button in a 33px header, the body and inputs in a column
//! that scrolls when it doesn't fit, the buttons in the footer or in a grid.
//!
//! The parts are [`element::Element`]s, made by the body and input handlers
//! ([`body`], [`input`]); each kind of dialog only decides which buttons go
//! where ([`DialogScreen::new`]). What a button does goes to the app as a
//! [`DialogRequest`], since running a command, showing the next dialog or
//! answering the server is the connection's business, not the screen's.

pub mod body;
pub mod element;
pub mod input;
mod waiting;
mod warning;

use std::sync::Arc;

use element::{ButtonAction, ButtonElement, Element, Press, WarningElement};
pub use waiting::WaitingScreen;
pub use warning::WarningScreen;

use crate::component::ClickEvent;
use crate::dialog::{
    ActionButton, AfterAction, Dialog, DialogClick, DialogKind, InputValue, connection,
};
use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::text::{self, StyledText};
use crate::gui::widgets::{Rect, Ui};

/// What a dialog asks the app to do.
#[derive(Clone, Debug)]
pub enum DialogRequest {
    /// `DialogScreen.runAction`: the dialog goes as `then` says, then the
    /// click (if any) runs.
    Run {
        click: Option<DialogClick>,
        then: AfterAction,
    },
    /// Show this dialog over the screens (the pause menu's button).
    Show(Arc<Dialog>),
    /// The Quick Actions key.
    QuickActions,
    /// The warning's Disconnect.
    Disconnect,
}

/// How a server's dialog treats a screen ([`Screen::dialog_layer`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogLayer {
    /// A dialog: replaced by the next one, closed by a clear.
    Dialog,
    /// `WaitingForResponseScreen`: replaced by the next dialog.
    Waiting,
    /// `DialogScreen.WarningScreen`: stays, the dialog changes behind it.
    Warning,
}

/// `HeaderAndFooterLayout.DEFAULT_HEADER_AND_FOOTER_HEIGHT`.
const HEADER_H: f32 = 33.0;
/// `ButtonListDialogScreen.FOOTER_MARGIN`: the footer without a button.
const FOOTER_MARGIN: f32 = 5.0;
/// The body column's spacing (`LinearLayout.vertical().spacing(10)`).
const BODY_SPACING: f32 = 10.0;
/// `packControlsIntoColumns`' cell spacing.
const GRID_SPACING: f32 = 2.0;
/// `SimpleDialogScreen`'s footer row spacing.
const FOOTER_SPACING: f32 = 8.0;
/// The title and the warning button (`createTitleWithWarningButton`).
const TITLE_SPACING: f32 = 10.0;
/// `AbstractScrollArea.SCROLLBAR_WIDTH` and the gap to the content.
const SCROLLBAR_W: f32 = 6.0;
const SCROLLBAR_GAP: f32 = 4.0;

/// A run of the body column: one element, or buttons in columns.
enum Block {
    One(usize),
    Grid { items: Vec<usize>, columns: usize },
}

/// Where a block's elements sit, from the body's top-left.
struct Placed {
    index: usize,
    x: f32,
    y: f32,
}

pub struct DialogScreen {
    dialog: Arc<Dialog>,
    local_server: bool,
    title: StyledText,
    title_at: (f32, f32),
    /// Every element: the warning button first, the body, the footer.
    elements: Vec<Box<dyn Element>>,
    blocks: Vec<Block>,
    footer: Vec<usize>,
    footer_h: f32,
    /// The inputs' keys and elements, in the dialog's order.
    inputs: Vec<(String, usize)>,
    /// The body's layout, from its top-left, and where that is.
    placed: Vec<Placed>,
    body_origin: (f32, f32),
    body_size: (f32, f32),
    content: Rect,
    scroll: f32,
    focus: Option<usize>,
    /// The element the left button went down on, while held.
    pressed: Option<usize>,
    mouse: (f32, f32),
    scrollbar_drag: Option<(f32, f32)>,
}

/// The warning button's index.
const WARNING: usize = 0;

impl DialogScreen {
    /// `DialogScreens.createFromData`: the dialog's screen.
    pub fn new(dialog: Arc<Dialog>, local_server: bool, ctx: &ScreenContext) -> Self {
        let mut elements: Vec<Box<dyn Element>> = vec![Box::new(WarningElement::new(ctx))];
        let mut blocks = Vec::new();
        let add = |elements: &mut Vec<Box<dyn Element>>, element: Box<dyn Element>| {
            elements.push(element);
            elements.len() - 1
        };
        for part in &dialog.common.body {
            let index = add(&mut elements, body::element(part, ctx));
            blocks.push(Block::One(index));
        }
        let mut inputs = Vec::new();
        for field in &dialog.common.inputs {
            let index = add(&mut elements, input::element(field, ctx));
            blocks.push(Block::One(index));
            inputs.push((field.key.clone(), index));
        }
        let button = |b: &ActionButton| -> Box<dyn Element> {
            Box::new(ButtonElement::new(
                &b.button.label,
                b.button.tooltip.as_ref(),
                b.button.width,
                ButtonAction::Action(b.action.clone()),
                ctx,
            ))
        };
        let mut footer = Vec::new();
        let mut footer_h = HEADER_H;
        let (grid, columns, exit): (Vec<Box<dyn Element>>, u32, Option<&ActionButton>) =
            match &dialog.kind {
                DialogKind::Notice { action } => {
                    footer.push(add(&mut elements, button(action)));
                    (Vec::new(), 1, None)
                }
                DialogKind::Confirmation { yes, no } => {
                    footer.push(add(&mut elements, button(yes)));
                    footer.push(add(&mut elements, button(no)));
                    (Vec::new(), 1, None)
                }
                DialogKind::MultiAction {
                    actions,
                    exit,
                    columns,
                } => (
                    actions.iter().map(button).collect(),
                    *columns,
                    exit.as_ref(),
                ),
                DialogKind::ServerLinks {
                    exit,
                    columns,
                    button_width,
                } => {
                    let links = connection::links()
                        .into_iter()
                        .map(|link| -> Box<dyn Element> {
                            let open = ClickEvent::OpenUrl {
                                url: link.url.clone(),
                            };
                            Box::new(ButtonElement::new(
                                &link.display_name(),
                                None,
                                *button_width,
                                ButtonAction::Action(Some(crate::dialog::Action::Static(open))),
                                ctx,
                            ))
                        })
                        .collect();
                    (links, *columns, exit.as_ref())
                }
                DialogKind::DialogList {
                    dialogs,
                    exit,
                    columns,
                    button_width,
                } => {
                    let entries = connection::resolve_set(dialogs)
                        .into_iter()
                        .map(|entry| -> Box<dyn Element> {
                            let title = entry.common.external_title().clone();
                            Box::new(ButtonElement::new(
                                &title,
                                None,
                                *button_width,
                                ButtonAction::Show(entry),
                                ctx,
                            ))
                        })
                        .collect();
                    (entries, *columns, exit.as_ref())
                }
            };
        if matches!(
            dialog.kind,
            DialogKind::MultiAction { .. }
                | DialogKind::ServerLinks { .. }
                | DialogKind::DialogList { .. }
        ) {
            let items: Vec<usize> = grid.into_iter().map(|b| add(&mut elements, b)).collect();
            blocks.push(Block::Grid {
                items,
                columns: columns.max(1) as usize,
            });
            match exit {
                Some(exit) => footer.push(add(&mut elements, button(exit))),
                None => footer_h = FOOTER_MARGIN,
            }
        }
        Self {
            title: StyledText::from_component(&dialog.common.title, ctx.lang),
            dialog,
            local_server,
            title_at: (0.0, 0.0),
            elements,
            blocks,
            footer,
            footer_h,
            inputs,
            placed: Vec::new(),
            body_origin: (0.0, 0.0),
            body_size: (0.0, 0.0),
            content: Rect::default(),
            scroll: 0.0,
            focus: None,
            pressed: None,
            mouse: (0.0, 0.0),
            scrollbar_drag: None,
        }
    }

    /// The inputs' values by key (`DialogControlSet.valueGetters`).
    fn values(&self) -> Vec<(String, InputValue)> {
        self.inputs
            .iter()
            .filter_map(|(key, index)| Some((key.clone(), self.elements[*index].value()?)))
            .collect()
    }

    /// `runAction` with the dialog's `after_action`.
    fn run(&self, click: Option<DialogClick>) -> ScreenAction {
        ScreenAction::Dialog(DialogRequest::Run {
            click,
            then: self.dialog.common.after_action,
        })
    }

    /// What a press on an element asks for.
    fn act(&self, press: Press, ctx: &ScreenContext) -> ScreenAction {
        match press {
            Press::Missed | Press::Taken => ScreenAction::None,
            Press::Click(event) => self.run(Some(DialogClick::from_event(event))),
            Press::Action(action) => self.run(action.map(|a| a.resolve(&self.values()))),
            Press::Show(dialog) => self.run(Some(DialogClick::Show(dialog))),
            Press::Warning => ScreenAction::open(WarningScreen::new(self.local_server, ctx)),
        }
    }

    fn is_body(&self, index: usize) -> bool {
        index != WARNING && !self.footer.contains(&index)
    }

    /// Whether the body element at `index` can be reached at `(x, y)`: in
    /// the visible part of a scrolled body.
    fn reachable(&self, index: usize, y: f32) -> bool {
        !self.is_body(index) || (y >= self.content.y && y < self.content.bottom())
    }

    fn max_scroll(&self) -> f32 {
        (self.body_size.1 - self.content.h).max(0.0)
    }

    /// Puts the body's elements where the scroll has them.
    fn place_body(&mut self) {
        let (x0, y0) = (self.body_origin.0, self.body_origin.1 - self.scroll);
        for placed in &self.placed {
            self.elements[placed.index].place(x0 + placed.x, y0 + placed.y);
        }
    }

    fn set_focus(&mut self, index: Option<usize>) {
        if let Some(old) = self.focus {
            self.elements[old].set_focused(false);
        }
        self.focus = index;
        if let Some(new) = index {
            self.elements[new].set_focused(true);
        }
    }

    /// Tab: the next element that takes the focus.
    fn tab(&mut self, backwards: bool) {
        let count = self.elements.len();
        let start = self
            .focus
            .unwrap_or(if backwards { count } else { usize::MAX });
        let next = (1..=count)
            .map(|step| {
                if backwards {
                    (start + count * 2 - step) % count
                } else {
                    start.wrapping_add(step) % count
                }
            })
            .find(|&i| self.elements[i].focusable());
        self.set_focus(next);
    }

    /// The scrollbar's thumb, when the body scrolls.
    fn scrollbar(&self) -> Option<(Rect, Rect)> {
        let max = self.max_scroll();
        if max <= 0.0 {
            return None;
        }
        let x = self.body_origin.0 + self.body_size.0 + SCROLLBAR_GAP;
        let track = Rect::new(x, self.content.y, SCROLLBAR_W, self.content.h);
        let h = (self.content.h * self.content.h / self.body_size.1)
            .clamp(32.0, (self.content.h - 8.0).max(32.0))
            .floor();
        let y = track.y + (self.scroll / max * (track.h - h)).round();
        Some((track, Rect::new(x, y, SCROLLBAR_W, h)))
    }
}

/// The columns' widths, the rows' heights and the grid's width:
/// `packControlsIntoColumns` with the last, short row centred.
fn grid_layout(sizes: &[(f32, f32)], columns: usize) -> (Vec<f32>, Vec<f32>, f32) {
    let full = sizes.len() / columns * columns;
    let mut widths = vec![0.0f32; columns];
    let mut heights = Vec::new();
    for (i, &(w, h)) in sizes.iter().enumerate() {
        if i < full {
            widths[i % columns] = widths[i % columns].max(w);
        }
        if i % columns == 0 && i < full || i == full {
            heights.push(0.0f32);
        }
        if let Some(last) = heights.last_mut() {
            *last = last.max(h);
        }
    }
    let full_w = if full > 0 {
        widths.iter().sum::<f32>() + GRID_SPACING * (columns - 1) as f32
    } else {
        0.0
    };
    let rest = &sizes[full..];
    let rest_w =
        rest.iter().map(|s| s.0).sum::<f32>() + GRID_SPACING * rest.len().saturating_sub(1) as f32;
    (widths, heights, full_w.max(rest_w))
}

impl Screen for DialogScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let sizes: Vec<(f32, f32)> = self.elements.iter_mut().map(|e| e.layout(ctx)).collect();

        // Header: the title and the warning button, centred.
        let title_w = text::width(&ctx.assets.font, &self.title);
        let row_w = title_w + TITLE_SPACING + 20.0;
        let x0 = ((width - row_w) / 2.0).floor();
        let y0 = ((HEADER_H - 20.0) / 2.0).floor();
        self.title_at = (x0, y0 + ((20.0f32 - 9.0) / 2.0).floor());
        let (mut wx, mut wy) = (x0 + title_w + TITLE_SPACING, y0);
        if wx < 0.0 || wy < 0.0 || wx > width - 20.0 || wy > height - 20.0 {
            // `makeSureWarningButtonIsInBounds`.
            wx = (width - 40.0).max(0.0);
            wy = 5.0f32.min(height);
        }
        self.elements[WARNING].place(wx, wy);

        // Footer: a centred row.
        let footer_w = self.footer.iter().map(|&i| sizes[i].0).sum::<f32>()
            + FOOTER_SPACING * self.footer.len().saturating_sub(1) as f32;
        let mut fx = ((width - footer_w) / 2.0).floor();
        let fy = height - self.footer_h + ((self.footer_h - 20.0) / 2.0).floor();
        for &index in &self.footer {
            self.elements[index].place(fx, fy);
            fx += sizes[index].0 + FOOTER_SPACING;
        }

        // Body: a centred column, blocks centred in it.
        let mut block_sizes = Vec::new();
        let mut grids = Vec::new();
        for block in &self.blocks {
            match block {
                Block::One(index) => block_sizes.push(sizes[*index]),
                Block::Grid { items, columns } => {
                    let item_sizes: Vec<_> = items.iter().map(|&i| sizes[i]).collect();
                    let (widths, heights, w) = grid_layout(&item_sizes, *columns);
                    let h = heights.iter().sum::<f32>()
                        + GRID_SPACING * heights.len().saturating_sub(1) as f32;
                    block_sizes.push((w, h));
                    grids.push((widths, heights, w));
                }
            }
        }
        let body_w = block_sizes.iter().map(|s| s.0).fold(0.0, f32::max);
        let body_h = block_sizes.iter().map(|s| s.1).sum::<f32>()
            + BODY_SPACING * block_sizes.len().saturating_sub(1) as f32;
        self.placed.clear();
        let mut y = 0.0;
        let mut grids = grids.into_iter();
        for (block, &(block_w, block_h)) in self.blocks.iter().zip(&block_sizes) {
            let bx = ((body_w - block_w) / 2.0).floor();
            match block {
                Block::One(index) => self.placed.push(Placed {
                    index: *index,
                    x: bx,
                    y,
                }),
                Block::Grid { items, columns } => {
                    let Some((widths, heights, grid_w)) = grids.next() else {
                        continue;
                    };
                    let full = items.len() / columns * columns;
                    let mut row_y = y;
                    for (row, chunk) in items.chunks(*columns).enumerate() {
                        let row_h = heights.get(row).copied().unwrap_or(20.0);
                        if row * columns < full {
                            let mut cx = bx;
                            for (column, &index) in chunk.iter().enumerate() {
                                let cell = widths[column];
                                self.placed.push(Placed {
                                    index,
                                    x: cx + ((cell - sizes[index].0) / 2.0).floor(),
                                    y: row_y,
                                });
                                cx += cell + GRID_SPACING;
                            }
                        } else {
                            let w = chunk.iter().map(|&i| sizes[i].0).sum::<f32>()
                                + GRID_SPACING * (chunk.len() - 1) as f32;
                            let mut cx = bx + ((grid_w - w) / 2.0).floor();
                            for &index in chunk {
                                self.placed.push(Placed {
                                    index,
                                    x: cx,
                                    y: row_y,
                                });
                                cx += sizes[index].0 + GRID_SPACING;
                            }
                        }
                        row_y += row_h + GRID_SPACING;
                    }
                }
            }
            y += block_h + BODY_SPACING;
        }

        let content_h = (height - HEADER_H - self.footer_h).max(0.0);
        self.content = Rect::new(0.0, HEADER_H, width, content_h);
        self.body_size = (body_w, body_h);
        let top = if body_h <= content_h {
            HEADER_H + ((content_h - body_h) / 2.0).floor()
        } else {
            HEADER_H
        };
        self.body_origin = (((width - body_w) / 2.0).floor(), top);
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        self.place_body();
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        if let Some((grab, scroll)) = self.scrollbar_drag {
            if crate::gui::input_state::left_held() {
                if let Some((track, thumb)) = self.scrollbar() {
                    let room = (track.h - thumb.h).max(1.0);
                    self.scroll = (scroll + (y - grab) / room * self.max_scroll())
                        .clamp(0.0, self.max_scroll());
                    self.place_body();
                }
                return;
            }
            self.scrollbar_drag = None;
        }
        if let Some(index) = self.pressed {
            if crate::gui::input_state::left_held() {
                self.elements[index].drag(x, y);
            } else {
                self.elements[index].release();
                self.pressed = None;
            }
        }
        for (i, element) in self.elements.iter_mut().enumerate() {
            let reachable = i == WARNING
                || self.footer.contains(&i)
                || (y >= self.content.y && y < self.content.bottom());
            if reachable {
                element.hover(x, y);
            } else {
                element.hover(f32::NEG_INFINITY, f32::NEG_INFINITY);
            }
        }
    }

    fn cursor(&self) -> CursorKind {
        if self.scrollbar_drag.is_some() {
            return CursorKind::ResizeNs;
        }
        self.elements
            .iter()
            .find_map(|e| e.cursor())
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if let Some((_, thumb)) = self.scrollbar()
            && thumb.contains(x, y)
        {
            self.scrollbar_drag = Some((y, self.scroll));
            return ScreenAction::None;
        }
        for index in 0..self.elements.len() {
            if !self.reachable(index, y) {
                continue;
            }
            let press = self.elements[index].press(x, y, ctx);
            if matches!(press, Press::Missed) {
                continue;
            }
            let focusable = self.elements[index].focusable();
            self.set_focus(focusable.then_some(index));
            self.pressed = Some(index);
            return self.act(press, ctx);
        }
        self.set_focus(None);
        ScreenAction::None
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.scrollbar_drag = None;
        if let Some(index) = self.pressed.take() {
            self.elements[index].release();
        }
    }

    fn on_scroll(&mut self, delta: f32) {
        // The focused element first (a multi-line box scrolls its text).
        if let Some(index) = self.focus
            && self.elements[index]
                .rect()
                .contains(self.mouse.0, self.mouse.1)
            && self.elements[index].scroll(delta)
        {
            return;
        }
        let max = self.max_scroll();
        if max > 0.0 {
            self.scroll = (self.scroll - delta * 10.0).clamp(0.0, max);
            self.place_body();
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        if let TextEvent::Tab { shift } = event {
            self.tab(shift);
            return ScreenAction::None;
        }
        let Some(index) = self.focus else {
            return ScreenAction::None;
        };
        let press = self.elements[index].key(&event, ctx);
        self.act(press, ctx)
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        ui.styled(
            &self.title,
            self.title_at.0,
            self.title_at.1,
            text::colors::WHITE,
        );
        let mouse = self.mouse;
        self.elements[WARNING].draw(ui, mouse);
        let scrolls = self.max_scroll() > 0.0;
        if scrolls {
            ui.clip(self.content);
        }
        for placed in &self.placed {
            let element = &self.elements[placed.index];
            let rect = element.rect();
            if rect.bottom() >= self.content.y && rect.y <= self.content.bottom() {
                element.draw(ui, mouse);
            }
        }
        if scrolls {
            ui.unclip();
        }
        if let Some((track, thumb)) = self.scrollbar() {
            let assets = ui.assets;
            assets
                .scroller_background
                .draw(ui.draw_list, track.x, track.y, track.w, track.h);
            assets
                .scroller
                .draw(ui.draw_list, thumb.x, thumb.y, thumb.w, thumb.h);
        }
        for &index in &self.footer {
            self.elements[index].draw(ui, mouse);
        }
        let tooltip = self.elements.iter().enumerate().find_map(|(i, e)| {
            self.reachable(i, mouse.1)
                .then(|| e.tooltip(mouse, ui))
                .flatten()
        });
        if let Some(lines) = tooltip {
            ui.mouse_tooltip(&lines, mouse);
        }
    }

    fn closes_on_escape(&self) -> bool {
        self.dialog.common.can_close_with_escape
    }

    /// `DialogScreen.isPauseScreen`.
    fn is_pause_screen(&self) -> bool {
        self.dialog.common.pause
    }

    /// `DialogScreen.onClose`: the cancel action, and back.
    fn on_escape(&mut self, _ctx: &ScreenContext) -> Option<ScreenAction> {
        let click = self.dialog.on_cancel().map(|a| a.resolve(&self.values()));
        Some(ScreenAction::Dialog(DialogRequest::Run {
            click,
            then: AfterAction::Close,
        }))
    }

    fn dialog_layer(&self) -> Option<DialogLayer> {
        Some(DialogLayer::Dialog)
    }

    fn visit_stacks(&self, visit: &mut dyn FnMut(&dyn crate::world::item_model::StackKey) -> bool) {
        for element in &self.elements {
            element.visit_stacks(visit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_fill_columns_and_centre_the_short_row() {
        let sizes = [(150.0, 20.0), (100.0, 20.0), (120.0, 20.0)];
        let (widths, heights, w) = grid_layout(&sizes, 2);
        assert_eq!(widths, vec![150.0, 100.0]);
        assert_eq!(heights, vec![20.0, 20.0]);
        assert_eq!(w, 252.0);
        let (_, heights, w) = grid_layout(&sizes[..1], 2);
        assert_eq!((heights, w), (vec![20.0], 150.0));
    }
}
