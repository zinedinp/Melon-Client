//! The level after the server sent the player to another dimension.

use super::{InWorld, LoadingReason, Received};

/// Ticks to wait for the player's chunk before giving up on the rest of the loading.
const TIMEOUT_TICKS: u32 = 30 * 20;
/// Ticks the surroundings get to mesh once the player's chunk is there.
const MESH_TICKS: u32 = 40;

/// A level being loaded.
#[derive(Clone, Copy, Debug)]
pub struct LevelLoad {
    /// Ticks since the respawn.
    ticks: u32,
    /// Whether the server has said where the player is in the new level.
    positioned: bool,
    /// Ticks since the player's chunk arrived.
    chunk_ticks: Option<u32>,
}

impl Default for LevelLoad {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelLoad {
    pub fn new() -> Self {
        Self {
            ticks: 0,
            positioned: false,
            chunk_ticks: None,
        }
    }
}

impl InWorld {
    /// The server put the player somewhere in the new level.
    pub(super) fn positioned_in_new_level(&mut self) {
        if let Some(load) = &mut self.loading {
            load.positioned = true;
        }
    }

    /// One tick of waiting: the player's chunk, then a moment for meshes.
    pub(super) fn tick_loading(&mut self) {
        let Some(mut load) = self.loading else {
            return;
        };
        load.ticks += 1;
        let feet = self.player.feet();
        if load.positioned
            && load.chunk_ticks.is_none()
            && self
                .level
                .is_loaded_at(feet.x.floor() as i32, feet.z.floor() as i32)
        {
            load.chunk_ticks = Some(0);
        }
        if let Some(ticks) = &mut load.chunk_ticks {
            *ticks += 1;
        }
        let meshed = self.level.mesh_stats().in_flight == 0;
        let ready = load
            .chunk_ticks
            .is_some_and(|ticks| ticks >= MESH_TICKS || (meshed && ticks >= 2));
        if ready || load.ticks >= TIMEOUT_TICKS {
            self.joined.session.send_player_loaded();
            self.loading = None;
            self.level_became_ready = true;
        } else {
            self.loading = Some(load);
        }
    }

    /// What this frame's loading means for the screens.
    pub(super) fn report_loading(
        &mut self,
        received: &mut Received,
        started: Option<LoadingReason>,
    ) {
        received.loading = started;
        received.loaded = std::mem::take(&mut self.level_became_ready);
    }
}
