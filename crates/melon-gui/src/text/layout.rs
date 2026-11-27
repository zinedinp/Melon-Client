//! Drawing and measuring [`StyledText`]: the way `Font.PreparedTextBuilder`

use super::{StyledText, TextStyle};
use crate::renderer::TextureId;
use crate::renderer::quad::DrawList;
use crate::text::font::{self, BASELINE_ASCENT, Font, GlyphKind, shadow_color};

/// How styled text is drawn: where, how opaque, with or without shadow.
#[derive(Clone, Copy)]
pub struct TextPaint<'a> {
    pub font: &'a Font,
    pub pages: &'a [TextureId],
    /// A white pixel, for underline and strikethrough.
    pub white: TextureId,
    /// The colour of runs without their own.
    pub color: [f32; 3],
    pub alpha: f32,
    pub shadow: bool,
    /// Seed for obfuscated glyphs; change it to make them flicker.
    pub obfuscation_seed: u64,
}

impl<'a> TextPaint<'a> {
    /// White, opaque and shadowed: `GuiGraphics.drawString`'s defaults.
    pub fn new(font: &'a Font, pages: &'a [TextureId], white: TextureId) -> Self {
        Self {
            font,
            pages,
            white,
            color: [1.0; 3],
            alpha: 1.0,
            shadow: true,
            obfuscation_seed: 0,
        }
    }
}

/// `Font.width` of styled text: bold glyphs are wider by their bold offset.
pub fn width(font: &Font, text: &StyledText) -> f32 {
    text.spans
        .iter()
        .map(|span| {
            let face = span.style.font.as_deref();
            span.text
                .chars()
                .map(|c| font.styled_advance_in(face, c, span.style.bold))
                .sum::<f32>()
        })
        .sum()
}

/// Draws `text` with its top-left at `(x, y)`; returns its width.
pub fn draw(draw_list: &mut DrawList, paint: &TextPaint, text: &StyledText, x: f32, y: f32) -> f32 {
    let program = draw_list.set_program(crate::renderer::quad::Program::Text);
    let width = draw_glyphs(draw_list, paint, text, x, y);
    draw_list.set_program(program);
    width
}

fn draw_glyphs(
    draw_list: &mut DrawList,
    paint: &TextPaint,
    text: &StyledText,
    x: f32,
    y: f32,
) -> f32 {
    let mut pen = x;
    let mut seed = paint.obfuscation_seed;
    for span in &text.spans {
        let style = &span.style;
        let face = style.font.as_deref();
        let [r, g, b] = style.color.unwrap_or(paint.color);
        let color = [r, g, b, paint.alpha];
        let shadow = match style.shadow_color {
            Some([r, g, b, a]) => [r, g, b, a * paint.alpha],
            None => shadow_color(color),
        };
        let start = pen;
        for ch in span.text.chars() {
            let mut ch = ch;
            if ch == super::OBJECT_CHAR
                && let Some(info) = &style.sprite
            {
                if let Some(quads) = paint.font.object_quads(info) {
                    for quad in [Some(quads.face), quads.hat].into_iter().flatten() {
                        if paint.shadow && shadow[3] > 0.0 {
                            let (dx, dy) = (font::SHADOW_OFFSET, font::SHADOW_OFFSET);
                            draw_list.push_rect(
                                quad.texture,
                                [pen + dx, y - 1.0 + dy, 8.0, 8.0],
                                quad.uv,
                                shadow,
                            );
                        }
                        draw_list.push_rect(quad.texture, [pen, y - 1.0, 8.0, 8.0], quad.uv, color);
                    }
                }
                pen += 8.0 + if style.bold { 1.0 } else { 0.0 };
                continue;
            }
            if style.obfuscated {
                ch = paint.font.random_glyph_in(face, ch, &mut seed);
            }
            let glyph = paint.font.glyph_in(face, ch);
            let advance = paint.font.advance_in(face, ch)
                + if style.bold {
                    glyph.map_or(1.0, |g| g.bold)
                } else {
                    0.0
                };
            if let Some(glyph) = glyph
                && let GlyphKind::Bitmap {
                    page,
                    u0,
                    v0,
                    u1,
                    v1,
                    width,
                    height,
                    ascent,
                } = glyph.kind
            {
                let texture = paint.pages[page];
                let uv = [u0, v0, u1, v1];
                let top = y + (BASELINE_ASCENT - ascent as f32);
                let mut glyph_at = |dx: f32, dy: f32, color: [f32; 4]| {
                    if style.italic {
                        let up = top - y;
                        draw_list.push_rect_sheared(
                            texture,
                            [pen + dx, top + dy, width, height],
                            [1.0 - 0.25 * up, 1.0 - 0.25 * (up + height)],
                            uv,
                            color,
                        );
                    } else {
                        draw_list.push_rect(
                            texture,
                            [pen + dx, top + dy, width, height],
                            uv,
                            color,
                        );
                    }
                };
                let bold = if style.bold { 2 } else { 1 };
                if paint.shadow && shadow[3] > 0.0 {
                    for i in 0..bold {
                        glyph_at(glyph.shadow + i as f32 * glyph.bold, glyph.shadow, shadow);
                    }
                }
                for i in 0..bold {
                    glyph_at(i as f32 * glyph.bold, 0.0, color);
                }
            }
            pen += advance;
        }
        let mut line = |line_y: f32, color: [f32; 4]| {
            draw_list.push_fill(
                paint.white,
                start - 1.0,
                line_y,
                pen - start + 1.0,
                1.0,
                color,
            );
        };
        for (is_shadow, color) in [(true, shadow), (false, color)] {
            if is_shadow && (!paint.shadow || shadow[3] <= 0.0) {
                continue;
            }
            let offset = if is_shadow {
                crate::text::font::SHADOW_OFFSET
            } else {
                0.0
            };
            if style.strikethrough {
                line(y + 3.5 + offset, color);
            }
            if style.underlined {
                line(y + 8.0 + offset, color);
            }
        }
    }
    pen - x
}

