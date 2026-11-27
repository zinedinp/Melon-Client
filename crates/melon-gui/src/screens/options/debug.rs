//! Vanilla `DebugOptionsScreen` (F3 + F6): every F3 entry, searchable,
//! each off, in the F3 overlay or always on; the two profiles; the debug
//! overlay's own GUI scale.

use crate::hud::debug::SharedEntries;
use crate::hud::debug_entries::{Category, Entry, Profile, Status};
use crate::screen::{ConfigChange, CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::text::{StyledText, TextStyle};
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, HeaderFooter};
use crate::widgets::scroll_list::{ListClick, ScrollList};
use crate::widgets::slider::Slider;
use crate::widgets::{Rect, Ui, Widget};
use melon_core::math::argb;

/// `new HeaderAndFooterLayout(this, 61, 33)`.
const LAYOUT: HeaderFooter = HeaderFooter::new(61.0, 33.0);
/// `OptionList.getRowWidth` and `ITEM_HEIGHT`.
const ROW_W: f32 = 350.0;
const ROW_H: f32 = 20.0;
/// `OptionEntry.BUTTON_WIDTH`, 16 high.
const STATUS_W: f32 = 60.0;
const STATUS_H: f32 = 16.0;
const PROFILE_W: f32 = 120.0;
const SCALE_W: f32 = 110.0;
const DONE_W: f32 = 60.0;
const FOOTER_GAP: f32 = 4.0;
/// `debug.options.warning`'s colour, `-2142128`.
const WARNING: u32 = 0xFFDF_5050;
/// The chosen button's text: always `-2142128`, overlay `-171`, off white;
/// the others `-4539718`.
const ALWAYS_COLOR: u32 = 0xFFDF_5050;
const OVERLAY_COLOR: u32 = 0xFFFF_FF55;
const NEVER_COLOR: u32 = 0xFFFF_FFFF;
const UNCHOSEN: u32 = 0xFFBA_BABA;
/// The name of an entry Melon does not have: italic, `-8355712`.
const NOT_THERE: u32 = 0xFF80_8080;
/// `AbstractSelectionList.Entry.CONTENT_PADDING`.
const CONTENT_PAD: f32 = 2.0;

enum Row {
    Category(Category),
    Entry {
        entry: Entry,
        /// Off, in the overlay, always: left to right.
        buttons: Box<[Button; 3]>,
    },
}

const STATUSES: [Status; 3] = [Status::Never, Status::InOverlay, Status::AlwaysOn];

pub struct DebugOptionsScreen {
    entries: SharedEntries,
    search: EditBox,
    rows: Vec<Row>,
    list: ScrollList,
    profiles: [Button; 2],
    scale: Slider,
    done: Button,
    pending: Vec<ConfigChange>,
    mouse: (f32, f32),
}

/// `options.guiScale` for the debug overlay: `-1` unchanged, `0` auto.
fn scale_label(ui: &Ui, value: i32) -> String {
    let value = match value {
        -1 => ui.tr("options.debugGuiScale.unchanged").to_owned(),
        0 => ui.tr("options.guiScale.auto").to_owned(),
        n => n.to_string(),
    };
    ui.lang.format(
        "options.generic_value",
        &[ui.tr("options.guiScale"), &value],
    )
}

pub fn debug_options_screen(entries: SharedEntries) -> DebugOptionsScreen {
    let mut screen = DebugOptionsScreen {
        entries,
        search: EditBox::new_search("debug.options.search", (ROW_W / 3.0).floor(), BUTTON_H),
        rows: Vec::new(),
        list: ScrollList::new(ROW_W, ROW_H),
        profiles: Profile::ALL.map(|p| Button::sized(p.label_key(), PROFILE_W, BUTTON_H)),
        scale: Slider::new(1, 0, SCALE_W, BUTTON_H),
        done: Button::sized("gui.done", DONE_W, BUTTON_H),
        pending: Vec::new(),
        mouse: (0.0, 0.0),
    };
    screen.search.set_focused(true);
    screen.update_search();
    screen
}

impl DebugOptionsScreen {
    /// `OptionList.updateSearch`: the entries whose path holds the search,
    /// by category then id, each category under its header.
    fn update_search(&mut self) {
        let value = self.search.value();
        let mut all: Vec<Entry> = Entry::ALL.to_vec();
        all.sort_by(|a, b| a.category().cmp(&b.category()).then(a.cmp_id(*b)));
        self.rows.clear();
        let mut category = None;
        for entry in all {
            if !entry.path().contains(value.as_str()) {
                continue;
            }
            if category != Some(entry.category()) {
                category = Some(entry.category());
                self.rows.push(Row::Category(entry.category()));
            }
            self.rows.push(Row::Entry {
                entry,
                buttons: Box::new(std::array::from_fn(|_| {
                    Button::sized("", STATUS_W, STATUS_H)
                })),
            });
        }
        self.list.set_len(self.rows.len());
        self.refresh();
        self.layout_rows();
    }

