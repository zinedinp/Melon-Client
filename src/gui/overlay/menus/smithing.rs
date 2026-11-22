//! `SmithingScreen`.

use std::time::Instant;

use super::{Loader, MenuDraw, MenuScreen, MenuSprites};
use crate::gui::sprite::GuiSprite;
use crate::world::inventory::OpenContainer;

/// `CyclingSlotBackground.ICON_CHANGE_TICK_RATE`, in milliseconds.
const ICON_MILLIS: u128 = 30 * 50;
/// The panel's own (slot, x, y) of the three inputs.
const INPUTS: [(usize, f32, f32); 3] = [(0, 8.0, 48.0), (1, 26.0, 48.0), (2, 44.0, 48.0)];
const ERROR: (f32, f32, f32, f32) = (65.0, 46.0, 28.0, 21.0);

#[derive(Clone, Copy)]
pub struct Sprites {
    error: GuiSprite,
    /// The outlines the slots cycle through, by name.
    armor_trim: GuiSprite,
    netherite_upgrade: GuiSprite,
    helmet: GuiSprite,
    chestplate: GuiSprite,
    leggings: GuiSprite,
    boots: GuiSprite,
    sword: GuiSprite,
    pickaxe: GuiSprite,
    axe: GuiSprite,
    hoe: GuiSprite,
    shovel: GuiSprite,
    ingot: GuiSprite,
    redstone: GuiSprite,
    lapis: GuiSprite,
    quartz: GuiSprite,
    diamond: GuiSprite,
    emerald: GuiSprite,
    amethyst: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        let mut slot = |name: &str| load.sprite(&format!("slot/{name}"));
        Ok(Self {
            armor_trim: slot("smithing_template_armor_trim")?,
            netherite_upgrade: slot("smithing_template_netherite_upgrade")?,
            helmet: slot("helmet")?,
            chestplate: slot("chestplate")?,
            leggings: slot("leggings")?,
            boots: slot("boots")?,
            sword: slot("sword")?,
            pickaxe: slot("pickaxe")?,
            axe: slot("axe")?,
            hoe: slot("hoe")?,
            shovel: slot("shovel")?,
            ingot: slot("ingot")?,
            redstone: slot("redstone_dust")?,
            lapis: slot("lapis_lazuli")?,
            quartz: slot("quartz")?,
            diamond: slot("diamond")?,
            emerald: slot("emerald")?,
            amethyst: slot("amethyst_shard")?,
            error: load.sprite("smithing/error")?,
        })
    }
}

/// The two templates vanilla ships, which decide what the other slots take.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Template {
    ArmorTrim,
    NetheriteUpgrade,
}

impl Template {
    fn of(registry_key: &str) -> Option<Self> {
        if registry_key.ends_with("_armor_trim_smithing_template") {
            Some(Self::ArmorTrim)
        } else if registry_key == "netherite_upgrade_smithing_template" {
            Some(Self::NetheriteUpgrade)
        } else {
            None
        }
    }

    /// `getBaseSlotEmptyIcons`.
    fn base_icons(self, s: &Sprites) -> Vec<GuiSprite> {
        match self {
            Self::ArmorTrim => vec![s.helmet, s.chestplate, s.leggings, s.boots],
            Self::NetheriteUpgrade => vec![
                s.helmet,
                s.sword,
                s.chestplate,
                s.pickaxe,
                s.leggings,
                s.axe,
                s.boots,
                s.hoe,
                s.shovel,
            ],
        }
    }

    /// `getAdditionalSlotEmptyIcons`.
    fn addition_icons(self, s: &Sprites) -> Vec<GuiSprite> {
        match self {
            Self::ArmorTrim => vec![
                s.ingot, s.redstone, s.lapis, s.quartz, s.diamond, s.emerald, s.amethyst,
            ],
            Self::NetheriteUpgrade => vec![s.ingot],
        }
    }

    fn lang_key(self) -> &'static str {
        match self {
            Self::ArmorTrim => "armor_trim",
            Self::NetheriteUpgrade => "netherite_upgrade",
        }
    }
}

pub struct SmithingScreen;

/// The template in the first slot, if it is one of the known.
fn template(menu: &OpenContainer) -> Option<Template> {
    let stack = &menu.slots[0];
    (!stack.is_empty())
        .then(|| Template::of(stack.item.registry_key))
        .flatten()
}

/// The icon number `icons` shows now.
fn cycle<T: Copy>(icons: &[T]) -> Option<T> {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    if icons.is_empty() {
        return None;
    }
    let step = START.get_or_init(Instant::now).elapsed().as_millis() / ICON_MILLIS;
    Some(icons[step as usize % icons.len()])
}

impl MenuScreen for SmithingScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        if draw.menu.data(0) > 0 {
            let error = draw.sprites.smithing.error;
            draw.sprite(&error, ERROR.0, ERROR.1, ERROR.2, ERROR.3);
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        let menu = draw.menu;
        if menu.data(0) > 0 && draw.frame.hovers(ERROR.0, ERROR.1, ERROR.2, ERROR.3) {
            let text = draw
                .ui
                .lang
                .get("container.upgrade.error_tooltip")
                .to_owned();
            draw.tooltip(&[text]);
            return;
        }
        let hovered = INPUTS
            .iter()
            .find(|&&(_, x, y)| draw.frame.hovers(x - 1.0, y - 1.0, 18.0, 18.0));
        let Some(&(slot, ..)) = hovered else {
            return;
        };
        let text = if menu.slots[0].is_empty() {
            (slot == 0).then(|| {
                draw.ui
                    .lang
                    .get("container.upgrade.missing_template_tooltip")
                    .to_owned()
            })
        } else if menu.slots[slot].is_empty() && slot > 0 {
            template(menu).map(|kind| {
                let part = if slot == 1 {
                    "base_slot"
                } else {
                    "additions_slot"
                };
                let key = format!(
                    "item.minecraft.smithing_template.{}.{part}_description",
                    kind.lang_key()
                );
                draw.ui.lang.get_dynamic(&key).to_owned()
            })
        } else {
            None
        };
        if let Some(text) = text {
            draw.tooltip(&[text]);
        }
    }

    fn empty_icon(
        &self,
        slot: usize,
        menu: &OpenContainer,
        sprites: &MenuSprites,
    ) -> Option<GuiSprite> {
        let s = &sprites.smithing;
        match slot {
            0 => cycle(&[s.armor_trim, s.netherite_upgrade]),
            1 => cycle(&template(menu)?.base_icons(s)),
            2 => cycle(&template(menu)?.addition_icons(s)),
            _ => None,
        }
    }
}
