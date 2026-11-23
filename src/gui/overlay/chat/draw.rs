//! `ChatComponent.extractRenderState` and its hit tests.

use super::{BOTTOM_MARGIN, Chat, Line, Tag};
use crate::gui::text::font::{Font, Glyphs};
use crate::gui::text::{self, StyledText, TextPaint, TextStyle, colors};
use crate::i18n::Lang;
use crate::renderer::quad::DrawList;

impl Chat {
    /// The lines on screen, newest first, with their alpha.
    fn visible_lines(&self, focused: bool) -> Vec<(usize, &Line, f32)> {
        let per_page = self.lines_per_page(focused);
        let shown = self.lines.len().saturating_sub(self.scroll).min(per_page);
        (0..shown)
            .filter_map(|i| {
                let line = &self.lines[i + self.scroll];
                let alpha = if focused {
                    1.0
                } else {
                    let age = self.ticks.wrapping_sub(line.added) as f64 / 200.0;
                    let t = ((1.0 - age) * 10.0).clamp(0.0, 1.0);
                    (t * t) as f32
                };
                (alpha > 1.0e-5).then_some((i, line, alpha))
            })
            .collect()
    }

    /// `ChatComponent.extractRenderState`.
    pub fn draw(
        &self,
        draw_list: &mut DrawList,
        glyphs: Glyphs<'_>,
        lang: &Lang,
        screen_height: f32,
        focused: bool,
    ) {
        let white = glyphs.white;
        let scale = self.scale();
        if scale <= 0.0 {
            return;
        }
        let max_width = (self.width() / scale).ceil();
        let chat_bottom = ((screen_height - BOTTOM_MARGIN) / scale).floor();
        let text_opacity = self.settings.opacity as f32 * 0.9 + 0.1;
        let background_opacity = self.settings.background_opacity as f32;
        let spacing = self.settings.line_spacing;
        let entry_height = self.line_height();
        let entry_bottom_to_text = (8.0 * (spacing + 1.0) - 4.0 * spacing).round() as f32;
        let to_screen = |x: f32, y: f32| ((x + 4.0) * scale, y * scale);
        let fill =
            |draw_list: &mut DrawList, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4]| {
                let (sx0, sy0) = to_screen(x0, y0);
                let (sx1, sy1) = to_screen(x1, y1);
                draw_list.push_fill(white, sx0, sy0, sx1 - sx0, sy1 - sy0, color);
            };
        let lines = self.visible_lines(focused);
        for &(i, _, alpha) in &lines {
            let bottom = chat_bottom - i as f32 * entry_height;
            fill(
                draw_list,
                -4.0,
                bottom - entry_height,
                max_width + 8.0,
                bottom,
                [0.0, 0.0, 0.0, alpha * background_opacity],
            );
        }
        let queue = self.queue_size();
        if queue > 0 {
            fill(
                draw_list,
                -2.0,
                chat_bottom,
                max_width + 4.0,
                chat_bottom + 9.0,
                [0.0, 0.0, 0.0, background_opacity],
            );
        }
        for &(i, line, alpha) in &lines {
            let bottom = chat_bottom - i as f32 * entry_height;
            let top = bottom - entry_height;
            let text_top = bottom - entry_bottom_to_text;
            if let Some(tag) = &line.tag {
                let [r, g, b] = tag.indicator;
                fill(
                    draw_list,
                    -4.0,
                    top,
                    -2.0,
                    bottom,
                    [r, g, b, alpha * text_opacity],
                );
            }
            self.draw_line(draw_list, glyphs, &line.content, text_top, alpha * text_opacity);
        }
        if queue > 0 {
            let text = StyledText::plain(&lang.format("chat.queue", &[&queue.to_string()]));
            self.draw_line(draw_list, glyphs, &text, chat_bottom + 1.0, 0.5 * text_opacity);
        }
        if focused && !self.lines.is_empty() {
            let total = self.lines.len() as f32;
            let chat_height = lines.len() as f32 * entry_height;
            let virtual_height = total * entry_height;
            if virtual_height != chat_height {
                let y = self.scroll as f32 * chat_height / total - chat_bottom;
                let height = chat_height * chat_height / virtual_height;
                let alpha = if y > 0.0 { 170.0 } else { 96.0 } / 255.0;
                let color = if self.new_since_scroll {
                    0xCC_3333
                } else {
                    0x33_33AA
                };
                let rgb = [(color >> 16) & 255, (color >> 8) & 255, color & 255]
                    .map(|c| c as f32 / 255.0);
                let x = max_width + 4.0;
                fill(
                    draw_list,
                    x,
                    -y - height,
                    x + 2.0,
                    -y,
                    [rgb[0], rgb[1], rgb[2], alpha],
                );
                fill(
                    draw_list,
                    x + 1.0,
                    -y - height,
                    x + 2.0,
                    -y,
                    [0.8, 0.8, 0.8, alpha],
                );
            }
        }
    }

    fn draw_line(
        &self,
        draw_list: &mut DrawList,
        glyphs: Glyphs<'_>,
        content: &StyledText,
        text_top: f32,
        alpha: f32,
    ) {
        let Glyphs { font, pages, white } = glyphs;
        let scale = self.scale();
        let paint = TextPaint {
            font,
            pages,
            white,
            color: colors::WHITE,
            alpha,
            shadow: true,
            obfuscation_seed: u64::from(self.ticks),
        };
        if (scale - 1.0).abs() < f32::EPSILON {
            text::draw(draw_list, &paint, content, 4.0, text_top);
        } else {
            let start = draw_list.vertices_len();
            text::draw(draw_list, &paint, content, 4.0, text_top);
            draw_list.scale_since(start, scale);
        }
    }

    /// The run under the mouse at `(x, y)` on a `screen_height` screen, if any.
    pub fn style_at(&self, font: &Font, screen_height: f32, x: f32, y: f32) -> Option<TextStyle> {
        let scale = self.scale();
        if scale <= 0.0 {
            return None;
        }
        let local_x = x / scale - 4.0 - 4.0;
        let local_y = y / scale;
        let chat_bottom = ((screen_height - BOTTOM_MARGIN) / scale).floor();
        let spacing = self.settings.line_spacing;
        let entry_height = self.line_height();
        let entry_bottom_to_text = (8.0 * (spacing + 1.0) - 4.0 * spacing).round() as f32;
        for (i, line, _) in self.visible_lines(true) {
            let bottom = chat_bottom - i as f32 * entry_height;
            let text_top = bottom - entry_bottom_to_text;
            if local_y >= text_top && local_y < text_top + 9.0 {
                return text::style_at(font, &line.content, local_x).cloned();
            }
        }
        None
    }

    /// The tag under the mouse, for its tooltip.
    pub fn tag_at(&self, screen_height: f32, x: f32, y: f32) -> Option<&Tag> {
        let scale = self.scale();
        if scale <= 0.0 {
            return None;
        }
        let local_x = x / scale - 4.0;
        let local_y = y / scale;
        let chat_bottom = ((screen_height - BOTTOM_MARGIN) / scale).floor();
        let entry_height = self.line_height();
        if !(-4.0..-2.0).contains(&local_x) {
            return None;
        }
        self.visible_lines(true)
            .into_iter()
            .find_map(|(i, line, _)| {
                let bottom = chat_bottom - i as f32 * entry_height;
                (local_y >= bottom - entry_height && local_y < bottom)
                    .then_some(line.tag.as_ref())
                    .flatten()
            })
    }

    /// Whether the mouse is over the queue line, which shows the next delayed message.
    pub fn is_over_queue(&self, screen_height: f32, x: f32, y: f32) -> bool {
        let scale = self.scale();
        if self.delayed.is_empty() || scale <= 0.0 {
            return false;
        }
        let chat_bottom = ((screen_height - BOTTOM_MARGIN) / scale).floor();
        let (lx, ly) = (x / scale - 4.0, y / scale);
        lx >= -2.0
            && lx < (self.width() / scale).ceil() + 4.0
            && ly >= chat_bottom
            && ly < chat_bottom + 9.0
    }
}
