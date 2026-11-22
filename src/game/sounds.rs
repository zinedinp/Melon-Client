//! The sounds of the world: the player's own (breaking, placing and hitting

use super::InWorld;
use crate::audio::SoundInstance;
use crate::audio::block_sounds::{BlockSound, SoundType};
use crate::audio::level_sounds::LevelSound;
use crate::audio::source::SoundSource;
use crate::network::play::game::sound::{BREAK_BLOCK, Explosion, LevelEvent, ServerSound, SoundAt};
use crate::world::{block_motion, physics};
use glam::DVec3;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId};
use std::time::{Duration, Instant};

/// `Entity.moveDist` grows by this much per block walked.
const STEP_FACTOR: f64 = 0.6;
/// `Entity.getOnPos()`: the supporting block itself.
const ON_POS: f64 = 1.0e-5_f32 as f64;
/// `Entity.getOnPosLegacy()`: the block a step or landing is heard from.
const ON_POS_LEGACY: f64 = 0.2_f32 as f64;
const SMALL_FALL: &str = "entity.player.small_fall";
const BIG_FALL: &str = "entity.player.big_fall";
/// How far from the camera a global level event is heard.
const GLOBAL_DISTANCE: f64 = 2.0;
/// A server sound this close to one the client just played itself is the
const ECHO_RANGE: f64 = 1.5;
const ECHO_WINDOW: Duration = Duration::from_millis(750);

/// Where the sounds are heard from: vanilla's `ListenerTransform`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ears {
    pub position: [f64; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

/// What the sounds keep between frames.
#[derive(Default)]
pub(super) struct SoundState {
    /// Sounds the player made, for the app to play.
    sounds: Vec<SoundInstance<'static>>,
    footsteps: Footsteps,
    /// The last rendered camera as the sounds hear it.
    pub(super) ears: Option<Ears>,
    /// What the client played itself lately, to not play it again when
    /// the server echoes it.
    echoes: Echoes,
    /// The sound channels in use, static and streaming, for F3.
    pub(super) channels: [u32; 2],
    /// The decoded sound files and their bytes, for F3.
    pub(super) cache: (usize, usize),
    /// The level changed: everything playing stops before what the new
    /// level queues plays ([`InWorld::take_stop_all`]).
    stop_all: bool,
}

impl SoundState {
    /// `Minecraft.updateLevelInEngines`' `soundManager.stop()`: what the old
    /// level still queued is dropped, and the app stops what plays.
    pub(super) fn stop_all(&mut self) {
        self.sounds.clear();
        self.stop_all = true;
    }

    /// A new `LocalPlayer`: its `moveDist` and `nextStep` start over.
    pub(super) fn respawned(&mut self) {
        self.footsteps = Footsteps::default();
    }

    /// Queues a sound for the app to play.
    pub(super) fn push(&mut self, sound: SoundInstance<'static>) {
        self.sounds.push(sound);
    }
}

impl Extend<SoundInstance<'static>> for SoundState {
    fn extend<I: IntoIterator<Item = SoundInstance<'static>>>(&mut self, sounds: I) {
        self.sounds.extend(sounds);
    }
}

/// `Entity.moveDist`, `nextStep` and the amethyst chime's state.
pub(super) struct Footsteps {
    last: Option<DVec3>,
    move_dist: f64,
    next_step: f64,
    /// `Entity.crystalSoundIntensity` and `lastCrystalSoundPlayTick`.
    crystal_intensity: f32,
    last_crystal_tick: u64,
}

impl Default for Footsteps {
    fn default() -> Self {
        Self {
            last: None,
            move_dist: 0.0,
            next_step: 1.0,
            crystal_intensity: 0.0,
            last_crystal_tick: 0,
        }
    }
}

/// What the client played itself lately: a server that tells every player
#[derive(Default)]
pub(super) struct Echoes {
    recent: Vec<(String, [f64; 3], Instant)>,
}

impl Echoes {
    fn remember(&mut self, sound: &SoundInstance, at: [f64; 3]) {
        let now = Instant::now();
        self.recent
            .retain(|(_, _, when)| now.duration_since(*when) < ECHO_WINDOW);
        self.recent.push((sound.event.to_string(), at, now));
    }

