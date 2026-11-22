//! `BlockStatePredictionHandler`.

use super::InWorld;
use std::collections::HashMap;

struct Verified {
    sequence: i32,
    state: u16,
    player_pos: glam::DVec3,
}

#[derive(Default)]
pub(super) struct Predictions {
    verified: HashMap<[i32; 3], Verified>,
    /// `currentSequenceNr`: 0 until the first prediction, which is 1.
    sequence: i32,
    last_teleport: Option<i32>,
}

impl Predictions {
    /// `startPredicting`: the sequence number of the action about to be sent.
    pub(super) fn start_predicting(&mut self) -> i32 {
        self.sequence += 1;
        self.sequence
    }

    /// `onTeleport`: a snap back to before the teleport would undo it.
    pub(super) fn on_teleport(&mut self) {
        self.last_teleport = Some(self.sequence);
    }

    /// `retainKnownServerState`.
    fn retain(&mut self, pos: [i32; 3], old: u16, sequence: i32, player: glam::DVec3) {
        self.verified
            .entry(pos)
            .and_modify(|v| v.sequence = sequence)
            .or_insert(Verified {
                sequence,
                state: old,
                player_pos: player,
            });
    }

    /// `updateKnownServerState`: whether the block is under prediction.
    fn update_known(&mut self, pos: [i32; 3], state: u16) -> bool {
        self.verified.get_mut(&pos).is_some_and(|v| {
            v.state = state;
            true
        })
    }
}

impl InWorld {
    /// A block the player's own action changes, shown now.
    pub(super) fn predict_block(&mut self, pos: [i32; 3], state: u16) {
        let player = self.player.feet();
        if let Some(old) = self.level.set_block(pos, state, true)
            && old != state
        {
            let sequence = self.predictions.sequence;
            self.predictions.retain(pos, old, sequence, player);
        }
    }

    /// `ClientLevel.setServerVerifiedBlockState`.
    pub(super) fn set_server_verified_block(&mut self, pos: [i32; 3], state: u16) {
        if !self.predictions.update_known(pos, state) {
            self.level.set_block(pos, state, false);
        }
    }

    /// `endPredictionsUpTo`, with `ClientLevel.syncBlockState`.
    pub(super) fn end_predictions_up_to(&mut self, sequence: i32) {
        let done: Vec<[i32; 3]> = self
            .predictions
            .verified
            .iter()
            .filter(|(_, v)| v.sequence <= sequence)
            .map(|(pos, _)| *pos)
            .collect();
        let snap_back = self.predictions.last_teleport.is_none_or(|t| t < sequence);
        for pos in done {
            let Some(v) = self.predictions.verified.remove(&pos) else {
                continue;
            };
            if self.level.block_state(pos[0], pos[1], pos[2]) == Some(v.state) {
                continue;
            }
            if crate::probe::enabled() {
                let shown = self.level.block_state(pos[0], pos[1], pos[2]);
                crate::probe::note("prediction.reverted", format!("{pos:?}"), || {
                    format!(
                        "client had {shown:?}, server says {}, player colliding {}",
                        v.state,
                        self.player.is_colliding_with_block(&self.level, pos)
                    )
                });
            }
            self.level.set_block(pos, v.state, false);
            if snap_back && self.player.is_colliding_with_block(&self.level, pos) {
                self.player.snap_to(v.player_pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    #[test]
    fn the_first_old_state_and_position_stay_while_newer_predictions_pile_on() {
        let mut p = Predictions::default();
        p.retain([1, 2, 3], 7, 1, DVec3::new(0.0, 64.0, 0.0));
        p.retain([1, 2, 3], 0, 2, DVec3::new(5.0, 64.0, 0.0));
        let v = &p.verified[&[1, 2, 3]];
        assert_eq!((v.sequence, v.state, v.player_pos.x), (2, 7, 0.0));
        assert!(p.update_known([1, 2, 3], 9));
        assert_eq!(p.verified[&[1, 2, 3]].state, 9);
        assert!(!p.update_known([4, 4, 4], 9));
    }
}
