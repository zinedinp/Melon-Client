//! The particles' wire formats.

use anyhow::{Result, anyhow};

use crate::network::protocol::packet::Reader;
use crate::particle::Kind;
use crate::particle::options::{Codec, ParticleData, ParticleSpec, PositionSource, codec};
use crate::particle::server::{LevelParticles, Randomization};

impl Reader<'_> {
    /// `ParticleTypes.STREAM_CODEC`: the type id, then its data.
    pub fn particle(&mut self) -> Result<ParticleSpec> {
        let id = self.var_int()?;
        let kind = u16::try_from(id)
            .ok()
            .and_then(Kind::from_id)
            .ok_or_else(|| anyhow!("unknown particle {id}"))?;
        let data = match codec(kind) {
            Codec::None => ParticleData::None,
            Codec::Block => ParticleData::Block(state_id(self.var_int()?)),
            Codec::Item => ParticleData::Item(self.particle_item()?),
            Codec::Dust => ParticleData::Dust {
                rgb: self.i32_be()? as u32,
                scale: self.f32_be()?.clamp(0.01, 4.0),
            },
            Codec::DustTransition => ParticleData::DustTransition {
                from: self.i32_be()? as u32,
                to: self.i32_be()? as u32,
                scale: self.f32_be()?.clamp(0.01, 4.0),
            },
            Codec::Color => ParticleData::Color(self.i32_be()? as u32),
            Codec::Spell => ParticleData::Spell {
                rgb: self.i32_be()? as u32,
                power: self.f32_be()?,
            },
            Codec::Power => ParticleData::Power(self.f32_be()?),
            Codec::SculkCharge => ParticleData::SculkCharge(self.f32_be()?),
            Codec::Shriek => ParticleData::Shriek(self.var_int()?),
            Codec::Vibration => {
                let to = match self.var_int()? {
                    0 => PositionSource::Block(self.block_pos()?),
                    1 => PositionSource::Entity {
                        id: self.var_int()?,
                        y_offset: self.f32_be()?,
                    },
                    other => return Err(anyhow!("unknown position source {other}")),
                };
                ParticleData::Vibration {
                    to,
                    ticks: self.var_int()?,
                }
            }
            Codec::Trail => ParticleData::Trail {
                target: self.vec3_f64()?,
                rgb: self.i32_be()? as u32,
                ticks: self.var_int()?,
            },
            Codec::Geyser => ParticleData::Geyser {
                water_blocks: self.i32_be()?,
            },
            Codec::GeyserBase => ParticleData::GeyserBase {
                water_blocks: self.i32_be()?,
                impulse: self.f32_be()?,
            },
        };
        Ok(ParticleSpec { kind, data })
    }

    /// `ItemStackTemplate.STREAM_CODEC` of an item particle.
    fn particle_item(&mut self) -> Result<u16> {
        let stack = self.item_template()?;
        Ok(stack.item.id)
    }
}

/// A block state id as sent; out of range is air.
fn state_id(id: i32) -> u16 {
    u16::try_from(id).unwrap_or(0)
}