    /// Whether `event` at `at` is one the client just played.
    fn heard(&self, event: &str, at: [f64; 3]) -> bool {
        let now = Instant::now();
        self.recent.iter().any(|(name, place, when)| {
            name == event
                && now.duration_since(*when) < ECHO_WINDOW
                && distance(*place, at) < ECHO_RANGE
        })
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The middle of a block.
pub(super) fn block_center(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|c| f64::from(c) + 0.5)
}

/// The level events whose sound depends on their data, not only the event.
fn special_level_sound(event: LevelEvent, at: [f64; 3]) -> Option<SoundInstance<'static>> {
    let data = event.data;
    let r = rand::random::<f32>;
    let sound = |name, source, volume: f32, pitch: f32| {
        SoundInstance::of(name)
            .on(source)
            .volume(volume)
            .pitch(pitch)
            .at(at)
    };
    Some(match event.event {
        1009 => match data {
            0 => sound(
                "block.fire.extinguish",
                SoundSource::Blocks,
                0.5,
                2.6 + (r() - r()) * 0.8,
            ),
            1 => sound(
                "entity.generic.extinguish_fire",
                SoundSource::Blocks,
                0.7,
                1.6 + (r() - r()) * 0.4,
            ),
            _ => return None,
        },
        1500 => sound(
            if data > 0 {
                "block.composter.fill_success"
            } else {
                "block.composter.fill"
            },
            SoundSource::Blocks,
            1.0,
            1.0,
        ),
        3006 => {
            let count = (data >> 6) as f32;
            if count <= 0.0 {
                sound("block.sculk.charge", SoundSource::Blocks, 1.0, 1.0)
            } else if r() < 0.3 + count * 0.1 {
                let volume = 0.15 + 0.02 * count * count * r();
                sound(
                    "block.sculk.charge",
                    SoundSource::Blocks,
                    volume,
                    0.4 + 0.3 * count * r(),
                )
            } else {
                return None;
            }
        }
        3008 => {
            let name = match block_name(u16::try_from(data).unwrap_or(0)) {
                "suspicious_sand" => "item.brush.brushing.sand.complete",
                "suspicious_gravel" => "item.brush.brushing.gravel.complete",
                _ => return None,
            };
            sound(name, SoundSource::Players, 1.0, 1.0)
        }
        3020 => sound(
            "block.trial_spawner.ominous_activate",
            SoundSource::Blocks,
            if data == 0 { 0.3 } else { 1.0 },
            1.0 + (r() - r()) * 0.2,
        ),
        _ => return None,
    })
}

/// Blocks that spawn no destroy event when a player breaks them
pub(super) fn breaks_silently(state: u16) -> bool {
    matches!(block_name(state), "fire" | "soul_fire")
}

/// The block name of a state, `air` for one that doesn't exist.
fn block_name(state: u16) -> &'static str {
    BlockStateId::new(state).map_or("air", |id| Block::from_state_id(id).name)
}

impl InWorld {
    /// Sounds with a subtitle that just started, for the subtitle overlay.
    pub fn hear(&mut self, heard: Vec<crate::audio::Heard>, lang: &crate::i18n::Lang) {
        let now = Instant::now();
        for heard in heard {
            self.hud.subtitles.hear(heard, lang, now);
        }
    }

