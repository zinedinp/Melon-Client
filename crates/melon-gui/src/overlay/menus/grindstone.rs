//! `GrindstoneScreen`.

use super::{Loader, MenuDraw, MenuScreen};
use crate::sprite::GuiSprite;

#[derive(Clone, Copy)]
pub struct Sprites {
    error: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            error: load.sprite("grindstone/error")?,
        })
    }
}

pub struct GrindstoneScreen;

impl MenuScreen for GrindstoneScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let slots = &draw.menu.slots;
        if (!slots[0].is_empty() || !slots[1].is_empty()) && slots[2].is_empty() {
            let error = draw.sprites.grindstone.error;
            draw.sprite(&error, 92.0, 31.0, 28.0, 21.0);
        }
    }
}
