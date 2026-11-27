//! The status effects on the local player (vanilla `MobEffectInstance`s).

use pumpkin_data::data_component_impl::IDSetContent;
use pumpkin_data::effect::{MobEffectCategory, StatusEffect};

/// `MobEffects` registry ids this client acts on.
pub const SPEED_ID: i32 = 0;
pub const SLOWNESS_ID: i32 = 1;
pub const BLINDNESS_ID: i32 = 14;
pub const SLOW_FALLING_ID: i32 = pumpkin_data::effect::StatusEffect::SLOW_FALLING.id as i32;
pub const JUMP_BOOST_ID: i32 = pumpkin_data::effect::StatusEffect::JUMP_BOOST.id as i32;
pub const DOLPHINS_GRACE_ID: i32 = pumpkin_data::effect::StatusEffect::DOLPHINS_GRACE.id as i32;
pub const NIGHT_VISION_ID: i32 = 15;
/// The effects the HUD's bars change for.
pub const REGENERATION_ID: i32 = 9;
pub const HUNGER_ID: i32 = 16;
pub const POISON_ID: i32 = 18;
pub const WITHER_ID: i32 = 19;

/// `ClientboundUpdateMobEffectPacket` flags.
const FLAG_AMBIENT: u8 = 1;
const FLAG_VISIBLE: u8 = 2;
const FLAG_SHOW_ICON: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveEffect {
    pub id: i32,
    pub amplifier: i32,
    /// Ticks left; `-1` never ends (`isInfiniteDuration`).
    pub duration: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
}

impl ActiveEffect {
    pub fn infinite(&self) -> bool {
        self.duration == -1
    }

    /// `MobEffectInstance.endsWithin`.
    pub fn ends_within(&self, ticks: i32) -> bool {
        !self.infinite() && self.duration <= ticks
    }

    fn data(&self) -> Option<&'static StatusEffect> {
        u16::try_from(self.id).ok().and_then(StatusEffect::from_id)
    }

    /// `MobEffect.isBeneficial`.
    pub fn beneficial(&self) -> bool {
        self.data()
            .is_some_and(|e| matches!(e.category, MobEffectCategory::Beneficial))
    }

    /// `MobEffect.getColor`.
    fn color(&self) -> i32 {
        self.data().map_or(0, |e| e.color)
    }

    /// The sprite under `mob_effect/`: `minecraft:speed` -> `speed`.
    pub fn sprite_name(&self) -> Option<&'static str> {
        self.data()
            .map(|e| e.minecraft_name.trim_start_matches("minecraft:"))
    }

    /// The translation key of its name (`effect.minecraft.speed`).
    pub fn translation_key(&self) -> Option<&'static str> {
        self.data().map(|e| e.translation_key)
    }

    /// `MobEffectInstance.compareTo`: the order of the lists.
    fn order(&self, other: &Self) -> std::cmp::Ordering {
        const CUT_OFF: i32 = 32147;
        if (self.duration <= CUT_OFF || other.duration <= CUT_OFF)
            && (!self.ambient || !other.ambient)
        {
            self.ambient
                .cmp(&other.ambient)
                .then(self.infinite().cmp(&other.infinite()))
                .then(self.duration.cmp(&other.duration))
                .then(self.color().cmp(&other.color()))
        } else {
            self.ambient
                .cmp(&other.ambient)
                .then(self.color().cmp(&other.color()))
        }
    }
}

/// The effects on the player, one per kind.
#[derive(Clone, Debug, Default)]
pub struct ActiveEffects {
    list: Vec<ActiveEffect>,
}

impl ActiveEffects {
    pub fn set(&mut self, id: i32, amplifier: i32, duration: i32, flags: u8) {
        let effect = ActiveEffect {
            id,
            amplifier,
            duration,
            ambient: flags & FLAG_AMBIENT != 0,
            visible: flags & FLAG_VISIBLE != 0,
            show_icon: flags & FLAG_SHOW_ICON != 0,
        };
        match self.list.iter_mut().find(|e| e.id == id) {
            Some(old) => *old = effect,
            None => self.list.push(effect),
        }
    }

