//! Sky geometry.

use glam::{Mat3, Mat4, Vec3};

use crate::clock::Environment;
use crate::geometry;
use crate::java_random::JavaRandom;
use crate::math::angle::{self, HALF_PI, PI, Rotation, TAU};
use crate::mesh::WorldVertex;

/// `SkyRenderer.SUN_HEIGHT` / `MOON_HEIGHT`.
const BODY_HEIGHT: f32 = 100.0;
/// `SkyRenderer.SUN_SIZE`.
const SUN_SIZE: f32 = 30.0;
/// `SkyRenderer.MOON_SIZE`.
const MOON_SIZE: f32 = 20.0;
/// `SkyRenderer.STAR_COUNT`.
const STAR_COUNT: usize = 1500;
/// `SkyRenderer.SKY_DISC_RADIUS`.
const SKY_DISC_RADIUS: f32 = 512.0;
/// How far the sunrise's glow reaches: `SkyRenderer.buildSunriseFan`.
const SUNRISE_RADIUS: f32 = 120.0;

/// `SkyRenderer.applyCelestialBodyTransform` after the Y-90 / X-angle pose.
pub fn body_model(angle_radians: f32, height: f32, scale: f32) -> Mat4 {
    Mat4::rot_y_deg(-90.0)
        * Mat4::rot_x(angle_radians)
        * Mat4::from_translation(Vec3::new(0.0, height, 0.0))
        * Mat4::from_scale(Vec3::new(scale, 1.0, scale))
}

pub fn sun_model(env: &Environment) -> Mat4 {
    body_model(env.sun_angle, BODY_HEIGHT, SUN_SIZE)
}

pub fn moon_model(env: &Environment) -> Mat4 {
    body_model(env.moon_angle, BODY_HEIGHT, MOON_SIZE)
}

pub fn star_model(env: &Environment) -> Mat4 {
    body_model(env.star_angle, 0.0, 1.0)
}

/// Camera view with the translation stripped, so the sky sits at infinity.
pub fn rotation_view(view: Mat4) -> Mat4 {
    let mut rotation = view;
    rotation.w_axis.x = 0.0;
    rotation.w_axis.y = 0.0;
    rotation.w_axis.z = 0.0;
    rotation
}

/// `SkyRenderer.buildSkyDisc`: a fan of eight triangles around `(0, y, 0)`
pub fn sky_disc(y: f32) -> Vec<WorldVertex> {
    let x = y.signum() * SKY_DISC_RADIUS;
    let ring = |degrees: i32| {
        let angle = (degrees as f32).to_radians();
        [
            x * angle::cos(angle),
            y,
            SKY_DISC_RADIUS * angle::sin(angle),
        ]
    };
    let center = [0.0, y, 0.0];
    let corner = |at: [f32; 3]| WorldVertex::new(at, [0.0, 0.0], [1.0; 3]);
    let mut out = Vec::with_capacity(32);
    for degrees in (-180..180).step_by(45) {
        out.extend([center, ring(degrees), ring(degrees + 45), center].map(corner));
    }
    out
}

/// `SkyRenderer.buildSunriseFan`.
pub fn sunrise_fan() -> Vec<WorldVertex> {
    const STEPS: usize = 16;
    let at = |at: [f32; 3], alpha: u8| {
        let mut v = WorldVertex::new(at, [0.0, 0.0], [1.0; 3]);
        v.color[3] = alpha;
        v
    };
    let center = at([0.0, 100.0, 0.0], 255);
    let ring = |i: usize| {
        let angle = i as f32 * TAU / STEPS as f32;
        at(
            [
                angle::sin(angle) * SUNRISE_RADIUS,
                angle::cos(angle) * SUNRISE_RADIUS,
                -angle::cos(angle) * 40.0,
            ],
            0,
        )
    };
    let mut out = Vec::with_capacity(STEPS * 4);
    for i in 0..STEPS {
        out.extend([center, ring(i), ring(i + 1), center]);
    }
    out
}

/// The sunrise fan's place for a sun at `sun_angle` radians.
pub fn sunrise_model(sun_angle: f32, strength: f32) -> Mat4 {
    let flip = if angle::sin(sun_angle) < 0.0 {
        180.0f32
    } else {
        0.0
    };
    Mat4::rot_x_deg(90.0f32)
        * Mat4::rot_z_deg(flip + 90.0)
        * Mat4::from_scale(Vec3::new(1.0, 1.0, strength))
}

/// `SkyRenderer.buildEndSky`.
pub fn end_sky_box() -> Vec<WorldVertex> {
    const TILES: usize = 16;
    const SHADE: f32 = 40.0 / 255.0;
    let face_poses = [
        Mat4::IDENTITY,
        Mat4::rot_x(HALF_PI),
        Mat4::rot_x(-HALF_PI),
        Mat4::rot_x(PI),
        Mat4::rot_z(HALF_PI),
        Mat4::rot_z(-HALF_PI),
    ];
    let mut out = Vec::with_capacity(6 * TILES * TILES * 4);
    for pose in face_poses {
        for tx in 0..TILES {
            for tz in 0..TILES {
                let x = |i: usize| -100.0 + 200.0 * i as f32 / TILES as f32;
                let (x0, x1, z0, z1) = (x(tx), x(tx + 1), x(tz), x(tz + 1));
                for (px, pz, u, v) in [
                    (x0, z0, 0.0, 0.0),
                    (x0, z1, 0.0, 1.0),
                    (x1, z1, 1.0, 1.0),
                    (x1, z0, 1.0, 0.0),
                ] {
                    let at = pose.transform_point3(Vec3::new(px, -100.0, pz));
                    out.push(WorldVertex::new(at.to_array(), [u, v], [SHADE; 3]));
                }
            }
        }
    }
    out
}

