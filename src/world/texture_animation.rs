//! Animated atlas sprites (vanilla `SpriteContents.AnimatedTexture`).

use std::path::Path;

use image::RgbaImage;
use serde_json::Value;

/// An animated texture's frames and schedule (`(frame, ticks)` in play order).
pub struct Frames {
    pub images: Vec<RgbaImage>,
    pub schedule: Vec<(usize, u32)>,
    pub interpolate: bool,
}

impl Frames {
    /// The frame shown first, as packed into the atlas.
    pub fn first(&self) -> &RgbaImage {
        &self.images[self.schedule.first().map_or(0, |&(f, _)| f)]
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
    frames: Vec<RgbaImage>,
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
    pub fn new(x: u32, y: u32, frames: Frames) -> Self {
        Self {
            x,
            y,
            frames: frames.images,
            schedule: frames.schedule,
            interpolate: frames.interpolate,
            step: 0,
            sub_tick: 0,
        }
    }

    /// `AnimatedTexture.tick`: one client tick on.
    fn tick(&mut self) -> Option<RgbaImage> {
        if self.schedule.is_empty() {
            return None;
        }
        self.sub_tick += 1;
        let (frame, time) = self.schedule[self.step];
        if self.sub_tick >= time {
            self.sub_tick = 0;
            self.step = (self.step + 1) % self.schedule.len();
            let (next, _) = self.schedule[self.step];
            return (next != frame || self.interpolate).then(|| self.frames[next].clone());
        }
        if !self.interpolate {
            return None;
        }
        let (next, _) = self.schedule[(self.step + 1) % self.schedule.len()];
        let ratio = self.sub_tick as f32 / time as f32;
        let (a, b) = (&self.frames[frame], &self.frames[next]);
        let mut out = a.clone();
        for (pixel, (pa, pb)) in out.pixels_mut().zip(a.pixels().zip(b.pixels())) {
            for c in 0..4 {
                pixel.0[c] =
                    (f32::from(pa.0[c]) * (1.0 - ratio) + f32::from(pb.0[c]) * ratio) as u8;
            }
        }
        Some(out)
    }
}

/// Every animated sprite of one atlas.
#[derive(Default)]
pub struct TextureAnimator {
    pub sprites: Vec<SpriteAnimation>,
}

impl TextureAnimator {
    /// One client tick: `(x, y, image)` for every sprite region to rewrite.
    pub fn tick(&mut self) -> Vec<(u32, u32, RgbaImage)> {
        self.sprites
            .iter_mut()
            .filter_map(|sprite| Some((sprite.x, sprite.y, sprite.tick()?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: u8) -> RgbaImage {
        RgbaImage::from_pixel(2, 2, image::Rgba([value, value, value, 255]))
    }

    #[test]
    fn steps_through_the_schedule_and_blends() {
        let mut held = SpriteAnimation::new(
            0,
            0,
            Frames {
                images: vec![frame(0), frame(100)],
                schedule: vec![(0, 2), (1, 2)],
                interpolate: false,
            },
        );
        assert!(held.tick().is_none());
        assert_eq!(held.tick().unwrap().get_pixel(0, 0).0[0], 100);
        assert!(held.tick().is_none());
        assert_eq!(held.tick().unwrap().get_pixel(0, 0).0[0], 0);

        let mut blended = SpriteAnimation::new(
            0,
            0,
            Frames {
                images: vec![frame(0), frame(100)],
                schedule: vec![(0, 4), (1, 4)],
                interpolate: true,
            },
        );
        assert_eq!(blended.tick().unwrap().get_pixel(0, 0).0[0], 25);
        assert_eq!(blended.tick().unwrap().get_pixel(0, 0).0[0], 50);
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
