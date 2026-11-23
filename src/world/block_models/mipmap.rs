//! Mipmaps for the block atlas (vanilla `MipmapGenerator`).

use image::RgbaImage;

/// Levels below the full-size image (vanilla's default `mipmapLevels`).
pub const LEVELS: u32 = 4;
/// Sprites start on multiples of this, so level [`LEVELS`] still has whole texels.
pub const ALIGN: u32 = 1 << LEVELS;
/// `MipmapGenerator.ALPHA_CUTOFF`: the alpha a cutout sprite's levels keep
/// the coverage at, the cutout terrain's shader cutoff.
const ALPHA_CUTOFF: f32 = 0.5;

/// `sprite`'s levels 1 to [`LEVELS`], each half the size of the one before
/// (`generateMipLevels`). With `holes` (`MipmapStrategy.CUTOUT`) each level's
/// alpha is scaled to keep the sprite's coverage; `sprite` should be
/// [`solidify`]d first. Melon averages colour by alpha where vanilla takes
/// the plain `ARGB.meanLinear`: the same once clear pixels are solidified.
pub fn chain(sprite: &RgbaImage, holes: bool) -> Vec<RgbaImage> {
    let target = holes.then(|| coverage(sprite, 1.0));
    let mut levels: Vec<RgbaImage> = Vec::with_capacity(LEVELS as usize);
    for _ in 0..LEVELS {
        let mut next = halve(levels.last().unwrap_or(sprite));
        if let Some(target) = target {
            scale_to_coverage(&mut next, target);
        }
        levels.push(next);
    }
    levels
}

/// `TextureUtil.solidify`: each clear pixel takes the colour of the nearest
/// visible one, its alpha left at 0, so the smaller levels and the blend
/// between them don't bring the clear pixels' colour (often black) into
/// the edges.
pub fn solidify(image: &mut RgbaImage) {
    let (w, h) = image.dimensions();
    let index = |x: u32, y: u32| (y * w + x) as usize;
    let mut nearest = vec![[0u8; 3]; (w * h) as usize];
    let mut distance = vec![u32::MAX; (w * h) as usize];
    let mut queue = std::collections::VecDeque::new();
    for x in 0..w {
        for y in 0..h {
            let p = image.get_pixel(x, y).0;
            if p[3] != 0 {
                distance[index(x, y)] = 0;
                nearest[index(x, y)] = [p[0], p[1], p[2]];
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let here = index(x, y);
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
            if nx < 0 || ny < 0 || nx >= i64::from(w) || ny >= i64::from(h) {
                continue;
            }
            let (nx, ny) = (nx as u32, ny as u32);
            let next = index(nx, ny);
            if distance[next] > distance[here] + 1 {
                distance[next] = distance[here] + 1;
                nearest[next] = nearest[here];
                queue.push_back((nx, ny));
            }
        }
    }
    for (x, y, p) in image.enumerate_pixels_mut() {
        if p.0[3] == 0 {
            let [r, g, b] = nearest[index(x, y)];
            p.0 = [r, g, b, 0];
        }
    }
}

/// Each 2x2 block averaged.
pub(super) fn halve(image: &RgbaImage) -> RgbaImage {
    let (w, h) = image.dimensions();
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let lut = linear_lut();
    RgbaImage::from_fn(nw, nh, |x, y| {
        let mut sum = [0.0f32; 3];
        let mut plain = [0.0f32; 3];
        let mut alpha = 0u32;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let p = image
                .get_pixel((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1))
                .0;
            let a = f32::from(p[3]) / 255.0;
            for c in 0..3 {
                let linear = lut[usize::from(p[c])];
                sum[c] += linear * a;
                plain[c] += linear;
            }
            alpha += u32::from(p[3]);
        }
        let weight = alpha as f32 / 255.0;
        let color = |c: usize| {
            let linear = if weight > 0.0 {
                sum[c] / weight
            } else {
                plain[c] / 4.0
            };
            (linear.powf(1.0 / 2.2) * 255.0).round() as u8
        };
        image::Rgba([color(0), color(1), color(2), ((alpha + 2) / 4) as u8])
    })
}

/// Byte to linear light.
fn linear_lut() -> &'static [f32; 256] {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| std::array::from_fn(|i| (i as f32 / 255.0).powf(2.2)))
}

