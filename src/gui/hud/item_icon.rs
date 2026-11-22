//! Item icons for the hotbar and inventory, baked on the CPU from the item models.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use image::RgbaImage;
use pumpkin_data::item_stack::ItemStack;

use crate::assets::json_model::{Display, TextureCache};
use crate::gui::hud::icon_atlas::{IconAtlas, Slot};
use crate::gui::sprite::GuiSprite;
use crate::math::angle::Rotation;
use crate::renderer::quad::DrawList;
use crate::renderer::{Renderer, TextureId};
use crate::world::block_entity::models::SkullType;
use crate::world::glint::GlintUv;
use crate::world::head_profile::HeadProfile;
use crate::world::item_model::{self, Count, ItemKey, ItemModel, ItemQuad};
use crate::world::texture_animation;

/// Edge length of a baked block icon.
const BLOCK_ICON_SIZE: u32 = 64;
/// `ItemTransform`'s default for block models (`block/block.json`'s `gui`).
const DEFAULT_BLOCK_GUI: Display = Display {
    rotation: Vec3::new(30.0, 225.0, 0.0),
    translation: Vec3::ZERO,
    scale: Vec3::splat(0.625),
};
/// The item atlas the flat icons are drawn from: its texture and its
/// sprites (`[u0, v0, u1, v1]` and whether partly see-through) by
/// `namespace:path`.
#[derive(Clone)]
pub struct ItemSheet {
    pub texture: TextureId,
    pub sprites: Arc<HashMap<String, ([f32; 4], crate::world::transparency::AlphaMap)>>,
}

/// One item's icon, ready to draw.
pub struct Icon {
    look: Look,
    home: Home,
}

enum Look {
    /// A flat item's layers in the item atlas, bottom first, each with its
    /// tint (a potion's liquid over its bottle).
    Layers(TextureId, Vec<([f32; 4], [f32; 4])>),
    /// A baked image.
    Sprite(GuiSprite),
}

/// What an icon's pixels take up, to give back when it goes.
enum Home {
    /// Only sprites of the item atlas, which stays.
    Sheet,
    Atlas(Slot),
    /// A texture of its own: too big for an atlas page.
    Own(TextureId),
}

impl Icon {
    /// Over `x, y, w, h`.
    pub fn draw(&self, list: &mut DrawList, x: f32, y: f32, w: f32, h: f32) {
        match &self.look {
            Look::Sprite(sprite) => sprite.draw(list, x, y, w, h),
            Look::Layers(texture, layers) => {
                for &(uv, color) in layers {
                    list.push_rect(*texture, x, y, w, h, uv, color);
                }
            }
        }
    }
}

/// Icons by item and tint, made on first use and kept.
pub struct ItemIcons {
    assets_root: PathBuf,
    sheet: ItemSheet,
    atlas: IconAtlas,
    icons: rustc_hash::FxHashMap<ItemKey, Option<Icon>>,
    /// Player heads with a profile, by [`HeadProfile::skin_key`]: whether
    /// the skin was the owner's own, and the icon.
    heads: HashMap<uuid::Uuid, (bool, Option<Icon>)>,
}

impl ItemIcons {
    pub fn new(assets_root: &Path, sheet: ItemSheet, max_texture_side: u32) -> Self {
        Self {
            assets_root: assets_root.to_path_buf(),
            sheet,
            atlas: IconAtlas::new(max_texture_side),
            icons: Default::default(),
            heads: HashMap::new(),
        }
    }

    /// Makes sure `stack`'s icon exists, making it on first sight. Whether
    /// it had to.
    pub fn prepare(&mut self, renderer: &mut Renderer, stack: &ItemStack) -> bool {
        let Some((key, name)) = self.missing(stack) else {
            return false;
        };
        let made = make(&self.assets_root, name, Some(&key), None, &self.sheet);
        let icon = made.and_then(|made| self.upload(renderer, name, made));
        self.icons.insert(key, icon);
        true
    }

    fn missing(&self, stack: &ItemStack) -> Option<(ItemKey, &'static str)> {
        // Air (most slots) needs no key: it takes a lock and a hash.
        if stack.item.id == 0 {
            return None;
        }
        let key = ItemKey::of(&self.assets_root, stack);
        if self.icons.contains_key(&key) {
            None
        } else {
            Some((key, stack.item.registry_key))
        }
    }

