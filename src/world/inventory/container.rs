//! A server-opened menu (an `AbstractContainerMenu` that isn't the player's own).

use super::Inventory;
use super::{
    Click, ClickType, Drag, HOTBAR_SLOTS, MAIN_SLOTS, OUTSIDE, empty, shrink, split_slot,
    stacks_equal,
};
use crate::gui::text::StyledText;
use pumpkin_data::item_stack::ItemStack;

/// Player inventory portion of every vanilla container: 27 main + 9 hotbar.
pub const PLAYER_SLOTS: usize = 36;

/// What a mount's screen shows besides its slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MountLook {
    /// The tracked entity drawn on the left.
    pub entity: i32,
    /// `shouldRenderSaddleSlot`.
    pub saddle: bool,
    /// `shouldRenderArmorSlot`.
    pub armor: bool,
    /// `NautilusInventoryScreen` has its own sheet.
    pub nautilus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    Generic {
        rows: u8,
    },
    Hopper,
    Dispenser,
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
    Anvil,
    Grindstone,
    Smithing,
    Cartography,
    Loom,
    Stonecutter,
    Brewing,
    Beacon,
    Enchantment,
    Merchant,
    /// A horse's, donkey's, llama's or nautilus' inventory.
    Mount {
        columns: u8,
        look: MountLook,
    },
    /// Known window type we don't lay out: still closable.
    Unsupported,
}

impl ContainerKind {
    /// `MenuType` network id (the order of `MenuType`'s registrations).
    pub fn from_window_type(id: i32) -> Self {
        match id {
            0 => Self::Generic { rows: 1 },
            1 => Self::Generic { rows: 2 },
            2 | 20 => Self::Generic { rows: 3 },
            3 => Self::Generic { rows: 4 },
            4 => Self::Generic { rows: 5 },
            5 => Self::Generic { rows: 6 },
            6 => Self::Dispenser,
            8 => Self::Anvil,
            9 => Self::Beacon,
            10 => Self::BlastFurnace,
            11 => Self::Brewing,
            12 => Self::Crafting,
            13 => Self::Enchantment,
            14 => Self::Furnace,
            15 => Self::Grindstone,
            16 => Self::Hopper,
            18 => Self::Loom,
            19 => Self::Merchant,
            21 => Self::Smithing,
            22 => Self::Smoker,
            23 => Self::Cartography,
            24 => Self::Stonecutter,
            _ => Self::Unsupported,
        }
    }

    /// The slots of the menu's own container, before the player's 36.
    pub fn container_slots(self) -> usize {
        match self {
            Self::Generic { rows } => usize::from(rows) * 9,
            Self::Hopper | Self::Brewing => 5,
            Self::Dispenser => 9,
            Self::Crafting => 10,
            Self::Furnace
            | Self::BlastFurnace
            | Self::Smoker
            | Self::Anvil
            | Self::Grindstone
            | Self::Cartography
            | Self::Merchant => 3,
            Self::Smithing | Self::Loom => 4,
            Self::Stonecutter | Self::Enchantment => 2,
            Self::Beacon => 1,
            Self::Mount { columns, .. } => 2 + usize::from(columns) * 3,
            Self::Unsupported => 0,
        }
    }

    pub fn slot_count(self) -> usize {
        match self {
            Self::Unsupported => 0,
            other => other.container_slots() + PLAYER_SLOTS,
        }
    }

    /// The menu's output slot, which nothing may be placed in.
    pub fn result_slot(self) -> Option<usize> {
        match self {
            Self::Crafting => Some(0),
            Self::Furnace
            | Self::BlastFurnace
            | Self::Smoker
            | Self::Anvil
            | Self::Grindstone
            | Self::Cartography
            | Self::Merchant => Some(2),
            Self::Smithing | Self::Loom => Some(3),
            Self::Stonecutter => Some(1),
            _ => None,
        }
    }

    pub fn is_result(self, slot: usize) -> bool {
        self.result_slot() == Some(slot)
    }

    /// Whether `quickMoveStack` is predicted here.
    pub fn predicts_quick_move(self) -> bool {
        matches!(
            self,
            Self::Generic { .. } | Self::Hopper | Self::Dispenser | Self::Crafting
        )
    }

    /// Smelting menus: furnace, blast furnace, smoker.
    pub fn is_furnace(self) -> bool {
        matches!(self, Self::Furnace | Self::BlastFurnace | Self::Smoker)
    }
}

