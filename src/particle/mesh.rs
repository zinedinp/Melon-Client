//! Drawing the particles: each frame one camera-facing quad per particle

use std::collections::HashMap;
use std::path::Path;

use glam::{DVec3, IVec3, Mat4, Quat, Vec3};

use super::Particle;
use super::base::{Atlas, Facing};
use super::behavior::Behavior;
use crate::renderer::{EntityDraw, MeshId, RenderLayer, Renderer, TextureId, WorldVertex};
use crate::world::culling::frustum::Frustum;
use crate::world::level::Level;

/// The quads of one texture and blend mode.
#[derive(Default)]
pub struct Batch {
    pub vertices: Vec<WorldVertex>,
}

/// What the quads are built for.
pub struct View<'a> {
    /// The block the vertices are relative to: the camera's.
    pub origin: IVec3,
    /// Where the camera is: [`Self::frustum`] is relative to it.
    pub camera: DVec3,
    /// The camera's right and up in the world.
    pub right: Vec3,
    pub up: Vec3,
    /// Its yaw alone, as a rotation (`LOOKAT_Y`).
    pub yaw: Quat,
    pub partial: f32,
    pub frustum: Option<&'a Frustum>,
}

/// The light a particle gets at `pos`: block and sky in `0..=240`;
fn world_light(level: &Level, pos: DVec3) -> [u8; 2] {
    let at = pos.floor().as_ivec3();
    if !level.is_loaded_at(at.x, at.z) {
        return [0, 240];
    }
    let (block, sky) = level.light_at(at.x, at.y, at.z);
    [block.min(15) * 16, sky.min(15) * 16]
}

/// TODO(perf): measure with many particles (vanilla allows 16384). Each
/// particle hashes its batch key, looks up the world light alone and tests
/// the frustum alone; `Particle` is a large struct with an enum, so the
/// loop strides over far more bytes than it reads. Steps: batch the light
/// and frustum tests, build the corners with `Vec3A`; structure of arrays
/// only if that is not enough.
/// The quads of `particles`, sorted into `out` by `(atlas, translucent)`.
pub fn build(
    particles: &[Particle],
    level: &Level,
    view: &View,
    out: &mut HashMap<(Atlas, bool), Batch>,
) {
    let partial = view.partial;
    for p in particles {
        if !p.behavior.drawn() {
            continue;
        }
        let pos = p.lerp_pos(partial);
        let size = p.size(partial);
        if size <= 0.0 {
            continue;
        }
        if let Some(frustum) = view.frustum
            && !frustum.sees_centered_box((pos - view.camera).as_vec3(), Vec3::splat(size))
        {
            continue;
        }
        let alpha = p.alpha(partial);
        if alpha <= 0.0 {
            continue;
        }
        let mut rgba = [p.color[0], p.color[1], p.color[2], alpha];
        if let Behavior::DustTransition { from, to } = p.behavior {
            let t = (p.age as f32 + partial) / (p.lifetime as f32 + 1.0);
            for i in 0..3 {
                rgba[i] = from[i] + (to[i] - from[i]) * t;
            }
        }
        let light = p
            .look
            .glow
            .light(world_light(level, pos), p.age, p.lifetime, partial);
        let center = (pos - view.origin.as_dvec3()).as_vec3();
        let batch = out.entry((p.look.atlas, p.look.translucent)).or_default();
        let roll = p.prev_roll + (p.roll - p.prev_roll) * partial;
        match p.behavior {
            Behavior::Vibration {
                rot,
                prev_rot,
                pitch,
                prev_pitch,
                ..
            } => {
                let sway = ((p.age as f32 + partial - crate::math::angle::TAU) * 0.05).sin() * 2.0;
                let rot = prev_rot + (rot - prev_rot) * partial;
                let pitch =
                    prev_pitch + (pitch - prev_pitch) * partial + crate::math::angle::HALF_PI;
                let front = Quat::from_rotation_y(rot)
                    * Quat::from_rotation_x(-pitch)
                    * Quat::from_rotation_y(sway);
                let back = Quat::from_rotation_y(-crate::math::angle::PI + rot)
                    * Quat::from_rotation_x(pitch)
                    * Quat::from_rotation_y(sway);
                for q in [front, back] {
                    quad(
                        batch,
                        center,
                        q * Vec3::X,
                        q * Vec3::Y,
                        size,
                        p.look.uv,
                        rgba,
                        light,
                    );
                }
            }
            Behavior::Shriek { .. } => {
                use crate::math::angle::{PI, THIRD_PI};
                let front = Quat::from_rotation_x(-THIRD_PI);
                let back = Quat::from_euler(glam::EulerRot::YXZ, -PI, THIRD_PI, 0.0);
                for q in [front, back] {
                    quad(
                        batch,
                        center,
                        q * Vec3::X,
                        q * Vec3::Y,
                        size,
                        p.look.uv,
                        rgba,
                        light,
                    );
                }
            }
            _ => {
                let (mut right, mut up) = match p.look.facing {
                    Facing::Camera => (view.right, view.up),
                    Facing::Upright => (view.yaw * Vec3::X, Vec3::Y),
                };
                if roll != 0.0 {
                    let (s, c) = roll.sin_cos();
                    let (r, u) = (right, up);
                    right = r * c + u * s;
                    up = u * c - r * s;
                }
                quad(batch, center, right, up, size, p.look.uv, rgba, light);
            }
        }
    }
}

