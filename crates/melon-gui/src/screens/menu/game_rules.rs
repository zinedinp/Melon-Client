//! Vanilla `WorldCreationGameRulesScreen` (`AbstractGameRulesScreen`).

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::text::font::LINE_HEIGHT;
use crate::text::{StyledText, TextStyle, colors};
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, SPACING};
use crate::widgets::scroll_list::ScrollList;
use crate::widgets::set::{KeyResult, WidgetSet};
use crate::widgets::{Rect, Ui};
use melon_net::singleplayer::rules::{self, Category, Changes, Rule, Value};

/// `RuleList.ITEM_HEIGHT`, and `getRowWidth()`.
const ROW_H: f32 = 24.0;
const ROW_W: f32 = 220.0;
/// `new EditBox(font, 10, 5, 44, 20, ..)` / `create(10, 5, 44, 20, ..)`.
const VALUE_W: f32 = 44.0;
/// `SEARCH_BOX_HEIGHT` and its width.
const SEARCH_H: f32 = 15.0;
const SEARCH_W: f32 = 200.0;
/// `GameRuleEntry`'s caption is split at 170 and at most two lines.
const CAPTION_W: f32 = 170.0;
/// The description in a tooltip is split at 150.
const TOOLTIP_W: f32 = 150.0;
/// `(int)(12.0 + 9.0 + 15.0)`: title, gap, search box.
const LAYOUT: HeaderFooter = HeaderFooter::new(36.0, 33.0);
/// `EditBox.setTextColor(-65536)` for a value that doesn't parse.
const INVALID: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Search,
    Done,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Row {
    Category(Category),
    /// An index into the screen's rules.
    Rule(usize),
}

pub struct GameRulesScreen {
    rules: Vec<Rule>,
    /// What the screen edits: the Create World screen's changes, copied.
    work: Changes,
    /// Where Done writes them.
    output: Rc<RefCell<Changes>>,
    /// Number boxes by rule, kept while the filter hides their rows.
    inputs: FxHashMap<usize, EditBox>,
    /// The rules whose box holds something that isn't a valid value.
    invalid: FxHashSet<usize>,
    /// The rows the filter leaves.
    rows: Vec<Row>,
    /// The rule whose box has the keyboard focus (the search box otherwise).
    typing: Option<usize>,
    list: ScrollList,
    widgets: WidgetSet<Id>,
    mouse: (f32, f32),
    width: f32,
}

impl GameRulesScreen {
    /// `enabled` are the data packs and experiments on; a rule that needs another is left out.
    pub fn new(output: Rc<RefCell<Changes>>, enabled: &[String]) -> Self {
        let rules: Vec<Rule> = rules::all()
            .into_iter()
            .filter(|rule| {
                rule.feature
                    .is_none_or(|feature| enabled.iter().any(|id| id == feature))
            })
            .collect();
        let work = output.borrow().clone();
        let mut inputs = FxHashMap::default();
        for (index, rule) in rules.iter().enumerate() {
            if let Value::Int(i) = work.get(&rule.name).copied().unwrap_or(rule.default) {
                let mut field = EditBox::new("", VALUE_W, BUTTON_H);
                field.set_value(&i.to_string());
                inputs.insert(index, field);
            }
        }
        let mut widgets = WidgetSet::new()
            .with_field(
                Id::Search,
                EditBox::new_search("gui.game_rule.search", SEARCH_W, SEARCH_H),
            )
            .with_button(Id::Done, Button::sized("gui.done", BUTTON_W, BUTTON_H))
            .with_button(Id::Cancel, Button::sized("gui.cancel", BUTTON_W, BUTTON_H));
        widgets.focus(Id::Search);
        Self {
            rules,
            work,
            output,
            inputs,
            invalid: FxHashSet::default(),
            rows: Vec::new(),
            typing: None,
            list: ScrollList::new(ROW_W, ROW_H),
            widgets,
            mouse: (-1.0, -1.0),
            width: 0.0,
        }
    }

    fn value(&self, index: usize) -> Value {
        let rule = &self.rules[index];
        self.work.get(&rule.name).copied().unwrap_or(rule.default)
    }

