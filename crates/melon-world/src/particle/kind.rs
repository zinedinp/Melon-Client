//! The particle types (`ParticleTypes`), by network id.

pub use pumpkin_data::particle::Particle as Kind;

/// How many types there are; ids run `0..COUNT`.
pub const COUNT: u16 = 128;

/// Every type, by id.
pub fn all() -> impl Iterator<Item = Kind> {
    (0..COUNT).filter_map(Kind::from_id)
}

/// `ParticleType.getOverrideLimiter`.
pub fn overrides_limiter(kind: Kind) -> bool {
    use Kind as P;
    matches!(
        kind,
        P::BlockMarker
            | P::Geyser
            | P::GeyserBase
            | P::GeyserPoof
            | P::GeyserPlume
            | P::DamageIndicator
            | P::ElderGuardian
            | P::ExplosionEmitter
            | P::Explosion
            | P::Gust
            | P::GustEmitterLarge
            | P::GustEmitterSmall
            | P::SonicBoom
            | P::SculkCharge
            | P::SculkChargePop
            | P::Vibration
            | P::Poof
            | P::Spit
            | P::SquidInk
            | P::SweepAttack
            | P::CampfireCosySmoke
            | P::CampfireSignalSmoke
            | P::GlowSquidInk
            | P::Glow
            | P::WaxOn
            | P::WaxOff
            | P::ElectricSpark
            | P::Scrape
            | P::TrialSpawnerDetection
            | P::TrialSpawnerDetectionOminous
            | P::VaultConnection
            | P::OminousSpawning
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cover_every_type() {
        assert_eq!(all().count(), usize::from(COUNT));
        assert_eq!(Kind::from_id(COUNT), None);
        assert!(overrides_limiter(Kind::Explosion));
        assert!(!overrides_limiter(Kind::Flame));
    }
}
