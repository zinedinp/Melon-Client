//! `Hud.extractAirBubbles`.

use super::Layout;
use crate::hud::{ICON, JavaRandom, PlayerStatus};
use melon_render::quad::DrawList;
use crate::sprite::GuiSprite;

/// `Entity.getMaxAirSupply` of a player.
pub const MAX_AIR: i32 = 300;

pub struct Sprites {
    full: GuiSprite,
    popping: GuiSprite,
    empty: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut impl FnMut(&str) -> anyhow::Result<GuiSprite>) -> anyhow::Result<Self> {
        Ok(Self {
            full: load("air")?,
            popping: load("air_bursting")?,
            empty: load("air_empty")?,
        })
    }
}

/// `getCurrentAirSupplyBubble`.
pub fn bubbles(air: i32, offset: i32) -> i32 {
    (((air + offset) * 10) as f32 / MAX_AIR as f32).ceil() as i32
}

pub fn draw(
    draw_list: &mut DrawList,
    sprites: &Sprites,
    status: &PlayerStatus,
    ticks: u32,
    layout: &Layout,
    line: f32,
    random: &mut JavaRandom,
) {
    let air = status.air.clamp(0, MAX_AIR);
    let under = status.under_water;
    if !under && air >= MAX_AIR {
        return;
    }
    let full = bubbles(air, -2);
    let popping = bubbles(air, 0);
    let delay = i32::from(air != 0 && under);
    let empty = 10 - bubbles(air, delay);
    let is_popping = full != popping;
    for bubble in 1..=10 {
        let x = layout.right() - (bubble - 1) as f32 * 8.0 - 9.0;
        if bubble <= full {
            sprites.full.draw(draw_list, x, line, ICON, ICON);
        } else if is_popping && bubble == popping && under {
            sprites.popping.draw(draw_list, x, line, ICON, ICON);
        } else if bubble > 10 - empty {
            let wobble = if empty == 10 && ticks.is_multiple_of(2) {
                random.next_int(2) as f32
            } else {
                0.0
            };
            sprites.empty.draw(draw_list, x, line + wobble, ICON, ICON);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bubble_goes_for_every_thirty_ticks_of_air() {
        assert_eq!(bubbles(300, -2), 10);
        assert_eq!(bubbles(270, -2), 9);
        assert_eq!(bubbles(0, -2), 0);
        assert_eq!(bubbles(269, 0), 9);
        assert_eq!(bubbles(269, -2), 9);
        assert_eq!(bubbles(271, -2), 9);
    }
}
