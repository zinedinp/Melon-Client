//! The packets about entities, decoded into [`EntityPacket`]s: spawning

mod data;
mod equipment;
mod movement;
mod spawn;

use anyhow::Result;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::packet::clientbound::play as cb;

pub use data::DataValue;
pub use equipment::EquipmentSlot;
pub use movement::{MoveEntity, SetPosition, relative};
pub use spawn::AddEntity;

use crate::network::protocol::packet::Reader;
use crate::world::player::hands::{Hand, Swing};

/// `Entity.FLAG_ON_FIRE`.
pub const FLAG_ON_FIRE: u8 = 0;
/// `MobEffects.NAUSEA` registry id.
pub const NAUSEA_ID: i32 = 8;

/// `Entity.DATA_SHARED_FLAGS_ID`' index.
pub const SHARED_FLAGS_INDEX: u8 = 0;
/// `LivingEntity.DATA_LIVING_ENTITY_FLAGS`' index, and its bits: using an
pub const LIVING_FLAGS_INDEX: u8 = 8;
pub const LIVING_USING_ITEM: u8 = 1;
pub const LIVING_OFF_HAND: u8 = 2;

/// `EntityEvent.DEATH`.
pub const EVENT_DEATH: u8 = 3;

/// One packet about an entity.
#[derive(Clone)]
pub enum EntityPacket {
    Add(AddEntity),
    Remove(Vec<i32>),
    /// `ClientboundTakeItemEntityPacket`: `taker` picks up `amount` of
    Take {
        item: i32,
        taker: i32,
        amount: i32,
    },
    Move(MoveEntity),
    SetPosition(SetPosition),
    /// `ClientboundSetEntityMotionPacket` for an entity: its velocity per
    Motion {
        id: i32,
        velocity: [f64; 3],
    },
    /// `ClientboundRotateHeadPacket`: the head's yaw, degrees.
    HeadRotation {
        id: i32,
        yaw: f32,
    },
    /// `ClientboundSetEntityDataPacket`.
    Data {
        id: i32,
        values: Vec<(u8, DataValue)>,
    },
    /// `ClientboundSetEquipmentPacket`.
    Equipment {
        id: i32,
        slots: Vec<(EquipmentSlot, ItemStack)>,
    },
    /// `ClientboundHurtAnimationPacket`.
    Hurt {
        id: i32,
        /// Where the blow came from, relative to the entity's look
        yaw: f32,
    },
    /// `ClientboundDamageEventPacket`: `damage` is the damage type's
    Damage {
        id: i32,
        damage: i32,
    },
    /// `ClientboundSwingAnimationPacket`: a hand swings, as its item does.
    Swing {
        id: i32,
        hand: Hand,
        swing: Swing,
    },
    /// `ClientboundEntityEventPacket`, except the permission levels.
    Event {
        id: i32,
        event: u8,
    },
    /// `ClientboundSetEntityLinkPacket`: `id` is on a lead held by
    Leash {
        id: i32,
        holder: Option<i32>,
    },
    /// `ClientboundSetPassengersPacket`: everyone now riding `id`, in seat
    Passengers {
        id: i32,
        passengers: Vec<i32>,
    },
    /// `ClientboundUpdateAttributesPacket`: the attributes the server sets,
    Attributes {
        id: i32,
        values: Vec<(i32, f64)>,
        /// The same attributes with their base and modifiers, as the
        snapshots: Vec<AttributeSnapshot>,
    },
}

/// One attribute of `ClientboundUpdateAttributesPacket`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSnapshot {
    pub attribute: i32,
    pub base: f64,
    /// `(id, amount, operation)`; operation 0 adds, 1 multiplies the base,
    pub modifiers: Vec<(String, f64, i8)>,
}

/// `MAX_HEALTH`, `ARMOR` and the other attribute ids Melon reads.
pub const ATTRIBUTE_MAX_HEALTH: i32 = pumpkin_data::attributes::Attributes::MAX_HEALTH.id as i32;
pub const ATTRIBUTE_ARMOR: i32 = pumpkin_data::attributes::Attributes::ARMOR.id as i32;
pub const ATTRIBUTE_MOVEMENT_SPEED: i32 =
    pumpkin_data::attributes::Attributes::MOVEMENT_SPEED.id as i32;
pub const ATTRIBUTE_JUMP_STRENGTH: i32 =
    pumpkin_data::attributes::Attributes::JUMP_STRENGTH.id as i32;
pub const ATTRIBUTE_STEP_HEIGHT: i32 = pumpkin_data::attributes::Attributes::STEP_HEIGHT.id as i32;
pub const ATTRIBUTE_NAME_TAG_DISTANCE: i32 =
    pumpkin_data::attributes::Attributes::NAME_TAG_DISTANCE.id as i32;
