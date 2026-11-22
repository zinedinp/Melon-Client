//! Vanilla `UnihexProvider`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};

use anyhow::{Context, bail};
use image::{Rgba, RgbaImage};

/// Every glyph is this many pixels tall.
pub const GLYPH_HEIGHT: u32 = 16;

/// `UnihexProvider.Glyph.getAdvance`: half the width, rounded down, and one.
pub fn advance(width: i32) -> f32 {
    (width / 2 + 1) as f32
}

/// A `size_overrides` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeOverride {
    pub from: char,
    pub to: char,
    pub left: i32,
    pub right: i32,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    /// Where the rows start in [`Unihex::rows`].
    offset: u32,
    /// How many bytes a row has: 1, 2, 3 or 4 (8 to 32 pixels).
    row_bytes: u8,
    /// The columns the glyph covers, counted from the left of its bitmap.
    left: i8,
    right: i8,
}

/// A `unihex` provider that reads its zip the first time a character needs it.
pub struct Lazy {
    root: std::path::PathBuf,
    file: String,
    overrides: Vec<SizeOverride>,
    loaded: std::cell::OnceCell<Option<Unihex>>,
}

impl Lazy {
    pub fn new(root: std::path::PathBuf, file: String, overrides: Vec<SizeOverride>) -> Self {
        Self {
            root,
            file,
            overrides,
            loaded: std::cell::OnceCell::new(),
        }
    }

    /// Already loaded (tests).
    #[cfg(test)]
    pub fn ready(unihex: Unihex) -> Self {
        let lazy = Self::new(std::path::PathBuf::new(), String::new(), Vec::new());
        lazy.loaded.set(Some(unihex)).ok();
        lazy
    }

    /// The glyphs, read on the first call.
    pub fn get(&self) -> Option<&Unihex> {
        self.loaded
            .get_or_init(|| {
                let (namespace, path) = crate::assets::split_id(&self.file);
                let Some(zip) = crate::assets::resource_top(&self.root, namespace, path) else {
                    crate::warn!("hex file {} missing (launcher: Prepare)", self.file);
                    return None;
                };
                match Unihex::from_zip(&zip, &self.overrides) {
                    Ok(unihex) => Some(unihex),
                    Err(e) => {
                        crate::warn!("hex file {}: {e:#}", self.file);
                        None
                    }
                }
            })
            .as_ref()
    }
}

/// One `unihex` provider: the glyphs of its hex files.
#[derive(Default)]
pub struct Unihex {
    glyphs: HashMap<char, Entry>,
    /// The bitmaps, 16 rows each, `row_bytes` a row, left bit first.
    rows: Vec<u8>,
}

fn hex_digit(byte: u8) -> anyhow::Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => bail!("expected a hex digit, got {:?}", byte as char),
    }
}

impl Unihex {
    /// Loads a zip's `.hex` files, then applies `overrides` (the widths a provider sets).
    pub fn from_zip(zip: &[u8], overrides: &[SizeOverride]) -> anyhow::Result<Self> {
        let mut unihex = Self::default();
        let mut parsed = Vec::new();
        zip_entries(zip, |name, text| {
            if name.ends_with(".hex") {
                unihex.read_hex(text, &mut parsed)?;
            }
            Ok(())
        })?;
        unihex.set_widths(overrides);
        unihex.rows.shrink_to_fit();
        unihex.glyphs.shrink_to_fit();
        Ok(unihex)
    }

    /// A hex file's text, for tests.
    #[cfg(test)]
    pub fn load_text(&mut self, text: &str) -> anyhow::Result<()> {
        self.read_hex(&mut text.as_bytes(), &mut Vec::new())?;
        self.set_widths(&[]);
        Ok(())
    }

    /// `readFromStream`.
    fn read_hex(&mut self, text: &mut dyn BufRead, scratch: &mut Vec<u8>) -> anyhow::Result<()> {
        let mut buffer = Vec::new();
        for number in 0.. {
            buffer.clear();
            if text.read_until(b'\n', &mut buffer)? == 0 {
                break;
            }
            let line = buffer.strip_suffix(b"\n").unwrap_or(&buffer);
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            let Some(colon) = line.iter().position(|&b| b == b':') else {
                bail!("line {number}: expected a colon");
            };
            let (digits, data) = (&line[..colon], &line[colon + 1..]);
            if !(4..=6).contains(&digits.len()) {
                bail!("line {number}: expected 4, 5 or 6 digits before the colon");
            }
            let mut code = 0u32;
            for &digit in digits {
                code = code << 4 | u32::from(hex_digit(digit)?);
            }
            let row_bytes = match data.len() {
                32 => 1,
                64 => 2,
                96 => 3,
                128 => 4,
                n => {
                    bail!("line {number}: expected an (8, 16, 24, 32) x 16 bitmap, got {n} digits")
                }
            };
            scratch.clear();
            for pair in data.chunks(2) {
                scratch.push(hex_digit(pair[0])? << 4 | hex_digit(pair[1])?);
            }
            let Some(c) = char::from_u32(code) else {
                continue;
            };
            let offset = u32::try_from(self.rows.len()).context("hex data too large")?;
            self.rows.extend_from_slice(scratch);
            self.glyphs.insert(
                c,
                Entry {
                    offset,
                    row_bytes,
                    left: 0,
                    right: 0,
                },
            );
        }
        Ok(())
    }

