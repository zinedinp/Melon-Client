//! The player's inventory as the server's `InventoryMenu` numbers it.

mod click;
mod container;
pub mod loom;
pub mod merchant;
mod rules;
pub mod stonecutting;

pub use container::{
    ContainerKind, MountLook, OpenContainer, PLAYER_SLOTS as CONTAINER_PLAYER_SLOTS,
};

use pumpkin_data::item_stack::ItemStack;
use rules::{shrink, split_slot};

pub use rules::{
    can_item_quick_replace, empty, equipment_slot, may_place, menu_slot_of_inventory_index,
    quick_craft_place_count, slot_max, stacks_equal,
};

/// `ContainerInput` (was `ClickType`), as the wire numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickType {
    Pickup = 0,
    QuickMove = 1,
    Swap = 2,
    /// Middle click in creative: a full stack of the slot's item.
    Clone = 3,
    Throw = 4,
    /// Dragging the cursor stack over slots.
    QuickCraft = 5,
    /// Double click: gathers matching items onto the cursor.
    PickupAll = 6,
}

/// `AbstractContainerMenu.getQuickcraftMask`.
pub fn quick_craft_mask(stage: i8, kind: i8) -> i8 {
    (stage & 3) | ((kind & 3) << 2)
}

pub const SLOT_COUNT: usize = 46;
pub const RESULT_SLOT: usize = 0;
pub const CRAFT_SLOTS: std::ops::Range<usize> = 1..5;
pub const ARMOR_SLOTS: std::ops::Range<usize> = 5..9;
pub const MAIN_SLOTS: std::ops::Range<usize> = 9..36;
pub const HOTBAR_SLOTS: std::ops::Range<usize> = 36..45;
pub const OFFHAND_SLOT: usize = 45;
/// `AbstractContainerMenu.SLOT_CLICKED_OUTSIDE`.
pub const OUTSIDE: i16 = -999;
/// `Inventory.SLOT_OFFHAND`: the swap button for the offhand (`F`).
pub const SWAP_OFFHAND_BUTTON: i8 = 40;

pub struct Inventory {
    /// The menu's [`SLOT_COUNT`] slots.
    slots: Vec<ItemStack>,
    /// The stack on the mouse cursor.
    carried: ItemStack,
    /// `AbstractContainerMenu.stateId`.
    state_id: i32,
    /// Hotbar slot 0-8 in the main hand (`Inventory.selected`).
    selected: u8,
    /// A drag in progress (`quickcraftStatus`/`quickcraftType`).
    drag: Option<Drag>,
}

/// `AbstractContainerMenu`'s quick craft state.
#[derive(Clone, Debug)]
struct Drag {
    /// 0 split evenly, 1 one each, 2 full stacks (creative).
    kind: i8,
    slots: Vec<usize>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: vec![empty(); SLOT_COUNT],
            carried: empty(),
            state_id: 0,
            selected: 0,
            drag: None,
        }
    }
}

/// A click to send.
pub struct Click {
    pub slot: i16,
    pub button: i8,
    pub kind: ClickType,
    pub changed: Vec<i16>,
}

impl Inventory {
    /// The menu's slots, by menu slot index.
    pub fn slots(&self) -> &[ItemStack] {
        &self.slots
    }

    /// The slots to change in place; their number stays [`SLOT_COUNT`].
    pub fn slots_mut(&mut self) -> &mut [ItemStack] {
        &mut self.slots
    }

    /// The stack on the mouse cursor.
    pub fn carried(&self) -> &ItemStack {
        &self.carried
    }

    pub fn carried_mut(&mut self) -> &mut ItemStack {
        &mut self.carried
    }

    pub fn set_carried(&mut self, stack: ItemStack) {
        self.carried = stack;
    }

    /// The cursor stack, leaving the cursor empty.
    pub fn take_carried(&mut self) -> ItemStack {
        std::mem::replace(&mut self.carried, empty())
    }

    /// The slots and the cursor stack, both to change at once.
    pub fn slots_and_carried_mut(&mut self) -> (&mut [ItemStack], &mut ItemStack) {
        (&mut self.slots, &mut self.carried)
    }

    /// The menu's `stateId`, as the server last set it.
    pub fn state_id(&self) -> i32 {
        self.state_id
    }

    /// The selected hotbar slot, 0-8.
    pub fn selected(&self) -> u8 {
        self.selected
    }

    /// Selects hotbar slot `slot`; ignored unless it is 0-8.
    pub fn select(&mut self, slot: u8) {
        if usize::from(slot) < HOTBAR_SLOTS.len() {
            self.selected = slot;
        }
    }

    pub fn hotbar(&self, index: usize) -> &ItemStack {
        &self.slots[HOTBAR_SLOTS.start + index]
    }

