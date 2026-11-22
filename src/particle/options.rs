//! `ParticleOptions`.

use super::Kind;

/// Where a vibration goes (`PositionSource`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionSource {
    Block([i32; 3]),
    Entity { id: i32, y_offset: f32 },
}

/// The data after the type id, by the codec its type uses.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ParticleData {
    /// `SimpleParticleType`: nothing.
    #[default]
    None,
    /// `BlockParticleOption`: the block state id.
    Block(u16),
    /// `ItemParticleOption`: the item's registry id.
    Item(u16),
    /// `DustParticleOptions`: RGB and a scale clamped to `0.01..=4`.
    Dust { rgb: u32, scale: f32 },
    /// `DustColorTransitionOptions`.
    DustTransition { from: u32, to: u32, scale: f32 },
    /// `ColorParticleOption`: ARGB.
    Color(u32),
    /// `SpellParticleOption`: RGB and how fast it flies off.
    Spell { rgb: u32, power: f32 },
    /// `PowerParticleOption`.
    Power(f32),
    /// `SculkChargeParticleOptions`: the roll, in radians.
    SculkCharge(f32),
    /// `ShriekParticleOption`: ticks before it shows.
    Shriek(i32),
    /// `VibrationParticleOption`: where to and in how many ticks.
    Vibration { to: PositionSource, ticks: i32 },
    /// `TrailParticleOption`: where it goes, RGB, in how many ticks.
    Trail {
        target: [f64; 3],
        rgb: u32,
        ticks: i32,
    },
    /// `GeyserParticleOptions`.
    Geyser { water_blocks: i32 },
    /// `GeyserBaseParticleOptions`.
    GeyserBase { water_blocks: i32, impulse: f32 },
}

/// A particle type and its data: what `addParticle` is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSpec {
    pub kind: Kind,
    pub data: ParticleData,
}

impl ParticleSpec {
    /// A type without data.
    pub const fn simple(kind: Kind) -> Self {
        Self {
            kind,
            data: ParticleData::None,
        }
    }

    pub const fn block(kind: Kind, state: u16) -> Self {
        Self {
            kind,
            data: ParticleData::Block(state),
        }
    }

    pub const fn item(item: u16) -> Self {
        Self {
            kind: Kind::Item,
            data: ParticleData::Item(item),
        }
    }

    /// `DustParticleOptions.REDSTONE`: red, full size.
    pub const REDSTONE: Self = Self {
        kind: Kind::Dust,
        data: ParticleData::Dust {
            rgb: 0xFF0000,
            scale: 1.0,
        },
    };
}

/// Which codec a type reads after its id.
pub fn codec(kind: Kind) -> Codec {
    use Kind as P;
    match kind {
        P::Block | P::BlockMarker | P::FallingDust | P::DustPillar | P::BlockCrumble => {
            Codec::Block
        }
        P::Item => Codec::Item,
        P::Dust => Codec::Dust,
        P::DustColorTransition => Codec::DustTransition,
        P::EntityEffect | P::TintedLeaves | P::Flash => Codec::Color,
        P::Effect | P::InstantEffect => Codec::Spell,
        P::DragonBreath => Codec::Power,
        P::SculkCharge => Codec::SculkCharge,
        P::Shriek => Codec::Shriek,
        P::Vibration => Codec::Vibration,
        P::Trail => Codec::Trail,
        P::Geyser | P::GeyserPlume => Codec::Geyser,
        P::GeyserBase | P::GeyserPoof => Codec::GeyserBase,
        _ => Codec::None,
    }
}

/// The data codecs (`ParticleTypes`' `StreamCodec`s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    None,
    Block,
    Item,
    Dust,
    DustTransition,
    Color,
    Spell,
    Power,
    SculkCharge,
    Shriek,
    Vibration,
    Trail,
    Geyser,
    GeyserBase,
}

/// Whether `kind`'s codec carries data.
pub fn has_data(kind: Kind) -> bool {
    codec(kind) != Codec::None
}
