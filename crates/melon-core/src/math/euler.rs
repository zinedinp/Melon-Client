//! Yaw and pitch, and everything vanilla turns them into, in one place:
//! the look vector gameplay uses (`Entity.calculateViewVector`, from
//! `Mth`'s table) and the orientation drawing uses (JOML
//! `Quaternionf.rotationYXZ`, exact sines: the camera, displays,
//! billboards). The JOML parts here give vanilla's bits, not just its
//! values: the same operations in the same order, `fma` where JOML has
//! one.

use glam::{Mat3, Mat4, Quat, Vec3};

use super::angle::{self, DEG_TO_RAD, PI, cos_from_sin, sin_exact};

/// A yaw and pitch in vanilla's convention, degrees: `yRot` 0 looks
/// south (`+Z`), 90 west (`-X`); `xRot` above 0 looks down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    pub y_rot: f32,
    pub x_rot: f32,
}

impl Look {
    pub const fn new(y_rot: f32, x_rot: f32) -> Self {
        Self { y_rot, x_rot }
    }

    /// From Melon's player yaw, where 0 looks north.
    pub fn from_melon(yaw: f32, pitch: f32) -> Self {
        Self::new(yaw + 180.0, pitch)
    }

    /// `Camera.setRotation(yRot + 180, -xRot)`: the front view's.
    pub fn mirrored(self) -> Self {
        Self::new(self.y_rot + 180.0, -self.x_rot)
    }

    /// `Entity.calculateViewVector`: where an entity looks, as gameplay
    /// (picking, gliding, swimming) reads it, from `Mth`'s table.
    #[inline]
    pub fn view_vector(self) -> Vec3 {
        let x_rad = self.x_rot * DEG_TO_RAD;
        let y_rad = -self.y_rot * DEG_TO_RAD;
        let (y_sin, y_cos) = angle::sin_cos(y_rad);
        let (x_sin, x_cos) = angle::sin_cos(x_rad);
        Vec3::new(y_sin * x_cos, -x_sin, y_cos * x_cos)
    }

    /// `Camera.setRotation`'s `rotation`: camera space (`-Z` ahead) to
    /// world space.
    #[inline]
    pub fn camera_rotation(self) -> Quat {
        rotation_yxz(PI - self.y_rot * DEG_TO_RAD, -self.x_rot * DEG_TO_RAD, 0.0)
    }
}

/// A camera's orientation as vanilla's `Camera` keeps it: the
/// rotation, its axes in the world and the view matrix, worked out once.
#[derive(Clone, Copy, Debug)]
pub struct Orientation {
    pub look: Look,
    /// Camera space to world space.
    pub rotation: Quat,
    /// `Camera.forwards`, `up` and `left`.
    pub forwards: Vec3,
    pub up: Vec3,
    pub left: Vec3,
    /// `Camera.getViewRotationMatrix`: world to camera space, no
    /// translation (positions are made relative to the camera first).
    pub view: Mat4,
}

impl Orientation {
    pub fn new(look: Look) -> Self {
        let rotation = look.camera_rotation();
        Self {
            look,
            rotation,
            forwards: transform(rotation, Vec3::NEG_Z),
            up: transform(rotation, Vec3::Y),
            left: transform(rotation, Vec3::NEG_X),
            view: Mat4::from_mat3(rotation_matrix(rotation.conjugate())),
        }
    }
}

/// JOML `Quaternionf.rotationYXZ(y, x, z)`: about Y, then X, then Z.
#[inline]
pub fn rotation_yxz(angle_y: f32, angle_x: f32, angle_z: f32) -> Quat {
    let half = |a: f32| {
        let s = sin_exact(a * 0.5);
        (s, cos_from_sin(s, a * 0.5))
    };
    let (sx, cx) = half(angle_x);
    let (sy, cy) = half(angle_y);
    let (sz, cz) = half(angle_z);
    let x = cy * sx;
    let y = sy * cx;
    let z = sy * sx;
    let w = cy * cx;
    Quat::from_xyzw(
        x * cz + y * sz,
        y * cz - x * sz,
        w * sz - z * cz,
        w * cz + z * sz,
    )
}

