//! Vanilla `AbstractContainerScreen`'s mouse handling, shared by the container overlays.

use std::time::{Duration, Instant};

use pumpkin_data::item_stack::ItemStack;

use crate::settings::input::{Action, InputBind};
use crate::settings::options::Config;
use crate::world::inventory::{ClickType, quick_craft_mask};
use crate::world::inventory::{OUTSIDE, can_item_quick_replace, empty};

/// `MouseHandler`'s double click window.
const DOUBLE_CLICK: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButtonKind {
    Left,
    Right,
    Middle,
}

impl MouseButtonKind {
    /// `AbstractContainerScreen.getContainerClickButton`.
    pub fn click_button(self) -> i8 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
        }
    }
}

/// One `slotClicked` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotClick {
    pub slot: Option<usize>,
    pub slot_id: i16,
    pub button: i8,
    pub kind: ClickType,
}

/// `keyHotbarSlots`: the number keys 1-9 as hotbar indices 0-8.
pub fn hotbar_key(key: &str) -> Option<u8> {
    let digit = key.parse::<u8>().ok()?;
    (1..=9).contains(&digit).then(|| digit - 1)
}

/// `AbstractContainerScreen.keyPressed` on the hovered `slot`.
pub fn slot_bind(
    bind: &InputBind,
    config: &Config,
    slot: usize,
    carried_empty: bool,
    control: bool,
) -> Option<SlotClick> {
    let click = |button: i8, kind: ClickType| SlotClick {
        slot: Some(slot),
        slot_id: slot as i16,
        button,
        kind,
    };
    let binds = &config.input.binds;
    if carried_empty && binds.get(Action::SwapOffhand) == bind {
        Some(click(
            crate::world::inventory::SWAP_OFFHAND_BUTTON,
            ClickType::Swap,
        ))
    } else if let Some(n) = binds
        .actions_for(bind)
        .find_map(Action::hotbar_index)
        .filter(|_| carried_empty)
    {
        Some(click(n as i8, ClickType::Swap))
    } else if binds.get(Action::Drop) == bind {
        Some(click(i8::from(control), ClickType::Throw))
    } else {
        None
    }
}

/// The inventory key closes a container screen.
pub fn is_close_bind(bind: &InputBind, config: &Config) -> bool {
    config.input.binds.get(Action::Inventory) == bind
}

/// What the mouse logic needs to know about a screen's slots.
pub trait ContainerView {
    /// `getHoveredSlot`.
    fn slot_at(&self, x: f32, y: f32) -> Option<usize>;
    /// `hasClickedOutside`.
    fn clicked_outside(&self, x: f32, y: f32) -> bool;
    fn carried(&self) -> &ItemStack;
    fn item(&self, slot: usize) -> &ItemStack;
    /// Every slot, for Shift + double click.
    fn slots(&self) -> Vec<usize>;
    /// The menu id of a slot.
    fn slot_id(&self, slot: usize) -> i16 {
        slot as i16
    }
    /// `canItemQuickReplace && mayPlace && menu.canDragTo`.
    fn can_drag_to(&self, slot: usize, carried: &ItemStack) -> bool;
    /// `menu.canTakeItemForPickAll`.
    fn can_pickup_all(&self, _slot: usize) -> bool {
        true
    }
    /// Whether two slots belong to the same container (Shift + double click).
    fn same_container(&self, a: usize, b: usize) -> bool;
}

/// A drag of the cursor stack (`isQuickCrafting`).
#[derive(Clone, Debug)]
pub struct QuickCraft {
    pub button: MouseButtonKind,
    /// 0 split evenly, 1 one each, 2 full stacks.
    pub kind: i8,
    pub slots: Vec<usize>,
}

pub struct ContainerMouse {
    last_click: Option<(Instant, MouseButtonKind)>,
    last_click_slot: Option<usize>,
    double_click: bool,
    skip_next_release: bool,
    quick_craft: Option<QuickCraft>,
    /// `lastQuickMoved`: what Shift-click moved, for Shift + double click.
    last_quick_moved: ItemStack,
}

impl Default for ContainerMouse {
    fn default() -> Self {
        Self {
            last_click: None,
            last_click_slot: None,
            double_click: false,
            skip_next_release: false,
            quick_craft: None,
            last_quick_moved: empty(),
        }
    }
}

impl ContainerMouse {
    pub fn quick_craft(&self) -> Option<&QuickCraft> {
        self.quick_craft.as_ref()
    }

    /// `selectTab` and friends: a drag in progress ends.
    pub fn cancel(&mut self) {
        self.quick_craft = None;
    }

