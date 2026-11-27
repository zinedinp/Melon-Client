//! Vanilla `ExperimentsScreen`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::singleplayer::data_packs::EXPERIMENTS;
use crate::text::font::LINE_HEIGHT;
use crate::text::{StyledText, colors};
use crate::widgets::button::Button;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, SPACING};
use crate::widgets::scroll_list::{ListClick, ScrollList};
use crate::widgets::set::{KeyResult, WidgetSet};
use crate::widgets::{Rect, Ui};

const LAYOUT: HeaderFooter = HeaderFooter::DEFAULT;

/// `MAIN_CONTENT_WIDTH` and `SwitchGrid.builder(299)`.
const CONTENT_W: f32 = 310.0;
const GRID_W: f32 = 299.0;
/// `SCROLL_AREA_MIN_HEIGHT`.
const SCROLL_MIN_H: f32 = 130.0;
/// `paddingBottom(15)` under the warning.
const INFO_GAP: f32 = 15.0;
const SWITCH_W: f32 = 44.0;
/// A switch row.
const ROW_H: f32 = BUTTON_H + 4.0 + 2.0 * LINE_HEIGHT - 4.0 + 4.0;
/// `withInfoUnderneath(2, true)`.
const INFO_ROWS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Done,
    Cancel,
}

pub struct ExperimentsScreen {
    /// The pack ids the list shows, and whether each is switched on.
    rows: Vec<(&'static str, bool)>,
    /// The Create World screen's data pack list this edits on Done.
    output: Rc<RefCell<Vec<String>>>,
    list: ScrollList,
    widgets: WidgetSet<Id>,
    mouse: (f32, f32),
    /// The warning's lines, wrapped when the screen was laid out, and their top.
    info: Vec<String>,
    info_y: f32,
}

impl ExperimentsScreen {
    pub fn new(output: Rc<RefCell<Vec<String>>>) -> Self {
        let chosen = output.borrow().clone();
        let rows = EXPERIMENTS
            .iter()
            .map(|&id| (id, chosen.iter().any(|c| c == id)))
            .collect();
        let widgets = WidgetSet::new()
            .with_button(Id::Done, Button::sized("gui.done", BUTTON_W, BUTTON_H))
            .with_button(Id::Cancel, Button::sized("gui.cancel", BUTTON_W, BUTTON_H));
        Self {
            rows,
            output,
            list: ScrollList::new(GRID_W, ROW_H),
            widgets,
            mouse: (-1.0, -1.0),
            info: Vec::new(),
            info_y: 0.0,
        }
    }

    /// `onDone`.
    fn done(&self) {
        let mut chosen = self.output.borrow_mut();
        chosen.retain(|id| !EXPERIMENTS.contains(&id.as_str()));
        chosen.extend(
            self.rows
                .iter()
                .filter(|(_, on)| *on)
                .map(|(id, _)| (*id).to_owned()),
        );
    }

    /// The switch of row `index`, at the right of its row.
    fn switch_rect(&self, index: usize) -> Rect {
        let row = self.list.row_rect(index);
        Rect::new(row.right() - SWITCH_W, row.y, SWITCH_W, BUTTON_H)
    }

    fn switch_at(&self, x: f32, y: f32) -> Option<usize> {
        let index = self.list.row_at(x, y)?;
        self.switch_rect(index).contains(x, y).then_some(index)
    }
}

impl Screen for ExperimentsScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let lang = ctx.lang;
        self.info = ctx
            .assets
            .font
            .wrap(lang.get("selectWorld.experiments.info"), CONTENT_W);
        let info_h = self.info.len() as f32 * LINE_HEIGHT;
        let content_h = info_h + INFO_GAP + SCROLL_MIN_H;
        let area_h = LAYOUT.bottom(height) - LAYOUT.top();
        let top = LAYOUT.top() + ((area_h - content_h) / 2.0).trunc().max(0.0);
        self.info_y = top;
        let scroll_top = top + info_h + INFO_GAP;
        let left = ((width - CONTENT_W) / 2.0).floor();
        self.list.set_area(
            left,
            scroll_top - 2.0,
            LAYOUT.bottom(height).max(scroll_top + 1.0),
            CONTENT_W,
        );
        self.list.set_len(self.rows.len());
        self.widgets.centered_row(
            &[Id::Done, Id::Cancel],
            width,
            LAYOUT.footer_button_y(height),
            SPACING,
        );
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.list.on_mouse_move(x, y);
        self.widgets.hover(x, y);
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    fn cursor(&self) -> CursorKind {
        if self.switch_at(self.mouse.0, self.mouse.1).is_some() {
            return CursorKind::Pointer;
        }
        self.list
            .cursor()
            .or(self.widgets.cursor())
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if let Some(id) = self.widgets.click(x, y) {
            ctx.click();
            if id == Id::Done {
                self.done();
            }
            return ScreenAction::Back;
        }
        if let Some(index) = self.switch_at(x, y) {
            ctx.click();
            self.rows[index].1 = !self.rows[index].1;
            return ScreenAction::None;
        }
        let _ = matches!(self.list.on_click(x, y), ListClick::Scrollbar);
        self.list.select(None);
        ScreenAction::None
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        if let KeyResult::Pressed(id) = self.widgets.key(&event) {
            ctx.click();
            if id == Id::Done {
                self.done();
            }
            return ScreenAction::Back;
        }
        ScreenAction::None
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        ui.title(ui.tr("selectWorld.experiments"), LAYOUT.title_y());

        let red = [colors::RED[0], colors::RED[1], colors::RED[2], 1.0];
        let left = ((ui.width - CONTENT_W) / 2.0).floor();
        let mut y = self.info_y;
        for line in &self.info {
            ui.text(line, left, y, red);
            y += LINE_HEIGHT;
        }

        let lang = ui.lang;
        self.list.draw_rows(ui, |ui, index, rect| {
            let (id, on) = self.rows[index];
            let name_key = format!("dataPack.{id}.name");
            let name = if lang.has(&name_key) {
                lang.get_dynamic(&name_key).to_owned()
            } else {
                id.to_owned()
            };
            ui.text(
                &name,
                rect.x,
                rect.y + ((BUTTON_H - LINE_HEIGHT) / 2.0).round(),
                [1.0; 4],
            );
            let mut button = Button::sized("", SWITCH_W, BUTTON_H);
            let switch = Rect::new(rect.right() - SWITCH_W, rect.y, SWITCH_W, BUTTON_H);
            button.x = switch.x;
            button.y = switch.y;
            button.hovered = switch.contains(self.mouse.0, self.mouse.1);
            button.draw_labeled(ui, lang.get(if on { "options.on" } else { "options.off" }));
            let desc_key = format!("dataPack.{id}.description");
            if lang.has(&desc_key) {
                let text = StyledText::colored(lang.get_dynamic(&desc_key), colors::GRAY);
                let lines = ui.wrap_styled(&text, GRID_W - SWITCH_W);
                for (i, line) in lines.iter().take(INFO_ROWS).enumerate() {
                    ui.styled(
                        line,
                        rect.x,
                        rect.y + BUTTON_H + i as f32 * LINE_HEIGHT,
                        colors::GRAY,
                    );
                }
            }
        });
        self.widgets.draw(ui);
    }
}
