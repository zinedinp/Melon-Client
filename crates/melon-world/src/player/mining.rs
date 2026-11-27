//! How fast the player breaks blocks.

use pumpkin_data::data_component_impl::{EnchantmentsImpl, ToolImpl};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{Block, BlockStateId};

/// `MobEffects` ids of the effects that change mining.
pub const HASTE_ID: i32 = 2;
pub const MINING_FATIGUE_ID: i32 = 3;
pub const CONDUIT_POWER_ID: i32 = 28;

/// The mining effects on the player, by amplifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MiningEffects {
    pub haste: Option<i32>,
    pub conduit_power: Option<i32>,
    pub mining_fatigue: Option<i32>,
}

impl MiningEffects {
    /// `ClientboundUpdateMobEffectPacket`/`RemoveMobEffect` for one of the mining effects.
    pub fn set(&mut self, effect: i32, amplifier: Option<i32>) {
        match effect {
            HASTE_ID => self.haste = amplifier,
            CONDUIT_POWER_ID => self.conduit_power = amplifier,
            MINING_FATIGUE_ID => self.mining_fatigue = amplifier,
            _ => {}
        }
    }

    /// `MobEffectUtil.getDigSpeedAmplification`.
    fn dig_speed(self) -> Option<i32> {
        match (self.haste, self.conduit_power) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// `LivingEntity.getModifiedSwingDuration`.
    pub fn swing_duration(self, duration: i32) -> i32 {
        if let Some(amplifier) = self.dig_speed() {
            duration - (1 + amplifier)
        } else if let Some(amplifier) = self.mining_fatigue {
            duration + (1 + amplifier) * 2
        } else {
            duration
        }
    }
}

/// What the speed depends on besides the block.
#[derive(Clone, Copy)]
pub struct Miner<'a> {
    pub held: &'a ItemStack,
    /// For Aqua Affinity.
    pub helmet: &'a ItemStack,
    pub effects: MiningEffects,
    /// [`PlayerAttributes::mining_efficiency`](super::PlayerAttributes::mining_efficiency).
    pub efficiency: Option<f32>,
    /// [`PlayerAttributes::block_break_speed`](super::PlayerAttributes::block_break_speed).
    pub break_speed: f32,
    pub eye_in_water: bool,
    pub on_ground: bool,
}

pub fn enchantment_level(stack: &ItemStack, name: &str) -> i32 {
    stack
        .get_data_component::<EnchantmentsImpl>()
        .and_then(|enchantments| {
            enchantments
                .enchantment
                .iter()
                .find(|(enchantment, _)| enchantment.name == name)
                .map(|&(_, level)| level)
        })
        .unwrap_or(0)
}

impl Miner<'_> {
    /// `Player.getDestroySpeed`.
    fn destroy_speed(&self, block: &'static Block) -> f32 {
        let mut speed = self.held.get_speed(block);
        if speed > 1.0 {
            speed += self.efficiency.unwrap_or_else(|| {
                let level = enchantment_level(self.held, "minecraft:efficiency");
                if level > 0 {
                    (level * level + 1) as f32
                } else {
                    0.0
                }
            });
        }
        if let Some(amplifier) = self.effects.dig_speed() {
            speed *= 1.0 + (amplifier + 1) as f32 * 0.2;
        }
        if let Some(amplifier) = self.effects.mining_fatigue {
            speed *= 0.3f32.powi(amplifier + 1);
        }
        speed *= self.break_speed;
        if self.eye_in_water && enchantment_level(self.helmet, "minecraft:aqua_affinity") == 0 {
            speed *= 0.2;
        }
        if !self.on_ground {
            speed /= 5.0;
        }
        speed
    }

    /// `BlockBehaviour.getDestroyProgress`.
    pub fn destroy_progress(&self, state: u16) -> f32 {
        let Some(id) = BlockStateId::new(state) else {
            return 0.0;
        };
        let block_state = id.to_state();
        let block = Block::from_state_id(id);
        let hardness = if block_state.hardness >= 0.0 {
            block_state.hardness
        } else {
            block.hardness
        };
        if hardness < 0.0 {
            return 0.0;
        }
        if hardness == 0.0 {
            return 1.0;
        }
        let correct = !block_state.tool_required() || self.held.is_correct_for_drops(block);
        let modifier = if correct { 30.0 } else { 100.0 };
        self.destroy_speed(block) / hardness / modifier
    }
}

