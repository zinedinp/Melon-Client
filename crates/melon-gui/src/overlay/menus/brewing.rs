//! `BrewingStandScreen`.

use super::{Loader, MenuDraw, MenuScreen, MenuSprites};
use crate::sprite::GuiSprite;
use melon_world::inventory::OpenContainer;

/// `BrewingStandMenu`'s `ContainerData` indices.
const BREW_TICKS: usize = 0;
const FUEL: usize = 1;
const BREW_TOTAL: usize = 2;
const FUEL_TOTAL: usize = 3;
/// What a brew and a full gauge are when the server doesn't say.
const DEFAULT_BREW_TOTAL: i16 = 400;
const DEFAULT_FUEL_TOTAL: i16 = 20;
/// `BUBBLELENGTHS`: how tall the bubbles are as the ticks go by.
const BUBBLES: [i32; 7] = [29, 24, 20, 16, 11, 6, 0];

#[derive(Clone, Copy)]
pub struct Sprites {
    fuel_length: GuiSprite,
    progress: GuiSprite,
    bubbles: GuiSprite,
    empty_fuel: GuiSprite,
    empty_potion: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            fuel_length: load.sprite("brewing_stand/fuel_length")?,
            progress: load.sprite("brewing_stand/brew_progress")?,
            bubbles: load.sprite("brewing_stand/bubbles")?,
            empty_fuel: load.sprite("slot/brewing_fuel")?,
            empty_potion: load.sprite("slot/potion")?,
        })
    }
}

/// How much of the 18 pixel gauge is lit.
fn fuel_length(fuel: i16, total: i16) -> i32 {
    if total > 0 {
        (18 * i32::from(fuel) + i32::from(total) - 1)
            .div_euclid(i32::from(total))
            .clamp(0, 18)
    } else {
        0
    }
}

/// The arrow's length, `(int)(28 * (1 - ticks / total))`.
fn arrow_length(ticks: i16, total: i16) -> i32 {
    (28.0 * (1.0 - f32::from(ticks) / f32::from(total))) as i32
}

pub struct BrewingScreen;

impl MenuScreen for BrewingScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let sprites = draw.sprites.brewing;
        let menu = draw.menu;
        let or = |value: i16, default| if value == 0 { default } else { value };
        let length = fuel_length(
            menu.data(FUEL),
            or(menu.data(FUEL_TOTAL), DEFAULT_FUEL_TOTAL),
        );
        if length > 0 {
            draw.sprite_part(
                &sprites.fuel_length,
                [18.0, 4.0],
                [0.0, 0.0],
                (60.0, 44.0),
                (length as f32, 4.0),
            );
        }
        let ticks = menu.data(BREW_TICKS);
        let total = or(menu.data(BREW_TOTAL), DEFAULT_BREW_TOTAL);
        if ticks > 0 && total > 0 {
            let length = arrow_length(ticks, total);
            if length > 0 {
                draw.sprite_part(
                    &sprites.progress,
                    [9.0, 28.0],
                    [0.0, 0.0],
                    (97.0, 16.0),
                    (9.0, length as f32),
                );
            }
            let length = BUBBLES[(ticks / 2 % 7) as usize];
            if length > 0 {
                draw.sprite_part(
                    &sprites.bubbles,
                    [12.0, 29.0],
                    [0.0, (29 - length) as f32],
                    (63.0, 14.0 + (29 - length) as f32),
                    (12.0, length as f32),
                );
            }
        }
    }

    fn empty_icon(
        &self,
        slot: usize,
        _menu: &OpenContainer,
        sprites: &MenuSprites,
    ) -> Option<GuiSprite> {
        match slot {
            0..=2 => Some(sprites.brewing.empty_potion),
            4 => Some(sprites.brewing.empty_fuel),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fuel_gauge_rounds_up_to_whole_pixels() {
        assert_eq!(fuel_length(0, 20), 0);
        assert_eq!(fuel_length(1, 20), 1);
        assert_eq!(fuel_length(20, 20), 18);
        assert_eq!(fuel_length(10, 20), 9);
        assert_eq!(fuel_length(5, 0), 0);
    }

    #[test]
    fn the_arrow_fills_as_the_brew_runs_down() {
        assert_eq!(arrow_length(400, 400), 0);
        assert_eq!(arrow_length(200, 400), 14);
        assert_eq!(arrow_length(1, 400), 27);
    }
}
