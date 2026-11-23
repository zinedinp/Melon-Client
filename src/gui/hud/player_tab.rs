//! Vanilla `PlayerTabOverlay`: the player list while the Tab key is held.

use std::collections::HashMap;

use crate::assets::Assets;
use crate::component::Component;
use crate::gui::face::draw_face;
use crate::gui::hud::bars::hearts::{HeartKind, Sprites as HeartSprites};
use crate::gui::hud::item_slot::ItemPainter;
use crate::gui::sprite::GuiSprite;
use crate::gui::text::{self, StyledText, colors};
use crate::i18n::Lang;
use crate::renderer::quad::DrawList;
use crate::renderer::{Renderer, TextureId};
use crate::world::scoreboard::{NumberFormat, Objective, RenderType, Score};

/// `PlayerTabOverlay.MAX_ROWS_PER_COL`.
pub const MAX_ROWS_PER_COL: usize = 20;
/// `getPlayerInfos`' `limit(80)`.
pub const MAX_PLAYERS: usize = 80;
/// `Integer.MIN_VALUE`: half-transparent black behind each block.
const BLOCK_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
/// `options.getBackgroundColor(0x20FFFFFF)`: a row's background.
const ROW_BACKGROUND: [f32; 4] = [1.0, 1.0, 1.0, 32.0 / 255.0];
/// `0x90FFFFFF`: a spectator's name.
const SPECTATOR_ALPHA: f32 = 144.0 / 255.0;

/// `icon/ping_unknown` and `icon/ping_1` (worst) to `ping_5` (best).
pub struct TabTextures {
    ping: [GuiSprite; 6],
}

impl TabTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut load = |name: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/icon/{name}.png"),
            )
        };
        Ok(Self {
            ping: [
                load("ping_unknown")?,
                load("ping_1")?,
                load("ping_2")?,
                load("ping_3")?,
                load("ping_4")?,
                load("ping_5")?,
            ],
        })
    }

    fn ping(&self, latency: i32) -> GuiSprite {
        self.ping[ping_bars(latency)]
    }
}

/// `extractPingIcon`: bars by milliseconds, `0` for unknown.
fn ping_bars(latency: i32) -> usize {
    match latency {
        ..0 => 0,
        0..150 => 5,
        150..300 => 4,
        300..600 => 3,
        600..1000 => 2,
        _ => 1,
    }
}

/// One listed player, in the order the list shows them.
pub struct TabEntry<'a> {
    pub id: uuid::Uuid,
    /// `getNameForDisplay`.
    pub name: StyledText,
    pub spectator: bool,
    /// Milliseconds; negative while unknown.
    pub latency: i32,
    pub hat: bool,
    /// `AvatarRenderer.isPlayerUpsideDown`.
    pub flip: bool,
    pub skin: TextureId,
    /// The player's score in the listed objective.
    pub score: Option<&'a Score>,
}

/// What a frame of the list needs.
pub struct TabDraw<'a> {
    pub textures: &'a TabTextures,
    pub hearts: &'a HeartSprites,
    pub painter: ItemPainter<'a>,
    pub lang: &'a Lang,
    pub entries: &'a [TabEntry<'a>],
    /// The objective in the `list` slot.
    pub objective: Option<&'a Objective>,
    /// `ClientPacketListener.onlineMode`: faces only where skins are real.
    pub show_heads: bool,
    pub width: f32,
    /// `Hud.getGuiTicks`, for the hearts' blink.
    pub ticks: u32,
}

#[derive(Default)]
pub struct PlayerTabOverlay {
    header: Option<Component>,
    footer: Option<Component>,
    visible: bool,
    health: HashMap<uuid::Uuid, HealthState>,
}

impl PlayerTabOverlay {
    /// `handleTabListCustomisation`; text that says nothing is no header.
    pub fn set_header_footer(&mut self, header: Component, footer: Component) {
        self.header = Some(header);
        self.footer = Some(footer);
    }

