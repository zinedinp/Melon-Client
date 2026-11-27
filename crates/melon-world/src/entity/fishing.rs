//! A fishing rod's cast (`FishingHookRenderer`).

use glam::Vec3;

use crate::renderer::WorldVertex;

/// Steps the line is drawn in (`FishingHookRenderer.submit`).
pub const STEPS: usize = 16;

/// How far the line is lifted off the bobber's centre.
pub const LINE_LIFT: f32 = 0.25;

/// `FishingHookRenderer.getHoldingArm`.
pub fn holds_right(main_arm_right: bool, main_hand_has_rod: bool) -> bool {
    main_arm_right == main_hand_has_rod
}

/// `FishingRodCast`.
pub fn rod_looks_cast(fishing: bool, is_main_hand: bool, main_hand_has_rod: bool) -> bool {
    fishing
        && if is_main_hand {
            main_hand_has_rod
        } else {
            !main_hand_has_rod
        }
}

/// `getPlayerHandPos` for any view but the owner's own first person.
pub fn hand_third_person(
    eye: Vec3,
    body_yaw: f32,
    right: bool,
    crouching: bool,
    scale: f32,
) -> Vec3 {
    let invert = if right { 1.0 } else { -1.0 };
    let (sin, cos) = body_yaw.to_radians().sin_cos();
    let side = invert * 0.35 * scale;
    let forward = 0.8 * scale;
    let down = if crouching { -0.1875 } else { 0.0 };
    eye + Vec3::new(
        -cos * side - sin * forward,
        down - 0.45 * scale,
        -sin * side + cos * forward,
    )
}

/// The camera's near plane, enough to find a point on it.
#[derive(Clone, Copy)]
pub struct NearPlane {
    pub forward: Vec3,
    pub left: Vec3,
    pub up: Vec3,
    pub near: f32,
    pub fov: f32,
    pub aspect: f32,
}

/// `getPlayerHandPos` for the owner's own first person.
pub fn hand_first_person(eye: Vec3, plane: &NearPlane, right: bool, swing: f32) -> Vec3 {
    let invert = if right { 1.0 } else { -1.0 };
    let height = (plane.fov.to_radians() / 2.0).tan() * plane.near;
    let width = height * plane.aspect;
    let point = plane.forward * plane.near + plane.up * (height * -0.1)
        - plane.left * (width * invert * 0.525);
    let point = point * (960.0 / plane.fov);
    let (sin, cos) = (swing * 0.5).sin_cos();
    let point = Vec3::new(
        point.x * cos + point.z * sin,
        point.y,
        point.z * cos - point.x * sin,
    );
    let (sin, cos) = (-swing * 0.7).sin_cos();
    let point = Vec3::new(
        point.x,
        point.y * cos + point.z * sin,
        point.z * cos - point.y * sin,
    );
    eye + point
}

/// `swing` of the line's hand: `sin(sqrt(attack) * pi)`.
pub fn swing_of(attack: f32) -> f32 {
    (attack.max(0.0).sqrt() * std::f32::consts::PI).sin()
}

/// Where along the line, `i` of [`STEPS`] steps from the bobber to the rod.
pub fn line_point(offset: Vec3, i: usize) -> Vec3 {
    let a = i as f32 / STEPS as f32;
    Vec3::new(
        offset.x * a,
        offset.y * (a * a + a) * 0.5 + LINE_LIFT,
        offset.z * a,
    )
}

/// The line as flat strips turned to the camera, relative to `origin`
pub fn line_quads(
    bobber: Vec3,
    hand: Vec3,
    camera: Vec3,
    origin: Vec3,
    light: [u8; 2],
) -> Vec<WorldVertex> {
    let offset = hand - (bobber + Vec3::Y * LINE_LIFT);
    let mut out = Vec::with_capacity(STEPS * 4);
    for i in 0..STEPS {
        let a = bobber + line_point(offset, i);
        let b = bobber + line_point(offset, i + 1);
        let along = b - a;
        let to_camera = camera - (a + b) * 0.5;
        let side = along.cross(to_camera);
        let side = if side.length_squared() > 1.0e-12 {
            side.normalize()
        } else {
            Vec3::X
        };
        let half = (0.006 + 0.0012 * to_camera.length()).min(0.05) * side;
        let vertex = |p: Vec3| {
            let mut v = WorldVertex::new((p - origin).to_array(), [0.5, 0.5], [0.0; 3]);
            v.light = light;
            v
        };
        out.extend([
            vertex(a - half),
            vertex(a + half),
            vertex(b + half),
            vertex(b - half),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rod_is_in_the_main_arm_unless_the_main_hand_is_empty_of_one() {
        assert!(holds_right(true, true));
        assert!(!holds_right(true, false));
        assert!(holds_right(false, false));
    }

    #[test]
    fn only_the_holding_hands_rod_looks_cast() {
        assert!(rod_looks_cast(true, true, true));
        assert!(!rod_looks_cast(true, false, true));
        assert!(rod_looks_cast(true, false, false));
        assert!(!rod_looks_cast(false, true, true));
    }

    #[test]
    fn the_hand_is_in_front_and_to_the_side_of_the_body() {
        let hand = hand_third_person(Vec3::new(0.0, 1.62, 0.0), 0.0, true, false, 1.0);
        assert!(
            (hand - Vec3::new(-0.35, 1.17, 0.8)).length() < 1e-5,
            "{hand}"
        );
    }

    #[test]
    fn the_line_runs_from_the_bobber_to_the_hand() {
        let offset = Vec3::new(2.0, 1.0, -3.0);
        assert!((line_point(offset, 0) - Vec3::Y * LINE_LIFT).length() < 1e-6);
        let end = line_point(offset, STEPS);
        assert!(
            (end - (offset + Vec3::Y * LINE_LIFT)).length() < 1e-5,
            "{end}"
        );
    }

    #[test]
    fn a_line_has_one_strip_a_step_and_finite_corners() {
        let quads = line_quads(
            Vec3::ZERO,
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(3.0, 1.0, 0.0),
            Vec3::ZERO,
            [0, 240],
        );
        assert_eq!(quads.len(), STEPS * 4);
        assert!(quads.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
    }
}