/// JOML `Quaternionf.transform(Vector3f)`: `v` turned by `q`, which
/// needn't be quite of unit length.
#[inline]
pub fn transform(q: Quat, v: Vec3) -> Vec3 {
    let (xx, yy, zz, ww) = (q.x * q.x, q.y * q.y, q.z * q.z, q.w * q.w);
    let (xy, xz, yz) = (q.x * q.y, q.x * q.z, q.y * q.z);
    let (xw, zw, yw) = (q.x * q.w, q.z * q.w, q.y * q.w);
    let k = 1.0 / (xx + yy + zz + ww);
    let row = |a: f32, b: f32, c: f32| a.mul_add(v.x, b.mul_add(v.y, c * v.z));
    Vec3::new(
        row(
            (xx - yy - zz + ww) * k,
            2.0 * (xy - zw) * k,
            2.0 * (xz + yw) * k,
        ),
        row(
            2.0 * (xy + zw) * k,
            (yy - xx - zz + ww) * k,
            2.0 * (yz - xw) * k,
        ),
        row(
            2.0 * (xz - yw) * k,
            2.0 * (yz + xw) * k,
            (zz - xx - yy + ww) * k,
        ),
    )
}

/// JOML `Matrix4f.rotation(Quaternionfc)`.
#[inline]
pub fn rotation_matrix(q: Quat) -> Mat3 {
    let (w2, x2, y2, z2) = (q.w * q.w, q.x * q.x, q.y * q.y, q.z * q.z);
    let twice = |a: f32, b: f32| {
        let p = a * b;
        p + p
    };
    let dzw = twice(q.z, q.w);
    let dxy = twice(q.x, q.y);
    let dxz = twice(q.x, q.z);
    let dyw = twice(q.y, q.w);
    let dyz = twice(q.y, q.z);
    let dxw = twice(q.x, q.w);
    Mat3::from_cols(
        Vec3::new(w2 + x2 - z2 - y2, dxy + dzw, dxz - dyw),
        Vec3::new(-dzw + dxy, y2 - z2 + w2 - x2, dyz + dxw),
        Vec3::new(dyw + dxz, dyz - dxw, z2 - y2 - x2 + w2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JOML 1.10.9 on Java 25 (`Camera.setRotation`, `FORWARDS.rotate`,
    /// `Matrix4f.rotation(rotation.conjugate())`), as bits: yaw, pitch,
    /// the quaternion `xyzw`, the forward vector and the matrix's
    /// columns' first three rows.
    /// `(yaw, pitch)`, quaternion, forwards, view matrix.
    type Bits = ((f32, f32), [u32; 4], [u32; 3], [u32; 12]);

    const JOML: [Bits; 6] = [
        (
            (0.0, 0.0),
            [0x0000_0000, 0x3f80_0000, 0x0000_0000, 0x8000_0000],
            [0x0000_0000, 0x0000_0000, 0x3f80_0000],
            [
                0xbf80_0000,
                0,
                0,
                0,
                0,
                0x3f80_0000,
                0,
                0,
                0,
                0,
                0xbf80_0000,
                0,
            ],
        ),
        (
            (37.25, -20.5),
            [0x3d68_c6eb, 0x3f6e_b8b0, 0xbe2c_abe6, 0x3ea0_e8c3],
            [0xbf11_246c, 0x3eb3_4e62, 0x3f3e_df2d],
            [
                0xbf4b_c6c8,
                0x3e59_10de,
                0x3f11_246b,
                0,
                0xb200_0000,
                0x3f6f_c9be,
                0xbeb3_4e61,
                0,
                0xbf1a_f48b,
                0xbe8e_ba63,
                0xbf3e_df2c,
                0,
            ],
        ),
        (
            (-140.0, 85.0),
            [0x3f22_8557, 0x3e81_1b9c, 0x3e6c_9c6f, 0xbf31_5c44],
            [0x3d65_780e, 0xbf7f_069c, 0xbd88_bc37],
            [
                0x3f44_1b7c,
                0x3f23_ed70,
                0xbd65_7810,
                0,
                0x0000_0000,
                0x3db2_7ec4,
                0x3f7f_069e,
                0,
                0x3f24_8dbd,
                0xbf43_5c72,
                0x3d88_bc3c,
                0,
            ],
        ),
        (
            (275.123, 10.7),
            [0xbd80_db16, 0xbf3c_1aff, 0xbd8c_ecbc, 0x3f2b_fef4],
            [0x3f7a_8b47, 0xbe3e_1f63, 0x3db3_b189],
            [
                0xbdb6_df84,
                0x3e3d_5cf6,
                0xbf7a_8b45,
                0,
                0x0000_0000,
                0x3f7b_8c83,
                0x3e3e_1f62,
                0,
                0x3f7e_fa35,
                0x3c87_d054,
                0xbdb3_b188,
                0,
            ],
        ),
        (
            (179.99, -89.9),
            [0x3f34_dc7e, 0x3881_836f, 0xb881_499e, 0x3f35_2d5f],
            [0xb4a3_8701, 0x3f7f_ffe8, 0xbae4_c202],
            [
                0x3f7f_ffff,
                0x3936_ffed,
                0x34a3_8700,
                0,
                0x0000_0000,
                0x3ae4_c200,
                0xbf7f_ffe6,
                0,
                0xb937_0000,
                0x3f7f_ffe6,
                0x3ae4_c200,
                0,
            ],
        ),
        (
            (12345.678, 45.0),
            [0xbe9c_25c2, 0x3f0e_de30, 0x3e6c_b61b, 0x3f3c_7c97],
            [0xbf2e_48ef, 0xbf35_04f5, 0xbe43_a9f8],
            [
                0x3e8a_5af4,
                0xbf2e_48f0,
                0x3f2e_48ee,
                0,
                0x3300_0000,
                0x3f35_04f2,
                0x3f35_04f4,
                0,
                0xbf76_79de,
                0xbe43_a9fa,
                0x3e43_a9f6,
                0,
            ],
        ),
    ];

    #[test]
    fn the_camera_is_jomls_to_the_bit() {
        for ((y_rot, x_rot), quat, forwards, matrix) in JOML {
            let at = Orientation::new(Look::new(y_rot, x_rot));
            let q = at.rotation;
            assert_eq!(
                [q.x, q.y, q.z, q.w].map(f32::to_bits),
                quat,
                "rotation {y_rot} {x_rot}"
            );
            assert_eq!(
                at.forwards.to_array().map(f32::to_bits),
                forwards,
                "forwards {y_rot} {x_rot}"
            );
            let view = at.view.to_cols_array();
            for col in 0..3 {
                for row in 0..3 {
                    // A zero's sign aside (`-0.0` and `0.0` draw the same).
                    let (ours, theirs) = (view[col * 4 + row], matrix[col * 4 + row]);
                    if theirs & 0x7fff_ffff == 0 {
                        assert_eq!(ours, 0.0, "view {y_rot} {x_rot} [{col}][{row}]");
                    } else {
                        assert_eq!(
                            ours.to_bits(),
                            theirs,
                            "view {y_rot} {x_rot} [{col}][{row}]"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_view_turns_forwards_ahead() {
        for (y_rot, x_rot) in [(0.0, 0.0), (37.0, -20.0), (-140.0, 85.0), (275.0, 10.0)] {
            let at = Orientation::new(Look::new(y_rot, x_rot));
            let ahead = at.view.transform_vector3(at.forwards);
            assert!(ahead.abs_diff_eq(Vec3::NEG_Z, 1e-6), "{ahead}");
            assert!(at.view.transform_vector3(at.up).abs_diff_eq(Vec3::Y, 1e-6));
            assert!(
                at.view
                    .transform_vector3(at.left)
                    .abs_diff_eq(Vec3::NEG_X, 1e-6)
            );
        }
    }

    #[test]
    fn the_view_vector_is_mths() {
        let look = Look::new(90.0, 0.0);
        let v = look.view_vector();
        assert!(v.abs_diff_eq(Vec3::NEG_X, 1e-4), "{v}");
        // South at yaw 0, and the camera agrees.
        let south = Look::new(0.0, 30.0);
        let camera = Orientation::new(south).forwards;
        assert!(south.view_vector().abs_diff_eq(camera, 1e-4));
        let k = -0.123_f32;
        assert_eq!(
            Look::new(k, 0.0).view_vector().x,
            angle::sin(-k * DEG_TO_RAD) * angle::cos(0.0)
        );
    }
}
