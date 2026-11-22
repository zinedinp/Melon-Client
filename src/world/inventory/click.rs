//! `AbstractContainerMenu.clicked` for the player's inventory menu.

use super::{
    ARMOR_SLOTS, CRAFT_SLOTS, Click, ClickType, Drag, HOTBAR_SLOTS, Inventory, MAIN_SLOTS,
    OFFHAND_SLOT, OUTSIDE, RESULT_SLOT, SLOT_COUNT, SWAP_OFFHAND_BUTTON, can_item_quick_replace,
    empty, equipment_slot, may_place, quick_craft_place_count, shrink, slot_max, split_slot,
    stacks_equal,
};
use pumpkin_data::item_stack::ItemStack;

impl Inventory {
    /// Runs a click locally and returns what to send.
    pub fn click(&mut self, slot: i16, button: i8, kind: ClickType, creative: bool) -> Click {
        let before = self.slots.clone();
        let index = usize::try_from(slot).ok().filter(|&i| i < SLOT_COUNT);
        if kind != ClickType::QuickCraft {
            self.drag = None;
        }
        match kind {
            ClickType::Pickup => self.pickup(slot, button),
            ClickType::QuickMove => {
                if let Some(index) = index {
                    self.quick_move(index);
                }
            }
            ClickType::Swap => {
                if let Some(index) = index {
                    self.swap(index, button);
                }
            }
            ClickType::Clone => {
                if let Some(index) = index
                    && creative
                    && self.carried.is_empty()
                    && !self.slots[index].is_empty()
                {
                    let stack = &self.slots[index];
                    self.carried = stack.copy_with_count(stack.get_max_stack_size());
                }
            }
            ClickType::Throw => {
                if let Some(index) = index
                    && self.carried.is_empty()
                {
                    let stack = &mut self.slots[index];
                    let count = if button == 1 { stack.item_count } else { 1 };
                    shrink(stack, count);
                }
            }
            ClickType::QuickCraft => self.quick_craft(slot, button, creative),
            ClickType::PickupAll => {
                if let Some(index) = index {
                    self.pickup_all(index, button);
                }
            }
        }
        let changed = (0..SLOT_COUNT)
            .filter(|&i| !stacks_equal(&before[i], &self.slots[i]))
            .map(|i| i as i16)
            .collect();
        Click {
            slot,
            button,
            kind,
            changed,
        }
    }

    /// `doClick`'s `ContainerInput.PICKUP` branch.
    pub(super) fn pickup(&mut self, slot: i16, button: i8) {
        if slot == OUTSIDE {
            let count = if button == 0 {
                self.carried.item_count
            } else {
                1
            };
            shrink(&mut self.carried, count);
            return;
        }
        let Ok(index) = usize::try_from(slot) else {
            return;
        };
        if index >= SLOT_COUNT || index == RESULT_SLOT {
            return;
        }
        let stack = &mut self.slots[index];
        let carried = &mut self.carried;
        if stack.is_empty() {
            if !carried.is_empty() && may_place(index, carried) {
                let amount = if button == 0 { carried.item_count } else { 1 };
                let amount = amount.min(slot_max(index, carried));
                *stack = split_slot(carried, amount);
            }
        } else if carried.is_empty() {
            let amount = if button == 0 {
                stack.item_count
            } else {
                stack.item_count.div_ceil(2)
            };
            *carried = split_slot(stack, amount);
        } else if may_place(index, carried) {
            if stack.are_items_and_components_equal(carried) {
                let room = slot_max(index, carried).saturating_sub(stack.item_count);
                let amount = if button == 0 {
                    carried.item_count.min(room)
                } else {
                    1.min(room)
                };
                stack.item_count += amount;
                shrink(carried, amount);
            } else if carried.item_count <= slot_max(index, carried) {
                std::mem::swap(stack, carried);
            }
        } else if stack.are_items_and_components_equal(carried) {
            let room = carried
                .get_max_stack_size()
                .saturating_sub(carried.item_count);
            let amount = stack.item_count.min(room);
            carried.item_count += amount;
            shrink(stack, amount);
        }
    }

    /// `ContainerInput.SWAP`.
    pub(super) fn swap(&mut self, index: usize, button: i8) {
        let target = match button {
            0..=8 => HOTBAR_SLOTS.start + button as usize,
            SWAP_OFFHAND_BUTTON => OFFHAND_SLOT,
            _ => return,
        };
        if index == RESULT_SLOT || index == target {
            return;
        }
        let incoming = &self.slots[target];
        if !incoming.is_empty() && !may_place(index, incoming) {
            return;
        }
        self.slots.swap(index, target);
    }

