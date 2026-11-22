//! `ClientPacketListener.handleMovePlayer`: the server moves the player.

use glam::DVec3;

use crate::game::{InWorld, melon_yaw, minecraft_yaw};
use crate::network::Teleport;

impl InWorld {
    /// `handleMovePlayer`, with its relative flags for position, rotation
    /// and motion; the teleport is accepted unless it already was.
    pub(in crate::game) fn teleport(&mut self, t: &Teleport) {
        self.predictions.on_teleport();
        self.positioned_in_new_level();
        if self.entities.own_vehicle().is_none() {
            let feet = self.player.feet();
            let old_yaw = minecraft_yaw(self.player.yaw());
            let old_pitch = self.player.pitch();
            let new_yaw = if t.relative & Teleport::YAW != 0 {
                old_yaw + t.yaw
            } else {
                t.yaw
            };
            let new_pitch = if t.relative & Teleport::PITCH != 0 {
                old_pitch + t.pitch
            } else {
                t.pitch
            }
            .clamp(-90.0, 90.0);
            let mut velocity = self.player.velocity();
            if t.relative & Teleport::ROTATE_DELTA != 0 {
                velocity = rotate_motion(velocity, old_yaw - new_yaw, old_pitch - new_pitch);
            }
            let (x, y, z) = t.pos;
            if crate::probe::enabled() {
                let (from, rel) = (feet, t.relative);
                crate::probe::note(
                    "teleport",
                    format!(
                        "to=({x:.2},{y:.2},{z:.2}) {}",
                        crate::probe::relative_flags(rel)
                    ),
                    || {
                        format!(
                            "id={} from={from:?} delta={:?} dist={:.4} on_ground={} handler=game-thread apply then accept_teleportation",
                            t.id,
                            t.delta,
                            from.distance(DVec3::new(x, y, z)),
                            self.player.on_ground()
                        )
                    },
                );
                let now = self.clocks.game_time();
                let last =
                    crate::game::tick::LAST_HOLD.swap(now, std::sync::atomic::Ordering::Relaxed);
                if rel & 0x7 == 0 && (now - last).abs() > 100 {
                    crate::game::tick::HOLD_TRACE_UNTIL
                        .store(now + 30, std::sync::atomic::Ordering::Relaxed);
                    let level = &self.level;
                    crate::probe::note("hold", format!("{now}"), || {
                        let (bx, by, bz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
                        let mut cells = String::new();
                        for dy in (-1..=3).rev() {
                            for dz in -1..=1 {
                                for dx in -1..=1 {
                                    let at = (bx + dx, by + dy, bz + dz);
                                    let id =
                                        crate::world::physics::block_at(level, at.0, at.1, at.2);
                                    if id == 0 {
                                        continue;
                                    }
                                    let boxes: Vec<String> =
                                        crate::world::physics::block_shapes(level, at)
                                            .map(|b| format!("y{:.3}..{:.3}", b.min.y, b.max.y))
                                            .collect();
                                    cells.push_str(&format!("\n  {at:?} state={id} {boxes:?}"));
                                }
                            }
                        }
                        format!(
                            "packets of the last 5 s: {}\nwe sent (last 5 s): {}\nour last moves:{}\nmove.log:{}\nblocks around the pin ({x:.2},{y:.2},{z:.2}), y-1..head+1:{cells}",
                            crate::probe::recent_packets(5),
                            crate::probe::recent_sent(5),
                            crate::probe::recent_moves(),
                            crate::probe::recent_io()
                        )
                    });
                }
            }
            self.player.teleport(
                DVec3::new(
                    t.resolve(Teleport::X, feet.x, x),
                    t.resolve(Teleport::Y, feet.y, y),
                    t.resolve(Teleport::Z, feet.z, z),
                ),
                DVec3::new(
                    t.resolve(Teleport::DELTA_X, velocity.x, t.delta[0]),
                    t.resolve(Teleport::DELTA_Y, velocity.y, t.delta[1]),
                    t.resolve(Teleport::DELTA_Z, velocity.z, t.delta[2]),
                ),
                DVec3::new(
                    t.resolve(Teleport::X, self.player.prev_feet().x, x),
                    t.resolve(Teleport::Y, self.player.prev_feet().y, y),
                    t.resolve(Teleport::Z, self.player.prev_feet().z, z),
                ),
            );
            self.player.settle_after_teleport(&self.level);
            self.player.set_rotation(melon_yaw(new_yaw), new_pitch);
        }
        if !t.acked {
            let feet = self.player.feet();
            self.joined.session.send_accept_teleport(
                t.id,
                (feet.x, feet.y, feet.z),
                minecraft_yaw(self.player.yaw()),
                self.player.pitch(),
            );
        }
        self.sent.input_dirty = true;
        self.abort_destroy();
    }
}

/// `Vec3.xRot(pitch).yRot(yaw)`, the angles in degrees: a motion turned
/// with the look.
fn rotate_motion(v: DVec3, yaw: f32, pitch: f32) -> DVec3 {
    let (sin, cos) = f64::from(pitch).to_radians().sin_cos();
    let v = DVec3::new(v.x, v.y * cos + v.z * sin, v.z * cos - v.y * sin);
    let (sin, cos) = f64::from(yaw).to_radians().sin_cos();
    DVec3::new(v.x * cos + v.z * sin, v.y, v.z * cos - v.x * sin)
}

#[cfg(test)]
mod rotate_tests {
    use super::*;

    #[test]
    fn a_motion_turns_with_the_look() {
        let v = rotate_motion(DVec3::new(0.0, 0.0, 1.0), 90.0, 0.0);
        assert!((v.x - 1.0).abs() < 1e-12 && v.z.abs() < 1e-12);
        assert_eq!(rotate_motion(DVec3::Y, 0.0, 0.0), DVec3::Y);
    }
}