/// `StringSplitter.splitLines`.
pub fn wrap(font: &Font, text: &StyledText, max_width: f32) -> Vec<StyledText> {
    let chars: Vec<(char, usize)> = text
        .spans
        .iter()
        .enumerate()
        .flat_map(|(i, span)| span.text.chars().map(move |c| (c, i)))
        .collect();
    let advance = |&(c, i): &(char, usize)| {
        let style = &text.spans[i].style;
        font.styled_advance_in(style.font.as_deref(), c, style.bold)
    };
    let mut lines = Vec::new();
    let mut start = 0;
    while start <= chars.len() {
        let mut width = 0.0;
        let mut end = start;
        let mut last_space = None;
        let mut newline = false;
        while end < chars.len() {
            let c = chars[end];
            if c.0 == '\n' {
                newline = true;
                break;
            }
            let w = advance(&c);
            if width + w > max_width && end > start {
                break;
            }
            if c.0 == ' ' {
                last_space = Some(end);
            }
            width += w;
            end += 1;
        }
        let (cut, next) = if newline || end >= chars.len() {
            (end, end + 1)
        } else if let Some(space) = last_space.filter(|&s| s > start) {
            (space, space + 1)
        } else {
            (end, end)
        };
        let mut line = StyledText::default();
        for &(c, i) in &chars[start..cut] {
            line.push(c.encode_utf8(&mut [0; 4]), &text.spans[i].style);
        }
        lines.push(line);
        if next > chars.len() {
            break;
        }
        start = next;
        if start == chars.len() && !newline {
            break;
        }
    }
    if lines.is_empty() {
        lines.push(StyledText::default());
    }
    lines
}

/// The run under `x` pixels from the start of `text` (`Font.getStyleAt`).
pub fn style_at<'a>(font: &Font, text: &'a StyledText, x: f32) -> Option<&'a TextStyle> {
    if x < 0.0 {
        return None;
    }
    let mut pen = 0.0;
    for span in &text.spans {
        let face = span.style.font.as_deref();
        for c in span.text.chars() {
            pen += font.styled_advance_in(face, c, span.style.bold);
            if x < pen {
                return Some(&span.style);
            }
        }
    }
    None
}

/// Whether a run carries anything to click or hover.
pub fn is_active(style: &TextStyle) -> bool {
    style.click.is_some() || style.hover.is_some() || style.insertion.is_some()
}
