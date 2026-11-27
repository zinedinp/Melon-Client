//! Vanilla `ModelPart` for block entity models.

use glam::{Mat3, Mat4, Vec2, Vec3};

use crate::block::direction::{Direction, Directions};
use crate::geometry;
use crate::mesh::WorldVertex as Vertex;
use melon_core::math::angle::Rotation;

/// All six faces.
pub const ALL_FACES: Directions = Directions(0b11_1111);

/// One `ModelPart.Cube`.
#[derive(Clone, Copy, Debug)]
pub struct Cube {
    pub min: Vec3,
    pub size: Vec3,
    /// `texOffs`.
    pub uv: Vec2,
    /// `CubeDeformation`.
    pub grow: Vec3,
    pub mirror: bool,
    pub faces: Directions,
    /// `addBox(..., uScale, vScale)`: the texture taken as this many times
    /// its size for this box alone (an arrow's shaft).
    pub tex_scale: Vec2,
}

impl Cube {
    /// `CubeListBuilder.texOffs(u, v).addBox(x, y, z, w, h, d)`.
    pub fn new(uv: (f32, f32), min: (f32, f32, f32), size: (f32, f32, f32)) -> Self {
        Self {
            min: Vec3::new(min.0, min.1, min.2),
            size: Vec3::new(size.0, size.1, size.2),
            uv: Vec2::new(uv.0, uv.1),
            grow: Vec3::ZERO,
            mirror: false,
            faces: ALL_FACES,
            tex_scale: Vec2::ONE,
        }
    }

    /// `CubeListBuilder.addBox(..., CubeDeformation.NONE, u, v)`.
    pub fn tex_scaled(mut self, u: f32, v: f32) -> Self {
        self.tex_scale = Vec2::new(u, v);
        self
    }

    pub fn grown(mut self, by: f32) -> Self {
        self.grow = Vec3::splat(by);
        self
    }

    /// `CubeDeformation(x, y, z)`: grown differently along each axis.
    pub fn grown_by(mut self, by: Vec3) -> Self {
        self.grow = by;
        self
    }

    pub fn mirrored(mut self) -> Self {
        self.mirror = true;
        self
    }

    /// Only these faces (`addBox(..., visibleFaces)`).
    pub fn only(mut self, faces: Directions) -> Self {
        self.faces = faces;
        self
    }

    /// `ModelPart.Cube`'s six polygons.
    fn polygons(&self) -> Vec<([Vec3; 4], [Vec2; 4], Direction)> {
        let mut lo = self.min - self.grow;
        let mut hi = self.min + self.size + self.grow;
        if self.mirror {
            std::mem::swap(&mut lo.x, &mut hi.x);
        }
        let t0 = Vec3::new(lo.x, lo.y, lo.z);
        let t1 = Vec3::new(hi.x, lo.y, lo.z);
        let t2 = Vec3::new(hi.x, hi.y, lo.z);
        let t3 = Vec3::new(lo.x, hi.y, lo.z);
        let l0 = Vec3::new(lo.x, lo.y, hi.z);
        let l1 = Vec3::new(hi.x, lo.y, hi.z);
        let l2 = Vec3::new(hi.x, hi.y, hi.z);
        let l3 = Vec3::new(lo.x, hi.y, hi.z);
        let (w, h, d) = (self.size.x, self.size.y, self.size.z);
        let (u, v) = (self.uv.x, self.uv.y);
        let (u0, u1, u2, u22) = (u, u + d, u + d + w, u + d + w + w);
        let (u3, u4) = (u + d + w + d, u + d + w + d + w);
        let (v0, v1, v2) = (v, v + d, v + d + h);
        let faces = [
            (Direction::Down, [l1, l0, t0, t1], (u1, v0, u2, v1)),
            (Direction::Up, [t2, t3, l3, l2], (u2, v1, u22, v0)),
            (Direction::West, [t0, l0, l3, t3], (u0, v1, u1, v2)),
            (Direction::North, [t1, t0, t3, t2], (u1, v1, u2, v2)),
            (Direction::East, [l1, t1, t2, l2], (u2, v1, u3, v2)),
            (Direction::South, [l0, l1, l2, l3], (u3, v1, u4, v2)),
        ];
        faces
            .into_iter()
            .filter(|(direction, ..)| self.faces.contains(*direction))
            .map(|(direction, mut corners, (a, b, c, e))| {
                let mut uvs = [
                    Vec2::new(c, b),
                    Vec2::new(a, b),
                    Vec2::new(a, e),
                    Vec2::new(c, e),
                ]
                .map(|uv| uv / self.tex_scale);
                if self.mirror {
                    corners.reverse();
                    uvs.reverse();
                }
                (corners, uvs, direction)
            })
            .collect()
    }
}

