//! Packing every block texture into one atlas (vanilla `Stitcher`).

use std::collections::HashMap;
use std::path::Path;

use image::RgbaImage;

use super::mipmap;
use crate::texture_animation::{self, Frames, SpriteAnimation, TextureAnimator};
use crate::transparency::AlphaMap;

/// The packed atlas.
pub(super) struct PackedAtlas {
    pub(super) image: RgbaImage,
    /// [`mipmap::LEVELS`] smaller levels of `image`.
    pub(super) mips: Vec<RgbaImage>,
    pub(super) rects: Vec<[f32; 4]>,
    /// Each texture's size in the atlas, pixels.
    pub(super) sizes: Vec<(u32, u32)>,
    /// Per texture, its texels' transparency, as loaded (before any
    /// halving to fit).
    pub(super) alpha: Vec<AlphaMap>,
    pub(super) animator: TextureAnimator,
}

impl PackedAtlas {
    pub(super) fn longest_side(&self) -> u32 {
        self.image.width().max(self.image.height())
    }
}

/// `Transparency.TRANSPARENT`: some pixel fully clear.
pub(super) fn has_holes(image: &RgbaImage) -> bool {
    image.pixels().any(|p| p.0[3] == 0)
}

/// The biggest atlas side Melon makes even when the GPU allows more.
const MAX_SIDE: u32 = 16384;

/// One texture as loaded: its (first) frame, and all frames if animated.
struct Sprite {
    image: RgbaImage,
    frames: Option<Frames>,
    /// An animation's levels per frame ([`Frames::mipmapped`]), once fitted.
    levels: Vec<Vec<RgbaImage>>,
}

/// Packs the textures into one atlas no wider or taller than `max_side`
pub(super) fn pack_atlas(
    assets_root: &Path,
    textures: Vec<(String, Option<RgbaImage>)>,
    max_side: u32,
) -> PackedAtlas {
    let max_side = max_side.clamp(256, MAX_SIDE) / mipmap::ALIGN * mipmap::ALIGN;
    let mut sprites: Vec<Sprite> =
        parallel_map(textures, |(id, image)| load(assets_root, id, image));
    let alpha: Vec<AlphaMap> = sprites
        .iter()
        .map(|s| match &s.frames {
            Some(frames) => AlphaMap::of_frames(frames),
            None => AlphaMap::of(&s.image),
        })
        .collect();
    fit(&mut sprites, max_side);
    // `generateMipLevels`: a cutout sprite's clear pixels take their
    // neighbours' colour; an animation is mipmapped as its whole strip.
    let sprites: Vec<Sprite> = parallel_map(sprites, |mut sprite| {
        if let Some(frames) = &sprite.frames {
            sprite.levels = frames.mipmapped();
            sprite.image = sprite.levels[frames.first_index()][0].clone();
        } else if has_holes(&sprite.image) {
            mipmap::solidify(&mut sprite.image);
        }
        sprite
    });

    let mut unique: Vec<usize> = Vec::new();
    let mut slot_of = vec![0usize; sprites.len()];
    let mut by_pixels: HashMap<(u32, u32, &[u8]), usize> = HashMap::new();
    for (i, sprite) in sprites.iter().enumerate() {
        let key = (
            sprite.image.width(),
            sprite.image.height(),
            sprite.image.as_raw().as_slice(),
        );
        let slot = if sprite.frames.is_none() {
            *by_pixels.entry(key).or_insert_with(|| {
                unique.push(i);
                unique.len() - 1
            })
        } else {
            unique.push(i);
            unique.len() - 1
        };
        slot_of[i] = slot;
    }

    let cells: Vec<(u32, u32)> = unique
        .iter()
        .map(|&i| {
            let (w, h) = sprites[i].image.dimensions();
            (cell(w), cell(h))
        })
        .collect();
    let (width, height, places) = pack(&cells, max_side);

    let chains: Vec<Vec<RgbaImage>> = parallel_map(unique.clone(), |i| {
        let sprite = &sprites[i];
        match &sprite.frames {
            Some(frames) => sprite.levels[frames.first_index()][1..].to_vec(),
            None => mipmap::chain(&sprite.image, has_holes(&sprite.image)),
        }
    });

    let mut atlas = RgbaImage::new(width, height);
    let mut mips: Vec<RgbaImage> = (1..=mipmap::LEVELS)
        .map(|level| RgbaImage::new(width >> level, height >> level))
        .collect();
    for ((&i, &(x, y)), chain) in unique.iter().zip(&places).zip(chains) {
        image::imageops::replace(&mut atlas, &sprites[i].image, i64::from(x), i64::from(y));
        for (level, (mip, small)) in mips.iter_mut().zip(chain).enumerate() {
            let shift = level + 1;
            image::imageops::replace(mip, &small, i64::from(x >> shift), i64::from(y >> shift));
        }
    }

    let mut rects = Vec::with_capacity(sprites.len());
    let mut sizes = Vec::with_capacity(sprites.len());
    for (sprite, &slot) in sprites.iter().zip(&slot_of) {
        let (x, y) = places[slot];
        let (w, h) = sprite.image.dimensions();
        rects.push([
            x as f32 / width as f32,
            y as f32 / height as f32,
            (x + w) as f32 / width as f32,
            (y + h) as f32 / height as f32,
        ]);
        sizes.push((w, h));
    }
    let animator = TextureAnimator {
        sprites: sprites
            .into_iter()
            .zip(&slot_of)
            .filter_map(|(sprite, &slot)| {
                let (x, y) = places[slot];
                Some(SpriteAnimation::new(x, y, sprite.frames?, sprite.levels))
            })
            .collect(),
    };
    PackedAtlas {
        image: atlas,
        mips,
        rects,
        sizes,
        alpha,
        animator,
    }
}

