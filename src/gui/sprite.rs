//! GUI sprites with what their `.mcmeta` says about them, as vanilla's `gui`

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::Value;

use crate::assets::{Assets, decode_png, packs, split_id};
use crate::renderer::quad::DrawList;
use crate::renderer::{Renderer, TextureId};
use crate::world::texture_animation;

/// `GuiSpriteScaling`: how a sprite covers a rectangle of another size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    /// The whole sprite stretched over the rectangle (the default).
    Stretch,
    /// Repeated from the top left, a `width`x`height` copy at a time.
    Tile { width: u32, height: u32 },
    /// Corners as they are, edges and centre tiled or stretched.
    NineSlice {
        width: u32,
        height: u32,
        border: [u32; 4],
        stretch_inner: bool,
    },
}

impl Scaling {
    /// The `gui.scaling` section of a sprite's `.mcmeta`; stretched when it has none.
    pub fn parse(meta: &Value) -> Self {
        let scaling = &meta["gui"]["scaling"];
        let positive = |v: &Value| v.as_u64().filter(|&n| n > 0).map(|n| n as u32);
        let size = (positive(&scaling["width"]), positive(&scaling["height"]));
        match (scaling["type"].as_str(), size) {
            (Some("tile"), (Some(width), Some(height))) => Self::Tile { width, height },
            (Some("nine_slice"), (Some(width), Some(height))) => {
                let border = &scaling["border"];
                let border = match border.as_u64() {
                    Some(all) => [all as u32; 4],
                    None => {
                        let side = |key: &str| border[key].as_u64().map(|n| n as u32);
                        match (side("left"), side("top"), side("right"), side("bottom")) {
                            (Some(l), Some(t), Some(r), Some(b)) => [l, t, r, b],
                            _ => return Self::Stretch,
                        }
                    }
                };
                let [left, top, right, bottom] = border;
                if left + right >= width || top + bottom >= height {
                    return Self::Stretch;
                }
                Self::NineSlice {
                    width,
                    height,
                    border,
                    stretch_inner: scaling["stretch_inner"].as_bool().unwrap_or(false),
                }
            }
            _ => Self::Stretch,
        }
    }
}

/// A sprite: its texture, scaling and animation frames.
#[derive(Clone, Copy, Debug)]
pub struct GuiSprite {
    pub texture: TextureId,
    pub scaling: Scaling,
    /// One frame's share of the texture, `[u, v]`.
    frame: [f32; 2],
    /// Frames per row of the image.
    columns: u32,
    /// `(frame, ticks)` in play order; empty for a still sprite.
    schedule: &'static [(u32, u32)],
    /// The part of the texture the sprite is, `[u, v, width, height]`: all
    /// of it, or one slot of a shared sheet.
    region: [f32; 4],
}