    /// `refreshEntry`: the chosen status's button is the one that cannot
    /// be pressed; a profile in use neither.
    fn refresh(&mut self) {
        let entries = self.entries.borrow();
        for row in &mut self.rows {
            if let Row::Entry { entry, buttons } = row {
                let status = entries.status(*entry);
                for (button, choice) in buttons.iter_mut().zip(STATUSES) {
                    button.enabled = status != choice;
                }
            }
        }
        for (button, profile) in self.profiles.iter_mut().zip(Profile::ALL) {
            button.enabled = !entries.is_using_profile(profile);
        }
    }

    fn layout_rows(&mut self) {
        for (index, row) in self.rows.iter_mut().enumerate() {
            let Row::Entry { buttons, .. } = row else {
                continue;
            };
            let rect = self.list.row_rect(index);
            let content_w = rect.w - 2.0 * CONTENT_PAD;
            let mut x = rect.x + CONTENT_PAD + content_w - 3.0 * STATUS_W;
            for button in buttons {
                button.x = x;
                button.y = rect.y + CONTENT_PAD;
                x += STATUS_W;
            }
        }
    }

    fn status_label(choice: Status, chosen: bool, ui: &Ui) -> StyledText {
        let (key, color) = match choice {
            Status::AlwaysOn => ("debug.entry.always", ALWAYS_COLOR),
            Status::InOverlay => ("debug.entry.overlay", OVERLAY_COLOR),
            Status::Never => ("options.off", NEVER_COLOR),
        };
        let rgb = argb(if chosen { color } else { UNCHOSEN });
        let mut label = StyledText::default();
        label.push(ui.tr(key), &TextStyle::colored([rgb[0], rgb[1], rgb[2]]));
        label
    }

    /// The overlay's GUI scale steps: `-1` (unchanged) to the window's
    /// largest.
    fn scale_steps(ctx: &ScreenContext) -> u32 {
        u32::from(ctx.scale.max_scale()) + 1
    }
}

