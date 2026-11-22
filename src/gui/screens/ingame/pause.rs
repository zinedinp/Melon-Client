//! The Escape menu (vanilla `client/gui/screens/PauseScreen`).

use std::sync::Arc;

use crate::dialog::Dialog;
use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::screens::dialog::DialogRequest;
use crate::gui::screens::options::OptionsScreen;
use crate::gui::text::StyledText;
use crate::gui::widgets::Ui;
use crate::gui::widgets::Widget;
use crate::gui::widgets::button::{Button, IconSprite};
use crate::gui::widgets::set::{KeyResult, WidgetSet};
use crate::i18n::Lang;

/// `PauseScreen.BUTTON_WIDTH_FULL` / `BUTTON_WIDTH_HALF`, and the 20px button height.
const BUTTON_W: f32 = 204.0;
const HALF_BUTTON_W: f32 = 98.0;
const BUTTON_H: f32 = 20.0;
/// `PauseScreen.BUTTON_PADDING`, the grid's horizontal/top cell padding.
const PADDING: f32 = 4.0;
/// `PauseScreen.MENU_PADDING_TOP`: the first row's extra top padding.
const MENU_PADDING_TOP: f32 = 50.0;
/// `SpriteIconButton.builder(...).width(20)` with a 15x15 sprite, four of them in a row.
const ICON_BUTTON: f32 = 20.0;
const ICON_SPACING: f32 = 4.0;
const ICON_ROW_W: f32 = ICON_BUTTON * 4.0 + ICON_SPACING * 3.0;

/// Grid width.
const GRID_W: f32 = BUTTON_W + PADDING * 2.0;
/// Row heights.
const GRID_H: f32 = (BUTTON_H + MENU_PADDING_TOP) + (BUTTON_H + PADDING) * 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    ReturnToGame,
    Advancements,
    Statistics,
    /// Report Bugs / Give Feedback / Friends / Player Reporting, in the icon row.
    Icon(usize),
    /// `addCustomDialogButtons`: the server's links or options.
    Custom,
    Options,
    /// 26.3's `WorldOptionsScreen`, in a singleplayer world here.
    WorldOptions,
    Disconnect,
}

const ICONS: [Id; 4] = [Id::Icon(0), Id::Icon(1), Id::Icon(2), Id::Icon(3)];

pub struct PauseScreen {
    widgets: WidgetSet<Id>,
    /// `getCustomAdditions`: the dialog the custom button opens.
    custom: Option<Arc<Dialog>>,
    mouse: (f32, f32),
}

impl PauseScreen {
    pub fn new(local_server: bool, custom: Option<Arc<Dialog>>, lang: &Lang) -> Self {
        let icons: [(&str, IconSprite); 4] = [
            ("menu.reportBugs", |a| a.bug_icon),
            ("menu.sendFeedback", |a| a.feedback_icon),
            ("gui.friends.open", |a| a.friends_icon),
            ("menu.playerReporting", |a| a.player_reporting_icon),
        ];
        let mut widgets = WidgetSet::new()
            .with_button(
                Id::ReturnToGame,
                Button::sized("menu.returnToGame", BUTTON_W, BUTTON_H),
            )
            .with_button(
                Id::Advancements,
                Button::sized("gui.advancements", HALF_BUTTON_W, BUTTON_H),
            )
            .with_button(
                Id::Statistics,
                Button::sized("gui.stats", HALF_BUTTON_W, BUTTON_H),
            );
        for (id, (label, icon)) in ICONS.into_iter().zip(icons) {
            widgets = widgets.with_button(id, Button::icon(label, icon, ICON_BUTTON));
        }
        if let Some(dialog) = &custom {
            let mut button = Button::sized(String::new(), BUTTON_W, BUTTON_H);
            button.styled = Some(StyledText::from_component(
                dialog.common.external_title(),
                lang,
            ));
            widgets = widgets.with_button(Id::Custom, button);
        }
        let options_w = if local_server {
            HALF_BUTTON_W
        } else {
            BUTTON_W
        };
        let mut widgets = widgets
            .with_button(
                Id::Options,
                Button::sized("menu.options", options_w, BUTTON_H),
            )
            .with_button(
                Id::WorldOptions,
                Button::sized("options.worldOptions.button", HALF_BUTTON_W, BUTTON_H),
            )
            .with_button(
                Id::Disconnect,
                Button::sized(
                    if local_server {
                        "menu.returnToMenu"
                    } else {
                        "menu.disconnect"
                    },
                    BUTTON_W,
                    BUTTON_H,
                ),
            );
        widgets.set_visible(Id::WorldOptions, local_server);
        Self {
            widgets,
            custom,
            mouse: (0.0, 0.0),
        }
    }

    /// The grid's height: one row more with the custom button.
    fn grid_h(&self) -> f32 {
        GRID_H
            + if self.custom.is_some() {
                BUTTON_H + PADDING
            } else {
                0.0
            }
    }

    /// What a button does; the inert ones do nothing.
    fn press(&self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        match id {
            Id::ReturnToGame => ScreenAction::Back,
            Id::Custom => match &self.custom {
                Some(dialog) => ScreenAction::Dialog(DialogRequest::Show(Arc::clone(dialog))),
                None => ScreenAction::None,
            },
            Id::Options => ScreenAction::open(OptionsScreen::new(ctx)),
            Id::WorldOptions => ScreenAction::OpenWorldOptions,
            Id::Disconnect => ScreenAction::LeaveWorld,
            Id::Advancements | Id::Statistics | Id::Icon(_) => ScreenAction::None,
        }
    }
}

impl Screen for PauseScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let grid_x = ((width - GRID_W) * 0.5).round();
        let grid_y = ((height - self.grid_h()) * 0.25).round();
        let left = grid_x + PADDING;
        let w = &mut self.widgets;

        let mut row_y = grid_y + MENU_PADDING_TOP;
        w.set_position(Id::ReturnToGame, left, row_y);

        row_y += BUTTON_H + PADDING;
        w.set_position(Id::Advancements, left, row_y);
        w.set_position(Id::Statistics, left + HALF_BUTTON_W + PADDING * 2.0, row_y);

        row_y += BUTTON_H + PADDING;
        let icon_x = left + ((BUTTON_W - ICON_ROW_W) / 2.0).round();
        w.row(&ICONS, icon_x, row_y, ICON_SPACING);

        row_y += BUTTON_H + PADDING;
        if self.custom.is_some() {
            w.set_position(Id::Custom, left, row_y);
            row_y += BUTTON_H + PADDING;
        }
        w.set_position(Id::Options, left, row_y);
        w.set_position(Id::WorldOptions, left + HALF_BUTTON_W + PADDING * 2.0, row_y);
        w.set_position(Id::Disconnect, left, row_y + BUTTON_H + PADDING);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        let Some(id) = self.widgets.click(x, y) else {
            return ScreenAction::None;
        };
        let action = self.press(id, ctx);
        if !matches!(action, ScreenAction::None) {
            ctx.click();
        }
        action
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => {
                ctx.click();
                self.press(id, ctx)
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("menu.game");
        ui.title(title, 40.0);
        self.widgets.draw(ui);
        // `CUSTOM_OPTIONS_TOOLTIP`.
        if self.custom.is_some() {
            let button = self.widgets.button(Id::Custom);
            if button.hovered || button.focused {
                let text = ui.tr("menu.custom_options.tooltip");
                ui.widget_tooltip(text, self.mouse, button.rect());
            }
        }
    }
}
