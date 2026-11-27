//! Vanilla's JSON block and item models (`BlockModel`, `BlockElement`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};
use image::RgbaImage;
use melon_core::math::angle::{self, Rotation};
use serde_json::Value;

/// `ItemTransform`: the model's placement in one display context.
#[derive(Clone, Copy)]
pub struct Display {
    pub rotation: Vec3,
    pub translation: Vec3,
    pub scale: Vec3,
}

/// A block or item model with its parents merged in (`BlockModel`'s resolved form).
pub struct Model {
    pub textures: HashMap<String, String>,
    pub elements: Vec<Value>,
    pub gui: Option<Display>,
    /// Every `display` transform by context (`gui`, `firstperson_righthand`, ...).
    pub displays: HashMap<String, Display>,
    /// `gui_light: front`.
    pub front_light: bool,
    /// Inherits from `builtin/generated`: a flat item made of `layerN`s.
    pub generated: bool,
    /// `ambientocclusion`, the nearest one set.
    pub ambient_occlusion: bool,
    /// Texture IDs bound with `force_translucent` (`Material`).
    pub forced_translucent: std::collections::HashSet<String>,
}

impl Model {
    pub fn resolve(assets_root: &Path, id: &str) -> Option<Self> {
        Self::resolve_with(id, &|id| {
            melon_assets::assets::json_resource(assets_root, "models", id).map(Arc::new)
        })
    }

    /// [`Self::resolve`], each `models/<id>.json` read through `load`.
    pub fn resolve_with(id: &str, load: &dyn Fn(&str) -> Option<Arc<Value>>) -> Option<Self> {
        let mut model = Self {
            textures: HashMap::new(),
            elements: Vec::new(),
            gui: None,
            displays: HashMap::new(),
            front_light: false,
            generated: false,
            ambient_occlusion: true,
            forced_translucent: Default::default(),
        };
        let mut ao_set = false;
        let mut light_set = false;
        let mut next = Some(id.to_owned());
        let mut namespace = melon_assets::assets::split_id(id).0.to_owned();
        for _ in 0..16 {
            let Some(id) = next.take() else { break };
            let (id_ns, path) = melon_assets::assets::split_id(&id);
            if path == "builtin/generated" {
                model.generated = true;
                break;
            }
            if path == "builtin/entity" {
                break;
            }
            let json = load(&id).or_else(|| {
                if !id.contains(':') {
                    load(&format!("{namespace}:{path}"))
                } else {
                    None
                }
            });
            let Some(json) = json else {
                if !model.elements.is_empty() || model.generated {
                    break;
                }
                return None;
            };
            namespace = id_ns.to_owned();
            if let Some(textures) = json["textures"].as_object() {
                for (name, value) in textures {
                    if value["force_translucent"].as_bool() == Some(true)
                        && let Some(sprite) = value["sprite"].as_str()
                    {
                        model.forced_translucent.insert(sprite.to_owned());
                    }
                    if let Some(value) = value.as_str().or_else(|| value["sprite"].as_str()) {
                        model
                            .textures
                            .entry(name.clone())
                            .or_insert_with(|| value.to_owned());
                    }
                }
            }
            if model.elements.is_empty()
                && let Some(elements) = json["elements"].as_array()
            {
                model.elements = elements.clone();
            }
            if let Some(displays) = json["display"].as_object() {
                for (context, display) in displays {
                    model
                        .displays
                        .entry(context.clone())
                        .or_insert_with(|| Display {
                            rotation: vec3(&display["rotation"]).unwrap_or(Vec3::ZERO),
                            translation: vec3(&display["translation"]).unwrap_or(Vec3::ZERO),
                            scale: vec3(&display["scale"]).unwrap_or(Vec3::ONE),
                        });
                }
            }
            if !light_set && let Some(light) = json["gui_light"].as_str() {
                model.front_light = light == "front";
                light_set = true;
            }
            if !ao_set && let Some(ao) = json["ambientocclusion"].as_bool() {
                model.ambient_occlusion = ao;
                ao_set = true;
            }
            next = json["parent"].as_str().map(str::to_owned);
        }
        model.gui = model.displays.get("gui").copied();
        Some(model)
    }

