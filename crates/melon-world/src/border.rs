//! The world border (`WorldBorder`, as the client keeps it).

use glam::DVec3;

use crate::mesh::WorldVertex;
use crate::physics::Aabb;

/// `WorldBorder.MAX_SIZE`: a border this wide is no border.
const DEFAULT_SIZE: f64 = 5.999_996_8E7;
/// `WorldBorder.MAX_CENTER_COORDINATE`'s neighbour.
const DEFAULT_ABSOLUTE_MAX: i32 = 29_999_984;
/// How far the collision slabs reach out past the border.
const SLAB: f64 = 1.0e8;

/// What the server tells about the border, one packet each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderUpdate {
    Initialize {
        center: (f64, f64),
        from: f64,
        to: f64,
        ticks: i64,
        absolute_max: i32,
        warning_blocks: i32,
        warning_time: i32,
    },
    Center(f64, f64),
    Lerp {
        from: f64,
        to: f64,
        ticks: i64,
    },
    Size(f64),
    WarningTime(i32),
    WarningBlocks(i32),
}

/// `BorderStatus`: which way the border moves, by the colour of its wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Growing,
    Shrinking,
    Stationary,
}

impl Status {
    /// `BorderStatus.getColor`.
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Growing => [0x40, 0xFF, 0x80],
            Self::Shrinking => [0xFF, 0x30, 0x30],
            Self::Stationary => [0x20, 0xA0, 0xFF],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldBorder {
    center: (f64, f64),
    from: f64,
    to: f64,
    /// The length of the lerp, and the ticks of it still to go.
    total: i64,
    left: i64,
    absolute_max: i32,
    warning_blocks: i32,
    warning_time: i32,
}

impl Default for WorldBorder {
    fn default() -> Self {
        Self {
            center: (0.0, 0.0),
            from: DEFAULT_SIZE,
            to: DEFAULT_SIZE,
            total: 0,
            left: 0,
            absolute_max: DEFAULT_ABSOLUTE_MAX,
            warning_blocks: 5,
            warning_time: 15,
        }
    }
}

impl WorldBorder {
    pub fn apply(&mut self, update: BorderUpdate) {
        match update {
            BorderUpdate::Initialize {
                center,
                from,
                to,
                ticks,
                absolute_max,
                warning_blocks,
                warning_time,
            } => {
                self.center = center;
                self.absolute_max = absolute_max;
                self.warning_blocks = warning_blocks;
                self.warning_time = warning_time;
                self.lerp(from, to, ticks);
            }
            BorderUpdate::Center(x, z) => self.center = (x, z),
            BorderUpdate::Lerp { from, to, ticks } => self.lerp(from, to, ticks),
            BorderUpdate::Size(size) => self.lerp(size, size, 0),
            BorderUpdate::WarningTime(time) => self.warning_time = time,
            BorderUpdate::WarningBlocks(blocks) => self.warning_blocks = blocks,
        }
    }

    /// `setLerp`: from `from` to `to` over `ticks`; none at all sets it.
    fn lerp(&mut self, from: f64, to: f64, ticks: i64) {
        self.from = from;
        self.to = to;
        self.total = ticks.max(0);
        self.left = self.total;
    }

    /// `WorldBorder.tick`: one tick of the lerp.
    pub fn tick(&mut self) {
        self.left = (self.left - 1).max(0);
    }

    fn moving(&self) -> bool {
        self.total > 0 && self.left > 0
    }

    /// The width now (`getSize`).
    pub fn size(&self) -> f64 {
        if !self.moving() {
            return self.to;
        }
        let done = (self.total - self.left) as f64 / self.total as f64;
        self.from + (self.to - self.from) * done
    }

    pub fn status(&self) -> Status {
        if !self.moving() || self.from == self.to {
            Status::Stationary
        } else if self.from < self.to {
            Status::Growing
        } else {
            Status::Shrinking
        }
    }