/// Sun quad in the XZ plane (`SkyRenderer.buildCelestialQuad`).
pub fn sun_quad() -> Vec<WorldVertex> {
    celestial_quad(false, [1.0, 1.0, 1.0])
}

/// Moon quad, UVs flipped (`SkyRenderer.buildMoonPhases`).
pub fn moon_quad() -> Vec<WorldVertex> {
    celestial_quad(true, [1.0, 1.0, 1.0])
}

fn celestial_quad(flip_uv: bool, color: [f32; 3]) -> Vec<WorldVertex> {
    let (u0, u1, v0, v1) = if flip_uv {
        (1.0, 0.0, 1.0, 0.0)
    } else {
        (0.0, 1.0, 0.0, 1.0)
    };
    let mut out = Vec::with_capacity(4);
    geometry::push_quad(
        &mut out,
        [
            ([-1.0, 0.0, -1.0], [u0, v0]),
            ([1.0, 0.0, -1.0], [u1, v0]),
            ([1.0, 0.0, 1.0], [u1, v1]),
            ([-1.0, 0.0, 1.0], [u0, v1]),
        ],
        color,
        false,
    );
    out
}

/// `SkyRenderer.buildStars`, same count and size range.
pub fn star_quads() -> Vec<WorldVertex> {
    let mut rng = JavaRandom::new(10842);
    let mut out = Vec::with_capacity(STAR_COUNT * 4);
    let mut placed = 0;
    while placed < STAR_COUNT {
        let x = rng.next_float() * 2.0 - 1.0;
        let y = rng.next_float() * 2.0 - 1.0;
        let z = rng.next_float() * 2.0 - 1.0;
        let size = 0.15 + rng.next_float() * 0.1;
        let length_sq = x * x + y * y + z * z;
        if !(0.010_000_001..=1.0).contains(&length_sq) {
            continue;
        }
        let center = Vec3::new(x, y, z).normalize() * 100.0;
        let z_rot = rng.next_float() * TAU;
        let toward = rotate_towards(-center, Vec3::Y);
        let rotation = Mat3::rot_z(-z_rot) * toward;
        let corners = [
            Vec3::new(size, -size, 0.0),
            Vec3::new(size, size, 0.0),
            Vec3::new(-size, size, 0.0),
            Vec3::new(-size, -size, 0.0),
        ];
        let mut quad = Vec::with_capacity(4);
        geometry::push_quad(
            &mut quad,
            [
                ((rotation * corners[0] + center).into(), [0.0, 0.0]),
                ((rotation * corners[1] + center).into(), [1.0, 0.0]),
                ((rotation * corners[2] + center).into(), [1.0, 1.0]),
                ((rotation * corners[3] + center).into(), [0.0, 1.0]),
            ],
            [1.0, 1.0, 1.0],
            false,
        );
        out.extend(quad);
        placed += 1;
    }
    out
}

/// JOML `Matrix3f.rotateTowards(dir, up)`.
fn rotate_towards(dir: Vec3, up: Vec3) -> Mat3 {
    let z = dir.normalize_or_zero();
    let x = up.cross(z).normalize_or_zero();
    let y = z.cross(x);
    Mat3::from_cols(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Environment;

    #[test]
    fn sun_at_noon_is_above() {
        let model = sun_model(&Environment::at(6000.0));
        let pos = model.transform_point3(Vec3::ZERO);
        assert!(
            pos.y > 90.0 && pos.x.abs() < 5.0 && pos.z.abs() < 5.0,
            "noon sun at {pos}"
        );
    }

    #[test]
    fn the_disc_reaches_five_hundred_blocks_round_and_the_dark_one_lies_below() {
        let top = sky_disc(16.0);
        assert_eq!(top.len(), 32);
        assert!(top.iter().all(|v| v.pos[1] == 16.0));
        let far = top
            .iter()
            .map(|v| (v.pos[0] * v.pos[0] + v.pos[2] * v.pos[2]).sqrt())
            .fold(0.0, f32::max);
        assert!((far - 512.0).abs() < 1e-3);
        let bottom = sky_disc(-16.0);
        assert!(bottom.iter().all(|v| v.pos[1] == -16.0));
        assert!((bottom[1].pos[0] - 512.0).abs() < 1e-3);
        assert!((top[1].pos[0] + 512.0).abs() < 1e-3);
    }

    #[test]
    fn the_sunrise_fan_fades_to_nothing_at_its_rim() {
        let fan = sunrise_fan();
        assert_eq!(fan.len(), 64);
        assert_eq!(fan[0].color[3], 255);
        assert_eq!(fan[1].color[3], 0);
        assert!((fan[1].pos[1] - 120.0).abs() < 1e-4);
        let at_dusk = sunrise_model(1.5, 1.0);
        let flat = sunrise_model(1.5, 0.0);
        let tip = Vec3::new(0.0, 120.0, -40.0);
        assert!(at_dusk.transform_vector3(tip) != flat.transform_vector3(tip));
    }

    #[test]
    fn the_end_sky_is_six_faces_of_tiles() {
        let sky = end_sky_box();
        assert_eq!(sky.len(), 6 * 16 * 16 * 4);
        assert!(sky.iter().all(|v| v.color[..3] == [40, 40, 40]));
        let reach = sky
            .iter()
            .flat_map(|v| v.pos)
            .map(f32::abs)
            .fold(0.0, f32::max);
        assert!((reach - 100.0).abs() < 1e-3);
    }

    #[test]
    fn stars_fill_the_sky() {
        let stars = star_quads();
        assert_eq!(stars.len(), STAR_COUNT * 4);
    }
}