    /// Follows `#name` references to a texture ID.
    pub fn texture(&self, name: &str) -> Option<String> {
        let mut name = name.trim_start_matches('#');
        for _ in 0..8 {
            let value = self.textures.get(name)?;
            match value.strip_prefix('#') {
                Some(reference) => name = reference,
                None => return Some(value.clone()),
            }
        }
        None
    }
}

pub fn vec3(value: &Value) -> Option<Vec3> {
    let a = value.as_array()?;
    Some(Vec3::new(
        a.first()?.as_f64()? as f32,
        a.get(1)?.as_f64()? as f32,
        a.get(2)?.as_f64()? as f32,
    ))
}

pub fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Decoded textures by ID, only the first frame of an animated one.
pub struct TextureCache<'a> {
    assets_root: &'a Path,
    images: HashMap<String, Option<RgbaImage>>,
    /// The animated textures loaded so far, by ID.
    animations: HashMap<String, crate::texture_animation::Frames>,
    tick: u64,
    /// The IDs given an image by [`Self::insert`] (a skin, a composed
    /// banner): not the file an atlas holds under that name.
    inserted: std::collections::HashSet<String>,
    /// [`Self::alpha`] so far.
    alphas: HashMap<String, Option<crate::transparency::AlphaMap>>,
}

impl<'a> TextureCache<'a> {
    pub fn new(assets_root: &'a Path) -> Self {
        Self {
            assets_root,
            images: HashMap::new(),
            animations: HashMap::new(),
            tick: 0,
            inserted: std::collections::HashSet::new(),
            alphas: HashMap::new(),
        }
    }

    /// Uses `image` for `id` instead of loading it.
    pub fn insert(&mut self, id: &str, image: RgbaImage) {
        self.animations.remove(id);
        self.alphas.remove(id);
        self.images.insert(id.to_owned(), Some(image));
        self.inserted.insert(id.to_owned());
    }

    /// Whether `id` holds an image given by [`Self::insert`].
    pub fn is_inserted(&self, id: &str) -> bool {
        self.inserted.contains(id)
    }

    /// A texture by ID, loaded on first use; an animated one shows its current frame.
    pub fn get(&mut self, id: &str) -> Option<&RgbaImage> {
        if !self.images.contains_key(id) {
            let image = self.load(id);
            self.images.insert(id.to_owned(), image);
        }
        self.images.get(id)?.as_ref()
    }

    fn load(&mut self, id: &str) -> Option<RgbaImage> {
        let Some((png, meta)) = melon_assets::assets::texture_with_meta(self.assets_root, id)
        else {
            return melon_assets::assets::palette::resolve(self.assets_root, id);
        };
        let img = melon_assets::assets::decode_png(&png)?;
        if let Some(frames) = meta
            .as_ref()
            .and_then(|meta| meta.get("animation"))
            .and_then(|animation| crate::texture_animation::frames_of(&img, animation))
        {
            let frame = frames.at(self.tick).clone();
            self.animations.insert(id.to_owned(), frames);
            return Some(frame);
        }
        let side = img.width();
        if img.height() > side {
            Some(image::imageops::crop_imm(&img, 0, 0, side, side).to_image())
        } else {
            Some(img)
        }
    }

    /// The texels' transparency of texture `id`: an animated one's over
    /// all its frames, not just the one showing.
    pub fn alpha(&mut self, id: &str) -> Option<crate::transparency::AlphaMap> {
        use crate::transparency::AlphaMap;
        if !self.alphas.contains_key(id) {
            // Loads it, an animation's frames too.
            let still = self.get(id).map(AlphaMap::of);
            let alpha = match self.animations.get(id) {
                Some(frames) => Some(AlphaMap::of_frames(frames)),
                None => still,
            };
            self.alphas.insert(id.to_owned(), alpha);
        }
        self.alphas.get(id)?.clone()
    }

