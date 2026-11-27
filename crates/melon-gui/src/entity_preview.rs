//! An entity drawn into a box of a screen, turned toward the mouse.

use glam::{Mat3, Mat4, Vec3};
use image::RgbaImage;

use crate::math::angle::{PI, Rotation};
use crate::world::entity::model::{Model, ModelPose, PartId};

/// Where an entity is shown and where the mouse is, in GUI pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityPreview {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// `size`: GUI pixels per block.
    pub size: f32,
    /// `offsetY`, in blocks.
    pub offset_y: f32,
    pub mouse: (f32, f32),
    /// The tracked entity to show instead of the own player (a mount's preview).
    pub entity: Option<i32>,
}

impl EntityPreview {
    /// `xAngle`/`yAngle`: how far the mouse is beside and above the box's centre.
    pub fn angles(&self) -> (f32, f32) {
        let center_x = (self.x0 + self.x1) / 2.0;
        let center_y = (self.y0 + self.y1) / 2.0;
        (
            ((center_x - self.mouse.0) / 40.0).atan(),
            ((center_y - self.mouse.1) / 40.0).atan(),
        )
    }

    /// The texture's size at `gui_scale`.
    pub fn pixels(&self, gui_scale: f32) -> (u32, u32) {
        (
            ((self.x1 - self.x0) * gui_scale).round().max(1.0) as u32,
            ((self.y1 - self.y0) * gui_scale).round().max(1.0) as u32,
        )
    }

    /// From the entity's world space (feet at the origin) to the texture's clip space.
    pub fn view(&self, gui_scale: f32, height: f32) -> Mat4 {
        let (width_px, height_px) = self.pixels(gui_scale);
        let (_, y_angle) = self.angles();
        let scale = gui_scale * self.size;
        Mat4::from_translation(Vec3::new(
            width_px as f32 / 2.0,
            height_px as f32 / 2.0,
            0.0,
        )) * Mat4::from_scale(Vec3::new(scale, scale, -scale))
            * Mat4::from_translation(Vec3::new(0.0, height / 2.0 + self.offset_y, 0.0))
            * Mat4::rot_z(PI)
            * Mat4::rot_x_deg(y_angle * 20.0)
    }
}

/// `Lighting.INVENTORY_DIFFUSE_LIGHT_0`/`_1`, in the texture's space.
fn inventory_light(normal: Vec3) -> f32 {
    let l0 = Vec3::new(0.2, -1.0, 1.0).normalize();
    let l1 = Vec3::new(-0.2, -1.0, 0.0).normalize();
    (0.4 + 0.6 * (normal.dot(l0).max(0.0) + normal.dot(l1).max(0.0))).min(1.0)
}

/// One model of a picture.
pub struct PreviewLayer<'a> {
    pub model: &'a Model,
    pub pose: &'a ModelPose,
    pub root: Mat4,
    pub texture: &'a RgbaImage,
    pub tint: [u8; 3],
    /// Parts left out (a player's outer skin, drawn in a later pass).
    pub skip: &'a [PartId],
    /// When set, only these parts (the outer skin).
    pub only: Option<&'a [PartId]>,
    /// Keep the previous layer's depth so a translucent overlay tests against it.
    pub share_depth: bool,
    /// Armor glint (`enchanted_glint_armor`), sampled with [`crate::world::glint::ARMOR_SCALE`].
    pub glint: Option<&'a RgbaImage>,
}

/// Draws `model` posed by `pose` through `root` into a `width` x `height`
pub fn rasterize(
    model: &Model,
    pose: &ModelPose,
    root: Mat4,
    texture: &RgbaImage,
    width: u32,
    height: u32,
) -> RgbaImage {
    let layer = PreviewLayer {
        model,
        pose,
        root,
        texture,
        tint: [255; 3],
        skip: &[],
        only: None,
        share_depth: false,
        glint: None,
    };
    rasterize_layers(&[layer], width, height)
}

