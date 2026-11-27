//! `ParticleResources.registerProviders`: what each particle type becomes when
//! it is spawned.
//!
//! [`provider`] is the table: each type's provider, one function each, in
//! a file per family. The `match` is exhaustive, so a new [`Kind`] does not
//! compile until it has one.

mod block;
mod liquid;
mod living;
mod magic;
mod nature;
mod smoke;

use super::Kind as P;
use super::animation::Rect;
use super::animation::fade::{Glow, LifetimeAlpha, SizeCurve};
use super::base::{Atlas, Particle};
use super::behavior::{Behavior, Drip, DripStage, Leaf, Plume};
use super::motion::{self, emitter::Emitter};
use super::options::{ParticleData, ParticleSpec};
use super::{Kind, TickCtx};
use crate::fluid::FluidKind;
use crate::java_random::JavaRandom;
use glam::DVec3;

/// What `ParticleProvider.createParticle` is handed: the options, where,
/// the packet's or caller's `aux` (speed or offset), and the level.
#[derive(Clone, Copy)]
struct Cx<'a> {
    spec: &'a ParticleSpec,
    kind: Kind,
    pos: DVec3,
    aux: DVec3,
    ctx: &'a TickCtx<'a>,
}

impl Cx<'_> {
    /// `SpriteSet.get(random)`: one of the type's sprites, at random.
    fn sprite(&self, rng: &mut JavaRandom) -> Rect {
        self.ctx.sprites.random(self.kind, rng.next_float())
    }
}

/// One type's provider.
type Provider = fn(&Cx, &mut JavaRandom) -> Option<Particle>;

/// A new particle of `spec` at `pos`; `aux` is the packet's or caller's
/// speed or offset. `None` when the type has no provider or its options do
/// not fit.
pub fn create(
    spec: &ParticleSpec,
    pos: DVec3,
    aux: DVec3,
    ctx: &TickCtx,
    rng: &mut JavaRandom,
) -> Option<Particle> {
    let kind = spec.kind;
    let cx = Cx {
        spec,
        kind,
        pos,
        aux,
        ctx,
    };
    let mut p = provider(kind)?(&cx, rng)?;
    if p.look.animated {
        p.look.uv = ctx.sprites.from_age(kind, p.age, p.lifetime);
    }
    Some(p)
}

/// `ParticleResources.registerProviders`: each type's provider. The
/// elder guardian's is a model, not a particle (`MobAppearanceParticle`).
fn provider(kind: Kind) -> Option<Provider> {
    Some(match kind {
        P::Heart => living::heart,
        P::AngryVillager => living::angry_villager,
        P::Block | P::BlockCrumble | P::DustPillar => block::block,
        P::BlockMarker => block::block_marker,
        P::BubbleColumnUp => liquid::bubble_column_up,
        P::CurrentDown => liquid::current_down,
        P::Bubble => liquid::bubble,
        P::BubblePop => liquid::bubble_pop,
        P::CampfireCosySmoke | P::CampfireSignalSmoke => smoke::campfire_smoke,
        P::Cloud | P::Sneeze => smoke::cloud,
        P::Composter | P::Dolphin | P::EggCrack | P::HappyVillager | P::Mycelium => {
            living::composter
        }
        P::Flame | P::SoulFireFlame | P::CopperFireFlame | P::SmallFlame => smoke::flame,
        P::Crit | P::DamageIndicator | P::EnchantedHit => living::crit,
        P::DragonBreath => smoke::dragon_breath,
        P::DrippingLava
        | P::FallingLava
        | P::LandingLava
        | P::DrippingWater
        | P::FallingWater
        | P::DrippingHoney
        | P::FallingHoney
        | P::LandingHoney
        | P::FallingNectar
        | P::FallingSporeBlossom
        | P::DrippingObsidianTear
        | P::FallingObsidianTear
        | P::LandingObsidianTear
        | P::DrippingDripstoneLava
        | P::FallingDripstoneLava
        | P::DrippingDripstoneWater
        | P::FallingDripstoneWater => liquid::drip,
        P::Dust | P::DustColorTransition => magic::dust,
        P::Effect
        | P::InstantEffect
        | P::EntityEffect
        | P::Witch
        | P::Infested
        | P::RaidOmen
        | P::TrialOmen => magic::effect,
        P::Enchant | P::Nautilus | P::VaultConnection => magic::enchant,
        P::EndRod | P::TotemOfUndying => magic::end_rod,
        P::ExplosionEmitter => smoke::explosion_emitter,
        P::Explosion | P::SonicBoom => smoke::explosion,
        P::SweepAttack => living::sweep_attack,
        P::FallingDust => block::falling_dust,
        P::Glow | P::WaxOn | P::WaxOff | P::ElectricSpark | P::Scrape => magic::glow,
        P::Item | P::ItemSlime | P::ItemCobweb | P::ItemSnowball | P::SulfurCubeGoo => block::item,
        P::LargeSmoke | P::Smoke | P::WhiteSmoke => smoke::smoke,
        P::Ash => smoke::ash,
        P::WhiteAsh => smoke::white_ash,
        P::Lava => liquid::lava,
        P::Note => magic::note,
        P::Poof | P::Spit => smoke::poof,
        P::Portal | P::ReversePortal => magic::portal,
        P::Rain | P::Splash => liquid::rain,
        P::SquidInk | P::GlowSquidInk => liquid::squid_ink,
        P::Underwater | P::CrimsonSpore | P::WarpedSpore | P::SporeBlossomAir => nature::underwater,
        P::Snowflake => nature::snowflake,
        P::Trail => magic::trail,
        P::Vibration => magic::vibration,
        P::PauseMobGrowth | P::ResetMobGrowth => magic::mob_growth,
        P::Soul | P::SculkSoul => smoke::soul,
        P::SulfurBubbles => liquid::sulfur_bubbles,
        P::NoxiousGas => smoke::noxious_gas,
        P::NoxiousGasCloud => smoke::noxious_gas_cloud,
        P::Geyser => liquid::geyser,
        P::GeyserBase | P::GeyserPoof => liquid::geyser_base,
        P::GeyserPlume => liquid::geyser_plume,
        P::Gust | P::SmallGust => smoke::gust,
        P::GustEmitterLarge | P::GustEmitterSmall => smoke::gust_emitter,
        P::Firework => magic::firework,
        P::Flash => magic::flash,
        P::Fishing => liquid::fishing,
        P::CherryLeaves
        | P::PaleOakLeaves
        | P::RedPoplarLeaves
        | P::OrangePoplarLeaves
        | P::YellowPoplarLeaves
        | P::TintedLeaves => nature::leaves,
        P::SculkCharge | P::SculkChargePop => magic::sculk_charge,
        P::Shriek => magic::shriek,
        P::DustPlume => smoke::dust_plume,
        P::TrialSpawnerDetection | P::TrialSpawnerDetectionOminous => {
            magic::trial_spawner_detection
        }
        P::OminousSpawning => magic::ominous_spawning,
        P::Firefly => nature::firefly,
        P::ElderGuardian => return None,
    })
}

/// `0xRRGGBB` as floats.
pub fn rgb_of(rgb: u32) -> [f32; 3] {
    [
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    ]
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}
