//! Rain and snow columns (vanilla `WeatherEffectRenderer`).

use std::sync::OnceLock;

use crate::renderer::WorldVertex;
use crate::world::java_random::JavaRandom;

const TABLE: usize = 32;
const HALF: i32 = 16;

/// Rain or snow at one XZ column.
#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub x: i32,
    pub z: i32,
    pub bottom_y: i32,
    pub top_y: i32,
    pub u_offset: f32,
    pub v_offset: f32,
    pub block_light: u8,
    pub sky_light: u8,
    pub snow: bool,
}

/// What [`columns`] needs of the world.
#[derive(Clone, Copy)]
pub struct WeatherQuery<'a> {
    pub intensity: f32,
    pub radius: i32,
    pub camera: glam::DVec3,
    pub game_time: i64,
    pub partial_tick: f32,
    pub height: &'a dyn Fn(i32, i32) -> i32,
    pub precipitation: &'a dyn Fn(i32, i32, i32) -> Precipitation,
    pub light: &'a dyn Fn(i32, i32, i32) -> (u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

/// Last `RAIN_LEVEL_CHANGE` / `THUNDER_LEVEL_CHANGE` the client applied.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeatherLevels {
    pub rain: f32,
    pub thunder: f32,
}

impl WeatherLevels {
    pub fn set_rain(&mut self, level: f32) {
        self.rain = level.clamp(0.0, 1.0);
    }

    pub fn set_thunder(&mut self, level: f32) {
        self.thunder = level.clamp(0.0, 1.0);
    }
}

/// A column's part that holds while the camera stays in its block.
#[derive(Clone, Copy, Debug)]
pub struct Prepared {
    x: i32,
    z: i32,
    bottom_y: i32,
    top_y: i32,
    fall: Fall,
    block_light: u8,
    sky_light: u8,
}

#[derive(Clone, Copy, Debug)]
enum Fall {
    Rain {
        tick_offset: i32,
        speed: f32,
    },
    /// `u = a + time * 0.01 * b`, `v = c + time * d * 0.001`.
    Snow {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
    },
}

impl Prepared {
    /// This column's offsets at `game_time` and `partial_tick`.
    pub fn column(&self, game_time: i64, partial_tick: f32) -> Column {
        let ticks = game_time & 131_071;
        let (u_offset, v_offset, snow) = match self.fall {
            Fall::Rain { tick_offset, speed } => {
                let texture_offset =
                    -(ticks as f32 + tick_offset as f32 + partial_tick) / 32.0 * speed;
                (0.0, texture_offset % 32.0, false)
            }
            Fall::Snow { a, b, c, d } => {
                let time = ticks as f32 + partial_tick;
                let u = a + time * 0.01 * b;
                let v = c + time * d * 0.001;
                let v_offset = -((game_time & 511) as f32 + partial_tick) / 512.0;
                (u, v_offset + v, true)
            }
        };
        Column {
            x: self.x,
            z: self.z,
            bottom_y: self.bottom_y,
            top_y: self.top_y,
            u_offset,
            v_offset,
            block_light: self.block_light,
            sky_light: self.sky_light,
            snow,
        }
    }
}

/// What [`prepare`]'s result stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrepareKey {
    pub block: [i32; 3],
    pub revision: u64,
    pub radius: i32,
}

/// The world's side of the columns around the camera.
pub fn prepare(query: &WeatherQuery<'_>) -> Vec<Prepared> {
    if query.radius <= 0 {
        return Vec::new();
    }
    let cam_x = query.camera.x.floor() as i32;
    let cam_y = query.camera.y.floor() as i32;
    let cam_z = query.camera.z.floor() as i32;
    let r = query.radius;
    let mut out = Vec::new();
    for z in (cam_z - r)..=(cam_z + r) {
        for x in (cam_x - r)..=(cam_x + r) {
            let terrain = (query.height)(x, z);
            let y0 = (cam_y - r).max(terrain);
            let y1 = (cam_y + r).max(terrain);
            if y1 == y0 {
                continue;
            }
            let kind = (query.precipitation)(x, cam_y, z);
            if kind == Precipitation::None {
                continue;
            }
            let mut rng = JavaRandom::with_seed(column_seed(x, z));
            let light_y = cam_y.max(terrain);
            let (block, sky) = (query.light)(x, light_y, z);
            let (fall, block, sky) = match kind {
                Precipitation::Rain => (
                    Fall::Rain {
                        tick_offset: rain_tick_offset(x, z),
                        speed: 3.0 + rng.next_float(),
                    },
                    block,
                    sky,
                ),
                Precipitation::Snow => {
                    let a = rng.next_double() as f32;
                    let b = rng.next_gaussian() as f32;
                    let c = rng.next_double() as f32;
                    let d = rng.next_gaussian() as f32;
                    let block = ((i32::from(block) * 3 + 15) / 4).clamp(0, 15) as u8;
                    let sky = ((i32::from(sky) * 3 + 15) / 4).clamp(0, 15) as u8;
                    (Fall::Snow { a, b, c, d }, block, sky)
                }
                Precipitation::None => continue,
            };
            out.push(Prepared {
                x,
                z,
                bottom_y: y0,
                top_y: y1,
                fall,
                block_light: block,
                sky_light: sky,
            });
        }
    }
    out
}