    /// Makes a parallel batch of missing icons from `stacks` on the CPU,
    /// then uploads them.
    pub fn prepare_batch(
        &mut self,
        renderer: &mut Renderer,
        stacks: impl FnOnce(&mut dyn FnMut(&ItemStack)),
    ) {
        let mut jobs = Vec::new();
        let mut seen = HashSet::new();
        stacks(&mut |stack| {
            if let Some((key, name)) = self.missing(stack)
                && seen.insert(key.clone())
            {
                jobs.push((key, name));
            }
        });
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1);
        jobs.truncate(threads.saturating_mul(8).max(8));
        if jobs.is_empty() {
            return;
        }
        let root = &self.assets_root;
        let sheet = &self.sheet;
        let made: Vec<_> = std::thread::scope(|scope| {
            let size = jobs.len().div_ceil(threads).max(1);
            let mut joins = Vec::new();
            for chunk in jobs.chunks(size) {
                joins.push(scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(key, name)| {
                            (key.clone(), *name, make(root, name, Some(key), None, sheet))
                        })
                        .collect::<Vec<_>>()
                }));
            }
            joins
                .into_iter()
                .flat_map(|job| job.join().expect("item icon bake"))
                .collect()
        });
        for (key, name, made) in made {
            let icon = made.and_then(|made| self.upload(renderer, name, made));
            self.icons.insert(key, icon);
        }
    }

    /// The icon of a player head with `profile`, drawn with `skin` (its
    /// owner's when `own_skin`, else a default one to replace later).
    pub fn prepare_head(
        &mut self,
        renderer: &mut Renderer,
        stack: &ItemStack,
        profile: &HeadProfile,
        skin: &RgbaImage,
        own_skin: bool,
    ) -> bool {
        let key = profile.skin_key();
        if let Some(&(baked_own, _)) = self.heads.get(&key)
            && (baked_own || !own_skin)
        {
            return false;
        }
        let name = stack.item.registry_key;
        let made = make(&self.assets_root, name, None, Some(skin), &self.sheet);
        let icon = made.and_then(|made| self.upload(renderer, name, made));
        if let Some((_, Some(old))) = self.heads.insert(key, (own_skin, icon)) {
            self.free(renderer, old);
        }
        true
    }

    /// Frees every icon and the atlas pages.
    pub fn release(&mut self, renderer: &mut Renderer) {
        item_model::forget_count_cuts();
        let heads: Vec<_> = self.heads.drain().filter_map(|(_, (_, i))| i).collect();
        let icons: Vec<_> = self.icons.drain().filter_map(|(_, i)| i).collect();
        for icon in icons.into_iter().chain(heads) {
            self.free(renderer, icon);
        }
        self.atlas.release(renderer);
    }

    /// The icon, if [`Self::prepare`] managed to make it; a player head's
    /// own when [`Self::prepare_head`] made one.
    pub fn get(&self, stack: &ItemStack) -> Option<&Icon> {
        if let Some(profile) = HeadProfile::of_stack(stack)
            && let Some((_, Some(icon))) = self.heads.get(&profile.skin_key())
        {
            return Some(icon);
        }
        self.icons
            .get(&ItemKey::of(&self.assets_root, stack))
            .and_then(Option::as_ref)
    }

    fn free(&mut self, renderer: &mut Renderer, icon: Icon) {
        match icon.home {
            Home::Sheet => {}
            Home::Atlas(slot) => self.atlas.free(slot),
            Home::Own(texture) => renderer.free_texture(texture),
        }
    }

    /// Puts a made icon on the GPU: layers need nothing, a baked image
    /// takes an atlas slot (a texture of its own only when too big).
    fn upload(&mut self, renderer: &mut Renderer, name: &str, made: Made) -> Option<Icon> {
        let mut icon = match made {
            Made::Layers(layers) => {
                return Some(Icon {
                    look: Look::Layers(self.sheet.texture, layers),
                    home: Home::Sheet,
                });
            }
            Made::Baked(icon) => icon,
        };
        let strip = icon.strip();
        if let Some((texture, slot, uv)) = self.atlas.place(renderer, &strip) {
            let sprite = GuiSprite::with_meta(texture, strip.dimensions(), &icon.meta()).within(uv);
            return Some(Icon {
                look: Look::Sprite(sprite),
                home: Home::Atlas(slot),
            });
        }
        if icon.frames[0].height() * icon.frames.len() as u32 > MAX_STRIP_HEIGHT {
            icon.frames.truncate(1);
            icon.schedule = vec![(0, 1)];
        }
        let strip = icon.strip();
        let texture = renderer
            .load_texture(&strip)
            .map_err(|e| crate::warn!("Item icon {name} not uploaded: {e:#}"))
            .ok()?;
        Some(Icon {
            look: Look::Sprite(GuiSprite::with_meta(
                texture,
                strip.dimensions(),
                &icon.meta(),
            )),
            home: Home::Own(texture),
        })
    }
}

