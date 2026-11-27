//! Animated atlas sprites (vanilla `SpriteContents.AnimatedTexture`).

use std::path::Path;

use image::RgbaImage;
use serde_json::Value;

use crate::block_models::mipmap;

/// An animated texture's frames and schedule (`(frame, ticks)` in play order).
pub struct Frames {
    /// The whole image the frames are cut from.
    pub strip: RgbaImage,
    pub images: Vec<RgbaImage>,
    pub schedule: Vec<(usize, u32)>,
    pub interpolate: bool,
}

impl Frames {
    /// The frame shown first, as packed into the atlas.
    pub fn first(&self) -> &RgbaImage {
        &self.images[self.first_index()]
    }

    /// The index of the frame shown first.
    pub fn first_index(&self) -> usize {
        self.schedule.first().map_or(0, |&(f, _)| f)
    }

    /// Every frame's levels 0 to [`mipmap::LEVELS`], cut from the whole
    /// strip's (`SpriteContents.byMipLevel`): solidified, and with holes
    /// scaled to the strip's coverage.
    pub fn mipmapped(&self) -> Vec<Vec<RgbaImage>> {
        let mut strip = self.strip.clone();
        let holes = strip.pixels().any(|p| p.0[3] == 0);
        if holes {
            mipmap::solidify(&mut strip);
        }
        let chain = mipmap::chain(&strip, holes);
        let (width, height) = self.images[0].dimensions();
        let per_row = (strip.width() / width).max(1);
        (0..self.images.len() as u32)
            .map(|i| {
                let (x, y) = ((i % per_row) * width, (i / per_row) * height);
                std::iter::once(&strip)
                    .chain(&chain)
                    .enumerate()
                    .map(|(level, image)| {
                        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
                        image::imageops::crop_imm(image, x >> level, y >> level, w, h).to_image()
                    })
                    .collect()
            })
            .collect()
    }

    /// Ticks until the schedule starts over.
    pub fn period(&self) -> u64 {
        self.schedule.iter().map(|&(_, t)| u64::from(t)).sum()
    }

    /// The frame showing `tick` ticks after the start (no blending).
    pub fn at(&self, tick: u64) -> &RgbaImage {
        let period = self.period();
        let mut left = if period == 0 { 0 } else { tick % period };
        for &(frame, time) in &self.schedule {
            if left < u64::from(time) {
                return &self.images[frame];
            }
            left -= u64::from(time);
        }
        self.first()
    }
}

/// The most frames an animated icon is baked into.
const MAX_FRAMES: usize = 64;
/// The longest stretch of ticks an icon's animations are followed over.
const MAX_PERIOD: u64 = 1200;

