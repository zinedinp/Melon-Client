//! The game's side of the glide and the riptide spin.

use super::InWorld;
use melon_net::play::game::PlayerCommand;
use pumpkin_data::data_component_impl::{
    DamageImpl, EquipmentSlot, EquippableImpl, GliderImpl, MaxDamageImpl,
};
use pumpkin_data::item_stack::ItemStack;

/// `InventoryMenu`'s armor slots: head, chest, legs, feet.
const ARMOR_SLOTS: std::ops::Range<usize> = 5..9;

/// `LivingEntity.canGlideUsing`.
fn glides_in(stack: &ItemStack, index: usize) -> bool {
    if stack.get_data_component::<GliderImpl>().is_none() {
        return false;
    }
    let Some(equippable) = stack.get_data_component::<EquippableImpl>() else {
        return false;
    };
    let fits = matches!(
        (index, equippable.slot),
        (5, EquipmentSlot::Head(_))
            | (6, EquipmentSlot::Chest(_))
            | (7, EquipmentSlot::Legs(_))
            | (8, EquipmentSlot::Feet(_))
    );
    let breaks = stack
        .get_data_component::<MaxDamageImpl>()
        .is_some_and(|max| {
            let damage = stack
                .get_data_component::<DamageImpl>()
                .map_or(0, |d| d.damage);
            damage >= max.max_damage - 1
        });
    fits && !breaks
}

impl InWorld {
    /// Whether any worn armor lets the player glide.
    pub(super) fn glider_worn(&self) -> bool {
        let slots = self.inventory.slots();
        ARMOR_SLOTS
            .clone()
            .any(|i| slots.get(i).is_some_and(|stack| glides_in(stack, i)))
    }

    /// After the player's tick.
    pub(super) fn after_airborne_tick(&mut self) {
        if self.player.take_glide_start() {
            self.joined
                .session
                .send_player_command(PlayerCommand::StartFallFlying);
        }
        if let Some(sweep) = self.player.take_spin_sweep() {
            let hit = self.entities.any_living_in(&sweep);
            self.player.resolve_spin_attack(hit);
        }
    }
}
