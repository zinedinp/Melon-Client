//! Particles, after vanilla's `ParticleEngine` and `ClientLevel.addParticle`.

use std::sync::Arc;

use glam::DVec3;

use crate::block_models::BlockModels;
use crate::java_random::JavaRandom;
use crate::level::Level;
use melon_core::cycle::Cycle;

pub mod animation;
pub mod base;
pub mod behavior;
pub mod client;
pub mod kind;
pub mod mesh;
pub mod motion;
pub mod options;
pub mod provider;
pub mod server;
pub mod wire;

pub use base::Particle;
pub use kind::Kind;
pub use options::{ParticleData, ParticleSpec, PositionSource};

use animation::SpriteSets;

/// `ParticleGroup.MAX_PARTICLES`: at most this many of a group at once.
pub const MAX_PER_GROUP: usize = 16_384;
/// `ParticleGroup.RESERVOIR_START`: past this many, new ones get rarer.
const RESERVOIR_START: usize = 12_288;
/// `ParticleLimit.SPORE_BLOSSOM`.
const SPORE_BLOSSOM_LIMIT: usize = 1000;
/// `doAddParticle`: further than 32 blocks from the camera, none.
const MAX_DISTANCE_SQ: f64 = 1024.0;

/// An entity as particles see it: feet, box and motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityBox {
    pub pos: DVec3,
    /// Where it was the tick before.
    pub prev: DVec3,
    pub width: f32,
    pub height: f32,
    pub vel: DVec3,
    /// Its type's registry name (`snowball`, `villager`).
    pub name: &'static str,
    /// `getYRot`, `getXRot`: vanilla's degrees.
    pub yaw: f32,
    pub pitch: f32,
}

impl EntityBox {
    /// `Entity.getRandomX(1)`, `getRandomY()`, `getRandomZ(1)`.
    pub fn random_point(&self, rng: &mut JavaRandom) -> DVec3 {
        let w = f64::from(self.width);
        let x = self.pos.x + w * (2.0 * rng.next_double() - 1.0);
        let y = self.pos.y + f64::from(self.height) * rng.next_double();
        let z = self.pos.z + w * (2.0 * rng.next_double() - 1.0);
        DVec3::new(x, y, z)
    }

    /// `getEyeY`, as `Entity`'s default eye: 0.85 of the height.
    pub fn eye(&self) -> DVec3 {
        self.pos + DVec3::Y * f64::from(self.height * 0.85)
    }
}

/// The item an entity has in a slot, by registry id.
pub type EquipmentAt<'a> = &'a dyn Fn(i32, crate::entity::wire::EquipmentSlot) -> Option<u16>;

/// What a tick or a spawn reads of the world.
pub struct TickCtx<'a> {
    pub level: &'a Level,
    pub sprites: &'a SpriteSets,
    /// `None` until the block models are loaded: no block particles.
    pub blocks: Option<&'a BlockModels>,
    /// Where an entity is, by id.
    pub entity: &'a dyn Fn(i32) -> Option<EntityBox>,
    /// `getNearestPlayer(pos, distance, false)`.
    pub nearest_player: NearestPlayer<'a>,
    /// The eyes of the player looking through a spyglass in first person.
    pub scoping_eye: Option<DVec3>,
}

impl TickCtx<'_> {
    /// `SpellParticle.isCloseToScopingPlayer`: within 3 of a spyglass.
    pub fn close_to_scope(&self, pos: DVec3) -> bool {
        self.scoping_eye
            .is_some_and(|eye| eye.distance_squared(pos) <= 9.0)
    }
}

/// The nearest player within a distance of a point, spectators too.
pub type NearestPlayer<'a> = &'a dyn Fn(DVec3, f64) -> Option<EntityBox>;