/// An icon made on the CPU, before upload.
enum Made {
    Layers(Vec<([f32; 4], [f32; 4])>),
    Baked(BakedIcon),
}

/// The icon of item `name`: a flat item whose layers all are in the item
/// atlas as its layers, anything else baked.
fn make(
    assets_root: &Path,
    name: &str,
    key: Option<&ItemKey>,
    skin: Option<&RgbaImage>,
    sheet: &ItemSheet,
) -> Option<Made> {
    let item = resolve(assets_root, name, key)?;
    if skin.is_none()
        && let Some(layers) = atlas_layers(&item, sheet)
    {
        return Some(Made::Layers(layers));
    }
    bake_item(assets_root, &item, skin).map(Made::Baked)
}

/// A flat item's layers as item atlas sprites with their tints; `None`
/// when it is not flat or some layer is not in the atlas.
fn atlas_layers(item: &ItemModel, sheet: &ItemSheet) -> Option<Vec<([f32; 4], [f32; 4])>> {
    if !item.is_flat() {
        return None;
    }
    let layers = item.flat_layers();
    if layers.is_empty() {
        return None;
    }
    layers
        .into_iter()
        .map(|(texture, tint)| {
            let &(uv, _) = sheet
                .sprites
                .get(&crate::world::item_atlas::canonical(&texture))?;
            let [r, g, b] = tint.map(|c| f32::from(c) / 255.0);
            Some((uv, [r, g, b, 1.0]))
        })
        .collect()
}

/// Bakes the icon for the item with registry key `key` (e.g.
pub fn bake(assets_root: &Path, key: &str) -> Option<RgbaImage> {
    let item = resolve(assets_root, key, None)?;
    bake_item(assets_root, &item, None).map(|mut icon| icon.frames.swap_remove(0))
}

/// The tallest strip of frames an animated icon is uploaded as.
const MAX_STRIP_HEIGHT: u32 = 8192;
/// A baked icon.
struct BakedIcon {
    frames: Vec<RgbaImage>,
    /// `(frame, ticks)` in play order.
    schedule: Vec<(usize, u32)>,
}

impl BakedIcon {
    /// The frames stacked top to bottom, as an animated sprite is.
    fn strip(&self) -> RgbaImage {
        let (w, h) = self.frames[0].dimensions();
        let mut strip = RgbaImage::new(w, h * self.frames.len() as u32);
        for (i, frame) in self.frames.iter().enumerate() {
            image::imageops::replace(&mut strip, frame, 0, i64::from(h) * i as i64);
        }
        strip
    }

    /// The `.mcmeta` [`Self::strip`] would have.
    fn meta(&self) -> serde_json::Value {
        let (w, h) = self.frames[0].dimensions();
        let frames: Vec<_> = self
            .schedule
            .iter()
            .map(|&(index, time)| serde_json::json!({"index": index, "time": time}))
            .collect();
        serde_json::json!({"animation": {"width": w, "height": h, "frames": frames}})
    }
}

/// The model of item `name` as `key` (its potion colour, count and
/// variant) says, not in use.
fn resolve(assets_root: &Path, name: &str, key: Option<&ItemKey>) -> Option<ItemModel> {
    let (potion, count) = key.map_or((None, Count::ONE), |k| (k.potion_color, k.count()));
    let variant = key.and_then(|k| k.variant.as_deref());
    ItemModel::resolve_in_use(assets_root, name, potion, (false, count), variant)
}

/// `item` drawn on the CPU (with `skin` for a player head), every frame
/// of its animated textures.
fn bake_item(assets_root: &Path, item: &ItemModel, skin: Option<&RgbaImage>) -> Option<BakedIcon> {
    let mut textures = TextureCache::new(assets_root);
    if let Some(skin) = skin {
        textures.insert(SkullType::Player.texture(), skin.clone());
    }
    let quads = if item.is_flat() {
        Vec::new()
    } else {
        item.quads(assets_root, &mut textures)
    };
    let first = bake_frame(item, &quads, &mut textures)?;
    let changes = texture_animation::frame_changes(textures.animations());
    let mut icon = BakedIcon {
        frames: vec![first],
        schedule: Vec::new(),
    };
    let Some((starts, period)) = changes else {
        icon.schedule.push((0, 1));
        return Some(icon);
    };
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(period);
        let time = (end - start) as u32;
        if i > 0 {
            textures.set_tick(start);
            let Some(image) = bake_frame(item, &quads, &mut textures) else {
                continue;
            };
            if *icon.frames.last().unwrap_or(&image) == image {
                if let Some(last) = icon.schedule.last_mut() {
                    last.1 += time;
                    continue;
                }
            } else {
                icon.frames.push(image);
            }
        }
        icon.schedule.push((icon.frames.len() - 1, time));
    }
    Some(icon)
}

