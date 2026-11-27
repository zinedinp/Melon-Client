//! `AbstractFurnaceScreen`.

use super::{Loader, MenuDraw, MenuScreen};
use crate::sprite::GuiSprite;
use crate::world::inventory::{ContainerKind, OpenContainer};

/// `AbstractFurnaceMenu`'s `ContainerData` indices.
const LIT_TIME: usize = 0;
const LIT_DURATION: usize = 1;
const COOKING_PROGRESS: usize = 2;
const COOKING_TOTAL: usize = 3;

/// The flame and arrow of one of the three.
#[derive(Clone, Copy)]
pub struct Sprites {
    lit: [GuiSprite; 3],
    burn: [GuiSprite; 3],
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        let mut pair = |dir: &str| -> anyhow::Result<_> {
            Ok((
                load.sprite(&format!("{dir}/lit_progress"))?,
                load.sprite(&format!("{dir}/burn_progress"))?,
            ))
        };
        let (furnace, blast, smoker) = (pair("furnace")?, pair("blast_furnace")?, pair("smoker")?);
        Ok(Self {
            lit: [furnace.0, blast.0, smoker.0],
            burn: [furnace.1, blast.1, smoker.1],
        })
    }
}

pub struct FurnaceScreen {
    /// Which of the three sprite sets.
    variant: usize,
}

impl FurnaceScreen {
    pub fn new(kind: ContainerKind) -> Self {
        Self {
            variant: match kind {
                ContainerKind::BlastFurnace => 1,
                ContainerKind::Smoker => 2,
                _ => 0,
            },
        }
    }
}

/// `AbstractFurnaceMenu.isLit`.
fn is_lit(menu: &OpenContainer) -> bool {
    menu.data(LIT_TIME) > 0
}

/// `getLitProgress`.
fn lit_progress(menu: &OpenContainer) -> f32 {
    let duration = match menu.data(LIT_DURATION) {
        0 => 200,
        duration => duration,
    };
    (f32::from(menu.data(LIT_TIME)) / f32::from(duration)).clamp(0.0, 1.0)
}

/// `getBurnProgress`: how far the item has cooked, 0 to 1.
fn burn_progress(menu: &OpenContainer) -> f32 {
    let (progress, total) = (menu.data(COOKING_PROGRESS), menu.data(COOKING_TOTAL));
    if total == 0 || progress == 0 {
        0.0
    } else {
        (f32::from(progress) / f32::from(total)).clamp(0.0, 1.0)
    }
}

impl MenuScreen for FurnaceScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let sprites = draw.sprites.furnace;
        let menu = draw.menu;
        if is_lit(menu) {
            let height = (lit_progress(menu) * 13.0).ceil() + 1.0;
            draw.sprite_part(
                &sprites.lit[self.variant],
                [14.0, 14.0],
                [0.0, 14.0 - height],
                (56.0, 36.0 + 14.0 - height),
                (14.0, height),
            );
        }
        let width = (burn_progress(menu) * 24.0).ceil();
        draw.sprite_part(
            &sprites.burn[self.variant],
            [24.0, 16.0],
            [0.0, 0.0],
            (79.0, 34.0),
            (width, 16.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(data: [i16; 4]) -> OpenContainer {
        let mut menu = OpenContainer::new(1, ContainerKind::Furnace, String::new());
        for (id, value) in data.into_iter().enumerate() {
            menu.set_data(id, value);
        }
        menu
    }

    #[test]
    fn a_furnace_is_lit_while_it_has_fuel_burning() {
        assert!(!is_lit(&menu([0, 1600, 0, 0])));
        assert!(is_lit(&menu([1, 1600, 0, 0])));
    }

    #[test]
    fn the_flame_and_the_arrow_fill_with_their_ratios() {
        assert_eq!(lit_progress(&menu([800, 1600, 0, 0])), 0.5);
        assert_eq!(burn_progress(&menu([0, 0, 100, 200])), 0.5);
        assert_eq!(burn_progress(&menu([0, 0, 0, 200])), 0.0);
        assert_eq!(burn_progress(&menu([0, 0, 5, 0])), 0.0);
    }

    #[test]
    fn an_unknown_burn_time_counts_as_the_default_200() {
        assert_eq!(lit_progress(&menu([100, 0, 0, 0])), 0.5);
    }
}
