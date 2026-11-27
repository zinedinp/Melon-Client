//! `BeaconScreen`.

use pumpkin_data::effect::StatusEffect;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use super::{Frame, Loader, MenuDraw, MenuScreen};
use crate::overlay::CLICK;
use crate::overlay::container_input::MouseButtonKind;
use crate::overlay::{ChatExit, OverlayAction, OverlayContext};
use crate::sprite::GuiSprite;
use crate::world::inventory::OpenContainer;

/// `BeaconMenu`'s `ContainerData` indices.
const LEVELS: usize = 0;
const PRIMARY: usize = 1;
const SECONDARY: usize = 2;

/// `BeaconBlockEntity.BEACON_EFFECTS`.
const TIERS: [&[&StatusEffect]; 4] = [
    &[&StatusEffect::SPEED, &StatusEffect::HASTE],
    &[&StatusEffect::RESISTANCE, &StatusEffect::JUMP_BOOST],
    &[&StatusEffect::STRENGTH],
    &[&StatusEffect::REGENERATION],
];
const BUTTON: f32 = 22.0;

#[derive(Clone, Copy)]
pub struct Sprites {
    normal: GuiSprite,
    disabled: GuiSprite,
    highlighted: GuiSprite,
    selected: GuiSprite,
    confirm: GuiSprite,
    cancel: GuiSprite,
    /// Speed, haste, resistance, jump boost, strength, regeneration.
    effects: [GuiSprite; 6],
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        let mut effect = |name: &str| load.effect(name);
        let effects = [
            effect("speed")?,
            effect("haste")?,
            effect("resistance")?,
            effect("jump_boost")?,
            effect("strength")?,
            effect("regeneration")?,
        ];
        Ok(Self {
            normal: load.sprite("beacon/button")?,
            disabled: load.sprite("beacon/button_disabled")?,
            highlighted: load.sprite("beacon/button_highlighted")?,
            selected: load.sprite("beacon/button_selected")?,
            confirm: load.sprite("beacon/confirm")?,
            cancel: load.sprite("beacon/cancel")?,
            effects,
        })
    }

    fn effect(&self, effect: &StatusEffect) -> GuiSprite {
        let index = TIERS
            .iter()
            .flat_map(|tier| tier.iter())
            .position(|known| known.id == effect.id)
            .unwrap_or(0);
        self.effects[index]
    }
}

/// What a button does.
#[derive(Clone, Copy, PartialEq)]
enum Action {
    /// Picks `effect` as the primary or the secondary power.
    Power {
        effect: &'static StatusEffect,
        primary: bool,
    },
    /// The tier-three button after the powers.
    Upgrade {
        effect: &'static StatusEffect,
    },
    Confirm,
    Cancel,
}

#[derive(Clone, Copy)]
struct Button {
    action: Action,
    x: f32,
    y: f32,
    /// The pyramid level it needs.
    tier: usize,
}

#[derive(Default)]
pub struct BeaconScreen {
    primary: Option<i32>,
    secondary: Option<i32>,
    /// The server's powers as last seen, to notice a change.
    server: Option<(i16, i16)>,
}

/// `BeaconMenu.decodeEffect`: the data holds the registry id plus one.
fn decode(value: i16) -> Option<i32> {
    (value > 0).then(|| i32::from(value) - 1)
}

fn effect_by_id(id: i32) -> Option<&'static StatusEffect> {
    TIERS
        .iter()
        .flat_map(|tier| tier.iter().copied())
        .find(|effect| i32::from(effect.id) == id)
}

impl BeaconScreen {
    /// `dataChanged`: takes the server's powers when they changed.
    fn follow(&mut self, menu: &OpenContainer) {
        let server = (menu.data(PRIMARY), menu.data(SECONDARY));
        if self.server != Some(server) {
            self.server = Some(server);
            self.primary = decode(server.0);
            self.secondary = decode(server.1);
        }
    }