/// The icon with `textures` as they show now.
fn bake_frame(
    item: &ItemModel,
    quads: &[ItemQuad],
    textures: &mut TextureCache,
) -> Option<RgbaImage> {
    if item.is_flat() {
        let mut out: Option<RgbaImage> = None;
        for (texture, tint) in item.flat_layers() {
            let Some(img) = textures.get(&texture) else {
                continue;
            };
            let mut img = img.clone();
            crate::assets::tint(&mut img, tint);
            match &mut out {
                None => out = Some(img),
                Some(base) => image::imageops::overlay(base, &img, 0, 0),
            }
        }
        return out;
    }
    if quads.is_empty() {
        let texture = item.model.texture("particle")?;
        return textures.get(&texture).cloned();
    }
    let display = item.model.gui.unwrap_or(DEFAULT_BLOCK_GUI);
    Some(rasterize(quads, display, item.model.front_light, textures))
}

/// Draws the faces orthographically through the GUI display transform.
fn rasterize(
    quads: &[ItemQuad],
    display: Display,
    front_light: bool,
    textures: &mut TextureCache,
) -> RgbaImage {
    let size = BLOCK_ICON_SIZE;
    let rotation = Mat3::rot_x_deg(display.rotation.x)
        * Mat3::rot_y_deg(display.rotation.y)
        * Mat3::rot_z_deg(display.rotation.z);
    let to_view = |p: Vec3| -> Vec3 {
        let centred = p - Vec3::splat(0.5);
        rotation * (centred * display.scale) + display.translation / 16.0
    };

    let mut out = RgbaImage::new(size, size);
    let mut depth = vec![f32::NEG_INFINITY; (size * size) as usize];
    // Translucent faces last (`MaterialInfo.itemRenderType`).
    let mut faces: Vec<(&ItemQuad, bool)> = quads
        .iter()
        .filter_map(|quad| {
            let alpha = textures.alpha(&quad.texture)?;
            Some((quad, quad.alpha.judge(&alpha).translucent))
        })
        .collect();
    faces.sort_by_key(|&(_, see_through)| see_through);
    for (quad, _) in faces {
        let Some(texture) = textures.get(&quad.texture) else {
            continue;
        };
        let normal = (rotation * quad.normal).normalize_or_zero();
        if normal.z < -1.0e-4 {
            continue;
        }
        let shade = gui_light(normal, front_light);
        let view = quad.corners.map(to_view);
        let screen =
            view.map(|v| Vec3::new((v.x + 0.5) * size as f32, (0.5 - v.y) * size as f32, v.z));
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            draw_triangle(
                &mut out,
                &mut depth,
                [screen[a], screen[b], screen[c]],
                [quad.uvs[a], quad.uvs[b], quad.uvs[c]],
                texture,
                shade,
                quad.tint,
                None,
            );
        }
    }
    out
}

/// `Lighting.setupFor3DItems`.
fn gui_light(normal: Vec3, front: bool) -> f32 {
    let mut l0 = Vec3::new(0.2, 1.0, -0.7).normalize();
    let mut l1 = Vec3::new(-0.2, 1.0, 0.7).normalize();
    if front {
        let turn = Mat3::rot_y(-22.5f32.to_radians()) * Mat3::rot_x_deg(135.0f32);
        l0 = turn * l0;
        l1 = turn * l1;
    }
    (0.4 + 0.6 * (normal.dot(l0).max(0.0) + normal.dot(l1).max(0.0))).min(1.0)
}