/// The open menu's slots and `stateId`.
pub struct OpenContainer {
    pub id: i32,
    pub kind: ContainerKind,
    pub title: StyledText,
    pub slots: Vec<ItemStack>,
    pub state_id: i32,
    /// `ContainerData`, as `ClientboundContainerSetDataPacket` sets it:
    data: Vec<i16>,
    /// A trader's offers (`ClientboundMerchantOffersPacket`).
    pub trades: Option<super::merchant::Offers>,
    drag: Option<Drag>,
}

impl OpenContainer {
    pub fn new(id: i32, kind: ContainerKind, title: impl Into<StyledText>) -> Self {
        let n = kind.slot_count();
        Self {
            id,
            kind,
            title: title.into(),
            slots: vec![empty(); n],
            state_id: 0,
            data: Vec::new(),
            trades: None,
            drag: None,
        }
    }

    /// `AbstractContainerMenu.setData`.
    pub fn set_data(&mut self, id: usize, value: i16) {
        if self.data.len() <= id {
            self.data.resize(id + 1, 0);
        }
        self.data[id] = value;
    }

    /// Data slot `id`, `0` until the server sets it.
    pub fn data(&self, id: usize) -> i16 {
        self.data.get(id).copied().unwrap_or(0)
    }

    pub fn set_all(&mut self, state_id: i32, slots: Vec<ItemStack>) {
        self.state_id = state_id;
        for (slot, item) in self.slots.iter_mut().zip(slots) {
            *slot = item;
        }
    }

    pub fn set_slot(&mut self, state_id: i32, slot: i16, item: ItemStack) {
        self.state_id = state_id;
        if let Some(target) = usize::try_from(slot)
            .ok()
            .and_then(|s| self.slots.get_mut(s))
        {
            *target = item;
        }
    }

    /// Player-inventory slice (last 36), for copying into the HUD inventory.
    pub fn player_slots(&self) -> Option<&[ItemStack]> {
        let n = self.slots.len();
        (n >= PLAYER_SLOTS).then(|| &self.slots[n - PLAYER_SLOTS..])
    }

    /// Copies the player's main inventory and hotbar into the last 36 slots.
    pub fn copy_player_from(&mut self, inventory: &Inventory) {
        let Some(start) = self.slots.len().checked_sub(PLAYER_SLOTS) else {
            return;
        };
        for i in 0..27 {
            self.slots[start + i] = inventory.slots()[MAIN_SLOTS.start + i].clone();
        }
        for i in 0..9 {
            self.slots[start + 27 + i] = inventory.slots()[HOTBAR_SLOTS.start + i].clone();
        }
    }

    /// Writes the last 36 slots back into the player's own inventory.
    pub fn copy_player_into(&self, inventory: &mut Inventory) {
        let Some(player) = self.player_slots() else {
            return;
        };
        let (main, hotbar) = player.split_at(27);
        inventory.slots_mut()[MAIN_SLOTS].clone_from_slice(main);
        inventory.slots_mut()[HOTBAR_SLOTS].clone_from_slice(&hotbar[..9]);
    }

    pub fn click(
        &mut self,
        slot: i16,
        button: i8,
        kind: ClickType,
        creative: bool,
        carried: &mut ItemStack,
    ) -> Click {
        let before = self.slots.clone();
        let index = usize::try_from(slot).ok().filter(|&i| i < self.slots.len());
        if kind != ClickType::QuickCraft {
            self.drag = None;
        }
        match kind {
            ClickType::Pickup => self.pickup(slot, button, carried),
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
                    && carried.is_empty()
                    && !self.slots[index].is_empty()
                {
                    let stack = &self.slots[index];
                    *carried = stack.copy_with_count(stack.get_max_stack_size());
                }
            }
            ClickType::Throw => {
                if let Some(index) = index
                    && carried.is_empty()
                {
                    let stack = &mut self.slots[index];
                    let count = if button == 1 { stack.item_count } else { 1 };
                    shrink(stack, count);
                }
            }
            ClickType::QuickCraft => self.quick_craft(slot, button, creative, carried),
            ClickType::PickupAll => {
                if let Some(index) = index {
                    self.pickup_all(index, carried);
                }
            }
        }
        let changed: Vec<i16> = (0..self.slots.len())
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

