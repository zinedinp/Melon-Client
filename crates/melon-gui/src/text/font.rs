//! Vanilla `BitmapProvider` / `FontManager` (`font/default.json`).

use anyhow::Context;
use image::RgbaImage;
use std::collections::HashMap;

use super::{StyledText, TextPaint};
use melon_assets::assets::Assets;
use melon_base::gfx::TextureId;
use melon_base::gfx::quad::{DrawList, Program, Transform2D};

mod load;
pub mod unihex;

use load::load_provider_file;
use std::cell::RefCell;
use std::rc::Rc;
use unihex::Lazy;

/// Standard Minecraft text line height in (unscaled) GUI pixels.
pub const LINE_HEIGHT: f32 = 9.0;

/// Baseline ascent of the `ascii`/`accented` pages.
pub const BASELINE_ASCENT: f32 = 7.0;

/// Vanilla `GlyphInfo.getShadowOffset()` default.
pub const SHADOW_OFFSET: f32 = 1.0;

/// `ARGB.scaleRGB(textColor, 0.25F)` (`Font.PreparedTextBuilder.getShadowColor`).
pub fn shadow_color(color: [f32; 4]) -> [f32; 4] {
    [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]]
}

#[derive(Clone, Copy)]
pub enum GlyphKind {
    Bitmap {
        page: usize,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        width: f32,
        height: f32,
        ascent: i32,
    },
    /// Advance-only glyph (space).
    Space,
}

#[derive(Clone, Copy)]
pub struct Glyph {
    pub kind: GlyphKind,
    pub advance: f32,
    /// `GlyphInfo.getShadowOffset` and `getBoldOffset`.
    pub shadow: f32,
    pub bold: f32,
}

impl Glyph {
    pub const fn new(kind: GlyphKind, advance: f32) -> Self {
        Self {
            kind,
            advance,
            shadow: SHADOW_OFFSET,
            bold: 1.0,
        }
    }
}

const SPRITE_ADVANCE: f32 = 8.0;

/// How big a texture of Unifont glyphs is (`FontTexture`'s 256).
pub const DYNAMIC_PAGE: u32 = 256;

/// The characters of one font by `ceil(advance)`: `FontSet.glyphsByWidth`.
type WidthPool = rustc_hash::FxHashMap<i32, Vec<char>>;

/// Glyphs made when a text first needed them, and what still has to go to the GPU.
#[derive(Default)]
struct Dynamic {
    /// The characters asked for that aren't made yet.
    wanted: Vec<char>,
    /// Made: the glyph, its page (counting after the font's own).
    baked: GlyphMap,
    /// Where the next glyph of each page goes (a shelf per row).
    pages: Vec<(u32, u32)>,
}

/// What [`Font::bake_pending`] made.
#[derive(Default)]
pub struct DynamicWork {
    pub new_pages: usize,
    pub writes: Vec<GlyphWrite>,
}

pub struct GlyphWrite {
    /// Counting from the font's first page, like [`GlyphKind::Bitmap::page`].
    pub page: usize,
    pub x: u32,
    pub y: u32,
    pub image: RgbaImage,
}

impl DynamicWork {
    pub fn is_empty(&self) -> bool {
        self.new_pages == 0 && self.writes.is_empty()
    }
}

/// A font besides the default one (`minecraft:alt`, `minecraft:uniform`, a pack's).
struct Face {
    glyphs: GlyphMap,
    unihex: bool,
}

/// Glyphs by character, hashed with [`CharHasher`]: a lookup per drawn
/// character, every frame, which SipHash made a visible cost.
pub type GlyphMap = HashMap<char, Glyph, std::hash::BuildHasherDefault<CharHasher>>;

/// A multiplicative hash of a `char` (its `u32`): one multiply. Not for
/// keys from the network; the fonts' are from the resource packs.
#[derive(Default)]
pub struct CharHasher(u64);

