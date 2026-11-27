//! Vanilla `Frustum`: the six planes of the camera's view volume.

use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy)]
struct Plane {
    normal: Vec3,
    /// `|normal|` per component.
    reach: Vec3,
    distance: f32,
}

impl Plane {
    fn new(row: Vec4) -> Self {
        let length = row.truncate().length();
        let row = if length > 0.0 { row / length } else { row };
        Self {
            normal: row.truncate(),
            reach: row.truncate().abs(),
            distance: row.w,
        }
    }

    /// Signed distance of `point`: positive on the inside.
    #[inline]
    fn signed_distance(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

pub struct Frustum {
    planes: [Plane; 6],
}

impl Frustum {
    /// From `projection * view` with Vulkan's `0..1` depth range.
    pub fn from_view_projection(m: Mat4) -> Self {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        Self {
            planes: [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(Plane::new),
        }
    }

    /// `Frustum.isVisible(AABB)` for a box given by centre and half-size.
    #[inline]
    pub fn sees_centered_box(&self, center: Vec3, half: Vec3) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.signed_distance(center) + plane.reach.dot(half) >= 0.0)
    }

    /// Whether all of the box is inside every plane.
    #[inline]
    pub fn contains_box(&self, min: Vec3, max: Vec3) -> bool {
        let (center, half) = ((min + max) * 0.5, (max - min) * 0.5);
        self.planes
            .iter()
            .all(|plane| plane.signed_distance(center) - plane.reach.dot(half) >= 0.0)
    }

    /// [`Self::sees_centered_box`] for a box given by its corners.
    #[inline]
    pub fn sees_box(&self, min: Vec3, max: Vec3) -> bool {
        self.sees_centered_box((min + max) * 0.5, (max - min) * 0.5)
    }

    /// Each plane as `(normal, |normal|, distance)`, for kernels that test
    /// many boxes at once ([`super::section_cull`]).
    pub fn planes(&self) -> [(Vec3, Vec3, f32); 6] {
        self.planes.map(|p| (p.normal, p.reach, p.distance))
    }

    /// Whether any of a sphere can be inside.
    #[inline]
    pub fn sees_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.signed_distance(center) >= -radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looking_down_minus_z() -> Frustum {
        let projection = crate::renderer::world_projection(70.0, 16.0 / 9.0, 512.0);
        Frustum::from_view_projection(projection * Mat4::IDENTITY)
    }

    #[test]
    fn a_box_wholly_inside_holds_every_box_within_it() {
        let frustum = looking_down_minus_z();
        let (min, max) = (Vec3::new(-2.0, -2.0, -40.0), Vec3::new(2.0, 2.0, -20.0));
        assert!(frustum.contains_box(min, max));
        for z in -40..-20 {
            let at = Vec3::new(-2.0, -2.0, z as f32);
            assert!(frustum.sees_box(at, at + Vec3::ONE));
        }
        let wide = Vec3::new(200.0, 2.0, -20.0);
        assert!(frustum.sees_box(min, wide));
        assert!(!frustum.contains_box(min, wide));
    }

    #[test]
    fn keeps_what_is_ahead_and_drops_what_is_behind() {
        let frustum = looking_down_minus_z();
        let block = |z: f32| (Vec3::new(-0.5, -0.5, z - 0.5), Vec3::new(0.5, 0.5, z + 0.5));
        let (min, max) = block(-10.0);
        assert!(frustum.sees_box(min, max));
        let (min, max) = block(10.0);
        assert!(!frustum.sees_box(min, max));
        assert!(!frustum.sees_box(Vec3::new(100.0, 0.0, -11.0), Vec3::new(101.0, 1.0, -10.0)));
        assert!(!frustum.sees_box(Vec3::new(0.0, 0.0, -2000.0), Vec3::new(1.0, 1.0, -1999.0)));
    }

    #[test]
    fn a_box_around_the_camera_is_visible() {
        let frustum = looking_down_minus_z();
        assert!(frustum.sees_box(Vec3::splat(-8.0), Vec3::splat(8.0)));
    }

    #[test]
    fn spheres_count_by_their_edge() {
        let frustum = looking_down_minus_z();
        assert!(frustum.sees_sphere(Vec3::new(0.0, 0.0, -10.0), 0.5));
        assert!(!frustum.sees_sphere(Vec3::new(0.0, 0.0, 10.0), 0.5));
        assert!(frustum.sees_sphere(Vec3::new(0.0, 0.0, 1.0), 2.0));
    }

    #[test]
    fn the_box_test_agrees_with_the_corner_test() {
        let frustum = looking_down_minus_z();
        for i in 0..400 {
            let center = Vec3::new(
                (i % 20) as f32 * 7.0 - 70.0,
                (i / 20) as f32 * 3.0 - 30.0,
                -((i * 13) % 50) as f32 * 4.0 + 20.0,
            );
            let half = Vec3::new(8.0, 2.0, 5.0);
            let nearest_plane = frustum
                .planes
                .iter()
                .map(|plane| {
                    let furthest =
                        Vec3::select(plane.normal.cmpge(Vec3::ZERO), center + half, center - half);
                    plane.signed_distance(furthest)
                })
                .fold(f32::INFINITY, f32::min);
            if nearest_plane.abs() > 1e-3 {
                assert_eq!(
                    frustum.sees_centered_box(center, half),
                    nearest_plane >= 0.0,
                    "{center}"
                );
            }
        }
    }
}
