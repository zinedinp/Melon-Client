//! What the status bars show of the local player beyond health and food.

use super::InWorld;
use melon_gui::hud::bars::air::MAX_AIR;
use melon_gui::hud::bars::hearts::HeartKind;
use melon_net::play::game::DataValue;
use melon_world::player::status_effects::{HUNGER_ID, POISON_ID, REGENERATION_ID, WITHER_ID};
use pumpkin_data::tracked_data::{avatar, entity, player};

/// `Attributes.MAX_HEALTH`'s default: a new player's health.
const DEFAULT_MAX_HEALTH: f32 = 20.0;

/// `Entity.getTicksRequiredToFreeze`: frozen hearts from here.
const FROZEN_TICKS: i32 = 140;

/// What `LocalPlayer` keeps of its own body beyond [`PlayerState`]'s
/// movement. Vanilla makes a new `LocalPlayer` on every respawn, so these
/// start over then (see [`Self::respawned`]).
///
/// [`PlayerState`]: melon_world::player::PlayerState
#[derive(Default)]
pub(super) struct OwnBody {
    /// `LivingEntity.hurtTime`: ticks the body still flashes red.
    pub(super) hurt: i32,
    /// `LivingEntity.deathTime`: ticks since dying, 0 while alive.
    pub(super) death: i32,
    /// `LivingEntity.hurtDir`: where the last blow came from, for the
    /// camera's tilt.
    pub(super) hurt_dir: f32,
    /// `Entity.DATA_TICKS_FROZEN`.
    pub(super) frozen_ticks: i32,
    /// `LocalPlayer.waterVisionTime`: ticks the eyes have been under water.
    pub(super) water_vision_time: i32,
}

impl OwnBody {
    /// `ClientPacketListener.handleRespawn`: the new player starts afresh,
    /// keeping the synced data only under `KEEP_ENTITY_DATA`.
    pub(super) fn respawned(&mut self, keep_entity_data: bool) {
        let frozen_ticks = if keep_entity_data {
            self.frozen_ticks
        } else {
            0
        };
        *self = Self {
            frozen_ticks,
            ..Self::default()
        };
    }
}

impl InWorld {
    /// The attributes the server sets on the local player.
    pub(super) fn own_attributes(
        &mut self,
        snapshots: &[melon_net::play::game::AttributeSnapshot],
    ) {
        for snapshot in snapshots {
            self.player.set_attribute(
                snapshot.attribute as u8,
                snapshot.base,
                snapshot.modifiers.clone(),
            );
        }
        self.refresh_attribute_bars();
    }

    /// The bars that read attributes, from what the player holds of them
    /// now: `getMaxHealth` for the hearts, `getArmorValue` for the armour
    /// row. Called whenever the attributes change, by the server or by a
    /// respawn dropping their modifiers.
    pub(super) fn refresh_attribute_bars(&mut self) {
        let attributes = self.player.attributes();
        self.status.max_health = attributes.max_health();
        self.status.armor = attributes.armor();
    }

    /// A new `LocalPlayer` (`handleRespawn`, `handleLogin`): what the bars
    /// show starts as a new player's. Without `KEEP_ENTITY_DATA` its synced
    /// data is the defaults: full health (`LivingEntity` starts at
    /// `getMaxHealth()`, 20 before any attribute is sent), no absorption, a
    /// full breath. Always new: `FoodData` (20, 5), the experience (0) and
    /// no effects; the server sends its own right after.
    pub(super) fn respawn_status(&mut self, keep_entity_data: bool) {
        if !keep_entity_data {
            self.status.health = DEFAULT_MAX_HEALTH;
            self.status.absorption = 0.0;
            self.status.air = MAX_AIR;
        }
        let fresh = melon_gui::hud::PlayerStatus::default();
        self.status.food = fresh.food;
        self.status.saturation = fresh.saturation;
        self.status.experience_progress = fresh.experience_progress;
        self.status.experience_level = fresh.experience_level;
        self.status_effects.clear();
        self.mining_effects = Default::default();
        self.effects.set_nausea_ticks(0);
        self.refresh_attribute_bars();
        self.tick_vitals();
    }

    /// The bars' numbers out of the local player's synced data.
    pub(super) fn own_vitals_data(&mut self, values: &[(u8, DataValue)]) {
        for (index, value) in values {
            if *index == player::DATA_PLAYER_ABSORPTION_ID.id.0 {
                self.status.absorption = value.float().unwrap_or(0.0);
            } else if *index == avatar::DATA_AIR_SUPPLY_ID.id.0 {
                self.status.air = value.int().unwrap_or(MAX_AIR);
            } else if *index == entity::DATA_TICKS_FROZEN.id.0 {
                self.body.frozen_ticks = value.int().unwrap_or(0);
            }
        }
    }

    /// Once a tick.
    pub(super) fn tick_vitals(&mut self) {
        let effects = &self.status_effects;
        self.status.hunger = effects.get(HUNGER_ID).is_some();
        self.status.regeneration = effects.get(REGENERATION_ID).is_some();
        self.status.heart_kind = HeartKind::of(
            effects.get(WITHER_ID).is_some(),
            effects.get(POISON_ID).is_some(),
            self.body.frozen_ticks >= FROZEN_TICKS,
        );
        self.status.under_water = self.player.eye_in_water();
        self.status.mount = self.entities.own_mount_health();
    }
}
