//! `EnchantmentScreen`.

use super::{Frame, Galactic, Loader, MenuDraw, MenuScreen, MenuSprites};
use crate::hud::item_slot;
use crate::overlay::container_input::MouseButtonKind;
use crate::overlay::{OverlayAction, OverlayContext};
use crate::sprite::GuiSprite;
use crate::text::{StyledText, colors};
use crate::world::inventory::OpenContainer;
use crate::world::java_random::JavaRandom;

/// `EnchantmentMenu`'s `ContainerData`: costs, seed, clues.
const COSTS: usize = 0;
const SEED: usize = 3;
const ENCHANT_CLUE: usize = 4;
const LEVEL_CLUE: usize = 7;
/// An offer: where the first is, its size and the pitch.
const ROW_X: f32 = 60.0;
const ROW_Y: f32 = 14.0;
const ROW: (f32, f32) = (108.0, 19.0);
const PITCH: f32 = 19.0;

/// `EnchantmentNames.words`.
const WORDS: &[&str] = &[
    "the",
    "elder",
    "scrolls",
    "klaatu",
    "berata",
    "niktu",
    "xyzzy",
    "bless",
    "curse",
    "light",
    "darkness",
    "fire",
    "air",
    "earth",
    "water",
    "hot",
    "dry",
    "cold",
    "wet",
    "ignite",
    "snuff",
    "embiggen",
    "twist",
    "shorten",
    "stretch",
    "fiddle",
    "destroy",
    "imbue",
    "galvanize",
    "enchant",
    "free",
    "limited",
    "range",
    "of",
    "towards",
    "inside",
    "sphere",
    "cube",
    "self",
    "other",
    "ball",
    "mental",
    "physical",
    "grow",
    "shrink",
    "demon",
    "elemental",
    "spirit",
    "animal",
    "creature",
    "beast",
    "humanoid",
    "undead",
    "fresh",
    "stale",
    "phnglui",
    "mglwnafh",
    "cthulhu",
    "rlyeh",
    "wgahnagl",
    "fhtagn",
    "baguette",
];

#[derive(Clone, Copy)]
pub struct Sprites {
    slot: GuiSprite,
    highlighted: GuiSprite,
    disabled: GuiSprite,
    level: [GuiSprite; 3],
    level_disabled: [GuiSprite; 3],
    empty_lapis: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        let mut sprite = |name: &str| load.sprite(&format!("enchanting_table/{name}"));
        Ok(Self {
            slot: sprite("enchantment_slot")?,
            highlighted: sprite("enchantment_slot_highlighted")?,
            disabled: sprite("enchantment_slot_disabled")?,
            level: [sprite("level_1")?, sprite("level_2")?, sprite("level_3")?],
            level_disabled: [
                sprite("level_1_disabled")?,
                sprite("level_2_disabled")?,
                sprite("level_3_disabled")?,
            ],
            empty_lapis: load.sprite("slot/lapis_lazuli")?,
        })
    }
}

#[derive(Default)]
pub struct EnchantmentScreen;

/// `getRandomName`.
fn random_name(random: &mut JavaRandom, galactic: &Galactic, max_width: f32) -> String {
    let count = random.next_int(2) + 3;
    let name = (0..count)
        .map(|_| WORDS[random.next_int(WORDS.len() as i32) as usize])
        .collect::<Vec<_>>()
        .join(" ");
    let mut head = String::new();
    for c in name.chars() {
        head.push(c);
        if galactic.font.text_width(&head) > max_width {
            head.pop();
            break;
        }
    }
    head
}

/// The lapis in the second slot (`getGoldCount`).
fn lapis(menu: &OpenContainer) -> i32 {
    i32::from(menu.slots[1].item_count) * i32::from(!menu.slots[1].is_empty())
}

/// Whether the player can take offer `i`.
fn affordable(menu: &OpenContainer, i: usize, level: i32, free: bool) -> bool {
    let cost = i32::from(menu.data(COSTS + i));
    let wanted = i as i32 + 1;
    free || (lapis(menu) >= wanted && level >= wanted && level >= cost)
}