/// Seeded columns around the camera.
pub fn columns(query: WeatherQuery<'_>) -> Vec<Column> {
    if query.intensity <= 0.0 {
        return Vec::new();
    }
    prepare(&query)
        .iter()
        .map(|p| p.column(query.game_time, query.partial_tick))
        .collect()
}

/// `x * x * 3121 + x * 45238971 ^ z * z * 418711 + z * 13761` as Java ints.
pub fn column_seed(x: i32, z: i32) -> i64 {
    let left = x
        .wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971));
    let right = z
        .wrapping_mul(z)
        .wrapping_mul(418_711)
        .wrapping_add(z.wrapping_mul(13_761));
    i64::from(left ^ right)
}

fn rain_tick_offset(x: i32, z: i32) -> i32 {
    x.wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(418_711))
        .wrapping_add(z.wrapping_mul(13_761))
        & 0xFF
}

/// One drop streak of a weather texture: texel `column`, rows
/// `top..bottom`, every texel in between not clear. Everything around them
/// is clear, so only they need drawing: ~2.5% of `rain.png`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Streak {
    pub column: u32,
    pub top: u32,
    pub bottom: u32,
}

/// The streaks of `image`: each texel column's runs of texels with alpha.
pub fn streaks(image: &image::RgbaImage) -> Vec<Streak> {
    let (width, height) = image.dimensions();
    let mut out = Vec::new();
    for column in 0..width {
        let mut top = None;
        for row in 0..=height {
            let drawn = row < height && image.get_pixel(column, row)[3] > 0;
            match (drawn, top) {
                (true, None) => top = Some(row),
                (false, Some(start)) => {
                    out.push(Streak {
                        column,
                        top: start,
                        bottom: row,
                    });
                    top = None;
                }
                _ => {}
            }
        }
    }
    out
}

/// The streaks as quads for `weather.vert`, twice: first as they are, then
/// copies for a streak that wraps round the column's edge under a
/// fractional `u` (only snow's; [`Pieces::wraps`]). `pos` is the texel
/// column and the rows as fractions of the height, `uv.x` one texel's
/// width, `light` the corner (`0..4`) and the copy.
pub fn template(streaks: &[Streak], width: u32, height: u32) -> Vec<WorldVertex> {
    let mut out = Vec::with_capacity(streaks.len() * 8);
    let (w, h) = (width as f32, height as f32);
    for copy in 0..2 {
        for streak in streaks {
            let pos = [
                streak.column as f32,
                streak.top as f32 / h,
                streak.bottom as f32 / h,
            ];
            for corner in 0..4 {
                let mut v = WorldVertex::new(pos, [1.0 / w, 0.0], [1.0; 3]);
                v.light = [corner, copy];
                out.push(v);
            }
        }
    }
    out
}

/// One kind's pieces, and whether any needs the wrapped copies.
#[derive(Default)]
pub struct Pieces {
    pub vertices: Vec<WorldVertex>,
    pub wraps: bool,
}