    /// `mouseClicked`. `creative`: the middle button clones.
    pub fn press(
        &mut self,
        view: &dyn ContainerView,
        button: MouseButtonKind,
        (x, y): (f32, f32),
        shift: bool,
        creative: bool,
    ) -> Vec<SlotClick> {
        let now = Instant::now();
        let cloning = button == MouseButtonKind::Middle && creative;
        let slot = view.slot_at(x, y);
        let double = self
            .last_click
            .is_some_and(|(at, last)| last == button && now.duration_since(at) < DOUBLE_CLICK);
        self.double_click = self.last_click_slot == slot && double;
        self.skip_next_release = false;
        let mut clicks = Vec::new();
        if button != MouseButtonKind::Middle || cloning {
            let slot_id = if view.clicked_outside(x, y) {
                OUTSIDE
            } else {
                slot.map_or(-1, |s| view.slot_id(s))
            };
            if slot_id != -1 && self.quick_craft.is_none() {
                if view.carried().is_empty() {
                    if cloning {
                        clicks.push(SlotClick {
                            slot,
                            slot_id,
                            button: 2,
                            kind: ClickType::Clone,
                        });
                    } else {
                        let quick = slot_id != OUTSIDE && shift;
                        let kind = if quick {
                            self.last_quick_moved =
                                slot.map_or_else(empty, |s| view.item(s).clone());
                            ClickType::QuickMove
                        } else if slot_id == OUTSIDE {
                            ClickType::Throw
                        } else {
                            ClickType::Pickup
                        };
                        clicks.push(SlotClick {
                            slot,
                            slot_id,
                            button: button.click_button(),
                            kind,
                        });
                    }
                    self.skip_next_release = true;
                } else {
                    self.quick_craft = Some(QuickCraft {
                        button,
                        kind: match button {
                            MouseButtonKind::Left => 0,
                            MouseButtonKind::Right => 1,
                            MouseButtonKind::Middle => 2,
                        },
                        slots: Vec::new(),
                    });
                }
            }
        }
        self.last_click_slot = slot;
        self.last_click = Some((now, button));
        clicks
    }

    /// `mouseDragged`: a slot crossed while dragging joins the drag.
    pub fn drag(&mut self, view: &dyn ContainerView, (x, y): (f32, f32)) {
        let Some(slot) = view.slot_at(x, y) else {
            return;
        };
        let carried = view.carried();
        if let Some(drag) = &mut self.quick_craft
            && !carried.is_empty()
            && (usize::from(carried.item_count) > drag.slots.len() || drag.kind == 2)
            && !drag.slots.contains(&slot)
            && view.can_drag_to(slot, carried)
        {
            drag.slots.push(slot);
        }
    }

    /// `mouseReleased`.
    pub fn release(
        &mut self,
        view: &dyn ContainerView,
        button: MouseButtonKind,
        (x, y): (f32, f32),
        shift: bool,
    ) -> Vec<SlotClick> {
        let slot = view.slot_at(x, y);
        let slot_id = if view.clicked_outside(x, y) {
            OUTSIDE
        } else {
            slot.map_or(-1, |s| view.slot_id(s))
        };
        let mut clicks = Vec::new();
        if self.double_click
            && let Some(target) = slot
            && button == MouseButtonKind::Left
            && view.can_pickup_all(target)
        {
            if shift {
                if !self.last_quick_moved.is_empty() {
                    for other in view.slots() {
                        let item = view.item(other);
                        if !item.is_empty()
                            && view.same_container(other, target)
                            && can_item_quick_replace(item, &self.last_quick_moved, true)
                        {
                            clicks.push(SlotClick {
                                slot: Some(other),
                                slot_id: view.slot_id(other),
                                button: 0,
                                kind: ClickType::QuickMove,
                            });
                        }
                    }
                }
            } else {
                clicks.push(SlotClick {
                    slot,
                    slot_id,
                    button: 0,
                    kind: ClickType::PickupAll,
                });
            }
            self.double_click = false;
        } else {
            if let Some(drag) = &self.quick_craft
                && drag.button != button
            {
                self.quick_craft = None;
                self.skip_next_release = true;
                return clicks;
            }
            if std::mem::take(&mut self.skip_next_release) {
                self.quick_craft = None;
                return clicks;
            }
            match self.quick_craft.take() {
                Some(drag) if !drag.slots.is_empty() => {
                    let click = |slot: Option<usize>, slot_id, stage| SlotClick {
                        slot,
                        slot_id,
                        button: quick_craft_mask(stage, drag.kind),
                        kind: ClickType::QuickCraft,
                    };
                    clicks.push(click(None, OUTSIDE, 0));
                    for &s in &drag.slots {
                        clicks.push(click(Some(s), view.slot_id(s), 1));
                    }
                    clicks.push(click(None, OUTSIDE, 2));
                }
                _ if !view.carried().is_empty() => {
                    if button == MouseButtonKind::Middle {
                        clicks.push(SlotClick {
                            slot,
                            slot_id,
                            button: 2,
                            kind: ClickType::Clone,
                        });
                    } else {
                        let quick = slot_id != OUTSIDE && shift;
                        if quick {
                            self.last_quick_moved =
                                slot.map_or_else(empty, |s| view.item(s).clone());
                        }
                        clicks.push(SlotClick {
                            slot,
                            slot_id,
                            button: button.click_button(),
                            kind: if quick {
                                ClickType::QuickMove
                            } else {
                                ClickType::Pickup
                            },
                        });
                    }
                }
                _ => {}
            }
        }
        self.quick_craft = None;
        clicks.retain(|c| c.slot_id != -1 || c.kind == ClickType::QuickCraft);
        clicks
    }
}

