//! Vanilla `Hud.displayScoreboardSidebar`.

use crate::gui::hud::item_slot::ItemPainter;
use crate::gui::text::{self, StyledText, colors};
use crate::math::argb;
use crate::renderer::quad::DrawList;

/// `Options.getBackgroundColor(0.3F)` and `(0.4F)`.
const ROWS_BACKGROUND: u32 = 0x4C00_0000;
const TITLE_BACKGROUND: u32 = 0x6600_0000;
const LINE: i32 = 9;

/// One row: the holder's name and its score, both already styled.
pub struct SidebarRow {
    pub name: StyledText,
    pub score: StyledText,
}

/// Draws `title` and `rows` on a GUI `width` x `height` pixels.
pub fn draw(
    draw_list: &mut DrawList,
    painter: ItemPainter,
    title: &StyledText,
    rows: &[SidebarRow],
    (width, height): (f32, f32),
) {
    let font = painter.font;
    let measure = |text: &StyledText| text::width(font, text) as i32;
    let title_width = measure(title);
    let spacer = measure(&StyledText::plain(": "));
    let widths: Vec<(i32, i32)> = rows
        .iter()
        .map(|row| (measure(&row.name), measure(&row.score)))
        .collect();
    let biggest = widths
        .iter()
        .map(|&(name, score)| name + if score > 0 { spacer + score } else { 0 })
        .fold(title_width, i32::max);

    let (gui_width, gui_height) = (width as i32, height as i32);
    let count = rows.len() as i32;
    let bottom = gui_height / 2 + count * LINE / 3;
    let left = gui_width - biggest - 3;
    let right = gui_width - 3 + 2;
    let header = bottom - count * LINE;
    let fill = |draw_list: &mut DrawList, x0: i32, y0: i32, x1: i32, y1: i32, color: u32| {
        draw_list.push_fill(
            painter.white,
            x0 as f32,
            y0 as f32,
            (x1 - x0) as f32,
            (y1 - y0) as f32,
            argb(color),
        );
    };
    fill(
        draw_list,
        left - 2,
        header - LINE - 1,
        right,
        header - 1,
        TITLE_BACKGROUND,
    );
    fill(
        draw_list,
        left - 2,
        header - 1,
        right,
        bottom,
        ROWS_BACKGROUND,
    );

    let mut paint = painter.paint(colors::WHITE);
    paint.shadow = false;
    let title_x = left + biggest / 2 - title_width / 2;
    text::draw(
        draw_list,
        &paint,
        title,
        title_x as f32,
        (header - LINE) as f32,
    );
    for (i, (row, &(_, score_width))) in rows.iter().zip(&widths).enumerate() {
        let y = (bottom - (count - i as i32) * LINE) as f32;
        text::draw(draw_list, &paint, &row.name, left as f32, y);
        text::draw(
            draw_list,
            &paint,
            &row.score,
            (right - score_width) as f32,
            y,
        );
    }
}
