//! Shared layout measurements.

use crate::gui::text::font::LINE_HEIGHT;
use crate::gui::widgets::Widget;

/// `Button.DEFAULT_WIDTH`/`DEFAULT_HEIGHT`/`DEFAULT_SPACING`.
pub const BUTTON_W: f32 = 150.0;
pub const BUTTON_H: f32 = 20.0;
pub const SPACING: f32 = 8.0;
/// The 200 wide buttons of the older centred menus (`width / 2 - 100`).
pub const WIDE_BUTTON_W: f32 = 200.0;

/// `HeaderAndFooterLayout`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderFooter {
    pub header: f32,
    pub footer: f32,
}

impl HeaderFooter {
    /// `HeaderAndFooterLayout.DEFAULT_HEADER_AND_FOOTER_HEIGHT`.
    pub const DEFAULT: HeaderFooter = HeaderFooter::new(33.0, 33.0);

    pub const fn new(header: f32, footer: f32) -> Self {
        Self { header, footer }
    }

    /// Where the contents start.
    pub fn top(&self) -> f32 {
        self.header
    }

    /// Where the contents end and the footer starts.
    pub fn bottom(&self, height: f32) -> f32 {
        height - self.footer
    }

    /// A title alone in the header, centred in it.
    pub fn title_y(&self) -> f32 {
        ((self.header - LINE_HEIGHT) / 2.0).floor()
    }

    /// The top of something `content_h` tall centred in the footer.
    pub fn footer_y(&self, height: f32, content_h: f32) -> f32 {
        self.bottom(height) + ((self.footer - content_h) / 2.0).floor()
    }

    /// A single row of buttons in the footer.
    pub fn footer_button_y(&self, height: f32) -> f32 {
        self.footer_y(height, BUTTON_H)
    }
}

/// The left edge of the older centred menus' 200 wide column.
pub fn wide_column_left(width: f32) -> f32 {
    (width / 2.0).floor() - WIDE_BUTTON_W / 2.0
}

/// The older menus' anchor a quarter down the screen.
pub fn quarter(height: f32) -> f32 {
    (height / 4.0).floor()
}

/// The left edge of a row `row_w` wide centred on the screen.
pub fn centered_left(width: f32, row_w: f32) -> f32 {
    ((width - row_w) / 2.0).floor()
}

/// Places widgets left to right from `x`, each `gap` after the last.
pub fn row(widgets: &mut [&mut dyn Widget], x: f32, y: f32, gap: f32) {
    let mut x = x;
    for widget in widgets {
        widget.set_position(x, y);
        x += widget.rect().w + gap;
    }
}

/// The width of a row of widgets `gap` apart.
pub fn row_width(widgets: &[&mut dyn Widget], gap: f32) -> f32 {
    let widths: f32 = widgets.iter().map(|w| w.rect().w).sum();
    widths + gap * widgets.len().saturating_sub(1) as f32
}

/// Places widgets in a row centred on the screen.
pub fn centered_row(widgets: &mut [&mut dyn Widget], width: f32, y: f32, gap: f32) {
    let left = centered_left(width, row_width(widgets, gap));
    row(widgets, left, y, gap);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::widgets::button::Button;

    #[test]
    fn rows_are_centred_and_spaced() {
        let mut a = Button::sized("a", 150.0, 20.0);
        let mut b = Button::sized("b", 150.0, 20.0);
        centered_row(&mut [&mut a, &mut b], 400.0, 10.0, 8.0);
        assert_eq!((a.x, b.x), (46.0, 204.0));
        assert_eq!(HeaderFooter::DEFAULT.footer_button_y(240.0), 213.0);
    }
}
