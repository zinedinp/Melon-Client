//! Vanilla `Transparency` and the texels it is read from: a whole
//! texture's (`SpriteContents.transparency`) or the part of it a face
//! shows (`SpriteContents.computeTransparency`), which picks a quad's
//! chunk layer and item render type (`BakedQuad.MaterialInfo.of`).

use std::sync::Arc;

use glam::Vec2;
use image::RgbaImage;

use crate::world::texture_animation::Frames;

/// Whether some texel is fully clear and whether some is partly
/// see-through (vanilla `Transparency`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Transparency {
    /// `hasTransparent`: some alpha is 0.
    pub holes: bool,
    /// `hasTranslucent`: some alpha is neither 0 nor 255.
    pub translucent: bool,
}

impl Transparency {
    pub const NONE: Self = Self {
        holes: false,
        translucent: false,
    };
    pub const TRANSLUCENT: Self = Self {
        holes: false,
        translucent: true,
    };

    fn of_alpha(alpha: u8) -> Self {
        Self {
            holes: alpha == 0,
            translucent: alpha != 0 && alpha != 255,
        }
    }

    pub fn or(self, other: Self) -> Self {
        Self {
            holes: self.holes || other.holes,
            translucent: self.translucent || other.translucent,
        }
    }

    pub fn is_opaque(self) -> bool {
        !self.holes && !self.translucent
    }

    fn bits(self) -> u8 {
        u8::from(self.holes) | u8::from(self.translucent) << 1
    }

    fn from_bits(bits: u8) -> Self {
        Self {
            holes: bits & 1 != 0,
            translucent: bits & 2 != 0,
        }
    }
}

/// A texture's [`Transparency`], whole and per texel, so any rectangle
/// of it can be asked about. Cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct AlphaMap {
    width: u32,
    height: u32,
    /// The whole file, every frame (`SpriteContents.transparency`).
    whole: Transparency,
    /// Per texel of one frame, [`Transparency::bits`] over every frame
    /// the animation shows; `None` when the texture is opaque.
    texels: Option<Arc<[u8]>>,
}

impl AlphaMap {
    /// A still texture's.
    pub fn of(image: &RgbaImage) -> Self {
        Self::build(image.dimensions(), [image], [image])
    }

    /// An animated texture's: the whole of every frame cut from the
    /// file, a rectangle over the frames its schedule shows.
    pub fn of_frames(frames: &Frames) -> Self {
        let mut shown: Vec<usize> = frames.schedule.iter().map(|&(f, _)| f).collect();
        shown.sort_unstable();
        shown.dedup();
        let shown = shown.into_iter().filter_map(|f| frames.images.get(f));
        Self::build(frames.first().dimensions(), &frames.images, shown)
    }

    fn build<'a>(
        (width, height): (u32, u32),
        all: impl IntoIterator<Item = &'a RgbaImage>,
        shown: impl IntoIterator<Item = &'a RgbaImage>,
    ) -> Self {
        let whole = all.into_iter().fold(Transparency::NONE, |t, image| {
            image
                .pixels()
                .fold(t, |t, p| t.or(Transparency::of_alpha(p[3])))
        });
        let texels = (!whole.is_opaque()).then(|| {
            let mut texels = vec![0u8; width as usize * height as usize];
            for image in shown {
                if image.dimensions() != (width, height) {
                    continue;
                }
                for (texel, p) in texels.iter_mut().zip(image.pixels()) {
                    *texel |= Transparency::of_alpha(p[3]).bits();
                }
            }
            texels.into()
        });
        Self {
            width,
            height,
            whole,
            texels,
        }
    }

    /// The whole texture's.
    pub fn whole(&self) -> Transparency {
        self.whole
    }

    /// The texels from `min` to `max` (`0..1` in the texture) cover, the
    /// partly covered ones too.
    pub fn region(&self, min: Vec2, max: Vec2) -> Transparency {
        let Some(texels) = &self.texels else {
            return self.whole;
        };
        if min == Vec2::ZERO && max == Vec2::ONE {
            return self.whole;
        }
        let (w, h) = (self.width as f32, self.height as f32);
        let x0 = (min.x * w).floor().clamp(0.0, w) as usize;
        let y0 = (min.y * h).floor().clamp(0.0, h) as usize;
        let x1 = (max.x * w).ceil().clamp(0.0, w) as usize;
        let y1 = (max.y * h).ceil().clamp(0.0, h) as usize;
        let mut bits = 0;
        for y in y0..y1 {
            let row = y * self.width as usize;
            for &texel in &texels[row + x0..row + x1.max(x0)] {
                bits |= texel;
            }
            if bits == 3 {
                break;
            }
        }
        Transparency::from_bits(bits)
    }
}

