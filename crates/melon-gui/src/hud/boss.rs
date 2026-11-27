//! `BossHealthOverlay`: the boss bars under the top edge.

use std::time::Instant;

use uuid::Uuid;

use super::item_slot::ItemPainter;
use crate::sprite::GuiSprite;
use crate::text::{self, StyledText, TextPaint, colors};
use melon_assets::assets::Assets;
use melon_net::play::game::boss::{BossEvent, COLORS, OVERLAYS};
use melon_render::Renderer;
use melon_render::quad::DrawList;

const BAR_WIDTH: f32 = 182.0;
const BAR_HEIGHT: f32 = 5.0;
/// `LerpingBossEvent.LERP_MILLISECONDS`.
const LERP_SECONDS: f32 = 0.1;

const COLOR_NAMES: [&str; COLORS] = ["pink", "blue", "red", "green", "yellow", "purple", "white"];
const OVERLAY_NAMES: [&str; OVERLAYS - 1] = ["notched_6", "notched_10", "notched_12", "notched_20"];

/// `boss_bar/*` sprites: background and progress of each color and notch.
pub struct BossSprites {
    color_background: Vec<GuiSprite>,
    color_progress: Vec<GuiSprite>,
    overlay_background: Vec<GuiSprite>,
    overlay_progress: Vec<GuiSprite>,
}

impl BossSprites {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut load = |name: &str, kind: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/boss_bar/{name}_{kind}.png"),
            )
        };
        let mut all = |names: &[&str], kind: &str| {
            names
                .iter()
                .map(|name| load(name, kind))
                .collect::<anyhow::Result<Vec<_>>>()
        };
        Ok(Self {
            color_background: all(&COLOR_NAMES, "background")?,
            color_progress: all(&COLOR_NAMES, "progress")?,
            overlay_background: all(&OVERLAY_NAMES, "background")?,
            overlay_progress: all(&OVERLAY_NAMES, "progress")?,
        })
    }
}

/// `LerpingBossEvent`.
struct Bar {
    id: Uuid,
    name: StyledText,
    from: f32,
    target: f32,
    set: Instant,
    color: u8,
    overlay: u8,
    flags: u8,
}

impl Bar {
    fn progress(&self) -> f32 {
        let t = (self.set.elapsed().as_secs_f32() / LERP_SECONDS).clamp(0.0, 1.0);
        self.from + (self.target - self.from) * t
    }
}

/// The bars the server shows, in the order they were added.
#[derive(Default)]
pub struct BossBars(Vec<Bar>);

impl BossBars {
    /// `BossHealthOverlay.update`.
    pub fn apply(&mut self, event: BossEvent, lang: &melon_assets::i18n::Lang) {
        let find = |bars: &mut Vec<Bar>, id: Uuid| bars.iter().position(|bar| bar.id == id);
        match event {
            BossEvent::Add {
                id,
                name,
                progress,
                color,
                overlay,
                flags,
            } => {
                let bar = Bar {
                    id,
                    name: StyledText::from_component(&name, lang),
                    from: progress,
                    target: progress,
                    set: Instant::now(),
                    color,
                    overlay,
                    flags,
                };
                match find(&mut self.0, id) {
                    Some(at) => self.0[at] = bar,
                    None => self.0.push(bar),
                }
            }
            BossEvent::Remove(id) => self.0.retain(|bar| bar.id != id),
            BossEvent::Progress(id, progress) => {
                if let Some(at) = find(&mut self.0, id) {
                    let bar = &mut self.0[at];
                    bar.from = bar.progress();
                    bar.target = progress;
                    bar.set = Instant::now();
                }
            }
            BossEvent::Name(id, name) => {
                if let Some(at) = find(&mut self.0, id) {
                    self.0[at].name = StyledText::from_component(&name, lang);
                }
            }
            BossEvent::Style(id, color, overlay) => {
                if let Some(at) = find(&mut self.0, id) {
                    self.0[at].color = color;
                    self.0[at].overlay = overlay;
                }
            }
            BossEvent::Properties(id, flags) => {
                if let Some(at) = find(&mut self.0, id) {
                    self.0[at].flags = flags;
                }
            }
        }
    }

    /// `BossHealthOverlay.extractRenderState`.
    pub fn draw(
        &self,
        draw_list: &mut DrawList,
        sprites: &BossSprites,
        painter: ItemPainter,
        size: (f32, f32),
        seed: u32,
    ) {
        let mut y = 12.0;
        let left = (size.0 / 2.0).floor() - 91.0;
        for bar in &self.0 {
            let width = (bar.progress().clamp(0.0, 1.0) * BAR_WIDTH).floor();
            let color = bar.color as usize;
            let notch = (bar.overlay as usize).checked_sub(1);
            sprites.color_background[color].draw(draw_list, left, y, BAR_WIDTH, BAR_HEIGHT);
            if let Some(n) = notch {
                sprites.overlay_background[n].draw(draw_list, left, y, BAR_WIDTH, BAR_HEIGHT);
            }
            if width > 0.0 {
                let size = [BAR_WIDTH, BAR_HEIGHT];
                sprites.color_progress[color].draw_region(
                    draw_list,
                    size,
                    [0.0; 2],
                    [left, y, width, BAR_HEIGHT],
                );
                if let Some(n) = notch {
                    sprites.overlay_progress[n].draw_region(
                        draw_list,
                        size,
                        [0.0; 2],
                        [left, y, width, BAR_HEIGHT],
                    );
                }
            }
            let paint = TextPaint {
                obfuscation_seed: u64::from(seed),
                ..painter.paint(colors::WHITE)
            };
            let text_width = text::width(painter.font, &bar.name);
            let x = (size.0 / 2.0).floor() - (text_width / 2.0).floor();
            text::draw(draw_list, &paint, &bar.name, x, y - 9.0);
            y += 19.0;
            if y >= (size.1 / 3.0).floor() {
                break;
            }
        }
    }
}