/// A sprite's cell side: rounded up to [`mipmap::ALIGN`].
fn cell(n: u32) -> u32 {
    n.max(1).div_ceil(mipmap::ALIGN) * mipmap::ALIGN
}

/// A texture handed in, else its animation's frames, else its PNG.
fn load(assets_root: &Path, id: String, image: Option<RgbaImage>) -> Sprite {
    if let Some(image) = image {
        return Sprite {
            image,
            frames: None,
            levels: Vec::new(),
        };
    }
    if let Some(frames) = texture_animation::load_frames(assets_root, &id) {
        return Sprite {
            image: frames.first().clone(),
            frames: Some(frames),
            levels: Vec::new(),
        };
    }
    let image = melon_assets::assets::texture_image(assets_root, &id).map(|image| {
        let side = image.width();
        if image.height() > side {
            image::imageops::crop_imm(&image, 0, 0, side, side).to_image()
        } else {
            image
        }
    });
    Sprite {
        image: image.unwrap_or_else(melon_assets::assets::missing_texture),
        frames: None,
        levels: Vec::new(),
    }
}

/// Halves the biggest sprites (and their frames) until all of them fit an atlas of `max_side`.
fn fit(sprites: &mut [Sprite], max_side: u32) {
    let budget = u64::from(max_side) * u64::from(max_side) * 7 / 8;
    let area = |sprites: &[Sprite], cap: u32| -> u64 {
        sprites
            .iter()
            .map(|s| {
                let (w, h) = scaled_size(s.image.dimensions(), cap);
                u64::from(cell(w)) * u64::from(cell(h))
            })
            .sum()
    };
    let largest = sprites
        .iter()
        .map(|s| s.image.width().max(s.image.height()))
        .max()
        .unwrap_or(0);
    let mut cap = largest.max(1).next_power_of_two();
    while cap > max_side || (cap > mipmap::ALIGN && area(sprites, cap) > budget) {
        cap /= 2;
    }
    if cap >= largest {
        return;
    }
    melon_base::warn!(
        "Block atlas: textures over {cap}px are scaled down to fit {max_side}x{max_side}"
    );
    for sprite in sprites.iter_mut() {
        while sprite.image.width().max(sprite.image.height()) > cap {
            sprite.image = mipmap::halve(&sprite.image);
            if let Some(frames) = &mut sprite.frames {
                frames.strip = mipmap::halve(&frames.strip);
                for frame in &mut frames.images {
                    *frame = mipmap::halve(frame);
                }
            }
        }
    }
}

/// `size` after halving until neither side is over `cap`.
fn scaled_size((mut w, mut h): (u32, u32), cap: u32) -> (u32, u32) {
    while w.max(h) > cap {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    (w, h)
}

/// Places `cells` (all multiples of [`mipmap::ALIGN`]) in the atlas.
fn pack(cells: &[(u32, u32)], max_side: u32) -> (u32, u32, Vec<(u32, u32)>) {
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse((cells[i].1, cells[i].0)));
    let area: u64 = cells
        .iter()
        .map(|&(w, h)| u64::from(w) * u64::from(h))
        .sum();
    let widest = cells.iter().map(|c| c.0).max().unwrap_or(mipmap::ALIGN);
    let square = ((area as f64).sqrt().ceil() as u32)
        .max(widest)
        .max(mipmap::ALIGN);
    /// Area, width, height and corners of the best packing so far.
    type Packing = (u64, u32, u32, Vec<(u32, u32)>);
    let mut best: Option<Packing> = None;
    for step in 0..8 {
        let width = cell(square + square * step / 8).min(max_side).max(widest);
        let Some((height, places)) = skyline(cells, &order, width, max_side) else {
            continue;
        };
        let height = cell(height);
        let cost = u64::from(width) * u64::from(height);
        if best.as_ref().is_none_or(|b| {
            cost < b.0 || (cost == b.0 && width.abs_diff(height) < b.1.abs_diff(b.2))
        }) {
            best = Some((cost, width, height, places));
        }
        if width == max_side {
            break;
        }
    }
    let (_, width, height, places) = best.unwrap_or_else(|| {
        let (height, places) =
            skyline(cells, &order, max_side, u32::MAX).expect("an unbounded atlas fits");
        (0, max_side, cell(height), places)
    });
    (width.max(mipmap::ALIGN), height.max(mipmap::ALIGN), places)
}