    /// Shows every animated texture as it is `tick` ticks in.
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
        for (id, frames) in &self.animations {
            self.images
                .insert(id.clone(), Some(frames.at(tick).clone()));
        }
    }

    /// The animated textures loaded so far.
    pub fn animations(&self) -> impl Iterator<Item = &crate::texture_animation::Frames> {
        self.animations.values()
    }
}

/// One `BlockElementFace` ready to draw.
pub struct Quad {
    /// Texture top-left, bottom-left, bottom-right, top-right as seen from outside.
    pub corners: [Vec3; 4],
    /// `0..1` within the face's texture.
    pub uvs: [Vec2; 4],
    pub normal: Vec3,
    /// `#name` or a texture ID.
    pub texture: String,
    pub tint: Option<usize>,
    /// `cullface`: hidden when the neighbour on that side is solid.
    pub cullface: Option<String>,
    /// The element's `shade_direction_override`.
    pub shade_override: Option<crate::block::direction::Direction>,
    /// The element's `light_emission` (`0..=15`).
    pub light_emission: u8,
}

/// The faces of one model element (`BlockElement`), in model space (0-16).
pub fn element_quads(element: &Value) -> Vec<Quad> {
    let (Some(from), Some(to)) = (vec3(&element["from"]), vec3(&element["to"])) else {
        return Vec::new();
    };
    let rotate = element_rotation(&element["rotation"]);
    let mut quads = Vec::new();
    let Some(faces) = element["faces"].as_object() else {
        return quads;
    };
    for (name, face) in faces {
        let Some((corners, normal, default_uv)) = face_geometry(name, from, to) else {
            continue;
        };
        let uv = face["uv"]
            .as_array()
            .and_then(|a| {
                Some([
                    a.first()?.as_f64()? as f32,
                    a.get(1)?.as_f64()? as f32,
                    a.get(2)?.as_f64()? as f32,
                    a.get(3)?.as_f64()? as f32,
                ])
            })
            .unwrap_or(default_uv);
        let mut uvs = [
            Vec2::new(uv[0], uv[1]),
            Vec2::new(uv[0], uv[3]),
            Vec2::new(uv[2], uv[3]),
            Vec2::new(uv[2], uv[1]),
        ];
        let turns = (face["rotation"].as_i64().unwrap_or(0) / 90).rem_euclid(4) as usize;
        uvs.rotate_left(turns);
        quads.push(Quad {
            corners: corners.map(&rotate),
            uvs: uvs.map(|uv| uv / 16.0),
            normal: rotate(normal + Vec3::splat(8.0)) - rotate(Vec3::splat(8.0)),
            texture: face["texture"].as_str().unwrap_or("").to_owned(),
            tint: face["tintindex"].as_u64().map(|i| i as usize),
            cullface: face["cullface"].as_str().map(str::to_owned),
            shade_override: element["shade_direction_override"]
                .as_str()
                .and_then(crate::block::direction::Direction::from_name),
            light_emission: element["light_emission"].as_u64().unwrap_or(0).min(15) as u8,
        });
    }
    quads
}

/// `BlockElementRotation`.
pub fn element_rotation(rotation: &Value) -> impl Fn(Vec3) -> Vec3 {
    let origin = vec3(&rotation["origin"]).unwrap_or(Vec3::splat(8.0));
    let angle = rotation["angle"].as_f64().unwrap_or(0.0) as f32;
    let axis = rotation["axis"].as_str().unwrap_or("y").to_owned();
    let rescale = rotation["rescale"].as_bool().unwrap_or(false);
    let radians = angle.to_radians();
    let matrix = match axis.as_str() {
        "x" => Mat3::rot_x(radians),
        "z" => Mat3::rot_z(radians),
        _ => Mat3::rot_y(radians),
    };
    let stretch = if rescale && angle != 0.0 {
        let s = 1.0 / angle::cos(radians).abs().max(1.0e-3);
        match axis.as_str() {
            "x" => Vec3::new(1.0, s, s),
            "z" => Vec3::new(s, s, 1.0),
            _ => Vec3::new(s, 1.0, s),
        }
    } else {
        Vec3::ONE
    };
    move |p| origin + (matrix * (p - origin)) * stretch
}