    fn pickup(&mut self, slot: i16, button: i8, carried: &mut ItemStack) {
        if slot == OUTSIDE {
            let count = if button == 0 { carried.item_count } else { 1 };
            shrink(carried, count);
            return;
        }
        let Ok(index) = usize::try_from(slot) else {
            return;
        };
        if index >= self.slots.len() || self.kind.is_result(index) {
            return;
        }
        let stack = &mut self.slots[index];
        let max = stack.get_max_stack_size().max(carried.get_max_stack_size());
        if stack.is_empty() {
            if !carried.is_empty() {
                let amount = if button == 0 { carried.item_count } else { 1 };
                *stack = split_slot(carried, amount.min(max));
            }
        } else if carried.is_empty() {
            let amount = if button == 0 {
                stack.item_count
            } else {
                stack.item_count.div_ceil(2)
            };
            *carried = split_slot(stack, amount);
        } else if stack.are_items_and_components_equal(carried) {
            let room = max.saturating_sub(stack.item_count);
            let amount = if button == 0 {
                carried.item_count.min(room)
            } else {
                1.min(room)
            };
            stack.item_count += amount;
            shrink(carried, amount);
        } else if carried.item_count <= max {
            std::mem::swap(stack, carried);
        }
    }

    fn swap(&mut self, index: usize, button: i8) {
        if self.kind.is_result(index) {
            return;
        }
        let n = self.slots.len();
        if n < PLAYER_SLOTS {
            return;
        }
        let hotbar_start = n - 9;
        let target = match button {
            0..=8 => hotbar_start + button as usize,
            _ => return,
        };
        if index == target {
            return;
        }
        self.slots.swap(index, target);
    }

    /// `quickMoveStack` of `ChestMenu`, `HopperMenu`, `DispenserMenu` and their kin.
    fn quick_move(&mut self, index: usize) {
        if !self.kind.predicts_quick_move()
            || self.slots[index].is_empty()
            || self.kind.is_result(index)
        {
            return;
        }
        let n = self.slots.len();
        if n < PLAYER_SLOTS {
            return;
        }
        let container = self.kind.container_slots();
        let mut moving = std::mem::replace(&mut self.slots[index], empty());
        if index < container {
            self.move_into(&mut moving, container..n, true);
        } else if self.kind == ContainerKind::Crafting {
            let hotbar = n - 9;
            if !self.move_into(&mut moving, 1..container, false) {
                let other = if index < hotbar {
                    hotbar..n
                } else {
                    container..hotbar
                };
                self.move_into(&mut moving, other, false);
            }
        } else {
            self.move_into(&mut moving, 0..container, false);
        }
        self.slots[index] = moving;
    }

    /// `AbstractContainerMenu.moveItemStackTo`.
    fn move_into(
        &mut self,
        moving: &mut ItemStack,
        targets: std::ops::Range<usize>,
        backwards: bool,
    ) -> bool {
        let order = |targets: std::ops::Range<usize>| -> Vec<usize> {
            if backwards {
                targets.rev().collect()
            } else {
                targets.collect()
            }
        };
        let before = moving.item_count;
        if moving.is_stackable() {
            for i in order(targets.clone()) {
                if moving.is_empty() {
                    break;
                }
                if self.kind.is_result(i) {
                    continue;
                }
                let target = &mut self.slots[i];
                if !target.is_empty() && target.are_items_and_components_equal(moving) {
                    let max = target.get_max_stack_size();
                    let amount = moving.item_count.min(max.saturating_sub(target.item_count));
                    target.item_count += amount;
                    shrink(moving, amount);
                }
            }
        }
        if !moving.is_empty() {
            for i in order(targets) {
                if self.kind.is_result(i) || !self.slots[i].is_empty() {
                    continue;
                }
                let max = moving.get_max_stack_size();
                let count = moving.item_count.min(max);
                self.slots[i] = split_slot(moving, count);
                break;
            }
        }
        moving.is_empty() || moving.item_count != before
    }

    fn pickup_all(&mut self, index: usize, carried: &mut ItemStack) {
        if carried.is_empty() {
            return;
        }
        let max = carried.get_max_stack_size();
        for i in 0..self.slots.len() {
            if i == index || self.kind.is_result(i) {
                continue;
            }
            if carried.item_count >= max {
                break;
            }
            let stack = &mut self.slots[i];
            if stack.are_items_and_components_equal(carried) {
                let amount = stack.item_count.min(max.saturating_sub(carried.item_count));
                carried.item_count += amount;
                shrink(stack, amount);
            }
        }
    }

