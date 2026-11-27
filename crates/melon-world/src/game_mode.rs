//! Vanilla `GameType` and `Abilities`.

/// `GameType`, by its network id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GameMode {
    #[default]
    Survival = 0,
    Creative = 1,
    Adventure = 2,
    Spectator = 3,
}

impl GameMode {
    pub const ALL: [GameMode; 4] = [
        GameMode::Survival,
        GameMode::Creative,
        GameMode::Adventure,
        GameMode::Spectator,
    ];

    /// `GameType.byId`: unknown ids are survival.
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Survival,
        }
    }

    /// `GameType.OPTIONAL_STREAM_CODEC`'s `-1` is "none".
    pub fn from_optional_id(id: i32) -> Option<Self> {
        (0..=3).contains(&id).then(|| Self::from_id(id))
    }

    pub fn id(self) -> u8 {
        self as u8
    }

    /// `gameMode.<name>`, the translated name.
    pub fn translation_key(self) -> &'static str {
        match self {
            Self::Survival => "gameMode.survival",
            Self::Creative => "gameMode.creative",
            Self::Adventure => "gameMode.adventure",
            Self::Spectator => "gameMode.spectator",
        }
    }

    /// `MultiPlayerGameMode.canHurtPlayer`.
    pub fn can_hurt(self) -> bool {
        matches!(self, Self::Survival | Self::Adventure)
    }

    /// `Player.hasInfiniteMaterials`: creative's item picker and clone.
    pub fn has_infinite_materials(self) -> bool {
        self == Self::Creative
    }

    pub fn is_spectator(self) -> bool {
        self == Self::Spectator
    }

    /// `GameType.updatePlayerAbilities`.
    pub fn abilities(self, previous: Abilities) -> Abilities {
        let (may_fly, instabuild, invulnerable) = match self {
            Self::Creative => (true, true, true),
            Self::Spectator => (true, false, true),
            Self::Survival | Self::Adventure => (false, false, false),
        };
        Abilities {
            may_fly,
            instabuild,
            invulnerable,
            flying: match self {
                Self::Creative => previous.flying,
                Self::Spectator => true,
                Self::Survival | Self::Adventure => false,
            },
            may_build: self != Self::Adventure && self != Self::Spectator,
            ..previous
        }
    }
}

/// `Abilities`: the flags and speeds `ClientboundPlayerAbilitiesPacket`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub may_fly: bool,
    /// Creative's instant breaking and infinite blocks.
    pub instabuild: bool,
    pub may_build: bool,
    /// Blocks per tick of acceleration while flying (0.05 by default).
    pub flying_speed: f32,
    /// The movement speed attribute's base (0.1 by default).
    pub walking_speed: f32,
}

impl Default for Abilities {
    fn default() -> Self {
        Self {
            invulnerable: false,
            flying: false,
            may_fly: false,
            instabuild: false,
            may_build: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        }
    }
}

impl Abilities {
    /// `ClientboundPlayerAbilitiesPacket`'s flag bits.
    pub const INVULNERABLE: u8 = 0x01;
    pub const FLYING: u8 = 0x02;
    pub const MAY_FLY: u8 = 0x04;
    pub const INSTABUILD: u8 = 0x08;

    /// The abilities a packet sets; `may_build` is not sent and stays.
    pub fn from_packet(&self, flags: u8, flying_speed: f32, walking_speed: f32) -> Self {
        Self {
            invulnerable: flags & Self::INVULNERABLE != 0,
            flying: flags & Self::FLYING != 0,
            may_fly: flags & Self::MAY_FLY != 0,
            instabuild: flags & Self::INSTABUILD != 0,
            may_build: self.may_build,
            flying_speed,
            walking_speed,
        }
    }

    /// `ServerboundPlayerAbilitiesPacket`: only the flying bit is read.
    pub fn packet_flags(&self) -> u8 {
        if self.flying { Self::FLYING } else { 0 }
    }
}

/// `MultiPlayerGameMode`'s local mode.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalGameMode {
    pub current: GameMode,
    pub previous: Option<GameMode>,
}

impl LocalGameMode {
    /// `MultiPlayerGameMode.setLocalMode(GameType)`.
    pub fn set(&mut self, mode: GameMode) {
        if mode != self.current {
            self.previous = Some(self.current);
        }
        self.current = mode;
    }

    /// `setLocalMode(GameType, GameType)`, from a login or respawn.
    pub fn set_both(&mut self, mode: GameMode, previous: Option<GameMode>) {
        self.current = mode;
        self.previous = previous;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_mode_remembers_the_previous_one() {
        let mut mode = LocalGameMode::default();
        mode.set(GameMode::Creative);
        mode.set(GameMode::Creative);
        assert_eq!(mode.previous, Some(GameMode::Survival));
        mode.set(GameMode::Spectator);
        assert_eq!(mode.previous, Some(GameMode::Creative));
    }

    #[test]
    fn ability_flags_round_trip() {
        let abilities = Abilities::default().from_packet(0x0F, 0.05, 0.1);
        assert!(abilities.flying && abilities.may_fly && abilities.instabuild);
        assert_eq!(abilities.packet_flags(), Abilities::FLYING);
    }
}