    /// The sounds queued since the last call.
    pub fn take_sounds(&mut self) -> Vec<SoundInstance<'static>> {
        std::mem::take(&mut self.audio.sounds)
    }

    /// Whether the level changed since the last call, so that every sound
    /// playing stops ([`crate::audio::AudioEngine::stop_all`]) before
    /// [`Self::take_sounds`]' are played.
    pub fn take_stop_all(&mut self) -> bool {
        std::mem::take(&mut self.audio.stop_all)
    }

    /// `SoundEngine.updateSource(camera)`: the last rendered camera, else
    /// the eye looking along the player's yaw and pitch.
    pub fn listener(&self) -> Ears {
        self.audio.ears.unwrap_or_else(|| {
            let eye = self.player.tick_eye();
            let forward =
                crate::world::player::forward_vector_for(self.player.yaw(), self.player.pitch());
            Ears {
                position: [eye.x, eye.y, eye.z],
                forward: forward.to_array(),
                up: [0.0, 1.0, 0.0],
            }
        })
    }

    /// Queues what block `state` sounds like for `kind`, heard from `at`.
    pub(super) fn queue_block_sound(&mut self, state: u16, kind: BlockSound, at: [f64; 3]) {
        if let Some(sound) = SoundType::of(block_name(state)).sound(kind) {
            let sound = sound.at(at);
            self.audio.echoes.remember(&sound, at);
            self.audio.sounds.push(sound);
        }
    }

    /// `Consumable.onUseTick`: the bite or sip of the food or drink being
    pub(super) fn play_consume_sound(&mut self) {
        let Some(hand) = self.hands.consuming.take() else {
            return;
        };
        let stack = self.hand_stack(hand);
        if super::particles::has_consume_particles(stack) {
            let item = stack.item.id;
            self.eating_particles(item, 5);
        }
        let Some((event, volume, pitch)) =
            crate::world::player::hands::consume_sound(self.hand_stack(hand), rand::random::<f32>)
        else {
            return;
        };
        let eye = self.player.tick_eye();
        let mut sound = SoundInstance::of(event).at([eye.x, eye.y, eye.z]);
        sound.source = SoundSource::Players;
        sound.volume = volume;
        sound.pitch = pitch;
        self.audio.sounds.push(sound);
    }

    /// `ClientboundSoundPacket` and `ClientboundSoundEntityPacket`.
    pub(super) fn on_server_sound(&mut self, sound: &ServerSound) {
        let at = match sound.at {
            SoundAt::Position(at) => at,
            SoundAt::Entity(id) if id == self.joined.session.info.entity_id => {
                let feet = self.player.feet();
                [feet.x, feet.y, feet.z]
            }
            SoundAt::Entity(id) => match self.entities.get(id) {
                Some(entity) => entity.tick_pos().to_array(),
                None => return,
            },
        };
        if self.audio.echoes.heard(&sound.event, at) {
            return;
        }
        self.audio.sounds.push(
            SoundInstance::of(sound.event.clone())
                .on(sound.source)
                .volume(sound.volume)
                .pitch(sound.pitch)
                .at(at),
        );
    }

    /// `ClientHandler.handleExplosion`: the blast sound, at volume 4 and a
    pub(super) fn on_explosion(&mut self, explosion: &Explosion) {
        let Some(sound) = &explosion.sound else {
            return;
        };
        let jitter = rand::random::<f32>() - rand::random::<f32>();
        self.audio.sounds.push(
            SoundInstance::of(sound.clone())
                .on(SoundSource::Blocks)
                .volume(4.0)
                .pitch((1.0 + jitter * 0.2) * 0.7)
                .at(explosion.center),
        );
    }

    /// `ClientboundLevelEventPacket`: `LevelEventHandler.levelEvent`'s
    pub(super) fn on_level_event(&mut self, event: LevelEvent) {
        let at = block_center(event.pos);
        let sound = if event.event == BREAK_BLOCK {
            let state = u16::try_from(event.data).unwrap_or(0);
            let Some(sound) = SoundType::of(block_name(state)).sound(BlockSound::Break) else {
                return;
            };
            if self.audio.echoes.heard(&sound.event, at) {
                return;
            }
            if event.global { sound } else { sound.at(at) }
        } else if let Some(sound) = LevelSound::of(event.event) {
            if event.global {
                sound.instance(self.toward(at, GLOBAL_DISTANCE))
            } else {
                sound.instance(at)
            }
        } else if let Some(sound) = special_level_sound(event, at) {
            sound
        } else {
            return;
        };
        self.audio.sounds.push(sound);
    }

    /// The point `distance` blocks from the eye toward `target`.
    fn toward(&self, target: [f64; 3], distance: f64) -> [f64; 3] {
        let eye = self.player.tick_eye();
        let eye = [eye.x, eye.y, eye.z];
        let delta = [0, 1, 2].map(|i| target[i] - eye[i]);
        let length = delta.iter().map(|d| d * d).sum::<f64>().sqrt().max(1e-9);
        [0, 1, 2].map(|i| eye[i] + delta[i] / length * distance)
    }

    /// `Entity.move` after the player moved: `checkFallDamage`'s landing
    /// sounds, then `applyMovementEmissionAndPlaySound`'s footsteps.
    pub(super) fn tick_footsteps(&mut self) {
        let feet = self.player.feet();
        let landing = self.player.take_landing();
        let Some(last) = self.audio.footsteps.last.replace(feet) else {
            return;
        };
        if self.game_mode.current.is_spectator() || self.player.riding() {
            return;
        }
        if let Some(fallen) = landing {
            self.landing_sounds(fallen);
        }
        // `Player.getMovementEmission`: flying or sneaking on the ground is silent.
        if self.player.flying() || (self.player.on_ground() && self.keys.sneak) {
            return;
        }
        let moved = feet - last;
        let support = self.player.supporting_block();
        let supporting_pos = block_motion::on_pos(&self.level, feet, support, ON_POS);
        let supporting = self.state_at(supporting_pos);
        let horizontal = moved.x.hypot(moved.z);
        let walked = if climbable(supporting) {
            moved.length()
        } else {
            horizontal
        };
        self.audio.footsteps.move_dist += walked * STEP_FACTOR;
        if self.audio.footsteps.move_dist <= self.audio.footsteps.next_step || is_air(supporting) {
            return;
        }
        let effect_pos = block_motion::on_pos(&self.level, feet, support, ON_POS_LEGACY);
        let effect = self.state_at(effect_pos);
        let mut stepped = self.step_effects(effect_pos, effect, true, moved.y);
        if effect_pos != supporting_pos {
            stepped |= self.step_effects(supporting_pos, supporting, false, moved.y);
        }
        if stepped || self.player.in_water() {
            self.audio.footsteps.next_step = self.audio.footsteps.move_dist.floor() + 1.0;
        }
        if !stepped && self.player.in_water() {
            self.swim_sound();
        }
    }

    /// `Entity.vibrationAndSoundEffectsFromBlock`: whether the block counts
    /// as stepped on, and its sound if `sound`.
    fn step_effects(
        &mut self,
        pos: (i32, i32, i32),
        state: u16,
        sound: bool,
        moved_y: f64,
    ) -> bool {
        if is_air(state) {
            return false;
        }
        let touching = self.player.on_ground()
            || climbable(state)
            || (self.player.crouching() && moved_y == 0.0);
        if !touching || self.player.swimming() {
            return false;
        }
        if sound {
            self.walking_step_sound(pos, state);
        }
        true
    }

    /// `Entity.walkingStepSound` with `Player.playStepSound`: a carpet, snow
    /// layer or the like on top is heard over the block under it.
    fn walking_step_sound(&mut self, (x, y, z): (i32, i32, i32), state: u16) {
        if self.player.in_water() {
            self.swim_sound();
            self.queue_step(state, BlockSound::MuffledStep);
        } else {
            let above = self.state_at((x, y + 1, z));
            let above_block = block_of(above);
            if above_block.has_tag(&tag::Block::MINECRAFT_COMBINATION_STEP_SOUND_BLOCKS) {
                self.queue_step(above, BlockSound::Step);
                self.queue_step(state, BlockSound::MuffledStep);
            } else if above_block.has_tag(&tag::Block::MINECRAFT_INSIDE_STEP_SOUND_BLOCKS) {
                self.queue_step(above, BlockSound::Step);
            } else {
                self.queue_step(state, BlockSound::Step);
            }
        }
        let ticks = self.ticks;
        let steps = &mut self.audio.footsteps;
        if block_of(state).has_tag(&tag::Block::MINECRAFT_CRYSTAL_SOUND_BLOCKS)
            && ticks >= steps.last_crystal_tick + 20
        {
            // `Entity.playAmethystStepSound`.
            let since = (ticks - steps.last_crystal_tick) as f64;
            steps.crystal_intensity *= 0.997_f64.powf(since) as f32;
            steps.crystal_intensity = (steps.crystal_intensity + 0.07).min(1.0);
            let intensity = steps.crystal_intensity;
            steps.last_crystal_tick = ticks;
            let pitch = 0.5 + intensity * rand::random::<f32>() * 1.2;
            self.queue_player_sound("block.amethyst_block.chime", 0.1 + intensity * 1.2, pitch);
        }
    }

    /// `Entity.waterSwimSound` for the player itself.
    fn swim_sound(&mut self) {
        let v = self.player.velocity();
        let speed = (v.x * v.x * 0.2 + v.y * v.y + v.z * v.z * 0.2).sqrt() as f32;
        let volume = (speed * 0.35).min(1.0);
        let pitch = 1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.4;
        self.queue_player_sound("entity.player.swim", volume, pitch);
    }

    /// `Block.fallOn` and `LivingEntity.causeFallDamage` on landing: the
    /// fall sounds only come with fall damage, except honey's slide and
    /// powder snow's thud.
    fn landing_sounds(&mut self, fallen: f64) {
        let feet = self.player.feet();
        let support = self.player.supporting_block();
        let on = block_of(self.state_at(block_motion::on_pos(
            &self.level,
            feet,
            support,
            ON_POS_LEGACY,
        )));
        let modifier = match on.name {
            "hay_block" => 0.2,
            "honey_block" => {
                self.queue_player_sound("block.honey_block.slide", 1.0, 1.0);
                0.2
            }
            "slime_block" if !self.keys.sneak => 0.0,
            "powder_snow" => {
                if fallen >= 4.0 {
                    let event = if fallen < 7.0 { SMALL_FALL } else { BIG_FALL };
                    self.queue_player_sound(event, 1.0, 1.0);
                }
                return;
            }
            _ => 1.0,
        };
        // `Player.causeFallDamage`: no damage, and so no sound, for who may fly.
        if self.player.abilities().may_fly {
            return;
        }
        let attributes = self.player.attributes();
        let power = fallen + 1.0e-6 - attributes.safe_fall_distance();
        let damage = (power * modifier * attributes.fall_damage_multiplier()).floor();
        if damage <= 0.0 {
            return;
        }
        self.queue_player_sound(if damage > 4.0 { BIG_FALL } else { SMALL_FALL }, 1.0, 1.0);
        // `LivingEntity.playBlockFallSound`: the plain cell 0.2 under the feet.
        let below = (feet - DVec3::new(0.0, f64::from(0.2_f32), 0.0)).floor();
        let state = self.state_at((below.x as i32, below.y as i32, below.z as i32));
        if !is_air(state) {
            self.queue_step(state, BlockSound::Fall);
        }
    }

    /// A block's step or fall sound, heard from the player's feet.
    fn queue_step(&mut self, state: u16, kind: BlockSound) {
        let feet = self.player.feet();
        self.queue_block_sound(state, kind, [feet.x, feet.y, feet.z]);
    }

    /// `Player.playSound`: on the player's source, from its feet.
    fn queue_player_sound(&mut self, event: &'static str, volume: f32, pitch: f32) {
        let feet = self.player.feet();
        let sound = SoundInstance::of(event)
            .on(SoundSource::Players)
            .volume(volume)
            .pitch(pitch)
            .at([feet.x, feet.y, feet.z]);
        self.audio.echoes.remember(&sound, [feet.x, feet.y, feet.z]);
        self.audio.sounds.push(sound);
    }

    fn state_at(&self, (x, y, z): (i32, i32, i32)) -> u16 {
        physics::block_at(&self.level, x, y, z)
    }
}

