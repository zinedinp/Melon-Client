//! Vanilla `LanguageSelectScreen` / `LanguageSelectionList` (`ObjectSelectionList`).

use melon_assets::i18n::available_languages;
use melon_assets::i18n::languages::Language;
use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::screens::menu::placeholder::placeholder_screen;
use crate::text::font::LINE_HEIGHT;
use crate::widgets::Ui;
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, SPACING, wide_column_left};
use crate::widgets::scroll_list::{ListClick, ScrollList};
use crate::widgets::set::{KeyResult, WidgetSet};
use melon_assets::assets::Assets;

/// `AbstractSelectionList`'s default row width (`220`), `+50` as `LanguageSelectionList`
/// overrides it.
const ROW_W: f32 = 270.0;
/// `itemHeight` passed to the `ObjectSelectionList` constructor.
const ROW_H: f32 = 18.0;
/// The header (`OptionsSubScreen.addTitle`'s) and the footer with the warning.
const LAYOUT: HeaderFooter = HeaderFooter::new(36.0, 53.0);
const WARNING_COLOR: [f32; 4] = [0.73, 0.73, 0.73, 1.0];

fn label(language: Language) -> String {
    format!("{} ({})", language.name(), language.region())
}

pub struct LanguageScreen {
    entries: Vec<Language>,
    /// Each entry's row text, `Name (Region)`.
    labels: Vec<String>,
    /// Visible `entries` indices after the search filter; the list's rows.
    filtered: Vec<usize>,
    list: ScrollList,
    /// The search box, and the footer's buttons.
    widgets: WidgetSet<Id>,
    height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Search,
    FontSettings,
    Done,
}

impl LanguageScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        let assets = Assets::from_root(ctx.assets_root.to_path_buf());
        let entries = available_languages(&assets);
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let selected = entries.iter().position(|&e| e == ctx.lang.language);

        let mut list = ScrollList::new(ROW_W, ROW_H);
        list.set_len(filtered.len());
        list.select(selected);
        let mut screen = Self {
            labels: entries.iter().map(|&e| label(e)).collect(),
            entries,
            filtered,
            list,
            widgets: WidgetSet::new()
                .with_field(
                    Id::Search,
                    EditBox::new_search("gui.language.search", 200.0, 15.0),
                )
                .with_button(
                    Id::FontSettings,
                    Button::sized("options.font", BUTTON_W, BUTTON_H),
                )
                .with_button(Id::Done, Button::sized("gui.done", BUTTON_W, BUTTON_H)),
            height: ctx.height(),
        };
        screen.widgets.focus(Id::Search);
        screen.resize(ctx);
        if let Some(selected) = selected {
            screen
                .list
                .center_on(selected, ctx.height() - 33.0 - LAYOUT.footer);
        }
        screen
    }

    fn apply_filter(&mut self) {
        let needle = self.widgets.field(Id::Search).value().to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                needle.is_empty()
                    || e.name().to_lowercase().contains(&needle)
                    || e.region().to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        self.list.set_len(self.filtered.len());
        self.list.select(None);
    }

    fn selected_language(&self) -> Option<Language> {
        self.list
            .selected()
            .map(|row| self.entries[self.filtered[row]])
    }
}

impl LanguageScreen {
    fn press(&self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        match id {
            Id::FontSettings => ScreenAction::open(placeholder_screen("options.font")),
            Id::Done => match self.selected_language() {
                Some(language) if language != ctx.lang.language => {
                    ScreenAction::SetLanguage(language)
                }
                _ => ScreenAction::Back,
            },
            Id::Search => ScreenAction::None,
        }
    }
}

impl Screen for LanguageScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.height = height;
        self.widgets
            .set_position(Id::Search, wide_column_left(width), 17.0);
        self.list
            .set_bounds(LAYOUT.top(), LAYOUT.bottom(height), width);

        let button_y = LAYOUT.bottom(height) + 8.0 + LINE_HEIGHT + SPACING;
        self.widgets
            .centered_row(&[Id::FontSettings, Id::Done], width, button_y, SPACING);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
        self.list.on_mouse_move(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets
            .cursor()
            .or(self.list.cursor())
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        self.list.blur();
        match self.widgets.click(x, y) {
            Some(Id::Search) => ScreenAction::None,
            Some(id) => self.press(id, ctx),
            None => {
                let _: ListClick = self.list.on_click(x, y);
                ScreenAction::None
            }
        }
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => self.press(id, ctx),
            KeyResult::Edited(_) => {
                self.apply_filter();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("options.language.title");
        ui.title(title, 4.0);

        self.list.draw(ui, |ui, row, rect| {
            let text = &self.labels[self.filtered[row]];
            let text_w = ui.text_width(text).ceil();
            let x = (ui.width / 2.0).floor() - (text_w / 2.0).floor();
            ui.text(text, x, rect.y + 5.0, [1.0; 4]);
        });

        self.widgets.draw(ui);
        let warning = ui.tr("options.languageAccuracyWarning");
        ui.title_colored(warning, LAYOUT.bottom(self.height) + 8.0, WARNING_COLOR);
    }
}
