//! A villager's or wandering trader's offers, as `ClientboundMerchantOffersPacket`

use pumpkin_data::item_stack::ItemStack;

/// `MerchantOffer`.
#[derive(Clone)]
pub struct Offer {
    /// `getBaseCostA`.
    pub base_cost_a: ItemStack,
    /// `getCostB`; empty for a trade with one payment.
    pub cost_b: ItemStack,
    pub result: ItemStack,
    pub out_of_stock: bool,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub special_price_diff: i32,
    pub price_multiplier: f32,
    pub demand: i32,
}

impl Offer {
    /// `getCostA`.
    pub fn cost_a(&self) -> ItemStack {
        let base = i32::from(self.base_cost_a.item_count);
        let demand = (base as f32 * self.demand as f32 * self.price_multiplier)
            .floor()
            .max(0.0) as i32;
        let max = i32::from(self.base_cost_a.get_max_stack_size());
        let count = (base + demand + self.special_price_diff).clamp(1, max);
        self.base_cost_a.copy_with_count(count as u8)
    }
}

/// The packet's offers with the trader's level and experience.
#[derive(Clone, Default)]
pub struct Offers {
    pub offers: Vec<Offer>,
    /// `villagerLevel`, 1 (novice) to 5 (master).
    pub level: i32,
    pub xp: i32,
    /// Whether the experience bar shows (a villager, not a wandering trader).
    pub show_progress: bool,
    pub can_restock: bool,
}

/// `NEXT_LEVEL_XP_THRESHOLDS`: the experience each level starts at.
const LEVEL_XP: [i32; 5] = [0, 10, 70, 150, 250];

/// `VillagerData.getMinXpPerLevel`; nothing past the top level.
pub fn min_xp(level: i32) -> i32 {
    if can_level_up(level) {
        LEVEL_XP[(level - 1) as usize]
    } else {
        0
    }
}

/// `VillagerData.getMaxXpPerLevel`: where the next level starts.
pub fn max_xp(level: i32) -> i32 {
    if can_level_up(level) {
        LEVEL_XP[level as usize]
    } else {
        0
    }
}

/// `VillagerData.canLevelUp`.
pub fn can_level_up(level: i32) -> bool {
    (1..5).contains(&level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    fn offer(count: u8, demand: i32, diff: i32, multiplier: f32) -> Offer {
        Offer {
            base_cost_a: ItemStack::new(count, &Item::EMERALD),
            cost_b: crate::inventory::empty(),
            result: ItemStack::new(1, &Item::BREAD),
            out_of_stock: false,
            uses: 0,
            max_uses: 12,
            xp: 1,
            special_price_diff: diff,
            price_multiplier: multiplier,
            demand,
        }
    }

    #[test]
    fn demand_raises_the_price_and_reputation_lowers_it() {
        assert_eq!(offer(10, 0, 0, 0.05).cost_a().item_count, 10);
        assert_eq!(offer(10, 4, 0, 0.05).cost_a().item_count, 12);
        assert_eq!(offer(10, 4, -5, 0.05).cost_a().item_count, 7);
    }

    #[test]
    fn a_price_stays_between_one_and_a_stack() {
        assert_eq!(offer(2, 0, -50, 0.05).cost_a().item_count, 1);
        assert_eq!(offer(60, 100, 0, 1.0).cost_a().item_count, 64);
    }

    #[test]
    fn levels_start_at_vanillas_experience() {
        assert_eq!((min_xp(1), max_xp(1)), (0, 10));
        assert_eq!((min_xp(4), max_xp(4)), (150, 250));
        assert!(can_level_up(4));
        assert!(!can_level_up(5));
        assert_eq!((min_xp(5), max_xp(5)), (0, 0));
    }
}