pub const ATTRIBUTE_BELOW_NAME_DISTANCE: i32 =
    pumpkin_data::attributes::Attributes::BELOW_NAME_DISTANCE.id as i32;
pub const ATTRIBUTE_SCALE: i32 = pumpkin_data::attributes::Attributes::SCALE.id as i32;

/// `AttributeInstance.calculateValue`: the base plus the added amounts,
pub fn attribute_value(base: f64, modifiers: &[(i8, f64)]) -> f64 {
    let sum = |op: i8| -> f64 { modifiers.iter().filter(|m| m.0 == op).map(|m| m.1).sum() };
    let base = base + sum(0);
    let mut value = base + base * sum(1);
    for (_, amount) in modifiers.iter().filter(|m| m.0 == 2) {
        value *= 1.0 + amount;
    }
    value
}

fn read_attributes(r: &mut Reader<'_>) -> Result<EntityPacket> {
    let id = r.var_int()?;
    let count = r.var_int()?.clamp(0, 256);
    let mut values = Vec::new();
    let mut snapshots = Vec::new();
    for _ in 0..count {
        let attribute = r.var_int()?;
        let base = r.f64_be()?;
        let modifiers = r.var_int()?.clamp(0, 256);
        let mut list = Vec::new();
        let mut named = Vec::new();
        for _ in 0..modifiers {
            let modifier_id = r.string()?;
            let amount = r.f64_be()?;
            let operation = r.u8()? as i8;
            list.push((operation, amount));
            named.push((modifier_id.to_string(), amount, operation));
        }
        values.push((attribute, attribute_value(base, &list)));
        snapshots.push(AttributeSnapshot {
            attribute,
            base,
            modifiers: named,
        });
    }
    Ok(EntityPacket::Attributes {
        id,
        values,
        snapshots,
    })
}

/// The entity packet `pid` is, if it is one.
pub fn decode(pid: i32, r: &mut Reader<'_>) -> Result<Option<EntityPacket>> {
    Ok(Some(match pid {
        p if p == cb::ADD_ENTITY.0 => EntityPacket::Add(spawn::read_add_entity(r)?),
        p if p == cb::REMOVE_ENTITIES.0 => EntityPacket::Remove(spawn::read_remove_entities(r)?),
        p if p == cb::TAKE_ITEM_ENTITY.0 => EntityPacket::Take {
            item: r.var_int()?,
            taker: r.var_int()?,
            amount: r.var_int()?,
        },
        p if p == cb::MOVE_ENTITY_POS.0 => EntityPacket::Move(movement::read_move_pos(r, false)?),
        p if p == cb::MOVE_ENTITY_POS_ROT.0 => {
            EntityPacket::Move(movement::read_move_pos(r, true)?)
        }
        p if p == cb::MOVE_ENTITY_ROT.0 => EntityPacket::Move(movement::read_move_rot(r)?),
        p if p == cb::ENTITY_POSITION_SYNC.0 => {
            EntityPacket::SetPosition(movement::read_position_sync(r)?)
        }
        p if p == cb::TELEPORT_ENTITY.0 => EntityPacket::SetPosition(movement::read_teleport(r)?),
        p if p == cb::ROTATE_HEAD.0 => {
            let (id, yaw) = movement::read_rotate_head(r)?;
            EntityPacket::HeadRotation { id, yaw }
        }
        p if p == cb::SET_ENTITY_DATA.0 => EntityPacket::Data {
            id: r.var_int()?,
            values: data::read_entity_data(r),
        },
        p if p == cb::SET_EQUIPMENT.0 => {
            let (id, slots) = equipment::read_set_equipment(r)?;
            EntityPacket::Equipment { id, slots }
        }
        p if p == cb::HURT_ANIMATION.0 => EntityPacket::Hurt {
            id: r.var_int()?,
            yaw: r.f32_be().unwrap_or(0.0),
        },
        p if p == cb::DAMAGE_EVENT.0 => {
            let id = r.var_int()?;
            let damage = r.var_int()? - 1;
            EntityPacket::Damage { id, damage }
        }
        p if p == cb::SWING_ANIMATION.0 => {
            let id = r.var_int()?;
            let hand = if r.var_int()? == 1 {
                Hand::Off
            } else {
                Hand::Main
            };
            let kind = r.var_int()?;
            let duration = r.var_int()?;
            EntityPacket::Swing {
                id,
                hand,
                swing: Swing {
                    whack: kind == 1,
                    duration,
                },
            }
        }
        p if p == cb::SET_ENTITY_LINK.0 => {
            let id = r.i32_be()?;
            let holder = r.i32_be()?;
            EntityPacket::Leash {
                id,
                holder: (holder != 0).then_some(holder),
            }
        }
        p if p == cb::UPDATE_ATTRIBUTES.0 => read_attributes(r)?,
        p if p == cb::SET_PASSENGERS.0 => {
            let id = r.var_int()?;
            let count = r.var_int()?.clamp(0, 4096);
            let mut passengers = Vec::with_capacity(count as usize);
            for _ in 0..count {
                passengers.push(r.var_int()?);
            }
            EntityPacket::Passengers { id, passengers }
        }
        _ => return Ok(None),
    }))
}

