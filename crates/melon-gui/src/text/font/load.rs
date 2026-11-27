//! Reading `font/*.json`.

use super::unihex::{Lazy, SizeOverride};
use super::*;

/// How deep `reference` providers may nest, so a pack whose fonts refer to each other can't
/// loop.
const MAX_REFERENCE_DEPTH: u32 = 8;

/// Loads font `id` (`font/<path>.json`).
pub(super) fn load_provider_file(
    assets: &Assets,
    id: &str,
    pages: &mut Vec<RgbaImage>,
    page_index_by_path: &mut HashMap<String, usize>,
    glyphs: &mut GlyphMap,
    unihex: &mut Vec<Lazy>,
) -> anyhow::Result<()> {
    load_font_stack(assets, id, pages, page_index_by_path, glyphs, unihex, 0)
}

fn load_font_stack(
    assets: &Assets,
    id: &str,
    pages: &mut Vec<RgbaImage>,
    page_index_by_path: &mut HashMap<String, usize>,
    glyphs: &mut GlyphMap,
    unihex: &mut Vec<Lazy>,
    depth: u32,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        depth <= MAX_REFERENCE_DEPTH,
        "Font-Verweise zu tief verschachtelt: {id}"
    );
    let (namespace, path) = melon_assets::assets::split_id(id);
    let copies = melon_assets::assets::resource_stack(
        assets.root(),
        namespace,
        &format!("font/{path}.json"),
    );
    anyhow::ensure!(!copies.is_empty(), "could not read font definition: {id}");
    for bytes in copies.iter().rev() {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(
            melon_assets::assets::packs::meta::strip_bom(bytes),
        ) else {
            crate::warn!("{id} is not valid JSON, skipped");
            continue;
        };
        let providers = value
            .get("providers")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        load_providers(
            assets,
            providers,
            pages,
            page_index_by_path,
            glyphs,
            unihex,
            depth,
        )?;
    }
    Ok(())
}

fn load_providers(
    assets: &Assets,
    providers: Vec<serde_json::Value>,
    pages: &mut Vec<RgbaImage>,
    page_index_by_path: &mut HashMap<String, usize>,
    glyphs: &mut GlyphMap,
    unihex: &mut Vec<Lazy>,
    depth: u32,
) -> anyhow::Result<()> {
    for provider in providers {
        if !filter_allows(&provider) {
            continue;
        }
        let provider_type = provider.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match provider_type {
            "bitmap" => {
                if let Err(e) = load_bitmap(assets, &provider, pages, page_index_by_path, glyphs) {
                    crate::warn!("{e:#}");
                }
            }
            "space" => load_space(&provider, glyphs),
            "unihex" => match load_unihex(assets, &provider) {
                Ok(provider) => unihex.push(provider),
                Err(e) => crate::warn!("{e:#}"),
            },
            "reference" => {
                if let Some(ref_id) = provider.get("id").and_then(|v| v.as_str()) {
                    load_font_stack(
                        assets,
                        ref_id,
                        pages,
                        page_index_by_path,
                        glyphs,
                        unihex,
                        depth + 1,
                    )?;
                }
            }
            other => {
                crate::warn!(
                    "provider type '{other}' not supported yet, skipped (see notes/Assets.md)"
                );
            }
        }
    }
    Ok(())
}

/// `FontOption.Filter`: a provider that names options (`uniform`, `jp`)
fn filter_allows(provider: &serde_json::Value) -> bool {
    let Some(filter) = provider.get("filter").and_then(|f| f.as_object()) else {
        return true;
    };
    filter
        .values()
        .all(|wanted| wanted.as_bool() == Some(false))
}