impl From<TextureId> for GuiSprite {
    /// A texture as a still, stretched sprite (a server icon, a skin face).
    fn from(texture: TextureId) -> Self {
        Self {
            texture,
            scaling: Scaling::Stretch,
            frame: [1.0, 1.0],
            columns: 1,
            schedule: &[],
            region: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

impl GuiSprite {
    /// `textures/<path>` from the top resource pack that has it, with its `.mcmeta`.
    pub fn load(renderer: &mut Renderer, assets: &Assets, id: &str) -> anyhow::Result<Self> {
        let (namespace, path) = split_id(id);
        let path = format!("textures/{path}");
        let stack = packs::active();
        let (bytes, layer) = stack
            .find(assets.root(), namespace, &path)
            .ok_or_else(|| anyhow::anyhow!("texture missing: {id}"))?;
        let image = decode_png(&bytes).ok_or_else(|| anyhow::anyhow!("Textur kaputt: {id}"))?;
        let meta = stack
            .find_metadata(assets.root(), namespace, &path, layer)
            .and_then(|m| serde_json::from_slice(packs::meta::strip_bom(&m)).ok())
            .unwrap_or(Value::Null);
        let texture = renderer.load_texture(&image)?;
        Ok(Self::with_meta(texture, image.dimensions(), &meta))
    }

    /// `texture`, `width`x`height` pixels, as its `.mcmeta` describes it.
    pub fn with_meta(texture: TextureId, (width, height): (u32, u32), meta: &Value) -> Self {
        let mut sprite = Self::from(texture);
        sprite.scaling = Scaling::parse(meta);
        let Some(animation) = meta.get("animation") else {
            return sprite;
        };
        let (fw, fh) = texture_animation::frame_size(animation, width, height);
        let columns = (width / fw).max(1);
        let count = (columns * (height / fh)).max(1) as usize;
        let schedule = texture_animation::schedule(animation, count);
        if schedule.is_empty() {
            return sprite;
        }
        sprite.frame = [fw as f32 / width as f32, fh as f32 / height as f32];
        sprite.columns = columns;
        sprite.schedule = intern(schedule.iter().map(|&(f, t)| (f as u32, t)).collect());
        sprite
    }

    /// The sprite as `region` (`[u0, v0, u1, v1]`) of its texture: its
    /// frames and scaling then cover that part only.
    pub fn within(mut self, [u0, v0, u1, v1]: [f32; 4]) -> Self {
        self.region = [u0, v0, u1 - u0, v1 - v0];
        self
    }

    /// The frame showing now, as a [`Blit`] multiplied by `color`.
    fn blit(&self, color: [f32; 4]) -> Blit {
        let [ru, rv, rw, rh] = self.region;
        let [ou, ov] = self.frame_origin();
        Blit {
            texture: self.texture,
            origin: [ru + ou * rw, rv + ov * rh],
            frame: [self.frame[0] * rw, self.frame[1] * rh],
            color,
        }
    }

    /// Whether it has more than one frame.
    pub fn is_animated(&self) -> bool {
        self.schedule.len() > 1
    }

    /// The frame showing now (vanilla ticks the GUI atlas with the client:
    fn frame_origin(&self) -> [f32; 2] {
        let period: u64 = self.schedule.iter().map(|&(_, t)| u64::from(t)).sum();
        let frame = if period == 0 {
            self.schedule.first().map_or(0, |&(f, _)| f)
        } else {
            let mut tick = ticks() % period;
            let mut frame = 0;
            for &(f, time) in self.schedule {
                frame = f;
                if tick < u64::from(time) {
                    break;
                }
                tick -= u64::from(time);
            }
            frame
        };
        let (column, row) = (frame % self.columns, frame / self.columns);
        [column as f32 * self.frame[0], row as f32 * self.frame[1]]
    }

    /// `GuiGraphics.blitSprite`: over `x, y, w, h` as its scaling says.
    pub fn draw(&self, list: &mut DrawList, x: f32, y: f32, w: f32, h: f32) {
        self.draw_tinted(list, x, y, w, h, [1.0; 4]);
    }

    /// [`Self::draw`] multiplied by `color`.
    pub fn draw_tinted(
        &self,
        list: &mut DrawList,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let blit = self.blit(color);
        match self.scaling {
            Scaling::Stretch => blit.part(list, [x, y, w, h], [0.0, 0.0, 1.0, 1.0]),
            Scaling::Tile { width, height } => {
                let (tw, th) = (width as f32, height as f32);
                blit.tiled(list, [x, y, w, h], [0.0, 0.0, tw, th], [tw, th]);
            }
            Scaling::NineSlice {
                width,
                height,
                border,
                stretch_inner,
            } => blit.nine_slice(list, [x, y, w, h], [width, height], border, stretch_inner),
        }
    }

    /// `blitSprite` with a texture region: `rect` shows the `sprite_size` sprite from `tex` on.
    pub fn draw_region(
        &self,
        list: &mut DrawList,
        sprite_size: [f32; 2],
        tex: [f32; 2],
        rect: [f32; 4],
    ) {
        let [x, y, w, h] = rect;
        let [sw, sh] = sprite_size;
        let [tx, ty] = tex;
        if self.scaling == Scaling::Stretch {
            let blit = self.blit([1.0; 4]);
            let uv = [tx / sw, ty / sh, (tx + w) / sw, (ty + h) / sh];
            blit.part(list, [x, y, w, h], uv);
        } else {
            let previous = list.scissor();
            list.set_scissor(Some(intersect(previous, [x, y, w, h])));
            self.draw(list, x - tx, y - ty, sw, sh);
            list.set_scissor(previous);
        }
    }
}

/// The overlap of the scissor in force (if any) and `rect`.
fn intersect(scissor: Option<[f32; 4]>, rect: [f32; 4]) -> [f32; 4] {
    let Some([sx, sy, sw, sh]) = scissor else {
        return rect;
    };
    let [x, y, w, h] = rect;
    let (x0, y0) = (x.max(sx), y.max(sy));
    let (x1, y1) = ((x + w).min(sx + sw), (y + h).min(sy + sh));
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

/// Client ticks since the first sprite was drawn: the GUI animations' clock.
fn ticks() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    (START.get_or_init(Instant::now).elapsed().as_millis() / 50) as u64
}

/// Keeps one copy of each schedule for the life of the program.
fn intern(schedule: Vec<(u32, u32)>) -> &'static [(u32, u32)] {
    static SEEN: Mutex<Vec<&'static [(u32, u32)]>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&known) = seen.iter().find(|&&known| known == schedule.as_slice()) {
        return known;
    }
    let leaked: &'static [(u32, u32)] = Box::leak(schedule.into_boxed_slice());
    seen.push(leaked);
    leaked
}

/// One frame of a sprite, ready to cut quads from.
struct Blit {
    texture: TextureId,
    /// The frame's top left corner in the texture.
    origin: [f32; 2],
    /// The frame's size in the texture.
    frame: [f32; 2],
    color: [f32; 4],
}

impl Blit {
    /// `rect` showing `uv`, a region of the frame in 0..1.
    fn part(&self, list: &mut DrawList, rect: [f32; 4], uv: [f32; 4]) {
        let [x, y, w, h] = rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let [ou, ov] = self.origin;
        let [fu, fv] = self.frame;
        let [u0, v0, u1, v1] = uv;
        let uv = [ou + u0 * fu, ov + v0 * fv, ou + u1 * fu, ov + v1 * fv];
        list.push_rect(self.texture, [x, y, w, h], uv, self.color);
    }