/// `alphaTestCoverage`: the share of bilinear samples (4x4 per texel) over
/// [`ALPHA_CUTOFF`] once alpha is scaled by `scale`. NaN for an image one
/// pixel wide or high, as vanilla's 0/0.
fn coverage(image: &RgbaImage, scale: f32) -> f32 {
    let (w, h) = image.dimensions();
    let alpha = |x: u32, y: u32| (f32::from(image.get_pixel(x, y).0[3]) / 255.0 * scale).clamp(0.0, 1.0);
    let mut total = 0.0f32;
    for y in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let (a00, a10, a01, a11) = (alpha(x, y), alpha(x + 1, y), alpha(x, y + 1), alpha(x + 1, y + 1));
            let mut texel = 0.0f32;
            for sy in 0..4 {
                let fy = (sy as f32 + 0.5) / 4.0;
                for sx in 0..4 {
                    let fx = (sx as f32 + 0.5) / 4.0;
                    let a = a00 * (1.0 - fx) * (1.0 - fy)
                        + a10 * fx * (1.0 - fy)
                        + a01 * (1.0 - fx) * fy
                        + a11 * fx * fy;
                    if a > ALPHA_CUTOFF {
                        texel += 1.0;
                    }
                }
            }
            total += texel / 16.0;
        }
    }
    total / (w.saturating_sub(1) * h.saturating_sub(1)) as f32
}

/// `scaleAlphaToCoverage` (no `alpha_cutoff_bias`): five halving steps for
/// the alpha scale closest to `desired`, then every alpha scaled and raised by 0.025.
fn scale_to_coverage(image: &mut RgbaImage, desired: f32) {
    let (mut min, mut max) = (0.0f32, 4.0f32);
    let (mut scale, mut best_scale, mut best_error) = (1.0f32, 1.0f32, f32::MAX);
    for _ in 0..5 {
        let current = coverage(image, scale);
        let error = (current - desired).abs();
        if error < best_error {
            best_error = error;
            best_scale = scale;
        }
        if current < desired {
            min = scale;
        } else if current > desired {
            max = scale;
        } else {
            break;
        }
        scale = (min + max) * 0.5;
    }
    for p in image.pixels_mut() {
        let alpha = (f32::from(p.0[3]) / 255.0 * best_scale + 0.025).clamp(0.0, 1.0);
        p.0[3] = (alpha * 255.0).floor() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_sprite_halves_to_its_average() {
        let mut sprite = RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]));
        for y in 0..16 {
            for x in (0..16).step_by(2) {
                sprite.put_pixel(x, y, image::Rgba([0, 0, 255, 255]));
            }
        }
        let levels = chain(&sprite, false);
        assert_eq!(levels.len(), LEVELS as usize);
        assert_eq!(levels[0].dimensions(), (8, 8));
        assert_eq!(levels[3].dimensions(), (1, 1));
        let p = levels[0].get_pixel(3, 3).0;
        assert!(p[0] > 128 && p[2] > 128 && p[3] == 255, "{p:?}");
    }

    /// A 16x16 cutout: a green checker of 2x2 blocks on clear black.
    fn checker() -> RgbaImage {
        RgbaImage::from_fn(16, 16, |x, y| {
            if (x / 2 + y / 2) % 2 == 0 {
                image::Rgba([40, 200, 40, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        })
    }

    #[test]
    fn solidify_fills_clear_pixels_with_the_nearest_colour() {
        let mut sprite = checker();
        solidify(&mut sprite);
        for p in sprite.pixels() {
            assert_eq!(&p.0[..3], &[40, 200, 40]);
        }
        assert_eq!(sprite.get_pixel(2, 0).0[3], 0);
    }

    #[test]
    fn clear_pixels_do_not_darken() {
        let mut sprite = checker();
        solidify(&mut sprite);
        for level in chain(&sprite, true) {
            for p in level.pixels() {
                assert_eq!(&p.0[..3], &[40, 200, 40], "{p:?}");
            }
        }
    }

    #[test]
    fn cutout_keeps_its_coverage() {
        let sprite = checker();
        let target = coverage(&sprite, 1.0);
        for level in chain(&sprite, true).iter().take(2) {
            let shown = coverage(level, 1.0);
            assert!((shown - target).abs() <= 0.15, "{shown} vs {target}");
        }
    }
}
