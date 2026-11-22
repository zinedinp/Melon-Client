//! The status effects on screen.

use std::collections::HashMap;

use crate::assets::Assets;
use crate::gui::sprite::GuiSprite;
use crate::gui::text::StyledText;
use crate::i18n::Lang;
use crate::math::argb;
use crate::renderer::Renderer;
use crate::renderer::quad::DrawList;
use crate::world::player::status_effects::{ActiveEffect, ActiveEffects, format_duration};

use super::item_slot::ItemPainter;

/// `EffectsInInventory.SPRITE_SQUARE_SIZE`: the narrowest an entry is.
const SQUARE: f32 = 32.0;
/// `EffectsInInventory.SPACING`.
const SPACING: f32 = 7.0;
/// `EffectsInInventory.ICON_SIZE`.
const ICON: f32 = 18.0;
/// The HUD's effect tile.
const HUD_TILE: f32 = 24.0;

/// `mob_effect/*` icons and the two backgrounds, HUD and inventory.
pub struct EffectTextures {
    icons: HashMap<&'static str, GuiSprite>,
    hud: GuiSprite,
    hud_ambient: GuiSprite,
    inventory: GuiSprite,
    inventory_ambient: GuiSprite,
}

impl EffectTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut icons = HashMap::new();
        for id in 0..=u16::from(u8::MAX) {
            let Some(effect) = <pumpkin_data::effect::StatusEffect as pumpkin_data::data_component_impl::IDSetContent>::from_id(id) else {
                continue;
            };
            let name = effect.minecraft_name.trim_start_matches("minecraft:");
            let path = format!("minecraft:mob_effect/{name}.png");
            if let Ok(sprite) = GuiSprite::load(renderer, assets, &path) {
                icons.insert(name, sprite);
            }
        }
        let mut load = |path: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/{path}.png"),
            )
        };
        Ok(Self {
            icons,
            hud: load("hud/effect_background")?,
            hud_ambient: load("hud/effect_background_ambient")?,
            inventory: load("container/inventory/effect_background")?,
            inventory_ambient: load("container/inventory/effect_background_ambient")?,
        })
    }

    fn icon(&self, effect: &ActiveEffect) -> Option<GuiSprite> {
        effect
            .sprite_name()
            .and_then(|name| self.icons.get(name).copied())
    }
}

/// `Hud.extractEffects`.
pub fn draw_hud(
    draw_list: &mut DrawList,
    textures: &EffectTextures,
    effects: &ActiveEffects,
    width: f32,
) {
    for (effect, x, y) in hud_tiles(effects, width) {
        let mut alpha = 1.0;
        if effect.ambient {
            textures
                .hud_ambient
                .draw(draw_list, x, y, HUD_TILE, HUD_TILE);
        } else {
            textures.hud.draw(draw_list, x, y, HUD_TILE, HUD_TILE);
            if effect.ends_within(200) {
                alpha = blink(effect.duration);
            }
        }
        if let Some(icon) = textures.icon(&effect) {
            icon.draw_tinted(
                draw_list,
                x + 3.0,
                y + 3.0,
                ICON,
                ICON,
                [1.0, 1.0, 1.0, alpha],
            );
        }
    }
}

/// The tiles of the HUD, each effect with its top-left corner.
fn hud_tiles(effects: &ActiveEffects, width: f32) -> Vec<(ActiveEffect, f32, f32)> {
    let (mut beneficial, mut harmful) = (0.0_f32, 0.0_f32);
    let mut tiles = Vec::new();
    for effect in effects.sorted().into_iter().rev() {
        if !effect.show_icon {
            continue;
        }
        let (x, y) = if effect.beneficial() {
            beneficial += 1.0;
            (width - 25.0 * beneficial, 1.0)
        } else {
            harmful += 1.0;
            (width - 25.0 * harmful, 1.0 + 26.0)
        };
        tiles.push((effect, x, y));
    }
    tiles
}

/// The icon's alpha in its last ten seconds.
fn blink(duration: i32) -> f32 {
    let remaining = duration as f32;
    let used_seconds = 10 - duration / 20;
    let alpha = (remaining / 10.0 / 5.0 * 0.5).clamp(0.0, 0.5)
        + crate::math::angle::cos(remaining * crate::math::angle::PI / 5.0)
            * (used_seconds as f32 / 10.0 * 0.25).clamp(0.0, 0.25);
    alpha.clamp(0.0, 1.0)
}

/// Where the inventory's list goes, from the panel it hangs beside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectsPanel {
    /// The panel's left, its top and its width (`leftPos`, `topPos`, `imageWidth`).
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub mouse: (f32, f32),
}

impl EffectsPanel {
    /// `EffectsInInventory.canSeeEffects`.
    pub fn fits(&self, screen_width: f32) -> bool {
        screen_width - (self.left + self.width + 2.0) >= SQUARE
    }
}

