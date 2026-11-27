//! The sound packets: `ClientboundSoundPacket`, `ClientboundSoundEntityPacket`

use anyhow::Result;
use pumpkin_data::sound::Sound;

use crate::audio::source::SoundSource;
use crate::particle::ParticleSpec;
use crate::particle::server::explosion::Debris;
use crate::protocol::packet::Reader;
use crate::protocol::reader::initial_capacity;

/// Where a server sound is heard from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundAt {
    Position([f64; 3]),
    /// Follows an entity.
    Entity(i32),
}

/// A sound the server asks the client to play.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerSound {
    /// The sound event, `minecraft:` or not.
    pub event: String,
    pub source: SoundSource,
    pub at: SoundAt,
    pub volume: f32,
    pub pitch: f32,
}

/// `ClientboundLevelEventPacket`: a numbered happening at a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelEvent {
    pub event: i32,
    pub pos: [i32; 3],
    pub data: i32,
    /// `disableRelativeVolume`: heard at full volume wherever the player is.
    pub global: bool,
}

/// `LevelEvent.PARTICLES_DESTROY_BLOCK`.
pub const BREAK_BLOCK: i32 = 2001;

/// A `Holder<SoundEvent>`.
fn read_event(r: &mut Reader) -> Result<String> {
    let id = r.var_int()?;
    if id == 0 {
        let name = r.string()?;
        if r.bool()? {
            r.f32_be()?;
        }
        return Ok(name);
    }
    Sound::NAMES
        .get((id - 1) as usize)
        .map(|name| (*name).to_owned())
        .ok_or_else(|| anyhow::anyhow!("unknown sound event {id}"))
}

/// `ClientboundExplodePacket`, what a client acts on.
#[derive(Clone, Debug, PartialEq)]
pub struct Explosion {
    pub center: [f64; 3],
    pub radius: f32,
    /// `blockCount`: how many blocks it broke, for the debris.
    pub block_count: i32,
    /// The push this player takes (`playerKnockback`).
    pub knockback: Option<[f64; 3]>,
    /// The blast itself (`explosionParticle`).
    pub particle: ParticleSpec,
    /// The sound event, unless the packet says none (`playSound`).
    pub sound: Option<String>,
    /// `blockParticles`: what flies off the broken blocks.
    pub debris: Vec<Debris>,
}

/// `ClientboundExplodePacket`.
pub(super) fn read_explosion(r: &mut Reader) -> Result<Explosion> {
    let center = r.vec3_f64()?;
    let radius = r.f32_be()?;
    let block_count = r.i32_be()?;
    let knockback = if r.bool()? { Some(r.vec3_f64()?) } else { None };
    let particle = super::particle::particle(r)?;
    let event = read_event(r)?;
    let pool = r.list_count()?;
    let mut debris = Vec::with_capacity(initial_capacity(pool));
    for _ in 0..pool {
        debris.push(Debris {
            spec: super::particle::particle(r)?,
            scaling: r.f32_be()?,
            speed: r.f32_be()?,
            weight: r.var_int()?,
        });
    }
    let play = r.bool()?;
    Ok(Explosion {
        center,
        radius,
        block_count,
        knockback,
        particle,
        sound: play.then_some(event),
        debris,
    })
}

/// A `SoundSource`, by its place in the enum.
fn read_source(r: &mut Reader) -> Result<SoundSource> {
    let index = r.var_int()?;
    Ok(usize::try_from(index)
        .ok()
        .and_then(|i| SoundSource::ALL.get(i))
        .copied()
        .unwrap_or(SoundSource::Master))
}

/// `ClientboundSoundPacket`.
pub(super) fn read_sound(r: &mut Reader) -> Result<ServerSound> {
    let event = read_event(r)?;
    let source = read_source(r)?;
    let mut position = [0.0; 3];
    for axis in &mut position {
        *axis = f64::from(r.i32_be()?) / 8.0;
    }
    let volume = r.f32_be()?;
    let pitch = r.f32_be()?;
    r.i64_be()?;
    Ok(ServerSound {
        event,
        source,
        at: SoundAt::Position(position),
        volume,
        pitch,
    })
}

/// `ClientboundSoundEntityPacket`: event, source, entity, volume, pitch, seed.
pub(super) fn read_entity_sound(r: &mut Reader) -> Result<ServerSound> {
    let event = read_event(r)?;
    let source = read_source(r)?;
    let entity = r.var_int()?;
    let volume = r.f32_be()?;
    let pitch = r.f32_be()?;
    r.i64_be()?;
    Ok(ServerSound {
        event,
        source,
        at: SoundAt::Entity(entity),
        volume,
        pitch,
    })
}

