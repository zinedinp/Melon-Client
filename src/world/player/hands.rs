//! What the local player's hands are doing, tick by tick.

use pumpkin_data::AttributeModifierSlot;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component_impl::{
    AttributeModifiersImpl, BlocksAttacksImpl, ConsumableImpl, ConsumeAnimation, FoodImpl,
    Operation, SwingAnimationImpl, SwingAnimationType, UseEffectsImpl,
};
use pumpkin_data::item_stack::ItemStack;

use crate::world::entity::humanoid::ArmPose;
use crate::world::entity::player::Arm;

/// `InteractionHand`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Main,
    Off,
}

impl Hand {
    /// `InteractionHand.values()`: the order right-click tries them in.
    pub const ALL: [Self; 2] = [Self::Main, Self::Off];

    /// `asArm`: which arm holds it.
    pub fn arm(self, main_arm: Arm) -> Arm {
        match self {
            Self::Main => main_arm,
            Self::Off => main_arm.opposite(),
        }
    }

    /// The id the packets send.
    pub fn id(self) -> i32 {
        match self {
            Self::Main => 0,
            Self::Off => 1,
        }
    }
}

/// `SwingAnimation`: how an item swings and for how many ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swing {
    /// `WHACK` moves the arm; `NONE` doesn't.
    pub whack: bool,
    pub duration: i32,
}

impl Swing {
    /// `SwingAnimation.DEFAULT`.
    pub const DEFAULT: Self = Self {
        whack: true,
        duration: 6,
    };

    /// `ItemStack.getAttackAnimation`/`getInteractAnimation`.
    pub fn of(stack: &ItemStack) -> Self {
        stack
            .get_data_component::<SwingAnimationImpl>()
            .map_or(Self::DEFAULT, |swing| Self {
                whack: swing.animation_type == SwingAnimationType::Whack,
                duration: swing.duration,
            })
    }
}

/// `LivingEntity.SwingState`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SwingState {
    current: Option<(Hand, Swing)>,
    ticks: i32,
    old_animation: f32,
    animation: f32,
}

impl SwingState {
    /// `startIfAble`: a new swing, unless one is in its first half.
    pub fn start_if_able(&mut self, hand: Hand, swing: Swing) -> bool {
        if let Some((_, current)) = self.current
            && self.ticks <= current.duration / 2
            && self.ticks > 0
        {
            return false;
        }
        if self.current != Some((hand, swing)) {
            self.old_animation = 0.0;
            self.animation = 0.0;
        }
        self.current = Some((hand, swing));
        self.ticks = 0;
        true
    }

    pub fn tick(&mut self) {
        self.old_animation = self.animation;
        let Some((_, swing)) = self.current else {
            self.animation = 0.0;
            return;
        };
        if swing.duration > 0 {
            self.animation = (self.ticks as f32 / swing.duration as f32).min(1.0);
        }
        let ticks = self.ticks;
        self.ticks += 1;
        if ticks > swing.duration {
            self.current = None;
            self.animation = 0.0;
        }
    }

    /// `getAnimation`.
    pub fn animation(&self, partial: f32) -> f32 {
        let end = if self.animation >= self.old_animation {
            self.animation
        } else {
            self.animation + 1.0
        };
        self.old_animation + (end - self.old_animation) * partial
    }

    /// The hand swinging and how, if one is.
    pub fn current(&self) -> Option<(Hand, Swing)> {
        self.current
    }
}

/// `ItemUseAnimation`, the ones Melon draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseAnimation {
    None,
    Eat,
    Drink,
    Block,
    Bow,
    Trident,
}

impl UseAnimation {
    /// `AvatarRenderer.getArmPose` for an arm using an item that way.
    pub fn arm_pose(self) -> ArmPose {
        match self {
            Self::Block => ArmPose::Block,
            Self::Bow => ArmPose::BowAndArrow,
            Self::Trident => ArmPose::ThrowTrident,
            Self::None | Self::Eat | Self::Drink => ArmPose::Item,
        }
    }
}

/// What holding use with an item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemUse {
    pub animation: UseAnimation,
    pub duration: i32,
    pub can_sprint: bool,
    pub speed: f32,
}

impl ItemUse {
    /// `Consumable.shouldEmitParticlesAndSounds`.
    pub fn consume_effect_due(&self, remaining: i32) -> bool {
        if !matches!(self.animation, UseAnimation::Eat | UseAnimation::Drink) {
            return false;
        }
        let used = self.duration - remaining;
        let wait = (self.duration as f32 * 0.21875) as i32;
        used > wait && remaining % 4 == 0
    }

