//! What an item looks like in 3D, from the vanilla item definitions.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
use image::RgbaImage;
use pumpkin_data::data_component::DataComponent;
use pumpkin_data::item_stack::ItemStack;
use serde_json::{Value, json};

use crate::assets::json_model::{Model, TextureCache, element_quads, vec3};
use crate::math::angle::{self, DEG_TO_RAD, HALF_PI, PI, Rotation};
use crate::renderer::WorldVertex;
use crate::world::texture_animation;
use crate::world::transparency::{AlphaMap, AlphaRule};
use crate::world::{block_entity, geometry, model_part};

mod key;

pub use key::{ItemKey, KeyedStack, StackKey, Variant};

/// `grass_block`'s and other `minecraft:grass` tints away from any biome.
const GRASS_TINT: [u8; 3] = [124, 189, 107];
/// `minecraft:foliage` without a biome (`FoliageColor.FOLIAGE_DEFAULT`).
const FOLIAGE_TINT: [u8; 3] = [72, 181, 24];

/// The definition a stack is drawn by: its `item_model`, else its own.
fn definition_id<'a>(key: &'a str, variant: Option<&'a Variant>) -> &'a str {
    variant.and_then(|v| v.model.as_deref()).unwrap_or(key)
}

/// A stack size and the item's largest, for `minecraft:count`.
#[derive(Clone, Copy, Debug)]
pub struct Count {
    pub value: i32,
    pub max_stack: i32,
}

impl Count {
    pub const ONE: Count = Count {
        value: 1,
        max_stack: 64,
    };
}

/// The counts at which an item's model changes (`minecraft:count` range dispatch), by item.
static COUNT_CUTS: Mutex<Option<rustc_hash::FxHashMap<String, Vec<i32>>>> = Mutex::new(None);

/// Drops what is known of the items' count thresholds.
pub fn forget_count_cuts() {
    *COUNT_CUTS.lock().unwrap_or_else(|e| e.into_inner()) = None;
    key::resources_changed();
}

/// The stack sizes from which a `minecraft:count` dispatch in `model`
fn count_cuts(model: &Value, max_stack: i32, out: &mut Vec<i32>) {
    match model {
        Value::Object(map) => {
            if map.get("property").and_then(Value::as_str) == Some("minecraft:count")
                && let Some(entries) = map.get("entries").and_then(Value::as_array)
            {
                let (unit, scale) = count_unit(model, max_stack);
                for entry in entries {
                    if let Some(threshold) = entry["threshold"].as_f64() {
                        out.push((threshold / scale * unit).ceil() as i32);
                    }
                }
            }
            for value in map.values() {
                count_cuts(value, max_stack, out);
            }
        }
        Value::Array(values) => {
            for value in values {
                count_cuts(value, max_stack, out);
            }
        }
        _ => {}
    }
}

/// `(count per unit, scale)` of a `minecraft:count` dispatch.
fn count_unit(model: &Value, max_stack: i32) -> (f64, f64) {
    let normalize = model["normalize"].as_bool().unwrap_or(true);
    let unit = if normalize {
        f64::from(max_stack.max(1))
    } else {
        1.0
    };
    (unit, model["scale"].as_f64().unwrap_or(1.0))
}

/// `PotionContents.getColorOptional`: the custom colour, or the effects' mix.
pub fn potion_color(stack: &ItemStack) -> Option<[u8; 3]> {
    use pumpkin_data::data_component_impl::PotionContentsImpl;
    let contents = stack.get_data_component::<PotionContentsImpl>()?;
    if let Some(color) = contents.custom_color {
        let color = color as u32;
        return Some([(color >> 16) as u8, (color >> 8) as u8, color as u8]);
    }
    let potion = contents
        .potion_id
        .and_then(|id| u8::try_from(id).ok())
        .and_then(pumpkin_data::potion::Potion::from_id)?;
    let (mut r, mut g, mut b, mut weight) = (0u32, 0u32, 0u32, 0u32);
    for effect in potion
        .effects
        .iter()
        .filter(|e| e.show_icon || e.show_particles)
    {
        let color = effect.effect_type.color as u32;
        let amplifier = u32::from(effect.amplifier) + 1;
        r += amplifier * ((color >> 16) & 255);
        g += amplifier * ((color >> 8) & 255);
        b += amplifier * (color & 255);
        weight += amplifier;
    }
    (weight > 0).then(|| [(r / weight) as u8, (g / weight) as u8, (b / weight) as u8])
}

/// One thing an item definition draws in a GUI slot (`ItemModel.Unbaked`).
pub(crate) struct Layer {
    source: Source,
    /// Model space (blocks) to the item's.
    transform: Mat4,
}

pub(crate) enum Source {
    /// A JSON model, with the tint for each `tintindex`.
    Model {
        model: String,
        tints: Vec<[u8; 3]>,
        tint_kinds: Vec<String>,
    },
    /// A special renderer (`spec`, its `type` and options) with the model `base`.
    Special { base: String, spec: Value },
}

impl Source {
    /// The JSON model behind it.
    fn model(&self) -> &str {
        match self {
            Source::Model { model, .. } => model,
            Source::Special { base, .. } => base,
        }
    }
}

/// An item definition resolved for a GUI slot or a hand.
pub struct ItemModel {
    /// The first layer's JSON model.
    pub model: Model,
    layers: Vec<Layer>,
}

impl ItemModel {
    /// The item with registry key `key` (e.g. `oak_stairs`).
    pub fn resolve(assets_root: &Path, key: &str, potion: Option<[u8; 3]>) -> Option<Self> {
        Self::resolve_in_use(assets_root, key, potion, (false, Count::ONE), None)
    }

    /// [`Self::resolve`] for an item being used or not.
    pub fn resolve_in_use(
        assets_root: &Path,
        key: &str,
        potion: Option<[u8; 3]>,
        (held, count): (impl Into<HeldUse>, Count),
        variant: Option<&Variant>,
    ) -> Option<Self> {
        Self::resolve_for_display(
            assets_root,
            key,
            potion,
            (held, count),
            variant,
            HandPose::Gui,
        )
    }

    /// [`Self::resolve_in_use`] with the item's `ItemDisplayContext`.
    pub fn resolve_for_display(
        assets_root: &Path,
        key: &str,
        potion: Option<[u8; 3]>,
        (held, count): (impl Into<HeldUse>, Count),
        variant: Option<&Variant>,
        display: HandPose,
    ) -> Option<Self> {
        let id = definition_id(key, variant);
        let Some(definition) = crate::assets::json_resource(assets_root, "items", id) else {
            if crate::probe::enabled() {
                crate::probe::count("item.definition_missing", id);
            }
            return None;
        };
        let mut layers = Vec::new();
        let held = held.into();
        let state = State {
            using: held == HeldUse::Using,
            cast: held == HeldUse::Cast,
            count,
            variant,
            display,
        };
        collect_layers(&definition["model"], Mat4::IDENTITY, &state, &mut layers);
        if let Some(color) = potion {
            for layer in &mut layers {
                if let Source::Model {
                    tints, tint_kinds, ..
                } = &mut layer.source
                {
                    for (tint, kind) in tints.iter_mut().zip(tint_kinds.iter()) {
                        if kind == "potion" {
                            *tint = color;
                        }
                    }
                }
            }
        }
        let first = layers.first()?.source.model();
        let Some(model) = Model::resolve(assets_root, first) else {
            if crate::probe::enabled() {
                crate::probe::note("item.model_missing", first, || format!("from {id}"));
            }
            return None;
        };
        if crate::probe::enabled() && variant.is_some() {
            crate::probe::note("item.custom", id, || {
                let v = variant.expect("checked");
                format!(
                    "floats {:?}, strings {:?}, flags {:?}, colors {:?}, dye {:?}; {} layer(s), first model {first}",
                    v.floats
                        .iter()
                        .map(|b| f32::from_bits(*b))
                        .collect::<Vec<_>>(),
                    v.strings,
                    v.flags,
                    v.colors,
                    v.dye,
                    layers.len()
                )
            });
        }
        Some(Self { model, layers })
    }

