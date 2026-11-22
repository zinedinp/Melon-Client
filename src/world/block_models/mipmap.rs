//! Mipmaps for the block atlas (vanilla `MipmapGenerator`).

use image::RgbaImage;

/// Levels below the full-size image (vanilla's default `mipmapLevels`).
pub const LEVELS: u32 = 4;
/// Sprites start on multiples of this, so level [`LEVELS`] still has whole texels.
pub const ALIGN: u32 = 1 << LEVELS;
/// The cutout shader's alpha cutoff (0.1), as a byte.
const CUTOFF: u8 = 26;

/// `sprite`'s levels 1 to [`LEVELS`], each half the size of the one before.
pub fn chain(sprite: &RgbaImage, holes: bool) -> Vec<RgbaImage> {
    let target = holes.then(|| coverage(sprite, 1.0));
    let mut levels = Vec::with_capacity(LEVELS as usize);
    let mut previous = sprite.clone();
    for _ in 0..LEVELS {
        let next = halve(&previous);
        let shown = match target {
            Some(target) => keep_coverage(&next, target),
            None => next.clone(),
        };
        levels.push(shown);
        previous = next;
    }
    levels
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

/// The share of pixels at or over the cutoff once alpha is scaled by `scale`.
fn coverage(image: &RgbaImage, scale: f32) -> f32 {
    let shown = image
        .pixels()
        .filter(|p| scaled(p.0[3], scale) >= CUTOFF)
        .count();
    shown as f32 / (image.width() * image.height()) as f32
}

/// `image` with alpha scaled so its coverage is closest to `target`.
fn keep_coverage(image: &RgbaImage, target: f32) -> RgbaImage {
    let (mut low, mut high) = (0.0f32, 8.0f32);
    for _ in 0..16 {
        let mid = (low + high) / 2.0;
        if coverage(image, mid) < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    let scale = if (coverage(image, low) - target).abs() < (coverage(image, high) - target).abs() {
        low
    } else {
        high
    };
    let mut out = image.clone();
    for p in out.pixels_mut() {
        p.0[3] = scaled(p.0[3], scale);
    }
    out
}

/// `alpha` times `scale`, as stored.
fn scaled(alpha: u8, scale: f32) -> u8 {
    (f32::from(alpha) * scale).round().min(255.0) as u8
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

    #[test]
    fn clear_pixels_do_not_darken() {
        let mut sprite = RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 0, 0]));
        for y in 0..16 {
            for x in (0..16).step_by(2) {
                sprite.put_pixel(x, y, image::Rgba([40, 200, 40, 255]));
            }
        }
        let p = chain(&sprite, true)[0].get_pixel(0, 0).0;
        assert_eq!(&p[..3], &[40, 200, 40]);
    }

    #[test]
    fn cutout_keeps_its_coverage() {
        let mut sprite = RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 0, 0]));
        for y in (0..16).step_by(2) {
            for x in (0..16).step_by(2) {
                sprite.put_pixel(x, y, image::Rgba([30, 120, 30, 255]));
            }
        }
        let target = coverage(&sprite, 1.0);
        for level in chain(&sprite, true) {
            assert!(
                (coverage(&level, 1.0) - target).abs() <= 0.26,
                "{}",
                coverage(&level, 1.0)
            );
        }
    }
}