    /// How `stack` is used when held down, if it is.
    pub fn of(stack: &ItemStack) -> Option<Self> {
        let (animation, duration) =
            if let Some(consumable) = stack.get_data_component::<ConsumableImpl>() {
                let animation = match consumable.animation {
                    ConsumeAnimation::Eat => UseAnimation::Eat,
                    ConsumeAnimation::Drink => UseAnimation::Drink,
                    ConsumeAnimation::Block => UseAnimation::Block,
                    _ => UseAnimation::None,
                };
                (animation, (consumable.consume_seconds * 20.0) as i32)
            } else if stack.get_data_component::<BlocksAttacksImpl>().is_some() {
                (UseAnimation::Block, 72_000)
            } else {
                let (animation, duration) = match stack.item.registry_key {
                    "bow" => (UseAnimation::Bow, 72_000),
                    "trident" => (UseAnimation::Trident, 72_000),
                    "crossbow" => (UseAnimation::None, 72_000),
                    "spyglass" => (UseAnimation::None, 1_200),
                    "goat_horn" => (UseAnimation::None, 140),
                    "brush" => (UseAnimation::None, 200),
                    _ => return None,
                };
                (animation, duration)
            };
        let effects = stack
            .get_data_component::<UseEffectsImpl>()
            .copied()
            .unwrap_or_default();
        Some(Self {
            animation,
            duration,
            can_sprint: effects.can_sprint,
            speed: effects.speed_multiplier,
        })
    }

    /// `Consumable.canConsume`.
    pub fn can_start(stack: &ItemStack, food: i32) -> bool {
        match stack.get_data_component::<FoodImpl>() {
            Some(edible) => edible.can_always_eat || food < 20,
            None => true,
        }
    }
}

/// The item being used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsingItem {
    pub hand: Hand,
    pub item_use: ItemUse,
    pub remaining: i32,
    /// Ticks since `remaining` ran out with the server still saying the item is in use.
    pub overrun: u8,
}

/// Ticks the server gets, past the time being up, to say the item is done.
pub const OVERRUN_TICKS: u8 = 3;

/// `Consumable.emitParticlesAndSounds`' sound.
pub fn consume_sound(stack: &ItemStack, mut r: impl FnMut() -> f32) -> Option<(String, f32, f32)> {
    use pumpkin_data::data_component_impl::IdOr;
    let consumable = stack.get_data_component::<ConsumableImpl>()?;
    let name = match &consumable.sound_event {
        IdOr::Id(sound) => sound.to_name().to_owned(),
        IdOr::Value(event) => event.sound_name.clone(),
    };
    let name = name.strip_prefix("minecraft:").unwrap_or(&name).to_owned();
    let (volume, pitch) = if consumable.animation == ConsumeAnimation::Drink {
        (0.5, 0.9 + r() * 0.1)
    } else {
        (if r() < 0.5 { 0.5 } else { 1.0 }, 1.0 + 0.2 * (r() - r()))
    };
    Some((name, volume, pitch))
}

/// `Player.getCurrentItemAttackStrengthDelay`.
pub fn attack_delay(stack: &ItemStack) -> f32 {
    let mut speed = Attributes::ATTACK_SPEED.default_value;
    if let Some(modifiers) = stack.get_data_component::<AttributeModifiersImpl>() {
        let applies = |slot: &AttributeModifierSlot| {
            matches!(
                slot,
                AttributeModifierSlot::MainHand
                    | AttributeModifierSlot::Hand
                    | AttributeModifierSlot::Any
            )
        };
        let speeds = modifiers
            .attribute_modifiers
            .iter()
            .filter(|m| m.r#type.name == Attributes::ATTACK_SPEED.name && applies(&m.slot));
        let (mut add, mut base, mut total) = (0.0, 0.0, 1.0);
        for modifier in speeds {
            match modifier.operation {
                Operation::AddValue => add += modifier.amount,
                Operation::AddMultipliedBase => base += modifier.amount,
                Operation::AddMultipliedTotal => total *= 1.0 + modifier.amount,
            }
        }
        let value = speed + add;
        speed = (value + value * base) * total;
    }
    (1.0 / speed.max(1.0e-3) * 20.0) as f32
}

/// The same item, so a change to it (fewer, damaged) doesn't restart the use.
fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.is_empty() && b.is_empty() || a.item.registry_key == b.item.registry_key
}