    /// `(min_x, min_z, max_x, max_z)`, kept within the world's edge.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let half = self.size() / 2.0;
        let edge = f64::from(self.absolute_max);
        (
            (self.center.0 - half).max(-edge),
            (self.center.1 - half).max(-edge),
            (self.center.0 + half).min(edge),
            (self.center.1 + half).min(edge),
        )
    }

    /// `getDistanceToBorder`: to the nearest side, negative outside.
    pub fn distance_to(&self, x: f64, z: f64) -> f64 {
        let (min_x, min_z, max_x, max_z) = self.bounds();
        (x - min_x).min(max_x - x).min((z - min_z).min(max_z - z))
    }

    /// `isWithinBounds`.
    pub fn contains(&self, x: f64, z: f64) -> bool {
        let (min_x, min_z, max_x, max_z) = self.bounds();
        x >= min_x && x < max_x && z >= min_z && z < max_z
    }

    /// How red the vignette is at `(x, z)`, `0..=1` (`Gui.updateVignette`).
    pub fn warning(&self, x: f64, z: f64) -> f32 {
        let speed = if self.moving() {
            (self.to - self.from).abs() / (self.total as f64 * 50.0)
        } else {
            0.0
        };
        let reach =
            (speed * f64::from(self.warning_time) * 1000.0).min((self.to - self.size()).abs());
        let reach = reach.max(f64::from(self.warning_blocks));
        let distance = self.distance_to(x, z);
        if distance < reach {
            (1.0 - distance / reach) as f32
        } else {
            0.0
        }
    }

    /// What stops a body inside the border from leaving it.
    pub fn collision_boxes(&self, mover: &Aabb) -> Vec<Aabb> {
        let (min_x, min_z, max_x, max_z) = self.bounds();
        let inside = mover.min.x >= min_x - 1.0e-7
            && mover.max.x <= max_x + 1.0e-7
            && mover.min.z >= min_z - 1.0e-7
            && mover.max.z <= max_z + 1.0e-7;
        if !inside {
            return Vec::new();
        }
        let slab = |min: DVec3, max: DVec3| Aabb { min, max };
        vec![
            slab(
                DVec3::new(-SLAB, -SLAB, -SLAB),
                DVec3::new(min_x, SLAB, SLAB),
            ),
            slab(
                DVec3::new(max_x, -SLAB, -SLAB),
                DVec3::new(SLAB, SLAB, SLAB),
            ),
            slab(
                DVec3::new(-SLAB, -SLAB, -SLAB),
                DVec3::new(SLAB, SLAB, min_z),
            ),
            slab(
                DVec3::new(-SLAB, -SLAB, max_z),
                DVec3::new(SLAB, SLAB, SLAB),
            ),
        ]
    }