/// `extractSlot`'s preview of a drag.
pub fn drag_preview(
    drag: &QuickCraft,
    slot: usize,
    current: &ItemStack,
    carried: &ItemStack,
    max_in_slot: u8,
) -> Option<(ItemStack, bool)> {
    if drag.slots.len() < 2 || !drag.slots.contains(&slot) || carried.is_empty() {
        return None;
    }
    let max = i32::from(carried.get_max_stack_size().min(max_in_slot));
    let carry = if current.is_empty() {
        0
    } else {
        i32::from(current.item_count)
    };
    let placed =
        crate::world::inventory::quick_craft_place_count(drag.slots.len(), drag.kind, carried)
            + carry;
    Some(if placed > max {
        (carried.copy_with_count(max as u8), true)
    } else {
        (carried.copy_with_count(placed.max(0) as u8), false)
    })
}

/// `recalculateQuickCraftRemaining`: what stays on the cursor.
pub fn drag_remainder(
    drag: &QuickCraft,
    carried: &ItemStack,
    slot_item: impl Fn(usize) -> (ItemStack, u8),
) -> i32 {
    if drag.kind == 2 {
        return i32::from(carried.get_max_stack_size());
    }
    let mut remaining = i32::from(carried.item_count);
    for &slot in &drag.slots {
        let (current, max_in_slot) = slot_item(slot);
        let carry = if current.is_empty() {
            0
        } else {
            i32::from(current.item_count)
        };
        let max = i32::from(carried.get_max_stack_size().min(max_in_slot));
        let placed = (crate::world::inventory::quick_craft_place_count(
            drag.slots.len(),
            drag.kind,
            carried,
        ) + carry)
            .min(max);
        remaining -= placed - carry;
    }
    remaining
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    /// Three slots in a row at x = 0, 20, 40; x >= 100 is outside.
    struct Row {
        items: Vec<ItemStack>,
        carried: ItemStack,
    }

    impl ContainerView for Row {
        fn slot_at(&self, x: f32, _y: f32) -> Option<usize> {
            (x < 60.0).then(|| (x / 20.0) as usize)
        }
        fn clicked_outside(&self, x: f32, _y: f32) -> bool {
            x >= 100.0
        }
        fn carried(&self) -> &ItemStack {
            &self.carried
        }
        fn item(&self, slot: usize) -> &ItemStack {
            &self.items[slot]
        }
        fn slots(&self) -> Vec<usize> {
            (0..3).collect()
        }
        fn can_drag_to(&self, slot: usize, carried: &ItemStack) -> bool {
            can_item_quick_replace(&self.items[slot], carried, true)
        }
        fn same_container(&self, _a: usize, _b: usize) -> bool {
            true
        }
    }

    #[test]
    fn a_press_with_an_empty_cursor_picks_up_at_once() {
        let row = Row {
            items: vec![ItemStack::new(5, &Item::DIRT), empty(), empty()],
            carried: empty(),
        };
        let mut mouse = ContainerMouse::default();
        let clicks = mouse.press(&row, MouseButtonKind::Left, (5.0, 0.0), false, false);
        assert_eq!(clicks[0].kind, ClickType::Pickup);
        assert!(
            mouse
                .release(&row, MouseButtonKind::Left, (5.0, 0.0), false)
                .is_empty()
        );
    }

    #[test]
    fn carrying_places_on_release_or_splits_over_a_drag() {
        let row = Row {
            items: vec![empty(), empty(), empty()],
            carried: ItemStack::new(9, &Item::DIRT),
        };
        let mut mouse = ContainerMouse::default();
        assert!(
            mouse
                .press(&row, MouseButtonKind::Left, (5.0, 0.0), false, false)
                .is_empty()
        );
        let clicks = mouse.release(&row, MouseButtonKind::Left, (25.0, 0.0), false);
        assert_eq!(clicks.len(), 1);
        assert_eq!(
            (clicks[0].slot, clicks[0].kind),
            (Some(1), ClickType::Pickup)
        );

        mouse.press(&row, MouseButtonKind::Right, (5.0, 0.0), false, false);
        for x in [5.0, 25.0, 45.0] {
            mouse.drag(&row, (x, 0.0));
        }
        let clicks = mouse.release(&row, MouseButtonKind::Right, (45.0, 0.0), false);
        assert_eq!(clicks.len(), 5);
        assert!(clicks.iter().all(|c| c.kind == ClickType::QuickCraft));
        assert_eq!(clicks[1].button, quick_craft_mask(1, 1));
    }
}
