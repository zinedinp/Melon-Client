//! The options that pick one of a few values, each with the vanilla name it has in
//! `options.txt`.

use super::*;

/// `InactivityFpsLimit`: when the framerate drops while nobody plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InactivityFpsLimit {
    /// Only while the window is minimized.
    Minimized,
    /// Also after a minute without input (30 fps), and ten (10 fps).
    #[default]
    Afk,
}

impl InactivityFpsLimit {
    /// The `options.txt` value.
    pub fn id(self) -> &'static str {
        match self {
            Self::Minimized => "minimized",
            Self::Afk => "afk",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Minimized => "options.inactivityFpsLimit.minimized",
            Self::Afk => "options.inactivityFpsLimit.afk",
        }
    }
}

impl Cycle for InactivityFpsLimit {
    const ALL: &'static [Self] = &[Self::Minimized, Self::Afk];
}

/// How finished frames reach the display (Melon's own option). `Auto`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PresentMode {
    /// FIFO with VSync on, else Immediate, else Mailbox.
    #[default]
    Auto,
    /// Waits for vblank, never tears, a queue of frames adds latency.
    Fifo,
    /// Vblank-synced, never tears, the newest finished frame wins.
    Mailbox,
    /// Presents at once: lowest latency, may tear.
    Immediate,
    /// Like FIFO, but at vblank shows the newest finished frame instead of the oldest.
    FifoLatestReady,
}

impl PresentMode {
    /// The `options.txt` value.
    pub fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fifo => "fifo",
            Self::Mailbox => "mailbox",
            Self::Immediate => "immediate",
            Self::FifoLatestReady => "fifo_latest_ready",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|mode| mode.id() == id)
            .unwrap_or_default()
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "melon.options.presentMode.auto",
            Self::Fifo => "melon.options.presentMode.fifo",
            Self::Mailbox => "melon.options.presentMode.mailbox",
            Self::Immediate => "melon.options.presentMode.immediate",
            Self::FifoLatestReady => "melon.options.presentMode.fifoLatestReady",
        }
    }

    pub fn tooltip_key(self) -> &'static str {
        match self {
            Self::Auto => "melon.options.presentMode.auto.tooltip",
            Self::Fifo => "melon.options.presentMode.fifo.tooltip",
            Self::Mailbox => "melon.options.presentMode.mailbox.tooltip",
            Self::Immediate => "melon.options.presentMode.immediate.tooltip",
            Self::FifoLatestReady => "melon.options.presentMode.fifoLatestReady.tooltip",
        }
    }
}

impl Cycle for PresentMode {
    const ALL: &'static [Self] = &[
        Self::Auto,
        Self::Fifo,
        Self::Mailbox,
        Self::Immediate,
        Self::FifoLatestReady,
    ];
}

/// The speakers sounds are rendered for (Melon's own option; vanilla
/// leaves this to OpenAL Soft's `alsoft.conf`). `Auto` opens the device
/// with the channels it prefers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeakerLayout {
    #[default]
    Auto,
    Mono,
    Stereo,
    Quad,
    Surround51,
    Surround71,
}

impl SpeakerLayout {
    /// The `options.txt` value.
    pub fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mono => "mono",
            Self::Stereo => "stereo",
            Self::Quad => "quad",
            Self::Surround51 => "surround51",
            Self::Surround71 => "surround71",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|layout| layout.id() == id)
            .unwrap_or_default()
    }

    /// The output channels to ask the device for; `None` for its own.
    pub fn channels(self) -> Option<u16> {
        match self {
            Self::Auto => None,
            Self::Mono => Some(1),
            Self::Stereo => Some(2),
            Self::Quad => Some(4),
            Self::Surround51 => Some(6),
            Self::Surround71 => Some(8),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "melon.options.speakerLayout.auto",
            Self::Mono => "melon.options.speakerLayout.mono",
            Self::Stereo => "melon.options.speakerLayout.stereo",
            Self::Quad => "melon.options.speakerLayout.quad",
            Self::Surround51 => "melon.options.speakerLayout.surround51",
            Self::Surround71 => "melon.options.speakerLayout.surround71",
        }
    }
}

impl Cycle for SpeakerLayout {
    const ALL: &'static [Self] = &[
        Self::Auto,
        Self::Mono,
        Self::Stereo,
        Self::Quad,
        Self::Surround51,
        Self::Surround71,
    ];
}

/// `PrioritizeChunkUpdates`, the "Chunk Builder".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChunkBuilder {
    /// Threaded: never waited for.
    #[default]
    None = 0,
    /// Semi Blocking: the player's own edits.
    PlayerAffected = 1,
    /// Fully Blocking: also every change near the camera.
    Nearby = 2,
}