/// Fills one triangle with the nearest texel, keeping the nearest surface
/// (`depth`, larger is nearer) and blending see-through texels over what is
/// there.
///
/// Each barycentric weight is a linear function of the pixel, worked out
/// once per triangle; pixels are read and written in the buffers directly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_triangle(
    out: &mut RgbaImage,
    depth: &mut [f32],
    p: [Vec3; 3],
    uv: [Vec2; 3],
    texture: &RgbaImage,
    shade: f32,
    tint: [u8; 3],
    glint: Option<&RgbaImage>,
) {
    let area = (p[1].x - p[0].x) * (p[2].y - p[0].y) - (p[2].x - p[0].x) * (p[1].y - p[0].y);
    if area.abs() < 1.0e-6 || texture.width() == 0 || texture.height() == 0 {
        return;
    }
    let (w, h) = (out.width() as i32, out.height() as i32);
    let xs = [p[0].x, p[1].x, p[2].x];
    let ys = [p[0].y, p[1].y, p[2].y];
    let min_x = xs
        .into_iter()
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_x = xs
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(w as f32) as i32;
    let min_y = ys
        .into_iter()
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = ys
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(h as f32) as i32;
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    // Weight `i` at `(px, py)` is `c[i] + a[i] * px + b[i] * py`: the edge
    // opposite corner `i` over the area.
    let inv = 1.0 / area;
    let edge = |j: usize, k: usize| {
        (
            (p[j].y - p[k].y) * inv,
            (p[k].x - p[j].x) * inv,
            (p[j].x * p[k].y - p[k].x * p[j].y) * inv,
        )
    };
    let (a0, b0, c0) = edge(1, 2);
    let (a1, b1, c1) = edge(2, 0);
    let (tw, th) = (texture.width() as usize, texture.height() as usize);
    let texels = texture.as_raw();
    let glint = glint.map(|g| {
        let millis = crate::world::glint::millis();
        (g, GlintUv::new(crate::world::glint::ARMOR_SCALE, millis))
    });
    let lit = tint.map(|t| f32::from(t) / 255.0 * shade);
    let width = w as usize;
    let pixels: &mut [u8] = out;
    for y in min_y..max_y {
        let py = y as f32 + 0.5;
        let (row0, row1) = (c0 + b0 * py, c1 + b1 * py);
        let Some((from, to)) = row_span(
            [(row0, a0), (row1, a1), (1.0 - row0 - row1, -a0 - a1)],
            (min_x, max_x),
        ) else {
            continue;
        };
        for x in from..to {
            let px = x as f32 + 0.5;
            let w0 = row0 + a0 * px;
            let w1 = row1 + a1 * px;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1.0e-4 || w1 < -1.0e-4 || w2 < -1.0e-4 {
                continue;
            }
            let z = w0 * p[0].z + w1 * p[1].z + w2 * p[2].z;
            let index = y as usize * width + x as usize;
            // `GL_LEQUAL`: a layer at the same depth (an armor trim on its
            // armor, the leather overlay) draws over the one before.
            if z < depth[index] {
                continue;
            }
            let t = uv[0] * w0 + uv[1] * w1 + uv[2] * w2;
            let tx = ((t.x * tw as f32) as i32).clamp(0, tw as i32 - 1) as usize;
            let ty = ((t.y * th as f32) as i32).clamp(0, th as i32 - 1) as usize;
            let at = (ty * tw + tx) * 4;
            let texel = &texels[at..at + 4];
            if texel[3] < 26 {
                continue;
            }
            let color = [0, 1, 2].map(|i| f32::from(texel[i]) * lit[i]);
            let pixel = &mut pixels[index * 4..index * 4 + 4];
            if texel[3] == 255 {
                depth[index] = z;
                for i in 0..3 {
                    pixel[i] = to_byte(color[i]);
                }
                pixel[3] = 255;
            } else {
                let alpha = f32::from(texel[3]) / 255.0;
                let below_alpha = f32::from(pixel[3]) / 255.0;
                let total = alpha + below_alpha * (1.0 - alpha);
                for i in 0..3 {
                    let below = f32::from(pixel[i]) * below_alpha * (1.0 - alpha);
                    pixel[i] = to_byte((color[i] * alpha + below) / total);
                }
                pixel[3] = to_byte(total * 255.0);
            }
            if let Some((glint, at)) = &glint {
                add_glint(pixel, at.at(t), glint);
            }
        }
    }
}