    /// `WorldBorderRenderer`'s wall.
    pub fn wall(
        &self,
        camera: DVec3,
        origin: glam::IVec3,
        range: f64,
        height: f64,
        scroll: f32,
    ) -> Vec<WorldVertex> {
        let (min_x, min_z, max_x, max_z) = self.bounds();
        let [r, g, b] = self.status().color();
        let at = origin.as_dvec3();
        let mut out = Vec::new();
        let sides = [
            (camera.x - min_x, min_x, true),
            (max_x - camera.x, max_x, true),
            (camera.z - min_z, min_z, false),
            (max_z - camera.z, max_z, false),
        ];
        for (distance, line, along_z) in sides {
            let distance = distance.abs();
            if distance >= range {
                continue;
            }
            let alpha = ((1.0 - distance / range).powi(4) as f32 * 255.0).round() as u8;
            if alpha == 0 {
                continue;
            }
            let (start, end) = if along_z {
                (camera.z - range, camera.z + range)
            } else {
                (camera.x - range, camera.x + range)
            };
            let (lo, hi) = if along_z {
                (min_z, max_z)
            } else {
                (min_x, max_x)
            };
            let (start, end) = (start.max(lo), end.min(hi));
            if end <= start {
                continue;
            }
            let u0 = scroll + (start * 0.5).rem_euclid(1.0) as f32;
            let u1 = u0 + ((end - start) * 0.5) as f32;
            let bottom = camera.y - height;
            let top = camera.y + height;
            let v_of = |y: f64| scroll + (y * 0.5).rem_euclid(1.0) as f32;
            let v_bottom = v_of(bottom);
            let v_top = v_bottom + ((top - bottom) * 0.5) as f32;
            let corner = |along: f64, y: f64, u: f32, v: f32| {
                let p = if along_z {
                    DVec3::new(line, y, along)
                } else {
                    DVec3::new(along, y, line)
                };
                WorldVertex {
                    pos: (p - at).as_vec3().to_array(),
                    uv: [u, v],
                    color: [r, g, b, alpha],
                    light: WorldVertex::SENTINEL_LIGHT,
                    face: 0,
                    _pad: 0,
                }
            };
            let quad = [
                corner(start, bottom, u0, v_bottom),
                corner(end, bottom, u1, v_bottom),
                corner(end, top, u1, v_top),
                corner(start, top, u0, v_top),
            ];
            out.extend(quad);
            out.extend([quad[3], quad[2], quad[1], quad[0]]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(size: f64) -> WorldBorder {
        let mut border = WorldBorder::default();
        border.apply(BorderUpdate::Size(size));
        border
    }

    #[test]
    fn the_border_grows_over_its_ticks_and_stops() {
        let mut border = WorldBorder::default();
        border.apply(BorderUpdate::Lerp {
            from: 100.0,
            to: 200.0,
            ticks: 100,
        });
        assert_eq!(border.size(), 100.0);
        assert_eq!(border.status(), Status::Growing);
        for _ in 0..50 {
            border.tick();
        }
        assert_eq!(border.size(), 150.0);
        for _ in 0..60 {
            border.tick();
        }
        assert_eq!(border.size(), 200.0);
        assert_eq!(border.status(), Status::Stationary);
    }

    #[test]
    fn distance_and_bounds_follow_centre_and_size() {
        let mut border = at(100.0);
        border.apply(BorderUpdate::Center(10.0, 0.0));
        assert_eq!(border.bounds(), (-40.0, -50.0, 60.0, 50.0));
        assert_eq!(border.distance_to(10.0, 0.0), 50.0);
        assert_eq!(border.distance_to(55.0, 0.0), 5.0);
        assert!(border.distance_to(70.0, 0.0) < 0.0);
        assert!(border.contains(0.0, 0.0) && !border.contains(60.0, 0.0));
    }

    #[test]
    fn the_vignette_reddens_inside_the_warning_distance() {
        let border = at(100.0);
        assert_eq!(border.warning(0.0, 0.0), 0.0);
        assert!((border.warning(50.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((border.warning(47.5, 0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_body_inside_is_held_in_and_one_outside_is_left_alone() {
        let border = at(100.0);
        let inside = Aabb::at_feet(DVec3::new(49.0, 64.0, 0.0), 0.6, 1.8);
        let boxes = border.collision_boxes(&inside);
        assert_eq!(boxes.len(), 4);
        let outside = Aabb::at_feet(DVec3::new(80.0, 64.0, 0.0), 0.6, 1.8);
        assert!(border.collision_boxes(&outside).is_empty());
    }

    #[test]
    fn the_wall_shows_only_sides_in_range_and_fades_in() {
        let border = at(100.0);
        let far = border.wall(
            DVec3::new(0.0, 64.0, 0.0),
            glam::IVec3::ZERO,
            32.0,
            64.0,
            0.0,
        );
        assert!(far.is_empty());
        let near = border.wall(
            DVec3::new(48.0, 64.0, 0.0),
            glam::IVec3::ZERO,
            32.0,
            64.0,
            0.0,
        );
        assert_eq!(near.len(), 8);
        assert!(near[0].color[3] > 0);
    }
}
