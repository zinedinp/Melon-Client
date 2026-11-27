//! `Hud.extractHearts`.

use super::Layout;
use crate::hud::{ICON, JavaRandom, PlayerStatus};
use melon_render::quad::DrawList;
use crate::sprite::GuiSprite;

/// `Hud.HeartType`: how the hearts look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeartKind {
    #[default]
    Normal,
    Poisoned,
    Withered,
    Absorbing,
    Frozen,
}

impl HeartKind {
    const ALL: [Self; 5] = [
        Self::Normal,
        Self::Poisoned,
        Self::Withered,
        Self::Absorbing,
        Self::Frozen,
    ];

    fn prefix(self) -> &'static str {
        match self {
            Self::Normal => "",
            Self::Poisoned => "poisoned_",
            Self::Withered => "withered_",
            Self::Absorbing => "absorbing_",
            Self::Frozen => "frozen_",
        }
    }

    /// `HeartType.forPlayer`: wither beats poison beats frost.
    pub fn of(wither: bool, poison: bool, frozen: bool) -> Self {
        if wither {
            Self::Withered
        } else if poison {
            Self::Poisoned
        } else if frozen {
            Self::Frozen
        } else {
            Self::Normal
        }
    }
}

/// Every heart sprite: by kind, hardcore, half, blinking.
pub struct Sprites {
    hearts: [[[[GuiSprite; 2]; 2]; 2]; 5],
    /// The container: hardcore, blinking.
    containers: [[GuiSprite; 2]; 2],
}

impl Sprites {
    pub fn load(load: &mut impl FnMut(&str) -> anyhow::Result<GuiSprite>) -> anyhow::Result<Self> {
        let mut one = |kind: HeartKind, hardcore: bool, half: bool, blink: bool| {
            load(&format!(
                "heart/{}{}{}{}",
                kind.prefix(),
                if hardcore { "hardcore_" } else { "" },
                if half { "half" } else { "full" },
                if blink { "_blinking" } else { "" },
            ))
        };
        let mut hearts = Vec::new();
        for kind in HeartKind::ALL {
            let mut by_hardcore = Vec::new();
            for hardcore in [false, true] {
                let mut by_half = Vec::new();
                for half in [false, true] {
                    by_half.push([
                        one(kind, hardcore, half, false)?,
                        one(kind, hardcore, half, true)?,
                    ]);
                }
                by_hardcore.push(<[_; 2]>::try_from(by_half).ok().unwrap());
            }
            hearts.push(<[_; 2]>::try_from(by_hardcore).ok().unwrap());
        }
        let mut container = |hardcore: bool, blink: bool| {
            load(&format!(
                "heart/container{}{}",
                if hardcore { "_hardcore" } else { "" },
                if blink { "_blinking" } else { "" }
            ))
        };
        Ok(Self {
            hearts: <[_; 5]>::try_from(hearts).ok().unwrap(),
            containers: [
                [container(false, false)?, container(false, true)?],
                [container(true, false)?, container(true, true)?],
            ],
        })
    }

    pub fn heart(&self, kind: HeartKind, hardcore: bool, half: bool, blink: bool) -> GuiSprite {
        let kind = HeartKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        self.hearts[kind][usize::from(hardcore)][usize::from(half)][usize::from(blink)]
    }

    /// The empty heart behind one.
    pub fn container(&self, hardcore: bool, blink: bool) -> GuiSprite {
        self.containers[usize::from(hardcore)][usize::from(blink)]
    }
}

/// `max(MAX_HEALTH, displayed, current)`.
pub fn max_health(status: &PlayerStatus) -> f32 {
    status.max_health.max(status.health.ceil())
}

/// The blink after a change in health (`Hud.healthBlinkTime`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Blink {
    last: i32,
    /// The health the flash is drawn over.
    pub display: i32,
    until: u32,
    /// The tick the display was taken, for its one-second life.
    taken: u32,
    /// The first number is not a change.
    seen: bool,
}

