//! Screen effects.

use melon_core::math::angle::{self, PI};
use melon_core::math::argb;
use melon_render::quad::{BlendMode, DrawList};
use melon_render::{Renderer, TextureId};
use melon_assets::assets::Assets;

/// The field of view the hand and these overlays are drawn with.
const HUD_FOV_DEGREES: f32 = 70.0;
/// How far in front of the camera their quads lie.
const OVERLAY_DEPTH: f32 = 0.5;

/// The textures the effects draw with.
pub struct EffectTextures {
    /// `textures/misc/vignette.png`, sampled linearly.
    vignette: TextureId,
    /// `textures/misc/underwater.png`, repeating.
    underwater: TextureId,
    /// `textures/misc/powder_snow_outline.png`.
    powder_snow: TextureId,
    /// `textures/misc/nausea.png`, sampled linearly (`blur: true`).
    nausea: TextureId,
}

impl EffectTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        Ok(Self {
            vignette: renderer
                .load_texture_linear(&assets.image("minecraft:misc/vignette.png")?)?,
            underwater: renderer
                .load_texture_tiled(&assets.image("minecraft:misc/underwater.png")?)?,
            powder_snow: renderer
                .load_texture_linear(&assets.image("minecraft:misc/powder_snow_outline.png")?)?,
            nausea: renderer.load_texture_linear(&assets.image("minecraft:misc/nausea.png")?)?,
        })
    }
}

/// A sprite of the block atlas, by its UV rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasSprite {
    pub atlas: TextureId,
    /// `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
}

/// The vignette's inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vignette {
    /// `Hud.vignetteBrightness`: how dark the place is, `0..=1`.
    pub brightness: f32,
    /// How close the world border is, `0..=1`: tints the vignette red.
    pub border_warning: f32,
}

/// `PlayerRenderState.WaterOverlay`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underwater {
    /// `ARGB.colorFromFloat(0.1, b, b, b)` of the light at the eyes.
    pub color: [f32; 4],
    /// `-yRot / 64` and `xRot / 64`: the texture slides as the head turns.
    pub u_offset: f32,
    pub v_offset: f32,
}

/// What lies over the view this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScreenEffects {
    /// A block the camera is inside of, seen from within.
    pub wall: Option<AtlasSprite>,
    pub underwater: Option<Underwater>,
    /// The fire's sprite while the camera burns.
    pub fire: Option<AtlasSprite>,
    pub vignette: Option<Vignette>,
    /// `Player.getPercentFrozen`, `0..=1`.
    pub frozen: f32,
    /// The nether portal's strength `0..=1` and its sprite.
    pub portal: f32,
    pub portal_sprite: Option<AtlasSprite>,
    /// `Hud.extractConfusionOverlay` strength, `0` to skip.
    pub nausea: f32,
}

impl ScreenEffects {
    /// Draws them in vanilla's order: the hand pass's overlays, then the
    /// `hud`: the HUD is shown (not hidden with F1); without it only the
    /// hand pass's overlays (inside a block, under water, on fire) draw,
    /// as `Hud.extractCameraOverlays` is skipped then.
    pub fn draw(
        &self,
        list: &mut DrawList,
        textures: &EffectTextures,
        size: (f32, f32),
        hud: bool,
    ) {
        let (width, height) = size;
        if let Some(wall) = self.wall {
            fill_sprite(list, wall, size, argb(0xFF19_1919));
        }
        if let Some(water) = self.underwater {
            underwater(list, textures.underwater, size, water);
        }
        if let Some(fire) = self.fire {
            fire_overlay(list, fire, size);
        }
        if !hud {
            return;
        }
        if let Some(color) = self.vignette.map(vignette_color).filter(|&c| darkens(c)) {
            list.set_blend(BlendMode::Vignette);
            list.push_rect(
                textures.vignette,
                [0.0, 0.0, width, height],
                [0.0, 0.0, 1.0, 1.0],
                color,
            );
            list.set_blend(BlendMode::Alpha);
        }
        if self.frozen > 0.0 {
            list.push_rect(
                textures.powder_snow,
                [0.0, 0.0, width, height],
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, self.frozen.clamp(0.0, 1.0)],
            );
        }
        if self.portal > 0.0
            && let Some(sprite) = self.portal_sprite
        {
            fill_sprite(
                list,
                sprite,
                size,
                [1.0, 1.0, 1.0, portal_alpha(self.portal)],
            );
        } else if self.nausea > 0.0 {
            nausea_overlay(list, textures.nausea, size, self.nausea);
        }
    }
}