/// `ItemStack.matches`: same item, count and components.
fn matches(a: &ItemStack, b: &ItemStack) -> bool {
    a.item_count == b.item_count && a.are_items_and_components_equal(b)
}

/// Everything the hands do between ticks.
#[derive(Clone)]
pub struct Hands {
    pub swing: SwingState,
    /// `itemSwapTicker`: ticks since the main hand last changed item.
    swap_ticker: i32,
    last_main: ItemStack,
    pub using: Option<UsingItem>,
    /// Set by a tick that is due the bites' sounds (`Consumable.onUseTick`):
    pub consuming: Option<Hand>,
    /// The items each first-person hand shows.
    shown: [ItemStack; 2],
    /// `mainHandHeight`/`offHandHeight` now and a tick ago.
    height: [f32; 2],
    old_height: [f32; 2],
}

impl Default for Hands {
    fn default() -> Self {
        Self {
            swing: SwingState::default(),
            swap_ticker: 0,
            last_main: ItemStack::EMPTY.clone(),
            using: None,
            consuming: None,
            shown: [ItemStack::EMPTY.clone(), ItemStack::EMPTY.clone()],
            height: [0.0; 2],
            old_height: [0.0; 2],
        }
    }
}

impl Hands {
    /// One tick with `main` and `off` held.
    pub fn tick(&mut self, main: &ItemStack, off: &ItemStack) {
        self.swap_ticker += 1;
        if !matches(&self.last_main, main) {
            if !same_item(&self.last_main, main) {
                self.swap_ticker = 0;
            }
            self.last_main = main.clone();
        }

        if let Some(using) = &mut self.using {
            let held = match using.hand {
                Hand::Main => main,
                Hand::Off => off,
            };
            if held.is_empty() || ItemUse::of(held) != Some(using.item_use) {
                self.using = None;
            } else if using.remaining > 0 {
                if using.item_use.consume_effect_due(using.remaining) {
                    self.consuming = Some(using.hand);
                }
                using.remaining -= 1;
            } else {
                using.overrun = using.overrun.saturating_add(1);
            }
        }

        self.swing.tick();

        let swap = (self.swap_ticker as f32 + 1.0) / attack_delay(main);
        let swap = swap.clamp(0.0, 1.0);
        for (index, next) in [main, off].into_iter().enumerate() {
            self.old_height[index] = self.height[index];
            if same_item(&self.shown[index], next) {
                self.shown[index] = next.clone();
            }
            let target = if !same_item(&self.shown[index], next) {
                0.0
            } else if index == 0 {
                swap * swap * swap
            } else {
                1.0
            };
            self.height[index] += (target - self.height[index]).clamp(-0.4, 0.4);
            if self.height[index] < 0.1 {
                self.shown[index] = next.clone();
            }
        }
    }

    /// `Player.getAttackStrengthScale(0)` with `main` held.
    pub fn attack_strength(&self, main: &ItemStack) -> f32 {
        (self.swap_ticker as f32 / attack_delay(main)).clamp(0.0, 1.0)
    }

    /// `resetAttackStrengthTicker`.
    pub fn reset_attack_strength(&mut self) {
        self.swap_ticker = 0;
    }

    /// `itemUsed`.
    pub fn item_used(&mut self, hand: Hand) {
        self.height[hand_index(hand)] = 0.0;
    }

    /// `swing`.
    pub fn swing(&mut self, hand: Hand, stack: &ItemStack) -> bool {
        self.swing.start_if_able(hand, Swing::of(stack))
    }

    /// The item `hand` shows in first person.
    pub fn shown(&self, hand: Hand) -> &ItemStack {
        &self.shown[hand_index(hand)]
    }

    /// `1 - height`, between the last tick and this one.
    pub fn lowered(&self, hand: Hand, partial: f32) -> f32 {
        let i = hand_index(hand);
        1.0 - (self.old_height[i] + (self.height[i] - self.old_height[i]) * partial)
    }

    /// Whether `hand` is using its item right now, and how.
    pub fn using_in(&self, hand: Hand) -> Option<&UsingItem> {
        self.using.as_ref().filter(|using| using.hand == hand)
    }
}

