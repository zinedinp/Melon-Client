//! `Hud.extractFood`.

use super::{Layout, fill};
use crate::hud::{ICON, JavaRandom, PlayerStatus};
use crate::sprite::GuiSprite;
use melon_render::quad::DrawList;

/// Empty, half and full, plain and with hunger.
pub struct Sprites {
    plain: [GuiSprite; 3],
    hunger: [GuiSprite; 3],
}

impl Sprites {
    pub fn load(load: &mut impl FnMut(&str) -> anyhow::Result<GuiSprite>) -> anyhow::Result<Self> {
        Ok(Self {
            plain: [load("food_empty")?, load("food_half")?, load("food_full")?],
            hunger: [
                load("food_empty_hunger")?,
                load("food_half_hunger")?,
                load("food_full_hunger")?,
            ],
        })
    }
}

/// Whether the icons shake this tick, which is not every tick.
pub fn shakes(status: &PlayerStatus, ticks: u32) -> bool {
    status.saturation <= 0.0 && ticks.is_multiple_of(status.food.clamp(0, 20) as u32 * 3 + 1)
}

pub fn draw(
    draw_list: &mut DrawList,
    sprites: &Sprites,
    status: &PlayerStatus,
    ticks: u32,
    layout: &Layout,
    random: &mut JavaRandom,
) {
    let food = status.food.clamp(0, 20);
    let set = if status.hunger {
        &sprites.hunger
    } else {
        &sprites.plain
    };
    let shaking = shakes(status, ticks);
    for i in 0..10 {
        let mut y = layout.base;
        if shaking {
            y += (random.next_int(3) - 1) as f32;
        }
        let x = layout.right() - i as f32 * 8.0 - 9.0;
        set[0].draw(draw_list, x, y, ICON, ICON);
        let state = fill(i, food);
        if state > 0 {
            set[state].draw(draw_list, x, y, ICON, ICON);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_stomach_without_saturation_shakes_and_on_its_own_beat() {
        let mut status = PlayerStatus::default();
        assert!(!shakes(&status, 0));
        status.saturation = 0.0;
        status.food = 3;
        assert!(shakes(&status, 0) && shakes(&status, 10) && !shakes(&status, 5));
        status.food = 0;
        assert!((0..5).all(|t| shakes(&status, t)));
    }
}