impl ChunkBuilder {
    /// `LEGACY_CODEC`'s `ByIdMap.continuous(..., WRAP)`.
    pub fn from_id(id: i64) -> Self {
        match id.rem_euclid(3) {
            1 => Self::PlayerAffected,
            2 => Self::Nearby,
            _ => Self::None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::None => "options.prioritizeChunkUpdates.none",
            Self::PlayerAffected => "options.prioritizeChunkUpdates.byPlayer",
            Self::Nearby => "options.prioritizeChunkUpdates.nearby",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::None => "options.prioritizeChunkUpdates.none.tooltip",
            Self::PlayerAffected => "options.prioritizeChunkUpdates.byPlayer.tooltip",
            Self::Nearby => "options.prioritizeChunkUpdates.nearby.tooltip",
        }
    }
}

impl Cycle for ChunkBuilder {
    const ALL: &'static [Self] = &[Self::None, Self::PlayerAffected, Self::Nearby];
}

/// `GraphicsPreset`: sets the "Quality & Performance" options together;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsPreset {
    Fast,
    #[default]
    Fancy,
    Fabulous,
    Custom,
}

impl GraphicsPreset {
    /// In slider order.
    pub const ALL: [Self; 4] = [Self::Fast, Self::Fancy, Self::Fabulous, Self::Custom];

    /// The `options.txt` value.
    pub fn id(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Fancy => "fancy",
            Self::Fabulous => "fabulous",
            Self::Custom => "custom",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Fast => "options.graphics.fast",
            Self::Fancy => "options.graphics.fancy",
            Self::Fabulous => "options.graphics.fabulous",
            Self::Custom => "options.graphics.custom",
        }
    }

    pub(super) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// `GraphicsPreset.apply`, for the options Melon has.
    pub fn apply(self, config: &mut Config) {
        let (render, builder, simulation, smooth, entities, blend, clouds, cloud_range, weather) =
            match self {
                Self::Fast => (
                    8,
                    ChunkBuilder::None,
                    6,
                    false,
                    0.75,
                    1,
                    CloudStatus::Fast,
                    32,
                    5,
                ),
                Self::Fancy => (
                    16,
                    ChunkBuilder::PlayerAffected,
                    12,
                    true,
                    1.0,
                    2,
                    CloudStatus::Fancy,
                    64,
                    10,
                ),
                Self::Fabulous => (
                    32,
                    ChunkBuilder::PlayerAffected,
                    12,
                    true,
                    1.25,
                    2,
                    CloudStatus::Fancy,
                    128,
                    10,
                ),
                Self::Custom => return,
            };
        config.biome_blend = blend;
        config.render_distance = render;
        config.chunk_builder = builder;
        config.simulation_distance = simulation;
        config.smooth_lighting = smooth;
        config.cutout_leaves = self != Self::Fast;
        config.cull.entity_distance = entities;
        config.clouds = clouds;
        config.cloud_range = cloud_range;
        config.weather_radius = weather;
        config.particles = if self == Self::Fast {
            ParticleStatus::Decreased
        } else {
            ParticleStatus::All
        };
    }
}

/// `AttackIndicatorStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttackIndicator {
    Off,
    #[default]
    Crosshair,
    Hotbar,
}

impl AttackIndicator {
    /// `BY_ID` with `OutOfBoundsStrategy.WRAP`.
    pub fn from_id(id: i64) -> Self {
        Self::ALL[id.rem_euclid(Self::ALL.len() as i64) as usize]
    }

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn caption_key(self) -> &'static str {
        match self {
            Self::Off => "options.off",
            Self::Crosshair => "options.attack.crosshair",
            Self::Hotbar => "options.attack.hotbar",
        }
    }
}

impl Cycle for AttackIndicator {
    const ALL: &'static [Self] = &[Self::Off, Self::Crosshair, Self::Hotbar];
}

/// `ParticleStatus`: how many particles show, by its `options.txt` id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParticleStatus {
    #[default]
    All,
    Decreased,
    Minimal,
}

impl ParticleStatus {
    /// `BY_ID` with `OutOfBoundsStrategy.WRAP`.
    pub fn from_id(id: i64) -> Self {
        Self::ALL[id.rem_euclid(Self::ALL.len() as i64) as usize]
    }

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn caption_key(self) -> &'static str {
        match self {
            Self::All => "options.particles.all",
            Self::Decreased => "options.particles.decreased",
            Self::Minimal => "options.particles.minimal",
        }
    }
}

impl Cycle for ParticleStatus {
    const ALL: &'static [Self] = &[Self::All, Self::Decreased, Self::Minimal];
}

/// `ChatVisiblity`, by its `options.txt` id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatVisibility {
    #[default]
    Full = 0,
    /// Commands only: server and client messages, no player chat.
    System = 1,
    Hidden = 2,
}

impl ChatVisibility {
    pub fn from_id(id: i64) -> Self {
        match id {
            1 => Self::System,
            2 => Self::Hidden,
            _ => Self::Full,
        }
    }

    /// `ChatVisiblity.getKey`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Full => "options.chat.visibility.full",
            Self::System => "options.chat.visibility.system",
            Self::Hidden => "options.chat.visibility.hidden",
        }
    }
}

impl Cycle for ChatVisibility {
    const ALL: &'static [Self] = &[Self::Full, Self::System, Self::Hidden];
}
