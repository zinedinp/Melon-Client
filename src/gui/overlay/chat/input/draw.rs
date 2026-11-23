//! Drawing the chat input.

use super::*;

impl ChatInput {
    /// The input's text left of the box's edge and its x.
    pub(super) fn text_x() -> f32 {
        4.0
    }

    /// Where the completion list's lines are: x, top of the first, width.
    fn suggestion_box(&self, font: &Font, size: (f32, f32)) -> Option<(f32, f32, f32, usize)> {
        let list = self.suggestions.as_ref()?;
        let shown = list.matches.len().min(SUGGESTION_LINES);
        let width = list
            .matches
            .iter()
            .map(|(m, _)| font.text_width(m))
            .fold(0.0, f32::max)
            + 1.0;
        let prefix = self
            .input
            .slice(self.input.display_start().min(list.start), list.start);
        let x = (Self::text_x() + font.text_width(&prefix))
            .min(size.0 - width - 1.0)
            .max(0.0);
        let top = size.1 - 12.0 - 1.0 - shown as f32 * 12.0;
        Some((x, top, width, shown))
    }

    pub(super) fn suggestion_at(&self, font: &Font, size: (f32, f32)) -> Option<usize> {
        let (x, top, width, shown) = self.suggestion_box(font, size)?;
        let (mx, my) = self.mouse;
        if mx < x || mx >= x + width || my < top || my >= top + shown as f32 * 12.0 {
            return None;
        }
        let offset = self.suggestions.as_ref()?.offset;
        Some(offset + ((my - top) / 12.0) as usize)
    }

    /// `extractRenderState`.
    pub(super) fn draw_screen(
        &mut self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        chat: &Chat,
        settings: &ChatSettings,
        lang: &Lang,
        size: (f32, f32),
    ) {
        let (width, height) = size;
        let font = painter.font;
        let alpha = settings.background_opacity as f32;
        draw_list.push_fill(
            painter.white,
            2.0,
            height - 14.0,
            width - 4.0,
            12.0,
            [0.0, 0.0, 0.0, alpha],
        );
        let (start, end) = self.input.window(font, width - 4.0);
        let shown = self.input.slice(start, end);
        let style = if self.is_draft {
            TextStyle {
                color: Some(colors::GRAY),
                italic: true,
                ..TextStyle::default()
            }
        } else {
            TextStyle::colored(TEXT_COLOR)
        };
        let mut line = StyledText::default();
        match &self.colors {
            Some((value, colors)) if !self.is_draft && *value == self.value() => {
                for (c, &color) in shown.chars().zip(&colors[start..]) {
                    let mut buffer = [0; 4];
                    line.push(c.encode_utf8(&mut buffer), &TextStyle::colored(color));
                }
            }
            _ => line.push(&shown, &style),
        }
        let paint = painter.paint(TEXT_COLOR);
        let text_y = height - 12.0;
        text::draw(draw_list, &paint, &line, Self::text_x(), text_y);
        self.input.draw_cursor(
            draw_list,
            painter.glyphs(),
            start,
            (Self::text_x(), text_y),
            [TEXT_COLOR[0], TEXT_COLOR[1], TEXT_COLOR[2], 1.0],
        );
        self.input.draw_selection(
            draw_list,
            painter.glyphs(),
            (start, end),
            (Self::text_x(), text_y),
        );

        if self.suggestions.is_none() && !self.usage.is_empty() {
            let usage_width = self
                .usage
                .iter()
                .map(|line| text::width(font, line))
                .fold(0.0, f32::max);
            let before = self.input.slice(0, self.usage_start);
            let x = (Self::text_x() + font.text_width(&before))
                .min(Self::text_x() + (width - 4.0) - usage_width)
                .max(0.0);
            for (i, line) in self.usage.iter().enumerate() {
                let y = height - USAGE_OFFSET - 12.0 * i as f32;
                draw_list.push_fill(
                    painter.white,
                    x - 1.0,
                    y,
                    usage_width + 2.0,
                    12.0,
                    argb(0xD000_0000),
                );
                text::draw(draw_list, &painter.paint(colors::WHITE), line, x, y + 2.0);
            }
        }

        if let Some((x, top, list_width, shown)) = self.suggestion_box(font, size)
            && let Some(list) = &self.suggestions
        {
            let more_above = list.offset > 0;
            let more_below = list.matches.len() > list.offset + shown;
            if more_above || more_below {
                draw_list.push_fill(
                    painter.white,
                    x,
                    top - 1.0,
                    list_width,
                    1.0,
                    argb(0xD000_0000),
                );
                draw_list.push_fill(
                    painter.white,
                    x,
                    top + shown as f32 * 12.0,
                    list_width,
                    1.0,
                    argb(0xD000_0000),
                );
            }
            let mut hovered_tooltip = None;
            for row in 0..shown {
                let index = list.offset + row;
                let y = top + row as f32 * 12.0;
                draw_list.push_fill(painter.white, x, y, list_width, 12.0, argb(0xD000_0000));
                let selected = index == list.selected;
                let color = if selected {
                    SUGGESTION_SELECTED
                } else {
                    SUGGESTION_COLOR
                };
                let (text, tooltip) = &list.matches[index];
                font.push_text(
                    draw_list,
                    painter.font_textures,
                    text,
                    x + 1.0,
                    y + 2.0,
                    [color[0], color[1], color[2], 1.0],
                );
                let (mx, my) = self.mouse;
                if mx >= x && mx < x + list_width && my >= y && my < y + 12.0 {
                    hovered_tooltip = tooltip.clone();
                }
            }
            if let Some(tooltip) = hovered_tooltip {
                painter.tooltip_lines(draw_list, &[tooltip], self.mouse, size);
                return;
            }
        }

        let (mx, my) = self.mouse;
        if let Some(tag) = chat.tag_at(height, mx, my) {
            painter.tooltip(draw_list, lang.get_dynamic(tag.text), self.mouse, size);
        } else if let Some(style) = chat.style_at(font, height, mx, my)
            && let Some(hover) = style.hover.as_deref()
        {
            let lines = match hover {
                Hover::Text(text) => text::wrap(font, text, (width / 2.0).max(200.0)),
                Hover::Entity(name) => vec![name.clone()],
                Hover::Item {
                    id,
                    count,
                    components,
                } => match crate::gui::hud::item_slot::hover_stack(id, *count, components.as_ref())
                {
                    Some(stack) => crate::gui::hud::item_slot::tooltip_lines(lang, &stack),
                    None => vec![StyledText::plain(id)],
                },
            };
            painter.tooltip_lines(draw_list, &lines, self.mouse, size);
        }
    }
}
