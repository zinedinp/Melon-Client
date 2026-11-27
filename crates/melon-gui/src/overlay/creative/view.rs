//! The creative inventory as the shared container input sees it.

use super::{CreativeOverlay, IMAGE_HEIGHT, IMAGE_WIDTH, Slot};
use crate::overlay::container_input::ContainerView;
use melon_world::inventory::{self, Inventory, can_item_quick_replace};
use pumpkin_data::item_stack::ItemStack;

impl CreativeOverlay {
    pub fn view<'a>(&'a self, inventory: &'a Inventory) -> CreativeView<'a> {
        CreativeView {
            screen: self,
            inventory,
        }
    }
}

/// The creative screen as the mouse logic sees it.
pub struct CreativeView<'a> {
    pub(super) screen: &'a CreativeOverlay,
    pub(super) inventory: &'a Inventory,
}

impl CreativeView<'_> {
    pub(super) fn max_in(&self, slot: usize) -> u8 {
        match self.screen.slot_kind(slot) {
            Some(Slot::Player(menu)) => inventory::slot_max(menu, self.inventory.carried()),
            _ => 0,
        }
    }
}

impl ContainerView for CreativeView<'_> {
    fn slot_at(&self, x: f32, y: f32) -> Option<usize> {
        self.screen.hovered_slot((x, y))
    }

    fn clicked_outside(&self, x: f32, y: f32) -> bool {
        let s = self.screen;
        let outside =
            x < s.left || y < s.top || x >= s.left + IMAGE_WIDTH || y >= s.top + IMAGE_HEIGHT;
        outside && s.tab_at((x, y)).is_none()
    }

    fn carried(&self) -> &ItemStack {
        self.inventory.carried()
    }

    fn item(&self, slot: usize) -> &ItemStack {
        match self.screen.slot_kind(slot) {
            Some(Slot::Picker(i)) => self.screen.picker(i).0,
            Some(Slot::Player(menu)) => &self.inventory.slots()[menu],
            _ => &self.screen.empty,
        }
    }

    fn slots(&self) -> Vec<usize> {
        self.screen.slots().map(|(i, _, _)| i).collect()
    }

    /// `ItemPickerMenu.canDragTo`: never onto the picker's own slots.
    fn can_drag_to(&self, slot: usize, carried: &ItemStack) -> bool {
        match self.screen.slot_kind(slot) {
            Some(Slot::Player(menu)) => {
                can_item_quick_replace(&self.inventory.slots()[menu], carried, true)
                    && inventory::may_place(menu, carried)
            }
            _ => false,
        }
    }

    fn can_pickup_all(&self, slot: usize) -> bool {
        matches!(self.screen.slot_kind(slot), Some(Slot::Player(_)))
    }

    fn same_container(&self, a: usize, b: usize) -> bool {
        let kind = |slot| match self.screen.slot_kind(slot) {
            Some(Slot::Player(_)) => 0,
            _ => 1,
        };
        kind(a) == kind(b)
    }
}