fn block_of(state: u16) -> &'static Block {
    Block::from_state_id(BlockStateId::new_or_air(state))
}

/// `BlockState.isAir`.
fn is_air(state: u16) -> bool {
    matches!(block_of(state).name, "air" | "cave_air" | "void_air")
}

/// `Entity.isStateClimbable`.
fn climbable(state: u16) -> bool {
    let block = block_of(state);
    block.has_tag(&tag::Block::MINECRAFT_CLIMBABLE) || block.name == "powder_snow"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_sound_next_to_one_just_played_is_an_echo() {
        let mut echoes = Echoes::default();
        let played = SoundInstance::of("block.stone.break");
        echoes.remember(&played, [10.5, 64.5, 3.5]);
        assert!(echoes.heard("block.stone.break", [10.5, 65.0, 3.5]));
        assert!(!echoes.heard("block.stone.break", [20.5, 64.5, 3.5]));
        assert!(!echoes.heard("block.stone.place", [10.5, 64.5, 3.5]));
    }

    #[test]
    fn fire_breaks_without_a_sound() {
        assert!(breaks_silently(Block::FIRE.default_state.id.as_u16()));
        assert!(breaks_silently(Block::SOUL_FIRE.default_state.id.as_u16()));
        assert!(!breaks_silently(Block::STONE.default_state.id.as_u16()));
    }

    #[test]
    fn the_step_tags_hold_vanilla_blocks() {
        let combination = &tag::Block::MINECRAFT_COMBINATION_STEP_SOUND_BLOCKS;
        let inside = &tag::Block::MINECRAFT_INSIDE_STEP_SOUND_BLOCKS;
        assert!(Block::SNOW.has_tag(combination));
        assert!(Block::WHITE_CARPET.has_tag(combination));
        assert!(Block::MOSS_CARPET.has_tag(combination));
        assert!(Block::POWDER_SNOW.has_tag(inside));
        assert!(Block::PINK_PETALS.has_tag(inside));
        assert!(!Block::SNOW_BLOCK.has_tag(combination));
        assert!(Block::AMETHYST_BLOCK.has_tag(&tag::Block::MINECRAFT_CRYSTAL_SOUND_BLOCKS));
        assert!(climbable(Block::LADDER.default_state.id.as_u16()));
        assert!(is_air(0));
    }

    #[test]
    fn a_block_is_heard_from_its_middle() {
        assert_eq!(block_center([1, 2, -3]), [1.5, 2.5, -2.5]);
    }
}
