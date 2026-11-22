//! 8×8 ASCII + Latin-1 atlas for the launcher.

use crate::gui::text::font::{Font, Glyph, GlyphKind};
use image::RgbaImage;
use melon_base::gfx::ascii8::{ATLAS, CELL, COLS, glyph_width, rows_for};

pub use melon_base::gfx::ascii8::atlas;

pub fn font(page: RgbaImage) -> Font {
    let mut glyphs = crate::gui::text::font::GlyphMap::default();
    glyphs.insert(' ', Glyph::new(GlyphKind::Space, 4.0));
    for code in 0u32..256 {
        if code == b' ' as u32 {
            continue;
        }
        let Some(ch) = char::from_u32(code) else {
            continue;
        };
        let Some(rows) = rows_for(code) else {
            continue;
        };
        let width = glyph_width(rows).max(1);
        let col = code % COLS;
        let row = code / COLS;
        let u0 = (col * CELL) as f32 / ATLAS as f32;
        let v0 = (row * CELL) as f32 / ATLAS as f32;
        let u1 = u0 + width as f32 / ATLAS as f32;
        let v1 = v0 + 8.0 / ATLAS as f32;
        glyphs.insert(
            ch,
            Glyph::new(
                GlyphKind::Bitmap {
                    page: 0,
                    u0,
                    v0,
                    u1,
                    v1,
                    width: width as f32,
                    height: 8.0,
                    ascent: 7,
                },
                width as f32 + 1.0,
            ),
        );
    }
    Font::from_bitmaps(glyphs, vec![page])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_and_letter_a_have_glyphs() {
        let font = font(atlas());
        assert!(font.advance(' ') > 0.0);
        assert!(font.advance('A') >= 5.0);
        assert!(font.advance('ü') >= 5.0);
        assert!(font.advance('~') > 0.0);
    }

    #[test]
    fn letter_a_has_a_crossbar() {
        assert_eq!(melon_base::gfx::ascii8::BASIC[b'A' as usize - 32][4], 0x3F);
        let img = atlas();
        let col = b'A' as u32 % COLS;
        let row = b'A' as u32 / COLS;
        let lit = (0..8)
            .filter(|&x| img.get_pixel(col * CELL + x, row * CELL + 4)[3] != 0)
            .count();
        assert!(lit >= 5, "A crossbar should span the glyph, got {lit} px");
    }
}