impl std::hash::Hasher for CharHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.0 = (self.0 ^ u64::from(n)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

pub use super::{DEFAULT_FONT, SPRITE_COUNT, SPRITE_FIRST, sprite_char, sprite_index};

pub struct Font {
    glyphs: GlyphMap,
    /// `glyphs`' ASCII part by code, for the lookup per drawn character.
    ascii: Box<[Option<Glyph>; 128]>,
    /// The other fonts by full id, for text whose `Style.font` names one.
    faces: HashMap<String, Face>,
    /// Unifont: the glyphs for everything the bitmaps lack, made on use.
    unihex: Vec<Lazy>,
    dynamic: RefCell<Dynamic>,
    /// [`Self::width_pools`] by font id, `""` for the default.
    pools: RefCell<HashMap<String, Rc<WidthPool>>>,
    /// The pictures of `object` components, made on use like Unifont's.
    pub(super) objects: RefCell<super::objects::ObjectSprites>,
    pub pages: Vec<RgbaImage>,
    /// A white pixel, for the underline and strikethrough of `§n`/`§m`
    pub white: TextureId,
}

/// A font with its page textures and a white pixel: what plain text and a cursor need.
#[derive(Clone, Copy)]
pub struct Glyphs<'a> {
    pub font: &'a Font,
    pub pages: &'a [TextureId],
    pub white: TextureId,
}

impl Font {
    pub fn load(assets: &Assets, root_id: &str) -> anyhow::Result<Self> {
        let mut pages = Vec::new();
        let mut page_index_by_path = HashMap::new();
        let mut glyphs = GlyphMap::default();
        let mut unihex = Vec::new();
        load_provider_file(
            assets,
            root_id,
            &mut pages,
            &mut page_index_by_path,
            &mut glyphs,
            &mut unihex,
        )?;
        if root_id == DEFAULT_FONT
            && let Err(e) = load_provider_file(
                assets,
                "minecraft:include/unifont_pua",
                &mut pages,
                &mut page_index_by_path,
                &mut glyphs,
                &mut unihex,
            )
        {
            melon_base::warn!("unifont_pua: {e:#}");
        }
        let mut faces = HashMap::new();
        if root_id == DEFAULT_FONT {
            for id in other_font_ids(assets) {
                let mut face = Face {
                    glyphs: GlyphMap::default(),
                    unihex: false,
                };
                let mut own_unihex = Vec::new();
                if let Err(e) = load_provider_file(
                    assets,
                    &id,
                    &mut pages,
                    &mut page_index_by_path,
                    &mut face.glyphs,
                    &mut own_unihex,
                ) {
                    melon_base::warn!("{id}: {e:#}");
                    continue;
                }
                face.unihex = !own_unihex.is_empty();
                if unihex.is_empty() {
                    unihex = own_unihex;
                }
                faces.insert(id, face);
            }
        }
        Ok(Self {
            ascii: ascii_table(&glyphs),
            glyphs,
            faces,
            unihex,
            dynamic: RefCell::default(),
            pools: RefCell::default(),
            objects: RefCell::default(),
            pages,
            white: TextureId(0),
        })
    }

    /// A launcher font: one bitmap page, no Unifont, no Mojang files.
    pub fn from_bitmaps(glyphs: GlyphMap, pages: Vec<RgbaImage>) -> Self {
        Self {
            ascii: ascii_table(&glyphs),
            glyphs,
            faces: HashMap::new(),
            unihex: Vec::new(),
            dynamic: RefCell::default(),
            pools: RefCell::default(),
            objects: RefCell::default(),
            pages,
            white: TextureId(0),
        }
    }

    /// The glyph for `c`: from the bitmap fonts, else Unifont's.
    pub fn glyph(&self, c: char) -> Option<Glyph> {
        if let Some(Some(glyph)) = self.ascii.get(c as usize) {
            return Some(*glyph);
        }
        if let Some(glyph) = self.glyphs.get(&c) {
            return Some(*glyph);
        }
        self.unihex_glyph(c)
    }

    /// [`Self::glyph`] in the font `font` names (`Style.font`).
    pub fn glyph_in(&self, font: Option<&str>, c: char) -> Option<Glyph> {
        // The default font isn't among `faces`: no lookup for its name.
        let font = font.filter(|&id| id != DEFAULT_FONT && id != "default");
        let Some(face) = font.and_then(|id| self.face(id)) else {
            if let Some(id) = font
                && melon_core::probe::enabled()
                && id != DEFAULT_FONT
                && id != "default"
            {
                melon_core::probe::count("font.missing", id);
            }
            let glyph = self.glyph(c);
            if glyph.is_none() && melon_core::probe::enabled() && !c.is_control() && c != ' ' {
                melon_core::probe::count(
                    "font.glyph_missing",
                    format!("U+{:04X} in default", c as u32),
                );
            }
            return glyph;
        };
        if let Some(glyph) = face.glyphs.get(&c) {
            return Some(*glyph);
        }
        if face.unihex {
            return self.unihex_glyph(c);
        }
        if melon_core::probe::enabled() && !c.is_control() && c != ' ' {
            melon_core::probe::count(
                "font.glyph_missing",
                format!("U+{:04X} in {}", c as u32, font.unwrap_or("")),
            );
        }
        None
    }