/// `Hud.extractVignette`'s colour.
pub fn vignette_color(v: Vignette) -> [f32; 4] {
    let brightness = v.brightness.clamp(0.0, 1.0);
    if v.border_warning > 0.0 {
        let warning = v.border_warning.clamp(0.0, 1.0);
        let red = brightness * (1.0 - warning);
        let green_blue = brightness + (1.0 - brightness) * warning;
        [red, green_blue, green_blue, 1.0]
    } else {
        [brightness, brightness, brightness, 1.0]
    }
}

/// Whether a vignette of `color` changes any pixel. It scales the screen by
/// `1 - texel * color`, so under half an 8-bit step it changes nothing, and
/// in daylight vanilla draws it over the whole screen for nothing.
fn darkens(color: [f32; 4]) -> bool {
    color[..3].iter().any(|&c| c * 255.0 >= 0.5)
}

/// `Hud.extractPortalOverlay`'s alpha: rising steeply at the end.
pub fn portal_alpha(intensity: f32) -> f32 {
    if intensity < 1.0 {
        let squared = intensity * intensity;
        squared * squared * 0.8 + 0.2
    } else {
        intensity
    }
}

/// The whole screen with a sprite, tinted.
fn fill_sprite(list: &mut DrawList, sprite: AtlasSprite, size: (f32, f32), color: [f32; 4]) {
    list.push_rect(sprite.atlas, [0.0, 0.0, size.0, size.1], sprite.uv, color);
}

/// Where a point of a quad lying `depth` in front of the camera, at `(x, y)`
fn project(x: f32, y: f32, depth: f32, size: (f32, f32)) -> [f32; 2] {
    let tan = (HUD_FOV_DEGREES.to_radians() / 2.0).tan();
    let aspect = size.0 / size.1;
    [
        size.0 / 2.0 * (1.0 + x / (depth * tan * aspect)),
        size.1 / 2.0 * (1.0 - y / (depth * tan)),
    ]
}

/// `ScreenEffectRenderer.submitWater`.
fn underwater(list: &mut DrawList, texture: TextureId, size: (f32, f32), water: Underwater) {
    let tan = (HUD_FOV_DEGREES.to_radians() / 2.0).tan();
    let half_height = OVERLAY_DEPTH * tan;
    let half_width = half_height * size.0 / size.1;
    let (u, v) = (water.u_offset, water.v_offset);
    let u_left = u + 2.0 + 2.0 * half_width;
    let u_right = u + 2.0 - 2.0 * half_width;
    let v_top = v + 2.0 - 2.0 * half_height;
    let v_bottom = v + 2.0 + 2.0 * half_height;
    list.push_quad(
        texture,
        [[0.0, 0.0], [size.0, 0.0], [size.0, size.1], [0.0, size.1]],
        [
            [u_left, v_top],
            [u_right, v_top],
            [u_right, v_bottom],
            [u_left, v_bottom],
        ],
        water.color,
    );
}

/// `ScreenEffectRenderer.submitFire`.
fn fire_overlay(list: &mut DrawList, fire: AtlasSprite, size: (f32, f32)) {
    let [u0, v0, u1, v1] = fire.uv;
    let color = argb(0xE5FF_FFFF);
    for (side, angle) in [(0.24f32, -PI / 18.0), (-0.24, PI / 18.0)] {
        let (sin, cos) = angle::sin_cos(angle);
        let corner = |x: f32, y: f32| {
            let (turned_x, turned_z) = (
                x * cos + -OVERLAY_DEPTH * sin,
                -x * sin + -OVERLAY_DEPTH * cos,
            );
            project(turned_x + side, y - 0.3, -turned_z, size)
        };
        list.push_quad(
            fire.atlas,
            [
                corner(-0.5, -0.5),
                corner(0.5, -0.5),
                corner(0.5, 0.5),
                corner(-0.5, 0.5),
            ],
            [[u1, v1], [u0, v1], [u0, v0], [u1, v0]],
            color,
        );
    }
}