    fn quick_craft(&mut self, slot: i16, button: i8, creative: bool, carried: &mut ItemStack) {
        let stage = button & 3;
        let kind = (button >> 2) & 3;
        match stage {
            0 => {
                let valid = kind == 0 || kind == 1 || (kind == 2 && creative);
                self.drag = (valid && !carried.is_empty()).then(|| Drag {
                    kind,
                    slots: Vec::new(),
                });
            }
            1 => {
                if let Ok(index) = usize::try_from(slot)
                    && let Some(drag) = &self.drag
                    && !drag.slots.contains(&index)
                    && index < self.slots.len()
                    && !self.kind.is_result(index)
                    && let Some(drag) = &mut self.drag
                {
                    drag.slots.push(index);
                }
            }
            2 => {
                let Some(drag) = self.drag.take() else { return };
                if drag.slots.is_empty() || carried.is_empty() {
                    return;
                }
                let n = drag.slots.len() as u8;
                let each = match drag.kind {
                    0 => carried.item_count / n.max(1),
                    1 => 1,
                    2 if creative => carried.get_max_stack_size(),
                    _ => 0,
                };
                for index in drag.slots {
                    if carried.is_empty() || each == 0 {
                        break;
                    }
                    let target = &mut self.slots[index];
                    let max = carried.get_max_stack_size();
                    if target.is_empty() {
                        *target = split_slot(carried, each.min(max));
                    } else if target.are_items_and_components_equal(carried) {
                        let room = max.saturating_sub(target.item_count);
                        let amount = each.min(room).min(carried.item_count);
                        target.item_count += amount;
                        shrink(carried, amount);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::text::{StyledText, colors};
    use crate::world::inventory::Inventory;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;

    #[test]
    fn the_window_title_keeps_its_colour() {
        let title = StyledText::colored("Warps", colors::GOLD);
        let menu = OpenContainer::new(1, ContainerKind::Generic { rows: 3 }, title);
        assert_eq!(menu.title.to_plain(), "Warps");
        assert_eq!(menu.title.spans[0].style.color, Some(colors::GOLD));
    }

    #[test]
    fn window_types_map_to_layouts() {
        assert_eq!(
            ContainerKind::from_window_type(2),
            ContainerKind::Generic { rows: 3 }
        );
        assert_eq!(
            ContainerKind::from_window_type(5),
            ContainerKind::Generic { rows: 6 }
        );
        assert_eq!(ContainerKind::from_window_type(6), ContainerKind::Dispenser);
        assert_eq!(ContainerKind::from_window_type(12), ContainerKind::Crafting);
        assert_eq!(ContainerKind::from_window_type(16), ContainerKind::Hopper);
        assert_eq!(
            ContainerKind::from_window_type(20),
            ContainerKind::Generic { rows: 3 }
        );
        assert_eq!(
            ContainerKind::from_window_type(99),
            ContainerKind::Unsupported
        );
    }

    #[test]
    fn every_menu_type_the_server_opens_has_its_vanilla_slot_count() {
        let expected = [
            (8, ContainerKind::Anvil, 3),
            (9, ContainerKind::Beacon, 1),
            (10, ContainerKind::BlastFurnace, 3),
            (11, ContainerKind::Brewing, 5),
            (13, ContainerKind::Enchantment, 2),
            (14, ContainerKind::Furnace, 3),
            (15, ContainerKind::Grindstone, 3),
            (18, ContainerKind::Loom, 4),
            (19, ContainerKind::Merchant, 3),
            (21, ContainerKind::Smithing, 4),
            (22, ContainerKind::Smoker, 3),
            (23, ContainerKind::Cartography, 3),
            (24, ContainerKind::Stonecutter, 2),
        ];
        for (id, kind, slots) in expected {
            assert_eq!(ContainerKind::from_window_type(id), kind, "{id}");
            assert_eq!(kind.container_slots(), slots, "{kind:?}");
            assert_eq!(kind.slot_count(), slots + PLAYER_SLOTS, "{kind:?}");
        }
    }

    #[test]
    fn the_output_slot_is_the_last_of_the_menus_own_inputs() {
        for kind in [
            ContainerKind::Furnace,
            ContainerKind::Anvil,
            ContainerKind::Grindstone,
            ContainerKind::Smithing,
            ContainerKind::Cartography,
            ContainerKind::Loom,
            ContainerKind::Stonecutter,
            ContainerKind::Merchant,
        ] {
            assert_eq!(
                kind.result_slot(),
                Some(kind.container_slots() - 1),
                "{kind:?}"
            );
        }
        assert_eq!(ContainerKind::Crafting.result_slot(), Some(0));
        assert_eq!(ContainerKind::Brewing.result_slot(), None);
    }

    #[test]
    fn nothing_is_placed_in_a_result_slot_and_the_server_moves_the_rest() {
        let mut menu = OpenContainer::new(1, ContainerKind::Stonecutter, String::new());
        let mut carried = ItemStack::new(3, &Item::STONE);
        menu.click(1, 0, ClickType::Pickup, false, &mut carried);
        assert!(menu.slots[1].is_empty());
        assert_eq!(carried.item_count, 3);
        let mut furnace = OpenContainer::new(1, ContainerKind::Furnace, String::new());
        furnace.slots[3] = ItemStack::new(3, &Item::STONE);
        let click = furnace.click(3, 0, ClickType::QuickMove, false, &mut empty());
        assert!(click.changed.is_empty());
        assert_eq!(furnace.slots[3].item_count, 3);
    }

    #[test]
    fn container_data_reads_zero_until_the_server_sets_it() {
        let mut menu = OpenContainer::new(1, ContainerKind::Furnace, String::new());
        assert_eq!(menu.data(2), 0);
        menu.set_data(2, 137);
        assert_eq!((menu.data(0), menu.data(2)), (0, 137));
    }

    #[test]
    fn player_slots_round_trip() {
        let mut inventory = Inventory::default();
        inventory.slots_mut()[9] = ItemStack::new(1, &Item::STONE);
        inventory.slots_mut()[36] = ItemStack::new(4, &Item::DIRT);
        let mut menu = OpenContainer::new(1, ContainerKind::Generic { rows: 3 }, "Chest");
        menu.copy_player_from(&inventory);
        let mut other = Inventory::default();
        menu.copy_player_into(&mut other);
        assert_eq!(other.slots()[9].item.registry_key, "stone");
        assert_eq!(other.slots()[36].item_count, 4);
    }

    fn quick_move(menu: &mut OpenContainer, slot: i16) {
        let mut carried = empty();
        menu.click(slot, 0, ClickType::QuickMove, false, &mut carried);
    }

    #[test]
    fn a_chest_quick_moves_into_the_hotbar_from_the_right() {
        let mut menu = OpenContainer::new(1, ContainerKind::Generic { rows: 3 }, String::new());
        menu.slots[0] = ItemStack::new(5, &Item::STONE);
        quick_move(&mut menu, 0);
        assert!(menu.slots[0].is_empty());
        assert_eq!(menu.slots[62].item_count, 5);
        assert!(menu.slots[27].is_empty());
    }

    #[test]
    fn a_chest_tops_up_the_last_matching_stack_first() {
        let mut menu = OpenContainer::new(1, ContainerKind::Generic { rows: 3 }, String::new());
        menu.slots[0] = ItemStack::new(10, &Item::STONE);
        menu.slots[27] = ItemStack::new(60, &Item::STONE);
        menu.slots[54] = ItemStack::new(60, &Item::STONE);
        quick_move(&mut menu, 0);
        assert_eq!(menu.slots[54].item_count, 64);
        assert_eq!(menu.slots[27].item_count, 64);
        assert_eq!(menu.slots[62].item_count, 2);
    }

    #[test]
    fn the_player_quick_moves_into_a_chest_from_the_front() {
        let mut menu = OpenContainer::new(1, ContainerKind::Generic { rows: 3 }, String::new());
        menu.slots[60] = ItemStack::new(3, &Item::DIRT);
        quick_move(&mut menu, 60);
        assert_eq!(menu.slots[0].item_count, 3);
    }

    #[test]
    fn a_crafting_table_falls_back_to_the_hotbar_when_its_grid_is_full() {
        let mut menu = OpenContainer::new(1, ContainerKind::Crafting, String::new());
        for slot in &mut menu.slots[1..10] {
            *slot = ItemStack::new(1, &Item::DIRT);
        }
        menu.slots[10] = ItemStack::new(1, &Item::STONE);
        quick_move(&mut menu, 10);
        assert!(menu.slots[10].is_empty());
        assert_eq!(menu.slots[37].item_count, 1);
        quick_move(&mut menu, 37);
        assert_eq!(menu.slots[10].item_count, 1);
    }

    #[test]
    fn a_crafting_table_fills_its_grid_first() {
        let mut menu = OpenContainer::new(1, ContainerKind::Crafting, String::new());
        menu.slots[40] = ItemStack::new(2, &Item::DIRT);
        quick_move(&mut menu, 40);
        assert_eq!(menu.slots[1].item_count, 2);
        assert!(menu.slots[0].is_empty());
    }
}