/// `PartPose`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub offset: Vec3,
    /// `xRot`, `yRot`, `zRot`, radians.
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl Pose {
    pub const ZERO: Pose = Pose {
        offset: Vec3::ZERO,
        rotation: Vec3::ZERO,
        scale: Vec3::ONE,
    };

    pub fn offset(x: f32, y: f32, z: f32) -> Pose {
        Pose {
            offset: Vec3::new(x, y, z),
            ..Pose::ZERO
        }
    }

    pub fn offset_rotation(offset: (f32, f32, f32), rotation: (f32, f32, f32)) -> Pose {
        Pose {
            offset: Vec3::new(offset.0, offset.1, offset.2),
            rotation: Vec3::new(rotation.0, rotation.1, rotation.2),
            scale: Vec3::ONE,
        }
    }

    /// `PartPose.scaled`: the offset scales along.
    pub fn scaled(self, factor: f32) -> Pose {
        Pose {
            offset: self.offset * factor,
            scale: self.scale * factor,
            ..self
        }
    }

    /// `ModelPart.translateAndRotate`.
    #[inline]
    pub fn matrix(&self) -> Mat4 {
        let r = Mat3::euler_zyx(self.rotation);
        Mat4::from_cols(
            (r.x_axis * self.scale.x).extend(0.0),
            (r.y_axis * self.scale.y).extend(0.0),
            (r.z_axis * self.scale.z).extend(0.0),
            (self.offset / 16.0).extend(1.0),
        )
    }

    /// `parent * self.matrix()`, the step down a model's part tree.
    #[inline]
    pub fn placed_in(&self, parent: Mat4) -> Mat4 {
        let turn = (self.rotation != Vec3::ZERO).then(|| Mat3::euler_zyx(self.rotation));
        self.placed_in_turned(parent, turn)
    }

    /// [`Self::placed_in`] with the rotation worked out already: `None`
    /// when the part doesn't turn.
    #[inline]
    pub fn placed_in_turned(&self, parent: Mat4, turn: Option<Mat3>) -> Mat4 {
        let column = |c: Vec3| parent.x_axis * c.x + parent.y_axis * c.y + parent.z_axis * c.z;
        let origin = column(self.offset / 16.0) + parent.w_axis;
        if turn.is_none() && self.scale == Vec3::ONE {
            return Mat4::from_cols(parent.x_axis, parent.y_axis, parent.z_axis, origin);
        }
        let r = turn.unwrap_or(Mat3::IDENTITY);
        Mat4::from_cols(
            column(r.x_axis * self.scale.x),
            column(r.y_axis * self.scale.y),
            column(r.z_axis * self.scale.z),
            origin,
        )
    }
}

/// A `ModelPart`: its pose, boxes and children.
#[derive(Clone, Debug)]
pub struct Part {
    pub pose: Pose,
    pub cubes: Vec<Cube>,
    pub children: Vec<Part>,
}

impl Part {
    pub fn new(pose: Pose, cubes: Vec<Cube>) -> Self {
        Self {
            pose,
            cubes,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<Part>) -> Self {
        self.children = children;
        self
    }
}

/// Where a part's texture sits.
#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    pub rect: [f32; 4],
    pub size: Vec2,
}

impl Sprite {
    fn atlas_uv(&self, texel: Vec2) -> [f32; 2] {
        let local = (texel / self.size).clamp(Vec2::ZERO, Vec2::ONE);
        geometry::atlas_uv(self.rect, local.x, local.y)
    }
}

/// One face of a part, through its transform.
pub struct Face {
    pub corners: [Vec3; 4],
    pub texels: [Vec2; 4],
    pub normal: Vec3,
    /// Whether the transform mirrors, which turns the winding inside out.
    pub flipped: bool,
}

