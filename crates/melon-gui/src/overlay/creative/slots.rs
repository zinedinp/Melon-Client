//! Where the slots and tabs are.

use super::{
    COLUMNS, CreativeOverlay, DESTROY_SLOT, IMAGE_HEIGHT, IMAGE_WIDTH, ROWS, Slot, TAB_HEIGHT,
    TAB_WIDTH,
};
use crate::world::creative_tabs::{CreativeTab, Row, TabType, tabs};
use crate::world::inventory::{self, HOTBAR_SLOTS, SLOT_COUNT};

impl CreativeOverlay {
    /// Every slot on the current tab with its position, by menu index.
    pub(super) fn slots(&self) -> impl Iterator<Item = (usize, Slot, (f32, f32))> + use<> {
        let inventory = self.tab_type() == TabType::Inventory;
        let player = (inventory::ARMOR_SLOTS.start..SLOT_COUNT)
            .filter(move |_| inventory)
            .map(|slot| (slot, Slot::Player(slot), inventory_tab_position(slot)))
            .chain(inventory.then_some((DESTROY_SLOT, Slot::Destroy, (173.0, 112.0))));
        let picker = (0..ROWS * COLUMNS + 9)
            .filter(move |_| !inventory)
            .map(|i| match i.checked_sub(ROWS * COLUMNS) {
                None => (
                    i,
                    Slot::Picker(i),
                    (
                        9.0 + (i % COLUMNS) as f32 * 18.0,
                        18.0 + (i / COLUMNS) as f32 * 18.0,
                    ),
                ),
                Some(h) => (
                    i,
                    Slot::Player(HOTBAR_SLOTS.start + h),
                    (9.0 + h as f32 * 18.0, 112.0),
                ),
            });
        player.chain(picker)
    }

    pub(super) fn slot_kind(&self, index: usize) -> Option<Slot> {
        self.slots()
            .find(|(i, _, _)| *i == index)
            .map(|(_, slot, _)| slot)
    }

    pub(super) fn hovered_slot(&self, (mx, my): (f32, f32)) -> Option<usize> {
        self.slots().find_map(|(i, _, (x, y))| {
            let (x, y) = (self.left + x, self.top + y);
            (mx >= x - 1.0 && mx < x + 17.0 && my >= y - 1.0 && my < y + 17.0).then_some(i)
        })
    }

    /// `getTabX`/`getTabY`, relative to the panel.
    pub(super) fn tab_position(tab: &CreativeTab) -> (f32, f32) {
        let x = if tab.aligned_right {
            IMAGE_WIDTH - 27.0 * (7.0 - f32::from(tab.column)) + 1.0
        } else {
            27.0 * f32::from(tab.column)
        };
        let y = match tab.row {
            Row::Top => -32.0,
            Row::Bottom => IMAGE_HEIGHT,
        };
        (x, y)
    }

    /// `checkTabClicked`: the tab under the mouse.
    pub(super) fn tab_at(&self, (mx, my): (f32, f32)) -> Option<usize> {
        let (xm, ym) = (mx - self.left, my - self.top);
        self.tabs.iter().position(|&i| {
            let (x, y) = Self::tab_position(&tabs()[i]);
            xm >= x && xm <= x + TAB_WIDTH && ym >= y && ym <= y + TAB_HEIGHT
        })
    }

    pub(super) fn inside_scrollbar(&self, (mx, my): (f32, f32)) -> bool {
        let x = self.left + 175.0;
        let y = self.top + 18.0;
        mx >= x && my >= y && mx < x + 14.0 && my < y + 112.0 && self.tab().scrollbar
    }
}

/// `selectTab`'s layout of the player's slots on the inventory tab.
pub(super) fn inventory_tab_position(slot: usize) -> (f32, f32) {
    match slot {
        5..=8 => {
            let pos = slot - 5;
            (
                54.0 + (pos / 2) as f32 * 54.0,
                6.0 + (pos % 2) as f32 * 27.0,
            )
        }
        inventory::OFFHAND_SLOT => (35.0, 20.0),
        _ => {
            let pos = slot - 9;
            let x = 9.0 + (pos % 9) as f32 * 18.0;
            let y = if slot >= HOTBAR_SLOTS.start {
                112.0
            } else {
                54.0 + (pos / 9) as f32 * 18.0
            };
            (x, y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::overlay::menu_layout::SLOT_SIZE;

    #[test]
    fn the_inventory_tab_keeps_every_slot_inside_its_panel() {
        for slot in inventory::ARMOR_SLOTS.start..SLOT_COUNT {
            let (x, y) = inventory_tab_position(slot);
            assert!(
                x >= 0.0
                    && y >= 0.0
                    && x + SLOT_SIZE <= IMAGE_WIDTH
                    && y + SLOT_SIZE <= IMAGE_HEIGHT,
                "slot {slot} at ({x}, {y})"
            );
        }
    }
}