    /// `Definition.loadData`'s second half.
    fn set_widths(&mut self, overrides: &[SizeOverride]) {
        let rows = &self.rows;
        for (&c, entry) in &mut self.glyphs {
            let by_override = overrides
                .iter()
                .find(|o| (o.from..=o.to).contains(&c))
                .map(|o| (o.left, o.right));
            let (left, right) = by_override.unwrap_or_else(|| bitmap_columns(entry, rows));
            entry.left = left as i8;
            entry.right = right as i8;
        }
    }

    pub fn contains(&self, c: char) -> bool {
        self.glyphs.contains_key(&c)
    }

    /// How many glyphs there are.
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// Every glyph with its pixel width.
    pub fn widths(&self) -> impl Iterator<Item = (char, i32)> + '_ {
        self.glyphs
            .iter()
            .map(|(&c, e)| (c, i32::from(e.right) - i32::from(e.left) + 1))
    }

    /// The pixel width of `c`'s glyph (`right - left + 1`).
    pub fn width(&self, c: char) -> Option<i32> {
        self.glyphs
            .get(&c)
            .map(|e| i32::from(e.right) - i32::from(e.left) + 1)
    }

    /// `c`'s glyph as pixels, `width` wide and 16 tall, white where the bits are set.
    pub fn raster(&self, c: char) -> Option<RgbaImage> {
        let entry = self.glyphs.get(&c)?;
        let width = (i32::from(entry.right) - i32::from(entry.left) + 1).max(1) as u32;
        let mut image = RgbaImage::new(width, GLYPH_HEIGHT);
        for y in 0..GLYPH_HEIGHT {
            let row = row_bits(entry, &self.rows, y);
            for x in 0..width {
                let column = i32::from(entry.left) + x as i32;
                if (0..32).contains(&column) && row >> (31 - column) & 1 != 0 {
                    image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                }
            }
        }
        Some(image)
    }
}

