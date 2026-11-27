//! The status bars above the hotbar (vanilla `Hud.extractPlayerHealth`).

pub mod air;
pub mod food;
pub mod hearts;
pub mod vehicle;

use super::{ICON, JavaRandom, PlayerStatus};
use crate::renderer::quad::DrawList;
use crate::sprite::GuiSprite;

/// Where the bars sit and how tall their rows are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The hotbar's centre, and the screen's height.
    pub center: f32,
    pub height: f32,
    /// `yLineBase`: the top of the hearts' first row.
    pub base: f32,
    /// Rows of hearts (health and absorption), and each row's step.
    pub heart_rows: i32,
    pub row_height: i32,
}

impl Layout {
    pub fn new(status: &PlayerStatus, center: f32, height: f32) -> Self {
        let max = hearts::max_health(status);
        let containers = (max + status.absorption.ceil().max(0.0)) / 2.0;
        let rows = (containers / 10.0).ceil() as i32;
        Self {
            center,
            height,
            base: height - 39.0,
            heart_rows: rows,
            row_height: (10 - (rows - 2)).max(3),
        }
    }

    pub fn left(&self) -> f32 {
        self.center - 91.0
    }

    pub fn right(&self) -> f32 {
        self.center + 91.0
    }
}

/// All the bars' sprites.
pub struct BarSprites {
    pub hearts: hearts::Sprites,
    pub food: food::Sprites,
    pub air: air::Sprites,
    pub vehicle: vehicle::Sprites,
    pub armor: [GuiSprite; 3],
}

impl BarSprites {
    pub fn load(load: &mut impl FnMut(&str) -> anyhow::Result<GuiSprite>) -> anyhow::Result<Self> {
        Ok(Self {
            hearts: hearts::Sprites::load(load)?,
            food: food::Sprites::load(load)?,
            air: air::Sprites::load(load)?,
            vehicle: vehicle::Sprites::load(load)?,
            armor: [
                load("armor_empty")?,
                load("armor_half")?,
                load("armor_full")?,
            ],
        })
    }
}

/// What the bars remember between frames.
#[derive(Default)]
pub struct BarState {
    pub hearts: hearts::Blink,
}

impl BarState {
    /// Once a tick.
    pub fn tick(&mut self, status: &PlayerStatus, ticks: u32) {
        self.hearts
            .tick(status.health.ceil().max(0.0) as i32, ticks);
    }
}

/// `Hud.extractPlayerHealth` and `extractVehicleHealth`.
pub fn draw(
    draw_list: &mut DrawList,
    sprites: &BarSprites,
    state: &BarState,
    status: &PlayerStatus,
    ticks: u32,
    center: f32,
    height: f32,
) {
    let mut random = JavaRandom::new(i64::from(ticks.wrapping_mul(312_871) as i32));
    let layout = Layout::new(status, center, height);
    armor(draw_list, sprites, status, &layout);
    hearts::draw(
        draw_list,
        &sprites.hearts,
        &state.hearts,
        status,
        ticks,
        &layout,
        &mut random,
    );
    let mount = vehicle::hearts(status.mount);
    let mut air_line = layout.base - 10.0;
    if mount == 0 {
        food::draw(
            draw_list,
            &sprites.food,
            status,
            ticks,
            &layout,
            &mut random,
        );
        air_line -= 10.0;
    } else {
        vehicle::draw(draw_list, &sprites.vehicle, status, &layout);
    }
    air_line -= (vehicle::rows(mount) - 1) as f32 * 10.0;
    air::draw(
        draw_list,
        &sprites.air,
        status,
        ticks,
        &layout,
        air_line,
        &mut random,
    );
}

/// `Hud.extractArmor`: one row over the top row of hearts.
fn armor(draw_list: &mut DrawList, sprites: &BarSprites, status: &PlayerStatus, layout: &Layout) {
    if status.armor <= 0 {
        return;
    }
    let y = layout.base - ((layout.heart_rows - 1) * layout.row_height) as f32 - 10.0;
    for i in 0..10 {
        let halves = i * 2 + 1;
        let sprite = match halves.cmp(&status.armor) {
            std::cmp::Ordering::Less => sprites.armor[2],
            std::cmp::Ordering::Equal => sprites.armor[1],
            std::cmp::Ordering::Greater => sprites.armor[0],
        };
        sprite.draw(draw_list, layout.left() + i as f32 * 8.0, y, ICON, ICON);
    }
}

/// Which of an icon's three states shows for icon `index` of a bar of `value` halves.
pub fn fill(index: i32, value: i32) -> usize {
    let halves = index * 2 + 1;
    match halves.cmp(&value) {
        std::cmp::Ordering::Less => 2,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Greater => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_follows_the_servers_maximum_health() {
        let mut status = PlayerStatus::default();
        let normal = Layout::new(&status, 100.0, 200.0);
        assert_eq!((normal.heart_rows, normal.row_height), (1, 11));
        status.max_health = 40.0;
        let big = Layout::new(&status, 100.0, 200.0);
        assert_eq!((big.heart_rows, big.row_height), (2, 10));
        status.max_health = 60.0;
        let huge = Layout::new(&status, 100.0, 200.0);
        assert_eq!((huge.heart_rows, huge.row_height), (3, 9));
        status.max_health = 20.0;
        status.absorption = 20.0;
        assert_eq!(Layout::new(&status, 100.0, 200.0).heart_rows, 2);
        status.absorption = 22.0;
        assert_eq!(Layout::new(&status, 100.0, 200.0).heart_rows, 3);
    }

    #[test]
    fn an_icon_is_full_half_or_empty() {
        assert_eq!(fill(0, 1), 1);
        assert_eq!(fill(0, 2), 2);
        assert_eq!(fill(1, 2), 0);
        assert_eq!(fill(9, 20), 2);
    }
}
