//! `Hud.extractVehicleHealth`.

use super::{Layout, fill};
use crate::gui::hud::{ICON, PlayerStatus};
use crate::gui::sprite::GuiSprite;
use crate::renderer::quad::DrawList;

pub struct Sprites {
    sprites: [GuiSprite; 3],
}

impl Sprites {
    pub fn load(load: &mut impl FnMut(&str) -> anyhow::Result<GuiSprite>) -> anyhow::Result<Self> {
        Ok(Self {
            sprites: [
                load("heart/vehicle_container")?,
                load("heart/vehicle_half")?,
                load("heart/vehicle_full")?,
            ],
        })
    }
}

/// `getVehicleMaxHearts`: its maximum health, rounded, halved, up to 30.
pub fn hearts(mount: Option<(f32, f32)>) -> i32 {
    mount.map_or(0, |(_, max)| (((max + 0.5) as i32) / 2).clamp(0, 30))
}

/// `getVisibleVehicleHeartRows`.
pub fn rows(hearts: i32) -> i32 {
    (hearts + 9) / 10
}

pub fn draw(draw_list: &mut DrawList, sprites: &Sprites, status: &PlayerStatus, layout: &Layout) {
    let Some((health, _)) = status.mount else {
        return;
    };
    let current = health.ceil() as i32;
    let mut left = hearts(status.mount);
    let mut y = layout.base;
    let mut base = 0;
    while left > 0 {
        let row = left.min(10);
        left -= row;
        for i in 0..row {
            let x = layout.right() - i as f32 * 8.0 - 9.0;
            sprites.sprites[0].draw(draw_list, x, y, ICON, ICON);
            let state = fill(i + base / 2, current);
            if state > 0 {
                sprites.sprites[state].draw(draw_list, x, y, ICON, ICON);
            }
        }
        y -= 10.0;
        base += 20;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mounts_hearts_come_from_its_maximum_health() {
        assert_eq!(hearts(None), 0);
        assert_eq!(hearts(Some((10.0, 10.0))), 5);
        assert_eq!(hearts(Some((24.0, 30.0))), 15);
        assert_eq!(hearts(Some((1.0, 14.6))), 7);
        assert_eq!(hearts(Some((1.0, 100.0))), 30);
        assert_eq!((rows(5), rows(10), rows(11), rows(30)), (1, 1, 2, 3));
    }
}