    /// `setVisible`: the hearts' memory starts over each time.
    pub fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.health.clear();
            self.visible = visible;
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// `extractRenderState`.
    pub fn draw(&mut self, draw_list: &mut DrawList, frame: &TabDraw) {
        let font = frame.painter.font;
        let width_of = |text: &StyledText| text::width(font, text).ceil() as i32;
        let screen_width = frame.width as i32;
        let spacer = font.text_width(" ").ceil() as i32;
        let hearts = frame
            .objective
            .is_some_and(|o| o.render_type == RenderType::Hearts);
        let default_format = frame
            .objective
            .and_then(|o| o.number_format.clone())
            .unwrap_or_else(NumberFormat::player_list_default);
        let mut max_name = 0;
        let mut max_score = 0;
        let scores: Vec<Option<(StyledText, i32)>> = frame
            .entries
            .iter()
            .map(|entry| {
                max_name = max_name.max(width_of(&entry.name));
                if frame.objective.is_none() || hearts {
                    return None;
                }
                let shown = entry.score.map(|score| {
                    let text =
                        StyledText::from_component(&score.format(&default_format), frame.lang);
                    let width = width_of(&text);
                    (text, width)
                });
                let width = shown.as_ref().map_or(0, |s| s.1);
                max_score = max_score.max(if width > 0 { spacer + width } else { 0 });
                shown
            })
            .collect();
        if !self.health.is_empty() {
            self.health
                .retain(|id, _| frame.entries.iter().any(|e| e.id == *id));
        }

        let slots = frame.entries.len();
        let mut rows = slots;
        let mut cols = 1;
        while rows > MAX_ROWS_PER_COL {
            cols += 1;
            rows = slots.div_ceil(cols);
        }
        let (rows, cols) = (rows as i32, cols as i32);
        let head = if frame.show_heads { 9 } else { 0 };
        let width_for_score = match frame.objective {
            None => 0,
            Some(_) if hearts => 90,
            Some(_) => max_score,
        };
        let slot_width =
            (cols * (head + max_name + width_for_score + 13)).min(screen_width - 50) / cols;
        let left = screen_width / 2 - (slot_width * cols + (cols - 1) * 5) / 2;
        let mut y = 10;
        let mut max_line = slot_width * cols + (cols - 1) * 5;
        let lines = |text: &Option<Component>| {
            let text = StyledText::from_component(text.as_ref()?, frame.lang);
            let lines = (!text.to_plain().is_empty())
                .then(|| text::wrap(font, &text, (screen_width - 50) as f32));
            crate::probe::note(
                "tab.text",
                format!("header/footer {:?}", text.to_plain()),
                || {
                    let fonts: Vec<&str> = text
                        .spans
                        .iter()
                        .filter_map(|s| s.style.font.as_deref())
                        .collect();
                    let widths: Vec<String> = lines
                        .iter()
                        .flatten()
                        .map(|l| format!("{:.1}", text::width(font, l)))
                        .collect();
                    let advances: Vec<String> = text
                        .spans
                        .iter()
                        .flat_map(|s| {
                            s.text
                                .chars()
                                .map(move |c| (c, font.advance_in(s.style.font.as_deref(), c)))
                        })
                        .take(60)
                        .map(|(c, a)| format!("U+{:04X}={a}", c as u32))
                        .collect();
                    format!(
                        "fonts={fonts:?} max_width={} line_widths={widths:?} advances={advances:?} raw={:?}",
                        screen_width - 50,
                        text.spans
                            .iter()
                            .map(|s| (&s.text, &s.style.font))
                            .collect::<Vec<_>>()
                    )
                },
            );
            lines
        };
        let header = lines(&self.header);
        let footer = lines(&self.footer);
        for line in header.iter().chain(&footer).flatten() {
            max_line = max_line.max(width_of(line));
        }
        let center = screen_width / 2;
        let white = frame.painter.white;
        let block = |draw_list: &mut DrawList, y: i32, height: i32| {
            draw_list.push_fill(
                white,
                (center - max_line / 2 - 1) as f32,
                (y - 1) as f32,
                (max_line / 2 * 2 + 2) as f32,
                (height + 1) as f32,
                BLOCK_BACKGROUND,
            );
        };
        let paint = frame.painter.paint(colors::WHITE);
        let centered = |draw_list: &mut DrawList, lines: &[StyledText], y: &mut i32| {
            for line in lines {
                let x = center - width_of(line) / 2;
                text::draw(draw_list, &paint, line, x as f32, *y as f32);
                *y += 9;
            }
        };

        if let Some(header) = &header {
            block(draw_list, y, header.len() as i32 * 9);
            centered(draw_list, header, &mut y);
            y += 1;
        }
        block(draw_list, y, rows * 9);
        for (i, entry) in frame.entries.iter().enumerate() {
            let (col, row) = (i as i32 / rows, i as i32 % rows);
            let mut x = left + col * slot_width + col * 5;
            let row_y = y + row * 9;
            draw_list.push_fill(
                white,
                x as f32,
                row_y as f32,
                slot_width as f32,
                8.0,
                ROW_BACKGROUND,
            );
            if frame.show_heads {
                let tint = [1.0; 4];
                draw_face(draw_list, entry.skin, [x as f32, row_y as f32], 8.0, entry.hat, entry.flip, tint);
                x += 9;
            }
            let mut name_paint = paint;
            if entry.spectator {
                name_paint.alpha = SPECTATOR_ALPHA;
            }
            text::draw(draw_list, &name_paint, &entry.name, x as f32, row_y as f32);
            if let Some(objective) = frame.objective
                && !entry.spectator
            {
                let score_left = x + max_name + 1;
                let score_right = score_left + width_for_score;
                if score_right - score_left > 5 {
                    if objective.render_type == RenderType::Hearts {
                        let value = entry.score.map_or(0, |s| s.value);
                        self.draw_hearts(
                            draw_list,
                            frame,
                            entry.id,
                            value,
                            row_y,
                            score_left,
                            score_right,
                        );
                    } else if let Some((text, width)) = &scores[i] {
                        let at = (score_right - width) as f32;
                        text::draw(draw_list, &paint, text, at, row_y as f32);
                    }
                }
            }
            let ping = frame.textures.ping(entry.latency);
            ping.draw(
                draw_list,
                (x - head + slot_width - 11) as f32,
                row_y as f32,
                10.0,
                8.0,
            );
        }
        if let Some(footer) = &footer {
            y += rows * 9 + 1;
            block(draw_list, y, footer.len() as i32 * 9);
            centered(draw_list, footer, &mut y);
        }
    }