/// One quad, counter-clockwise as the camera sees it.
#[allow(clippy::too_many_arguments)]
fn quad(
    batch: &mut Batch,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    size: f32,
    [u0, v0, u1, v1]: [f32; 4],
    rgba: [f32; 4],
    light: [u8; 2],
) {
    let corner = |x: f32, y: f32, u: f32, v: f32| {
        let pos = center + (right * x + up * y) * size;
        let mut vertex = WorldVertex::rgba(pos.to_array(), [u, v], rgba);
        vertex.light = light;
        vertex
    };
    batch.vertices.extend([
        corner(-1.0, -1.0, u1, v1),
        corner(1.0, -1.0, u0, v1),
        corner(1.0, 1.0, u0, v0),
        corner(-1.0, 1.0, u1, v0),
    ]);
}

/// The particles' GPU side: this frame's meshes and the item textures
#[derive(Default)]
pub struct ParticleMeshes {
    meshes: Vec<MeshId>,
    /// By item id; `None` for one without a texture.
    items: HashMap<u16, Option<TextureId>>,
    batches: HashMap<(Atlas, bool), Batch>,
    /// Quads drawn last frame.
    pub drawn: usize,
}

impl ParticleMeshes {
    /// Uploads this frame's quads and adds their draws to `out`.
    #[allow(clippy::too_many_arguments)]
    pub fn draws(
        &mut self,
        renderer: &mut Renderer,
        assets_root: &Path,
        blocks: TextureId,
        particles: &[Particle],
        level: &Level,
        view: &View,
        out: &mut Vec<EntityDraw>,
    ) {
        for mesh in self.meshes.drain(..) {
            renderer.free_mesh(mesh);
        }
        for batch in self.batches.values_mut() {
            batch.vertices.clear();
        }
        build(particles, level, view, &mut self.batches);
        self.drawn = 0;
        for (&(atlas, translucent), batch) in &self.batches {
            if batch.vertices.is_empty() {
                continue;
            }
            let texture = match atlas {
                Atlas::Blocks => Some(blocks),
                Atlas::Item(item) => *self
                    .items
                    .entry(item)
                    .or_insert_with(|| item_texture(renderer, assets_root, item)),
            };
            let Some(texture) = texture else {
                continue;
            };
            match renderer.upload_mesh(&batch.vertices) {
                Ok(mesh) => {
                    self.meshes.push(mesh);
                    self.drawn += batch.vertices.len() / 4;
                    out.push(EntityDraw {
                        scroll: None,
                        alpha: 1.0,
                        mesh,
                        atlas: texture,
                        model: Mat4::IDENTITY,
                        origin: view.origin,
                        layer: RenderLayer::Particles,
                        blend: crate::renderer::Blending::of(translucent),
                        light: Some([1.0; 3]),
                        overlay: 0.0,
                        glint: crate::renderer::Glint::None,
                    });
                }
                Err(e) => crate::warn!("Particle mesh failed: {e:#}"),
            }
        }
        self.batches.retain(|_, batch| !batch.vertices.is_empty());
    }

    /// Frees everything, for new resources or a new level.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for mesh in self.meshes.drain(..) {
            renderer.free_mesh(mesh);
        }
        for texture in self.items.drain().filter_map(|(_, t)| t) {
            renderer.free_texture(texture);
        }
        self.batches.clear();
    }
}

/// An item's particle texture (its model's `particle`, else `layer0`):
fn item_texture(renderer: &mut Renderer, assets_root: &Path, item: u16) -> Option<TextureId> {
    let key = pumpkin_data::item::Item::from_id(item)?.registry_key;
    let model = crate::world::item_model::ItemModel::resolve(assets_root, key, None)?;
    let id = model
        .model
        .texture("particle")
        .or_else(|| model.model.texture("layer0"))?;
    let mut image = crate::assets::texture_image(assets_root, &id)?;
    if image.height() > image.width() {
        let side = image.width();
        image = image::imageops::crop_imm(&image, 0, 0, side, side).to_image();
    }
    renderer.load_texture(&image).ok()
}
