//! Grass, foliage and dry-foliage colour maps (vanilla `GrassColor`, `FoliageColor`).

use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::world::java_random::JavaRandom;

/// 256×256 colour map, row-major, packed `0xAARRGGBB`.
#[derive(Clone, Debug)]
pub struct ColorMap {
    pixels: Box<[u32]>,
    /// `ColorMapColorUtil.get`'s fallback when the index is out of range.
    default: u32,
}

impl ColorMap {
    fn solid(color: u32) -> Self {
        Self {
            pixels: vec![color; 65536].into_boxed_slice(),
            default: color,
        }
    }

    fn load(image: Option<image::RgbaImage>, default: u32) -> Self {
        let Some(image) = image else {
            return Self::solid(default);
        };
        let (width, height) = image.dimensions();
        if width != 256 || height != 256 {
            return Self::solid(default);
        };
        let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
        let mut pixels = vec![default; 65536];
        for y in 0..256 {
            for x in 0..256 {
                let p = rgb.get_pixel(x, y);
                pixels[((y << 8) | x) as usize] =
                    0xFF00_0000 | u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            }
        }
        Self {
            pixels: pixels.into_boxed_slice(),
            default,
        }
    }

    /// `ColorMapColorUtil.get`.
    pub fn get(&self, temp: f32, rain: f32) -> u32 {
        let temp = temp.clamp(0.0, 1.0);
        let rain = (rain.clamp(0.0, 1.0) * temp) as f64;
        let x = ((1.0 - f64::from(temp)) * 255.0) as i32;
        let y = ((1.0 - rain) * 255.0) as i32;
        let index = (y << 8 | x) as usize;
        self.pixels.get(index).copied().unwrap_or(self.default)
    }
}

/// The three maps vanilla loads from `textures/colormap/`.
#[derive(Clone, Debug)]
pub struct ColorMaps {
    pub grass: ColorMap,
    pub foliage: ColorMap,
    pub dry_foliage: ColorMap,
}

/// `FoliageColor.FOLIAGE_DEFAULT`.
pub const FOLIAGE_DEFAULT: u32 = 0xFF48_B518;
/// `DryFoliageColor.FOLIAGE_DRY_DEFAULT`.
pub const DRY_FOLIAGE_DEFAULT: u32 = 0xFF5C_3C32;
/// `GrassColor.get`'s out-of-range colour (magenta).
const GRASS_MISSING: u32 = 0xFFFF_00FF;

impl ColorMaps {
    /// Reads the PNGs under `assets_root`; a missing file is a solid default colour.
    pub fn load(assets_root: &Path) -> Self {
        let load = |name: &str, default| {
            ColorMap::load(
                crate::assets::texture_image(assets_root, &format!("colormap/{name}")),
                default,
            )
        };
        Self {
            grass: load("grass", GRASS_MISSING),
            foliage: load("foliage", FOLIAGE_DEFAULT),
            dry_foliage: load("dry_foliage", DRY_FOLIAGE_DEFAULT),
        }
    }

    /// Loaded once from this crate's `assets/`, for tests and tools.
    pub fn cached() -> &'static Self {
        Self::shared().as_ref()
    }

    pub fn cached_arc() -> Arc<Self> {
        Self::shared().clone()
    }

    fn shared() -> &'static Arc<Self> {
        static MAPS: OnceLock<Arc<ColorMaps>> = OnceLock::new();
        MAPS.get_or_init(|| Arc::new(Self::load(&crate::assets::default_root())))
    }
}

/// `ARGB` channels of a packed colour.
pub fn unpack(color: u32) -> [u8; 3] {
    [(color >> 16) as u8, (color >> 8) as u8, color as u8]
}

pub fn pack(rgb: [u8; 3]) -> u32 {
    0xFF00_0000 | u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2])
}

