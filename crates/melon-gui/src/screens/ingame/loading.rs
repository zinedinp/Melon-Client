//! Vanilla `LevelLoadingScreen`.

use crate::background;
use crate::effects::AtlasSprite;
use melon_core::math::argb;
use crate::screen::{Screen, ScreenAction, ScreenContext};
use crate::widgets::Ui;

/// `LevelLoadingScreen.Reason`: what the loading screen paints behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadingReason {
    NetherPortal,
    EndPortal,
    Other,
}

pub struct LevelLoadingScreen {
    reason: LoadingReason,
    /// The nether portal's sprite, painted over the whole screen for a portal journey.
    portal: Option<AtlasSprite>,
}

impl LevelLoadingScreen {
    pub fn new(reason: LoadingReason, portal: Option<AtlasSprite>) -> Self {
        Self { reason, portal }
    }
}

impl Screen for LevelLoadingScreen {
    fn resize(&mut self, _ctx: &ScreenContext) {}

    fn on_mouse_move(&mut self, _x: f32, _y: f32) {}

    fn on_click(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) -> ScreenAction {
        ScreenAction::None
    }

    fn closes_on_escape(&self) -> bool {
        false
    }

    fn blurs_background(&self) -> bool {
        false
    }

    fn is_level_loading(&self) -> bool {
        true
    }

    /// `extractBackground`.
    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        let (width, height) = (ui.width, ui.height);
        let white = ui.assets.white;
        match (self.reason, self.portal) {
            (LoadingReason::NetherPortal, Some(sprite)) => {
                ui.draw_list
                    .push_rect(sprite.atlas, [0.0, 0.0, width, height], sprite.uv, [1.0; 4])
            }
            (LoadingReason::EndPortal, _) => {
                ui.draw_list
                    .push_fill(white, 0.0, 0.0, width, height, argb(0xFF08_0A0C));
            }
            _ => {
                background::push_gradient(ui.draw_list, white, width, height);
                background::push_menu_overlay(ui.draw_list, white, width, height);
            }
        }
        let text = ui.tr("multiplayer.downloadingTerrain");
        let x = (width / 2.0 - ui.text_width(text) / 2.0).floor();
        ui.text(text, x, (height / 2.0 - 50.0).floor(), [1.0; 4]);
    }
}