    /// `GameRules.set`.
    fn set(&mut self, index: usize, value: Value) {
        let rule = &self.rules[index];
        if value == rule.default {
            self.work.remove(&rule.name);
        } else {
            self.work.insert(rule.name.clone(), value);
        }
    }

    /// The caption of a rule: its language entry, else its name spelled out.
    fn caption(rule: &Rule, lang: &melon_assets::i18n::Lang) -> String {
        let key = rules::lang_key(&rule.name);
        if lang.has(&key) {
            lang.get_dynamic(&key).to_owned()
        } else {
            rules::humanized(&rule.name)
        }
    }

    fn description(rule: &Rule, lang: &melon_assets::i18n::Lang) -> Option<String> {
        let key = format!("{}.description", rules::lang_key(&rule.name));
        lang.has(&key).then(|| lang.get_dynamic(&key).to_owned())
    }

    /// `RuleList.populateChildren`.
    fn populate(&mut self, lang: &melon_assets::i18n::Lang) {
        let needle = self.widgets.field(Id::Search).value().to_lowercase();
        let mut shown: Vec<(Category, String, usize)> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| {
                needle.is_empty()
                    || rule.name.to_lowercase().contains(&needle)
                    || Self::caption(rule, lang).to_lowercase().contains(&needle)
                    || lang
                        .get_dynamic(&rule.category.lang_key())
                        .to_lowercase()
                        .contains(&needle)
                    || Self::description(rule, lang)
                        .is_some_and(|d| d.to_lowercase().contains(&needle))
            })
            .map(|(i, rule)| (rule.category, format!("gamerule.{}", rule.name), i))
            .collect();
        shown.sort();
        self.rows.clear();
        let mut category = None;
        for (cat, _, index) in shown {
            if category != Some(cat) {
                category = Some(cat);
                self.rows.push(Row::Category(cat));
            }
            self.rows.push(Row::Rule(index));
        }
        self.list.set_len(0);
        self.list.set_len(self.rows.len());
        self.layout_controls();
    }

    /// Moves every number box to its row, which follows the scrolling.
    fn layout_controls(&mut self) {
        for row in 0..self.rows.len() {
            if let Row::Rule(index) = self.rows[row] {
                let rect = self.control_rect(row);
                if let Some(field) = self.inputs.get_mut(&index) {
                    field.x = rect.x;
                    field.y = rect.y;
                }
            }
        }
    }

    /// The control of a rule's row, at the right of its content.
    fn control_rect(&self, row: usize) -> Rect {
        let rect = self.list.row_rect(row);
        Rect::new(
            rect.right() - 2.0 - VALUE_W - 1.0,
            rect.y + 2.0,
            VALUE_W,
            BUTTON_H,
        )
    }

    /// The rule row and its index under `(x, y)`.
    fn rule_row_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let row = self.list.row_at(x, y)?;
        match self.rows[row] {
            Row::Rule(index) => Some((row, index)),
            Row::Category(_) => None,
        }
    }

    fn can_finish(&self) -> bool {
        self.invalid.is_empty()
    }

    /// `closeAndApplyChanges`.
    fn apply(&self) {
        *self.output.borrow_mut() = self.work.clone();
    }

    /// Takes the keyboard focus to a rule's box (or the search box).
    fn focus_box(&mut self, target: Option<usize>) {
        if let Some(old) = self.typing.and_then(|i| self.inputs.get_mut(&i)) {
            old.set_focused(false);
        }
        self.typing = target;
        match target {
            Some(index) => {
                self.widgets.blur();
                if let Some(field) = self.inputs.get_mut(&index) {
                    field.set_focused(true);
                }
            }
            None => self.widgets.focus(Id::Search),
        }
    }

    /// `IntegerRuleEntry`'s responder.
    fn number_edited(&mut self, index: usize) {
        let text = self.inputs[&index].value();
        match self.rules[index].parse_int(&text) {
            Some(n) => {
                self.invalid.remove(&index);
                if let Some(field) = self.inputs.get_mut(&index) {
                    field.set_text_color(None);
                }
                self.set(index, Value::Int(n));
            }
            None => {
                self.invalid.insert(index);
                if let Some(field) = self.inputs.get_mut(&index) {
                    field.set_text_color(Some(INVALID));
                }
            }
        }
        let finish = self.can_finish();
        self.widgets.set_enabled(Id::Done, finish);
    }

    /// The tooltip of a rule: its id, its description, its default.
    fn tooltip(&self, index: usize, ui: &Ui) -> Vec<StyledText> {
        let rule = &self.rules[index];
        let mut lines = vec![StyledText::colored(&rule.name, colors::YELLOW)];
        if let Some(description) = Self::description(rule, ui.lang) {
            for line in ui.assets.font.wrap(&description, TOOLTIP_W) {
                lines.push(StyledText::plain(&line));
            }
        }
        let default = ui
            .lang
            .format("editGamerule.default", &[Rule::text(rule.default).as_str()]);
        lines.push(StyledText::colored(&default, colors::GRAY));
        lines
    }
}

