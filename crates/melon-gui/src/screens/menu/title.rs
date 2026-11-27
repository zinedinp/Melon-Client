//! Vanilla `TitleScreen.init()` / `extractRenderState`.

use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::screens::menu::create_world::CreateWorldScreen;
use crate::screens::menu::multiplayer::MultiplayerScreen;
use crate::screens::menu::placeholder::placeholder_screen;
use crate::screens::menu::singleplayer::SingleplayerScreen;
use crate::screens::options::OptionsScreen;
use crate::screens::options::language::LanguageScreen;
use crate::text::splash;
use crate::widgets::button::Button;
use crate::widgets::layout::{BUTTON_H, WIDE_BUTTON_W, quarter};
use crate::widgets::set::{KeyResult, WidgetSet};
use crate::widgets::{Ui, Widget};
use melon_net::singleplayer;
use std::time::Instant;

const HALF_BUTTON_W: f32 = 98.0;
const SPACING: f32 = 24.0;
const ICON_SIZE: f32 = 20.0;
const ICON_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Singleplayer,
    Multiplayer,
    Realms,
    Options,
    Quit,
    Friends,
    Language,
    Accessibility,
}

pub struct TitleScreen {
    widgets: WidgetSet<Id>,
    /// Where the mouse last was, for the icon buttons' tooltips.
    mouse: (f32, f32),
    splash_text: String,
    splash_start: Instant,
    /// `"Melon <version> <suffix>"`, built once per language.
    version: String,
}

fn version_text(suffix: &str) -> String {
    format!("Melon {} {suffix}", env!("CARGO_PKG_VERSION"))
}

impl TitleScreen {
    pub fn new(splashes: &[String], ctx: &ScreenContext) -> Self {
        Self {
            widgets: WidgetSet::new()
                .with_button(
                    Id::Singleplayer,
                    Button::sized("menu.singleplayer", WIDE_BUTTON_W, BUTTON_H),
                )
                .with_button(
                    Id::Multiplayer,
                    Button::sized("menu.multiplayer", WIDE_BUTTON_W, BUTTON_H),
                )
                .with_button(
                    Id::Realms,
                    Button::sized("menu.online", WIDE_BUTTON_W, BUTTON_H),
                )
                .with_button(Id::Friends, Button::icon("", |a| a.friends_icon, ICON_SIZE))
                .with_button(
                    Id::Language,
                    Button::icon("", |a| a.language_icon, ICON_SIZE),
                )
                .with_button(
                    Id::Accessibility,
                    Button::icon("", |a| a.accessibility_icon, ICON_SIZE),
                )
                .with_button(
                    Id::Options,
                    Button::sized("menu.options", HALF_BUTTON_W, BUTTON_H),
                )
                .with_button(
                    Id::Quit,
                    Button::sized("menu.quit", HALF_BUTTON_W, BUTTON_H),
                ),
            mouse: (0.0, 0.0),
            splash_text: splash::pick_splash(splashes, None),
            splash_start: Instant::now(),
            version: version_text(ctx.lang.get("melon.version_suffix")),
        }
    }
}

impl Screen for TitleScreen {
    /// `TitleScreen.extractBackground`: the panorama as it is.
    fn blurs_background(&self) -> bool {
        false
    }

    fn is_world_exit(&self, _integrated: bool) -> bool {
        true
    }

    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let center = width / 2.0;
        let left = center - WIDE_BUTTON_W / 2.0;
        let mut top = quarter(height) + 48.0;
        self.widgets.column(
            &[Id::Singleplayer, Id::Multiplayer, Id::Realms],
            left,
            top,
            SPACING,
        );
        top += SPACING * 3.0;

        let icons_w = ICON_SIZE * 3.0 + ICON_GAP * 2.0;
        self.widgets.row(
            &[Id::Friends, Id::Language, Id::Accessibility],
            center - icons_w / 2.0,
            top,
            ICON_GAP,
        );
        top += SPACING;

        self.widgets.set_position(Id::Options, left, top);
        self.widgets.set_position(Id::Quit, center + 2.0, top);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.widgets.hover(x, y);
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        let Some(id) = self.widgets.click(x, y) else {
            return ScreenAction::None;
        };
        ctx.click();
        press(id, ctx)
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => {
                ctx.click();
                press(id, ctx)
            }
            _ => ScreenAction::None,
        }
    }

    /// Menu contents only.
    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        let (width, height) = (ui.width, ui.height);
        let assets = ui.assets;
        let logo_x = (width / 2.0 - 128.0).round();
        ui.draw_list.push_rect(
            assets.logo,
            [logo_x, 30.0, 256.0, 44.0],
            [0.0, 0.0, 1.0, 44.0 / 64.0],
            [1.0; 4],
        );
        let edition_x = (width / 2.0 - 64.0).round();
        ui.draw_list.push_rect(
            assets.edition,
            [edition_x, 67.0, 128.0, 14.0],
            [0.0, 0.0, 1.0, 14.0 / 16.0],
            [1.0; 4],
        );

        if !self.splash_text.is_empty() {
            let text_width = ui.text_width(&self.splash_text);
            let elapsed_ms = self.splash_start.elapsed().as_millis();
            let transform = splash::transform(width, text_width, elapsed_ms);
            assets.font.push_text_transformed(
                ui.draw_list,
                &assets.font_textures,
                &self.splash_text,
                &transform,
                [-text_width / 2.0, -8.0],
                [1.0, 1.0, 0.0, 1.0],
            );
        }

        self.widgets.draw(ui);
        let realms = self.widgets.button(Id::Realms);
        let icon_right = realms.x + realms.w - 3.0;
        let base_y = realms.y + 2.0;
        assets
            .realms_trial_icon
            .draw(ui.draw_list, icon_right - 10.0, base_y + 4.0, 8.0, 8.0);

        ui.text(&self.version, 2.0, height - 10.0, [1.0; 4]);
        let credit = ui.tr("melon.credit");
        let credit_w = ui.text_width(credit);
        ui.text(
            credit,
            width - credit_w - 2.0,
            height - 10.0,
            [0.7, 0.7, 0.7, 1.0],
        );

        for (id, key) in [
            (Id::Language, "options.language.tooltip"),
            (Id::Accessibility, "options.accessibility.tooltip"),
        ] {
            let button = self.widgets.button(id);
            if button.hovered {
                let text = ui.tr(key).to_owned();
                ui.widget_tooltip(&text, self.mouse, Widget::rect(button));
            }
        }
    }

    fn refresh_lang(&mut self, lang: &melon_assets::i18n::Lang) {
        self.version = version_text(lang.get("melon.version_suffix"));
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }
}

/// What each button opens.
fn press(id: Id, ctx: &ScreenContext) -> ScreenAction {
    match id {
        Id::Singleplayer if singleplayer::list_worlds(ctx.game_dir).is_empty() => {
            ScreenAction::open(CreateWorldScreen::new_create(ctx))
        }
        Id::Singleplayer => ScreenAction::open(SingleplayerScreen::new(ctx)),
        Id::Multiplayer => ScreenAction::open(MultiplayerScreen::new(ctx)),
        Id::Realms => ScreenAction::open(placeholder_screen("menu.online")),
        Id::Options => ScreenAction::open(OptionsScreen::new(ctx)),
        Id::Quit => ScreenAction::Quit,
        Id::Friends => ScreenAction::open(placeholder_screen("gui.friends.open")),
        Id::Language => ScreenAction::open(LanguageScreen::new(ctx)),
        Id::Accessibility => ScreenAction::open(placeholder_screen("options.accessibility.title")),
    }
}