    /// A block's JSON model drawn as it is (`BlockModelResolver` with a block's model).
    pub fn block(assets_root: &Path, model: &str) -> Option<Self> {
        let mut resolved = Model::resolve(assets_root, model)?;
        resolved.displays.clear();
        Some(Self {
            model: resolved,
            layers: vec![Layer {
                source: Source::Model {
                    model: model.to_owned(),
                    tints: Vec::new(),
                    tint_kinds: Vec::new(),
                },
                transform: Mat4::IDENTITY,
            }],
        })
    }

    /// Whether this is one flat sprite stack (`item/generated`), drawn as a single layer.
    pub fn is_flat(&self) -> bool {
        self.model.generated && self.layers.len() == 1
    }

    /// For a flat item: each `layerN` texture with its tint, bottom first.
    pub fn flat_layers(&self) -> Vec<(String, [u8; 3])> {
        let tints = match &self.layers[0].source {
            Source::Model { tints, .. } => tints.as_slice(),
            Source::Special { .. } => &[],
        };
        (0..)
            .map_while(|layer| self.model.texture(&format!("layer{layer}")))
            .enumerate()
            .map(|(layer, texture)| {
                (
                    texture,
                    tints.get(layer).copied().unwrap_or([255, 255, 255]),
                )
            })
            .collect()
    }

    /// Every face, counter-clockwise from outside.
    pub fn quads(&self, assets_root: &Path, textures: &mut TextureCache) -> Vec<ItemQuad> {
        let mut quads = Vec::new();
        for layer in &self.layers {
            layer_quads(assets_root, layer, textures, &mut quads);
        }
        for quad in &quads {
            if let Some(image) = block_entity::item::texture_image(assets_root, &quad.texture) {
                textures.insert(&quad.texture, image);
            }
        }
        quads
    }

    /// The meshes for every [`HandPose`], split by the texture they sample:
    /// an atlas `sprites` finds, or the item's own for the rest.
    pub fn held(&self, assets_root: &Path, sprites: SpriteLookup) -> Option<HeldItem> {
        self.held_with_skin(assets_root, None, sprites)
    }

    /// [`Self::held`] resolved once per [`HandPose`]: `display_context`
    pub fn held_poses(
        assets_root: &Path,
        key: &str,
        potion: Option<[u8; 3]>,
        (held, count): (impl Into<HeldUse>, Count),
        variant: Option<&Variant>,
        skin: Option<&RgbaImage>,
        sprites: SpriteLookup,
    ) -> Option<HeldItem> {
        let held = held.into();
        let mut textures = TextureCache::new(assets_root);
        if let Some(skin) = skin {
            textures.insert(
                crate::world::block_entity::models::SkullType::Player.texture(),
                skin.clone(),
            );
        }
        let mut per_pose: [Option<(ItemModel, Vec<ItemQuad>)>; 10] = std::array::from_fn(|_| None);
        for pose in HandPose::ALL {
            if let Some(item) =
                Self::resolve_for_display(assets_root, key, potion, (held, count), variant, pose)
            {
                let quads = item.quads(assets_root, &mut textures);
                per_pose[pose as usize] = Some((item, quads));
            }
        }
        let poses = std::array::from_fn(|i| {
            per_pose[i]
                .as_ref()
                .map(|(item, quads)| (item, quads.as_slice()))
        });
        bake_held(&poses, &mut textures, sprites)
    }

    /// [`Self::held`] with `skin` in place of Steve's.
    pub fn held_with_skin(
        &self,
        assets_root: &Path,
        skin: Option<&image::RgbaImage>,
        sprites: SpriteLookup,
    ) -> Option<HeldItem> {
        let mut textures = TextureCache::new(assets_root);
        if let Some(skin) = skin {
            textures.insert(
                crate::world::block_entity::models::SkullType::Player.texture(),
                skin.clone(),
            );
        }
        let quads = self.quads(assets_root, &mut textures);
        let poses = [Some((self, quads.as_slice())); 10];
        bake_held(&poses, &mut textures, sprites)
    }

    /// One pose's vertices: `ItemTransform.apply` baked in. Only the quads
    /// `place` (given each one's index) finds a spot for (origin and size
    /// in their texture) are drawn.
    fn mesh_in_pose(
        &self,
        pose: HandPose,
        quads: &[ItemQuad],
        place: impl Fn(usize, &ItemQuad) -> Option<(Vec2, Vec2)>,
    ) -> Vec<WorldVertex> {
        let display = pose.display(&self.model);
        let shade_frame = pose.shade_frame() * Mat3::from_mat4(display);
        let mut out = Vec::with_capacity(quads.len() * 4);
        for (i, quad) in quads.iter().enumerate() {
            let Some((origin, size)) = place(i, quad) else {
                continue;
            };
            let shade_normal = if pose.bakes_shade_in_model_space() {
                quad.normal
            } else {
                shade_frame * quad.normal
            };
            let shade = if quad.shade {
                geometry::shade(shade_normal.normalize_or_zero())
            } else {
                1.0
            };
            let color = quad.tint.map(|c| f32::from(c) / 255.0 * shade);
            let corners = std::array::from_fn(|i| {
                let pos = display.transform_point3(quad.corners[i]);
                let uv = origin + quad.uvs[i] * size;
                (pos.to_array(), uv.to_array())
            });
            geometry::push_quad(&mut out, corners, color, false);
        }
        out
    }
}

/// Which texture a part of a held item samples.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Sheet {
    /// The item atlas, built with the resources (`textures/item/`...).
    Items,
    /// The block atlas.
    Blocks,
    /// The item's own texture: what no atlas holds (a skin, a composed
    /// banner, a texture no atlas source lists).
    Own,
}

/// Finds a texture in the atlases built at load: its sheet (never
/// [`Sheet::Own`]), its `[u0, v0, u1, v1]` and its texels' transparency.
pub type SpriteLookup<'a> = &'a dyn Fn(&str) -> Option<(Sheet, [f32; 4], AlphaMap)>;

/// Where one texture of a held item is.
struct Place {
    sheet: Sheet,
    origin: Vec2,
    size: Vec2,
    alpha: AlphaMap,
}