    /// The menu slot of the selected hotbar stack.
    pub fn selected_slot(&self) -> usize {
        HOTBAR_SLOTS.start + usize::from(self.selected)
    }

    pub fn selected_stack(&self) -> &ItemStack {
        &self.slots[self.selected_slot()]
    }

    /// `Q` (whole stack with Control).
    pub fn drop_selected(&mut self, whole_stack: bool) -> bool {
        let slot = self.selected_slot();
        let stack = &mut self.slots[slot];
        if stack.is_empty() {
            return false;
        }
        let count = if whole_stack { stack.item_count } else { 1 };
        shrink(stack, count);
        true
    }

    /// `F`: the selected stack and the offhand change places.
    pub fn swap_offhand(&mut self) {
        let slot = self.selected_slot();
        self.slots.swap(slot, OFFHAND_SLOT);
    }

    /// `ClientboundContainerSetContentPacket` for container 0.
    pub fn set_all(&mut self, state_id: i32, slots: Vec<ItemStack>, carried: ItemStack) {
        self.state_id = state_id;
        for (slot, item) in self.slots.iter_mut().zip(slots) {
            *slot = item;
        }
        self.carried = carried;
    }

    /// `ClientboundContainerSetSlotPacket` for container 0.
    pub fn set_slot(&mut self, state_id: i32, slot: i16, item: ItemStack) {
        self.state_id = state_id;
        if let Some(target) = usize::try_from(slot)
            .ok()
            .and_then(|s| self.slots.get_mut(s))
        {
            *target = item;
        }
    }

    /// `ClientboundSetPlayerInventoryPacket`: `index` is an `Inventory`
    pub fn set_inventory_slot(&mut self, index: i32, item: ItemStack) {
        if let Some(slot) = menu_slot_of_inventory_index(index) {
            self.slots[slot] = item;
        }
    }

    /// The drag in progress: its kind and the slots it covers so far.
    pub fn drag(&self) -> Option<(i8, &[usize])> {
        self.drag.as_ref().map(|d| (d.kind, d.slots.as_slice()))
    }

    /// Whether a drag of `kind` may add `slot` (`mouseDragged`'s test).
    pub fn can_drag_to(&self, slot: usize, kind: i8, already: usize) -> bool {
        let carried = &self.carried;
        !carried.is_empty()
            && (usize::from(carried.item_count) > already || kind == 2)
            && slot < SLOT_COUNT
            && can_item_quick_replace(&self.slots[slot], carried, true)
            && may_place(slot, carried)
    }
}

#[cfg(test)]
mod tests {
    use super::quick_craft_mask;
    use super::*;
    use pumpkin_data::item::Item;

    fn stack(count: u8, item: &'static Item) -> ItemStack {
        ItemStack::new(count, item)
    }

    fn click(inv: &mut Inventory, slot: i16, button: i8, kind: ClickType) -> Click {
        inv.click(slot, button, kind, false)
    }

    #[test]
    fn left_click_picks_up_and_places_a_stack() {
        let mut inv = Inventory::default();
        inv.slots[10] = stack(32, &Item::DIRT);
        let result = click(&mut inv, 10, 0, ClickType::Pickup);
        assert_eq!(result.changed, vec![10]);
        assert_eq!(inv.carried.item_count, 32);
        assert!(inv.slots[10].is_empty());
        click(&mut inv, 20, 0, ClickType::Pickup);
        assert_eq!(inv.slots[20].item_count, 32);
        assert!(inv.carried.is_empty());
    }

    #[test]
    fn right_click_takes_half_and_places_one() {
        let mut inv = Inventory::default();
        inv.slots[10] = stack(5, &Item::DIRT);
        click(&mut inv, 10, 1, ClickType::Pickup);
        assert_eq!(inv.carried.item_count, 3);
        assert_eq!(inv.slots[10].item_count, 2);
        click(&mut inv, 11, 1, ClickType::Pickup);
        assert_eq!(inv.slots[11].item_count, 1);
        assert_eq!(inv.carried.item_count, 2);
    }

    #[test]
    fn stacks_merge_up_to_the_limit() {
        let mut inv = Inventory::default();
        inv.slots[10] = stack(60, &Item::DIRT);
        inv.carried = stack(10, &Item::DIRT);
        click(&mut inv, 10, 0, ClickType::Pickup);
        assert_eq!(inv.slots[10].item_count, 64);
        assert_eq!(inv.carried.item_count, 6);
    }

    #[test]
    fn shift_click_moves_the_hotbar_into_the_inventory() {
        let mut inv = Inventory::default();
        inv.slots[36] = stack(3, &Item::STONE);
        inv.slots[9] = stack(62, &Item::STONE);
        let result = click(&mut inv, 36, 0, ClickType::QuickMove);
        assert_eq!(inv.slots[9].item_count, 64);
        assert_eq!(inv.slots[10].item_count, 1);
        assert!(inv.slots[36].is_empty());
        assert_eq!(result.changed, vec![9, 10, 36]);
    }

