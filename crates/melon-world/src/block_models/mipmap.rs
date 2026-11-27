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
/// [`solidify`]d first.
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

/// Each 2x2 block averaged (`ARGB.meanLinear`): colour in linear light at
/// 10 bits, alpha plain, both rounded down. An odd last row or column is
/// averaged with itself.
pub(super) fn halve(image: &RgbaImage) -> RgbaImage {
    let (w, h) = image.dimensions();
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let (to_linear, to_srgb) = srgb_luts();
    let stride = w as usize * 4;
    let src = image.as_raw();
    let row = |y: u32| &src[y.min(h - 1) as usize * stride..][..stride];
    let mut out = Vec::with_capacity(nw as usize * nh as usize * 4);
    for y in 0..nh {
        let (top, bottom) = (row(2 * y), row(2 * y + 1));
        for x in 0..nw as usize {
            let (left, right) = (8 * x, (2 * x + 1).min(w as usize - 1) * 4);
            let quad = [
                &top[left..],
                &top[right..],
                &bottom[left..],
                &bottom[right..],
            ];
            for c in 0..3 {
                let linear: u32 = quad
                    .iter()
                    .map(|p| u32::from(to_linear[usize::from(p[c])]))
                    .sum();
                out.push(to_srgb[(linear / 4) as usize]);
            }
            let alpha: u32 = quad.iter().map(|p| u32::from(p[3])).sum();
            out.push((alpha / 4) as u8);
        }
    }
    RgbaImage::from_raw(nw, nh, out).expect("nw * nh pixels")
}

/// `ARGB.SRGB_TO_LINEAR` (a byte to linear light at 10 bits) and
/// `ARGB.LINEAR_TO_SRGB`, built the same way, value for value.
fn srgb_luts() -> &'static ([u16; 256], [u8; 1024]) {
    static LUTS: std::sync::OnceLock<([u16; 256], [u8; 1024])> = std::sync::OnceLock::new();
    LUTS.get_or_init(|| {
        let to_linear = std::array::from_fn(|i| {
            let x = i as f32 / 255.0;
            let linear = if x >= 0.04045 {
                ((f64::from(x) + 0.055) / 1.055).powf(2.4) as f32
            } else {
                x / 12.92
            };
            (linear * 1023.0).round() as u16
        });
        let to_srgb = std::array::from_fn(|i| {
            let x = i as f32 / 1023.0;
            let srgb = if x >= 0.003_130_8 {
                (1.055 * f64::from(x).powf(0.416_666_666_666_666_7) - 0.055) as f32
            } else {
                12.92 * x
            };
            (srgb * 255.0).round() as u8
        });
        (to_linear, to_srgb)
    })
}

/// `alphaTestCoverage`: the share of bilinear samples (4x4 per texel) over
/// [`ALPHA_CUTOFF`] once alpha is scaled by `scale`. NaN for an image one
/// pixel wide or high, as vanilla's 0/0. A texel whose four corners share
/// one alpha has the same samples as every other such texel, so those are
/// counted once per alpha.
fn coverage(image: &RgbaImage, scale: f32) -> f32 {
    let (w, h) = image.dimensions();
    let (w, h) = (w as usize, h as usize);
    let alphas: Vec<u8> = image.pixels().map(|p| p.0[3]).collect();
    let scaled = |a: u8| (f32::from(a) / 255.0 * scale).clamp(0.0, 1.0);
    let mut uniform = [None; 256];
    let mut total = 0.0f32;
    for y in 0..h.saturating_sub(1) {
        let (top, bottom) = (&alphas[y * w..][..w], &alphas[(y + 1) * w..][..w]);
        for x in 0..w.saturating_sub(1) {
            let corners = [top[x], top[x + 1], bottom[x], bottom[x + 1]];
            let hits = if corners.iter().all(|&a| a == corners[0]) {
                *uniform[usize::from(corners[0])]
                    .get_or_insert_with(|| samples_over_cutoff([scaled(corners[0]); 4]))
            } else {
                samples_over_cutoff(corners.map(scaled))
            };
            total += f32::from(hits) / 16.0;
        }
    }
    total / (w.saturating_sub(1) * h.saturating_sub(1)) as f32
}

/// How many of a texel's 4x4 bilinear samples between its corner alphas
/// (`[a00, a10, a01, a11]`) are over [`ALPHA_CUTOFF`], in vanilla's float order.
fn samples_over_cutoff([a00, a10, a01, a11]: [f32; 4]) -> u8 {
    let mut hits = 0;
    for sy in 0..4 {
        let fy = (sy as f32 + 0.5) / 4.0;
        for sx in 0..4 {
            let fx = (sx as f32 + 0.5) / 4.0;
            let a = a00 * (1.0 - fx) * (1.0 - fy)
                + a10 * fx * (1.0 - fy)
                + a01 * (1.0 - fx) * fy
                + a11 * fx * fy;
            hits += u8::from(a > ALPHA_CUTOFF);
        }
    }
    hits
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

    /// Spot values and sums of vanilla's tables, dumped from Java.
    #[test]
    fn srgb_tables_are_vanillas() {
        let (to_linear, to_srgb) = srgb_luts();
        let weighted = |t: &[u32]| {
            t.iter()
                .enumerate()
                .map(|(i, &v)| i as u64 * u64::from(v))
                .sum::<u64>()
        };
        let to_linear: Vec<u32> = to_linear.iter().map(|&v| u32::from(v)).collect();
        let to_srgb: Vec<u32> = to_srgb.iter().map(|&v| u32::from(v)).collect();
        assert_eq!(
            (to_linear[128], to_linear[200], to_linear[255]),
            (221, 591, 1023)
        );
        assert_eq!(
            (to_srgb[1], to_srgb[100], to_srgb[512], to_srgb[1023]),
            (3, 88, 188, 255)
        );
        assert_eq!(
            (to_linear.iter().sum::<u32>(), weighted(&to_linear)),
            (81452, 15822850)
        );
        assert_eq!(
            (to_srgb.iter().sum::<u32>(), weighted(&to_srgb)),
            (180055, 109293966)
        );
    }

    /// `ARGB.meanLinear` of black, white and two greys: linear light, alpha rounded down.
    #[test]
    fn halving_is_the_linear_mean() {
        let mut quad = RgbaImage::new(2, 2);
        quad.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        quad.put_pixel(1, 0, image::Rgba([255, 255, 255, 254]));
        quad.put_pixel(0, 1, image::Rgba([128, 128, 128, 0]));
        quad.put_pixel(1, 1, image::Rgba([128, 128, 128, 0]));
        let (to_linear, to_srgb) = srgb_luts();
        let grey = to_srgb[usize::from((1023 + 2 * to_linear[128]) / 4)];
        assert_eq!(halve(&quad).get_pixel(0, 0).0, [grey, grey, grey, 127]);
    }

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

    /// Only the first level: below it the checker averages to a flat 0.5,
    /// whose coverage is all or nothing for vanilla's search too.
    #[test]
    fn cutout_keeps_its_coverage() {
        let sprite = checker();
        let target = coverage(&sprite, 1.0);
        let shown = coverage(&chain(&sprite, true)[0], 1.0);
        assert!((shown - target).abs() <= 0.15, "{shown} vs {target}");
    }
}
