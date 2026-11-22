//! Vanilla `BitmapProvider` / `FontManager` (`font/default.json`:

use anyhow::Context;
use image::RgbaImage;
use std::collections::HashMap;

use super::{StyledText, TextPaint};
use crate::assets::Assets;
use crate::renderer::TextureId;
use crate::renderer::quad::{DrawList, Program, Transform2D};

mod load;
pub mod unihex;

use load::load_provider_file;
use std::cell::RefCell;
use std::rc::Rc;
use unihex::Lazy;

/// Standard Minecraft text line height in (unscaled) GUI pixels.
pub const LINE_HEIGHT: f32 = 9.0;

/// Baseline ascent of the `ascii`/`accented` pages.
pub(crate) const BASELINE_ASCENT: f32 = 7.0;

/// Vanilla `GlyphInfo.getShadowOffset()` default.
pub(crate) const SHADOW_OFFSET: f32 = 1.0;

/// `ARGB.scaleRGB(textColor, 0.25F)` (`Font.PreparedTextBuilder.getShadowColor`,
pub(crate) fn shadow_color(color: [f32; 4]) -> [f32; 4] {
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

/// Text that holds an inline sprite (a `sprite` object component) carries it as a private-use
/// character.
pub const SPRITE_FIRST: char = '\u{E000}';
pub const SPRITE_COUNT: usize = 256;
const SPRITE_ADVANCE: f32 = 8.0;

/// Which sprite a placeholder character stands for.
pub fn sprite_index(c: char) -> Option<usize> {
    let offset = (c as usize).checked_sub(SPRITE_FIRST as usize)?;
    (offset < SPRITE_COUNT).then_some(offset)
}

/// The placeholder character for sprite `index`.
pub fn sprite_char(index: usize) -> Option<char> {
    char::from_u32(
        SPRITE_FIRST as u32
            + u32::try_from(index)
                .ok()
                .filter(|i| *i < SPRITE_COUNT as u32)?,
    )
}

/// How big a texture of Unifont glyphs is (`FontTexture`'s 256).
pub const DYNAMIC_PAGE: u32 = 256;

/// The characters of one font by `ceil(advance)`: `FontSet.glyphsByWidth`.
type WidthPool = HashMap<i32, Vec<char>>;

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

/// The font a run without one of its own is drawn in.
pub const DEFAULT_FONT: &str = "minecraft:default";

pub struct Font {
    glyphs: GlyphMap,
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
            crate::warn!("unifont_pua: {e:#}");
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
                    crate::warn!("{id}: {e:#}");
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
                && crate::probe::enabled()
                && id != DEFAULT_FONT
                && id != "default"
            {
                crate::probe::count("font.missing", id);
            }
            let glyph = self.glyph(c);
            if glyph.is_none() && crate::probe::enabled() && !c.is_control() && c != ' ' {
                crate::probe::count(
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
        if crate::probe::enabled() && !c.is_control() && c != ' ' {
            crate::probe::count(
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
        let mut pools: HashMap<i32, Vec<char>> = HashMap::new();
        for (&c, glyph) in glyphs {
            pools
                .entry(glyph.advance.ceil() as i32)
                .or_default()
                .push(c);
        }
        if face.is_none_or(|face| face.unihex) {
            let mut seen = std::collections::HashSet::new();
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

    fn wrap_plain(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if !line.is_empty() && self.text_width(&candidate) > max_width {
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
        self.push_text_with(draw_list, page_textures, text, x, y, color, true)
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
        self.push_text_with(draw_list, page_textures, text, x, y, color, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_with(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        with_shadow: bool,
    ) -> f32 {
        if has_codes(text) {
            let paint = self.paint(page_textures, color, with_shadow);
            return super::layout::draw(draw_list, &paint, &StyledText::formatted(text), x, y);
        }
        let shadow = shadow_color(color);
        let program = draw_list.set_program(Program::Text);
        let mut pen = x;
        for ch in text.chars() {
            if let Some(glyph) = self.glyph(ch) {
                if let GlyphKind::Bitmap {
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
                    let draw_y = y + (BASELINE_ASCENT - ascent as f32);
                    if with_shadow {
                        draw_list.push_rect(
                            page_textures[page],
                            pen + glyph.shadow,
                            draw_y + glyph.shadow,
                            width,
                            height,
                            [u0, v0, u1, v1],
                            shadow,
                        );
                    }
                    draw_list.push_rect(
                        page_textures[page],
                        pen,
                        draw_y,
                        width,
                        height,
                        [u0, v0, u1, v1],
                        color,
                    );
                }
                pen += glyph.advance;
            } else {
                pen += self.advance(' ');
            }
        }
        draw_list.set_program(program);
        pen - x
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
        let shadow = shadow_color(color);
        let program = draw_list.set_program(Program::Text);
        let mut pen = x;
        for ch in text.chars() {
            if let Some(glyph) = self.glyph(ch) {
                if let GlyphKind::Bitmap {
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
                    let draw_y = y + (BASELINE_ASCENT - ascent as f32);
                    let up = draw_y - y;
                    let down = up + height;
                    let shear_top = 1.0 - 0.25 * up;
                    let shear_bottom = 1.0 - 0.25 * down;
                    draw_list.push_rect_sheared(
                        page_textures[page],
                        pen + glyph.shadow,
                        draw_y + glyph.shadow,
                        draw_y + height + glyph.shadow,
                        width,
                        shear_top,
                        shear_bottom,
                        [u0, v0, u1, v1],
                        shadow,
                    );
                    draw_list.push_rect_sheared(
                        page_textures[page],
                        pen,
                        draw_y,
                        draw_y + height,
                        width,
                        shear_top,
                        shear_bottom,
                        [u0, v0, u1, v1],
                        color,
                    );
                }
                pen += glyph.advance;
            } else {
                pen += self.advance(' ');
            }
        }
        draw_list.set_program(program);
        pen - x
    }

    /// `push_text` through `transform`. Vanilla `SplashRenderer.extractRenderState`
    pub fn push_text_transformed(
        &self,
        draw_list: &mut DrawList,
        page_textures: &[TextureId],
        text: &str,
        transform: &Transform2D,
        origin: [f32; 2],
        color: [f32; 4],
    ) -> f32 {
        let shadow = shadow_color(color);
        let program = draw_list.set_program(Program::Text);
        let mut pen = origin[0];
        for ch in strip_codes(text).chars() {
            if let Some(glyph) = self.glyph(ch) {
                if let GlyphKind::Bitmap {
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
                    let draw_y = origin[1] + (BASELINE_ASCENT - ascent as f32);
                    draw_list.push_rect_transformed(
                        page_textures[page],
                        transform,
                        pen + glyph.shadow,
                        draw_y + glyph.shadow,
                        width,
                        height,
                        [u0, v0, u1, v1],
                        shadow,
                    );
                    draw_list.push_rect_transformed(
                        page_textures[page],
                        transform,
                        pen,
                        draw_y,
                        width,
                        height,
                        [u0, v0, u1, v1],
                        color,
                    );
                }
                pen += glyph.advance;
            } else {
                pen += self.advance(' ');
            }
        }
        draw_list.set_program(program);
        pen - origin[0]
    }
}

impl Font {
    /// How plain text with `§` codes is drawn.
    fn paint<'a>(&'a self, pages: &'a [TextureId], color: [f32; 4], shadow: bool) -> TextPaint<'a> {
        TextPaint {
            font: self,
            pages,
            white: self.white,
            color: [color[0], color[1], color[2]],
            alpha: color[3],
            shadow,
            obfuscation_seed: 0,
        }
    }
}

/// The fonts the resource packs define besides the default one, as `namespace:name`.
fn other_font_ids(assets: &Assets) -> Vec<String> {
    let mut ids = Vec::new();
    let root = assets.root();
    for namespace in crate::assets::namespaces(root) {
        for path in crate::assets::packs::active().list(root, &namespace, "font") {
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
        let root = crate::assets::default_root();
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
        let root = crate::assets::default_root();
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
        let root = crate::assets::default_root();
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
