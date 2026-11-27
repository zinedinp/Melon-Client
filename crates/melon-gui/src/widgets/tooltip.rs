//! Tooltips: `TooltipRenderUtil`'s box (the `tooltip/*` sprites) around the text.

use crate::gui::assets::TooltipSprites;
use crate::gui::text::{self, StyledText, TextPaint};
use crate::gui::widgets::Rect;
use crate::math::argb;
use crate::renderer::TextureId;
use crate::renderer::quad::DrawList;

/// `Tooltip.MAX_WIDTH`: option tooltips wrap at this width.
pub const MAX_WIDTH: f32 = 170.0;

/// The text block's width and height.
pub fn size(paint: &TextPaint, lines: &[StyledText]) -> (f32, f32) {
    let width = lines
        .iter()
        .map(|line| text::width(paint.font, line))
        .fold(0.0, f32::max);
    let gap = if lines.len() > 1 { 2.0 } else { 0.0 };
    (width, lines.len() as f32 * 10.0 - 2.0 + gap)
}

/// `DefaultTooltipPositioner`.
pub fn beside_mouse(mouse: (f32, f32), text: (f32, f32), screen: (f32, f32)) -> (f32, f32) {
    let (width, height) = screen;
    let mut x = mouse.0 + 12.0;
    if x + text.0 + 4.0 > width {
        x = (mouse.0 - 16.0 - text.0).max(4.0);
    }
    let mut y = mouse.1 - 12.0;
    if y + text.1 + 3.0 > height {
        y = height - text.1 - 3.0;
    }
    (x, y.max(4.0))
}

/// `MenuTooltipPositioner`: right of the mouse, below the hovered `widget`
pub fn by_widget(
    mouse: (f32, f32),
    widget: Rect,
    text: (f32, f32),
    screen: (f32, f32),
) -> (f32, f32) {
    let offset = |mouse_y: f32, widget_y: f32| {
        let distance = (mouse_y - widget_y).abs().min(widget.h);
        let t = distance / widget.h;
        ((widget.h - 3.0) + t * (5.0 - (widget.h - 3.0))).round()
    };
    let (mx, my) = (mouse.0.floor(), mouse.1.floor());
    let mut x = mx + 12.0;
    if x + text.0 > screen.0 - 5.0 {
        x = (mx - 12.0 - text.0).max(9.0);
    }
    let mut y = my + 3.0;
    let padded = text.1 + 6.0;
    let bottom = widget.y + widget.h;
    let lowest = bottom + 3.0 + offset(0.0, 0.0);
    if lowest + padded <= screen.1 - 5.0 {
        y += offset(y, widget.y);
    } else {
        y -= padded + offset(y, bottom);
    }
    (x, y)
}

/// `TooltipRenderUtil.extractTooltipBackground` around the text at `text`.
pub fn draw(
    draw_list: &mut DrawList,
    sprites: Option<&TooltipSprites>,
    paint: &TextPaint,
    lines: &[StyledText],
    (x, y): (f32, f32),
) {
    if lines.is_empty() {
        return;
    }
    let (text_width, text_height) = size(paint, lines);
    let gap = if lines.len() > 1 { 2.0 } else { 0.0 };
    match sprites {
        Some(sprites) => {
            let (left, top) = (x - 3.0 - 9.0, y - 3.0 - 9.0);
            let (w, h) = (text_width + 6.0 + 18.0, text_height + 6.0 + 18.0);
            sprites.background.draw(draw_list, left, top, w, h);
            sprites.frame.draw(draw_list, left, top, w, h);
        }
        None => draw_plain_box(draw_list, paint.white, [x, y, text_width, text_height]),
    }
    let mut line_y = y;
    for (i, line) in lines.iter().enumerate() {
        text::draw(draw_list, paint, line, x, line_y);
        line_y += 10.0 + if i == 0 { gap } else { 0.0 };
    }
}

/// The box the `tooltip/*` sprites draw, by hand, for an `assets/` folder without them.
fn draw_plain_box(draw_list: &mut DrawList, white: TextureId, text: [f32; 4]) {
    let background = argb(0xF010_0010);
    let frame_top = argb(0x5050_00FF);
    let frame_bottom = argb(0x5028_007F);
    let (left, top) = (text[0] - 3.0, text[1] - 3.0);
    let (w, h) = (text[2] + 6.0, text[3] + 6.0);
    draw_list.push_fill(white, left, top - 1.0, w, 1.0, background);
    draw_list.push_fill(white, left, top + h, w, 1.0, background);
    draw_list.push_fill(white, left, top, w, h, background);
    draw_list.push_fill(white, left - 1.0, top, 1.0, h, background);
    draw_list.push_fill(white, left + w, top, 1.0, h, background);
    let side = [frame_top, frame_bottom];
    draw_list.push_vgradient(white, [left, top + 1.0, 1.0, h - 2.0], side[0], side[1]);
    draw_list.push_vgradient(
        white,
        [left + w - 1.0, top + 1.0, 1.0, h - 2.0],
        side[0],
        side[1],
    );
    draw_list.push_fill(white, left, top, w, 1.0, frame_top);
    draw_list.push_fill(white, left, top + h - 1.0, w, 1.0, frame_bottom);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_tooltips_sit_below_the_widget_or_above_it() {
        let button = Rect::new(100.0, 50.0, 150.0, 20.0);
        let (x, y) = by_widget((120.0, 50.0), button, (100.0, 28.0), (400.0, 300.0));
        assert_eq!((x, y), (132.0, 68.0));
        let low = Rect::new(100.0, 270.0, 150.0, 20.0);
        let (_, y) = by_widget((120.0, 280.0), low, (100.0, 28.0), (400.0, 300.0));
        assert!(y + 28.0 + 3.0 <= 270.0 + 3.0);
        let (x, _) = by_widget((390.0, 50.0), button, (100.0, 28.0), (400.0, 300.0));
        assert_eq!(x, 390.0 - 12.0 - 100.0);
    }
}