    fn face(&self, id: &str) -> Option<&Face> {
        match id.split_once(':') {
            Some(_) => self.faces.get(id),
            None => self.faces.get(&format!("minecraft:{id}")),
        }
    }

    /// Whether `id` names a font that is loaded, the default one included.
    pub fn has_font(&self, id: &str) -> bool {
        id == DEFAULT_FONT || id == "default" || self.face(id).is_some()
    }

    fn unihex_glyph(&self, c: char) -> Option<Glyph> {
        if self.unihex.is_empty() {
            return None;
        }
        let mut dynamic = self.dynamic.borrow_mut();
        if let Some(glyph) = dynamic.baked.get(&c) {
            return Some(*glyph);
        }
        let width = self.unihex.iter().find_map(|u| u.get()?.width(c))?;
        dynamic.wanted.push(c);
        Some(Glyph {
            kind: GlyphKind::Space,
            advance: unihex::advance(width),
            shadow: 0.5,
            bold: 0.5,
        })
    }

    /// Makes the Unifont glyphs asked for since the last call.
    pub fn bake_pending(&self) -> DynamicWork {
        let mut dynamic = self.dynamic.borrow_mut();
        if dynamic.wanted.is_empty() {
            return DynamicWork::default();
        }
        let mut wanted = std::mem::take(&mut dynamic.wanted);
        wanted.sort_unstable();
        wanted.dedup();
        let first_page = self.pages.len();
        let known_pages = dynamic.pages.len();
        let mut work = DynamicWork::default();
        let size = DYNAMIC_PAGE as f32;
        for c in wanted {
            if dynamic.baked.contains_key(&c) {
                continue;
            }
            let Some(image) = self.unihex.iter().find_map(|u| u.get()?.raster(c)) else {
                continue;
            };
            let width = image.width();
            let (mut x, mut y) = dynamic.pages.last().copied().unwrap_or((0, 0));
            if !dynamic.pages.is_empty() && x + width > DYNAMIC_PAGE {
                x = 0;
                y += unihex::GLYPH_HEIGHT + 1;
            }
            if dynamic.pages.is_empty() || y + unihex::GLYPH_HEIGHT > DYNAMIC_PAGE {
                dynamic.pages.push((0, 0));
                x = 0;
                y = 0;
            }
            let page = dynamic.pages.len() - 1;
            dynamic.pages[page] = (x + width + 1, y);
            let nudge = 0.01;
            let glyph = Glyph {
                kind: GlyphKind::Bitmap {
                    page: first_page + page,
                    u0: (x as f32 + nudge) / size,
                    v0: (y as f32 + nudge) / size,
                    u1: ((x + width) as f32 - nudge) / size,
                    v1: ((y + unihex::GLYPH_HEIGHT) as f32 - nudge) / size,
                    width: width as f32 / 2.0,
                    height: unihex::GLYPH_HEIGHT as f32 / 2.0,
                    ascent: 7,
                },
                advance: unihex::advance(width as i32),
                shadow: 0.5,
                bold: 0.5,
            };
            dynamic.baked.insert(c, glyph);
            work.writes.push(GlyphWrite {
                page: first_page + page,
                x,
                y,
                image,
            });
        }
        work.new_pages = dynamic.pages.len() - known_pages;
        work
    }

