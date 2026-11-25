//! What the client sends about the player's state.

use anyhow::Result;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;
use std::io::Write;

use crate::network::protocol::packet;
use crate::network::protocol::varint::write_var_int;

/// `ServerboundSetCreativeModeSlotPacket`.
pub fn write_creative_slot(buf: &mut Vec<u8>, slot: i16, stack: &ItemStack) -> Result<()> {
    buf.write_all(&slot.to_be_bytes())?;
    ItemStackSerializer::from(stack.clone())
        .write_length_prefixed_with_version(
            buf,
            &pumpkin_util::version::JavaMinecraftVersion::V_26_3,
        )
        .map_err(|e| anyhow::anyhow!("item stack: {e}"))
}

/// `ServerboundContainerClickPacket`: `click` with the stacks it `changed`.
pub fn write_container_click(
    buf: &mut Vec<u8>,
    container: i32,
    state_id: i32,
    click: &crate::world::inventory::Click,
    changed: &[(i16, &ItemStack)],
    carried: &ItemStack,
) -> Result<()> {
    write_var_int(buf, container)?;
    write_var_int(buf, state_id)?;
    buf.write_all(&click.slot.to_be_bytes())?;
    buf.write_all(&click.button.to_be_bytes())?;
    write_var_int(buf, click.kind as i32)?;
    write_var_int(buf, changed.len() as i32)?;
    for (slot, stack) in changed {
        buf.write_all(&slot.to_be_bytes())?;
        write_hashed_stack(buf, stack)?;
    }
    write_hashed_stack(buf, carried)
}

/// `HashedStack`.
fn write_hashed_stack(buf: &mut Vec<u8>, stack: &ItemStack) -> Result<()> {
    if stack.is_empty() {
        return packet::write_bool(buf, false);
    }
    packet::write_bool(buf, true)?;
    write_var_int(buf, i32::from(stack.item.id))?;
    write_var_int(buf, i32::from(stack.item_count))?;
    let added: Vec<_> = stack
        .patch
        .iter()
        .filter_map(|(id, data)| Some((id, data.as_ref()?)))
        .collect();
    write_var_int(buf, added.len() as i32)?;
    for (id, data) in added {
        write_var_int(buf, i32::from(id.to_id()))?;
        buf.write_all(&data.get_hash().to_be_bytes())?;
    }
    let removed: Vec<_> = stack
        .patch
        .iter()
        .filter(|(_, data)| data.is_none())
        .collect();
    write_var_int(buf, removed.len() as i32)?;
    for (id, _) in removed {
        write_var_int(buf, i32::from(id.to_id()))?;
    }
    Ok(())
}

/// `ServerboundPlayerActionPacket.Action`, 26.3 numbering.
#[derive(Clone, Copy)]
pub enum PlayerAction {
    StartDestroyBlock = 0,
    ChangeDestroyDirection = 1,
    AbortDestroyBlock = 2,
    StopDestroyBlock = 3,
    DropAllItems = 4,
    DropItem = 5,
    ReleaseUseItem = 6,
    SwapItemWithOffhand = 7,
    /// A left click with a piercing weapon (a spear), in place of an attack.
    Stab = 8,
}

/// `ServerboundPlayerCommandPacket.Action`, 1.21.6+ numbering.
#[derive(Clone, Copy)]
pub enum PlayerCommand {
    StartSprinting = 1,
    StopSprinting = 2,
    /// A jump on a ridden horse, its strength 0-100 in the packet's data.
    StartRidingJump = 3,
    /// The key that opens the inventory of a horse, donkey or llama the player rides.
    OpenInventory = 5,
    /// Jumping in the air with a glider on.
    StartFallFlying = 8,
}

/// `ServerboundPlayerInputPacket`'s flags.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerInput {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
}

impl PlayerInput {
    pub fn bits(self) -> u8 {
        [
            self.forward,
            self.backward,
            self.left,
            self.right,
            self.jump,
            self.sneak,
            self.sprint,
        ]
        .iter()
        .enumerate()
        .fold(0, |bits, (i, &set)| bits | (u8::from(set) << i))
    }
}
