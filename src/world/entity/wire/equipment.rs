//! `ClientboundSetEquipmentPacket`: what an entity holds and wears.

use anyhow::Result;
use pumpkin_data::item_stack::ItemStack;

use crate::network::protocol::packet::Reader;

/// `EquipmentSlot`, by its network id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

impl EquipmentSlot {
    pub fn from_id(id: u8) -> Option<Self> {
        Some(match id {
            0 => Self::MainHand,
            1 => Self::OffHand,
            2 => Self::Feet,
            3 => Self::Legs,
            4 => Self::Chest,
            5 => Self::Head,
            6 => Self::Body,
            7 => Self::Saddle,
            _ => return None,
        })
    }
}

/// The slots in the packet.
pub fn read_set_equipment(r: &mut Reader<'_>) -> Result<(i32, Vec<(EquipmentSlot, ItemStack)>)> {
    let id = r.var_int()?;
    let mut slots = Vec::new();
    loop {
        let byte = r.u8()?;
        let stack = r.item_stack()?;
        if let Some(slot) = EquipmentSlot::from_id(byte & 0x7F) {
            slots.push((slot, stack));
        }
        if byte & 0x80 == 0 {
            break;
        }
    }
    Ok((id, slots))
}