    /// `FontSet.glyphsByWidth`: the characters of `font` by `ceil(advance)`, Unifont's
    /// and the advance-only ones included.
    fn width_pools(&self, font: Option<&str>) -> Rc<WidthPool> {
        let key = font.unwrap_or("");
        if let Some(pools) = self.pools.borrow().get(key) {
            return pools.clone();
        }
        let face = font.and_then(|id| self.face(id));
        let glyphs = face.map_or(&self.glyphs, |face| &face.glyphs);
        let mut pools = WidthPool::default();
        for (&c, glyph) in glyphs {
            pools
                .entry(glyph.advance.ceil() as i32)
                .or_default()
                .push(c);
        }
        if face.is_none_or(|face| face.unihex) {
            let mut seen = rustc_hash::FxHashSet::default();
            for unihex in self.unihex.iter().filter_map(|u| u.get()) {
                for (c, width) in unihex.widths() {
                    if !glyphs.contains_key(&c) && seen.insert(c) {
                        pools
                            .entry(unihex::advance(width).ceil() as i32)
                            .or_default()
                            .push(c);
                    }
                }
            }
        }
        for chars in pools.values_mut() {
            chars.sort_unstable();
        }
        let pools = Rc::new(pools);
        self.pools
            .borrow_mut()
            .insert(key.to_owned(), pools.clone());
        pools
    }

    /// `FontSet.getRandomGlyph` for `§k`: a character of `font` as wide as `c`,
    /// picked by `seed`. A Unifont glyph not made yet is asked for and passed over.
    pub fn random_glyph_in(&self, font: Option<&str>, c: char, seed: &mut u64) -> char {
        if c == ' ' {
            return c;
        }
        let pools = self.width_pools(font);
        let Some(pool) = pools
            .get(&(self.advance_in(font, c).ceil() as i32))
            .filter(|pool| !pool.is_empty())
        else {
            return c;
        };
        let glyphs = font
            .and_then(|id| self.face(id))
            .map_or(&self.glyphs, |face| &face.glyphs);
        for _ in 0..8 {
            *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let pick = pool[(*seed >> 33) as usize % pool.len()];
            let pending = !glyphs.contains_key(&pick)
                && self
                    .unihex_glyph(pick)
                    .is_none_or(|g| matches!(g.kind, GlyphKind::Space));
            if !pending {
                return pick;
            }
        }
        c
    }

    pub fn advance(&self, c: char) -> f32 {
        self.advance_in(None, c)
    }

    /// `GlyphInfo.getAdvance(bold)`: [`Self::advance_in`], plus the glyph's
    /// bold offset when `bold` (1, half that for Unifont's).
    pub fn styled_advance_in(&self, font: Option<&str>, c: char, bold: bool) -> f32 {
        let advance = self.advance_in(font, c);
        if bold {
            advance + self.glyph_in(font, c).map_or(1.0, |g| g.bold)
        } else {
            advance
        }
    }

    /// [`Self::advance`] in font `font`.
    pub fn advance_in(&self, font: Option<&str>, c: char) -> f32 {
        if c == super::OBJECT_CHAR {
            return SPRITE_ADVANCE;
        }
        if let Some(glyph) = self.glyph_in(font, c) {
            return glyph.advance;
        }
        if sprite_index(c).is_some() {
            return SPRITE_ADVANCE;
        }
        0.0
    }

    /// `Font.width`: `§` codes take no room, bold glyphs a pixel more.
    pub fn text_width(&self, text: &str) -> f32 {
        if has_codes(text) {
            return super::layout::width(self, &StyledText::formatted(text));
        }
        text.chars().map(|c| self.advance(c)).sum()
    }

    /// Greedy wrap to `max_width` on whitespace (login modal / error prose).
    pub fn wrap(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = self.wrap_plain(text, max_width);
        if has_codes(text) {
            let mut carried = String::new();
            for line in &mut lines {
                let prefixed = format!("{carried}{line}");
                carried = active_codes(&prefixed);
                *line = prefixed;
            }
        }
        lines
    }