/// The ticks at which any animated texture `textures` loaded changes frame, and the period.
pub fn frame_changes<'a>(animations: impl Iterator<Item = &'a Frames>) -> Option<(Vec<u64>, u64)> {
    let animations: Vec<_> = animations.filter(|f| f.period() > 0).collect();
    let longest = animations.iter().map(|f| f.period()).max()?;
    let mut period = animations
        .iter()
        .try_fold(1u64, |lcm, f| {
            let p = f.period();
            let lcm = lcm / gcd(lcm, p) * p;
            (lcm <= MAX_PERIOD).then_some(lcm)
        })
        .unwrap_or(longest);
    let mut starts = vec![0];
    for frames in &animations {
        let mut tick = 0;
        while tick < period {
            for &(_, time) in &frames.schedule {
                tick += u64::from(time);
                if tick < period {
                    starts.push(tick);
                }
            }
        }
    }
    starts.sort_unstable();
    starts.dedup();
    if starts.len() > MAX_FRAMES {
        let frames = animations.iter().find(|f| f.period() == longest)?;
        period = longest;
        starts = vec![0];
        let mut tick = 0;
        for &(_, time) in &frames.schedule {
            tick += u64::from(time);
            if tick < period {
                starts.push(tick);
            }
        }
        starts.truncate(MAX_FRAMES);
    }
    (starts.len() > 1).then_some((starts, period))
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// One sprite's frames and where they go in the atlas.
pub struct SpriteAnimation {
    /// Top-left corner in the atlas, pixels.
    pub x: u32,
    pub y: u32,
    /// Per frame, its levels from 0 on ([`Frames::mipmapped`]).
    levels: Vec<Vec<RgbaImage>>,
    /// `(frame, ticks)` in play order (`AnimationMetadataSection.frames`).
    schedule: Vec<(usize, u32)>,
    interpolate: bool,
    step: usize,
    sub_tick: u32,
}

/// The frames of a texture, if it is animated.
pub fn load_frames(assets_root: &Path, texture_id: &str) -> Option<Frames> {
    let (png, meta) = crate::assets::texture_with_meta(assets_root, texture_id)?;
    frames_of(&crate::assets::decode_png(&png)?, meta?.get("animation")?)
}

/// `strip` cut into the frames its `animation` section describes, if there are any.
pub fn frames_of(strip: &RgbaImage, animation: &Value) -> Option<Frames> {
    let (width, height) = frame_size(animation, strip.width(), strip.height());
    let per_row = (strip.width() / width).max(1);
    let count = per_row * (strip.height() / height);
    let frames: Vec<RgbaImage> = (0..count)
        .map(|i| {
            let (fx, fy) = ((i % per_row) * width, (i / per_row) * height);
            image::imageops::crop_imm(strip, fx, fy, width, height).to_image()
        })
        .collect();
    let schedule = schedule(animation, frames.len());
    if frames.len() < 2 && schedule.len() < 2 {
        return None;
    }
    let interpolate = animation["interpolate"].as_bool().unwrap_or(false);
    Some(Frames {
        strip: strip.clone(),
        images: frames,
        schedule,
        interpolate,
    })
}

/// `AnimationMetadataSection.calculateFrameSize`.
pub fn frame_size(animation: &Value, image_width: u32, image_height: u32) -> (u32, u32) {
    let side = |key: &str| animation[key].as_u64().filter(|&n| n > 0).map(|n| n as u32);
    let square = image_width.min(image_height);
    let (width, height) = match (side("width"), side("height")) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, image_height),
        (None, Some(h)) => (image_width, h),
        (None, None) => (square, square),
    };
    (width.max(1), height.max(1))
}

/// `AnimationMetadataSection.frames`.
pub fn schedule(animation: &Value, count: usize) -> Vec<(usize, u32)> {
    let default_time = animation["frametime"].as_u64().unwrap_or(1).max(1) as u32;
    match animation["frames"].as_array() {
        Some(list) => list
            .iter()
            .filter_map(|entry| match entry {
                Value::Number(n) => Some((n.as_u64()? as usize, default_time)),
                Value::Object(_) => Some((
                    entry["index"].as_u64()? as usize,
                    entry["time"]
                        .as_u64()
                        .map_or(default_time, |t| t.max(1) as u32),
                )),
                _ => None,
            })
            .filter(|&(frame, _)| frame < count)
            .collect(),
        None => (0..count).map(|f| (f, default_time)).collect(),
    }
}

impl SpriteAnimation {
    /// `frames` placed at `(x, y)`, with `levels` from [`Frames::mipmapped`].
    pub fn new(x: u32, y: u32, frames: Frames, levels: Vec<Vec<RgbaImage>>) -> Self {
        Self {
            x,
            y,
            levels,
            schedule: frames.schedule,
            interpolate: frames.interpolate,
            step: 0,
            sub_tick: 0,
        }
    }

    /// `AnimationState.tick`: one client tick on; whether the sprite has to
    /// be drawn again (`needsToDraw`).
    fn tick(&mut self) -> bool {
        if self.schedule.is_empty() {
            return false;
        }
        self.sub_tick += 1;
        let (frame, time) = self.schedule[self.step];
        let mut changed = false;
        if self.sub_tick >= time {
            self.sub_tick = 0;
            self.step = (self.step + 1) % self.schedule.len();
            changed = self.schedule[self.step].0 != frame;
        }
        self.interpolate || changed
    }

    /// `drawToAtlas`: the levels showing now. Interpolated frames blend into
    /// the next one by the frame's progress in thousandths, rounded to the
    /// nearest byte as the GPU's `mix` writes it.
    fn shown(&self) -> Vec<RgbaImage> {
        let (frame, time) = self.schedule[self.step];
        let current = &self.levels[frame];
        if !self.interpolate {
            return current.clone();
        }
        let next = &self.levels[self.schedule[(self.step + 1) % self.schedule.len()].0];
        let progress = (self.sub_tick as f32 / time as f32 * 1000.0) as u16;
        current
            .iter()
            .zip(next)
            .map(|(a, b)| {
                let mut out = a.clone();
                crate::math::slices::lerp_u8(a.as_raw(), b.as_raw(), progress, 1000, &mut out);
                out
            })
            .collect()
    }
}