/// A `unihex` provider.
fn load_unihex(assets: &Assets, provider: &serde_json::Value) -> anyhow::Result<Lazy> {
    let file = provider
        .get("hex_file")
        .and_then(|v| v.as_str())
        .context("unihex-Provider ohne 'hex_file'")?;
    let char_of = |value: Option<&serde_json::Value>| {
        value
            .and_then(|v| v.as_str())
            .and_then(|s| s.chars().next())
    };
    let overrides: Vec<SizeOverride> = provider
        .get("size_overrides")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|o| {
                    Some(SizeOverride {
                        from: char_of(o.get("from"))?,
                        to: char_of(o.get("to"))?,
                        left: o.get("left")?.as_i64()? as i32,
                        right: o.get("right")?.as_i64()? as i32,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Lazy::new(
        assets.root().to_path_buf(),
        file.to_owned(),
        overrides,
    ))
}

pub(super) fn load_bitmap(
    assets: &Assets,
    provider: &serde_json::Value,
    pages: &mut Vec<RgbaImage>,
    page_index_by_path: &mut HashMap<String, usize>,
    glyphs: &mut GlyphMap,
) -> anyhow::Result<()> {
    let file = provider
        .get("file")
        .and_then(|v| v.as_str())
        .context("bitmap-Provider ohne 'file'")?;
    let height = provider.get("height").and_then(|v| v.as_i64()).unwrap_or(8) as i32;
    let ascent = provider.get("ascent").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let rows: Vec<String> = provider
        .get("chars")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();

    let cols = rows.first().map(|r| r.chars().count()).unwrap_or(0);
    let rows_count = rows.len();
    if cols == 0 || rows_count == 0 {
        return Ok(());
    }

    let page_index = match page_index_by_path.get(file) {
        Some(&idx) => idx,
        None => {
            let img = assets
                .image(file)
                .with_context(|| format!("could not load font texture: {file}"))?;
            let idx = pages.len();
            pages.push(img);
            page_index_by_path.insert(file.to_owned(), idx);
            idx
        }
    };

    let img = &pages[page_index];
    let (img_w, img_h) = (img.width(), img.height());
    let glyph_w = img_w / cols as u32;
    let glyph_h = img_h / rows_count as u32;
    if glyph_w == 0 || glyph_h == 0 {
        return Ok(());
    }
    let pixel_scale = height as f32 / glyph_h as f32;

    for (row_i, row) in rows.iter().enumerate() {
        for (col_i, ch) in row.chars().enumerate() {
            if ch == '\u{0}' || glyphs.contains_key(&ch) {
                continue;
            }
            let actual_w = actual_glyph_width(img, glyph_w, glyph_h, col_i as u32, row_i as u32);
            let advance = ((0.5 + actual_w as f32 * pixel_scale) as i32 + 1) as f32;
            let u0 = (col_i as u32 * glyph_w) as f32 / img_w as f32;
            let v0 = (row_i as u32 * glyph_h) as f32 / img_h as f32;
            let u1 = u0 + glyph_w as f32 / img_w as f32;
            let v1 = v0 + glyph_h as f32 / img_h as f32;
            glyphs.insert(
                ch,
                Glyph::new(
                    GlyphKind::Bitmap {
                        page: page_index,
                        u0,
                        v0,
                        u1,
                        v1,
                        width: glyph_w as f32 * pixel_scale,
                        height: height as f32,
                        ascent,
                    },
                    advance,
                ),
            );
        }
    }
    Ok(())
}

/// Vanilla `BitmapProvider.getActualGlyphWidth`: last non-transparent column.
pub(super) fn actual_glyph_width(
    img: &RgbaImage,
    glyph_w: u32,
    glyph_h: u32,
    slot_x: u32,
    slot_y: u32,
) -> u32 {
    for w in (0..glyph_w).rev() {
        let x = slot_x * glyph_w + w;
        for y in 0..glyph_h {
            let yy = slot_y * glyph_h + y;
            if img.get_pixel(x, yy)[3] != 0 {
                return w + 1;
            }
        }
    }
    0
}

pub(super) fn load_space(provider: &serde_json::Value, glyphs: &mut GlyphMap) {
    let Some(advances) = provider.get("advances").and_then(|v| v.as_object()) else {
        return;
    };
    for (key, value) in advances {
        let Some(ch) = key.chars().next() else {
            continue;
        };
        if glyphs.contains_key(&ch) {
            continue;
        }
        let advance = value.as_f64().unwrap_or(0.0) as f32;
        glyphs.insert(ch, Glyph::new(GlyphKind::Space, advance));
    }
}