/// The [`HeldItem`] of the models and quads `poses` holds, one entry per
/// [`HandPose`]. A quad samples the atlas `sprites` finds its texture in;
/// the rest are packed into the item's own texture. Each sheet and
/// see-through-ness becomes a part (`BakedQuad.MaterialInfo`'s render
/// type per quad).
fn bake_held(
    poses: &[Option<(&ItemModel, &[ItemQuad])>; 10],
    textures: &mut TextureCache,
    sprites: SpriteLookup,
) -> Option<HeldItem> {
    let mut places: HashMap<String, Place> = HashMap::new();
    let mut own: Vec<ItemQuad> = Vec::new();
    for (_, quads) in poses.iter().flatten() {
        for quad in quads.iter() {
            let id = &quad.texture;
            if places.contains_key(id) || own.iter().any(|q| q.texture == *id) {
                continue;
            }
            let found = if textures.is_inserted(id) {
                None
            } else {
                sprites(id)
            };
            match found {
                Some((sheet, [u0, v0, u1, v1], alpha)) => {
                    places.insert(
                        id.clone(),
                        Place {
                            sheet,
                            origin: Vec2::new(u0, v0),
                            size: Vec2::new(u1 - u0, v1 - v0),
                            alpha,
                        },
                    );
                }
                None => own.push(quad.clone()),
            }
        }
    }
    let (mut texture, mut animation) = (None, None);
    if let Some((image, own_places)) = pack_textures(&own, textures) {
        for (id, origin, size) in own_places {
            let alpha = textures.alpha(&id).unwrap_or_default();
            places.insert(
                id,
                Place {
                    sheet: Sheet::Own,
                    origin,
                    size,
                    alpha,
                },
            );
        }
        animation = animate_packed(&own, textures, &image);
        texture = Some(image);
    }
    // Per pose, each quad's part: its sheet and whether it is
    // translucent (`MaterialInfo.itemRenderType`).
    let part_of: Vec<Vec<Option<(bool, Sheet)>>> = poses
        .iter()
        .map(|pose| {
            pose.map_or(&[][..], |(_, quads)| quads)
                .iter()
                .map(|quad| {
                    let place = places.get(&quad.texture)?;
                    Some((quad.alpha.judge(&place.alpha).translucent, place.sheet))
                })
                .collect()
        })
        .collect();
    let mut groups: Vec<(bool, Sheet)> = part_of.iter().flatten().flatten().copied().collect();
    groups.sort();
    groups.dedup();
    let parts: Vec<HeldPart> = groups
        .into_iter()
        .map(|(translucent, sheet)| {
            let meshes = HandPose::ALL.map(|pose| {
                let index = pose as usize;
                poses[index]
                    .map(|(item, quads)| {
                        item.mesh_in_pose(pose, quads, |i, quad| {
                            (part_of[index][i] == Some((translucent, sheet)))
                                .then(|| places.get(&quad.texture))
                                .flatten()
                                .map(|p| (p.origin, p.size))
                        })
                    })
                    .unwrap_or_default()
            });
            HeldPart {
                sheet,
                translucent,
                meshes,
            }
        })
        .filter(|part| part.meshes.iter().any(|mesh| !mesh.is_empty()))
        .collect();
    if parts.is_empty() {
        return None;
    }
    if parts.iter().all(|part| part.sheet != Sheet::Own) {
        (texture, animation) = (None, None);
    }
    Some(HeldItem {
        texture,
        animation,
        parts,
    })
}

/// How an item is held, each with its own `display` transform.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HandPose {
    FirstPersonRight,
    FirstPersonLeft,
    ThirdPersonRight,
    ThirdPersonLeft,
    /// `ItemDisplayContext.GROUND`: what a villager holds in crossed arms.
    Ground,
    /// `ItemDisplayContext.HEAD`.
    Head,
    /// `ItemDisplayContext.FIXED`: an item in a frame or an item display.
    Fixed,
    /// `ItemDisplayContext.NONE`: the model as it is, no transform.
    Raw,
    /// `ItemDisplayContext.GUI`.
    Gui,
    /// `ItemDisplayContext.ON_SHELF`.
    OnShelf,
}

impl HandPose {
    pub const ALL: [HandPose; 10] = [
        HandPose::FirstPersonRight,
        HandPose::FirstPersonLeft,
        HandPose::ThirdPersonRight,
        HandPose::ThirdPersonLeft,
        HandPose::Ground,
        HandPose::Head,
        HandPose::Fixed,
        HandPose::Raw,
        HandPose::Gui,
        HandPose::OnShelf,
    ];

    /// Vanilla `ItemDisplayContext` by id (`ItemDisplay.DATA_ITEM_DISPLAY_ID`).
    pub fn from_display_context(id: u8) -> Self {
        match id {
            1 => Self::ThirdPersonLeft,
            2 => Self::ThirdPersonRight,
            3 => Self::FirstPersonLeft,
            4 => Self::FirstPersonRight,
            5 => Self::Head,
            6 => Self::Gui,
            7 => Self::Ground,
            8 => Self::Fixed,
            9 => Self::OnShelf,
            _ => Self::Raw,
        }
    }

    pub fn first_person(arm: crate::world::entity::player::Arm) -> Self {
        match arm {
            crate::world::entity::player::Arm::Right => HandPose::FirstPersonRight,
            crate::world::entity::player::Arm::Left => HandPose::FirstPersonLeft,
        }
    }

    pub fn third_person(arm: crate::world::entity::player::Arm) -> Self {
        match arm {
            crate::world::entity::player::Arm::Right => HandPose::ThirdPersonRight,
            crate::world::entity::player::Arm::Left => HandPose::ThirdPersonLeft,
        }
    }

    fn is_left(self) -> bool {
        matches!(self, HandPose::FirstPersonLeft | HandPose::ThirdPersonLeft)
    }

    /// `ItemDisplayContext.getSerializedName`.
    fn select_name(self) -> &'static str {
        match self {
            HandPose::FirstPersonRight => "firstperson_righthand",
            HandPose::FirstPersonLeft => "firstperson_lefthand",
            HandPose::ThirdPersonRight => "thirdperson_righthand",
            HandPose::ThirdPersonLeft => "thirdperson_lefthand",
            HandPose::Ground => "ground",
            HandPose::Head => "head",
            HandPose::Fixed => "fixed",
            HandPose::Raw => "none",
            HandPose::Gui => "gui",
            HandPose::OnShelf => "on_shelf",
        }
    }

    /// FaceBakery bakes shade in model space; world item displays keep it.
    fn bakes_shade_in_model_space(self) -> bool {
        matches!(
            self,
            HandPose::Head | HandPose::Fixed | HandPose::Raw | HandPose::Gui | HandPose::OnShelf
        )
    }

    /// The `display` entries.
    fn contexts(self) -> [&'static str; 2] {
        match self {
            HandPose::FirstPersonRight => ["firstperson_righthand"; 2],
            HandPose::FirstPersonLeft => ["firstperson_lefthand", "firstperson_righthand"],
            HandPose::ThirdPersonRight => ["thirdperson_righthand"; 2],
            HandPose::ThirdPersonLeft => ["thirdperson_lefthand", "thirdperson_righthand"],
            HandPose::Ground => ["ground"; 2],
            HandPose::Head => ["head"; 2],
            HandPose::Fixed => ["fixed"; 2],
            HandPose::Raw => [""; 2],
            HandPose::Gui => ["gui"; 2],
            HandPose::OnShelf => ["on_shelf", "fixed"],
        }
    }

    /// `ItemTransform.apply` for `model`, ending with the move from the model's centre.
    fn display(self, model: &Model) -> Mat4 {
        if self == HandPose::Raw {
            return Mat4::IDENTITY;
        }
        let centre = Mat4::from_translation(Vec3::splat(-0.5));
        let Some(display) = self.contexts().iter().find_map(|c| model.displays.get(*c)) else {
            return centre;
        };
        let mirror = if self.is_left() { -1.0 } else { 1.0 };
        let t = display.translation / 16.0;
        let r = display.rotation;
        Mat4::from_translation(Vec3::new(t.x * mirror, t.y, t.z))
            * Mat4::rot_x_deg(r.x)
            * Mat4::rot_y_deg(r.y * mirror)
            * Mat4::rot_z_deg(r.z * mirror)
            * Mat4::from_scale(display.scale)
            * centre
    }

    /// Turns the item's normals (after its display transform) to roughly where the light sees
    /// them.
    fn shade_frame(self) -> Mat3 {
        match self {
            HandPose::FirstPersonRight | HandPose::FirstPersonLeft => Mat3::IDENTITY,
            HandPose::ThirdPersonRight | HandPose::ThirdPersonLeft => {
                Mat3::from_diagonal(Vec3::new(-1.0, -1.0, 1.0))
                    * Mat3::rot_x(-HALF_PI)
                    * Mat3::rot_y(PI)
            }
            HandPose::Ground => {
                Mat3::from_diagonal(Vec3::new(-1.0, -1.0, 1.0)) * Mat3::rot_x(0.75 + PI)
            }
            HandPose::Head
            | HandPose::Fixed
            | HandPose::Raw
            | HandPose::Gui
            | HandPose::OnShelf => Mat3::IDENTITY,
        }
    }
}

