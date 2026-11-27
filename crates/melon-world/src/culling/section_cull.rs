//! The frustum and fog test of a chunk column's sections all at once, as
//! one bit per section.
//!
//! A column's sections differ only in y. So a plane's test of a section's
//! box, `n·c + d + |n|·half >= 0`, splits into a part that depends only on y
//! (`n.y·c.y + d + |n|·half`, the same in every column, worked out once a
//! frame) and one that depends only on the column (`n.x·c.x + n.z·c.z`).
//! Per column that leaves one compare per plane and section: eight sections
//! a register on AVX ([`super::x86::section_cull`]), one at a time otherwise.
//! The fog's nearest-point test splits the same way.

use glam::{IVec3, Vec3};

use melon_core::math::SECTION_SIZE;

/// Sections a column can have: one bit each in a `u32`.
pub const LANES: usize = 32;
/// Half a section's side.
const HALF: f32 = SECTION_SIZE as f32 / 2.0;

/// One value per section height, aligned for vector loads.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct Row(pub [f32; LANES]);

/// One frame's [`super::Culler`] as rows over the section heights.
pub struct SectionCull {
    /// Per plane, for each section height: the y part of its test.
    pub planes: Option<[Row; 6]>,
    /// Per plane, its normal's x and z.
    pub across: [(f32, f32); 6],
    /// For each section height: the squared y distance of its nearest
    /// point, and the fog's end squared.
    pub fog: Option<(Row, f32)>,
    eye_block: IVec3,
    eye_fraction: Vec3,
    #[cfg(target_arch = "x86_64")]
    avx: Option<super::x86::section_cull::CullAvx>,
}

impl SectionCull {
    /// `planes` relative to the eye (none: no frustum culling), the fog's
    /// end, and the eye split as [`super::Culler`] keeps it.
    pub fn new(
        planes: Option<[(Vec3, Vec3, f32); 6]>,
        fog_end: Option<f32>,
        eye_block: IVec3,
        eye_fraction: Vec3,
        min_y: i32,
    ) -> Self {
        let center_y = |y: usize| {
            (min_y + y as i32 * SECTION_SIZE - eye_block.y) as f32 - eye_fraction.y + HALF
        };
        let across = planes.map_or([(0.0, 0.0); 6], |planes| planes.map(|(n, ..)| (n.x, n.z)));
        let planes = planes.map(|planes| {
            planes.map(|(normal, reach, distance)| {
                let reach = reach.dot(Vec3::splat(HALF));
                Row(std::array::from_fn(|y| {
                    normal.y * center_y(y) + distance + reach
                }))
            })
        });
        let fog = fog_end.map(|end| {
            let near = Row(std::array::from_fn(|y| nearest(center_y(y)).powi(2)));
            (near, end * end)
        });
        Self {
            planes,
            across,
            fog,
            eye_block,
            eye_fraction,
            #[cfg(target_arch = "x86_64")]
            avx: super::x86::section_cull::CullAvx::new(),
        }
    }

    /// The sections of chunk `(x, z)` in view (bit per section height).
    #[inline]
    pub fn column(&self, (x, z): (i32, i32)) -> u32 {
        let center = |block: i32, eye: i32, fraction: f32| {
            (block * SECTION_SIZE - eye) as f32 - fraction + HALF
        };
        let cx = center(x, self.eye_block.x, self.eye_fraction.x);
        let cz = center(z, self.eye_block.z, self.eye_fraction.z);
        // A section is in front of plane `p` when its row is at least this.
        let least = self.across.map(|(nx, nz)| -(nx * cx + nz * cz));
        let fog_room = self
            .fog
            .as_ref()
            .map(|(_, end)| end - nearest(cx).powi(2) - nearest(cz).powi(2));
        #[cfg(target_arch = "x86_64")]
        if let Some(avx) = self.avx {
            return super::x86::section_cull::column(avx, self, &least, fog_room);
        }
        self.column_scalar(&least, fog_room)
    }