/// Hands each file of a zip held in memory to `each`, by the central directory.
fn zip_entries(
    zip: &[u8],
    mut each: impl FnMut(&str, &mut dyn BufRead) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let u16_at = |i: usize| -> anyhow::Result<usize> {
        let b = zip.get(i..i + 2).context("zip: truncated")?;
        Ok(usize::from(u16::from_le_bytes([b[0], b[1]])))
    };
    let u32_at = |i: usize| -> anyhow::Result<usize> {
        let b = zip.get(i..i + 4).context("zip: truncated")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let search_from = zip.len().saturating_sub(22 + 0xFFFF);
    let end = (search_from..=zip.len().saturating_sub(22))
        .rev()
        .find(|&i| zip.get(i..i + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
        .context("zip: no end record")?;
    let count = u16_at(end + 10)?;
    let mut at = u32_at(end + 16)?;
    for _ in 0..count {
        anyhow::ensure!(
            zip.get(at..at + 4) == Some(&[0x50, 0x4b, 0x01, 0x02]),
            "zip: bad directory entry"
        );
        let method = u16_at(at + 10)?;
        let compressed = u32_at(at + 20)?;
        let (name_len, extra_len, comment_len) =
            (u16_at(at + 28)?, u16_at(at + 30)?, u16_at(at + 32)?);
        let local = u32_at(at + 42)?;
        let name = String::from_utf8_lossy(
            zip.get(at + 46..at + 46 + name_len)
                .context("zip: truncated")?,
        )
        .into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        anyhow::ensure!(
            zip.get(local..local + 4) == Some(&[0x50, 0x4b, 0x03, 0x04]),
            "zip: bad local header"
        );
        let data = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        let raw = zip.get(data..data + compressed).context("zip: truncated")?;
        match method {
            0 => each(&name, &mut { raw })?,
            8 => each(
                &name,
                &mut BufReader::new(flate2::read::DeflateDecoder::new(raw)),
            )?,
            other => bail!("zip: compression method {other} of {name}"),
        }
    }
    Ok(())
}

/// A row of the glyph as 32 bits, its first pixel the highest.
fn row_bits(entry: &Entry, rows: &[u8], y: u32) -> u32 {
    let bytes = usize::from(entry.row_bytes);
    let start = entry.offset as usize + y as usize * bytes;
    let mut row = 0u32;
    for (i, &byte) in rows[start..start + bytes].iter().enumerate() {
        row |= u32::from(byte) << (24 - 8 * i);
    }
    row
}

/// `LineData.calculateWidth`.
fn bitmap_columns(entry: &Entry, rows: &[u8]) -> (i32, i32) {
    let mask = (0..GLYPH_HEIGHT).fold(0u32, |mask, y| mask | row_bits(entry, rows, y));
    if mask == 0 {
        (0, i32::from(entry.row_bytes) * 8)
    } else {
        (
            mask.leading_zeros() as i32,
            32 - mask.trailing_zeros() as i32 - 1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unihex(hex: &str, overrides: &[SizeOverride]) -> Unihex {
        let mut unihex = Unihex::default();
        unihex
            .read_hex(&mut hex.as_bytes(), &mut Vec::new())
            .unwrap();
        unihex.set_widths(overrides);
        unihex
    }

    /// 16 rows of 8 pixels: `row` repeated.
    fn narrow(row: &str) -> String {
        row.repeat(16)
    }

    #[test]
    fn a_glyph_is_as_wide_as_the_columns_its_bitmap_uses() {
        let u = unihex(&format!("0041:{}\n", narrow("38")), &[]);
        assert_eq!(u.width('A'), Some(3));
        let image = u.raster('A').unwrap();
        assert_eq!((image.width(), image.height()), (3, 16));
        assert!(image.pixels().all(|p| p.0[3] == 255));
    }

    #[test]
    fn a_16_pixel_glyph_is_read_left_bit_first() {
        let mut rows = vec!["0000"; 16];
        rows[0] = "8000";
        rows[15] = "0001";
        let u = unihex(&format!("4E00:{}\n", rows.concat()), &[]);
        assert_eq!(u.width('\u{4E00}'), Some(16));
        let image = u.raster('\u{4E00}').unwrap();
        assert_eq!(image.get_pixel(0, 0).0[3], 255);
        assert_eq!(image.get_pixel(15, 15).0[3], 255);
        assert_eq!(image.get_pixel(1, 0).0[3], 0);
    }

    #[test]
    fn an_override_fixes_the_columns_and_widths_give_the_advance() {
        let ranges = [SizeOverride {
            from: '\u{3000}',
            to: '\u{30FF}',
            left: 0,
            right: 15,
        }];
        let u = unihex(&format!("3042:{}\n", "00FF".repeat(16)), &ranges);
        assert_eq!(u.width('\u{3042}'), Some(16));
        assert_eq!(advance(16), 9.0);
        assert_eq!(advance(8), 5.0);
        assert_eq!(advance(5), 3.0);
    }

    #[test]
    fn an_empty_bitmap_spans_its_whole_width() {
        let u = unihex(&format!("0020:{}\n", narrow("00")), &[]);
        assert_eq!(u.width(' '), Some(9));
    }

    #[test]
    fn glyphs_load_from_a_deflated_zip_next_to_other_files() {
        use std::io::Write;
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("LICENSE.txt", options).unwrap();
            writer.write_all(b"not a font").unwrap();
            writer.start_file("unifont.hex", options).unwrap();
            writer
                .write_all(
                    format!("4E2D:{}\n0041:{}\n", "0FF0".repeat(16), narrow("38")).as_bytes(),
                )
                .unwrap();
            writer.finish().unwrap();
        }
        let u = Unihex::from_zip(&bytes, &[]).unwrap();
        assert_eq!(u.len(), 2);
        assert_eq!(u.width('\u{4E2D}'), Some(8));
        assert!(u.contains('A') && !u.contains('B'));
    }

    #[test]
    fn the_real_unifont_loads_and_knows_chinese_japanese_and_korean() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/minecraft/font");
        let Ok(zip) = std::fs::read(root.join("unifont.zip")) else {
            eprintln!("no unifont.zip (Launcher: Prepare): skipped");
            return;
        };
        let overrides = [SizeOverride {
            from: '\u{3200}',
            to: '\u{9FFF}',
            left: 0,
            right: 15,
        }];
        let u = Unihex::from_zip(&zip, &overrides).unwrap();
        assert!(u.len() > 50_000, "{}", u.len());
        assert_eq!(u.width('\u{4E2D}'), Some(16));
        for c in ['中', '文', 'あ', 'カ', '한', 'ع', 'Ж'] {
            assert!(u.contains(c), "{c}");
            let image = u.raster(c).unwrap();
            assert!(image.pixels().any(|p| p.0[3] != 0), "{c} is blank");
        }
        assert!(u.rows.len() < 4_500_000, "{}", u.rows.len());
    }

    #[test]
    fn malformed_lines_are_refused() {
        let mut u = Unihex::default();
        assert!(u.read_hex(&mut &b"41:00\n"[..], &mut Vec::new()).is_err());
        assert!(u.read_hex(&mut &b"0041:00\n"[..], &mut Vec::new()).is_err());
        assert!(u.read_hex(&mut &b"0041 00\n"[..], &mut Vec::new()).is_err());
    }
}