/// The world as the engine is given it.
pub struct World<'a> {
    pub level: &'a Level,
    pub entity: &'a dyn Fn(i32) -> Option<EntityBox>,
    pub nearest_player: NearestPlayer<'a>,
    /// `isRainingAt`: it rains, the block sees the sky and its biome rains.
    pub raining_at: &'a dyn Fn(glam::IVec3) -> bool,
    /// `minecraft:gameplay/sky_light_level`.
    pub sky_light: f32,
    /// What an entity holds and wears.
    pub equipment: EquipmentAt<'a>,
    /// `getMarkerParticleTarget`.
    pub marker: Option<&'static str>,
    /// See [`TickCtx::scoping_eye`].
    pub scoping_eye: Option<DVec3>,
    /// `isThundering`: the thunder level over 0.9.
    pub thundering: bool,
    /// `getGameTime`.
    pub game_time: i64,
}

impl<'a> World<'a> {
    /// A world of only blocks and entities: no players, no rain, full sky.
    pub fn plain(level: &'a Level, entity: &'a dyn Fn(i32) -> Option<EntityBox>) -> Self {
        Self {
            level,
            entity,
            nearest_player: &|_, _| None,
            raining_at: &|_| false,
            sky_light: 15.0,
            equipment: &|_, _| None,
            marker: None,
            scoping_eye: None,
            thundering: false,
            game_time: 0,
        }
    }

    /// `getMaxLocalRawBrightness`.
    pub fn raw_brightness(&self, pos: glam::IVec3) -> u8 {
        let (block, sky) = self.level.light_at(pos.x, pos.y, pos.z);
        let darken = (15.0 - self.sky_light).round().clamp(0.0, 15.0) as i32;
        block
            .min(15)
            .max((i32::from(sky) - darken).clamp(0, 15) as u8)
    }

    pub fn ctx<'b>(&'b self, looks: &'b Looks) -> TickCtx<'b> {
        TickCtx {
            level: self.level,
            sprites: &looks.sprites,
            blocks: Some(&looks.models),
            entity: self.entity,
            nearest_player: self.nearest_player,
            scoping_eye: self.scoping_eye,
        }
    }
}

/// A particle asked for (`level.addParticle`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn {
    pub spec: ParticleSpec,
    pub pos: DVec3,
    /// `xd, yd, zd`: a velocity for most types.
    pub aux: DVec3,
    /// Shown at any distance and setting.
    pub override_limiter: bool,
    /// `addAlwaysVisibleParticle`: Minimal shows some.
    pub always_show: bool,
}

impl Spawn {
    pub fn new(spec: ParticleSpec, pos: DVec3, aux: DVec3) -> Self {
        Self {
            spec,
            pos,
            aux,
            override_limiter: false,
            always_show: false,
        }
    }

    pub fn simple(kind: Kind, pos: DVec3, aux: DVec3) -> Self {
        Self::new(ParticleSpec::simple(kind), pos, aux)
    }

    /// The packet's `overrideLimiter`.
    pub fn forced(mut self, force: bool) -> Self {
        self.override_limiter |= force;
        self
    }

    pub fn always_show(mut self, always: bool) -> Self {
        self.always_show |= always;
        self
    }
}

/// The textures particles are drawn from.
#[derive(Clone)]
pub struct Looks {
    pub models: Arc<BlockModels>,
    pub sprites: Arc<SpriteSets>,
}

/// `ParticleEngine`: every live particle, ticked 20 times a second.
pub struct ParticleEngine {
    /// `SINGLE_QUADS`: the drawn ones.
    quads: Vec<Particle>,
    /// `NO_RENDER`: emitters.
    hidden: Vec<Particle>,
    /// `particlesToAdd`: made this tick, in from the next.
    pending: Vec<Particle>,
    /// Asked for since the last tick, to be made with the world at hand.
    requests: Vec<Spawn>,
    /// `trackedParticleCounts` for the spore blossom's limit.
    spore_blossoms: usize,
    rng: JavaRandom,
    /// The Particles option.
    pub status: ParticleStatus,
    /// Where the camera is, for the distance limit.
    pub camera: DVec3,
    looks: Option<Looks>,
    /// `ClientExplosionTracker`.
    pub explosions: server::explosion::ExplosionTracker,
}