/// A face's corners (texture top-left, bottom-left, bottom-right, top-right as seen from
/// outside), its normal and its default UV.
pub fn face_geometry(name: &str, f: Vec3, t: Vec3) -> Option<([Vec3; 4], Vec3, [f32; 4])> {
    let v = Vec3::new;
    Some(match name {
        "down" => (
            [
                v(f.x, f.y, t.z),
                v(f.x, f.y, f.z),
                v(t.x, f.y, f.z),
                v(t.x, f.y, t.z),
            ],
            v(0.0, -1.0, 0.0),
            [f.x, 16.0 - t.z, t.x, 16.0 - f.z],
        ),
        "up" => (
            [
                v(f.x, t.y, f.z),
                v(f.x, t.y, t.z),
                v(t.x, t.y, t.z),
                v(t.x, t.y, f.z),
            ],
            v(0.0, 1.0, 0.0),
            [f.x, f.z, t.x, t.z],
        ),
        "north" => (
            [
                v(t.x, t.y, f.z),
                v(t.x, f.y, f.z),
                v(f.x, f.y, f.z),
                v(f.x, t.y, f.z),
            ],
            v(0.0, 0.0, -1.0),
            [16.0 - t.x, 16.0 - t.y, 16.0 - f.x, 16.0 - f.y],
        ),
        "south" => (
            [
                v(f.x, t.y, t.z),
                v(f.x, f.y, t.z),
                v(t.x, f.y, t.z),
                v(t.x, t.y, t.z),
            ],
            v(0.0, 0.0, 1.0),
            [f.x, 16.0 - t.y, t.x, 16.0 - f.y],
        ),
        "west" => (
            [
                v(f.x, t.y, f.z),
                v(f.x, f.y, f.z),
                v(f.x, f.y, t.z),
                v(f.x, t.y, t.z),
            ],
            v(-1.0, 0.0, 0.0),
            [f.z, 16.0 - t.y, t.z, 16.0 - f.y],
        ),
        "east" => (
            [
                v(t.x, t.y, t.z),
                v(t.x, f.y, t.z),
                v(t.x, f.y, f.z),
                v(t.x, t.y, f.z),
            ],
            v(1.0, 0.0, 0.0),
            [16.0 - t.z, 16.0 - t.y, 16.0 - f.z, 16.0 - f.y],
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_with_elements_keeps_them_when_the_parent_file_is_missing() {
        let dir = std::env::temp_dir().join(format!("melon-model-parent-{}", std::process::id()));
        let models = dir.join("minecraft/models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(
            models.join("stove.json"),
            br##"{
                "parent": "wood_fire_stove",
                "textures": {"0": "block/stone"},
                "elements": [{"from": [0, 0, 0], "to": [16, 8, 16], "faces": {
                    "up": {"texture": "#0"}
                }}]
            }"##,
        )
        .unwrap();
        let model = Model::resolve(&dir, "stove").expect("child elements are the model");
        assert_eq!(model.elements.len(), 1);
        assert_eq!(
            model.textures.get("0").map(String::as_str),
            Some("block/stone")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_model_without_elements_or_parent_file_is_missing() {
        let dir = std::env::temp_dir().join(format!("melon-model-empty-{}", std::process::id()));
        let models = dir.join("minecraft/models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(
            models.join("ghost.json"),
            br#"{"parent": "does_not_exist"}"#,
        )
        .unwrap();
        assert!(Model::resolve(&dir, "ghost").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