impl Screen for DebugOptionsScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.list
            .set_bounds(LAYOUT.top(), LAYOUT.bottom(height), width);
        self.list.set_len(self.rows.len());
        // The header's first row: a third of the row as room, the title,
        // the search box; centred together.
        let title_w = ctx
            .assets
            .font
            .text_width(ctx.lang.get("debug.options.title"));
        let row_w = (ROW_W / 3.0).floor() + 8.0 + title_w + 8.0 + self.search.w;
        let left = ((width - row_w) / 2.0).floor();
        self.search.x = left + row_w - self.search.w;
        self.search.y = 8.0;
        let footer_w = 2.0 * PROFILE_W + SCALE_W + DONE_W + 3.0 * FOOTER_GAP;
        let mut x = ((width - footer_w) / 2.0).floor();
        let y = LAYOUT.footer_button_y(height);
        for button in &mut self.profiles {
            button.x = x;
            button.y = y;
            x += PROFILE_W + FOOTER_GAP;
        }
        let steps = Self::scale_steps(ctx);
        let value = ctx.config.debug_gui_scale.clamp(-1, steps as i32 - 1);
        self.scale = Slider::new(steps, (value + 1) as u32, SCALE_W, BUTTON_H);
        self.scale.x = x;
        self.scale.y = y;
        x += SCALE_W + FOOTER_GAP;
        self.done.x = x;
        self.done.y = y;
        self.layout_rows();
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.list.on_mouse_move(x, y);
        self.search.update_hover(x, y);
        for button in self.profiles.iter_mut().chain([&mut self.done]) {
            button.update_hover(x, y);
        }
        self.scale.update_hover(x, y);
        if self.scale.drag(x) {
            let value = self.scale.step() as i32 - 1;
            self.pending
                .push(Box::new(move |config| config.debug_gui_scale = value));
        }
        self.layout_rows();
        for row in &mut self.rows {
            if let Row::Entry { buttons, .. } = row {
                for button in buttons {
                    button.update_hover(x, y);
                }
            }
        }
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, ctx: &ScreenContext) {
        self.list.on_mouse_up();
        if self.scale.release() {
            ctx.click();
        }
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
        self.layout_rows();
    }

    fn cursor(&self) -> CursorKind {
        self.list
            .cursor()
            .or(self.done.cursor())
            .or(self.profiles.iter().find_map(Widget::cursor))
            .or(Widget::cursor(&self.scale))
            .or(Widget::cursor(&self.search))
            .or(self.rows.iter().find_map(|row| match row {
                Row::Entry { buttons, .. } => buttons.iter().find_map(Widget::cursor),
                Row::Category(_) => None,
            }))
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.search.click(x, y);
        if self.done.clicked(x, y) {
            ctx.click();
            return ScreenAction::Back;
        }
        for (i, profile) in Profile::ALL.into_iter().enumerate() {
            if self.profiles[i].clicked(x, y) && self.profiles[i].enabled {
                ctx.click();
                self.entries.borrow_mut().load_profile(profile);
                self.refresh();
                return ScreenAction::None;
            }
        }
        if let Some(changed) = self.scale.press(x, y) {
            if changed {
                let value = self.scale.step() as i32 - 1;
                self.pending
                    .push(Box::new(move |config| config.debug_gui_scale = value));
            }
            return self.poll();
        }
        if let ListClick::Row { index, .. } = self.list.on_click(x, y)
            && let Some(Row::Entry { entry, buttons }) = self.rows.get(index)
        {
            let entry = *entry;
            let choice = buttons
                .iter()
                .zip(STATUSES)
                .find(|(button, _)| button.enabled && button.clicked(x, y))
                .map(|(_, status)| status);
            if let Some(status) = choice {
                ctx.click();
                self.entries.borrow_mut().set_status(entry, status);
                self.refresh();
            }
        }
        ScreenAction::None
    }

    fn on_text_event(&mut self, event: TextEvent, _ctx: &ScreenContext) -> ScreenAction {
        if self.search.focused() && self.search.handle(&event) {
            self.update_search();
        }
        ScreenAction::None
    }

    fn poll(&mut self) -> ScreenAction {
        ScreenAction::change_config(std::mem::take(&mut self.pending))
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("debug.options.title");
        let title_x = self.search.x - 8.0 - ui.text_width(title);
        ui.text(
            title,
            title_x,
            self.search.y + (BUTTON_H - 8.0) / 2.0,
            [1.0; 4],
        );
        self.search.draw(ui);
        let warning = ui.tr("debug.options.warning");
        let lines = ui.assets.font.wrap(warning, ROW_W);
        let center = (ui.width / 2.0).floor();
        for (i, line) in lines.iter().enumerate() {
            ui.centered(
                line,
                center,
                8.0 + BUTTON_H + 8.0 + 9.0 * i as f32,
                argb(WARNING),
            );
        }
        let entries = self.entries.borrow();
        let mut tooltip = None;
        self.list
            .draw(ui, |ui, index, rect| match &self.rows[index] {
                Row::Category(category) => {
                    let text = ui.tr(category.label_key());
                    let y =
                        rect.y + CONTENT_PAD + ((rect.h - 2.0 * CONTENT_PAD - 9.0) / 2.0).floor();
                    ui.section_header(text, y);
                }
                Row::Entry { entry, buttons } => {
                    let x = rect.x + CONTENT_PAD;
                    let y = rect.y + CONTENT_PAD;
                    if entry.works() {
                        ui.text(entry.path(), x, y + 5.0, [1.0; 4]);
                    } else {
                        let name = format!("§o{}", entry.path());
                        let width = ui.text(&name, x, y + 5.0, argb(NOT_THERE));
                        // `NOT_ALLOWED_TOOLTIP`'s place, left of the buttons.
                        let name_rect = Rect::new(x, y, (buttons[0].x - x).max(width), STATUS_H);
                        if name_rect.contains(self.mouse.0, self.mouse.1)
                            && self.mouse.1 >= self.list.top()
                            && self.mouse.1 < self.list.bottom()
                        {
                            tooltip = Some(name_rect);
                        }
                    }
                    let status = entries.status(*entry);
                    for (button, choice) in buttons.iter().zip(STATUSES) {
                        button.draw_styled(ui, &Self::status_label(choice, status == choice, ui));
                    }
                }
            });
        for (button, profile) in self.profiles.iter().zip(Profile::ALL) {
            button.draw_labeled(ui, ui.tr(profile.label_key()));
        }
        let value = self.scale.step() as i32 - 1;
        self.scale.draw_labeled(ui, &scale_label(ui, value));
        self.done.draw(ui);
        if let Some(rect) = tooltip {
            ui.widget_tooltip(
                ui.tr("melon.debug.options.missing.tooltip"),
                self.mouse,
                rect,
            );
        }
        let _ = ctx;
    }
}
