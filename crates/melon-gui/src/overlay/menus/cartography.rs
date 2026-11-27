//! `CartographyTableScreen`: the frame of the map being made.

use pumpkin_data::item::Item;

use super::{Loader, MenuDraw, MenuScreen};
use crate::gui::sprite::GuiSprite;

#[derive(Clone, Copy)]
pub struct Sprites {
    map: GuiSprite,
    scaled: GuiSprite,
    duplicated: GuiSprite,
    locked: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            map: load.sprite("cartography_table/map")?,
            scaled: load.sprite("cartography_table/scaled_map")?,
            duplicated: load.sprite("cartography_table/duplicated_map")?,
            locked: load.sprite("cartography_table/locked")?,
        })
    }
}

pub struct CartographyScreen;

impl MenuScreen for CartographyScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let sprites = draw.sprites.cartography;
        let additional = draw.menu.slots[1].item;
        let duplicating = additional.id == Item::MAP.id;
        let scaling = additional.id == Item::PAPER.id;
        let locking = additional.id == Item::GLASS_PANE.id;
        if scaling {
            draw.sprite(&sprites.scaled, 67.0, 13.0, 66.0, 66.0);
        } else if duplicating {
            draw.sprite(&sprites.duplicated, 67.0 + 16.0, 13.0, 50.0, 66.0);
            draw.sprite(&sprites.duplicated, 67.0, 13.0 + 16.0, 50.0, 66.0);
        } else {
            draw.sprite(&sprites.map, 67.0, 13.0, 66.0, 66.0);
            if locking {
                draw.sprite(&sprites.locked, 118.0, 60.0, 10.0, 14.0);
            }
        }
    }
}
