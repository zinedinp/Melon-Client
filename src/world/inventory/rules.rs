//! Item rules the clicks follow.

use super::{ARMOR_SLOTS, HOTBAR_SLOTS, OFFHAND_SLOT, RESULT_SLOT};
use pumpkin_data::data_component_impl::{EquipmentSlot, EquippableImpl};
use pumpkin_data::item_stack::ItemStack;

/// The menu slot showing `Inventory` index `index` (0-8 hotbar, 9-35 the rest).
pub fn menu_slot_of_inventory_index(index: i32) -> Option<usize> {
    Some(match index {
        0..=8 => HOTBAR_SLOTS.start + index as usize,
        9..=35 => index as usize,
        36..=39 => ARMOR_SLOTS.end - 1 - (index as usize - 36),
        40 => OFFHAND_SLOT,
        _ => return None,
    })
}

/// A new empty stack.
pub fn empty() -> ItemStack {
    ItemStack::EMPTY.clone()
}

/// Takes `count` items off `stack`, leaving an empty stack at zero.
pub(super) fn shrink(stack: &mut ItemStack, count: u8) {
    stack.item_count = stack.item_count.saturating_sub(count);
    if stack.item_count == 0 {
        *stack = empty();
    }
}

/// `ItemStack.split`, then [`empty`] when nothing is left.
pub(super) fn split_slot(stack: &mut ItemStack, amount: u8) -> ItemStack {
    let taken = stack.split(amount);
    if stack.is_empty() {
        *stack = empty();
    }
    taken
}

/// `LivingEntity.getEquipmentSlotForItem`, as a menu slot.
pub fn equipment_slot(stack: &ItemStack) -> Option<usize> {
    let equippable = stack.get_data_component::<EquippableImpl>()?;
    Some(match equippable.slot {
        EquipmentSlot::Head(_) => 5,
        EquipmentSlot::Chest(_) => 6,
        EquipmentSlot::Legs(_) => 7,
        EquipmentSlot::Feet(_) => 8,
        EquipmentSlot::OffHand(_) => OFFHAND_SLOT,
        _ => return None,
    })
}

/// `Slot.mayPlace`.
pub fn may_place(index: usize, stack: &ItemStack) -> bool {
    if index == RESULT_SLOT {
        return false;
    }
    if ARMOR_SLOTS.contains(&index) {
        return equipment_slot(stack) == Some(index);
    }
    true
}

/// `Slot.getMaxStackSize(stack)`.
pub fn slot_max(index: usize, stack: &ItemStack) -> u8 {
    let max = stack.get_max_stack_size();
    if ARMOR_SLOTS.contains(&index) {
        max.min(1)
    } else {
        max
    }
}

/// `AbstractContainerMenu.canItemQuickReplace`.
pub fn can_item_quick_replace(slot: &ItemStack, stack: &ItemStack, ignore_size: bool) -> bool {
    if slot.is_empty() {
        return true;
    }
    slot.are_items_and_components_equal(stack)
        && u16::from(slot.item_count)
            + if ignore_size {
                0
            } else {
                u16::from(stack.item_count)
            }
            <= u16::from(stack.get_max_stack_size())
}

/// `AbstractContainerMenu.getQuickCraftPlaceCount`.
pub fn quick_craft_place_count(slots: usize, kind: i8, stack: &ItemStack) -> i32 {
    match kind {
        0 => i32::from(stack.item_count) / slots.max(1) as i32,
        1 => 1,
        2 => i32::from(stack.get_max_stack_size()),
        _ => i32::from(stack.item_count),
    }
}

/// Same item, count and components.
pub fn stacks_equal(a: &ItemStack, b: &ItemStack) -> bool {
    (a.is_empty() && b.is_empty())
        || (a.item_count == b.item_count && a.are_items_and_components_equal(b))
}