/// Every animated sprite of one atlas.
#[derive(Default)]
pub struct TextureAnimator {
    pub sprites: Vec<SpriteAnimation>,
}

impl TextureAnimator {
    /// `ticks` client ticks on: `(x, y, levels)` for every sprite to draw again.
    pub fn advance(&mut self, ticks: u32) -> Vec<(u32, u32, Vec<RgbaImage>)> {
        self.sprites
            .iter_mut()
            .filter_map(|sprite| {
                let draw = (0..ticks).fold(false, |draw, _| sprite.tick() | draw);
                draw.then(|| (sprite.x, sprite.y, sprite.shown()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: u8) -> RgbaImage {
        RgbaImage::from_pixel(2, 2, image::Rgba([value, value, value, 255]))
    }

    /// A sprite at the origin showing `values` as 2x2 frames, level 0 only.
    fn sprite(values: &[u8], schedule: Vec<(usize, u32)>, interpolate: bool) -> SpriteAnimation {
        let images: Vec<RgbaImage> = values.iter().map(|&v| frame(v)).collect();
        let levels = images.iter().map(|f| vec![f.clone()]).collect();
        let frames = Frames {
            strip: RgbaImage::new(2, 2 * values.len() as u32),
            images,
            schedule,
            interpolate,
        };
        SpriteAnimation::new(0, 0, frames, levels)
    }

    /// The red of the first pixel drawn after `ticks` more ticks, if any is.
    fn red(animator: &mut TextureAnimator, ticks: u32) -> Option<u8> {
        let mut drawn = animator.advance(ticks);
        drawn
            .pop()
            .map(|(_, _, levels)| levels[0].get_pixel(0, 0).0[0])
    }

    #[test]
    fn steps_through_the_schedule_and_blends() {
        let mut held = TextureAnimator {
            sprites: vec![sprite(&[0, 100], vec![(0, 2), (1, 2)], false)],
        };
        assert_eq!(red(&mut held, 1), None);
        assert_eq!(red(&mut held, 1), Some(100));
        assert_eq!(red(&mut held, 1), None);
        assert_eq!(red(&mut held, 1), Some(0));
        assert_eq!(red(&mut held, 4), Some(0));

        let mut blended = TextureAnimator {
            sprites: vec![sprite(&[0, 100], vec![(0, 4), (1, 4)], true)],
        };
        assert_eq!(red(&mut blended, 1), Some(25));
        assert_eq!(red(&mut blended, 1), Some(50));
        assert_eq!(red(&mut blended, 2), Some(100));
    }

    /// A third of the way from 0 to 255 is 84.9: rounded, not cut off.
    #[test]
    fn blends_round_to_the_nearest_byte() {
        let mut blended = TextureAnimator {
            sprites: vec![sprite(&[0, 255], vec![(0, 3), (1, 3)], true)],
        };
        assert_eq!(red(&mut blended, 1), Some(85));
    }

    /// Every frame is cut from the strip's levels, each level half the last.
    #[test]
    fn frames_are_cut_from_the_strips_levels() {
        let strip =
            RgbaImage::from_fn(16, 64, |_, y| image::Rgba([(y / 16 * 60) as u8, 0, 0, 255]));
        let frames = frames_of(&strip, &serde_json::json!({})).expect("four frames");
        let levels = frames.mipmapped();
        assert_eq!(levels.len(), 4);
        for (i, frame) in levels.iter().enumerate() {
            assert_eq!(frame.len(), 1 + mipmap::LEVELS as usize);
            for (level, image) in frame.iter().enumerate() {
                assert_eq!(image.dimensions(), (16 >> level, 16 >> level));
                assert_eq!(
                    image.get_pixel(0, 0).0,
                    [(i * 60) as u8, 0, 0, 255],
                    "frame {i} level {level}"
                );
            }
        }
    }

    #[test]
    fn reads_waters_strip() {
        let root = crate::assets::default_root();
        if !root
            .join("minecraft/textures/block/water_still.png")
            .exists()
        {
            return;
        }
        let Frames {
            images: frames,
            schedule,
            ..
        } = load_frames(&root, "minecraft:block/water_still").expect("animated");
        assert_eq!(frames[0].dimensions(), (16, 16));
        assert_eq!(schedule.len(), frames.len());
        assert!(schedule.iter().all(|&(_, t)| t == 2));
    }
}