/// TODO(perf): each face transforms its own four corners, 24 transforms a
/// cube for 8 corners. Transform the 8 once (then as eight lanes, AVX
/// f32x8, mul and add in `transform_point3`'s order, no FMA: same bits),
/// and take the normal from the matrix's column once a part, not a face.
pub fn faces(part: &Part, transform: Mat4, visit: &mut impl FnMut(Face)) {
    let transform = transform * part.pose.matrix();
    let normal_matrix = Mat3::from_mat4(transform);
    let flipped = normal_matrix.determinant() < 0.0;
    for cube in &part.cubes {
        for (corners, texels, direction) in cube.polygons() {
            let [sx, sy, sz] = direction.step();
            visit(Face {
                corners: corners.map(|c| transform.transform_point3(c / 16.0)),
                texels,
                normal: (normal_matrix * Vec3::new(sx as f32, sy as f32, sz as f32))
                    .normalize_or_zero(),
                flipped,
            });
        }
    }
    for child in &part.children {
        faces(child, transform, visit);
    }
}

/// Writes `part` and its children through `transform` (model pixels to the target space).
pub fn emit(part: &Part, transform: Mat4, sprite: &Sprite, tint: [f32; 3], out: &mut Vec<Vertex>) {
    faces(part, transform, &mut |face| {
        let color = tint.map(|c| c * geometry::shade(face.normal));
        let corners =
            std::array::from_fn(|i| (face.corners[i].to_array(), sprite.atlas_uv(face.texels[i])));
        geometry::push_quad(out, corners, color, face.flipped);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use melon_core::math::angle::HALF_PI;

    #[test]
    fn a_pose_matrix_is_translate_rotate_scale() {
        let pose = Pose {
            offset: Vec3::new(3.0, -5.0, 7.0),
            rotation: Vec3::new(0.4, -1.1, 2.3),
            scale: Vec3::new(1.5, 0.5, 2.0),
        };
        let expected = Mat4::from_translation(pose.offset / 16.0)
            * Mat4::euler_zyx(pose.rotation)
            * Mat4::from_scale(pose.scale);
        assert!(pose.matrix().abs_diff_eq(expected, 1e-6));
    }

    #[test]
    fn placing_a_part_is_the_parents_product_to_the_bit() {
        let parent = Mat4::from_translation(Vec3::new(1.0, 2.0, -3.0))
            * Mat4::euler_zyx(Vec3::new(0.3, 2.0, -0.7))
            * Mat4::from_scale(Vec3::new(-0.9, -0.9, 0.9));
        let turned = Pose {
            offset: Vec3::new(3.0, -5.0, 7.0),
            rotation: Vec3::new(0.4, -1.1, 2.3),
            scale: Vec3::new(1.5, 0.5, 2.0),
        };
        let still = Pose::offset(-2.0, 12.0, 0.5);
        for pose in [turned, still, Pose::ZERO] {
            assert_eq!(pose.placed_in(parent), parent * pose.matrix(), "{pose:?}");
        }
    }

    #[test]
    fn a_cube_spans_its_box_and_faces_outward() {
        let part = Part::new(
            Pose::ZERO,
            vec![Cube::new((0.0, 0.0), (0.0, 0.0, 0.0), (16.0, 16.0, 16.0))],
        );
        let sprite = Sprite {
            rect: [0.0, 0.0, 1.0, 1.0],
            size: Vec2::splat(64.0),
        };
        let mut out = Vec::new();
        emit(&part, Mat4::IDENTITY, &sprite, [1.0; 3], &mut out);
        assert_eq!(out.len(), 24);
        for quad in out.chunks(4) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(quad[i].pos));
            let normal = (b - a).cross(c - a);
            let center = (a + b + c) / 3.0 - Vec3::splat(0.5);
            assert!(normal.dot(center) > 0.0);
        }
    }

    #[test]
    fn poses_rotate_and_offset_in_pixels() {
        let pose = Pose::offset_rotation((16.0, 0.0, 0.0), (0.0, HALF_PI, 0.0));
        let p = pose.matrix().transform_point3(Vec3::new(0.0, 0.0, 1.0));
        assert!((p - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5, "{p}");
    }
}