/// Rain and snow as pieces for `weather.vert`, two vertices of per-instance
/// data each: a column (vanilla `WeatherEffectRenderer.renderInstances`'
/// quad, camera-relative) and which repeat `k` of the texture along it the
/// template's streaks are cut from. The texture repeats every 4 blocks, so
/// a column of `top - bottom` blocks takes that / 4 + 1 or 2 repeats.
///
/// First vertex: `pos` centre x, z and half width along x, `uv` half width
/// along z and the bottom, `color` the light and fade. Second: `pos` the
/// top, `u` and the `v` at the top, `uv.x` the repeat. `sees` gets each
/// column's box, camera-relative: what it says no to is left out.
pub fn pieces(
    columns: &[Column],
    camera: glam::DVec3,
    radius: i32,
    intensity: f32,
    lightmap: &crate::world::lightmap::LightmapParams,
    sees: impl Fn(glam::Vec3, glam::Vec3) -> bool,
) -> (Pieces, Pieces) {
    let mut rain = Pieces::default();
    let mut snow = Pieces::default();
    if radius <= 0 {
        return (rain, snow);
    }
    let radius_sq = (radius * radius) as f32;
    let cam_x = camera.x.floor() as i32;
    let cam_z = camera.z.floor() as i32;
    let sizes = column_sizes();
    // Most columns share their light: each pair sampled once.
    let mut lit = [None; 256];
    for column in columns {
        let relative_x = (f64::from(column.x) + 0.5 - camera.x) as f32;
        let relative_z = (f64::from(column.z) + 0.5 - camera.z) as f32;
        let y1 = (f64::from(column.top_y) - camera.y) as f32;
        let y0 = (f64::from(column.bottom_y) - camera.y) as f32;
        let min = glam::Vec3::new(relative_x - 1.0, y0, relative_z - 1.0);
        let max = glam::Vec3::new(relative_x + 1.0, y1, relative_z + 1.0);
        if !sees(min, max) {
            continue;
        }
        let dist_sq = relative_x * relative_x + relative_z * relative_z;
        let max_alpha = if column.snow { 0.8 } else { 1.0 };
        let t = (dist_sq / radius_sq).min(1.0);
        let alpha = (max_alpha + (0.5 - max_alpha) * t) * intensity;
        let idx = ((column.z - cam_z + HALF) * TABLE as i32 + (column.x - cam_x + HALF)) as usize;
        let (half_x, half_z) = sizes.get(idx).copied().unwrap_or((0.5, 0.0));
        // `v` grows downwards: `v0` at the top, `v1` at the bottom.
        let v0 = column.bottom_y as f32 * 0.25 + column.v_offset;
        let v1 = column.top_y as f32 * 0.25 + column.v_offset;
        let pair = usize::from(column.block_light & 15) | usize::from(column.sky_light & 15) << 4;
        let [lr, lg, lb] = *lit[pair].get_or_insert_with(|| {
            crate::world::lightmap::sample(column.block_light, column.sky_light, lightmap)
        });
        let place = WorldVertex::rgba(
            [relative_x, relative_z, half_x],
            [half_z, y0],
            [lr, lg, lb, alpha],
        );
        let out = if column.snow { &mut snow } else { &mut rain };
        // Repeat `k` holds `v` in `k..k + 1`, 4 blocks of the column each.
        // (Culling each piece as well halved the vertices but not the GPU
        // time, and cost more CPU than the whole column test.)
        for k in v0.floor() as i32..v1.ceil() as i32 {
            let span = WorldVertex::new([y1, column.u_offset, v0], [k as f32, 0.0], [1.0; 3]);
            out.vertices.extend([place, span]);
        }
        out.wraps |= column.u_offset.fract() != 0.0;
    }
    (rain, snow)
}