    /// The buttons as `init` lays them out; the upgrade button only with a fourth tier.
    fn buttons(&self) -> Vec<Button> {
        let mut buttons = Vec::new();
        for (tier, effects) in TIERS.iter().enumerate().take(3) {
            let total = effects.len() as f32 * 22.0 + (effects.len() as f32 - 1.0) * 2.0;
            for (c, &effect) in effects.iter().enumerate() {
                buttons.push(Button {
                    action: Action::Power {
                        effect,
                        primary: true,
                    },
                    x: 76.0 + c as f32 * 24.0 - (total / 2.0).floor(),
                    y: 22.0 + tier as f32 * 25.0,
                    tier,
                });
            }
        }
        let count = TIERS[3].len() + 1;
        let total = count as f32 * 22.0 + (count as f32 - 1.0) * 2.0;
        let x = |c: usize| 167.0 + c as f32 * 24.0 - (total / 2.0).floor();
        for (c, &effect) in TIERS[3].iter().enumerate() {
            buttons.push(Button {
                action: Action::Power {
                    effect,
                    primary: false,
                },
                x: x(c),
                y: 47.0,
                tier: 3,
            });
        }
        if let Some(effect) = self.primary.and_then(effect_by_id) {
            buttons.push(Button {
                action: Action::Upgrade { effect },
                x: x(count - 1),
                y: 47.0,
                tier: 3,
            });
        }
        buttons.push(Button {
            action: Action::Confirm,
            x: 164.0,
            y: 107.0,
            tier: 0,
        });
        buttons.push(Button {
            action: Action::Cancel,
            x: 190.0,
            y: 107.0,
            tier: 0,
        });
        buttons
    }

    fn is_active(&self, button: &Button, menu: &OpenContainer) -> bool {
        match button.action {
            Action::Power { .. } | Action::Upgrade { .. } => {
                button.tier < menu.data(LEVELS).max(0) as usize
            }
            Action::Confirm => !menu.slots[0].is_empty() && self.primary.is_some(),
            Action::Cancel => true,
        }
    }

    fn is_selected(&self, button: &Button) -> bool {
        match button.action {
            Action::Power { effect, primary } => {
                let chosen = if primary {
                    self.primary
                } else {
                    self.secondary
                };
                chosen == Some(i32::from(effect.id))
            }
            Action::Upgrade { effect } => self.secondary == Some(i32::from(effect.id)),
            _ => false,
        }
    }
}