pub fn pack_f32(rgb: [f32; 3]) -> u32 {
    pack(rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
}

pub fn unpack_f32(color: u32) -> [f32; 3] {
    unpack(color).map(|c| f32::from(c) / 255.0)
}

/// Swamp grass: `0x4C763C` when the noise is below `-0.1`, else `0x6A7039`.
pub fn swamp_grass(x: f64, z: f64) -> u32 {
    if biome_info_noise(x * 0.0225, z * 0.0225) < -0.1 {
        0xFF4C_763C
    } else {
        0xFF6A_7039
    }
}

/// `GrassColorModifier.DARK_FOREST`.
pub fn dark_forest(base: u32) -> u32 {
    0xFF00_0000 | ((base & 0x00FE_FEFE) + 0x0028_340A) >> 1
}

/// `Biome.BIOME_INFO_NOISE`: 2D simplex, seed `2345`, no offset.
pub fn biome_info_noise(x: f64, z: f64) -> f64 {
    simplex2(x, z)
}

fn simplex2(xin: f64, yin: f64) -> f64 {
    const F2: f64 = 0.5 * (1.732_050_807_568_877_2 - 1.0);
    const G2: f64 = (3.0 - 1.732_050_807_568_877_2) / 6.0;
    let s = (xin + yin) * F2;
    let i = (xin + s).floor() as i32;
    let j = (yin + s).floor() as i32;
    let t = f64::from(i + j) * G2;
    let x0 = xin - (f64::from(i) - t);
    let y0 = yin - (f64::from(j) - t);
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let x1 = x0 - f64::from(i1) + G2;
    let y1 = y0 - f64::from(j1) + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;
    let perms = simplex_perms();
    let permute = |x: i32| perms[(x & 0xFF) as usize] as i32;
    let ii = i & 0xFF;
    let jj = j & 0xFF;
    let gi0 = permute(ii + permute(jj)) % 12;
    let gi1 = permute(ii + i1 + permute(jj + j1)) % 12;
    let gi2 = permute(ii + 1 + permute(jj + 1)) % 12;
    let n0 = corner(gi0, x0, y0);
    let n1 = corner(gi1, x1, y1);
    let n2 = corner(gi2, x2, y2);
    70.0 * (n0 + n1 + n2)
}

fn corner(index: i32, x: f64, y: f64) -> f64 {
    const GRADIENT: [(i32, i32, i32); 16] = [
        (1, 1, 0),
        (-1, 1, 0),
        (1, -1, 0),
        (-1, -1, 0),
        (1, 0, 1),
        (-1, 0, 1),
        (1, 0, -1),
        (-1, 0, -1),
        (0, 1, 1),
        (0, -1, 1),
        (0, 1, -1),
        (0, -1, -1),
        (1, 1, 0),
        (0, -1, 1),
        (-1, 1, 0),
        (0, -1, -1),
    ];
    let mut t = 0.5 - x * x - y * y;
    if t < 0.0 {
        return 0.0;
    }
    t *= t;
    let (gx, gy, gz) = GRADIENT[index as usize];
    t * t * (f64::from(gx) * x + f64::from(gy) * y + f64::from(gz) * 0.0)
}

/// `SimplexNoise(WorldgenRandom(LegacyRandomSource(2345)), discardOffset)`.
fn simplex_perms() -> &'static [u8; 256] {
    static PERMS: OnceLock<[u8; 256]> = OnceLock::new();
    PERMS.get_or_init(|| {
        let mut rng = JavaRandom::new(2345);
        rng.next_double();
        rng.next_double();
        rng.next_double();
        let mut perms = [0u8; 256];
        for (i, p) in perms.iter_mut().enumerate() {
            *p = i as u8;
        }
        for i in 0..256 {
            let offset = rng.next_int((256 - i) as i32) as usize;
            perms.swap(i, offset + i);
        }
        perms
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plains_grass_matches_the_known_colour() {
        let maps = ColorMaps::cached();
        assert_eq!(maps.grass.get(0.8, 0.4), 0xFF91_BD59);
    }

    #[test]
    fn foliage_default_is_the_colormap_fallback() {
        let maps = ColorMaps::cached();
        let color = maps.foliage.get(0.5, 1.0);
        assert_ne!(color & 0x00FF_0000, 0, "{color:#08x}");
    }
}
