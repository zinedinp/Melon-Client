//! `AbstractMountInventoryScreen`.

use super::{Loader, MenuDraw, MenuScreen};
use crate::sprite::GuiSprite;
use melon_world::inventory::MountLook;

#[derive(Clone, Copy)]
pub struct Sprites {
    slot: GuiSprite,
    chest_slots: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            slot: load.sprite("slot")?,
            chest_slots: load.sprite("horse/chest_slots")?,
        })
    }
}

pub struct MountScreen {
    pub columns: u8,
    pub look: MountLook,
}

impl MenuScreen for MountScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let sprites = draw.sprites.mount;
        if self.columns > 0 {
            draw.sprite_part(
                &sprites.chest_slots,
                [90.0, 54.0],
                [0.0, 0.0],
                (79.0, 17.0),
                (f32::from(self.columns) * 18.0, 54.0),
            );
        }
        if self.look.saddle {
            draw.sprite(&sprites.slot, 7.0, 17.0, 18.0, 18.0);
        }
        if self.look.armor {
            draw.sprite(&sprites.slot, 7.0, 35.0, 18.0, 18.0);
        }
    }
}