/// The flag bytes of one metadata list the local player reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityFlags {
    /// `Entity.DATA_SHARED_FLAGS_ID` (index 0).
    pub shared: Option<u8>,
    /// `LivingEntity.DATA_LIVING_ENTITY_FLAGS` (index 8).
    pub living: Option<u8>,
}

impl EntityFlags {
    /// The flag bytes among `values`.
    pub fn of(values: &[(u8, DataValue)]) -> Self {
        let find = |index| {
            values
                .iter()
                .find(|(i, _)| *i == index)
                .and_then(|(_, v)| v.byte())
        };
        Self {
            shared: find(SHARED_FLAGS_INDEX),
            living: find(LIVING_FLAGS_INDEX),
        }
    }
}

/// `ClientboundUpdateMobEffectPacket`.
pub fn read_update_effect(r: &mut Reader<'_>) -> Result<(i32, i32, i32, i32, u8)> {
    let entity_id = r.var_int()?;
    let effect = r.var_int()?;
    let amplifier = r.var_int()?;
    let duration = r.var_int()?;
    let flags = r.u8()?;
    Ok((entity_id, effect, amplifier, duration, flags))
}

/// `ClientboundRemoveMobEffectPacket`.
pub fn read_remove_effect(r: &mut Reader<'_>) -> Result<(i32, i32)> {
    Ok((r.var_int()?, r.var_int()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_protocol::ClientPacket;
    use pumpkin_util::version::JavaMinecraftVersion;

    fn written(packet: &impl ClientPacket) -> Vec<u8> {
        let mut out = Vec::new();
        packet
            .write_packet_data(&mut out, &JavaMinecraftVersion::V_26_3)
            .unwrap();
        out
    }

    fn decoded(pid: pumpkin_data::packet::PacketId, body: &[u8]) -> EntityPacket {
        decode(pid.0, &mut Reader::new(body)).unwrap().unwrap()
    }

    #[test]
    fn reads_the_flags_after_other_values() {
        let mut r = Reader::new(&[1, 1, 0xAC, 0x02, 8, 0, 3, 0, 0, 1, 0xFF]);
        let flags = EntityFlags::of(&data::read_entity_data(&mut r));
        assert_eq!(flags.living, Some(LIVING_USING_ITEM | LIVING_OFF_HAND));
        assert_eq!(flags.shared.unwrap() & (1 << FLAG_ON_FIRE), 1);
    }

    #[test]
    fn pumpkins_swing_packet_round_trips() {
        use pumpkin_protocol::java::client::play::CSwingArm;
        let swing = decoded(
            cb::SWING_ANIMATION,
            &written(&CSwingArm::new(9.into(), true)),
        );
        let EntityPacket::Swing { id, hand, swing } = swing else {
            panic!()
        };
        assert_eq!((id, hand), (9, Hand::Off));
        assert_eq!((swing.whack, swing.duration), (true, 6));
    }

    #[test]
    fn pumpkins_leash_packet_round_trips() {
        use pumpkin_protocol::java::client::play::CSetEntityLink;
        let tied = decoded(
            cb::SET_ENTITY_LINK,
            &written(&CSetEntityLink::new(7, 12, true)),
        );
        assert!(matches!(
            tied,
            EntityPacket::Leash {
                id: 7,
                holder: Some(12)
            }
        ));
        let loose = decoded(
            cb::SET_ENTITY_LINK,
            &written(&CSetEntityLink::new(7, 0, false)),
        );
        assert!(matches!(
            loose,
            EntityPacket::Leash {
                id: 7,
                holder: None
            }
        ));
    }

    #[test]
    fn attributes_come_out_as_their_final_values() {
        use pumpkin_protocol::VarInt;
        use pumpkin_protocol::java::client::play::{
            AttributeModifier, CUpdateAttributes, Property,
        };
        let packet = CUpdateAttributes::new(
            VarInt(3),
            vec![
                Property::new(VarInt(ATTRIBUTE_MAX_HEALTH), 30.0, vec![]),
                Property::new(
                    VarInt(ATTRIBUTE_ARMOR),
                    0.0,
                    vec![AttributeModifier::new("a:b".into(), 8.0, 0)],
                ),
            ],
        );
        let EntityPacket::Attributes { id, values, .. } =
            decoded(cb::UPDATE_ATTRIBUTES, &written(&packet))
        else {
            panic!()
        };
        assert_eq!(id, 3);
        assert_eq!(
            values,
            vec![(ATTRIBUTE_MAX_HEALTH, 30.0), (ATTRIBUTE_ARMOR, 8.0)]
        );
        assert_eq!(attribute_value(20.0, &[(1, 0.5), (2, 0.1)]), 30.0 * 1.1);
    }

    #[test]
    fn pumpkins_passenger_packet_round_trips() {
        use pumpkin_protocol::VarInt;
        use pumpkin_protocol::java::client::play::CSetPassengers;
        let riders = [VarInt(5), VarInt(300)];
        let packet = decoded(
            cb::SET_PASSENGERS,
            &written(&CSetPassengers::new(12.into(), &riders)),
        );
        let EntityPacket::Passengers { id, passengers } = packet else {
            panic!()
        };
        assert_eq!((id, passengers), (12, vec![5, 300]));
        let EntityPacket::Passengers { passengers, .. } = decoded(
            cb::SET_PASSENGERS,
            &written(&CSetPassengers::new(12.into(), &[])),
        ) else {
            panic!()
        };
        assert!(passengers.is_empty());
    }

    #[test]
    fn pumpkins_spawn_packet_round_trips() {
        use pumpkin_protocol::java::client::play::CSpawnEntity;
        use pumpkin_util::math::vector3::Vector3;
        let uuid = uuid::Uuid::from_u128(7);
        let packet = CSpawnEntity::new(
            42.into(),
            uuid,
            101.into(),
            Vector3::new(1.5, 64.0, -3.25),
            -45.0,
            90.0,
            180.0,
            0.into(),
            Vector3::new(0.0, 0.0, 0.0),
        );
        let EntityPacket::Add(add) = decoded(cb::ADD_ENTITY, &written(&packet)) else {
            panic!()
        };
        assert_eq!((add.id, add.uuid, add.kind), (42, uuid, 101));
        assert_eq!(add.pos, [1.5, 64.0, -3.25]);
        assert!((add.yaw - 90.0).abs() < 1.5, "{}", add.yaw);
        assert!((add.pitch + 45.0).abs() < 1.5, "{}", add.pitch);
    }

    #[test]
    fn pumpkins_relative_moves_round_trip() {
        use pumpkin_protocol::java::client::play::{CUpdateEntityPos, CUpdateEntityPosRot};
        use pumpkin_util::math::vector3::Vector3;
        let packet = CUpdateEntityPos::new(5.into(), Vector3::new(4096, -2048, 1), true);
        let EntityPacket::Move(m) = decoded(cb::MOVE_ENTITY_POS, &written(&packet)) else {
            panic!()
        };
        assert_eq!(m.steps, vec![[4096, -2048, 1]]);
        assert!(m.on_ground && m.rotation.is_none());
        let packet = CUpdateEntityPosRot::new(5.into(), Vector3::new(0, 0, 0), 64, 0, false);
        let EntityPacket::Move(m) = decoded(cb::MOVE_ENTITY_POS_ROT, &written(&packet)) else {
            panic!()
        };
        assert_eq!(m.rotation, Some([90.0, 0.0]));
    }

    #[test]
    fn a_stepped_delta_keeps_every_step() {
        let mut body = vec![1, 1 | 2 << 1];
        for (ticks, x) in [(1u8, 100i16), (2, 50)] {
            body.push(ticks);
            body.extend_from_slice(&x.to_be_bytes());
            body.extend_from_slice(&[0, 0, 0, 0]);
        }
        let EntityPacket::Move(m) = decoded(cb::MOVE_ENTITY_POS, &body) else {
            panic!()
        };
        assert_eq!(m.steps, vec![[100, 0, 0], [50, 0, 0]]);
    }

    #[test]
    fn pumpkins_position_sync_round_trips() {
        use pumpkin_protocol::java::client::play::CEntityPositionSync;
        use pumpkin_util::math::vector3::Vector3;
        let packet = CEntityPositionSync::new(
            9.into(),
            Vector3::new(10.0, 70.0, 20.0),
            Vector3::new(0.0, 0.0, 0.0),
            45.0,
            10.0,
            true,
        );
        let EntityPacket::SetPosition(p) = decoded(cb::ENTITY_POSITION_SYNC, &written(&packet))
        else {
            panic!()
        };
        assert_eq!(
            (p.id, p.pos, p.yaw, p.pitch),
            (9, [10.0, 70.0, 20.0], 45.0, 10.0)
        );
    }
}