    /// `blitTiledSprite`.
    fn tiled(&self, list: &mut DrawList, rect: [f32; 4], tile: [f32; 4], sprite: [f32; 2]) {
        let [x, y, w, h] = rect;
        let [tx, ty, tw, th] = tile;
        let [sw, sh] = sprite;
        if w <= 0.0 || h <= 0.0 || tw <= 0.0 || th <= 0.0 {
            return;
        }
        let (u0, u1) = (tx / sw, (tx + tw) / sw);
        let (v0, v1) = (ty / sh, (ty + th) / sh);
        let mut dx = 0.0;
        while dx < w {
            let cw = tw.min(w - dx);
            let cu1 = u0 + (u1 - u0) * (cw / tw);
            let mut dy = 0.0;
            while dy < h {
                let ch = th.min(h - dy);
                let cv1 = v0 + (v1 - v0) * (ch / th);
                self.part(list, [x + dx, y + dy, cw, ch], [u0, v0, cu1, cv1]);
                dy += th;
            }
            dx += tw;
        }
    }

    /// `blitNineSlicedSprite`.
    fn nine_slice(
        &self,
        list: &mut DrawList,
        rect: [f32; 4],
        size: [u32; 2],
        border: [u32; 4],
        stretch_inner: bool,
    ) {
        let [x, y, w, h] = rect;
        let (sw, sh) = (size[0] as f32, size[1] as f32);
        let [left, top, right, bottom] = border.map(|b| b as f32);
        let (half_w, half_h) = ((w / 2.0).floor(), (h / 2.0).floor());
        let (left, right) = (left.min(half_w), right.min(half_w));
        let (top, bottom) = (top.min(half_h), bottom.min(half_h));
        let axis = |start: f32, len: f32, size: f32, lo: f32, hi: f32| -> Vec<Segment> {
            if len == size {
                vec![(start, len, 0.0, size, false)]
            } else {
                vec![
                    (start, lo, 0.0, lo, false),
                    (start + lo, len - lo - hi, lo, size - lo - hi, true),
                    (start + len - hi, hi, size - hi, hi, false),
                ]
            }
        };
        let columns = axis(x, w, sw, left, right);
        let rows = axis(y, h, sh, top, bottom);
        for &(ry, rh, tv, th, inner_row) in &rows {
            for &(cx, cw, tu, tw, inner_column) in &columns {
                if cw <= 0.0 || rh <= 0.0 {
                    continue;
                }
                if !(inner_row || inner_column) {
                    let uv = [tu / sw, tv / sh, (tu + cw) / sw, (tv + rh) / sh];
                    self.part(list, [cx, ry, cw, rh], uv);
                } else if stretch_inner {
                    let uv = [tu / sw, tv / sh, (tu + tw) / sw, (tv + th) / sh];
                    self.part(list, [cx, ry, cw, rh], uv);
                } else {
                    self.tiled(list, [cx, ry, cw, rh], [tu, tv, tw, th], [sw, sh]);
                }
            }
        }
    }
}

type Segment = (f32, f32, f32, f32, bool);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scaling_reads_every_kind() {
        assert_eq!(Scaling::parse(&Value::Null), Scaling::Stretch);
        let tile = json!({"gui": {"scaling": {"type": "tile", "width": 16, "height": 8}}});
        assert_eq!(
            Scaling::parse(&tile),
            Scaling::Tile {
                width: 16,
                height: 8
            }
        );
        let even = json!({"gui": {"scaling": {"type": "nine_slice", "width": 200, "height": 20, "border": 3}}});
        assert_eq!(
            Scaling::parse(&even),
            Scaling::NineSlice {
                width: 200,
                height: 20,
                border: [3; 4],
                stretch_inner: false
            }
        );
        let sides = json!({"gui": {"scaling": {"type": "nine_slice", "width": 109, "height": 15,
            "border": {"left": 54, "top": 7, "right": 54, "bottom": 7}, "stretch_inner": true}}});
        assert_eq!(
            Scaling::parse(&sides),
            Scaling::NineSlice {
                width: 109,
                height: 15,
                border: [54, 7, 54, 7],
                stretch_inner: true
            }
        );
        let bad = json!({"gui": {"scaling": {"type": "nine_slice", "width": 8, "height": 8, "border": 4}}});
        assert_eq!(Scaling::parse(&bad), Scaling::Stretch);
    }