fn hand_index(hand: Hand) -> usize {
    match hand {
        Hand::Main => 0,
        Hand::Off => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    fn stack(item: &'static Item) -> ItemStack {
        ItemStack::new(1, item)
    }

    #[test]
    fn a_swing_runs_six_ticks_and_restarts_only_in_its_second_half() {
        let mut swing = SwingState::default();
        assert!(swing.start_if_able(Hand::Main, Swing::DEFAULT));
        swing.tick();
        swing.tick();
        assert!(!swing.start_if_able(Hand::Main, Swing::DEFAULT));
        for _ in 0..2 {
            swing.tick();
        }
        assert!((swing.animation(1.0) - 3.0 / 6.0).abs() < 1e-6);
        assert!(swing.start_if_able(Hand::Main, Swing::DEFAULT));
        for _ in 0..8 {
            swing.tick();
        }
        assert!(swing.current().is_none());
        swing.tick();
        assert_eq!(swing.animation(1.0), 0.0);
    }

    #[test]
    fn a_sword_recharges_slower_than_a_fist() {
        assert!((attack_delay(ItemStack::EMPTY) - 5.0).abs() < 1e-4);
        let sword = attack_delay(&stack(&Item::DIAMOND_SWORD));
        assert!((sword - 12.5).abs() < 1e-3, "{sword}");
    }

    #[test]
    fn a_new_item_lowers_the_hand_then_raises_it() {
        let mut hands = Hands::default();
        let empty = ItemStack::EMPTY.clone();
        for _ in 0..20 {
            hands.tick(&empty, &empty);
        }
        assert_eq!(hands.lowered(Hand::Main, 1.0), 0.0);
        let pickaxe = stack(&Item::DIAMOND_PICKAXE);
        hands.tick(&pickaxe, &empty);
        assert!(hands.shown(Hand::Main).is_empty());
        assert!(hands.lowered(Hand::Main, 1.0) > 0.3);
        for _ in 0..3 {
            hands.tick(&pickaxe, &empty);
        }
        assert_eq!(hands.shown(Hand::Main).item.registry_key, "diamond_pickaxe");
        for _ in 0..30 {
            hands.tick(&pickaxe, &empty);
        }
        assert!(hands.lowered(Hand::Main, 1.0) < 1e-4);
    }

    #[test]
    fn the_same_item_or_an_empty_hand_again_has_no_cooldown() {
        let mut hands = Hands::default();
        let empty = ItemStack::EMPTY.clone();
        let many = ItemStack::new(64, &Item::DIRT);
        let few = ItemStack::new(3, &Item::DIRT);
        for _ in 0..20 {
            hands.tick(&many, &empty);
        }
        assert_eq!(hands.attack_strength(&many), 1.0);
        hands.tick(&few, &empty);
        assert_eq!(hands.lowered(Hand::Main, 1.0), 0.0);
        assert_eq!(hands.attack_strength(&few), 1.0);
        let mut hands = Hands::default();
        for _ in 0..20 {
            hands.tick(&empty, &empty);
        }
        hands.tick(&ItemStack::EMPTY.clone(), &empty);
        assert_eq!(hands.lowered(Hand::Main, 1.0), 0.0);
        assert_eq!(hands.attack_strength(&empty), 1.0);
        let sword = ItemStack::new(1, &Item::DIAMOND_SWORD);
        hands.tick(&sword, &empty);
        assert!(hands.attack_strength(&sword) < 0.2);
    }

    #[test]
    fn using_up_a_block_does_not_lower_the_hand_but_placing_does() {
        let mut hands = Hands::default();
        let off = ItemStack::EMPTY.clone();
        let mut dirt = ItemStack::new(5, &Item::DIRT);
        for _ in 0..20 {
            hands.tick(&dirt, &off);
        }
        dirt.item_count = 4;
        hands.tick(&dirt, &off);
        assert_eq!(hands.lowered(Hand::Main, 1.0), 0.0);
        hands.item_used(Hand::Main);
        assert_eq!(hands.lowered(Hand::Main, 1.0), 1.0);
    }

    #[test]
    fn a_shield_blocks_and_food_is_eaten() {
        let shield = ItemUse::of(&stack(&Item::SHIELD)).unwrap();
        assert_eq!(shield.animation, UseAnimation::Block);
        let bread = ItemUse::of(&stack(&Item::BREAD)).unwrap();
        assert_eq!(bread.animation, UseAnimation::Eat);
        assert_eq!(bread.duration, 32);
        assert!(!ItemUse::can_start(&stack(&Item::BREAD), 20));
        assert!(ItemUse::can_start(&stack(&Item::BREAD), 19));
        assert!(ItemUse::of(&stack(&Item::DIAMOND_PICKAXE)).is_none());
    }
}