pub(super) fn read_level_event(r: &mut Reader) -> Result<LevelEvent> {
    Ok(LevelEvent {
        event: r.i32_be()?,
        pos: r.block_pos()?,
        data: r.i32_be()?,
        global: r.bool()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::packet::Reader;
    use crate::protocol::reader::{pack_block_pos, write_bool, write_f32_be, write_string};
    use crate::protocol::varint::write_var_int;

    #[test]
    fn a_sound_packet_with_a_registry_id_is_read() {
        let stone_break = Sound::NAMES
            .iter()
            .position(|n| *n == "block.stone.break")
            .unwrap();
        let mut body = Vec::new();
        write_var_int(&mut body, stone_break as i32 + 1).unwrap();
        write_var_int(&mut body, 4).unwrap();
        for eighths in [80i32, -16, 8] {
            body.extend_from_slice(&eighths.to_be_bytes());
        }
        write_f32_be(&mut body, 1.0).unwrap();
        write_f32_be(&mut body, 0.8).unwrap();
        body.extend_from_slice(&7i64.to_be_bytes());
        let sound = read_sound(&mut Reader::new(&body)).unwrap();
        assert_eq!(sound.event, "block.stone.break");
        assert_eq!(sound.source, SoundSource::Blocks);
        assert_eq!(sound.at, SoundAt::Position([10.0, -2.0, 1.0]));
        assert_eq!((sound.volume, sound.pitch), (1.0, 0.8));
    }

    #[test]
    fn an_explosion_packet_yields_its_sound_and_knockback() {
        use pumpkin_data::particle::Particle;
        let blast = Sound::NAMES
            .iter()
            .position(|n| *n == "entity.generic.explode")
            .unwrap();
        let build = |particle: Particle, play: bool, knockback: bool| {
            let mut body = Vec::new();
            for c in [1.5f64, 64.0, -3.5] {
                body.extend_from_slice(&c.to_be_bytes());
            }
            write_f32_be(&mut body, 4.0).unwrap();
            body.extend_from_slice(&12i32.to_be_bytes());
            write_bool(&mut body, knockback).unwrap();
            if knockback {
                for c in [0.5f64, 0.25, 0.0] {
                    body.extend_from_slice(&c.to_be_bytes());
                }
            }
            write_var_int(&mut body, i32::from(particle.to_id())).unwrap();
            write_var_int(&mut body, blast as i32 + 1).unwrap();
            write_var_int(&mut body, 0).unwrap();
            write_bool(&mut body, play).unwrap();
            body
        };
        let e = read_explosion(&mut Reader::new(&build(
            Particle::ExplosionEmitter,
            true,
            true,
        )))
        .unwrap();
        assert_eq!(e.center, [1.5, 64.0, -3.5]);
        assert_eq!(e.knockback, Some([0.5, 0.25, 0.0]));
        assert_eq!(e.sound.as_deref(), Some("entity.generic.explode"));
        let quiet =
            read_explosion(&mut Reader::new(&build(Particle::Explosion, false, false))).unwrap();
        assert_eq!((quiet.knockback, quiet.sound), (None, None));
    }

    #[test]
    fn a_named_sound_and_an_entity_sound_are_read() {
        let mut body = Vec::new();
        write_var_int(&mut body, 0).unwrap();
        write_string(&mut body, "minecraft:custom.thing").unwrap();
        write_bool(&mut body, true).unwrap();
        write_f32_be(&mut body, 24.0).unwrap();
        write_var_int(&mut body, 7).unwrap();
        write_var_int(&mut body, 42).unwrap();
        write_f32_be(&mut body, 0.5).unwrap();
        write_f32_be(&mut body, 1.2).unwrap();
        body.extend_from_slice(&0i64.to_be_bytes());
        let sound = read_entity_sound(&mut Reader::new(&body)).unwrap();
        assert_eq!(sound.event, "minecraft:custom.thing");
        assert_eq!(sound.source, SoundSource::Players);
        assert_eq!(sound.at, SoundAt::Entity(42));
    }

    #[test]
    fn a_source_out_of_range_is_the_master() {
        let mut body = Vec::new();
        write_var_int(&mut body, 99).unwrap();
        assert_eq!(
            read_source(&mut Reader::new(&body)).unwrap(),
            SoundSource::Master
        );
        assert!(read_event(&mut Reader::new(&[0xFF, 0x7F])).is_err());
    }

    #[test]
    fn a_level_event_is_read() {
        let mut body = Vec::new();
        body.extend_from_slice(&BREAK_BLOCK.to_be_bytes());
        body.extend_from_slice(&pack_block_pos([5, 64, -3]).to_be_bytes());
        body.extend_from_slice(&1234i32.to_be_bytes());
        write_bool(&mut body, false).unwrap();
        let event = read_level_event(&mut Reader::new(&body)).unwrap();
        assert_eq!(
            event,
            LevelEvent {
                event: BREAK_BLOCK,
                pos: [5, 64, -3],
                data: 1234,
                global: false
            }
        );
    }
}
