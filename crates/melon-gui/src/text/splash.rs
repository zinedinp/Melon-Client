//! Vanilla `SplashManager` / `SplashRenderer` (rotated yellow text by the logo).

use melon_assets::assets::Assets;
use melon_core::math::angle::{self, PI, TAU};
use melon_render::quad::Transform2D;
use melon_world::java_random::java_string_hash;
use rand::RngExt;

const FILTERED_HASH: i32 = 125780783;
const WIDTH_OFFSET: f32 = 123.0;
const HEIGHT_OFFSET: f32 = 69.0;
const TEXT_ANGLE: f32 = -PI / 9.0;

pub fn load_splashes(assets: &Assets) -> Vec<String> {
    let Some(bytes) = melon_assets::assets::resource(assets.root(), "texts", "splashes", "txt")
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        .map(str::to_string)
        .filter(|line| !line.is_empty() && java_string_hash(line) != FILTERED_HASH)
        .collect()
}

/// Random splash. Vanilla 1-in-N `"<PLAYERNAME> IS YOU"` when `username` is set.
pub fn pick_splash(splashes: &[String], username: Option<&str>) -> String {
    if splashes.is_empty() {
        return String::new();
    }
    let mut rng = rand::rng();
    let index = rng.random_range(0..splashes.len());
    if let Some(name) = username
        && index == 42
    {
        return format!("{} IS YOU", name.to_uppercase());
    }
    splashes[index].clone()
}

/// Vanilla `1.8F - |sin(t)| * 0.1F` pulse (one cycle per second).
pub fn transform(virtual_width: f32, text_width: f32, time_ms: u128) -> Transform2D {
    let t = (time_ms % 1000) as f32 / 1000.0;
    let phase = 1.8 - angle::sin(t * TAU).abs() * 0.1;
    let scale = phase * 100.0 / (text_width + 32.0);
    Transform2D {
        translate: [virtual_width / 2.0 + WIDTH_OFFSET, HEIGHT_OFFSET],
        sin_cos: angle::sin_cos(TEXT_ANGLE),
        scale,
    }
}
