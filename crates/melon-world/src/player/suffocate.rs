//! `LocalPlayer.moveTowardsClosestSpace`.

use super::PlayerState;
use crate::level::Level;
use crate::physics::{self, Aabb};

/// The sides tried, in vanilla's order.
const SIDES: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

impl PlayerState {
    /// `LocalPlayer.suffocatesAt`: a suffocating block in the column `(x, z)`
    fn suffocates_at(&self, level: &Level, x: i32, z: i32) -> bool {
        let b = self.bounding_box();
        let test = Aabb {
            min: glam::DVec3::new(f64::from(x), b.min.y, f64::from(z)),
            max: glam::DVec3::new(f64::from(x + 1), b.max.y, f64::from(z + 1)),
        }
        .deflate(1.0e-7);
        physics::collides_with_suffocating(level, &test)
    }

    /// One corner of `moveTowardsClosestSpace(x, z)`.
    fn move_towards_closest_space(&mut self, level: &Level, x: f64, z: f64) {
        let (cx, cz) = (x.floor() as i32, z.floor() as i32);
        if !self.suffocates_at(level, cx, cz) {
            return;
        }
        let (xd, zd) = (x - f64::from(cx), z - f64::from(cz));
        let mut closest = f64::MAX;
        let mut dir = None;
        for (dx, dz) in SIDES {
            let along = if dx != 0 { xd } else { zd };
            let step = dx + dz;
            let to_edge = if step > 0 { 1.0 - along } else { along };
            if to_edge < closest && !self.suffocates_at(level, cx + dx, cz + dz) {
                closest = to_edge;
                dir = Some((dx, dz));
            }
        }
        if let Some((dx, dz)) = dir {
            if dx != 0 {
                self.velocity.x = 0.1 * f64::from(dx);
            } else {
                self.velocity.z = 0.1 * f64::from(dz);
            }
        }
    }

    /// The four corners `LocalPlayer.aiStep` tries, `0.35` of a width out.
    pub(super) fn push_out_of_blocks(&mut self, level: &Level) {
        if self.spectator || self.riding {
            return;
        }
        let reach = self.box_width() * 0.35;
        let (x, z) = (self.feet.x, self.feet.z);
        for (sx, sz) in [(-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0), (1.0, 1.0)] {
            self.move_towards_closest_space(level, x + sx * reach, z + sz * reach);
        }
    }
}
