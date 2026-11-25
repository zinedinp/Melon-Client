//! What changes over a particle's life without moving it.

use crate::math::angle::PI;

/// `getQuadSize(partial)`, by class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SizeCurve {
    /// `SingleQuadParticle`: the size as set.
    #[default]
    Fixed,
    /// Grows to full size in the first 32nd of its life (smoke, dust, ...).
    GrowIn,
    /// `FlameParticle`: shrinks to half by the end.
    Flame,
    /// `LavaParticle`: shrinks to nothing.
    Lava,
    /// `PortalParticle`: grows in fast, then holds.
    Portal,
    /// `ReversePortalParticle`: shrinks by two thirds.
    ReversePortal,
    /// `BlockMarker`: half a block.
    Marker,
    /// `FireworkParticles.OverlayParticle`.
    Flash,
    /// `ShriekParticle`: grows to full size by three quarters of its life.
    Shriek,
}

impl SizeCurve {
    /// The factor on `quad_size` at `age + partial` of `lifetime`.
    pub fn factor(self, age: i32, lifetime: i32, partial: f32) -> f32 {
        let life = lifetime.max(1) as f32;
        let s = (age as f32 + partial) / life;
        match self {
            Self::Fixed => 1.0,
            Self::GrowIn => (s * 32.0).clamp(0.0, 1.0),
            Self::Flame => 1.0 - s * s * 0.5,
            Self::Lava => 1.0 - s * s,
            Self::Portal => {
                let rest = 1.0 - s;
                1.0 - rest * rest
            }
            Self::ReversePortal => 1.0 - (age as f32 + partial) / (life * 1.5),
            Self::Marker => 1.0,
            Self::Flash => 7.1 * crate::math::sin((age as f32 + partial - 1.0) * 0.25 * PI),
            Self::Shriek => (s * 0.75).clamp(0.0, 1.0),
        }
    }
}

/// `getLightCoords`, by class: block and sky light in `0..=240`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Glow {
    /// The light where the particle is.
    #[default]
    World,
    /// `15728880`: full block and sky light (explosions, sweeps).
    Full,
    /// Block light raised to 15 (`LightCoordsUtil.withBlock(.., 15)`).
    Block,
    /// `addSmoothBlockEmission` by `age / lifetime` (flames, glow).
    Emission,
    /// `addSmoothBlockEmission` by `(age / lifetime)^4` (portals).
    EmissionQuartic,
    /// `FireflyParticle`: its own block light fading in and out, no sky.
    Firefly,
}

impl Glow {
    /// The `[block, sky]` light of a particle in `world` light.
    pub fn light(self, world: [u8; 2], age: i32, lifetime: i32, partial: f32) -> [u8; 2] {
        let s = ((age as f32 + partial) / lifetime.max(1) as f32).clamp(0.0, 1.0);
        let emit = |amount: f32| {
            let block = u16::from(world[0]) + (amount * 240.0) as u16;
            [block.min(240) as u8, world[1]]
        };
        match self {
            Self::World => world,
            Self::Full => [240, 240],
            Self::Block => [240, world[1]],
            Self::Emission => emit(s),
            Self::EmissionQuartic => {
                let b = (age as f32 / lifetime.max(1) as f32).clamp(0.0, 1.0);
                emit(b * b * b * b)
            }
            Self::Firefly => [(255.0 * firefly_fade(s, 0.1, 0.3)).min(240.0) as u8, 0],
        }
    }
}

/// `FireflyParticle.getFadeAmount`.
pub fn firefly_fade(progress: f32, fall: f32, rise: f32) -> f32 {
    if progress >= 1.0 - fall {
        (1.0 - progress) / fall
    } else if progress <= rise {
        progress / rise
    } else {
        1.0
    }
}

/// `Particle.LifetimeAlpha`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifetimeAlpha {
    pub start: f32,
    pub end: f32,
    pub from: f32,
    pub to: f32,
}

impl LifetimeAlpha {
    pub const OPAQUE: Self = Self {
        start: 1.0,
        end: 1.0,
        from: 0.0,
        to: 1.0,
    };

    pub fn is_opaque(self) -> bool {
        self.start >= 1.0 && self.end >= 1.0
    }

    /// `currentAlphaForAge`.
    pub fn at(self, age: i32, lifetime: i32, partial: f32) -> f32 {
        if (self.start - self.end).abs() < 1.0e-5 {
            return self.start;
        }
        let s = (age as f32 + partial) / lifetime.max(1) as f32;
        let t = if (self.to - self.from).abs() < 1.0e-6 {
            0.0
        } else {
            (s - self.from) / (self.to - self.from)
        };
        let t = t.clamp(0.0, 1.0);
        self.start + (self.end - self.start) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_match_their_classes() {
        assert_eq!(SizeCurve::GrowIn.factor(0, 64, 0.0), 0.0);
        assert_eq!(SizeCurve::GrowIn.factor(2, 64, 0.0), 1.0);
        assert_eq!(SizeCurve::Flame.factor(10, 10, 0.0), 0.5);
        assert_eq!(SizeCurve::Lava.factor(10, 10, 0.0), 0.0);
        assert_eq!(Glow::Full.light([0, 0], 0, 1, 0.0), [240, 240]);
        assert_eq!(Glow::Emission.light([0, 96], 5, 10, 0.0), [120, 96]);
        assert_eq!(Glow::Emission.light([200, 0], 10, 10, 0.0), [240, 0]);
        let fade = LifetimeAlpha {
            start: 0.0,
            end: 0.6,
            from: 0.25,
            to: 1.0,
        };
        assert_eq!(fade.at(0, 100, 0.0), 0.0);
        assert!((fade.at(100, 100, 0.0) - 0.6).abs() < 1e-6);
        assert!(!fade.is_opaque() && LifetimeAlpha::OPAQUE.is_opaque());
    }
}