impl Screen for GameRulesScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.width = width;
        self.populate(ctx.lang);
        self.list
            .set_bounds(LAYOUT.top(), LAYOUT.bottom(height), width);
        let search_x = (width / 2.0 - SEARCH_W / 2.0).floor();
        self.widgets
            .set_position(Id::Search, search_x, 4.0 + LINE_HEIGHT + 4.0);
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
        self.layout_controls();
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
        self.layout_controls();
    }

    fn cursor(&self) -> CursorKind {
        if let Some((row, index)) = self.rule_row_at(self.mouse.0, self.mouse.1)
            && self.control_rect(row).contains(self.mouse.0, self.mouse.1)
        {
            return if self.inputs.contains_key(&index) {
                CursorKind::Text
            } else {
                CursorKind::Pointer
            };
        }
        self.list
            .cursor()
            .or(self.widgets.cursor())
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if let Some(id) = self.widgets.click(x, y) {
            match id {
                Id::Done => {
                    ctx.click();
                    self.apply();
                    return ScreenAction::Back;
                }
                Id::Cancel => {
                    ctx.click();
                    return ScreenAction::Back;
                }
                Id::Search => {
                    self.focus_box(None);
                    return ScreenAction::None;
                }
            }
        }
        let hit = self
            .rule_row_at(x, y)
            .filter(|&(row, _)| self.control_rect(row).contains(x, y));
        match hit {
            Some((_, index)) if self.inputs.contains_key(&index) => {
                self.focus_box(Some(index));
                if let Some(field) = self.inputs.get_mut(&index) {
                    field.click(x, y);
                }
            }
            Some((_, index)) => {
                ctx.click();
                self.focus_box(None);
                self.widgets.blur();
                if let Value::Bool(on) = self.value(index) {
                    self.set(index, Value::Bool(!on));
                }
            }
            None => {
                self.list.on_click(x, y);
                self.list.select(None);
                if let Some(old) = self.typing.and_then(|i| self.inputs.get_mut(&i)) {
                    old.set_focused(false);
                }
                self.typing = None;
            }
        }
        ScreenAction::None
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        if let Some(index) = self.typing {
            if let Some(field) = self.inputs.get_mut(&index)
                && field.handle(&event)
            {
                self.number_edited(index);
            }
            return ScreenAction::None;
        }
        match self.widgets.key(&event) {
            KeyResult::Pressed(Id::Done) => {
                ctx.click();
                self.apply();
                ScreenAction::Back
            }
            KeyResult::Pressed(Id::Cancel) => {
                ctx.click();
                ScreenAction::Back
            }
            KeyResult::Edited(Id::Search) => {
                self.populate(ctx.lang);
                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let center = (self.width / 2.0).floor();
        ui.centered(ui.tr("editGamerule.title"), center, 4.0, [1.0; 4]);
        ui.list_background(LAYOUT.top(), LAYOUT.bottom(ui.height));
        let lang = ui.lang;
        self.list.draw_rows(ui, |ui, row, rect| {
            let (content_x, content_y) = (rect.x + 2.0, rect.y + 2.0);
            match self.rows[row] {
                Row::Category(category) => {
                    let style = TextStyle {
                        bold: true,
                        underlined: true,
                        color: Some(colors::YELLOW),
                        ..Default::default()
                    };
                    let mut label = StyledText::default();
                    label.push(lang.get_dynamic(&category.lang_key()), &style);
                    let x = (rect.x + rect.w / 2.0 - ui.styled_width(&label) / 2.0).floor();
                    ui.styled(&label, x, content_y + 6.0, colors::WHITE);
                }
                Row::Rule(index) => {
                    let rule = &self.rules[index];
                    let caption = Self::caption(rule, lang);
                    let lines = ui.assets.font.wrap(&caption, CAPTION_W);
                    if lines.len() == 1 {
                        ui.text(&lines[0], content_x, content_y + 5.0, [1.0; 4]);
                    } else {
                        for (i, line) in lines.iter().take(2).enumerate() {
                            ui.text(line, content_x, content_y + i as f32 * 10.0, [1.0; 4]);
                        }
                    }
                    let control = self.control_rect(row);
                    match self.inputs.get(&index) {
                        Some(field) => field.draw(ui),
                        None => {
                            let mut button = Button::sized("", VALUE_W, BUTTON_H);
                            button.x = control.x;
                            button.y = control.y;
                            button.hovered = control.contains(self.mouse.0, self.mouse.1);
                            let on = matches!(self.value(index), Value::Bool(true));
                            button.draw_labeled(
                                ui,
                                lang.get(if on { "options.on" } else { "options.off" }),
                            );
                        }
                    }
                }
            }
        });
        self.widgets.draw(ui);

        if let Some((_, index)) = self.rule_row_at(self.mouse.0, self.mouse.1) {
            let lines = self.tooltip(index, ui);
            ui.mouse_tooltip(&lines, self.mouse);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(changes: Changes, enabled: &[String]) -> GameRulesScreen {
        GameRulesScreen::new(Rc::new(RefCell::new(changes)), enabled)
    }

    fn index(screen: &GameRulesScreen, name: &str) -> usize {
        screen.rules.iter().position(|r| r.name == name).unwrap()
    }

    #[test]
    fn a_value_back_at_its_default_is_no_change() {
        let mut screen = screen(Changes::new(), &[]);
        let keep = index(&screen, "keep_inventory");
        screen.set(keep, Value::Bool(true));
        assert_eq!(screen.work.get("keep_inventory"), Some(&Value::Bool(true)));
        screen.set(keep, Value::Bool(false));
        assert!(screen.work.is_empty());
    }

    #[test]
    fn done_writes_the_changes_and_cancel_leaves_them() {
        let output = Rc::new(RefCell::new(Changes::new()));
        let mut screen = GameRulesScreen::new(output.clone(), &[]);
        let keep = index(&screen, "keep_inventory");
        screen.set(keep, Value::Bool(true));
        assert!(output.borrow().is_empty());
        screen.apply();
        assert_eq!(
            output.borrow().get("keep_inventory"),
            Some(&Value::Bool(true))
        );
    }

    #[test]
    fn the_boxes_start_from_the_given_changes() {
        let mut changes = Changes::new();
        changes.insert("max_entity_cramming".to_owned(), Value::Int(3));
        let screen = screen(changes, &[]);
        let at = index(&screen, "max_entity_cramming");
        assert_eq!(screen.inputs[&at].value(), "3");
    }

    #[test]
    fn a_number_out_of_range_is_invalid_and_holds_done() {
        let mut screen = screen(Changes::new(), &[]);
        let snow = index(&screen, "max_snow_accumulation_height");
        screen.inputs.get_mut(&snow).unwrap().set_value("9");
        screen.number_edited(snow);
        assert!(!screen.can_finish());
        assert!(screen.work.is_empty());
        screen.inputs.get_mut(&snow).unwrap().set_value("4");
        screen.number_edited(snow);
        assert!(screen.can_finish());
        assert_eq!(
            screen.work.get("max_snow_accumulation_height"),
            Some(&Value::Int(4))
        );
    }

    #[test]
    fn a_feature_rule_shows_only_with_its_pack() {
        assert!(
            screen(Changes::new(), &[])
                .rules
                .iter()
                .all(|r| r.name != "max_minecart_speed")
        );
        let with = screen(Changes::new(), &["minecart_improvements".to_owned()]);
        assert!(with.rules.iter().any(|r| r.name == "max_minecart_speed"));
    }
}