impl Default for ParticleEngine {
    fn default() -> Self {
        Self {
            quads: Vec::new(),
            hidden: Vec::new(),
            pending: Vec::new(),
            requests: Vec::new(),
            spore_blossoms: 0,
            rng: JavaRandom::new(seed()),
            status: ParticleStatus::All,
            camera: DVec3::ZERO,
            looks: None,
            explosions: Default::default(),
        }
    }
}

/// A seed that differs from run to run.
fn seed() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64)
}

impl ParticleEngine {
    /// The models and sprites to draw with; kept until they change.
    pub fn set_looks(&mut self, models: &Arc<BlockModels>, sprites: &Arc<SpriteSets>) {
        let same = self.looks.as_ref().is_some_and(|looks| {
            Arc::ptr_eq(&looks.models, models) && Arc::ptr_eq(&looks.sprites, sprites)
        });
        if !same {
            self.looks = Some(Looks {
                models: models.clone(),
                sprites: sprites.clone(),
            });
        }
    }

    /// `level.addParticle`: made at the next tick (or by [`Self::flush`]).
    pub fn request(&mut self, spawn: Spawn) {
        self.requests.push(spawn);
    }

    pub fn request_all(&mut self, spawns: impl IntoIterator<Item = Spawn>) {
        self.requests.extend(spawns);
    }

    /// The engine's random numbers, for spawners that roll their own.
    pub fn rng(&mut self) -> &mut JavaRandom {
        &mut self.rng
    }

    /// How many are alive: drawn, and in all.
    pub fn counts(&self) -> (usize, usize) {
        (self.quads.len(), self.quads.len() + self.hidden.len())
    }

    /// Everything gone, for a new level or dimension.
    pub fn clear(&mut self) {
        self.quads.clear();
        self.hidden.clear();
        self.pending.clear();
        self.requests.clear();
        self.spore_blossoms = 0;
        self.explosions = Default::default();
    }

    /// `calculateParticleLevel`.
    fn particle_level(&mut self, always_show: bool) -> ParticleStatus {
        let mut level = self.status;
        if always_show && level == ParticleStatus::Minimal && self.rng.next_int(10) == 0 {
            level = ParticleStatus::Decreased;
        }
        if level == ParticleStatus::Decreased && self.rng.next_int(3) == 0 {
            level = ParticleStatus::Minimal;
        }
        level
    }

    /// `doAddParticle`'s filter: whether a requested particle is made.
    fn admits(&mut self, spawn: &Spawn) -> bool {
        let level = self.particle_level(spawn.always_show);
        if spawn.override_limiter || kind::overrides_limiter(spawn.spec.kind) {
            return true;
        }
        self.camera.distance_squared(spawn.pos) <= MAX_DISTANCE_SQ
            && level != ParticleStatus::Minimal
    }

    /// Makes the requests asked for so far.
    pub fn flush(&mut self, world: &World) {
        let Some(looks) = self.looks.clone() else {
            self.requests.clear();
            return;
        };
        let ctx = world.ctx(&looks);
        let mut requests = std::mem::take(&mut self.requests);
        for spawn in requests.drain(..) {
            self.make(&spawn, &ctx);
        }
        self.requests = requests;
    }

    /// `LevelRenderer.addParticleInternal`: `spawn` made now if admitted.
    fn make(&mut self, spawn: &Spawn, ctx: &TickCtx) {
        if !self.admits(spawn) {
            return;
        }
        if let Some(p) = provider::create(&spawn.spec, spawn.pos, spawn.aux, ctx, &mut self.rng) {
            self.add(p);
        }
    }

    /// `ParticleEngine.add`: queued for the next tick, past any limit.
    pub fn add(&mut self, p: Particle) {
        if p.kind == Kind::SporeBlossomAir {
            if self.spore_blossoms >= SPORE_BLOSSOM_LIMIT {
                return;
            }
            self.spore_blossoms += 1;
        }
        self.pending.push(p);
    }