/// What [`rasterize_layers`] draws `layers` from, hashed: the same
/// signature, the same picture. `None` when a layer shimmers, whose glint
/// moves on every frame.
pub fn signature(layers: &[PreviewLayer], width: u32, height: u32) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    if layers.iter().any(|layer| layer.glint.is_some()) {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (width, height).hash(&mut hasher);
    for layer in layers {
        for matrix in layer.model.part_matrices(layer.pose, layer.root) {
            match matrix {
                Some(matrix) => matrix.to_cols_array().map(f32::to_bits).hash(&mut hasher),
                None => 0u8.hash(&mut hasher),
            }
        }
        std::ptr::from_ref(layer.model).hash(&mut hasher);
        layer.texture.dimensions().hash(&mut hasher);
        layer.texture.as_raw().hash(&mut hasher);
        layer.tint.hash(&mut hasher);
        let parts = |parts: &[PartId]| parts.iter().map(|p| p.0).collect::<Vec<_>>();
        parts(layer.skip).hash(&mut hasher);
        layer.only.map(parts).hash(&mut hasher);
        layer.share_depth.hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// The layers over one another, first at the back.
pub fn rasterize_layers(layers: &[PreviewLayer], width: u32, height: u32) -> RgbaImage {
    let mut out = RgbaImage::new(width, height);
    let mut depth = vec![f32::NEG_INFINITY; (width * height) as usize];
    for layer in layers {
        if !layer.share_depth {
            depth.fill(f32::NEG_INFINITY);
        }
        draw_layer(&mut out, &mut depth, layer);
    }
    out
}

fn draw_layer(out: &mut RgbaImage, depth: &mut [f32], layer: &PreviewLayer) {
    let model = layer.model;
    for (index, matrix) in model
        .part_matrices(layer.pose, layer.root)
        .into_iter()
        .enumerate()
        .filter(|(index, _)| {
            let id = PartId(*index);
            layer.only.is_none_or(|parts| parts.contains(&id))
                && layer.skip.iter().all(|p| *p != id)
        })
    {
        let Some(matrix) = matrix else {
            continue;
        };
        let normals = Mat3::from_mat4(matrix).inverse().transpose();
        model.part_faces(crate::world::entity::model::PartId(index), &mut |face| {
            let normal = (normals * face.normal).normalize_or_zero();
            if normal.z <= 0.0 {
                return;
            }
            let shade = inventory_light(normal);
            let screen = face.corners.map(|c| matrix.transform_point3(c));
            for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
                crate::gui::hud::item_icon::draw_triangle(
                    out,
                    depth,
                    [screen[a], screen[b], screen[c]],
                    [face.uvs[a], face.uvs[b], face.uvs[c]],
                    crate::gui::hud::item_icon::Surface {
                        texture: layer.texture,
                        tint: layer.tint,
                        glint: layer.glint,
                    },
                    shade,
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::player::{AvatarRenderState, PlayerModel, PlayerModelType};

    fn preview(mouse: (f32, f32)) -> EntityPreview {
        EntityPreview {
            x0: 26.0,
            y0: 8.0,
            x1: 75.0,
            y1: 78.0,
            size: 30.0,
            offset_y: 0.0625,
            mouse,
            entity: None,
        }
    }

    #[test]
    fn the_player_stands_in_the_middle_of_its_box() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let state = AvatarRenderState::default();
        let box_ = preview((50.5, 43.0));
        let root = box_.view(2.0, 1.8) * PlayerModel::root_transform(&state);
        let mut skin = RgbaImage::new(64, 64);
        skin.pixels_mut()
            .for_each(|p| *p = image::Rgba([200, 150, 100, 255]));
        let (w, h) = box_.pixels(2.0);
        let image = rasterize(&player.model, &player.pose(&state), root, &skin, w, h);
        assert_eq!((w, h), (98, 140));
        let drawn: Vec<(u32, u32)> = image
            .enumerate_pixels()
            .filter(|(_, _, p)| p[3] > 0)
            .map(|(x, y, _)| (x, y))
            .collect();
        assert!(!drawn.is_empty());
        let top = drawn.iter().map(|p| p.1).min().unwrap();
        let bottom = drawn.iter().map(|p| p.1).max().unwrap();
        let left = drawn.iter().map(|p| p.0).min().unwrap();
        let right = drawn.iter().map(|p| p.0).max().unwrap();
        assert!(top > 5 && bottom < 135, "{top} {bottom}");
        assert!((left + right) as i32 / 2 - 49 <= 1, "{left} {right}");
        let head = image.get_pixel((left + right) / 2, top + 3);
        assert!(head[3] == 255);
    }

    #[test]
    fn the_mouse_to_the_right_turns_the_head_right() {
        let left = preview((0.0, 43.0)).angles().0;
        let right = preview((100.0, 43.0)).angles().0;
        assert!(left > 0.0 && right < 0.0);
    }

    #[test]
    fn shared_depth_keeps_the_closer_layer_when_a_later_one_is_behind() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let state = AvatarRenderState::default();
        let box_ = preview((50.5, 43.0));
        let root = box_.view(2.0, 1.8) * PlayerModel::root_transform(&state);
        let pose = player.pose(&state);
        let mut skin = RgbaImage::new(64, 64);
        skin.pixels_mut()
            .for_each(|p| *p = image::Rgba([200, 150, 100, 255]));
        let mut red = RgbaImage::new(64, 64);
        red.pixels_mut()
            .for_each(|p| *p = image::Rgba([255, 0, 0, 255]));
        let (w, h) = box_.pixels(2.0);
        let behind = Mat4::from_translation(Vec3::new(0.0, 0.0, -50.0)) * root;
        let layer = |texture, root, share_depth| PreviewLayer {
            model: &player.model,
            pose: &pose,
            root,
            texture,
            tint: [255; 3],
            skip: &[],
            only: None,
            share_depth,
            glint: None,
        };
        let shared = rasterize_layers(
            &[layer(&skin, root, false), layer(&red, behind, true)],
            w,
            h,
        );
        let reset = rasterize_layers(
            &[layer(&skin, root, false), layer(&red, behind, false)],
            w,
            h,
        );
        let only_skin = rasterize_layers(&[layer(&skin, root, false)], w, h);
        let (x, y) = shared
            .enumerate_pixels()
            .find(|(_, _, p)| p[3] > 0)
            .map(|(x, y, _)| (x, y))
            .expect("drawn");
        assert_eq!(shared.get_pixel(x, y), only_skin.get_pixel(x, y));
        let painted = reset.get_pixel(x, y);
        assert!(
            painted[0] > 180 && painted[1] < 40 && painted[2] < 40,
            "{painted:?}"
        );
    }

    /// An armor trim lies exactly on its armor: drawn after it at the
    /// same depth, it shows (`GL_LEQUAL`).
    #[test]
    fn a_layer_at_the_same_depth_draws_over_the_one_before() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let state = AvatarRenderState::default();
        let box_ = preview((50.5, 43.0));
        let root = box_.view(2.0, 1.8) * PlayerModel::root_transform(&state);
        let pose = player.pose(&state);
        let mut armor = RgbaImage::new(64, 64);
        armor
            .pixels_mut()
            .for_each(|p| *p = image::Rgba([200, 200, 200, 255]));
        let mut trim = RgbaImage::new(64, 64);
        trim.pixels_mut()
            .for_each(|p| *p = image::Rgba([255, 0, 0, 255]));
        let (w, h) = box_.pixels(2.0);
        let layer = |texture, share_depth| PreviewLayer {
            model: &player.model,
            pose: &pose,
            root,
            texture,
            tint: [255; 3],
            skip: &[],
            only: None,
            share_depth,
            glint: None,
        };
        let image = rasterize_layers(&[layer(&armor, false), layer(&trim, true)], w, h);
        let drawn: Vec<_> = image.pixels().filter(|p| p[3] > 0).collect();
        assert!(!drawn.is_empty());
        assert!(
            drawn.iter().all(|p| p[0] > 100 && p[1] < 40),
            "the trim covers the armor"
        );
    }
}