/// The pixels `from..to` of `bounds` a row may cover: where every weight,
/// `row + step * (x + 0.5)`, can still be at least the inside tolerance.
/// A pixel wider each side, so the exact test per pixel decides.
fn row_span(weights: [(f32, f32); 3], (min_x, max_x): (i32, i32)) -> Option<(i32, i32)> {
    let (mut lo, mut hi) = (min_x as f32, max_x as f32);
    for (row, step) in weights {
        // `row + step * px >= -1e-4`.
        let edge = (-1.0e-4 - row) / step;
        if step > 0.0 {
            lo = lo.max(edge - 0.5);
        } else if step < 0.0 {
            hi = hi.min(edge - 0.5);
        } else if row < -1.0e-4 {
            return None;
        }
    }
    let from = (lo.floor() as i32 - 1).max(min_x);
    let to = (hi.ceil() as i32 + 2).min(max_x);
    (from < to).then_some((from, to))
}

/// `v.round() as u8` for `v >= 0`, without the `roundf` call `round`
/// becomes on CPUs before SSE 4.1. Saturates at 255 as the cast does.
#[inline]
fn to_byte(v: f32) -> u8 {
    (v + 0.5) as u8
}

/// `BlendFunction.GLINT`: `dst + src * src` of the scrolling armor glint.
fn add_glint(pixel: &mut [u8], uv: Vec2, glint: &RgbaImage) {
    let sample = crate::world::glint::sample(glint, uv);
    if sample[3] < 26 {
        return;
    }
    for i in 0..3 {
        let src = f32::from(sample[i]) / 255.0;
        pixel[i] = to_byte(f32::from(pixel[i]) + src * src * 255.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Option<PathBuf> {
        let root = crate::assets::default_root();
        root.join("minecraft/items/stone.json")
            .is_file()
            .then_some(root)
    }

    #[test]
    fn bakes_a_block_and_a_flat_item() {
        let Some(root) = assets() else {
            eprintln!("no extracted assets, skipping");
            return;
        };
        let stone = bake(&root, "stone").expect("stone icon");
        assert_eq!(stone.width(), BLOCK_ICON_SIZE);
        assert_eq!(stone.get_pixel(32, 32)[3], 255);
        assert_eq!(stone.get_pixel(0, 0)[3], 0);
        let diamond = bake(&root, "diamond").expect("diamond icon");
        assert_eq!(diamond.width(), 16);
        for key in [
            "grass_block",
            "oak_stairs",
            "oak_fence",
            "torch",
            "chest",
            "compass",
        ] {
            assert!(bake(&root, key).is_some(), "{key} has no icon");
        }
    }

    /// Flat items draw their item atlas layers, tinted: a potion is its
    /// bottle plus the liquid in its colour, no image of its own. A block
    /// is baked.
    #[test]
    fn flat_icons_are_atlas_layers_and_blocks_are_baked() {
        let Some(root) = assets() else {
            eprintln!("no extracted assets, skipping");
            return;
        };
        let ids = crate::world::item_atlas::texture_ids(&root);
        let packed = crate::world::block_models::pack_sprites(&root, ids, 4096);
        let sheet = ItemSheet {
            texture: TextureId(0),
            sprites: Arc::new(packed.sprites),
        };
        let key = |potion: Option<[u8; 3]>| ItemKey {
            item: 1,
            variant: None,
            potion_color: potion,
            count: 1,
            max_stack: 1,
        };
        let potion = make(&root, "potion", Some(&key(Some([255, 0, 0]))), None, &sheet);
        let Some(Made::Layers(layers)) = potion else {
            panic!("a potion is drawn from the atlas");
        };
        assert_eq!(layers.len(), 2, "overlay and bottle");
        assert!(
            layers.iter().any(|&(_, c)| c == [1.0, 0.0, 0.0, 1.0]),
            "{layers:?}"
        );
        assert!(matches!(
            make(&root, "diamond", None, None, &sheet),
            Some(Made::Layers(l)) if l.len() == 1
        ));
        assert!(matches!(
            make(&root, "stone", None, None, &sheet),
            Some(Made::Baked(_))
        ));
    }

    #[test]
    fn block_entity_items_are_drawn_as_models() {
        let Some(root) = assets() else {
            eprintln!("no extracted assets, skipping");
            return;
        };
        for key in [
            "chest",
            "ender_chest",
            "red_shulker_box",
            "blue_banner",
            "creeper_head",
            "player_head",
            "decorated_pot",
            "conduit",
            "white_bed",
            "shield",
        ] {
            let icon = bake(&root, key).unwrap_or_else(|| panic!("{key} has no icon"));
            assert_eq!(icon.width(), BLOCK_ICON_SIZE, "{key}");
            assert!(icon.pixels().any(|p| p[3] == 255), "{key} is empty");
        }
        assert_eq!(bake(&root, "trident").expect("trident").width(), 16);
    }
}