/// What a quad's [`Transparency`] is read from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AlphaRule {
    /// The whole texture: a flat item's extruded faces
    /// (`ItemModelGenerator`), a block entity part.
    Whole,
    /// The face's `uv` rectangle, `0..1` (`FaceBakery`).
    Region { min: Vec2, max: Vec2 },
    /// The model binds the texture with `force_translucent`.
    Translucent,
}

impl AlphaRule {
    /// The rectangle `uvs` span.
    pub fn region(uvs: &[Vec2; 4]) -> Self {
        let min = uvs.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
        let max = uvs.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
        Self::Region { min, max }
    }

    pub fn judge(self, alpha: &AlphaMap) -> Transparency {
        match self {
            Self::Whole => alpha.whole(),
            Self::Region { min, max } => alpha.region(min, max),
            Self::Translucent => Transparency::TRANSLUCENT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 4×1: opaque, clear, half, opaque.
    fn strip() -> RgbaImage {
        let mut image = RgbaImage::from_pixel(4, 1, Rgba([9, 9, 9, 255]));
        image.put_pixel(1, 0, Rgba([0, 0, 0, 0]));
        image.put_pixel(2, 0, Rgba([0, 0, 0, 128]));
        image
    }

    #[test]
    fn a_region_sees_only_its_texels() {
        let alpha = AlphaMap::of(&strip());
        let at = |x0: f32, x1: f32| alpha.region(Vec2::new(x0, 0.0), Vec2::new(x1, 1.0));
        assert_eq!(
            alpha.whole(),
            Transparency {
                holes: true,
                translucent: true
            }
        );
        assert!(at(0.0, 0.25).is_opaque());
        assert!(at(0.75, 1.0).is_opaque());
        assert_eq!(
            at(0.25, 0.5),
            Transparency {
                holes: true,
                translucent: false
            }
        );
        assert_eq!(at(0.5, 0.75), Transparency::TRANSLUCENT);
        // A texel only partly covered counts (`Mth.floor` / `Mth.ceil`).
        assert_eq!(
            at(0.1, 0.3),
            Transparency {
                holes: true,
                translucent: false
            }
        );
        // No texels, nothing to see through.
        assert!(at(0.5, 0.5).is_opaque());
    }

    #[test]
    fn an_animation_counts_every_frame() {
        let opaque = RgbaImage::from_pixel(4, 1, Rgba([9, 9, 9, 255]));
        let frames = Frames {
            strip: RgbaImage::new(4, 2),
            images: vec![opaque, strip()],
            schedule: vec![(0, 5), (1, 5)],
            interpolate: false,
        };
        let alpha = AlphaMap::of_frames(&frames);
        assert!(alpha.whole().translucent);
        assert_eq!(
            alpha.region(Vec2::new(0.5, 0.0), Vec2::new(0.75, 1.0)),
            Transparency::TRANSLUCENT
        );
    }

    #[test]
    fn rules() {
        let alpha = AlphaMap::of(&strip());
        let uvs = [
            Vec2::new(0.75, 0.0),
            Vec2::new(0.75, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        ];
        assert!(AlphaRule::region(&uvs).judge(&alpha).is_opaque());
        assert!(AlphaRule::Whole.judge(&alpha).translucent);
        let opaque = AlphaMap::of(&RgbaImage::from_pixel(2, 2, Rgba([1, 1, 1, 255])));
        assert!(AlphaRule::Translucent.judge(&opaque).translucent);
    }
}
