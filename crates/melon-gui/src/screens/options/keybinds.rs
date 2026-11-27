//! Vanilla `KeyBindsScreen` / `KeyBindsList`.

use crate::screen::{ConfigChange, CursorKind, Screen, ScreenAction, ScreenContext};
use melon_settings::input::{self, Action, InputBind};
use melon_settings::options::Config;
use crate::text::{StyledText, TextStyle, colors};
use crate::widgets::button::Button;
use crate::widgets::layout::{BUTTON_H, HeaderFooter, SPACING, WIDE_BUTTON_W};
use crate::widgets::scroll_list::{ListClick, ScrollList};
use crate::widgets::{Rect, Ui, Widget};

/// `KeyBindsList.ITEM_HEIGHT`.
const ROW_H: f32 = 20.0;
/// `KeyBindsList.getRowWidth`.
const ROW_W: f32 = 340.0;
const CHANGE_W: f32 = 75.0;
const RESET_W: f32 = 50.0;
/// `KeyEntry.PADDING` between the reset button and the scrollbar.
const RESET_PAD: f32 = 10.0;
const CHANGE_GAP: f32 = 5.0;
/// Collision stripe left of the change button.
const STRIPE_W: f32 = 3.0;
/// `AbstractSelectionList.Entry.CONTENT_PADDING`.
const CONTENT_PAD: f32 = 2.0;

const LAYOUT: HeaderFooter = HeaderFooter::DEFAULT;

enum Row {
    Category(&'static str),
    Bind {
        action: Action,
        change: Button,
        reset: Button,
    },
}

pub struct KeyBindsScreen {
    rows: Vec<Row>,
    list: ScrollList,
    reset_all: Button,
    done: Button,
    selected: Option<Action>,
    pending: Vec<ConfigChange>,
}

impl KeyBindsScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        let mut rows = Vec::new();
        for &category in input::CATEGORY_ORDER {
            rows.push(Row::Category(category));
            for &action in Action::ALL {
                if action.category_key() == category {
                    rows.push(Row::Bind {
                        action,
                        change: Button::sized("", CHANGE_W, BUTTON_H),
                        reset: Button::sized("controls.reset", RESET_W, BUTTON_H),
                    });
                }
            }
        }
        let mut screen = Self {
            rows,
            list: ScrollList::new(ROW_W, ROW_H),
            reset_all: Button::sized("controls.resetAll", WIDE_BUTTON_W, BUTTON_H),
            done: Button::sized("gui.done", WIDE_BUTTON_W, BUTTON_H),
            selected: None,
            pending: Vec::new(),
        };
        screen.refresh_buttons(ctx.config);
        screen
    }

    fn refresh_buttons(&mut self, config: &Config) {
        self.reset_all.enabled = config.input.binds.any_changed();
        for row in &mut self.rows {
            if let Row::Bind { action, reset, .. } = row {
                reset.enabled = !config.input.binds.is_default(*action);
            }
        }
    }

    fn set_bind(&mut self, action: Action, bind: InputBind) {
        self.pending.push(Box::new(move |config: &mut Config| {
            config.input.binds.set(action, bind);
        }));
        self.selected = None;
    }

    /// `KeyEntry.updateChangeButtonMessage`.
    fn change_label(
        action: Action,
        config: &Config,
        selected: Option<Action>,
        ui: &Ui,
    ) -> StyledText {
        let bind = config.input.binds.get(action);
        let name = bind.label(ui.lang);
        let yellow = TextStyle::colored(colors::YELLOW);
        let white = TextStyle::colored(colors::WHITE);
        let collision = config.input.binds.has_collision(action);
        let mut label = StyledText::default();
        if selected == Some(action) {
            let underlined = TextStyle {
                underlined: true,
                ..white
            };
            let key = if collision {
                format!("[ {name} ]")
            } else {
                name.into_owned()
            };
            label.push("> ", &yellow);
            label.push(&key, &underlined);
            label.push(" <", &yellow);
        } else if collision {
            label.push("[ ", &yellow);
            label.push(&name, &white);
            label.push(" ]", &yellow);
        } else {
            label.push(&name, &white);
        }
        label
    }
}