impl MenuScreen for BeaconScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        self.follow(draw.menu);
        let sprites = draw.sprites.beacon;
        for button in self.buttons() {
            let active = self.is_active(&button, draw.menu);
            let sprite = if !active {
                &sprites.disabled
            } else if self.is_selected(&button) {
                &sprites.selected
            } else if draw.frame.hovers(button.x, button.y, BUTTON, BUTTON) {
                &sprites.highlighted
            } else {
                &sprites.normal
            };
            draw.sprite(sprite, button.x, button.y, BUTTON, BUTTON);
            let icon = match button.action {
                Action::Power { effect, .. } | Action::Upgrade { effect } => sprites.effect(effect),
                Action::Confirm => sprites.confirm,
                Action::Cancel => sprites.cancel,
            };
            draw.sprite(&icon, button.x + 2.0, button.y + 2.0, 18.0, 18.0);
        }
        for (item, x) in [
            (&Item::NETHERITE_INGOT, 20.0),
            (&Item::EMERALD, 41.0),
            (&Item::DIAMOND, 63.0),
            (&Item::GOLD_INGOT, 86.0),
            (&Item::IRON_INGOT, 108.0),
        ] {
            draw.item(&ItemStack::new(1, item), x, 109.0);
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        for (key, x) in [
            ("block.minecraft.beacon.primary", 62.0),
            ("block.minecraft.beacon.secondary", 169.0),
        ] {
            let text = draw.ui.lang.get(key).to_owned();
            let width = draw.text_width(&text);
            draw.text_shadowed(&text, (x - width / 2.0).round(), 10.0, 0xFFE0_E0E0);
        }
        let hovered = self
            .buttons()
            .into_iter()
            .find(|b| draw.frame.hovers(b.x, b.y, BUTTON, BUTTON));
        let Some(button) = hovered else {
            return;
        };
        let lang = draw.ui.lang;
        let text = match button.action {
            Action::Power { effect, .. } => lang.get_dynamic(effect.translation_key).to_owned(),
            Action::Upgrade { effect } => {
                format!("{} II", lang.get_dynamic(effect.translation_key))
            }
            Action::Confirm => lang.get("gui.done").to_owned(),
            Action::Cancel => lang.get("gui.cancel").to_owned(),
        };
        draw.tooltip(&[text]);
    }

    fn press(
        &mut self,
        frame: Frame,
        _button: MouseButtonKind,
        ctx: &mut OverlayContext,
    ) -> Option<OverlayAction> {
        let menu = ctx.container.as_deref()?;
        let button = self
            .buttons()
            .into_iter()
            .find(|b| frame.hovers(b.x, b.y, BUTTON, BUTTON))?;
        if !self.is_active(&button, menu) {
            return Some(OverlayAction::None);
        }
        match button.action {
            Action::Power { effect, primary } => {
                if !self.is_selected(&button) {
                    let id = i32::from(effect.id);
                    if primary {
                        self.primary = Some(id);
                        if self.secondary != Some(id) {
                            self.secondary = None;
                        }
                    } else {
                        self.secondary = Some(id);
                    }
                }
                Some(OverlayAction::None.with_sound(CLICK))
            }
            Action::Upgrade { effect } => {
                self.secondary = Some(i32::from(effect.id));
                Some(OverlayAction::None.with_sound(CLICK))
            }
            Action::Confirm => Some(OverlayAction::Sequence(vec![
                OverlayAction::Sound(CLICK),
                OverlayAction::SetBeacon {
                    primary: self.primary,
                    secondary: self.secondary,
                },
                OverlayAction::Close(ChatExit::Done),
            ])),
            Action::Cancel => Some(OverlayAction::Close(ChatExit::Intentional).with_sound(CLICK)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::ContainerKind;

    fn menu(levels: i16, payment: bool) -> OpenContainer {
        let mut menu = OpenContainer::new(1, ContainerKind::Beacon, String::new());
        menu.set_data(LEVELS, levels);
        if payment {
            menu.slots[0] = ItemStack::new(1, &Item::EMERALD).into();
        }
        menu
    }

    fn find(screen: &BeaconScreen, action: impl Fn(&Action) -> bool) -> Button {
        screen
            .buttons()
            .into_iter()
            .find(|b| action(&b.action))
            .unwrap()
    }

    #[test]
    fn a_tier_opens_with_the_pyramid_level_that_unlocks_it() {
        let screen = BeaconScreen::default();
        let speed = find(
            &screen,
            |a| matches!(a, Action::Power { effect, .. } if effect.id == StatusEffect::SPEED.id),
        );
        let regeneration = find(
            &screen,
            |a| matches!(a, Action::Power { effect, primary: false } if effect.id == StatusEffect::REGENERATION.id),
        );
        assert!(!screen.is_active(&speed, &menu(0, false)));
        assert!(screen.is_active(&speed, &menu(1, false)));
        assert!(!screen.is_active(&regeneration, &menu(3, false)));
        assert!(screen.is_active(&regeneration, &menu(4, false)));
    }

    #[test]
    fn done_needs_a_payment_and_a_primary_power() {
        let mut screen = BeaconScreen::default();
        let done = |screen: &BeaconScreen| find(screen, |a| *a == Action::Confirm);
        assert!(!screen.is_active(&done(&screen), &menu(4, true)));
        screen.primary = Some(i32::from(StatusEffect::SPEED.id));
        assert!(!screen.is_active(&done(&screen), &menu(4, false)));
        assert!(screen.is_active(&done(&screen), &menu(4, true)));
    }

    #[test]
    fn the_upgrade_button_appears_with_a_primary_power() {
        let mut screen = BeaconScreen::default();
        let upgrades = |screen: &BeaconScreen| {
            screen
                .buttons()
                .iter()
                .filter(|b| matches!(b.action, Action::Upgrade { .. }))
                .count()
        };
        assert_eq!(upgrades(&screen), 0);
        screen.primary = Some(i32::from(StatusEffect::HASTE.id));
        assert_eq!(upgrades(&screen), 1);
    }

    #[test]
    fn the_servers_powers_replace_the_choice_when_they_change() {
        let mut screen = BeaconScreen::default();
        let mut menu = menu(4, true);
        screen.follow(&menu);
        screen.primary = Some(i32::from(StatusEffect::HASTE.id));
        screen.follow(&menu);
        assert_eq!(screen.primary, Some(i32::from(StatusEffect::HASTE.id)));
        menu.set_data(PRIMARY, i16::from(StatusEffect::SPEED.id) + 1);
        screen.follow(&menu);
        assert_eq!(screen.primary, Some(i32::from(StatusEffect::SPEED.id)));
    }
}