/// An item ready to upload for holding: its parts, opaque ones first, and
/// the texture of its own the [`Sheet::Own`] part samples.
pub struct HeldItem {
    /// What no atlas holds, side by side; `None` when no part needs it.
    pub texture: Option<RgbaImage>,
    /// The frames of `texture` when animated, packed like it.
    pub animation: Option<HeldAnimation>,
    pub parts: Vec<HeldPart>,
}

/// The quads of a held item that sample one sheet and are opaque or not.
pub struct HeldPart {
    pub sheet: Sheet,
    /// Some texel is partly see-through (stained glass, ice): drawn
    /// blended.
    pub translucent: bool,
    /// By [`HandPose`], in [`HandPose::ALL`]'s order.
    pub meshes: [Vec<WorldVertex>; 10],
}

/// An animated held item's frames and how long each shows.
pub struct HeldAnimation {
    pub frames: Vec<RgbaImage>,
    /// `(frame, ticks)` in play order.
    pub schedule: Vec<(usize, u32)>,
}

impl HeldAnimation {
    /// The frame showing `tick` ticks in.
    pub fn frame_at(&self, tick: u64) -> usize {
        let period: u64 = self.schedule.iter().map(|&(_, t)| u64::from(t)).sum();
        let mut left = if period == 0 { 0 } else { tick % period };
        for &(frame, time) in &self.schedule {
            if left < u64::from(time) {
                return frame;
            }
            left -= u64::from(time);
        }
        0
    }
}

/// The frames of `textures`' animations packed like [`pack_textures`] does.
fn animate_packed(
    quads: &[ItemQuad],
    textures: &mut TextureCache,
    first: &RgbaImage,
) -> Option<HeldAnimation> {
    let (starts, period) = texture_animation::frame_changes(textures.animations())?;
    let mut animation = HeldAnimation {
        frames: vec![first.clone()],
        schedule: Vec::new(),
    };
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(period);
        let time = (end - start) as u32;
        if i > 0 {
            textures.set_tick(start);
            let (image, _) = pack_textures(quads, textures)?;
            if animation.frames.last() == Some(&image) {
                if let Some(last) = animation.schedule.last_mut() {
                    last.1 += time;
                    continue;
                }
            } else {
                animation.frames.push(image);
            }
        }
        animation.schedule.push((animation.frames.len() - 1, time));
    }
    textures.set_tick(0);
    (animation.frames.len() > 1).then_some(animation)
}

/// Each texture's id and its corners in a packed image.
type PackedRects = Vec<(String, Vec2, Vec2)>;

/// Every texture `quads` sample side by side in one image.
fn pack_textures(
    quads: &[ItemQuad],
    textures: &mut TextureCache,
) -> Option<(RgbaImage, PackedRects)> {
    let mut images: Vec<(String, RgbaImage)> = Vec::new();
    for quad in quads {
        if images.iter().any(|(id, _)| *id == quad.texture) {
            continue;
        }
        if let Some(image) = textures.get(&quad.texture) {
            images.push((quad.texture.clone(), image.clone()));
        }
    }
    let width: u32 = images.iter().map(|(_, image)| image.width()).sum();
    let height = images.iter().map(|(_, image)| image.height()).max()?;
    let mut packed = RgbaImage::new(width, height);
    let mut places = Vec::with_capacity(images.len());
    let mut x = 0;
    for (id, image) in images {
        image::imageops::replace(&mut packed, &image, i64::from(x), 0);
        let size = Vec2::new(image.width() as f32, image.height() as f32);
        let whole = Vec2::new(width as f32, height as f32);
        places.push((id, Vec2::new(x as f32, 0.0) / whole, size / whole));
        x += image.width();
    }
    Some((packed, places))
}

