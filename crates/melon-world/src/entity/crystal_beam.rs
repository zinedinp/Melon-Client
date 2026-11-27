//! `EnderDragonRenderer.submitCrystalBeams`: the beam an end crystal casts
//! at its target, or a dragon draws from the crystal healing it. Eight
//! sides round a cone, black where it starts and white where it ends, its
//! texture scrolling along it with time.

use glam::{Mat4, Vec3};

use crate::geometry;
use crate::mesh::WorldVertex;

/// `EnderDragonRenderer.CRYSTAL_BEAM_LOCATION`.
pub const TEXTURE: &str = "minecraft:entity/end_crystal/end_crystal_beam";

/// `submitCrystalBeams`' pose: up two blocks, then turned so the beam's
/// +z runs along `delta`.
fn turn(delta: Vec3) -> Mat4 {
    let horizontal = (delta.x * delta.x + delta.z * delta.z).sqrt();
    let yaw = -delta.z.atan2(delta.x) - std::f32::consts::FRAC_PI_2;
    let pitch = -horizontal.atan2(delta.y) - std::f32::consts::FRAC_PI_2;
    Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0))
        * Mat4::from_rotation_y(yaw)
        * Mat4::from_rotation_x(pitch)
}

/// The beam from where it is cast toward `delta` (in blocks) at
/// `time_in_ticks`, relative to where it is cast from. Both sides of each
/// quad are kept: the pipeline draws it without culling.
pub fn vertices(delta: Vec3, time_in_ticks: f32) -> Vec<WorldVertex> {
    let length = delta.length();
    let pose = turn(delta);
    // `setNormal(pose, 0, -1, 0)` for every vertex.
    let normal = pose.transform_vector3(Vec3::NEG_Y).normalize_or_zero();
    let shade = geometry::shade(normal);
    let v0 = -time_in_ticks * 0.01;
    let v1 = length / 32.0 - time_in_ticks * 0.01;
    let at = |x: f32, y: f32, z: f32| pose.transform_point3(Vec3::new(x, y, z)).to_array();
    let dark = [0.0; 3];
    let lit = [shade; 3];
    let mut out = Vec::with_capacity(8 * 8);
    let (mut last_sin, mut last_cos, mut last_u) = (0.0f32, 0.75f32, 0.0f32);
    for i in 1..=8 {
        let angle = i as f32 * std::f32::consts::TAU / 8.0;
        let (sin, cos) = (angle.sin() * 0.75, angle.cos() * 0.75);
        let u = i as f32 / 8.0;
        let quad = [
            WorldVertex::new(at(last_sin * 0.2, last_cos * 0.2, 0.0), [last_u, v0], dark),
            WorldVertex::new(at(last_sin, last_cos, length), [last_u, v1], lit),
            WorldVertex::new(at(sin, cos, length), [u, v1], lit),
            WorldVertex::new(at(sin * 0.2, cos * 0.2, 0.0), [u, v0], dark),
        ];
        out.extend(geometry::QUAD_CORNERS.map(|i| quad[i]));
        out.extend(geometry::QUAD_CORNERS_FLIPPED.map(|i| quad[i]));
        (last_sin, last_cos, last_u) = (sin, cos, u);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_beam_runs_from_two_up_to_two_up_past_the_delta() {
        let delta = Vec3::new(3.0, -4.0, 5.0);
        let beam = vertices(delta, 0.0);
        assert_eq!(beam.len(), 8 * 8);
        // The far ring is centred on `delta` raised two blocks.
        let far: Vec<Vec3> = beam
            .iter()
            .filter(|v| v.color[0] != 0)
            .map(|v| Vec3::from_array(v.pos))
            .collect();
        let centre = far.iter().copied().sum::<Vec3>() / far.len() as f32;
        assert!(
            (centre - (delta + Vec3::Y * 2.0)).length() < 1e-3,
            "{centre}"
        );
        let near = beam.iter().find(|v| v.color[0] == 0).unwrap();
        assert!((Vec3::from_array(near.pos) - Vec3::Y * 2.0).length() <= 0.15 + 1e-4);
    }

    #[test]
    fn the_texture_scrolls_with_time() {
        let a = vertices(Vec3::X * 32.0, 0.0);
        let b = vertices(Vec3::X * 32.0, 10.0);
        assert!((a[0].uv[1] - b[0].uv[1] - 0.1).abs() < 1e-5);
    }
}