    /// [`Self::column`] without AVX: one all-ones or zero word per section, ANDed plane by
    /// plane, so LLVM compiles each pass to SSE2 or NEON compares.
    pub fn column_scalar(&self, least: &[f32; 6], fog_room: Option<f32>) -> u32 {
        let mut keep = [u32::MAX; LANES];
        if let Some(planes) = &self.planes {
            for (row, &least) in planes.iter().zip(least) {
                for (keep, &y_part) in keep.iter_mut().zip(&row.0) {
                    *keep &= u32::from(y_part >= least).wrapping_neg();
                }
            }
        }
        // Nearer than the fog's end, NaN not: as the AVX body's `cmplt`.
        if let (Some((near, _)), Some(room)) = (&self.fog, fog_room) {
            for (keep, &near) in keep.iter_mut().zip(&near.0) {
                *keep &= u32::from(near < room).wrapping_neg();
            }
        }
        keep.iter()
            .enumerate()
            .fold(0, |mask, (y, &keep)| mask | (keep & 1) << y)
    }
}

/// How far a section's box centred `center` away along an axis is at its
/// nearest: `0` when it spans the eye.
#[inline]
fn nearest(center: f32) -> f32 {
    (center.abs() - HALF).max(0.0)
}

#[cfg(test)]
mod tests {
    use glam::{DVec3, Mat4};

    use super::*;
    use crate::culling::frustum::Frustum;
    use crate::culling::{CullSettings, Culler};
    use melon_core::util::xorshift::XorShift;

    /// Every column of a view, by section, gives what the box test of
    /// each section gives; on every tier.
    #[test]
    fn columns_match_the_section_by_section_test() {
        let mut state = XorShift(0x9E37_79B9_7F4A_7C15);
        let (mut checked, mut seen) = (0, 0);
        for round in 0..64 {
            let eye = DVec3::new(
                state.next_f64() * 4000.0 - 2000.0,
                state.next_f64() * 300.0 - 64.0,
                state.next_f64() * 4000.0 - 2000.0,
            );
            let yaw = state.next_f64() as f32 * std::f32::consts::TAU;
            let pitch = (state.next_f64() as f32 - 0.5) * 3.0;
            let view = Mat4::from_rotation_x(pitch) * Mat4::from_rotation_y(yaw);
            let projection = crate::projection::world_projection(70.0, 16.0 / 9.0, 512.0);
            let settings = CullSettings {
                frustum: round % 7 != 0,
                ..CullSettings::default()
            };
            let mut culler = Culler::new(
                Frustum::from_view_projection(projection * view),
                None,
                eye,
                (-64, 24),
                settings,
            );
            if round % 3 != 0 {
                culler = culler.with_fog(96.0 + state.next_f64() as f32 * 300.0);
            }
            let cull = culler.section_cull();
            let (ex, ez) = ((eye.x / 16.0).floor() as i32, (eye.z / 16.0).floor() as i32);
            for x in ex - 20..=ex + 20 {
                for z in ez - 20..=ez + 20 {
                    let mask = cull.column((x, z));
                    for y in 0..24 {
                        let near_edge = edge_distance(&culler, (x, y, z)) < 1e-3;
                        let want = culler.sees_section((x, y, z));
                        let got = mask >> y & 1 != 0;
                        assert!(
                            near_edge || want == got,
                            "section {x} {y} {z} of round {round}: {got} not {want}"
                        );
                        checked += 1;
                        seen += usize::from(want);
                    }
                }
            }
        }
        assert!(checked > 1_000_000);
        // Neither all in view nor all culled.
        assert!(
            seen > checked / 50 && seen < checked / 2,
            "{seen} of {checked}"
        );
    }