/// Follows an item model's selectors.
fn collect_layers(
    model: &Value,
    transform: Mat4,
    state: &State,
    out: &mut Vec<Layer>,
) -> Option<()> {
    let (using, count) = (state.using, state.count);
    let transform = transform * transformation(&model["transformation"]);
    let kind = model["type"].as_str()?.trim_start_matches("minecraft:");
    match kind {
        "model" => {
            let tints = model["tints"].as_array().cloned().unwrap_or_default();
            out.push(Layer {
                source: Source::Model {
                    model: model["model"].as_str()?.to_owned(),
                    tints: tints.iter().map(|t| tint_color(t, state.variant)).collect(),
                    tint_kinds: tints
                        .iter()
                        .map(|t| {
                            t["type"]
                                .as_str()
                                .unwrap_or("")
                                .trim_start_matches("minecraft:")
                                .to_owned()
                        })
                        .collect(),
                },
                transform,
            });
        }
        "special" => out.push(Layer {
            source: Source::Special {
                base: model["base"].as_str()?.to_owned(),
                spec: model["model"].clone(),
            },
            transform,
        }),
        "condition" => {
            let on = match model["property"].as_str().map(trim_namespace) {
                Some("using_item") => using,
                Some("broken") => state.variant.is_some_and(|v| v.broken),
                Some("custom_model_data") => state
                    .variant
                    .and_then(|v| v.flags.get(index_of(model)))
                    .copied()
                    .unwrap_or(false),
                Some("fishing_rod/cast") => state.cast,
                Some("has_component") => has_component(model, state.variant),
                Some("bundle/has_selected_item") => false,
                other => {
                    if crate::probe::enabled() {
                        crate::probe::count(
                            "item.unsupported",
                            format!("condition {}", other.unwrap_or("?")),
                        );
                    }
                    false
                }
            };
            let branch = if on { "on_true" } else { "on_false" };
            collect_layers(&model[branch], transform, state, out)?;
        }
        "select" => {
            let cases = model["cases"].as_array();
            let property = model["property"].as_str().map(trim_namespace);
            if property == Some("custom_model_data") {
                let value = state
                    .variant
                    .and_then(|v| v.strings.get(index_of(model)))
                    .map(String::as_str);
                let case = value.and_then(|value| {
                    cases?
                        .iter()
                        .find(|case| when_has(&case["when"], value))
                        .map(|case| &case["model"])
                });
                let chosen =
                    case.or_else(|| model["fallback"].is_object().then(|| &model["fallback"]));
                if let Some(chosen) = chosen {
                    collect_layers(chosen, transform, state, out)?;
                }
                return Some(());
            }
            if property == Some("display_context") {
                let value = state.display.select_name();
                let case = cases.and_then(|cases| {
                    cases
                        .iter()
                        .find(|case| matches_display_context(&case["when"], value))
                });
                let chosen = case
                    .map(|case| &case["model"])
                    .or_else(|| model["fallback"].is_object().then(|| &model["fallback"]));
                if let Some(chosen) = chosen {
                    collect_layers(chosen, transform, state, out)?;
                }
                return Some(());
            }
            if property == Some("trim_material") {
                let value = state.variant.and_then(|v| v.trim_material.as_deref());
                let case = value.and_then(|value| {
                    cases?.iter().find(|case| {
                        when_has(&case["when"], value)
                            || when_has(&case["when"], trim_namespace(value))
                    })
                });
                let chosen = case
                    .map(|case| &case["model"])
                    .or_else(|| model["fallback"].is_object().then(|| &model["fallback"]));
                if let Some(chosen) = chosen {
                    collect_layers(chosen, transform, state, out)?;
                }
                return Some(());
            }
            let chosen = if model["fallback"].is_object() {
                &model["fallback"]
            } else {
                &cases?.first()?["model"]
            };
            collect_layers(chosen, transform, state, out)?;
        }
        "range_dispatch" => {
            if model["property"].as_str().map(trim_namespace) == Some("custom_model_data") {
                let value = state
                    .variant
                    .and_then(|v| v.float(index_of(model)))
                    .unwrap_or(0.0);
                let value = f64::from(value) * model["scale"].as_f64().unwrap_or(1.0);
                let chosen = highest_entry(model, value)
                    .or_else(|| model["fallback"].is_object().then(|| &model["fallback"]));
                if let Some(chosen) = chosen {
                    collect_layers(chosen, transform, state, out)?;
                }
                return Some(());
            }
            let counted = (model["property"] == "minecraft:count")
                .then(|| count_entry(model, count))
                .flatten();
            let chosen = if let Some(entry) = counted {
                entry
            } else if model["fallback"].is_object() {
                &model["fallback"]
            } else {
                &model["entries"].as_array()?.first()?["model"]
            };
            collect_layers(chosen, transform, state, out)?;
        }
        "composite" => {
            for part in model["models"].as_array()? {
                collect_layers(part, transform, state, out);
            }
        }
        "empty" => {}
        other => {
            if crate::probe::enabled() {
                crate::probe::count("item.unsupported", format!("model type {other}"));
            }
            return None;
        }
    }
    Some(())
}

/// What a held stack is doing, as far as its model tells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HeldUse {
    #[default]
    Idle,
    /// `using_item`.
    Using,
    /// `fishing_rod/cast` (`FishingRodCast`): a bobber of the holder's is out.
    Cast,
}

impl From<bool> for HeldUse {
    fn from(using: bool) -> Self {
        if using { Self::Using } else { Self::Idle }
    }
}

/// What an item definition is followed with besides the stack size.
struct State<'a> {
    using: bool,
    /// `fishing_rod/cast`: the holder has a bobber out.
    cast: bool,
    count: Count,
    variant: Option<&'a Variant>,
    /// `SelectItemModelProperties.DisplayContext`.
    display: HandPose,
}

/// `HasComponent.get`: `has` or `hasNonDefault` of the named component.
fn has_component(model: &Value, variant: Option<&Variant>) -> bool {
    let Some(id) = model["component"]
        .as_str()
        .and_then(DataComponent::try_from_name)
    else {
        return false;
    };
    let Some(variant) = variant else {
        return false;
    };
    let bit = 1u128 << id.to_id();
    if model["ignore_default"].as_bool().unwrap_or(false) {
        variant.patched & bit != 0
    } else {
        variant.present & bit != 0
    }
}

fn trim_namespace(id: &str) -> &str {
    id.trim_start_matches("minecraft:")
}

/// A property's `index` (default 0).
fn index_of(model: &Value) -> usize {
    model["index"].as_u64().unwrap_or(0) as usize
}

/// Whether a `select` case's `when` (one value or a list) has `value`.
fn when_has(when: &Value, value: &str) -> bool {
    match when {
        Value::String(s) => s == value,
        Value::Array(values) => values.iter().any(|v| when_has(v, value)),
        _ => false,
    }
}

