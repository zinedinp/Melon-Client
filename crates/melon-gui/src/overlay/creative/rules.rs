//! `CreativeModeInventoryScreen.slotClicked`.

use super::{CreativeChanges, CreativeOverlay, IMAGE_HEIGHT, IMAGE_WIDTH, Slot};
use crate::overlay::container_input::SlotClick;
use melon_world::creative_tabs::TabType;
use melon_world::inventory::ClickType;
use melon_world::inventory::{
    self, HOTBAR_SLOTS, Inventory, OUTSIDE, can_item_quick_replace, empty,
};
use pumpkin_data::item_stack::ItemStack;

impl CreativeOverlay {
    /// `CreativeModeInventoryScreen.slotClicked`.
    pub(super) fn slot_clicked(
        &mut self,
        inventory: &mut Inventory,
        click: SlotClick,
    ) -> CreativeChanges {
        let mut changes = CreativeChanges::default();
        let before = inventory.slots().to_vec();
        let slot = click.slot.and_then(|i| self.slot_kind(i));
        let quick = click.kind == ClickType::QuickMove;
        let kind = if click.slot_id == OUTSIDE && click.kind == ClickType::Pickup {
            ClickType::Throw
        } else {
            click.kind
        };
        let tab = self.tab_type();
        let button = click.button;
        if slot.is_none() && tab != TabType::Inventory && kind != ClickType::QuickCraft {
            self.click_outside(inventory, button, &mut changes);
            return changes;
        }
        if let Some(Slot::Picker(i)) = slot
            && self.picker(i).1
        {
            return changes;
        }
        if slot == Some(Slot::Destroy) && quick {
            for stack in inventory.slots_mut() {
                *stack = empty().into();
            }
        } else if tab == TabType::Inventory {
            click_inventory_tab(inventory, &click, slot, kind, &mut changes);
        } else if kind != ClickType::QuickCraft
            && let Some(Slot::Picker(i)) = slot
        {
            let clicked = self.picker(i).0.clone();
            click_picker(inventory, &clicked, button, kind, quick, &mut changes);
        } else {
            click_picker_menu(inventory, &click, slot, kind, &mut changes);
        }
        for (menu, (old, new)) in before.iter().zip(inventory.slots()).enumerate().skip(1) {
            if !inventory::stacks_equal(old, new) {
                changes.sets.push((menu as i16, ItemStack::clone(new)));
            }
        }
        changes
    }

    /// A click outside the window with a stack on the cursor drops it.
    fn click_outside(&self, inventory: &mut Inventory, button: i8, changes: &mut CreativeChanges) {
        if !inventory.carried().is_empty() && self.has_clicked_outside {
            if button == 0 {
                let carried = inventory.take_carried();
                changes.sets.push((-1, carried));
            } else if button == 1 {
                let one = inventory.carried_mut().split(1);
                if inventory.carried().item_count == 0 {
                    inventory.set_carried(empty());
                }
                changes.sets.push((-1, one));
            }
        }
    }

    /// `hasClickedOutside`, remembered for the next `slotClicked`.
    pub(super) fn note_outside(&mut self) {
        let (mx, my) = self.cursor;
        let outside = mx < self.left
            || my < self.top
            || mx >= self.left + IMAGE_WIDTH
            || my >= self.top + IMAGE_HEIGHT;
        self.has_clicked_outside = outside && self.tab_at(self.cursor).is_none();
    }
}

/// A click with the Survival Inventory tab open.
fn click_inventory_tab(
    inventory: &mut Inventory,
    click: &SlotClick,
    slot: Option<Slot>,
    kind: ClickType,
    changes: &mut CreativeChanges,
) {
    let button = click.button;
    match slot {
        Some(Slot::Destroy) => inventory.set_carried(empty()),
        Some(Slot::Player(menu)) if kind == ClickType::Throw => {
            let stack = &mut inventory.slots_mut()[menu];
            if !stack.is_empty() {
                let count = if button == 0 {
                    1
                } else {
                    stack.get_max_stack_size()
                };
                let count = count.min(stack.item_count);
                let dropped = stack.split(count);
                if stack.item_count == 0 {
                    *stack = empty().into();
                }
                changes.sets.push((-1, dropped));
            }
        }
        None if kind == ClickType::Throw && click.slot_id == OUTSIDE => {
            if !inventory.carried().is_empty() {
                let carried = inventory.take_carried();
                changes.sets.push((-1, carried));
            }
        }
        _ => {
            let target = match slot {
                Some(Slot::Player(menu)) => menu as i16,
                _ => click.slot_id,
            };
            inventory.click(target, button, kind, true);
        }
    }
}