fn column_sizes() -> &'static [(f32, f32); TABLE * TABLE] {
    static SIZES: OnceLock<[(f32, f32); TABLE * TABLE]> = OnceLock::new();
    SIZES.get_or_init(|| {
        let mut out = [(0.0, 0.0); TABLE * TABLE];
        for z in 0..TABLE {
            for x in 0..TABLE {
                let dx = x as f32 - 16.0;
                let dz = z as f32 - 16.0;
                let len = (dx * dx + dz * dz).sqrt().max(1e-6);
                out[z * TABLE + x] = ((-dz / len) * 0.5, (dx / len) * 0.5);
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_keeps_its_vanilla_seed() {
        let left = 3 * 3 * 3121 + 3 * 45_238_971;
        let right = 5 * 5 * 418_711 + 5 * 13_761;
        assert_eq!(column_seed(3, 5), i64::from(left ^ right));
    }

    #[test]
    fn the_same_query_rebuilds_the_same_columns() {
        let height = |_, _| 64;
        let precip = |_, _, _| Precipitation::Rain;
        let light = |_, _, _| (0u8, 15u8);
        let query = WeatherQuery {
            intensity: 1.0,
            radius: 2,
            camera: glam::DVec3::new(8.2, 70.0, 8.4),
            game_time: 2400,
            partial_tick: 0.25,
            height: &height,
            precipitation: &precip,
            light: &light,
        };
        let a = columns(query);
        let b = columns(WeatherQuery {
            height: &height,
            precipitation: &precip,
            light: &light,
            ..query
        });
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0].v_offset, b[0].v_offset);
    }

    #[test]
    fn prepared_columns_move_with_time_only() {
        let height = |_, _| 64;
        let precip = |x, _, _| {
            if x % 2 == 0 {
                Precipitation::Rain
            } else {
                Precipitation::Snow
            }
        };
        let light = |_, _, _| (3u8, 12u8);
        let query = WeatherQuery {
            intensity: 1.0,
            radius: 3,
            camera: glam::DVec3::new(8.2, 70.0, 8.4),
            game_time: 0,
            partial_tick: 0.0,
            height: &height,
            precipitation: &precip,
            light: &light,
        };
        let prepared = prepare(&query);
        assert_eq!(prepared.len(), columns(query).len());
        for p in &prepared {
            let (a, b) = (p.column(100, 0.5), p.column(101, 0.5));
            assert_eq!(
                (a.x, a.z, a.bottom_y, a.top_y),
                (b.x, b.z, b.bottom_y, b.top_y)
            );
            assert_ne!(a.v_offset, b.v_offset);
        }
    }

    #[test]
    fn the_lightmap_goes_into_the_colour() {
        let column = Column {
            x: 11,
            z: 8,
            bottom_y: 60,
            top_y: 80,
            u_offset: 0.0,
            v_offset: 0.0,
            block_light: 0,
            sky_light: 0,
            snow: false,
        };
        let params = crate::world::lightmap::LightmapParams::overworld([1.0; 3], 0.0);
        let camera = glam::DVec3::new(8.2, 70.0, 8.4);
        let dark = pieces(&[column], camera, 8, 1.0, &params, |_, _| true)
            .0
            .vertices;
        let lit = pieces(
            &[Column {
                sky_light: 15,
                ..column
            }],
            camera,
            8,
            1.0,
            &params,
            |_, _| true,
        )
        .0
        .vertices;
        // 20 blocks from v 15: repeats 15 to 19, two vertices each.
        assert_eq!(dark.len(), 10);
        assert!(lit[0].color[0] > dark[0].color[0]);
    }

    /// `weather.vert` for one streak copy on one piece: the strip's `s` and
    /// `v` ranges, `None` where it has nothing.
    fn strip(template: &[WorldVertex], piece: [WorldVertex; 2]) -> Option<([f32; 2], [f32; 2])> {
        let t = template[0];
        let [column, row_top, row_bottom] = t.pos;
        let width = t.uv[0];
        let bottom = piece[0].uv[1];
        let [top, u_offset, v_top] = piece[1].pos;
        let k = piece[1].uv[0];
        let s_lo = (column * width - u_offset).rem_euclid(1.0);
        let s_hi = s_lo + width;
        let s = if t.light[1] == 0 {
            [s_lo, s_hi.min(1.0)]
        } else {
            [0.0, s_hi - 1.0]
        };
        let v_bottom = v_top + (top - bottom) * 0.25;
        let v = [(k + row_top).max(v_top), (k + row_bottom).min(v_bottom)];
        (s[1] > s[0] && v[1] > v[0]).then_some((s, v))
    }

    #[test]
    fn the_streaks_cover_every_drawn_texel_of_the_column_once() {
        // A texture of 8 by 16 with a few streaks, one down a whole column.
        let mut image = image::RgbaImage::new(8, 16);
        for (x, rows) in [
            (0, 0..16),
            (2, 3..5),
            (2, 9..10),
            (5, 0..2),
            (5, 14..16),
            (7, 6..12),
        ] {
            for y in rows {
                image.put_pixel(x, y, image::Rgba([255, 255, 255, 200]));
            }
        }
        let streaks = streaks(&image);
        assert_eq!(streaks.len(), 6);
        let template = template(&streaks, 8, 16);
        let params = crate::world::lightmap::LightmapParams::overworld([1.0; 3], 0.0);
        let camera = glam::DVec3::new(3.3, 70.6, -2.2);
        for (u_offset, v_offset) in [(0.0, 0.0), (0.37, -5.81), (0.9, 12.3)] {
            let column = Column {
                x: 4,
                z: -1,
                bottom_y: 63,
                top_y: 80,
                u_offset,
                v_offset,
                block_light: 0,
                sky_light: 15,
                snow: false,
            };
            let rain = pieces(&[column], camera, 10, 1.0, &params, |_, _| true).0;
            assert_eq!(rain.wraps, u_offset != 0.0);
            let pieces = rain.vertices;
            let v0 = column.bottom_y as f32 * 0.25 + v_offset;
            let v1 = column.top_y as f32 * 0.25 + v_offset;
            // Sample the old quad's (s, v) on a grid between texel edges.
            for i in 0..97 {
                let s = (i as f32 + 0.5) / 97.0;
                for j in 0..301 {
                    let v = v0 + (v1 - v0) * (j as f32 + 0.5) / 301.0;
                    let u = u_offset + s;
                    let texel = (
                        ((u.rem_euclid(1.0)) * 8.0) as u32,
                        ((v.rem_euclid(1.0)) * 16.0) as u32,
                    );
                    let drawn = image.get_pixel(texel.0, texel.1)[3] > 0;
                    let mut covered = 0;
                    for piece in pieces.as_chunks::<2>().0 {
                        for quad in template.as_chunks::<4>().0 {
                            if let Some((ss, vv)) = strip(quad, *piece)
                                && (ss[0]..ss[1]).contains(&s)
                                && (vv[0]..vv[1]).contains(&v)
                            {
                                covered += 1;
                            }
                        }
                    }
                    assert_eq!(
                        covered,
                        usize::from(drawn),
                        "u offset {u_offset}, s {s}, v {v}: texel {texel:?}"
                    );
                }
            }
        }
    }
}