/// Bottom-left skyline packing of `cells` in `order` into `width`; `None`
fn skyline(
    cells: &[(u32, u32)],
    order: &[usize],
    width: u32,
    max_height: u32,
) -> Option<(u32, Vec<(u32, u32)>)> {
    let mut line: Vec<(u32, u32)> = vec![(0, 0)];
    let mut places = vec![(0, 0); cells.len()];
    let mut height = 0;
    for &i in order {
        let (w, h) = cells[i];
        if w > width {
            return None;
        }
        let mut best: Option<(u32, usize)> = None;
        for start in 0..line.len() {
            let x = line[start].0;
            if x + w > width {
                break;
            }
            let mut top = 0;
            for &(seg_x, seg_top) in &line[start..] {
                if seg_x >= x + w {
                    break;
                }
                top = top.max(seg_top);
            }
            if best.is_none_or(|(y, _)| top < y) {
                best = Some((top, start));
            }
        }
        let (y, start) = best?;
        let x = line[start].0;
        if y + h > max_height {
            return None;
        }
        places[i] = (x, y);
        height = height.max(y + h);
        let end = x + w;
        let resume = line.iter().rev().find(|s| s.0 <= end).map_or(0, |s| s.1);
        let mut next: Vec<(u32, u32)> = line.iter().copied().filter(|s| s.0 < x).collect();
        next.push((x, y + h));
        if end < width {
            if !line.iter().any(|s| s.0 == end) {
                next.push((end, resume));
            }
            next.extend(line.iter().copied().filter(|s| s.0 >= end));
        }
        line = next;
        line.dedup_by(|b, a| a.1 == b.1);
    }
    Some((height, places))
}

/// `f` over `items` on every core, results in order.
pub(super) fn parallel_map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(items.len().max(1));
    if threads <= 1 {
        return items.into_iter().map(f).collect();
    }
    let jobs: Vec<std::sync::Mutex<Option<T>>> = items
        .into_iter()
        .map(|t| std::sync::Mutex::new(Some(t)))
        .collect();
    let results: Vec<std::sync::Mutex<Option<R>>> =
        jobs.iter().map(|_| std::sync::Mutex::new(None)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else {
                        break;
                    };
                    let Some(item) = job.lock().ok().and_then(|mut j| j.take()) else {
                        continue;
                    };
                    let result = f(item);
                    if let Ok(mut slot) = results[i].lock() {
                        *slot = Some(result);
                    }
                }
            });
        }
    });
    results
        .into_iter()
        .map(|r| r.into_inner().ok().flatten().expect("every job ran"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    #[test]
    fn packs_without_overlap_and_tightly() {
        let mut cells = vec![(16, 16); 900];
        cells.extend(vec![(32, 32); 40]);
        cells.extend(vec![(64, 64); 12]);
        cells.extend(vec![(16, 32); 30]);
        let (w, h, places) = pack(&cells, 16384);
        let rects: Vec<_> = cells
            .iter()
            .zip(&places)
            .map(|(&(cw, ch), &(x, y))| (x, y, cw, ch))
            .collect();
        for (i, &a) in rects.iter().enumerate() {
            assert!(a.0 + a.2 <= w && a.1 + a.3 <= h, "{a:?} outside {w}x{h}");
            for &b in &rects[i + 1..] {
                assert!(!overlaps(a, b), "{a:?} overlaps {b:?}");
            }
        }
        let used: u32 = cells.iter().map(|&(w, h)| w * h).sum();
        assert!(w % mipmap::ALIGN == 0 && h % mipmap::ALIGN == 0);
        assert!(
            f64::from(used) / f64::from(w * h) > 0.9,
            "{w}x{h} for {used}"
        );
    }

    #[test]
    fn mixed_sizes_never_overlap() {
        let mut cells = Vec::new();
        let mut seed = 7u32;
        for _ in 0..400 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let w = cell(1 + (seed >> 16) % 100);
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let h = cell(1 + (seed >> 16) % 100);
            cells.push((w, h));
        }
        let (w, h, places) = pack(&cells, 4096);
        for (i, (&(aw, ah), &(ax, ay))) in cells.iter().zip(&places).enumerate() {
            assert!(ax + aw <= w && ay + ah <= h);
            for (&(bw, bh), &(bx, by)) in cells.iter().zip(&places).skip(i + 1) {
                assert!(!overlaps((ax, ay, aw, ah), (bx, by, bw, bh)));
            }
        }
    }

    #[test]
    fn shrinks_hd_sprites_to_fit() {
        let mut sprites: Vec<Sprite> = (0..20)
            .map(|_| Sprite {
                image: RgbaImage::new(512, 512),
                frames: None,
                levels: Vec::new(),
            })
            .collect();
        fit(&mut sprites, 1024);
        let area: u64 = sprites
            .iter()
            .map(|s| u64::from(s.image.width()) * u64::from(s.image.height()))
            .sum();
        assert!(area <= 1024 * 1024, "{area}");
        assert!(sprites.iter().all(|s| s.image.width() == 128));
    }

    #[test]
    fn parallel_map_keeps_order() {
        let out = parallel_map((0..1000).collect(), |i: u32| i * 2);
        assert_eq!(out, (0..1000).map(|i| i * 2).collect::<Vec<_>>());
    }
}