    /// `extractTablistHearts`.
    #[allow(clippy::too_many_arguments)]
    fn draw_hearts(
        &mut self,
        draw_list: &mut DrawList,
        frame: &TabDraw,
        id: uuid::Uuid,
        score: i32,
        y: i32,
        left: i32,
        right: i32,
    ) {
        let health = self
            .health
            .entry(id)
            .or_insert_with(|| HealthState::new(score));
        health.update(score, frame.ticks);
        let shown = health.displayed;
        let full_hearts = (score.max(shown).max(0) as u32).div_ceil(2) as i32;
        let hearts_to_render = score.max(shown.max(20)) / 2;
        let blink = health.blinking(frame.ticks);
        if full_hearts <= 0 {
            return;
        }
        let per_heart = ((right - left - 4) as f32 / hearts_to_render as f32)
            .min(9.0)
            .floor() as i32;
        if per_heart <= 3 {
            let pct = (score as f32 / 20.0).clamp(0.0, 1.0);
            let color = [1.0 - pct, pct, 0.0].map(|c| (c * 255.0) as u8 as f32 / 255.0);
            let hearts = score as f32 / 2.0;
            let number = format!("{hearts:?}");
            let hp = StyledText::plain(
                &frame
                    .lang
                    .get("multiplayer.player.list.hp")
                    .replacen("%s", &number, 1),
            );
            let font = frame.painter.font;
            let text = if right - text::width(font, &hp).ceil() as i32 >= left {
                hp
            } else {
                StyledText::plain(&number)
            };
            let x = (right + left - text::width(font, &text).ceil() as i32) / 2;
            text::draw(
                draw_list,
                &frame.painter.paint(color),
                &text,
                x as f32,
                y as f32,
            );
            return;
        }
        let sprites = frame.hearts;
        let at = |heart: i32| (left + heart * per_heart) as f32;
        let container = sprites.container(false, blink);
        for heart in full_hearts..hearts_to_render {
            container.draw(draw_list, at(heart), y as f32, 9.0, 9.0);
        }
        for heart in 0..full_hearts {
            let x = at(heart);
            container.draw(draw_list, x, y as f32, 9.0, 9.0);
            if blink {
                if heart * 2 + 1 < shown {
                    sprites
                        .heart(HeartKind::Normal, false, false, true)
                        .draw(draw_list, x, y as f32, 9.0, 9.0);
                }
                if heart * 2 + 1 == shown {
                    sprites
                        .heart(HeartKind::Normal, false, true, true)
                        .draw(draw_list, x, y as f32, 9.0, 9.0);
                }
            }
            let extra = heart >= 10;
            let kind = if extra {
                HeartKind::Absorbing
            } else {
                HeartKind::Normal
            };
            if heart * 2 + 1 < score {
                sprites
                    .heart(kind, false, false, extra)
                    .draw(draw_list, x, y as f32, 9.0, 9.0);
            }
            if heart * 2 + 1 == score {
                sprites
                    .heart(kind, false, true, extra)
                    .draw(draw_list, x, y as f32, 9.0, 9.0);
            }
        }
    }
}

/// `PlayerTabOverlay.HealthState`.
#[derive(Clone, Copy, Debug)]
struct HealthState {
    last: i32,
    displayed: i32,
    last_update: u32,
    blink_until: u32,
}

impl HealthState {
    fn new(value: i32) -> Self {
        Self {
            last: value,
            displayed: value,
            last_update: 0,
            blink_until: 0,
        }
    }

    fn update(&mut self, value: i32, tick: u32) {
        if value != self.last {
            self.blink_until = tick + if value < self.last { 20 } else { 10 };
            self.last = value;
            self.last_update = tick;
        }
        if tick.wrapping_sub(self.last_update) > 20 {
            self.displayed = value;
        }
    }

    fn blinking(&self, tick: u32) -> bool {
        self.blink_until > tick && (self.blink_until - tick) % 6 >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_bars_follow_the_latency() {
        assert_eq!(
            [-1, 0, 149, 150, 599, 999, 1000].map(ping_bars),
            [0, 5, 5, 4, 3, 2, 1]
        );
    }

    #[test]
    fn a_loss_blinks_longer_and_shows_the_old_value_a_second() {
        let mut health = HealthState::new(20);
        health.update(20, 100);
        health.update(14, 101);
        assert_eq!(health.displayed, 20);
        assert_eq!(health.blink_until, 121);
        assert!(!health.blinking(101));
        assert!(health.blinking(104));
        assert!(!health.blinking(121));
        health.update(14, 122);
        assert_eq!(health.displayed, 14);
        health.update(16, 130);
        assert_eq!(health.blink_until, 140);
    }
}