impl Blink {
    pub fn tick(&mut self, current: i32, ticks: u32) {
        if !self.seen {
            *self = Self {
                last: current,
                display: current,
                taken: ticks,
                seen: true,
                until: 0,
            };
            return;
        }
        if current < self.last {
            self.taken = ticks;
            self.until = ticks + 20;
        } else if current > self.last {
            self.taken = ticks;
            self.until = ticks + 10;
        }
        if ticks.wrapping_sub(self.taken) > 20 {
            self.display = current;
            self.taken = ticks;
        }
        self.last = current;
    }

    /// `(blinkTime - tickCount) / 3 % 2 == 1`.
    pub fn blinking(&self, ticks: u32) -> bool {
        self.until > ticks && (self.until - ticks) / 3 % 2 == 1
    }
}

pub fn draw(
    draw_list: &mut DrawList,
    sprites: &Sprites,
    blink: &Blink,
    status: &PlayerStatus,
    ticks: u32,
    layout: &Layout,
    random: &mut JavaRandom,
) {
    let kind = status.heart_kind;
    let hardcore = status.hardcore;
    let current = status.health.ceil().max(0.0) as i32;
    let old = blink.display;
    let max = max_health(status).max(old as f32);
    let absorption = status.absorption.ceil().max(0.0) as i32;
    let containers = (max / 2.0).ceil() as i32;
    let absorbing = (absorption as f32 / 2.0).ceil() as i32;
    let blinking = blink.blinking(ticks);
    let wave = status
        .regeneration
        .then(|| (ticks % (max + 5.0).ceil() as u32) as i32);
    for index in (0..containers + absorbing).rev() {
        let x = layout.left() + (index % 10) as f32 * 8.0;
        let mut y = layout.base - (index / 10 * layout.row_height) as f32;
        if current + absorption <= 4 {
            y += random.next_int(2) as f32;
        }
        if index < containers && Some(index) == wave {
            y -= 2.0;
        }
        let container = sprites.containers[usize::from(hardcore)][usize::from(blinking)];
        container.draw(draw_list, x, y, ICON, ICON);
        let halves = index * 2;
        if index >= containers {
            let taken = halves - containers * 2;
            if taken < absorption {
                let kind = if kind == HeartKind::Withered {
                    kind
                } else {
                    HeartKind::Absorbing
                };
                sprites
                    .heart(kind, hardcore, taken + 1 == absorption, false)
                    .draw(draw_list, x, y, ICON, ICON);
            }
        }
        if blinking && halves < old {
            sprites
                .heart(kind, hardcore, halves + 1 == old, true)
                .draw(draw_list, x, y, ICON, ICON);
        }
        if halves < current {
            sprites
                .heart(kind, hardcore, halves + 1 == current, false)
                .draw(draw_list, x, y, ICON, ICON);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wither_beats_poison_beats_frost() {
        assert_eq!(HeartKind::of(true, true, true), HeartKind::Withered);
        assert_eq!(HeartKind::of(false, true, true), HeartKind::Poisoned);
        assert_eq!(HeartKind::of(false, false, true), HeartKind::Frozen);
        assert_eq!(HeartKind::of(false, false, false), HeartKind::Normal);
    }

    #[test]
    fn the_servers_maximum_counts_and_more_health_stretches_it() {
        let mut status = PlayerStatus {
            max_health: 30.0,
            ..PlayerStatus::default()
        };
        assert_eq!(max_health(&status), 30.0);
        status.health = 33.5;
        assert_eq!(max_health(&status), 34.0);
    }

    #[test]
    fn a_loss_blinks_twenty_ticks_in_thirds_and_a_gain_ten() {
        let mut blink = Blink::default();
        blink.tick(20, 100);
        blink.tick(20, 101);
        assert!(!blink.blinking(101));
        blink.tick(14, 102);
        assert_eq!(blink.display, 20);
        assert!(!blink.blinking(102));
        assert!(blink.blinking(105));
        assert!(!blink.blinking(130));
        let mut gain = Blink::default();
        gain.tick(10, 0);
        gain.tick(12, 1);
        assert_eq!(gain.until, 11);
        gain.tick(12, 30);
        assert_eq!(gain.display, 12);
    }
}
