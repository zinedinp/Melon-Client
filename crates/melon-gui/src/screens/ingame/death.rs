//! Vanilla `DeathScreen`.

use std::time::{Duration, Instant};

use crate::gui::effects;
use crate::gui::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::gui::widgets::Ui;
use crate::gui::widgets::button::Button;
use crate::gui::widgets::layout::{BUTTON_H, WIDE_BUTTON_W, quarter, wide_column_left};
use crate::gui::widgets::set::{KeyResult, WidgetSet};
use crate::math::argb;
use crate::renderer::quad::Transform2D;

const BUTTON_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Respawn,
    TitleScreen,
}

pub struct DeathScreen {
    /// A hardcore death offers spectating instead of respawning.
    hardcore: bool,
    widgets: WidgetSet<Id>,
    opened: Instant,
    /// Respawn was clicked; vanilla greys the button out until the server answers.
    respawning: bool,
}

impl DeathScreen {
    pub fn new(hardcore: bool) -> Self {
        let respawn = if hardcore {
            "deathScreen.spectate"
        } else {
            "deathScreen.respawn"
        };
        let mut screen = Self {
            hardcore,
            widgets: WidgetSet::new()
                .with_button(Id::Respawn, Button::sized(respawn, WIDE_BUTTON_W, BUTTON_H))
                .with_button(
                    Id::TitleScreen,
                    Button::sized("deathScreen.titleScreen", WIDE_BUTTON_W, BUTTON_H),
                ),
            opened: Instant::now(),
            respawning: false,
        };
        screen.tick();
        screen
    }
}

impl DeathScreen {
    fn press(&mut self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        match id {
            Id::Respawn => {
                self.respawning = true;
                self.tick();
                ScreenAction::Respawn
            }
            Id::TitleScreen => ScreenAction::LeaveWorld,
        }
    }
}

impl Screen for DeathScreen {
    fn is_pause_screen(&self) -> bool {
        false
    }

    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let left = wide_column_left(width);
        let anchor = quarter(height);
        self.widgets
            .column(&[Id::Respawn, Id::TitleScreen], left, anchor + 72.0, 24.0);
    }

    fn tick(&mut self) {
        let ready = self.opened.elapsed() >= BUTTON_DELAY;
        self.widgets
            .set_enabled(Id::Respawn, ready && !self.respawning);
        self.widgets.set_enabled(Id::TitleScreen, ready);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.click(x, y) {
            Some(id) => self.press(id, ctx),
            None => ScreenAction::None,
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => self.press(id, ctx),
            _ => ScreenAction::None,
        }
    }

    fn closes_on_escape(&self) -> bool {
        false
    }

    /// `extractDeathBackground`: a red gradient over the sharp world.
    fn blurs_background(&self) -> bool {
        false
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        let (width, height) = (ui.width, ui.height);
        let assets = ui.assets;
        effects::death_veil(ui.draw_list, assets.white, (width, height));
        let middle = (width / 2.0).floor();

        let title = ui.tr(if self.hardcore {
            "deathScreen.title.hardcore"
        } else {
            "deathScreen.title"
        });
        let title_width = ui.text_width(title);
        let scale = Transform2D {
            translate: [0.0, 0.0],
            sin_cos: (0.0, 1.0),
            scale: 2.0,
        };
        assets.font.push_text_transformed(
            ui.draw_list,
            &assets.font_textures,
            title,
            &scale,
            [((middle / 2.0) - title_width / 2.0).floor(), 30.0],
            [1.0; 4],
        );

        let score_label = ui.tr("deathScreen.score.value");
        let (before, after) = score_label.split_once("%s").unwrap_or((score_label, ""));
        let score = "0";
        let total = ui.text_width(before) + ui.text_width(score) + ui.text_width(after);
        let mut x = (middle - total / 2.0).floor();
        x += ui.text(before, x, 100.0, [1.0; 4]);
        x += ui.text(score, x, 100.0, argb(0xFFFF_FF55));
        ui.text(after, x, 100.0, [1.0; 4]);

        self.widgets.draw(ui);
    }
}