/// `ClientboundLevelParticlesPacket.STREAM_CODEC`.
pub(super) fn read_level_particles(r: &mut Reader) -> Result<LevelParticles> {
    let spec = r.particle()?;
    let override_limiter = r.bool()?;
    let always_show = r.bool()?;
    let pos = r.vec3_f64()?;
    let mut dist = [0.0; 3];
    for axis in &mut dist {
        *axis = r.f32_be()?;
    }
    let mut max_speed = [0.0; 3];
    for axis in &mut max_speed {
        *axis = r.f32_be()?;
    }
    let count = r.var_int()?;
    let randomization = Randomization::from_id(r.var_int()?);
    Ok(LevelParticles {
        spec,
        override_limiter,
        always_show,
        pos,
        dist,
        max_speed,
        count,
        randomization,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particle::{Kind, ParticleData};
    use pumpkin_protocol::ClientPacket;
    use pumpkin_protocol::java::client::play::CParticle;
    use pumpkin_util::math::vector3::Vector3;
    use pumpkin_util::version::JavaMinecraftVersion;

    /// Pumpkin's packet, read back: what `/particle` sends.
    #[test]
    fn pumpkins_particle_packet_is_read() {
        let mut data = Vec::new();
        crate::network::protocol::varint::write_var_int(&mut data, 1).unwrap();
        let packet = CParticle::new(
            true,
            false,
            Vector3::new(1.5, 64.0, -2.0),
            Vector3::new(0.5, 0.25, 0.0),
            0.1,
            12,
            (Kind::Block.to_id() as i32).into(),
            &data,
        );
        let mut body = Vec::new();
        packet
            .write_packet_data(&mut body, &JavaMinecraftVersion::V_26_3)
            .unwrap();
        let mut r = Reader::new(&body);
        let read = read_level_particles(&mut r).unwrap();
        assert!(r.remaining().is_empty());
        assert_eq!(read.spec.kind, Kind::Block);
        assert_eq!(read.spec.data, ParticleData::Block(1));
        assert_eq!(read.pos, [1.5, 64.0, -2.0]);
        assert_eq!(read.dist, [0.5, 0.25, 0.0]);
        assert_eq!(read.max_speed, [0.1; 3]);
        assert_eq!(read.count, 12);
        assert_eq!(read.randomization, Randomization::Default);
    }
}

#[cfg(test)]
mod codec_tests {
    use super::*;
    use crate::network::protocol::reader::{pack_block_pos, write_f32_be};
    use crate::network::protocol::varint::write_var_int;

    fn read(body: &[u8]) -> ParticleSpec {
        let mut r = Reader::new(body);
        let spec = r.particle().unwrap();
        assert!(r.remaining().is_empty(), "{spec:?} left bytes over");
        spec
    }

    fn head(kind: Kind) -> Vec<u8> {
        let mut body = Vec::new();
        write_var_int(&mut body, i32::from(kind.to_id())).unwrap();
        body
    }

    #[test]
    fn every_codec_reads_its_bytes() {
        assert_eq!(read(&head(Kind::Flame)), ParticleSpec::simple(Kind::Flame));

        let mut block = head(Kind::Block);
        write_var_int(&mut block, 1).unwrap();
        assert_eq!(read(&block).data, ParticleData::Block(1));

        let mut dust = head(Kind::Dust);
        dust.extend(0x00FF_0000_i32.to_be_bytes());
        write_f32_be(&mut dust, 9.0).unwrap();
        assert_eq!(
            read(&dust).data,
            ParticleData::Dust {
                rgb: 0xFF0000,
                scale: 4.0
            }
        );

        let mut transition = head(Kind::DustColorTransition);
        transition.extend(1i32.to_be_bytes());
        transition.extend(2i32.to_be_bytes());
        write_f32_be(&mut transition, 1.0).unwrap();
        assert!(matches!(
            read(&transition).data,
            ParticleData::DustTransition { from: 1, to: 2, .. }
        ));

        let mut color = head(Kind::EntityEffect);
        color.extend((-1i32).to_be_bytes());
        assert_eq!(read(&color).data, ParticleData::Color(u32::MAX));

        let mut spell = head(Kind::Effect);
        spell.extend(0x123456i32.to_be_bytes());
        write_f32_be(&mut spell, 2.0).unwrap();
        assert_eq!(
            read(&spell).data,
            ParticleData::Spell {
                rgb: 0x123456,
                power: 2.0
            }
        );

        let mut power = head(Kind::DragonBreath);
        write_f32_be(&mut power, 0.5).unwrap();
        assert_eq!(read(&power).data, ParticleData::Power(0.5));

        let mut shriek = head(Kind::Shriek);
        write_var_int(&mut shriek, 15).unwrap();
        assert_eq!(read(&shriek).data, ParticleData::Shriek(15));

        let mut vibration = head(Kind::Vibration);
        write_var_int(&mut vibration, 0).unwrap();
        vibration.extend(pack_block_pos([1, 2, 3]).to_be_bytes());
        write_var_int(&mut vibration, 20).unwrap();
        assert_eq!(
            read(&vibration).data,
            ParticleData::Vibration {
                to: PositionSource::Block([1, 2, 3]),
                ticks: 20
            }
        );

        let mut to_entity = head(Kind::Vibration);
        write_var_int(&mut to_entity, 1).unwrap();
        write_var_int(&mut to_entity, 7).unwrap();
        write_f32_be(&mut to_entity, 1.5).unwrap();
        write_var_int(&mut to_entity, 4).unwrap();
        assert!(matches!(
            read(&to_entity).data,
            ParticleData::Vibration {
                to: PositionSource::Entity { id: 7, .. },
                ticks: 4
            }
        ));

        let mut trail = head(Kind::Trail);
        for c in [1.0f64, 2.0, 3.0] {
            trail.extend(c.to_be_bytes());
        }
        trail.extend(0xABCDEFi32.to_be_bytes());
        write_var_int(&mut trail, 30).unwrap();
        assert_eq!(
            read(&trail).data,
            ParticleData::Trail {
                target: [1.0, 2.0, 3.0],
                rgb: 0xABCDEF,
                ticks: 30
            }
        );

        let mut geyser = head(Kind::GeyserPoof);
        geyser.extend(3i32.to_be_bytes());
        write_f32_be(&mut geyser, 0.25).unwrap();
        assert!(matches!(
            read(&geyser).data,
            ParticleData::GeyserBase {
                water_blocks: 3,
                ..
            }
        ));
    }

    #[test]
    fn an_item_particle_keeps_its_item() {
        let apple = pumpkin_data::item::Item::APPLE.id;
        let mut item = head(Kind::Item);
        write_var_int(&mut item, i32::from(apple)).unwrap();
        write_var_int(&mut item, 1).unwrap();
        write_var_int(&mut item, 0).unwrap();
        write_var_int(&mut item, 0).unwrap();
        assert_eq!(read(&item).data, ParticleData::Item(apple));
    }

    #[test]
    fn an_unknown_type_is_an_error() {
        let mut body = Vec::new();
        write_var_int(&mut body, 9999).unwrap();
        assert!(Reader::new(&body).particle().is_err());
    }
}