    /// How close the section's box is to a plane or the fog's end: there
    /// a different rounding may land on the other side.
    fn edge_distance(culler: &Culler, (x, y, z): (i32, i32, i32)) -> f32 {
        let min = DVec3::new(
            f64::from(x * 16),
            f64::from(-64 + y * 16),
            f64::from(z * 16),
        ) - culler.eye();
        let (min, max) = (min.as_vec3(), (min + DVec3::splat(16.0)).as_vec3());
        let (center, half) = ((min + max) * 0.5, (max - min) * 0.5);
        let planes = culler.frustum.planes();
        let mut closest = planes
            .iter()
            .map(|(n, r, d)| (n.dot(center) + d + r.dot(half)).abs())
            .fold(f32::MAX, f32::min);
        if let Some(end) = culler.fog_end {
            let near = Vec3::ZERO.clamp(min, max).length();
            closest = closest.min((near - end).abs());
        }
        closest
    }

    /// Times the column test's bodies over a render distance 32 view.
    /// `cargo test --profile fast --lib column_cull_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing"]
    fn column_cull_speed() {
        let projection = crate::projection::world_projection(70.0, 16.0 / 9.0, 512.0);
        let culler = Culler::new(
            Frustum::from_view_projection(projection * Mat4::from_rotation_y(0.7)),
            None,
            DVec3::new(10.3, 70.2, -5.9),
            (-64, 24),
            CullSettings::default(),
        )
        .with_fog(512.0);
        let cull = culler.section_cull();
        let parts: Vec<_> = (-32..=32)
            .flat_map(|x| (-32..=32).map(move |z| (x, z)))
            .map(|(x, z)| {
                let cx = (x * 16 - 10) as f32 - 0.3 + HALF;
                let cz = (z * 16 + 6) as f32 - 0.1 + HALF;
                let least = cull.across.map(|(nx, nz)| -(nx * cx + nz * cz));
                let room = cull
                    .fog
                    .as_ref()
                    .map(|(_, end)| end - nearest(cx).powi(2) - nearest(cz).powi(2));
                (x, z, least, room)
            })
            .collect();
        type Body<'a> = Box<dyn Fn(&(i32, i32, [f32; 6], Option<f32>)) -> u32 + 'a>;
        let bodies: [(&str, Body); 2] = [
            ("column (dispatch)", Box::new(|p| cull.column((p.0, p.1)))),
            ("scalar, masks", Box::new(|p| cull.column_scalar(&p.2, p.3))),
        ];
        for (name, body) in &bodies {
            let mut best = std::time::Duration::MAX;
            let mut sum = 0u32;
            for _ in 0..200 {
                let t = std::time::Instant::now();
                for p in &parts {
                    sum = sum.wrapping_add(std::hint::black_box(body(std::hint::black_box(p))));
                }
                best = best.min(t.elapsed());
            }
            println!(
                "{name:<20} {:>8.1} µs per frame ({} columns), {sum}",
                best.as_secs_f64() * 1e6,
                parts.len()
            );
        }
    }

    /// The scalar way gives what the vector one does, bit for bit.
    #[test]
    fn scalar_and_vector_agree() {
        let projection = crate::projection::world_projection(70.0, 16.0 / 9.0, 512.0);
        let culler = Culler::new(
            Frustum::from_view_projection(projection * Mat4::from_rotation_y(0.7)),
            None,
            DVec3::new(10.3, 70.2, -5.9),
            (-64, 24),
            CullSettings::default(),
        )
        .with_fog(200.0);
        let cull = culler.section_cull();
        for x in -20..=20 {
            for z in -20..=20 {
                let cx = (x * 16 - 10) as f32 - 0.3 + HALF;
                let cz = (z * 16 + 6) as f32 - 0.1 + HALF;
                let least = cull.across.map(|(nx, nz)| -(nx * cx + nz * cz));
                let room = cull
                    .fog
                    .as_ref()
                    .map(|(_, end)| end - nearest(cx).powi(2) - nearest(cz).powi(2));
                assert_eq!(cull.column((x, z)), cull.column_scalar(&least, room));
            }
        }
    }
}
