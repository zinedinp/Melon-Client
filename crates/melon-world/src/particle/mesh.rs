//! Drawing the particles.

use std::collections::HashMap;
use std::path::Path;

use glam::{DVec3, IVec3, Quat, Vec3};

use super::Particle;
use super::base::{Atlas, Facing};
use super::behavior::Behavior;
use crate::renderer::{ParticleDraw, ParticleMeshId, ParticleQuad, Renderer, TextureId};
use crate::world::culling::frustum::Frustum;
use crate::world::level::Level;

/// The quads of one texture and blend mode.
#[derive(Default)]
pub struct Batch {
    pub quads: Vec<ParticleQuad>,
    /// Per quad its `right` and `up`, until [`ParticleQuad::encode_axes`].
    pub axes: Vec<f32>,
}

impl Batch {
    fn clear(&mut self) {
        self.quads.clear();
        self.axes.clear();
    }
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
                        (q * Vec3::X, q * Vec3::Y),
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
                        (q * Vec3::X, q * Vec3::Y),
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
                quad(batch, center, (right, up), size, p.look.uv, rgba, light);
            }
        }
    }
}

/// One quad, counter-clockwise as the camera sees it: `size` along `right`
/// and `up` from `center`.
fn quad(
    batch: &mut Batch,
    center: Vec3,
    (right, up): (Vec3, Vec3),
    size: f32,
    uv: [f32; 4],
    rgba: [f32; 4],
    light: [u8; 2],
) {
    let (right, up) = (right * size, up * size);
    batch
        .quads
        .push(ParticleQuad::new(center.to_array(), uv, rgba, light));
    batch
        .axes
        .extend_from_slice(&[right.x, right.y, right.z, up.x, up.y, up.z]);
}

/// The particles' GPU side.
#[derive(Default)]
pub struct ParticleMeshes {
    meshes: Vec<ParticleMeshId>,
    /// By item id; `None` for one without a texture.
    items: HashMap<u16, Option<TextureId>>,
    batches: HashMap<(Atlas, bool), Batch>,
    /// The axes' F16 bits on their way into the quads.
    bits: Vec<u16>,
    /// Quads drawn last frame.
    pub drawn: usize,
}

impl ParticleMeshes {
    /// Uploads this frame's quads and adds their draws to `out`. Item textures
    /// load from `assets_root`; block particles use the `blocks` atlas.
    pub fn draws(
        &mut self,
        renderer: &mut Renderer,
        (assets_root, blocks): (&Path, TextureId),
        particles: &[Particle],
        level: &Level,
        view: &View,
        out: &mut Vec<ParticleDraw>,
    ) {
        for mesh in self.meshes.drain(..) {
            renderer.free_particles(mesh);
        }
        for batch in self.batches.values_mut() {
            batch.clear();
        }
        build(particles, level, view, &mut self.batches);
        self.drawn = 0;
        for (&(atlas, translucent), batch) in &mut self.batches {
            if batch.quads.is_empty() {
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
            ParticleQuad::encode_axes(&mut batch.quads, &batch.axes, &mut self.bits);
            match renderer.upload_particles(&batch.quads) {
                Ok(mesh) => {
                    self.meshes.push(mesh);
                    self.drawn += batch.quads.len();
                    out.push(ParticleDraw {
                        mesh,
                        atlas: texture,
                        origin: view.origin,
                        translucent,
                    });
                }
                Err(e) => crate::warn!("Particle mesh failed: {e:#}"),
            }
        }
        self.batches.retain(|_, batch| !batch.quads.is_empty());
    }

    /// Frees everything, for new resources or a new level.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for mesh in self.meshes.drain(..) {
            renderer.free_particles(mesh);
        }
        for texture in self.items.drain().filter_map(|(_, t)| t) {
            renderer.free_texture(texture);
        }
        self.batches.clear();
    }
}

/// An item's particle texture (its model's `particle`, else `layer0`).
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

#[cfg(test)]
mod tests {
    use super::*;
    use miraculix::formats::MiniFloat;

    /// What `shaders/particle.vert` makes of a quad: corner positions and UVs,
    /// in the order the CPU built its four vertices before.
    fn corners(quad: &ParticleQuad) -> [(Vec3, [f32; 2]); 4] {
        let axis = |i: usize| -> [f32; 3] {
            std::array::from_fn(|k| MiniFloat::F16.decode(u32::from(quad.axes[i + k])))
        };
        let (right, up) = (Vec3::from_array(axis(0)), Vec3::from_array(axis(3)));
        let [u0, v0, u1, v1] = quad.uv.map(|q| f32::from(q) / f32::from(u16::MAX));
        let center = Vec3::from_array(quad.center);
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y): (f32, f32)| {
            let uv = [if x < 0.0 { u1 } else { u0 }, if y < 0.0 { v1 } else { v0 }];
            (center + right * x + up * y, uv)
        })
    }

    #[test]
    fn quads_land_where_the_vertices_did() {
        let mut batch = Batch::default();
        let mut cases = Vec::new();
        for i in 0..200 {
            let t = i as f32;
            let center = Vec3::new(
                (t * 7.3).sin() * 40.0,
                (t * 1.9).cos() * 30.0,
                t * 0.37 - 30.0,
            );
            let rotation = Quat::from_euler(glam::EulerRot::YXZ, t * 0.7, t * 1.3, t * 0.2);
            let (right, up) = (rotation * Vec3::X, rotation * Vec3::Y);
            let size = 0.02 + (t * 0.61).fract() * 4.0;
            // A sprite's corner of a 16384 texel atlas, flipped every other time.
            let (a, b) = (
                (i * 37 % 1000) as f32 / 16384.0,
                (i * 37 % 1000 + 3) as f32 / 16384.0,
            );
            let uv = if i % 2 == 0 {
                [a, a, b, b]
            } else {
                [b, a, a, b]
            };
            quad(
                &mut batch,
                center,
                (right, up),
                size,
                uv,
                [1.0; 4],
                [16, 240],
            );
            cases.push((center, right * size, up * size, uv));
        }
        let mut bits = Vec::new();
        ParticleQuad::encode_axes(&mut batch.quads, &batch.axes, &mut bits);
        for (quad, (center, right, up, [u0, v0, u1, v1])) in batch.quads.iter().zip(cases) {
            let before = [
                (center - right - up, [u1, v1]),
                (center + right - up, [u0, v1]),
                (center + right + up, [u0, v0]),
                (center - right + up, [u1, v0]),
            ];
            for ((at, uv), (was_at, was_uv)) in corners(quad).into_iter().zip(before) {
                // F16 is good to 2^-11 of the axes; the centre stays f32.
                let size = right.length().max(up.length());
                assert!(
                    at.distance(was_at) <= size / 1024.0,
                    "{at} against {was_at}"
                );
                for (now, was) in uv.into_iter().zip(was_uv) {
                    assert!((now - was).abs() <= 1.0 / 65535.0, "{now} against {was}");
                }
            }
            // UNORM16 rounds each edge toward the other: never past the sprite.
            let [q0, _, q1, _] = quad.uv.map(|q| f32::from(q) / f32::from(u16::MAX));
            let (lo, hi) = (u0.min(u1), u0.max(u1));
            assert!(
                q0.min(q1) >= lo && q0.max(q1) <= hi,
                "{q0}, {q1} outside {lo}..{hi}"
            );
        }
    }
}