    fn quads(list: &DrawList) -> usize {
        list.vertices.len() / 4
    }

    #[test]
    fn nine_slice_corners_stay_and_the_middle_tiles() {
        let meta = json!({"gui": {"scaling": {"type": "nine_slice", "width": 200, "height": 20, "border": 3}}});
        let sprite = GuiSprite::with_meta(TextureId(1), (200, 20), &meta);
        let mut list = DrawList::default();
        sprite.draw(&mut list, 0.0, 0.0, 200.0, 20.0);
        assert_eq!(quads(&list), 1);
        let mut list = DrawList::default();
        sprite.draw(&mut list, 0.0, 0.0, 300.0, 20.0);
        assert_eq!(quads(&list), 4);
        let mut list = DrawList::default();
        sprite.draw(&mut list, 0.0, 0.0, 20.0, 20.0);
        assert_eq!(quads(&list), 3);
    }

    #[test]
    fn animated_strips_show_one_frame() {
        let meta = json!({"animation": {"frametime": 15}});
        let sprite = GuiSprite::with_meta(TextureId(1), (24, 48), &meta);
        assert!(sprite.is_animated());
        assert_eq!(sprite.frame, [1.0, 0.5]);
        let mut list = DrawList::default();
        sprite.draw(&mut list, 0.0, 0.0, 24.0, 24.0);
        let v: Vec<f32> = list.vertices.iter().map(|v| v.uv[1]).collect();
        let (lo, hi) = v
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!((hi - lo - 0.5).abs() < 1e-6, "one frame tall: {lo}..{hi}");
    }
}