impl MenuScreen for EnchantmentScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let sprites = draw.sprites.enchantment;
        let menu = draw.menu;
        let mut random = JavaRandom::with_seed(i64::from(menu.data(SEED)));
        for i in 0..3 {
            let y = ROW_Y + PITCH * i as f32;
            let cost = i32::from(menu.data(COSTS + i));
            if cost == 0 {
                draw.sprite(&sprites.disabled, ROW_X, y, ROW.0, ROW.1);
                continue;
            }
            let cost_text = cost.to_string();
            let cost_width = draw.text_width(&cost_text);
            let name_width = 86.0 - cost_width;
            let name = random_name(&mut random, &draw.sprites.galactic, name_width);
            let usable = affordable(
                menu,
                i,
                draw.ui.experience_level,
                draw.ui.infinite_materials,
            ) || draw.ui.infinite_materials;
            let (name_color, cost_color) = if !usable {
                draw.sprite(&sprites.disabled, ROW_X, y, ROW.0, ROW.1);
                draw.sprite(&sprites.level_disabled[i], ROW_X + 1.0, y + 1.0, 16.0, 16.0);
                (0xFF34_2F25, 0xFF40_7F10)
            } else {
                let hovered = draw.frame.hovers(ROW_X, y, ROW.0, ROW.1);
                let (sprite, color) = if hovered {
                    (&sprites.highlighted, 0xFFFF_FF80)
                } else {
                    (&sprites.slot, 0xFF68_5E4A)
                };
                draw.sprite(sprite, ROW_X, y, ROW.0, ROW.1);
                draw.sprite(&sprites.level[i], ROW_X + 1.0, y + 1.0, 16.0, 16.0);
                (color, 0xFF80_FF20)
            };
            let (left, top) = (draw.frame.left, draw.frame.top);
            let galactic = &draw.sprites.galactic;
            galactic.font.push_text_flat(
                draw.ui.draw_list,
                &galactic.pages,
                &name,
                left + ROW_X + 20.0,
                top + y + 2.0,
                crate::math::argb(name_color),
            );
            draw.text_shadowed(
                &cost_text,
                ROW_X + 20.0 + 86.0 - cost_width,
                y + 2.0 + 7.0,
                cost_color,
            );
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        let menu = draw.menu;
        let (lang, free) = (draw.ui.lang, draw.ui.infinite_materials);
        for i in 0..3 {
            let y = ROW_Y + PITCH * i as f32;
            let min_level = i32::from(menu.data(COSTS + i));
            let level = i32::from(menu.data(LEVEL_CLUE + i));
            let enchantment = u8::try_from(menu.data(ENCHANT_CLUE + i))
                .ok()
                .and_then(pumpkin_data::Enchantment::from_id);
            let Some(enchantment) = enchantment else {
                continue;
            };
            if !(draw.frame.hovers(ROW_X, y, ROW.0, 17.0) && min_level > 0 && level >= 0) {
                continue;
            }
            let name = item_slot::enchantment_name(lang, enchantment, level);
            let mut lines = vec![StyledText::colored(
                &lang.format("container.enchant.clue", &[&name]),
                colors::WHITE,
            )];
            if !free {
                lines.push(StyledText::plain(""));
                let cost = i as i32 + 1;
                if draw.ui.experience_level < min_level {
                    lines.push(StyledText::colored(
                        &lang.format(
                            "container.enchant.level.requirement",
                            &[&min_level.to_string()],
                        ),
                        colors::RED,
                    ));
                } else {
                    let lapis_cost = if cost == 1 {
                        lang.get("container.enchant.lapis.one").to_owned()
                    } else {
                        lang.format("container.enchant.lapis.many", &[&cost.to_string()])
                    };
                    let level_cost = if cost == 1 {
                        lang.get("container.enchant.level.one").to_owned()
                    } else {
                        lang.format("container.enchant.level.many", &[&cost.to_string()])
                    };
                    let lapis_color = if lapis(menu) >= cost {
                        colors::GRAY
                    } else {
                        colors::RED
                    };
                    lines.push(StyledText::colored(&lapis_cost, lapis_color));
                    lines.push(StyledText::colored(&level_cost, colors::GRAY));
                }
            }
            draw.tooltip_styled(&lines);
            return;
        }
    }

    fn press(
        &mut self,
        frame: Frame,
        _button: MouseButtonKind,
        ctx: &mut OverlayContext,
    ) -> Option<OverlayAction> {
        let menu = ctx.container.as_deref()?;
        let free = ctx.game_mode.has_infinite_materials();
        let row = (0..3).find(|&i| frame.hovers(ROW_X, ROW_Y + PITCH * i as f32, ROW.0, ROW.1))?;
        let usable = !menu.slots[0].is_empty()
            && menu.data(COSTS + row) > 0
            && affordable(menu, row, ctx.experience_level, free);
        usable.then_some(OverlayAction::MenuButton {
            container: menu.id,
            button: row as i32,
        })
    }

    fn empty_icon(
        &self,
        slot: usize,
        _menu: &OpenContainer,
        sprites: &MenuSprites,
    ) -> Option<GuiSprite> {
        (slot == 1).then_some(sprites.enchantment.empty_lapis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::ContainerKind;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;

    fn menu(costs: [i16; 3], lapis: u8) -> OpenContainer {
        let mut menu = OpenContainer::new(1, ContainerKind::Enchantment, String::new());
        for (i, cost) in costs.into_iter().enumerate() {
            menu.set_data(COSTS + i, cost);
        }
        menu.slots[0] = ItemStack::new(1, &Item::DIAMOND_SWORD).into();
        if lapis > 0 {
            menu.slots[1] = ItemStack::new(lapis, &Item::LAPIS_LAZULI).into();
        }
        menu
    }

    #[test]
    fn an_offer_needs_its_lapis_and_the_levels_it_names() {
        let menu = menu([10, 20, 30], 2);
        assert!(!affordable(&menu, 0, 9, false));
        assert!(affordable(&menu, 0, 10, false));
        assert!(!affordable(&menu, 2, 30, false));
        assert!(affordable(&menu, 2, 0, true));
    }

    #[test]
    fn the_word_list_is_the_vanilla_sixty_two() {
        assert_eq!(WORDS.len(), 62);
    }
}