    /// [`Self::wrap`] before the codes carry over; each line's width is kept up word by word.
    fn wrap_plain(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            let mut width = LineWidth::default();
            for word in paragraph.split_whitespace() {
                if line.is_empty() {
                    line.push_str(word);
                    width.push_str(self, word);
                    continue;
                }
                let mut longer = width.clone();
                longer.push_str(self, " ");
                longer.push_str(self, word);
                if longer.width() > max_width {
                    lines.push(std::mem::take(&mut line));
                    line.push_str(word);
                    width = LineWidth::default();
                    width.push_str(self, word);
                } else {
                    line.push(' ');
                    line.push_str(word);
                    width = longer;
                }
            }
            lines.push(line);
        }
        lines
    }

    /// Draws `text` top-left at `(x, y)`, with vanilla's default drop shadow.
    pub fn push_text(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
    ) -> f32 {
        self.push_text_with(draw_list, page_textures, text, [x, y], color, true)
    }

    /// `GuiGraphics.centeredText`.
    pub fn push_centered(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        center_x: f32,
        y: f32,
        color: [f32; 4],
    ) {
        let x = (center_x - self.text_width(text) / 2.0).round();
        self.push_text(draw_list, page_textures, text, x, y, color);
    }

    /// `push_text` without the drop shadow (`GuiGraphics.text(..., false)`).
    pub fn push_text_flat(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
    ) -> f32 {
        self.push_text_with(draw_list, page_textures, text, [x, y], color, false)
    }

    fn push_text_with(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        [x, y]: [f32; 2],
        color: [f32; 4],
        with_shadow: bool,
    ) -> f32 {
        if has_codes(text) {
            let paint = self.paint(page_textures, color, with_shadow);
            return super::layout::draw(draw_list, &paint, &StyledText::formatted(text), x, y);
        }
        self.push_plain(
            draw_list,
            text,
            [x, y],
            color,
            with_shadow,
            |list, page, rect, uv, color, _| {
                list.push_rect(page_textures[page], rect, uv, color);
            },
        )
    }

    /// `push_text` with italic shear (`EditBox.SEARCH_HINT_STYLE`).
    pub fn push_text_italic(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
    ) -> f32 {
        if has_codes(text) {
            let paint = self.paint(page_textures, color, true);
            let mut styled = StyledText::default();
            let italic = super::TextStyle {
                italic: true,
                ..Default::default()
            };
            styled.push_formatted(text, &italic);
            return super::layout::draw(draw_list, &paint, &styled, x, y);
        }
        self.push_plain(
            draw_list,
            text,
            [x, y],
            color,
            true,
            |list, page, rect, uv, color, up| {
                let shear = [1.0 - 0.25 * up, 1.0 - 0.25 * (up + rect[3])];
                list.push_rect_sheared(page_textures[page], rect, shear, uv, color);
            },
        )
    }

    /// `push_text` through `transform` (vanilla `SplashRenderer.extractRenderState`).
    pub fn push_text_transformed(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        transform: &Transform2D,
        origin: [f32; 2],
        color: [f32; 4],
    ) -> f32 {
        let text = strip_codes(text);
        self.push_plain(
            draw_list,
            &text,
            origin,
            color,
            true,
            |list, page, rect, uv, color, _| {
                list.push_rect_transformed(page_textures[page], transform, rect, uv, color);
            },
        )
    }

    /// `Font.drawInBatch` for text without `§` codes, advancing like [`Self::text_width`].
    /// Each glyph goes to `quad` (page, `[x, y, w, h]`, uv, colour, top below `y`), shadow first.
    fn push_plain(
        &self,
        draw_list: &mut DrawList,
        text: &str,
        [x, y]: [f32; 2],
        color: [f32; 4],
        with_shadow: bool,
        mut quad: impl FnMut(&mut DrawList, usize, [f32; 4], [f32; 4], [f32; 4], f32),
    ) -> f32 {
        let shadow = shadow_color(color);
        let program = draw_list.set_program(Program::Text);
        let mut pen = x;
        for ch in text.chars() {
            let found = self.glyph(ch);
            if let Some(glyph) = found
                && let GlyphKind::Bitmap {
                    page,
                    u0,
                    v0,
                    u1,
                    v1,
                    width,
                    height,
                    ascent,
                } = glyph.kind
            {
                let up = BASELINE_ASCENT - ascent as f32;
                let uv = [u0, v0, u1, v1];
                if with_shadow {
                    let rect = [pen + glyph.shadow, y + up + glyph.shadow, width, height];
                    quad(draw_list, page, rect, uv, shadow, up);
                }
                quad(draw_list, page, [pen, y + up, width, height], uv, color, up);
            }
            // [`Self::advance`] without a second lookup.
            pen += match found {
                Some(glyph) if ch != super::OBJECT_CHAR => glyph.advance,
                _ => self.advance(ch),
            };
        }
        draw_list.set_program(program);
        pen - x
    }
}