    #[test]
    fn shift_click_puts_armor_on_and_takes_it_off() {
        let mut inv = Inventory::default();
        inv.slots[20] = stack(1, &Item::DIAMOND_HELMET);
        click(&mut inv, 20, 0, ClickType::QuickMove);
        assert!(!inv.slots[5].is_empty());
        click(&mut inv, 5, 0, ClickType::QuickMove);
        assert!(!inv.slots[9].is_empty());
        assert!(inv.slots[5].is_empty());
        assert_eq!(inv.slots[5].item.id, Item::AIR.id);
    }

    #[test]
    fn taking_armor_off_leaves_air_not_a_zero_count_piece() {
        let mut inv = Inventory::default();
        inv.slots[5] = stack(1, &Item::DIAMOND_HELMET);
        click(&mut inv, 5, 0, ClickType::Pickup);
        assert!(inv.slots[5].is_empty());
        assert_eq!(inv.slots[5].item.id, Item::AIR.id);
        assert_eq!(inv.carried.item.id, Item::DIAMOND_HELMET.id);
        assert_eq!(inv.carried.item_count, 1);
    }

    #[test]
    fn armor_slots_refuse_other_items() {
        let mut inv = Inventory {
            carried: stack(5, &Item::DIRT),
            ..Inventory::default()
        };
        click(&mut inv, 5, 0, ClickType::Pickup);
        assert!(inv.slots[5].is_empty());
        inv.carried = stack(1, &Item::IRON_BOOTS);
        click(&mut inv, 5, 0, ClickType::Pickup);
        assert!(inv.slots[5].is_empty());
        click(&mut inv, 8, 0, ClickType::Pickup);
        assert!(!inv.slots[8].is_empty());
    }

    #[test]
    fn dragging_splits_the_cursor_stack() {
        let mut inv = Inventory {
            carried: stack(10, &Item::DIRT),
            ..Inventory::default()
        };
        for (slot, stage) in [(OUTSIDE, 0), (10, 1), (11, 1), (12, 1), (OUTSIDE, 2)] {
            click(
                &mut inv,
                slot,
                quick_craft_mask(stage, 0),
                ClickType::QuickCraft,
            );
        }
        assert_eq!(inv.slots[10].item_count, 3);
        assert_eq!(inv.slots[12].item_count, 3);
        assert_eq!(inv.carried.item_count, 1);

        let mut inv = Inventory {
            carried: stack(10, &Item::DIRT),
            ..Inventory::default()
        };
        for (slot, stage) in [(OUTSIDE, 0), (10, 1), (11, 1), (OUTSIDE, 2)] {
            click(
                &mut inv,
                slot,
                quick_craft_mask(stage, 1),
                ClickType::QuickCraft,
            );
        }
        assert_eq!(inv.slots[11].item_count, 1);
        assert_eq!(inv.carried.item_count, 8);
    }

    #[test]
    fn double_click_gathers_partial_stacks_first() {
        let mut inv = Inventory {
            carried: stack(10, &Item::DIRT),
            ..Inventory::default()
        };
        inv.slots[12] = stack(64, &Item::DIRT);
        inv.slots[30] = stack(20, &Item::DIRT);
        click(&mut inv, 11, 0, ClickType::PickupAll);
        assert_eq!(inv.carried.item_count, 64);
        assert!(inv.slots[30].is_empty());
        assert_eq!(inv.slots[12].item_count, 30);
    }

    #[test]
    fn swap_with_the_offhand_and_clone_in_creative() {
        let mut inv = Inventory::default();
        inv.slots[15] = stack(1, &Item::SHIELD);
        click(&mut inv, 15, SWAP_OFFHAND_BUTTON, ClickType::Swap);
        assert!(!inv.slots[OFFHAND_SLOT].is_empty());

        inv.slots[16] = stack(3, &Item::STONE);
        click(&mut inv, 16, 2, ClickType::Clone);
        assert!(inv.carried.is_empty(), "survival can't clone");
        inv.click(16, 2, ClickType::Clone, true);
        assert_eq!(inv.carried.item_count, 64);
    }

    #[test]
    fn inventory_indices_map_to_menu_slots() {
        let mut inv = Inventory::default();
        inv.set_inventory_slot(0, stack(1, &Item::STONE));
        inv.set_inventory_slot(39, stack(1, &Item::DIAMOND_HELMET));
        inv.set_inventory_slot(40, stack(1, &Item::SHIELD));
        assert!(!inv.slots[36].is_empty());
        assert!(!inv.slots[5].is_empty());
        assert!(!inv.slots[45].is_empty());
    }
}
