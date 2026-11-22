//! `WorldCreationUiState`.

use super::{Difficulty, WorldCreationMode, WorldType};

/// `SelectedGameMode` without Debug (the world type decides that).
pub const GAME_MODES: [WorldCreationMode; 3] = [
    WorldCreationMode::Survival,
    WorldCreationMode::Hardcore,
    WorldCreationMode::Creative,
];

#[derive(Clone, Debug)]
pub struct CreationState {
    pub name: String,
    pub seed: String,
    pub world_type: WorldType,
    game_mode: WorldCreationMode,
    difficulty: Difficulty,
    /// `allowCommands`.
    allow_commands: Option<bool>,
    generate_structures: bool,
    bonus_chest: bool,
}

impl CreationState {
    pub fn new(name: String) -> Self {
        Self {
            name,
            seed: String::new(),
            world_type: WorldType::Default,
            game_mode: WorldCreationMode::Survival,
            difficulty: Difficulty::Normal,
            allow_commands: None,
            generate_structures: true,
            bonus_chest: false,
        }
    }

    /// `isDebug`.
    pub fn is_debug(&self) -> bool {
        self.world_type == WorldType::Debug
    }

    /// The mode the button shows; `None` for the Debug world's Spectator.
    pub fn game_mode(&self) -> Option<WorldCreationMode> {
        (!self.is_debug()).then_some(self.game_mode)
    }

    /// The mode the world is made with (a Debug world is Spectator, which starts as Creative).
    pub fn world_game_mode(&self) -> WorldCreationMode {
        if self.is_debug() {
            WorldCreationMode::Creative
        } else {
            self.game_mode
        }
    }

    pub fn set_game_mode(&mut self, mode: WorldCreationMode) {
        self.game_mode = mode;
    }

    /// `isHardcore`.
    pub fn is_hardcore(&self) -> bool {
        self.game_mode() == Some(WorldCreationMode::Hardcore)
    }

    /// `getDifficulty`: Hard in Hardcore.
    pub fn difficulty(&self) -> Difficulty {
        if self.is_hardcore() {
            Difficulty::Hard
        } else {
            self.difficulty
        }
    }

    pub fn set_difficulty(&mut self, difficulty: Difficulty) {
        self.difficulty = difficulty;
    }

    /// `isAllowCommands`.
    pub fn allow_commands(&self) -> bool {
        if self.is_debug() {
            true
        } else if self.is_hardcore() {
            false
        } else {
            self.allow_commands
                .unwrap_or(self.game_mode == WorldCreationMode::Creative)
        }
    }

    pub fn set_allow_commands(&mut self, allow: bool) {
        self.allow_commands = Some(allow);
    }

    /// `isGenerateStructures`.
    pub fn generate_structures(&self) -> bool {
        !self.is_debug() && self.generate_structures
    }

    pub fn set_generate_structures(&mut self, on: bool) {
        self.generate_structures = on;
    }

    /// `isBonusChest`.
    pub fn bonus_chest(&self) -> bool {
        !self.is_debug() && !self.is_hardcore() && self.bonus_chest
    }

    pub fn set_bonus_chest(&mut self, on: bool) {
        self.bonus_chest = on;
    }

    pub fn game_mode_active(&self) -> bool {
        !self.is_debug()
    }

    pub fn difficulty_active(&self) -> bool {
        !self.is_hardcore()
    }

    pub fn allow_commands_active(&self) -> bool {
        !self.is_debug() && !self.is_hardcore()
    }

    pub fn structures_active(&self) -> bool {
        !self.is_debug()
    }

    pub fn bonus_chest_active(&self) -> bool {
        !self.is_hardcore() && !self.is_debug()
    }

    pub fn customize_active(&self) -> bool {
        !self.is_debug() && self.world_type.has_editor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> CreationState {
        CreationState::new("New World".to_owned())
    }

    #[test]
    fn creative_turns_cheats_on_until_they_are_set_by_hand() {
        let mut s = state();
        assert!(!s.allow_commands());
        s.set_game_mode(WorldCreationMode::Creative);
        assert!(s.allow_commands());
        s.set_allow_commands(false);
        assert!(!s.allow_commands());
        s.set_game_mode(WorldCreationMode::Survival);
        assert!(!s.allow_commands());
        s.set_allow_commands(true);
        assert!(s.allow_commands());
    }

    #[test]
    fn hardcore_is_hard_without_cheats_or_a_bonus_chest() {
        let mut s = state();
        s.set_difficulty(Difficulty::Easy);
        s.set_allow_commands(true);
        s.set_bonus_chest(true);
        s.set_game_mode(WorldCreationMode::Hardcore);
        assert_eq!(s.difficulty(), Difficulty::Hard);
        assert!(!s.difficulty_active());
        assert!(!s.allow_commands() && !s.allow_commands_active());
        assert!(!s.bonus_chest() && !s.bonus_chest_active());
        s.set_game_mode(WorldCreationMode::Survival);
        assert_eq!(s.difficulty(), Difficulty::Easy);
        assert!(s.allow_commands() && s.bonus_chest());
    }

    #[test]
    fn the_debug_world_takes_the_mode_and_switches_the_rest_off() {
        let mut s = state();
        s.world_type = WorldType::Debug;
        assert_eq!(s.game_mode(), None);
        assert!(!s.game_mode_active());
        assert!(s.allow_commands() && !s.allow_commands_active());
        assert!(!s.generate_structures() && !s.structures_active());
        assert!(!s.bonus_chest_active() && !s.customize_active());
    }
}