impl Font {
    /// How plain text with `§` codes is drawn.
    fn paint<'a>(&'a self, pages: &'a [TextureId], color: [f32; 4], shadow: bool) -> TextPaint<'a> {
        TextPaint {
            color: [color[0], color[1], color[2]],
            alpha: color[3],
            shadow,
            ..TextPaint::new(self, pages, self.white)
        }
    }
}

/// The fonts the resource packs define besides the default one, as `namespace:name`.
fn other_font_ids(assets: &Assets) -> Vec<String> {
    let mut ids = Vec::new();
    let root = assets.root();
    for namespace in melon_assets::assets::namespaces(root) {
        for path in melon_assets::assets::packs::active().list(root, &namespace, "font") {
            let Some(name) = path
                .strip_prefix("font/")
                .and_then(|p| p.strip_suffix(".json"))
            else {
                continue;
            };
            let id = format!("{namespace}:{name}");
            if id != DEFAULT_FONT {
                ids.push(id);
            }
        }
    }
    ids
}

/// [`Font::text_width`] of a line that grows at its end: the same runs, summed in the same order.
#[derive(Clone, Default)]
struct LineWidth {
    /// The style the next character gets, as `StyledText::formatted` reads the codes.
    style: super::TextStyle,
    /// The style of the run being summed; `None` before the first character.
    run_style: Option<super::TextStyle>,
    /// The finished runs' widths, summed.
    done: f32,
    run: f32,
    /// A `§` waiting for its code.
    code: bool,
}

impl LineWidth {
    fn push_str(&mut self, font: &Font, text: &str) {
        for c in text.chars() {
            if std::mem::take(&mut self.code) {
                super::apply_code(&mut self.style, c, &super::TextStyle::default());
            } else if c == '§' {
                self.code = true;
            } else {
                if self.run_style.as_ref() != Some(&self.style) {
                    self.done += std::mem::take(&mut self.run);
                    self.run_style = Some(self.style.clone());
                }
                self.run += font.styled_advance_in(None, c, self.style.bold);
            }
        }
    }

    fn width(&self) -> f32 {
        self.done + self.run
    }
}

/// The ASCII glyphs of `glyphs`, by code.
fn ascii_table(glyphs: &GlyphMap) -> Box<[Option<Glyph>; 128]> {
    let mut table = Box::new([None; 128]);
    for (code, slot) in (0u8..).zip(table.iter_mut()) {
        *slot = glyphs.get(&char::from(code)).copied();
    }
    table
}

fn has_codes(text: &str) -> bool {
    text.contains('§')
}

/// `ChatFormatting.stripFormatting`: `text` without its `§` codes.
pub fn strip_codes(text: &str) -> std::borrow::Cow<'_, str> {
    if !has_codes(text) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// The codes still in force at the end of `text`.
fn active_codes(text: &str) -> String {
    let mut active = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '§' {
            continue;
        }
        let Some(code) = chars.next() else { break };
        match code.to_ascii_lowercase() {
            '0'..='9' | 'a'..='f' => {
                active.clear();
                active.push('§');
                active.push(code);
            }
            'k'..='o' => {
                active.push('§');
                active.push(code);
            }
            'r' => active.clear(),
            _ => {}
        }
    }
    active
}

#[cfg(test)]
mod wrap_tests {
    use super::*;