impl Screen for KeyBindsScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.list
            .set_bounds(LAYOUT.top(), LAYOUT.bottom(height), width);
        self.list.set_len(self.rows.len());
        let y = LAYOUT.footer_button_y(height);
        let pair = WIDE_BUTTON_W * 2.0 + SPACING;
        let left = (width / 2.0 - pair / 2.0).floor();
        self.reset_all.x = left;
        self.reset_all.y = y;
        self.done.x = left + WIDE_BUTTON_W + SPACING;
        self.done.y = y;
        self.layout_row_buttons();
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.list.on_mouse_move(x, y);
        self.reset_all.update_hover(x, y);
        self.done.update_hover(x, y);
        self.layout_row_buttons();
        for row in &mut self.rows {
            if let Row::Bind { change, reset, .. } = row {
                change.update_hover(x, y);
                reset.update_hover(x, y);
            }
        }
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {
        self.list.on_mouse_up();
    }

    fn on_scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
        self.layout_row_buttons();
    }

    fn cursor(&self) -> CursorKind {
        self.list
            .cursor()
            .or(self.reset_all.cursor())
            .or(self.done.cursor())
            .or(self.rows.iter().find_map(|row| match row {
                Row::Bind { change, reset, .. } => change.cursor().or(reset.cursor()),
                Row::Category(_) => None,
            }))
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if self.done.clicked(x, y) {
            ctx.click();
            return ScreenAction::Back;
        }
        if self.reset_all.clicked(x, y) && self.reset_all.enabled {
            ctx.click();
            self.pending
                .push(Box::new(|config| config.input.binds.reset_all()));
            self.selected = None;
            return self.poll();
        }
        if let ListClick::Row { index, .. } = self.list.on_click(x, y)
            && let Some(Row::Bind {
                action,
                change,
                reset,
            }) = self.rows.get(index)
        {
            let action = *action;
            if reset.clicked(x, y) && reset.enabled {
                ctx.click();
                self.pending.push(Box::new(move |config| {
                    config.input.binds.reset(action);
                }));
                self.selected = None;
            } else if change.clicked(x, y) {
                ctx.click();
                self.selected = Some(action);
            }
        }
        self.poll()
    }

    fn capturing_bind(&self) -> bool {
        self.selected.is_some()
    }

    fn closes_on_escape(&self) -> bool {
        self.selected.is_none()
    }

    fn on_bind(&mut self, bind: InputBind, ctx: &ScreenContext) -> ScreenAction {
        let Some(action) = self.selected else {
            return ScreenAction::None;
        };
        ctx.click();
        self.set_bind(action, bind);
        self.poll()
    }

    fn poll(&mut self) -> ScreenAction {
        ScreenAction::change_config(std::mem::take(&mut self.pending))
    }

    fn refresh_config(&mut self, config: &Config) {
        self.refresh_buttons(config);
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        ui.menu_background();
        ui.title(ui.tr("controls.keybinds.title"), LAYOUT.title_y());
        self.list
            .draw(ui, |ui, index, rect| match &self.rows[index] {
                Row::Category(key) => {
                    let text = ui.tr(key);
                    let y = rect.y + CONTENT_PAD + (rect.h - 2.0 * CONTENT_PAD) - (9.0 + 4.0);
                    ui.section_header(text, y);
                }
                Row::Bind {
                    action,
                    change,
                    reset,
                } => {
                    let name = ui.tr(action.vanilla_name());
                    let content_h = rect.h - 2.0 * CONTENT_PAD;
                    let name_y = rect.y + CONTENT_PAD + (content_h / 2.0).floor() - 4.0;
                    ui.text(name, rect.x + CONTENT_PAD, name_y, [1.0; 4]);
                    if ctx.config.input.binds.has_collision(*action) {
                        ui.fill(
                            Rect::new(
                                change.x - 6.0,
                                rect.y + CONTENT_PAD - 1.0,
                                STRIPE_W,
                                content_h + 1.0,
                            ),
                            [1.0, 1.0, 0.0, 1.0],
                        );
                    }
                    let label = Self::change_label(*action, ctx.config, self.selected, ui);
                    change.draw_styled(ui, &label);
                    reset.draw(ui);
                }
            });
        self.reset_all.draw(ui);
        self.done.draw(ui);
    }
}

impl KeyBindsScreen {
    fn layout_row_buttons(&mut self) {
        for (index, row) in self.rows.iter_mut().enumerate() {
            let Row::Bind { change, reset, .. } = row else {
                continue;
            };
            let rect = self.list.row_rect(index);
            let reset_x = rect.right() - RESET_W - RESET_PAD;
            reset.x = reset_x;
            reset.y = rect.y;
            change.x = reset_x - CHANGE_GAP - CHANGE_W;
            change.y = rect.y;
        }
    }
}