/// `Hud.extractConfusionOverlay`.
fn nausea_overlay(list: &mut DrawList, texture: TextureId, size: (f32, f32), strength: f32) {
    let strength = strength.clamp(0.0, 1.0);
    let [x0, y0, x1, y1] = nausea_rect(size, strength);
    list.set_blend(BlendMode::Additive);
    list.push_rect(
        texture,
        [x0, y0, x1 - x0, y1 - y0],
        [0.0, 0.0, 1.0, 1.0],
        [0.2 * strength, 0.4 * strength, 0.2 * strength, 1.0],
    );
    list.set_blend(BlendMode::Alpha);
}

/// Screen-space box of the nausea overlay after vanilla's centre scale.
pub fn nausea_rect(size: (f32, f32), strength: f32) -> [f32; 4] {
    let scale = 2.0 + (1.0 - 2.0) * strength.clamp(0.0, 1.0);
    let (width, height) = size;
    let cx = width * 0.5;
    let cy = height * 0.5;
    [
        cx + (0.0 - cx) * scale,
        cy + (0.0 - cy) * scale,
        cx + (width - cx) * scale,
        cy + (height - cy) * scale,
    ]
}

/// `extractDeathBackground`.
pub fn death_veil(list: &mut DrawList, white: TextureId, size: (f32, f32)) {
    veil(list, white, size, argb(0x6050_0000), argb(0xA080_3030));
}

/// A vertical gradient across the whole screen, from `top` to `bottom`
pub fn veil(
    list: &mut DrawList,
    white: TextureId,
    size: (f32, f32),
    top: [f32; 4],
    bottom: [f32; 4],
) {
    list.push_vgradient(white, [0.0, 0.0, size.0, size.1], top, bottom);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vignette_greys_with_the_darkness_and_reddens_nothing_but_near_the_border() {
        let dark = vignette_color(Vignette {
            brightness: 0.8,
            border_warning: 0.0,
        });
        assert_eq!(dark, [0.8, 0.8, 0.8, 1.0]);
        let border = vignette_color(Vignette {
            brightness: 0.2,
            border_warning: 1.0,
        });
        assert_eq!(border, [0.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn a_vignette_that_changes_no_pixel_is_left_out() {
        let at = |brightness, border_warning| {
            darkens(vignette_color(Vignette {
                brightness,
                border_warning,
            }))
        };
        assert!(!at(0.0, 0.0));
        assert!(!at(0.0015, 0.0));
        assert!(at(0.002, 0.0));
        assert!(at(0.0, 0.5));
    }

    #[test]
    fn the_portal_overlay_starts_at_a_fifth_and_climbs() {
        assert!((portal_alpha(0.0) - 0.2).abs() < 1e-6);
        assert!(portal_alpha(0.5) < 0.3);
        assert!((portal_alpha(0.9) - (0.9f32.powi(4) * 0.8 + 0.2)).abs() < 1e-6);
        assert_eq!(portal_alpha(1.0), 1.0);
    }

    #[test]
    fn the_nausea_overlay_fills_the_screen_at_full_strength() {
        assert_eq!(nausea_rect((200.0, 100.0), 1.0), [0.0, 0.0, 200.0, 100.0]);
        let wide = nausea_rect((200.0, 100.0), 0.0);
        assert!((wide[0] - -100.0).abs() < 1e-4);
        assert!((wide[2] - 300.0).abs() < 1e-4);
    }

    #[test]
    fn the_centre_of_the_view_projects_to_the_centre_of_the_screen() {
        let [x, y] = project(0.0, 0.0, 0.5, (320.0, 240.0));
        assert_eq!((x, y), (160.0, 120.0));
        assert!(project(0.0, 0.1, 0.5, (320.0, 240.0))[1] < 120.0);
        let top = project(0.0, 0.5 * (35f32.to_radians()).tan(), 0.5, (320.0, 240.0));
        assert!(top[1].abs() < 1e-3);
    }

    #[test]
    fn the_fire_stands_at_both_sides_and_low() {
        let mut list = DrawList::new();
        fire_overlay(
            &mut list,
            AtlasSprite {
                atlas: TextureId(1),
                uv: [0.0, 0.0, 0.1, 0.1],
            },
            (320.0, 240.0),
        );
        assert_eq!(list.vertices_len(), 8);
    }
}