    /// The wrap before it kept widths up: every candidate line measured from scratch.
    fn wrap_plain_reference(font: &Font, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if !line.is_empty() && font.text_width(&candidate) > max_width {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_string();
                } else {
                    line = candidate;
                }
            }
            lines.push(line);
        }
        lines
    }

    /// Advances that don't add up exactly in `f32`, so a different order of sums shows.
    fn odd_font() -> Font {
        let mut glyphs = GlyphMap::default();
        for (i, c) in ('a'..='z').chain('A'..='Z').chain('0'..='9').enumerate() {
            let mut glyph = Glyph::new(GlyphKind::Space, 0.1 + i as f32 * 0.37);
            glyph.bold = 0.3 + (i % 3) as f32 * 0.45;
            glyphs.insert(c, glyph);
        }
        glyphs.insert(' ', Glyph::new(GlyphKind::Space, 2.3));
        glyphs.insert('.', Glyph::new(GlyphKind::Space, 0.7));
        Font::from_bitmaps(glyphs, Vec::new())
    }

    #[test]
    fn linear_wrap_matches_the_reference() {
        let font = odd_font();
        let texts = [
            "",
            "one",
            "  lead and  trail  ",
            "the quick brown fox jumps over the lazy dog again and again",
            "a\nb c\n\nd e f g h i j k l m n o p",
            "Supercalifragilisticexpialidocious is long. so is pneumonoultramicroscopic",
            "\u{a7}lbold words \u{a7}rplain \u{a7}cred \u{a7}lred bold \u{a7}othen italic words here",
            "\u{a7}l\u{a7}l same \u{a7}r\u{a7}r style \u{a7}4\u{a7}4 twice",
            "trailing code \u{a7} l and \u{a7}",
            "\u{a7}Lupper \u{a7}Ocase \u{a7}Zunknown \u{a7}Rreset words go on and on",
            "x \u{a7} \u{a7}l y z w v u t s r q p o n m l k j i h g f e d c b a",
            "unknown \u{e9}t\u{e9} \u{4e2d} glyphs . . . 0 1 2 3 4 5 6 7 8 9",
        ];
        let mut checked = 0;
        for text in texts {
            for step in 0..400 {
                let max_width = step as f32 * 0.25;
                assert_eq!(
                    font.wrap_plain(text, max_width),
                    wrap_plain_reference(&font, text, max_width),
                    "{text:?} at {max_width}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, texts.len() * 400);
    }
}

#[cfg(test)]
mod unifont_tests {
    use super::*;
    use unihex::Unihex;

    fn font_with(hex: &str) -> Font {
        let mut unihex = Unihex::default();
        unihex.load_text(hex).unwrap();
        Font {
            glyphs: GlyphMap::default(),
            faces: HashMap::new(),
            unihex: vec![Lazy::ready(unihex)],
            dynamic: RefCell::default(),
            pools: RefCell::default(),
            objects: RefCell::default(),
            pages: Vec::new(),
            white: TextureId(0),
        }
    }

    fn rows(row: &str) -> String {
        row.repeat(16)
    }

    #[test]
    fn a_run_in_the_alt_font_draws_its_own_glyphs() {
        let root = melon_assets::assets::default_root();
        if !root.join("minecraft/font/alt.json").is_file() {
            eprintln!("no extracted assets, skipping");
            return;
        }
        let font = Font::load(&Assets::from_root(root), DEFAULT_FONT).unwrap();
        assert!(font.has_font("minecraft:alt") && font.has_font("alt"));
        let page = |glyph: Option<Glyph>| match glyph.map(|g| g.kind) {
            Some(GlyphKind::Bitmap { page, .. }) => Some(page),
            _ => None,
        };
        let default = page(font.glyph('a')).unwrap();
        let alt = page(font.glyph_in(Some("minecraft:alt"), 'a')).unwrap();
        assert_ne!(default, alt);
        assert_eq!(page(font.glyph_in(Some("nope:font"), 'a')), Some(default));
    }

    /// `Font.getGlyph`: a space stays, anything else becomes a glyph of the same
    /// `ceil(advance)`, a Unifont one only once it is made.
    #[test]
    fn an_obfuscated_character_becomes_one_of_its_width_from_every_provider() {
        let mut font = font_with(&format!(
            "0061:{}\n0062:{}\n4E2D:{}\n",
            rows("38"),
            rows("1C"),
            rows("FFFF")
        ));
        font.glyphs.insert(' ', Glyph::new(GlyphKind::Space, 4.0));
        font.glyphs
            .insert('\u{e000}', Glyph::new(GlyphKind::Space, 1.5));
        font.ascii = ascii_table(&font.glyphs);
        let mut seed = 7;
        assert_eq!(font.random_glyph_in(None, ' ', &mut seed), ' ');
        assert_ne!(font.random_glyph_in(None, 'a', &mut seed), 'b');
        for _ in 0..64 {
            font.random_glyph_in(None, 'a', &mut seed);
        }
        font.bake_pending();
        let picks: std::collections::HashSet<char> = (0..64)
            .map(|_| font.random_glyph_in(None, 'a', &mut seed))
            .collect();
        assert_eq!(
            picks,
            std::collections::HashSet::from(['a', 'b', '\u{e000}'])
        );
        assert_eq!(font.random_glyph_in(None, '中', &mut seed), '中');
    }

    #[test]
    fn a_unifont_glyph_has_its_advance_at_once_and_its_picture_after_baking() {
        let font = font_with(&format!("4E2D:{}\n0061:{}\n", rows("FFFF"), rows("38")));
        let glyph = font.glyph('中').unwrap();
        assert!(matches!(glyph.kind, GlyphKind::Space));
        assert_eq!(glyph.advance, 9.0);
        assert_eq!(font.advance('a'), 2.0);
        assert!(font.glyph('x').is_none());
        let work = font.bake_pending();
        assert_eq!((work.new_pages, work.writes.len()), (1, 2));
        assert!(font.bake_pending().is_empty());
        let GlyphKind::Bitmap {
            page,
            width,
            height,
            ascent,
            ..
        } = font.glyph('中').unwrap().kind
        else {
            panic!("baked glyphs are drawn");
        };
        assert_eq!((page, width, height, ascent), (0, 8.0, 8.0, 7));
        assert_eq!(font.glyph('中').unwrap().shadow, 0.5);
    }

    #[test]
    fn the_default_font_has_the_private_use_glyphs_of_unifont_pua() {
        let root = melon_assets::assets::default_root();
        if !root.join("minecraft/font/unifont_pua.zip").exists() {
            eprintln!("no unifont_pua.zip (Launcher: Prepare): skipped");
            return;
        }
        let font = Font::load(&Assets::from_root(root), "minecraft:default").unwrap();
        for c in ['\u{E000}', '\u{E001}', '\u{E002}'] {
            assert!(font.glyph(c).is_some(), "U+{:04X}", c as u32);
        }
    }

    #[test]
    fn the_default_font_draws_latin_cyrillic_greek_and_chinese_japanese_korean() {
        let root = melon_assets::assets::default_root();
        if !root.join("minecraft/font/unifont.zip").exists() {
            eprintln!("no unifont.zip (Launcher: Prepare): skipped");
            return;
        }
        let font = Font::load(&Assets::from_root(root), "minecraft:default").unwrap();
        for c in "Aa Привет Ωμέγα".chars() {
            assert!(
                font.glyph(c)
                    .is_some_and(|g| !matches!(g.kind, GlyphKind::Space) || c == ' '),
                "{c}"
            );
        }
        assert!(
            font.bake_pending().is_empty(),
            "bitmap glyphs need no baking"
        );
        for c in "中文日本語こんにちはカタカナ한국어".chars() {
            let glyph = font.glyph(c).unwrap_or_else(|| panic!("{c}"));
            assert!(glyph.advance >= 5.0, "{c}: {}", glyph.advance);
        }
        assert_eq!(font.advance('中'), 9.0);
        let work = font.bake_pending();
        assert!(work.new_pages >= 1 && work.writes.len() >= 14);
        assert!(matches!(
            font.glyph('中').unwrap().kind,
            GlyphKind::Bitmap { .. }
        ));
    }

    #[test]
    fn glyphs_fill_rows_then_new_pages_without_overlapping() {
        let mut hex = String::new();
        for code in 0x4E00u32..0x4E00 + 300 {
            hex.push_str(&format!("{code:04X}:{}\n", rows("FFFF")));
        }
        let font = font_with(&hex);
        for code in 0x4E00u32..0x4E00 + 300 {
            font.glyph(char::from_u32(code).unwrap());
        }
        let work = font.bake_pending();
        assert_eq!(work.new_pages, 2);
        assert_eq!(work.writes.len(), 300);
        let mut cells = std::collections::HashSet::new();
        for write in &work.writes {
            assert!(write.x + write.image.width() <= DYNAMIC_PAGE);
            assert!(write.y + 16 <= DYNAMIC_PAGE);
            assert!(cells.insert((write.page, write.x, write.y)), "overlap");
        }
    }
}

#[cfg(test)]
mod format_code_tests {
    use super::*;

    #[test]
    fn codes_are_stripped_and_carried() {
        assert_eq!(strip_codes("§fInventory"), "Inventory");
        assert_eq!(strip_codes("§r   §fCrafter§"), "   Crafter");
        assert_eq!(active_codes("§cred §lbold"), "§c§l");
        assert_eq!(active_codes("§c§l x §r y"), "");
        assert_eq!(active_codes("§lbold §ablue"), "§a");
    }
}