    /// `InventoryMenu.quickMoveStack` (shift-click).
    pub(super) fn quick_move(&mut self, index: usize) {
        if index == RESULT_SLOT || self.slots[index].is_empty() {
            return;
        }
        let equipment = equipment_slot(&self.slots[index]);
        let targets = if CRAFT_SLOTS.contains(&index) || ARMOR_SLOTS.contains(&index) {
            MAIN_SLOTS.start..HOTBAR_SLOTS.end
        } else if let Some(slot) = equipment.filter(|&s| self.slots[s].is_empty()) {
            slot..slot + 1
        } else if MAIN_SLOTS.contains(&index) {
            HOTBAR_SLOTS
        } else if HOTBAR_SLOTS.contains(&index) {
            MAIN_SLOTS
        } else {
            MAIN_SLOTS.start..HOTBAR_SLOTS.end
        };
        let mut moving = std::mem::replace(&mut self.slots[index], empty());
        self.move_into(&mut moving, targets);
        self.slots[index] = moving;
    }

    /// `AbstractContainerMenu.moveItemStackTo`.
    pub(super) fn move_into(&mut self, moving: &mut ItemStack, targets: std::ops::Range<usize>) {
        if moving.is_stackable() {
            for i in targets.clone() {
                if moving.is_empty() {
                    return;
                }
                let max = slot_max(i, moving);
                let target = &mut self.slots[i];
                if !target.is_empty() && target.are_items_and_components_equal(moving) {
                    let amount = moving.item_count.min(max.saturating_sub(target.item_count));
                    target.item_count += amount;
                    shrink(moving, amount);
                }
            }
        }
        if moving.is_empty() {
            return;
        }
        for i in targets {
            if self.slots[i].is_empty() && may_place(i, moving) {
                let count = moving.item_count.min(slot_max(i, moving));
                self.slots[i] = split_slot(moving, count);
                return;
            }
        }
    }

    /// `doClick`'s `ContainerInput.QUICK_CRAFT`.
    pub(super) fn quick_craft(&mut self, slot: i16, button: i8, creative: bool) {
        let stage = button & 3;
        let kind = (button >> 2) & 3;
        match stage {
            0 => {
                let valid = kind == 0 || kind == 1 || (kind == 2 && creative);
                self.drag = (valid && !self.carried.is_empty()).then(|| Drag {
                    kind,
                    slots: Vec::new(),
                });
            }
            1 => {
                let Some(drag) = &self.drag else { return };
                let (kind, count) = (drag.kind, drag.slots.len());
                if let Ok(index) = usize::try_from(slot)
                    && !drag.slots.contains(&index)
                    && self.can_drag_to(index, kind, count)
                    && let Some(drag) = &mut self.drag
                {
                    drag.slots.push(index);
                }
            }
            _ => {
                let Some(drag) = self.drag.take() else { return };
                if self.carried.is_empty() || drag.slots.is_empty() {
                    return;
                }
                if drag.slots.len() == 1 {
                    self.pickup(drag.slots[0] as i16, drag.kind);
                    return;
                }
                let source = self.carried.clone();
                let count = drag.slots.len();
                let mut remaining = i32::from(source.item_count);
                for &index in &drag.slots {
                    if !(can_item_quick_replace(&self.slots[index], &source, true)
                        && may_place(index, &source)
                        && (drag.kind == 2 || usize::from(self.carried.item_count) >= count))
                    {
                        continue;
                    }
                    let carry = i32::from(self.slots[index].item_count);
                    let max = i32::from(slot_max(index, &source).min(source.get_max_stack_size()));
                    let placed =
                        (quick_craft_place_count(count, drag.kind, &source) + carry).min(max);
                    remaining -= placed - carry;
                    self.slots[index] = source.copy_with_count(placed as u8);
                }
                self.carried = if remaining <= 0 {
                    empty()
                } else {
                    source.copy_with_count(remaining as u8)
                };
            }
        }
    }

    /// `doClick`'s `ContainerInput.PICKUP_ALL` (double click).
    pub(super) fn pickup_all(&mut self, index: usize, button: i8) {
        let clicked = &self.slots[index];
        if self.carried.is_empty() || !clicked.is_empty() {
            return;
        }
        let order: Vec<usize> = if button == 0 {
            (0..SLOT_COUNT).collect()
        } else {
            (0..SLOT_COUNT).rev().collect()
        };
        for pass in 0..2 {
            for &i in &order {
                let max = self.carried.get_max_stack_size();
                if self.carried.item_count >= max {
                    return;
                }
                if i == RESULT_SLOT {
                    continue;
                }
                let target = &mut self.slots[i];
                if target.is_empty()
                    || !can_item_quick_replace(target, &self.carried, true)
                    || (pass == 0 && target.item_count == target.get_max_stack_size())
                {
                    continue;
                }
                let amount = target.item_count.min(max - self.carried.item_count);
                self.carried.item_count += amount;
                shrink(target, amount);
            }
        }
    }
}
