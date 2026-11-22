//! Fallback menu background when the panorama cubemap is missing.

use crate::gui::assets::GuiAssets;
use crate::math::argb;
use crate::renderer::TextureId;
use crate::renderer::quad::DrawList;

/// Render-pass clear color. Hidden once a cubemap scene is present.
pub const CLEAR_COLOR: [f32; 4] = [0.02, 0.02, 0.04, 1.0];

/// Flat launcher chrome: one dark green-grey, no white list plates.
pub const LAUNCHER: [f32; 4] = [0.12, 0.14, 0.13, 1.0];
/// Column panels (instances, middle, accounts).
pub const LAUNCHER_PANEL: [f32; 4] = [0.09, 0.11, 0.10, 1.0];
pub const LAUNCHER_EDGE: [f32; 4] = [0.34, 0.42, 0.32, 1.0];
/// Selected / hovered launcher rows: a light veil, not an opaque tile.
pub const LAUNCHER_SELECT: [f32; 4] = [1.0, 1.0, 1.0, 0.10];
pub const LAUNCHER_HOVER: [f32; 4] = [1.0, 1.0, 1.0, 0.06];

pub fn push_launcher(draw_list: &mut DrawList, white: TextureId, width: f32, height: f32) {
    draw_list.push_rect(
        white,
        0.0,
        0.0,
        width,
        height,
        [0.0, 0.0, 1.0, 1.0],
        LAUNCHER,
    );
}

pub fn push_gradient(draw_list: &mut DrawList, white: TextureId, width: f32, height: f32) {
    draw_list.push_vgradient(
        white,
        0.0,
        0.0,
        width,
        height,
        [0.09, 0.09, 0.14, 1.0],
        [0.02, 0.02, 0.04, 1.0],
    );
}

/// `Screen.extractTransparentBackground`.
pub fn push_transparent_background(
    draw_list: &mut DrawList,
    white: TextureId,
    width: f32,
    height: f32,
) {
    draw_list.push_vgradient(
        white,
        0.0,
        0.0,
        width,
        height,
        argb(0xC010_1010),
        argb(0xD010_1010),
    );
}

/// `Screen.extractBackground` on every non-`TitleScreen` menu.
pub fn push_menu_overlay(draw_list: &mut DrawList, white: TextureId, width: f32, height: f32) {
    push_menu_overlay_rect(draw_list, white, 0.0, 0.0, width, height);
}

/// `Screen.extractMenuBackground(graphics, x, y, width, height)`.
pub fn push_menu_overlay_rect(
    draw_list: &mut DrawList,
    white: TextureId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    draw_list.push_rect(
        white,
        x,
        y,
        width,
        height,
        [0.0, 0.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 64.0 / 255.0],
    );
}

/// `menu_list_background.png`'s tile size.
const LIST_TILE: f32 = 16.0;
/// `Screen.HEADER_SEPARATOR`/`FOOTER_SEPARATOR`: 32 wide, tiled not stretched.
const SEPARATOR_TILE: f32 = 32.0;

/// `AbstractScrollArea.extractListBackground` and `extractListSeparators`:
pub fn push_list_background(
    draw_list: &mut DrawList,
    assets: &GuiAssets,
    top: f32,
    bottom: f32,
    width: f32,
) {
    if assets.launcher {
        return;
    }
    let height = (bottom - top).max(0.0);
    draw_list.push_rect(
        assets.list_background,
        0.0,
        top,
        width,
        height,
        [0.0, 0.0, width / LIST_TILE, height / LIST_TILE],
        [1.0; 4],
    );
    draw_list.push_rect_tiled_x(
        assets.header_separator,
        0.0,
        top - 2.0,
        width,
        2.0,
        SEPARATOR_TILE,
        [1.0; 4],
    );
    draw_list.push_rect_tiled_x(
        assets.footer_separator,
        0.0,
        bottom,
        width,
        2.0,
        SEPARATOR_TILE,
        [1.0; 4],
    );
}