/// The entry of a range dispatch with the highest threshold `value`
fn highest_entry(model: &Value, value: f64) -> Option<&Value> {
    model["entries"]
        .as_array()?
        .iter()
        .filter(|entry| entry["threshold"].as_f64().is_some_and(|t| t <= value))
        .max_by(|a, b| {
            let (a, b) = (a["threshold"].as_f64(), b["threshold"].as_f64());
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|entry| &entry["model"])
}

/// The entry a `minecraft:count` dispatch picks for `count`.
fn count_entry(model: &Value, count: Count) -> Option<&Value> {
    let (unit, scale) = count_unit(model, count.max_stack);
    let value = f64::from(count.value) / unit * scale;
    model["entries"]
        .as_array()?
        .iter()
        .filter(|entry| entry["threshold"].as_f64().is_some_and(|t| t <= value))
        .max_by(|a, b| {
            let (a, b) = (a["threshold"].as_f64(), b["threshold"].as_f64());
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|entry| &entry["model"])
}

/// `SelectItemModel` `display_context`: `gui`, `minecraft:gui`, or a list.
fn matches_display_context(when: &Value, name: &str) -> bool {
    match when {
        Value::String(s) => s.trim_start_matches("minecraft:") == name,
        Value::Array(values) => values.iter().any(|v| matches_display_context(v, name)),
        _ => false,
    }
}

/// `Transformation`.
fn transformation(value: &Value) -> Mat4 {
    if let Some(rows) = value.as_array() {
        let numbers: Vec<f32> = rows
            .iter()
            .filter_map(|n| n.as_f64())
            .map(|n| n as f32)
            .collect();
        return match <[f32; 16]>::try_from(numbers) {
            Ok(rows) => Mat4::from_cols_array(&rows).transpose(),
            Err(_) => Mat4::IDENTITY,
        };
    }
    if !value.is_object() {
        return Mat4::IDENTITY;
    }
    let rotation = |value: &Value| -> Quat {
        if let Some([x, y, z, w]) = value
            .as_array()
            .and_then(|a| <[Value; 4]>::try_from(a.clone()).ok())
            .map(|a| a.map(|n| n.as_f64().unwrap_or(0.0) as f32))
        {
            return Quat::from_xyzw(x, y, z, w).normalize();
        }
        match (vec3(&value["axis"]), value["angle"].as_f64()) {
            (Some(axis), Some(angle)) => {
                angle::axis_angle(axis.normalize_or_zero(), angle as f32 * DEG_TO_RAD)
            }
            _ => Quat::IDENTITY,
        }
    };
    Mat4::from_translation(vec3(&value["translation"]).unwrap_or(Vec3::ZERO))
        * Mat4::from_quat(rotation(&value["left_rotation"]))
        * Mat4::from_scale(vec3(&value["scale"]).unwrap_or(Vec3::ONE))
        * Mat4::from_quat(rotation(&value["right_rotation"]))
}

/// `ItemTintSource`, evaluated without a world: its default colour.
fn tint_color(source: &Value, variant: Option<&Variant>) -> [u8; 3] {
    let kind = source["type"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches("minecraft:");
    let argb = match kind {
        "grass" => return GRASS_TINT,
        "foliage" => return FOLIAGE_TINT,
        "constant" => source["value"].as_i64(),
        "custom_model_data" => variant
            .and_then(|v| v.colors.get(index_of(source)).copied())
            .map(i64::from)
            .or_else(|| source["default"].as_i64()),
        "dye" => variant
            .and_then(|v| v.dye)
            .map(i64::from)
            .or_else(|| source["default"].as_i64()),
        _ => source["default"].as_i64(),
    };
    match argb {
        Some(argb) => {
            let argb = argb as u32;
            [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8]
        }
        None => [255, 255, 255],
    }
}

/// One face of an item, in the item's model space.
#[derive(Clone)]
pub struct ItemQuad {
    /// Counter-clockwise from outside.
    pub corners: [Vec3; 4],
    /// `0..1` within `texture`.
    pub uvs: [Vec2; 4],
    pub normal: Vec3,
    pub texture: String,
    pub tint: [u8; 3],
    /// The element's `shade`, default true (`FaceBakery`).
    pub shade: bool,
    /// What its render type's see-through-ness is read from
    /// (`BakedQuad.MaterialInfo`).
    pub alpha: AlphaRule,
}

/// `layer`'s faces through its transform into `out`.
fn layer_quads(
    assets_root: &Path,
    layer: &Layer,
    textures: &mut TextureCache,
    out: &mut Vec<ItemQuad>,
) {
    let transform = layer.transform;
    let normal_matrix = Mat3::from_mat4(transform);
    let mirrors = normal_matrix.determinant() < 0.0;
    match &layer.source {
        Source::Model { model, tints, .. } => {
            let Some(model) = Model::resolve(assets_root, model) else {
                return;
            };
            let generated;
            let elements = if model.generated {
                generated = extruded_elements(&model, textures);
                &generated
            } else {
                &model.elements
            };
            for element in elements {
                let shade = element["shade"].as_bool().unwrap_or(true);
                for mut quad in element_quads(element) {
                    if mirrors {
                        quad.corners.reverse();
                        quad.uvs.reverse();
                    }
                    let Some(texture) = model.texture(&quad.texture) else {
                        continue;
                    };
                    // `ItemModelGenerator` takes the sprite's, `FaceBakery`
                    // the face's.
                    let alpha = if model.generated {
                        AlphaRule::Whole
                    } else if model.forced_translucent.contains(&texture) {
                        AlphaRule::Translucent
                    } else {
                        AlphaRule::region(&quad.uvs)
                    };
                    out.push(ItemQuad {
                        corners: quad.corners.map(|c| transform.transform_point3(c / 16.0)),
                        uvs: quad.uvs,
                        normal: (normal_matrix * quad.normal).normalize_or_zero(),
                        texture,
                        tint: quad
                            .tint
                            .and_then(|i| tints.get(i).copied())
                            .unwrap_or([255, 255, 255]),
                        shade,
                        alpha,
                    });
                }
            }
        }
        Source::Special { spec, .. } => {
            for part in block_entity::item::special_parts(spec).unwrap_or_default() {
                model_part::faces(&part.part, transform, &mut |face| {
                    let mut corners = face.corners;
                    let mut uvs = face.texels.map(|t| t / part.texture_size);
                    if face.flipped {
                        corners.reverse();
                        uvs.reverse();
                    }
                    out.push(ItemQuad {
                        corners,
                        uvs,
                        normal: face.normal,
                        texture: part.texture.clone(),
                        tint: [255, 255, 255],
                        shade: true,
                        alpha: AlphaRule::Whole,
                    });
                });
            }
        }
    }
}

/// `ItemModelGenerator`.
fn extruded_elements(model: &Model, textures: &mut TextureCache) -> Vec<Value> {
    const SHRINK: f32 = 0.1;
    let mut elements = Vec::new();
    for layer in 0.. {
        let name = format!("layer{layer}");
        let Some(id) = model.texture(&name) else {
            break;
        };
        let texture = format!("#{name}");
        let face = |uv: [f32; 4]| json!({ "uv": uv, "texture": texture, "tintindex": layer });
        elements.push(json!({
            "from": [0.0, 0.0, 7.5],
            "to": [16.0, 16.0, 8.5],
            "faces": {
                "south": face([0.0, 0.0, 16.0, 16.0]),
                "north": face([16.0, 0.0, 0.0, 16.0]),
            },
        }));
        let Some(image) = textures.get(&id) else {
            continue;
        };
        let (width, height) = (image.width() as i32, image.height() as i32);
        let (sx, sy) = (16.0 / width as f32, 16.0 / height as f32);
        let clear = |x: i32, y: i32| {
            x < 0
                || y < 0
                || x >= width
                || y >= height
                || image.get_pixel(x as u32, y as u32)[3] == 0
        };
        for y in 0..height {
            for x in 0..width {
                if clear(x, y) {
                    continue;
                }
                let (fx, fy) = (x as f32, y as f32);
                let (left, right) = (fx * sx, (fx + 1.0) * sx);
                let (top, bottom) = (16.0 - fy * sy, 16.0 - (fy + 1.0) * sy);
                let (u0, u1) = ((fx + SHRINK) * sx, (fx + 1.0 - SHRINK) * sx);
                let (v_top, v_bottom) = ((fy + SHRINK) * sy, (fy + 1.0 - SHRINK) * sy);
                for (side, dx, dy) in [
                    ("up", 0, -1),
                    ("down", 0, 1),
                    ("west", -1, 0),
                    ("east", 1, 0),
                ] {
                    if !clear(x + dx, y + dy) {
                        continue;
                    }
                    let (from, to, uv) = match side {
                        "up" => (
                            [left, top, 7.5],
                            [right, top, 8.5],
                            [u0, v_top, u1, v_bottom],
                        ),
                        "down" => (
                            [left, bottom, 7.5],
                            [right, bottom, 8.5],
                            [u0, v_top, u1, v_bottom],
                        ),
                        "west" => (
                            [left, bottom, 7.5],
                            [left, top, 8.5],
                            [u0, v_bottom, u1, v_top],
                        ),
                        _ => (
                            [right, bottom, 7.5],
                            [right, top, 8.5],
                            [u0, v_bottom, u1, v_top],
                        ),
                    };
                    elements.push(json!({
                        "from": from,
                        "to": to,
                        "faces": { side: face(uv) },
                    }));
                }
            }
        }
    }
    elements
}

#[cfg(test)]
mod tests {
    /// The models `definition` shows for a stack with `variant`.
    fn shown(definition: &Value, variant: Option<&Variant>) -> Vec<String> {
        let state = State {
            using: false,
            cast: false,
            count: Count::ONE,
            variant,
            display: HandPose::Gui,
        };
        let mut layers = Vec::new();
        collect_layers(definition, Mat4::IDENTITY, &state, &mut layers);
        layers.iter().map(|l| l.source.model().to_owned()).collect()
    }

    fn model(id: &str) -> Value {
        json!({"type": "minecraft:model", "model": id})
    }

    fn data(floats: &[f32], strings: &[&str], flags: &[bool]) -> Variant {
        Variant {
            floats: floats.iter().map(|f| f.to_bits()).collect(),
            strings: strings.iter().map(|s| (*s).to_owned()).collect(),
            flags: flags.to_vec(),
            ..Variant::default()
        }
    }

    #[test]
    fn custom_model_data_picks_a_range_entry_by_its_float() {
        let definition = json!({
            "type": "minecraft:range_dispatch",
            "property": "minecraft:custom_model_data",
            "index": 1,
            "scale": 2.0,
            "fallback": model("base"),
            "entries": [
                {"threshold": 10, "model": model("ten")},
                {"threshold": 1024, "model": model("big")}
            ]
        });
        assert_eq!(shown(&definition, None), ["base"]);
        assert_eq!(
            shown(&definition, Some(&data(&[0.0, 4.0], &[], &[]))),
            ["base"]
        );
        assert_eq!(
            shown(&definition, Some(&data(&[0.0, 5.0], &[], &[]))),
            ["ten"]
        );
        assert_eq!(
            shown(&definition, Some(&data(&[0.0, 512.0], &[], &[]))),
            ["big"]
        );
        assert_eq!(
            shown(&definition, Some(&data(&[512.0], &[], &[]))),
            ["base"]
        );
    }

    #[test]
    fn custom_model_data_selects_a_case_by_its_string() {
        let definition = json!({
            "type": "minecraft:select",
            "property": "minecraft:custom_model_data",
            "fallback": model("plain"),
            "cases": [
                {"when": "red", "model": model("red")},
                {"when": ["green", "lime"], "model": model("green")}
            ]
        });
        assert_eq!(
            shown(&definition, Some(&data(&[], &["lime"], &[]))),
            ["green"]
        );
        assert_eq!(shown(&definition, Some(&data(&[], &["red"], &[]))), ["red"]);
        assert_eq!(
            shown(&definition, Some(&data(&[], &["blue"], &[]))),
            ["plain"]
        );
        assert_eq!(shown(&definition, None), ["plain"]);
        let mut bare = definition;
        bare["fallback"] = Value::Null;
        assert!(shown(&bare, Some(&data(&[], &["blue"], &[]))).is_empty());
    }

    #[test]
    fn custom_model_data_flags_drive_a_condition() {
        let definition = json!({
            "type": "minecraft:condition",
            "property": "minecraft:custom_model_data",
            "index": 1,
            "on_true": model("lit"),
            "on_false": model("dark")
        });
        assert_eq!(
            shown(&definition, Some(&data(&[], &[], &[false, true]))),
            ["lit"]
        );
        assert_eq!(shown(&definition, Some(&data(&[], &[], &[true]))), ["dark"]);
        assert_eq!(shown(&definition, None), ["dark"]);
    }

    #[test]
    fn a_bundle_has_selected_item_condition_follows_the_false_branch() {
        let definition = json!({
            "type": "minecraft:condition",
            "property": "minecraft:bundle/has_selected_item",
            "on_true": model("open"),
            "on_false": model("closed")
        });
        assert_eq!(shown(&definition, None), ["closed"]);
    }

    #[test]
    fn a_broken_condition_follows_next_damage_will_break() {
        let definition = json!({
            "type": "minecraft:condition",
            "property": "minecraft:broken",
            "on_true": model("broken"),
            "on_false": model("whole")
        });
        let broken = Variant {
            broken: true,
            ..Variant::default()
        };
        assert_eq!(shown(&definition, Some(&broken)), ["broken"]);
        assert_eq!(shown(&definition, Some(&Variant::default())), ["whole"]);
        assert_eq!(shown(&definition, None), ["whole"]);
    }

    #[test]
    fn trim_material_selects_the_matching_case() {
        let definition = json!({
            "type": "minecraft:select",
            "property": "minecraft:trim_material",
            "cases": [
                {"when": "minecraft:gold", "model": {"type": "minecraft:model", "model": "gold"}},
                {"when": "minecraft:iron", "model": {"type": "minecraft:model", "model": "iron"}}
            ],
            "fallback": {"type": "minecraft:model", "model": "plain"}
        });
        let gold = Variant {
            trim_material: Some("minecraft:gold".into()),
            ..Variant::default()
        };
        assert_eq!(shown(&definition, Some(&gold)), ["gold"]);
        assert_eq!(shown(&definition, Some(&Variant::default())), ["plain"]);
        assert_eq!(shown(&definition, None), ["plain"]);
    }

    #[test]
    fn has_component_follows_the_stack_patch() {
        let dyed = json!({
            "type": "minecraft:condition",
            "property": "minecraft:has_component",
            "component": "minecraft:dyed_color",
            "on_true": model("dyed"),
            "on_false": model("plain")
        });
        let bit = 1u128 << DataComponent::DyedColor.to_id();
        let with = Variant {
            present: bit,
            patched: bit,
            ..Variant::default()
        };
        assert_eq!(shown(&dyed, Some(&with)), ["dyed"]);
        assert_eq!(shown(&dyed, Some(&Variant::default())), ["plain"]);
        assert_eq!(shown(&dyed, None), ["plain"]);
        let ignore = json!({
            "type": "minecraft:condition",
            "property": "minecraft:has_component",
            "component": "minecraft:dyed_color",
            "ignore_default": true,
            "on_true": model("dyed"),
            "on_false": model("plain")
        });
        let defaults_only = Variant {
            present: bit,
            patched: 0,
            ..Variant::default()
        };
        assert_eq!(shown(&ignore, Some(&defaults_only)), ["plain"]);
        assert_eq!(shown(&ignore, Some(&with)), ["dyed"]);
    }

    #[test]
    fn custom_model_data_and_dye_tints_read_the_stack() {
        let colored =
            json!({"type": "minecraft:custom_model_data", "index": 1, "default": 0xFFFFFF});
        let dye = json!({"type": "minecraft:dye", "default": 0xA06540});
        let variant = Variant {
            colors: vec![0x112233, 0x445566],
            dye: Some(0x010203),
            ..Variant::default()
        };
        assert_eq!(tint_color(&colored, Some(&variant)), [0x44, 0x55, 0x66]);
        assert_eq!(tint_color(&colored, None), [255, 255, 255]);
        assert_eq!(tint_color(&dye, Some(&variant)), [1, 2, 3]);
        assert_eq!(tint_color(&dye, None), [0xA0, 0x65, 0x40]);
    }

    #[test]
    fn display_context_ids_match_vanilla() {
        assert_eq!(HandPose::from_display_context(6), HandPose::Gui);
        assert_eq!(HandPose::from_display_context(8), HandPose::Fixed);
        assert_eq!(HandPose::from_display_context(9), HandPose::OnShelf);
        assert_eq!(HandPose::from_display_context(0), HandPose::Raw);
    }

    #[test]
    fn display_context_selects_the_matching_case() {
        let definition = json!({
            "type": "minecraft:select",
            "property": "minecraft:display_context",
            "cases": [
                {"when": "gui", "model": model("flat")},
                {"when": ["fixed", "minecraft:on_shelf"], "model": model("block")}
            ],
            "fallback": model("plain")
        });
        let shown_as = |display: HandPose| {
            let state = State {
                using: false,
                cast: false,
                count: Count::ONE,
                variant: None,
                display,
            };
            let mut layers = Vec::new();
            collect_layers(&definition, Mat4::IDENTITY, &state, &mut layers);
            layers
                .iter()
                .map(|l| l.source.model().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(shown_as(HandPose::Gui), ["flat"]);
        assert_eq!(shown_as(HandPose::Fixed), ["block"]);
        assert_eq!(shown_as(HandPose::OnShelf), ["block"]);
        assert_eq!(shown_as(HandPose::Ground), ["plain"]);
        assert_eq!(shown_as(HandPose::Raw), ["plain"]);
    }

    #[test]
    fn an_item_model_component_names_another_definition() {
        let variant = Variant {
            model: Some("hugo:sword".to_owned()),
            ..Variant::default()
        };
        assert_eq!(definition_id("stone", Some(&variant)), "hugo:sword");
        assert_eq!(definition_id("stone", None), "stone");
    }

    fn stick_dispatch() -> Value {
        json!({
            "type": "minecraft:range_dispatch",
            "property": "minecraft:count",
            "normalize": false,
            "entries": [
                {"threshold": 1, "model": {"type": "minecraft:model", "model": "item/stick"}},
                {"threshold": 2, "model": {"type": "minecraft:model", "model": "item/s1"}},
                {"threshold": 17, "model": {"type": "minecraft:model", "model": "item/s2"}}
            ]
        })
    }

    #[test]
    fn a_count_dispatch_picks_by_stack_size() {
        let model = stick_dispatch();
        let pick = |value| {
            count_entry(
                &model,
                Count {
                    value,
                    max_stack: 64,
                },
            )
            .map(|m| m["model"].as_str().unwrap())
        };
        assert_eq!(pick(1), Some("item/stick"));
        assert_eq!(pick(16), Some("item/s1"));
        assert_eq!(pick(17), Some("item/s2"));
        assert_eq!(pick(64), Some("item/s2"));
        let mut cuts = Vec::new();
        count_cuts(&json!({"fallback": model}), 64, &mut cuts);
        assert_eq!(cuts, [1, 2, 17]);
        let normalized = json!({"property": "minecraft:count", "entries": [
            {"threshold": 0.5, "model": {}}]});
        let mut cuts = Vec::new();
        count_cuts(&normalized, 64, &mut cuts);
        assert_eq!(cuts, [32]);
    }

    #[test]
    fn a_held_animation_follows_its_schedule() {
        let frame = || RgbaImage::new(1, 1);
        let animation = HeldAnimation {
            frames: vec![frame(), frame()],
            schedule: vec![(0, 2), (1, 3)],
        };
        let shown: Vec<_> = (0..6).map(|t| animation.frame_at(t)).collect();
        assert_eq!(shown, [0, 0, 1, 1, 1, 0]);
    }

    use super::*;

    fn assets() -> Option<&'static Path> {
        static ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(crate::assets::default_root).as_path();
        root.join("minecraft/items/stone.json")
            .is_file()
            .then_some(root)
    }

    /// The world pipeline culls back faces, so every face must wind around its normal.
    #[test]
    fn faces_wind_around_their_normals() {
        let Some(root) = assets() else {
            eprintln!("no extracted assets, skipping");
            return;
        };
        for key in [
            "diamond_sword",
            "stone",
            "oak_stairs",
            "chest",
            "shield",
            "red_banner",
            "white_bed",
        ] {
            let item = ItemModel::resolve(root, key, None).expect(key);
            let mut textures = TextureCache::new(root);
            let quads = item.quads(root, &mut textures);
            assert!(!quads.is_empty(), "{key}");
            for quad in &quads {
                let [a, b, c, _] = quad.corners;
                let winding = (b - a).cross(c - a);
                if winding.length() > 1e-6 {
                    assert!(winding.dot(quad.normal) > 0.0, "{key}: {:?}", quad.corners);
                }
            }
        }
    }

    /// The item atlas holds `textures/item/`, so a sword needs no texture
    /// of its own; a block item samples the block atlas; a banner's flag,
    /// composed per colour, stays the item's own.
    #[test]
    fn held_items_sample_the_atlases_and_keep_only_the_rest() {
        let Some(root) = assets() else {
            eprintln!("no extracted assets, skipping");
            return;
        };
        let ids = crate::world::item_atlas::texture_ids(root);
        assert!(ids.iter().any(|id| id == "minecraft:item/diamond_sword"));
        let count = ids.len();
        let atlas = crate::world::block_models::pack_sprites(root, ids, 4096);
        eprintln!(
            "item atlas: {count} textures, {}x{}",
            atlas.image.width(),
            atlas.image.height()
        );
        let lookup = |id: &str| {
            let id = crate::world::item_atlas::canonical(id);
            atlas
                .sprites
                .get(&id)
                .map(|(rect, alpha)| (Sheet::Items, *rect, alpha.clone()))
                .or_else(|| {
                    id.contains(":block/")
                        .then(|| (Sheet::Blocks, [0.0, 0.0, 1.0, 1.0], AlphaMap::default()))
                })
        };
        let held = |key: &str| {
            ItemModel::resolve(root, key, None)
                .expect(key)
                .held(root, &lookup)
                .expect(key)
        };
        let sheets = |item: &HeldItem| item.parts.iter().map(|p| p.sheet).collect::<Vec<_>>();

        let sword = held("diamond_sword");
        assert_eq!(sheets(&sword), [Sheet::Items]);
        assert!(sword.texture.is_none());
        let [u0, v0, u1, v1] = atlas.sprites["minecraft:item/diamond_sword"].0;
        for v in sword.parts[0].meshes.iter().flatten() {
            let [u, w] = v.uv;
            assert!((u0..=u1).contains(&u) && (v0..=v1).contains(&w), "{u} {w}");
        }

        // Trim permutations are recoloured into the atlas, not per item.
        let trim = &atlas.sprites["minecraft:trims/items/chestplate_trim_gold"];
        let [u0, v0, u1, v1] = trim.0;
        assert!(u1 > u0 && v1 > v0);
        let gold = crate::assets::texture_image(root, "minecraft:trims/items/chestplate_trim_gold")
            .expect("gold chestplate trim");
        assert!(gold.pixels().any(|p| p[3] == 255));

        let stone = held("stone");
        assert_eq!(sheets(&stone), [Sheet::Blocks]);
        assert!(stone.texture.is_none());

        let banner = held("red_banner");
        assert!(
            sheets(&banner).contains(&Sheet::Own),
            "{:?}",
            sheets(&banner)
        );
        assert!(banner.texture.is_some());
    }

    #[test]
    fn a_flat_item_is_one_pixel_thick_with_side_walls() {
        let Some(root) = assets() else {
            eprintln!("no extracted assets, skipping");
            return;
        };
        let item = ItemModel::resolve(root, "diamond_sword", None).unwrap();
        let quads = item.quads(root, &mut TextureCache::new(root));
        let z = quads.iter().flat_map(|q| q.corners.map(|c| c.z));
        let (min, max) = z.fold((f32::MAX, f32::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
        assert!((min - 7.5 / 16.0).abs() < 1e-5 && (max - 8.5 / 16.0).abs() < 1e-5);
        assert!(quads.len() > 2, "front and back plus side walls");
        let held = item.held(root, &|_| None).expect("a sword can be held");
        assert!(held.texture.is_some(), "no atlas: its own texture");
        assert_eq!(held.parts.len(), 1, "one texture, opaque");
        assert!(
            held.parts[0]
                .meshes
                .iter()
                .all(|mesh| mesh.len() == quads.len() * 4)
        );
    }
}
