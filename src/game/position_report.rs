//! `LocalPlayer.sendPosition`: what of the player's movement the server

use glam::DVec3;

/// The packet a tick sends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Report {
    PosRot,
    Pos,
    Rot,
    StatusOnly,
}

/// What the server last heard of the local player: `LocalPlayer`'s
/// `lastSentInput`, `wasSprinting` and position fields.
#[derive(Default)]
pub struct LastSent {
    pub position: PositionReport,
    pub input: crate::network::play::game::PlayerInput,
    pub sprinting: bool,
    /// Send input again even if the keys have not changed: after a teleport.
    pub input_dirty: bool,
}

impl LastSent {
    /// `handleRespawn`'s new `LocalPlayer`: `xLast`, `yRotLast`,
    /// `positionReminder`... start over; the input and sprinting carry over
    /// only under `KEEP_ENTITY_DATA` (`createPlayer(..., lastSentInput,
    /// wasSprinting, ...)`).
    pub fn respawned(&mut self, keep_entity_data: bool) {
        let kept = std::mem::take(self);
        if keep_entity_data {
            self.input = kept.input;
            self.sprinting = kept.sprinting;
            self.input_dirty = kept.input_dirty;
        }
    }
}

/// What the server last heard (`xLast`, `yRotLast`, `lastOnGround`, ...).
#[derive(Default)]
pub struct PositionReport {
    pos: DVec3,
    yaw: f32,
    pitch: f32,
    on_ground: bool,
    horizontal_collision: bool,
    reminder: u32,
}

impl PositionReport {
    /// Moved farther than this (squared: `2.0E-4`) to count.
    const MOVE_SQUARED: f64 = 2.0e-4 * 2.0e-4;
    const REMINDER_TICKS: u32 = 20;

    /// The packet for this tick, if one is due.
    pub fn tick(
        &mut self,
        pos: DVec3,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
        horizontal_collision: bool,
    ) -> Option<Report> {
        self.reminder += 1;
        let moved = (pos - self.pos).length_squared() > Self::MOVE_SQUARED
            || self.reminder >= Self::REMINDER_TICKS;
        let turned = yaw != self.yaw || pitch != self.pitch;
        let report = match (moved, turned) {
            (true, true) => Some(Report::PosRot),
            (true, false) => Some(Report::Pos),
            (false, true) => Some(Report::Rot),
            (false, false) => (self.on_ground != on_ground
                || self.horizontal_collision != horizontal_collision)
                .then_some(Report::StatusOnly),
        };
        if moved {
            self.pos = pos;
            self.reminder = 0;
        }
        if turned {
            self.yaw = yaw;
            self.pitch = pitch;
        }
        self.on_ground = on_ground;
        self.horizontal_collision = horizontal_collision;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_still_is_quiet_with_a_reminder_every_twenty_ticks() {
        let mut report = PositionReport::default();
        let at = DVec3::new(1.0, 64.0, 1.0);
        assert_eq!(report.tick(at, 0.0, 0.0, true, false), Some(Report::Pos));
        for _ in 0..19 {
            assert_eq!(report.tick(at, 0.0, 0.0, true, false), None);
        }
        assert_eq!(report.tick(at, 0.0, 0.0, true, false), Some(Report::Pos));
    }

    #[test]
    fn turning_and_status_changes_have_their_own_packets() {
        let mut report = PositionReport::default();
        let at = DVec3::ZERO;
        report.tick(at, 0.0, 0.0, true, false);
        assert_eq!(report.tick(at, 5.0, 0.0, true, false), Some(Report::Rot));
        assert_eq!(
            report.tick(at, 5.0, 0.0, false, false),
            Some(Report::StatusOnly)
        );
        assert_eq!(
            report.tick(DVec3::X, 9.0, 0.0, false, false),
            Some(Report::PosRot)
        );
    }
}
