//! What the server says about the level besides its blocks and border.

/// `ClientboundSetDefaultSpawnPositionPacket`: where the world's spawn is.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnPoint {
    pub dimension: String,
    pub pos: [i32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

/// One packet's worth of it.
#[derive(Clone, Debug, PartialEq)]
pub enum LevelSetting {
    /// `ClientboundChangeDifficultyPacket`: peaceful 0 to hard 3.
    Difficulty {
        difficulty: u8,
        locked: bool,
    },
    DefaultSpawn(SpawnPoint),
    /// `ClientboundSetChunkCacheCenterPacket`: the chunk the view is around.
    ChunkCenter {
        x: i32,
        z: i32,
    },
    /// `ClientboundSetChunkCacheRadiusPacket`: the server's view distance.
    ChunkRadius(i32),
    SimulationDistance(i32),
    /// `ClientboundTickingStatePacket`: ticks per second, and `/tick freeze`.
    TickingState {
        rate: f32,
        frozen: bool,
    },
    /// `ClientboundTickingStepPacket`: ticks to run while frozen.
    TickingStep(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LevelSettings {
    pub difficulty: u8,
    pub difficulty_locked: bool,
    pub spawn: Option<SpawnPoint>,
    pub chunk_center: (i32, i32),
    pub server_view_distance: Option<i32>,
    pub simulation_distance: Option<i32>,
    pub tick_rate: f32,
    pub frozen: bool,
    /// Ticks still to run while frozen.
    pub frozen_steps: i32,
    /// `TickRateManager.runGameElements`: as of the last
    /// [`Self::tick_rate_tick`], the level runs (not frozen, or stepping).
    runs_normally: bool,
}

impl Default for LevelSettings {
    fn default() -> Self {
        Self {
            difficulty: 2,
            difficulty_locked: false,
            spawn: None,
            chunk_center: (0, 0),
            server_view_distance: None,
            simulation_distance: None,
            tick_rate: 20.0,
            frozen: false,
            frozen_steps: 0,
            runs_normally: true,
        }
    }
}

/// `SharedConstants.TICKS_PER_SECOND`: the client's own pace.
const CLIENT_TICKS_PER_SECOND: f32 = 20.0;

impl LevelSettings {
    /// `TickRateManager.tick`, at the start of each client tick: whether
    /// this tick runs the level, and one step used up while frozen.
    pub fn tick_rate_tick(&mut self) {
        self.runs_normally = !self.frozen || self.frozen_steps > 0;
        if self.frozen_steps > 0 {
            self.frozen_steps -= 1;
        }
    }

    /// `TickRateManager.runsNormally`.
    pub fn runs_normally(&self) -> bool {
        self.runs_normally
    }

    /// `Minecraft.getTickTargetMillis`, in seconds: a slower server slows
    /// the client's ticks down with it, a faster one does not speed them
    /// up, and a frozen level ticks at the client's own pace.
    pub fn tick_seconds(&self) -> f32 {
        let own = 1.0 / CLIENT_TICKS_PER_SECOND;
        if self.runs_normally {
            own.max(1.0 / self.tick_rate)
        } else {
            own
        }
    }

    /// A new `ClientLevel` comes with a new `TickRateManager`: 20 a second,
    /// running, until the server says otherwise.
    pub fn new_level(&mut self) {
        let fresh = Self::default();
        self.tick_rate = fresh.tick_rate;
        self.frozen = fresh.frozen;
        self.frozen_steps = fresh.frozen_steps;
        self.runs_normally = fresh.runs_normally;
    }

    pub fn apply(&mut self, setting: LevelSetting) {
        match setting {
            LevelSetting::Difficulty { difficulty, locked } => {
                self.difficulty = difficulty;
                self.difficulty_locked = locked;
            }
            LevelSetting::DefaultSpawn(spawn) => self.spawn = Some(spawn),
            LevelSetting::ChunkCenter { x, z } => self.chunk_center = (x, z),
            LevelSetting::ChunkRadius(radius) => self.server_view_distance = Some(radius),
            LevelSetting::SimulationDistance(distance) => {
                self.simulation_distance = Some(distance);
            }
            LevelSetting::TickingState { rate, frozen } => {
                // `TickRateManager.setTickRate`: never below one a second.
                self.tick_rate = rate.max(1.0);
                self.frozen = frozen;
            }
            LevelSetting::TickingStep(steps) => self.frozen_steps = steps.max(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_ticks_slower_with_the_server_never_faster() {
        let mut settings = LevelSettings::default();
        settings.tick_rate_tick();
        assert_eq!(settings.tick_seconds(), 0.05);
        settings.apply(LevelSetting::TickingState {
            rate: 10.0,
            frozen: false,
        });
        assert_eq!(settings.tick_seconds(), 0.1);
        settings.apply(LevelSetting::TickingState {
            rate: 100.0,
            frozen: false,
        });
        assert_eq!(settings.tick_seconds(), 0.05);
        settings.apply(LevelSetting::TickingState {
            rate: 0.1,
            frozen: false,
        });
        assert_eq!(settings.tick_seconds(), 1.0);
    }

    #[test]
    fn a_frozen_level_runs_only_its_steps_at_the_client_pace() {
        let mut settings = LevelSettings::default();
        settings.apply(LevelSetting::TickingState {
            rate: 5.0,
            frozen: true,
        });
        settings.tick_rate_tick();
        assert!(!settings.runs_normally());
        assert_eq!(settings.tick_seconds(), 0.05);
        settings.apply(LevelSetting::TickingStep(2));
        let ran: Vec<bool> = (0..3)
            .map(|_| {
                settings.tick_rate_tick();
                settings.runs_normally()
            })
            .collect();
        assert_eq!(ran, [true, true, false]);
        settings.new_level();
        settings.tick_rate_tick();
        assert!(settings.runs_normally());
        assert_eq!(settings.tick_rate, 20.0);
    }

    #[test]
    fn each_packet_sets_its_part() {
        let mut settings = LevelSettings::default();
        settings.apply(LevelSetting::TickingState {
            rate: 5.0,
            frozen: true,
        });
        settings.apply(LevelSetting::Difficulty {
            difficulty: 3,
            locked: true,
        });
        settings.apply(LevelSetting::ChunkCenter { x: -2, z: 7 });
        assert_eq!((settings.tick_rate, settings.frozen), (5.0, true));
        assert_eq!((settings.difficulty, settings.difficulty_locked), (3, true));
        assert_eq!(settings.chunk_center, (-2, 7));
    }
}