/// `EffectsInInventory.extractRenderState`.
pub fn draw_inventory(
    draw_list: &mut DrawList,
    textures: &EffectTextures,
    painter: ItemPainter,
    lang: &Lang,
    effects: &ActiveEffects,
    panel: &EffectsPanel,
    screen_width: f32,
) -> Option<Vec<StyledText>> {
    let x0 = panel.left + panel.width + 2.0;
    let available = screen_width - x0;
    if effects.is_empty() || available < SQUARE {
        return None;
    }
    let max_width = if available >= 120.0 {
        available - SPACING
    } else {
        SQUARE
    };
    let mut step = 33.0;
    if effects.len() > 5 {
        step = (132 / (effects.len() as i32 - 1)) as f32;
    }
    let font = painter.font;
    let (mx, my) = panel.mouse;
    let mut tooltip = None;
    let mut y0 = panel.top;
    for effect in effects.sorted() {
        let name = effect_name(lang, &effect);
        let duration = effect_duration(lang, &effect);
        let name_width = SQUARE + font.text_width(&name) + SPACING;
        let duration_width = SQUARE + font.text_width(&duration) + SPACING;
        let width = max_width.min(name_width.max(duration_width));
        let background = if effect.ambient {
            textures.inventory_ambient
        } else {
            textures.inventory
        };
        background.draw(draw_list, x0, y0, width, SQUARE);

        let (text_x, text_y) = (x0 + SQUARE, y0 + SPACING);
        let room = width - SQUARE - SPACING;
        let mut compact = true;
        if room > 0.0 {
            let clipped = font.text_width(&name) > room;
            let shown = if clipped {
                clip(font, &name, room)
            } else {
                name.clone()
            };
            font.push_text(
                draw_list,
                painter.font_textures,
                &shown,
                text_x,
                text_y,
                argb(0xFFFF_FFFF),
            );
            font.push_text(
                draw_list,
                painter.font_textures,
                &duration,
                text_x,
                text_y + 9.0,
                argb(0xFF80_8080),
            );
            compact = clipped;
        }
        if compact && mx >= x0 && mx <= x0 + width && my >= y0 && my <= y0 + step {
            tooltip = Some(vec![StyledText::plain(&name), StyledText::plain(&duration)]);
        }
        if let Some(icon) = textures.icon(&effect) {
            icon.draw(draw_list, x0 + SPACING, y0 + SPACING, ICON, ICON);
        }
        y0 += step;
    }
    tooltip
}

/// `EffectsInInventory.getEffectName`.
pub fn effect_name(lang: &Lang, effect: &ActiveEffect) -> String {
    let mut name = effect.translation_key().map_or_else(
        || format!("effect.{}", effect.id),
        |key| lang.get(key).to_owned(),
    );
    const LEVELS: [&str; 10] = [
        "enchantment.level.1",
        "enchantment.level.2",
        "enchantment.level.3",
        "enchantment.level.4",
        "enchantment.level.5",
        "enchantment.level.6",
        "enchantment.level.7",
        "enchantment.level.8",
        "enchantment.level.9",
        "enchantment.level.10",
    ];
    if (1..=9).contains(&effect.amplifier) {
        name.push(' ');
        name.push_str(lang.get(LEVELS[effect.amplifier as usize]));
    }
    name
}

/// `MobEffectUtil.formatDuration`.
pub fn effect_duration(lang: &Lang, effect: &ActiveEffect) -> String {
    if effect.infinite() {
        lang.get("effect.duration.infinite").to_owned()
    } else {
        format_duration(effect.duration)
    }
}

/// `ComponentRenderUtils.clipText`: as much of `text` as fits in `width`
fn clip(font: &crate::gui::text::font::Font, text: &str, width: f32) -> String {
    let ellipsis = "...";
    let room = width - font.text_width(ellipsis);
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if font.text_width(&out) > room {
            out.pop();
            break;
        }
    }
    out.push_str(ellipsis);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_needs_thirty_two_pixels_right_of_the_panel() {
        let panel = EffectsPanel {
            left: 100.0,
            top: 50.0,
            width: 176.0,
            mouse: (0.0, 0.0),
        };
        assert!(panel.fits(310.0));
        assert!(!panel.fits(309.0));
    }

    #[test]
    fn several_effects_fill_two_rows_without_overlapping() {
        use crate::world::player::status_effects::*;
        let mut effects = ActiveEffects::default();
        effects.set(SPEED_ID, 0, 600, 6);
        effects.set(NIGHT_VISION_ID, 0, 900, 6);
        effects.set(SLOWNESS_ID, 0, 300, 6);
        effects.set(BLINDNESS_ID, 0, 200, 6);
        effects.set(2, 0, 100, 2);
        let tiles = hud_tiles(&effects, 400.0);
        assert_eq!(tiles.len(), 4);
        let mut seen = std::collections::HashSet::new();
        for (effect, x, y) in &tiles {
            assert!(
                seen.insert((x.to_bits(), y.to_bits())),
                "overlap at {x},{y}"
            );
            assert_eq!(*y, if effect.beneficial() { 1.0 } else { 27.0 });
        }
        assert!(tiles.iter().any(|(_, x, _)| *x == 375.0));
    }

    #[test]
    fn the_blink_stays_between_nothing_and_everything() {
        for duration in 0..=200 {
            let alpha = blink(duration);
            assert!((0.0..=1.0).contains(&alpha), "{duration}: {alpha}");
        }
        assert!((blink(200) - 0.5).abs() < 0.01, "{}", blink(200));
    }
}