    pub fn remove(&mut self, id: i32) {
        self.list.retain(|e| e.id != id);
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn get(&self, id: i32) -> Option<&ActiveEffect> {
        self.list.iter().find(|e| e.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// One tick down (`MobEffectInstance.tickDownDuration`).
    pub fn tick(&mut self) {
        for effect in &mut self.list {
            if effect.duration > 0 {
                effect.duration -= 1;
            }
        }
    }

    /// `Ordering.natural().sortedCopy`.
    pub fn sorted(&self) -> Vec<ActiveEffect> {
        let mut sorted = self.list.clone();
        sorted.sort_by(ActiveEffect::order);
        sorted
    }

    /// The player's `MOVEMENT_SPEED` from the effects.
    pub fn speed_factor(&self) -> f32 {
        let level = |id| self.get(id).map(|e| (e.amplifier + 1) as f32);
        let mut factor = 1.0;
        if let Some(l) = level(SPEED_ID) {
            factor *= 1.0 + 0.2 * l;
        }
        if let Some(l) = level(SLOWNESS_ID) {
            factor *= 1.0 - 0.15 * l;
        }
        factor.max(0.0)
    }
}

/// `StringUtil.formatTickDuration` at 20 ticks a second.
pub fn format_duration(ticks: i32) -> String {
    let seconds = (ticks.max(0) / 20) as i64;
    let (minutes, seconds) = (seconds / 60, seconds % 60);
    let (hours, minutes) = (minutes / 60, minutes % 60);
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_and_slowness_multiply_and_never_go_below_zero() {
        let mut effects = ActiveEffects::default();
        assert_eq!(effects.speed_factor(), 1.0);
        effects.set(SPEED_ID, 1, 600, 7);
        assert!((effects.speed_factor() - 1.4).abs() < 1e-6);
        effects.set(SLOWNESS_ID, 0, 600, 7);
        assert!((effects.speed_factor() - 1.4 * 0.85).abs() < 1e-6);
        effects.set(SLOWNESS_ID, 9, 600, 7);
        assert!(effects.speed_factor() >= 0.0);
        effects.remove(SPEED_ID);
        effects.remove(SLOWNESS_ID);
        assert!(effects.is_empty());
    }

    #[test]
    fn an_update_replaces_the_effect_and_the_flags_are_read() {
        let mut effects = ActiveEffects::default();
        effects.set(SPEED_ID, 0, 100, 1);
        effects.set(SPEED_ID, 2, 50, 2 | 4);
        assert_eq!(effects.len(), 1);
        let speed = effects.get(SPEED_ID).unwrap();
        assert_eq!((speed.amplifier, speed.duration), (2, 50));
        assert!(!speed.ambient && speed.visible && speed.show_icon);
        assert!(speed.beneficial());
        assert_eq!(speed.sprite_name(), Some("speed"));
        assert!(!effects.get(SPEED_ID).unwrap().infinite());
    }

    #[test]
    fn durations_count_down_and_stop_at_zero_but_infinite_ones_stay() {
        let mut effects = ActiveEffects::default();
        effects.set(SPEED_ID, 0, 1, 6);
        effects.set(NIGHT_VISION_ID, 0, -1, 6);
        effects.tick();
        effects.tick();
        assert_eq!(effects.get(SPEED_ID).unwrap().duration, 0);
        assert_eq!(effects.get(NIGHT_VISION_ID).unwrap().duration, -1);
    }

    #[test]
    fn the_list_puts_what_ends_first_first_and_ambient_ones_last() {
        let mut effects = ActiveEffects::default();
        effects.set(SPEED_ID, 0, 600, 6);
        effects.set(NIGHT_VISION_ID, 0, 100, 6);
        effects.set(BLINDNESS_ID, 0, 50, 1 | 2 | 4);
        let ids: Vec<i32> = effects.sorted().iter().map(|e| e.id).collect();
        assert_eq!(ids, [NIGHT_VISION_ID, SPEED_ID, BLINDNESS_ID]);
    }

    #[test]
    fn durations_read_like_vanillas_clock() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(20 * 75), "01:15");
        assert_eq!(format_duration(20 * 3725), "01:02:05");
    }
}
