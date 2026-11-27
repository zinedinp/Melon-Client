//! Vanilla `PlayerFaceExtractor`.

use melon_render::TextureId;
use melon_render::quad::DrawList;

/// Draws `skin`'s face `size` pixels square at `[x, y]`, with the hat on top.
pub fn draw_face(
    draw_list: &mut DrawList,
    skin: TextureId,
    [x, y]: [f32; 2],
    size: f32,
    hat: bool,
    flip: bool,
    tint: [f32; 4],
) {
    let (v0, v1) = if flip { (16.0, 8.0) } else { (8.0, 16.0) };
    let uv = |u: f32| [u / 64.0, v0 / 64.0, (u + 8.0) / 64.0, v1 / 64.0];
    draw_list.push_rect(skin, [x, y, size, size], uv(8.0), tint);
    if hat {
        draw_list.push_rect(skin, [x, y, size, size], uv(40.0), tint);
    }
}