/// `Item.canDestroyBlock` in creative.
pub fn breaks_blocks_in_creative(held: &ItemStack) -> bool {
    held.get_data_component::<ToolImpl>()
        .is_none_or(|tool| tool.can_destroy_blocks_in_creative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    fn state(name: &str) -> u16 {
        Block::from_registry_key(name)
            .unwrap()
            .default_state
            .id
            .as_u16()
    }

    fn miner(held: &ItemStack, effects: MiningEffects) -> Miner<'_> {
        Miner {
            held,
            helmet: ItemStack::EMPTY,
            effects,
            efficiency: None,
            break_speed: 1.0,
            eye_in_water: false,
            on_ground: true,
        }
    }

    #[test]
    fn weapons_do_not_break_blocks_in_creative() {
        for item in [
            &Item::DIAMOND_SWORD,
            &Item::WOODEN_SWORD,
            &Item::TRIDENT,
            &Item::MACE,
        ] {
            let stack = ItemStack::new(1, item);
            assert!(!breaks_blocks_in_creative(&stack), "{}", item.registry_key);
        }
        for item in [&Item::DIAMOND_PICKAXE, &Item::STICK, &Item::AIR] {
            let stack = ItemStack::new(1, item);
            assert!(breaks_blocks_in_creative(&stack), "{}", item.registry_key);
        }
    }

    #[test]
    fn bedrock_does_not_break_and_flowers_break_at_once() {
        let hand = ItemStack::EMPTY;
        assert_eq!(
            miner(hand, MiningEffects::default()).destroy_progress(state("bedrock")),
            0.0
        );
        assert_eq!(
            miner(hand, MiningEffects::default()).destroy_progress(state("dandelion")),
            1.0
        );
        let dirt = miner(hand, MiningEffects::default()).destroy_progress(state("dirt"));
        assert!(dirt > 0.0 && dirt < 1.0, "{dirt}");
    }

    #[test]
    fn haste_two_and_a_netherite_pickaxe_break_stone_at_once() {
        let pickaxe = ItemStack::new(1, &Item::NETHERITE_PICKAXE);
        let stone = state("stone");
        let plain = miner(&pickaxe, MiningEffects::default()).destroy_progress(stone);
        let hasted = miner(
            &pickaxe,
            MiningEffects {
                haste: Some(1),
                ..Default::default()
            },
        )
        .destroy_progress(stone);
        assert!(plain < 1.0);
        assert!((hasted / plain - 1.4).abs() < 1e-4, "{plain} {hasted}");
    }

    #[test]
    fn jumping_and_water_slow_mining() {
        let pickaxe = ItemStack::new(1, &Item::IRON_PICKAXE);
        let stone = state("stone");
        let ground = miner(&pickaxe, MiningEffects::default());
        let air = Miner {
            on_ground: false,
            ..ground
        };
        let water = Miner {
            eye_in_water: true,
            ..ground
        };
        let base = ground.destroy_progress(stone);
        assert!((air.destroy_progress(stone) * 5.0 - base).abs() < 1e-6);
        assert!((water.destroy_progress(stone) * 5.0 - base).abs() < 1e-6);
    }

    #[test]
    fn haste_swings_faster_and_fatigue_slower() {
        let haste = MiningEffects {
            haste: Some(1),
            ..Default::default()
        };
        assert_eq!(haste.swing_duration(6), 4);
        let fatigue = MiningEffects {
            mining_fatigue: Some(0),
            ..Default::default()
        };
        assert_eq!(fatigue.swing_duration(6), 8);
    }
}
