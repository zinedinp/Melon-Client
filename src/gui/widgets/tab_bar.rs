//! Vanilla `MenuTabBar` / `TabNavigationBar`.

use crate::gui::text::font::LINE_HEIGHT;
use crate::gui::widgets::{Rect, Ui};

/// `MenuTabBar.HEIGHT`.
pub const HEIGHT: f32 = 24.0;
/// `MenuTabBar.MAX_WIDTH` / `MenuTabBar.MARGIN`.
const MAX_W: f32 = 400.0;
const MARGIN: f32 = 28.0;
/// `MenuTabButton.SELECTED_OFFSET`: unselected tab labels sit 3px lower.
const SELECTED_OFFSET: f32 = 3.0;
const UNDERLINE_MARGIN_X: f32 = 4.0;

/// `Mth.roundToward`: round up to the next multiple.
fn round_toward(value: f32, multiple: f32) -> f32 {
    (value / multiple).ceil() * multiple
}

pub struct TabBar {
    /// Each tab's title key.
    titles: &'static [&'static str],
    selected: usize,
    /// `MenuTabBar.arrangeElements`: left edge and shared width of the tabs.
    x: f32,
    y: f32,
    tab_w: f32,
    hovered: Option<usize>,
}

impl TabBar {
    pub fn new(titles: &'static [&'static str]) -> Self {
        Self {
            titles,
            selected: 0,
            x: 0.0,
            y: 0.0,
            tab_w: 0.0,
            hovered: None,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// `MenuTabBar.arrangeElements`.
    pub fn resize(&mut self, width: f32) {
        self.resize_at(0.0, 0.0, width);
    }

    /// A row of tabs in a column: `y` is the top, `x` the column's left edge.
    pub fn resize_at(&mut self, x: f32, y: f32, width: f32) {
        self.y = y;
        let tabs_w = (width.min(MAX_W) - MARGIN).max(0.0).trunc();
        let n = self.titles.len().max(1) as f32;
        self.tab_w = round_toward((tabs_w / n).trunc().max(2.0), 2.0);
        self.x = round_toward((x + (width - tabs_w) / 2.0).trunc(), 2.0);
    }

    /// The tab under `(x, y)`, if any.
    fn tab_at(&self, x: f32, y: f32) -> Option<usize> {
        if !(self.y..self.y + HEIGHT).contains(&y) || self.tab_w <= 0.0 {
            return None;
        }
        let i = ((x - self.x) / self.tab_w).floor();
        (i >= 0.0 && i < self.titles.len() as f32).then_some(i as usize)
    }

    pub fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.hovered = self.tab_at(x, y);
    }

    pub fn hovered(&self) -> bool {
        self.hovered.is_some()
    }

    /// A click: `Some(tab)` when it hit one, whether or not it changed.
    pub fn on_click(&mut self, x: f32, y: f32) -> Option<usize> {
        self.tab_at(x, y)
    }

    pub fn select(&mut self, tab: usize) {
        self.selected = tab.min(self.titles.len().saturating_sub(1));
    }

    /// `MenuTabBar.extractWidgetRenderState`.
    pub fn draw(&self, ui: &mut Ui) {
        let separator_y = self.y + HEIGHT - 2.0;
        let right = self.x + self.tab_w * self.titles.len() as f32;
        if !ui.assets.launcher {
            let separator = ui.assets.header_separator;
            ui.draw_list.push_rect_tiled_x(
                separator,
                0.0,
                separator_y,
                self.x,
                2.0,
                32.0,
                [1.0; 4],
            );
            ui.draw_list.push_rect_tiled_x(
                separator,
                right,
                separator_y,
                (ui.width - right).max(0.0),
                2.0,
                32.0,
                [1.0; 4],
            );
        }
        for index in 0..self.titles.len() {
            self.draw_tab(ui, index);
        }
    }

    /// One `MenuTabBar.MenuTabButton`.
    fn draw_tab(&self, ui: &mut Ui, index: usize) {
        let x = self.x + index as f32 * self.tab_w;
        let w = self.tab_w;
        let selected = index == self.selected;
        let hovered = self.hovered == Some(index);
        let title = ui.tr(self.titles[index]);
        let assets = ui.assets;
        if assets.launcher {
            let color = match (selected, hovered) {
                (true, _) => [0.22, 0.32, 0.18, 1.0],
                (false, true) => [0.28, 0.28, 0.30, 1.0],
                _ => [0.16, 0.16, 0.18, 1.0],
            };
            ui.fill(Rect::new(x, self.y, w, HEIGHT), color);
        } else {
            assets
                .tab_textures
                .get(selected, hovered)
                .draw(ui.draw_list, x, self.y, w, HEIGHT);
        }

        if selected {
            if !assets.launcher {
                crate::gui::background::push_menu_overlay_rect(
                    ui.draw_list,
                    assets.white,
                    x + 2.0,
                    self.y + 2.0,
                    w - 4.0,
                    HEIGHT - 2.0,
                );
            }
            let underline_w = ui.text_width(title).min(w - UNDERLINE_MARGIN_X);
            ui.fill(
                Rect::new(
                    x + ((w - underline_w) / 2.0).trunc(),
                    self.y + HEIGHT - 2.0,
                    underline_w,
                    1.0,
                ),
                [1.0; 4],
            );
        }

        let top = if selected { 0.0 } else { SELECTED_OFFSET };
        let text_y = self.y + ((top + HEIGHT - LINE_HEIGHT) / 2.0).trunc() + 1.0;
        let text_w = ui.text_width(title);
        ui.text(
            title,
            (x + w / 2.0 - (text_w / 2.0).trunc()).round(),
            text_y,
            [1.0; 4],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_split_the_bar_evenly() {
        let mut bar = TabBar::new(&["a", "b", "c"]);
        bar.resize(480.0);
        assert_eq!((bar.x, bar.tab_w), (54.0, 124.0));
        assert_eq!(bar.on_click(54.0 + 130.0, 5.0), Some(1));
        assert_eq!(bar.on_click(54.0 + 130.0, 30.0), None);
    }
}