/// A click on an item of the picker.
fn click_picker(
    inventory: &mut Inventory,
    clicked: &ItemStack,
    button: i8,
    kind: ClickType,
    quick: bool,
    changes: &mut CreativeChanges,
) {
    let (slots, carried) = inventory.slots_and_carried_mut();
    match kind {
        ClickType::Swap => {
            if !clicked.is_empty() {
                let target = match button {
                    0..=8 => Some(HOTBAR_SLOTS.start + button as usize),
                    inventory::SWAP_OFFHAND_BUTTON => Some(inventory::OFFHAND_SLOT),
                    _ => None,
                };
                if let Some(target) = target {
                    slots[target] = clicked.copy_with_count(clicked.get_max_stack_size()).into();
                }
            }
        }
        ClickType::Clone => {
            if carried.is_empty() && !clicked.is_empty() {
                *carried = clicked.copy_with_count(clicked.get_max_stack_size()).into();
            }
        }
        ClickType::Throw => {
            if !clicked.is_empty() {
                let count = if button == 0 {
                    1
                } else {
                    clicked.get_max_stack_size()
                };
                changes.sets.push((-1, clicked.copy_with_count(count)));
            }
        }
        _ => {
            if !carried.is_empty()
                && !clicked.is_empty()
                && carried.are_items_and_components_equal(clicked)
            {
                if button == 0 {
                    let max = carried.get_max_stack_size();
                    if quick {
                        carried.item_count = max;
                    } else if carried.item_count < max {
                        carried.item_count += 1;
                    }
                } else {
                    carried.item_count -= 1;
                    if carried.item_count == 0 {
                        *carried = empty().into();
                    }
                }
            } else if !clicked.is_empty() && carried.is_empty() {
                let count = if quick {
                    clicked.get_max_stack_size()
                } else {
                    clicked.item_count
                };
                *carried = clicked.copy_with_count(count).into();
            } else if button == 0 {
                *carried = empty().into();
            } else if !carried.is_empty() {
                carried.item_count -= 1;
                if carried.item_count == 0 {
                    *carried = empty().into();
                }
            }
        }
    }
}

/// A click on the hotbar row or the destroy slot while a picker tab is open.
fn click_picker_menu(
    inventory: &mut Inventory,
    click: &SlotClick,
    slot: Option<Slot>,
    kind: ClickType,
    changes: &mut CreativeChanges,
) {
    let button = click.button;
    let target = match slot {
        Some(Slot::Player(menu)) => menu as i16,
        _ => click.slot_id,
    };
    match kind {
        ClickType::QuickMove => {
            if let Some(Slot::Player(menu)) = slot {
                inventory.slots_mut()[menu] = empty().into();
            }
        }
        ClickType::Throw => {
            if let Some(Slot::Player(menu)) = slot
                && inventory.carried().is_empty()
            {
                let stack = &mut inventory.slots_mut()[menu];
                if !stack.is_empty() {
                    let count = if button == 0 { 1 } else { stack.item_count };
                    let dropped = stack.split(count);
                    if stack.item_count == 0 {
                        *stack = empty().into();
                    }
                    changes.sets.push((-1, dropped));
                }
            }
        }
        ClickType::PickupAll => pickup_all_from_hotbar(inventory),
        _ => {
            inventory.click(target, button, kind, true);
        }
    }
}

/// `PICKUP_ALL` on the item picker menu, whose only player slots are the hotbar.
pub(super) fn pickup_all_from_hotbar(inventory: &mut Inventory) {
    let (slots, carried) = inventory.slots_and_carried_mut();
    if carried.is_empty() {
        return;
    }
    for pass in 0..2 {
        for slot in HOTBAR_SLOTS {
            let max = carried.get_max_stack_size();
            if carried.item_count >= max {
                return;
            }
            let target = &mut slots[slot];
            if target.is_empty()
                || !can_item_quick_replace(target, carried, true)
                || (pass == 0 && target.item_count == target.get_max_stack_size())
            {
                continue;
            }
            let amount = target.item_count.min(max - carried.item_count);
            carried.item_count += amount;
            target.item_count -= amount;
            if target.item_count == 0 {
                *target = empty().into();
            }
        }
    }
}