    /// Makes particles directly, through the world's looks.
    pub fn with_ctx<R>(
        &mut self,
        world: &World,
        f: impl FnOnce(&TickCtx, &mut Self) -> R,
    ) -> Option<R> {
        let looks = self.looks.clone()?;
        let ctx = world.ctx(&looks);
        Some(f(&ctx, self))
    }

    /// `ParticleEngine.tick`.
    pub fn tick(&mut self, world: &World) {
        let mut debris = Vec::new();
        self.explosions
            .tick(self.status, world.level, &mut self.rng, &mut debris);
        self.requests.extend(debris);
        self.flush(world);
        let Some(looks) = self.looks.clone() else {
            return;
        };
        let ctx = world.ctx(&looks);
        let mut spawned = Vec::new();
        for group in [&mut self.quads, &mut self.hidden] {
            for p in group.iter_mut() {
                behavior::tick(p, &ctx, &mut self.rng, &mut spawned);
                if p.look.animated && !p.removed {
                    p.look.uv = ctx.sprites.from_age(p.kind, p.age, p.lifetime);
                }
            }
        }
        let mut gone_blossoms = 0;
        for group in [&mut self.quads, &mut self.hidden] {
            group.retain(|p| {
                if p.removed && p.kind == Kind::SporeBlossomAir {
                    gone_blossoms += 1;
                }
                !p.removed
            });
        }
        self.spore_blossoms = self.spore_blossoms.saturating_sub(gone_blossoms);
        // What particles gave off is made in their tick, as vanilla's
        // `level.addParticle` does, and joins the groups below.
        for spawn in &spawned {
            self.make(spawn, &ctx);
        }
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            // `NoRenderParticle`s; a shriek still waiting is drawn later.
            let group = if matches!(p.behavior, behavior::Behavior::Emitter(_)) {
                &mut self.hidden
            } else {
                &mut self.quads
            };
            if !admit_to_group(group.len(), &mut self.rng) {
                if p.kind == Kind::SporeBlossomAir {
                    self.spore_blossoms = self.spore_blossoms.saturating_sub(1);
                }
                continue;
            }
            group.push(p);
        }
    }

    /// The drawn particles.
    pub fn quads(&self) -> &[Particle] {
        &self.quads
    }
}

/// `ParticleGroup.add`.
fn admit_to_group(size: usize, rng: &mut JavaRandom) -> bool {
    if size >= MAX_PER_GROUP {
        return false;
    }
    if size < RESERVOIR_START {
        return true;
    }
    let room = (MAX_PER_GROUP - size) as f32 / (MAX_PER_GROUP - RESERVOIR_START) as f32;
    rng.next_float() < room * room
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_group_takes_no_more_and_a_filling_one_fewer() {
        let mut rng = JavaRandom::new(3);
        assert!(admit_to_group(0, &mut rng));
        assert!(admit_to_group(RESERVOIR_START - 1, &mut rng));
        assert!(!admit_to_group(MAX_PER_GROUP, &mut rng));
        let taken = (0..1000)
            .filter(|_| admit_to_group(MAX_PER_GROUP - 512, &mut rng))
            .count();
        assert!(taken < 50, "{taken}");
    }

    #[test]
    fn the_setting_and_the_distance_thin_out_particles() {
        let mut engine = ParticleEngine::default();
        let near = Spawn::simple(Kind::Flame, DVec3::new(1.0, 0.0, 0.0), DVec3::ZERO);
        let far = Spawn::simple(Kind::Flame, DVec3::new(40.0, 0.0, 0.0), DVec3::ZERO);
        assert!(engine.admits(&near));
        assert!(!engine.admits(&far));
        assert!(engine.admits(&far.forced(true)));
        let blast = Spawn::simple(Kind::Explosion, DVec3::new(40.0, 0.0, 0.0), DVec3::ZERO);
        assert!(engine.admits(&blast));
        engine.status = ParticleStatus::Minimal;
        assert!(!engine.admits(&near));
        assert!(engine.admits(&near.forced(true)));
        engine.status = ParticleStatus::Decreased;
        let shown = (0..300).filter(|_| engine.admits(&near)).count();
        assert!((150..250).contains(&shown), "{shown}");
    }
}
