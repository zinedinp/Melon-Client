//! What animates on block entities.

use super::{BlockEntities, Kind, kind_of};
use crate::block::direction::Direction;
use crate::level::Level;
use melon_core::math::angle::{PI, TAU};
use glam::Vec3;

impl BlockEntities {
    pub(super) fn random(&mut self, bound: u32) -> i32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as i32 % bound as i32
    }

    /// `BlockEntity.triggerEvent`.
    pub fn event(&mut self, level: &Level, pos: [i32; 3], action: u8, param: u8) {
        let Some(kind) = level.block_state(pos[0], pos[1], pos[2]).and_then(kind_of) else {
            return;
        };
        if action != 1 {
            return;
        }
        self.still_revision += 1;
        match kind {
            Kind::Chest { .. } | Kind::Shulker { .. } => {
                let lid = self.anims.entry(pos).or_insert(Anim::Lid(Lid::default()));
                if let Anim::Lid(lid) = lid {
                    lid.open = param > 0;
                }
            }
            Kind::Bell => {
                let direction = Direction::ALL
                    .get(usize::from(param))
                    .copied()
                    .unwrap_or(Direction::North);
                self.anims.insert(
                    pos,
                    Anim::Bell {
                        ticks: 0,
                        shaking: true,
                        direction,
                    },
                );
            }
            Kind::DecoratedPot { .. } => {
                self.anims.insert(
                    pos,
                    Anim::Pot {
                        started: self.ticks,
                        negative: param == 1,
                    },
                );
            }
            _ => {}
        }
    }

    /// One client tick for every animation.
    pub fn tick(&mut self, level: &Level, player: Vec3) {
        self.ticks += 1;
        if self.seed == 0 {
            self.seed = 0x9E37_79B9_7F4A_7C15;
        }
        for pos in level.block_entities() {
            match level.block_state(pos[0], pos[1], pos[2]).and_then(kind_of) {
                Some(Kind::EnchantingTable) => {
                    self.anims.entry(pos).or_insert(Anim::Book(Book::default()));
                }
                Some(Kind::Skull { powered, .. }) => {
                    if *powered {
                        self.anims.entry(pos).or_insert(Anim::Skull {
                            ticks: 0,
                            animating: true,
                        });
                    }
                    if let Some(Anim::Skull { animating, .. }) = self.anims.get_mut(&pos) {
                        *animating = *powered;
                    }
                }
                _ => {}
            }
        }
        self.anims.retain(|pos, anim| {
            let kind = level.block_state(pos[0], pos[1], pos[2]).and_then(kind_of);
            matches!(
                (anim, kind),
                (
                    Anim::Lid(_),
                    Some(Kind::Chest { .. } | Kind::Shulker { .. })
                ) | (Anim::Bell { .. }, Some(Kind::Bell))
                    | (Anim::Book(_), Some(Kind::EnchantingTable))
                    | (Anim::Pot { .. }, Some(Kind::DecoratedPot { .. }))
                    | (Anim::Skull { .. }, Some(Kind::Skull { .. }))
            )
        });
        let positions: Vec<[i32; 3]> = self.anims.keys().copied().collect();
        for pos in positions {
            let mut anim = self.anims[&pos];
            match &mut anim {
                Anim::Lid(lid) => lid.tick(),
                Anim::Bell { ticks, shaking, .. } => {
                    if *shaking {
                        *ticks += 1;
                    }
                    if *ticks >= 50 {
                        *shaking = false;
                        *ticks = 0;
                    }
                }
                Anim::Book(book) => self.tick_book(book, pos, player),
                Anim::Skull {
                    ticks,
                    animating: true,
                } => *ticks += 1,
                Anim::Skull { .. } => {}
                Anim::Pot { .. } => {}
            }
            self.anims.insert(pos, anim);
        }
        let mut running: Vec<[i32; 3]> = self
            .anims
            .iter()
            .filter(|(_, anim)| anim.is_running(self.ticks))
            .map(|(&pos, _)| pos)
            .collect();
        running.sort_unstable();
        if running != self.running {
            self.running = running;
            self.still_revision += 1;
        }
    }

    /// `EnchantingTableBlockEntity.bookAnimationTick`.
    pub(super) fn tick_book(&mut self, book: &mut Book, pos: [i32; 3], player: Vec3) {
        book.old_open = book.open;
        book.old_rot = book.rot;
        let center = Vec3::new(
            pos[0] as f32 + 0.5,
            pos[1] as f32 + 0.5,
            pos[2] as f32 + 0.5,
        );
        if player.distance_squared(center) < 3.0 * 3.0 {
            book.target_rot = (player.z - center.z).atan2(player.x - center.x);
            book.open += 0.1;
            if book.open < 0.5 || self.random(40) == 0 {
                let old = book.flip_target;
                while book.flip_target == old {
                    book.flip_target += (self.random(4) - self.random(4)) as f32;
                }
            }
        } else {
            book.target_rot += 0.02;
            book.open -= 0.1;
        }
        book.rot = wrap_radians(book.rot);
        book.target_rot = wrap_radians(book.target_rot);
        let turn = wrap_radians(book.target_rot - book.rot);
        book.rot += turn * 0.4;
        book.open = book.open.clamp(0.0, 1.0);
        book.time += 1;
        book.old_flip = book.flip;
        let diff = ((book.flip_target - book.flip) * 0.4).clamp(-0.2, 0.2);
        book.flip_speed += (diff - book.flip_speed) * 0.9;
        book.flip += book.flip_speed;
    }

    pub(super) fn lid(&self, pos: [i32; 3], partial: f32) -> f32 {
        match self.anims.get(&pos) {
            Some(Anim::Lid(lid)) => lid.at(partial),
            _ => 0.0,
        }
    }
}

/// `ChestLidController`.
#[derive(Default, Clone, Copy)]
pub(super) struct Lid {
    pub(super) open: bool,
    pub(super) openness: f32,
    pub(super) old: f32,
}

impl Lid {
    pub(super) fn tick(&mut self) {
        self.old = self.openness;
        self.openness = if self.open {
            (self.openness + 0.1).min(1.0)
        } else {
            (self.openness - 0.1).max(0.0)
        };
    }

    pub(super) fn at(&self, partial: f32) -> f32 {
        self.old + (self.openness - self.old) * partial
    }
}

/// `EnchantingTableBlockEntity`'s book state.
#[derive(Default, Clone, Copy)]
pub(super) struct Book {
    pub(super) time: u32,
    pub(super) flip: f32,
    pub(super) old_flip: f32,
    pub(super) flip_target: f32,
    pub(super) flip_speed: f32,
    pub(super) open: f32,
    pub(super) old_open: f32,
    pub(super) rot: f32,
    pub(super) old_rot: f32,
    pub(super) target_rot: f32,
}

/// What animates, per block entity.
#[derive(Clone, Copy)]
pub(super) enum Anim {
    Lid(Lid),
    Bell {
        ticks: u32,
        shaking: bool,
        direction: Direction,
    },
    Book(Book),
    Pot {
        started: u64,
        negative: bool,
    },
    Skull {
        ticks: u32,
        /// Powered now: the count goes on.
        animating: bool,
    },
}

impl Anim {
    /// Whether it changes the model from one frame to the next.
    pub(super) fn is_running(&self, now: u64) -> bool {
        match *self {
            Anim::Lid(lid) => lid.old != lid.openness,
            Anim::Bell { shaking, .. } => shaking,
            Anim::Book(_) => true,
            Anim::Skull { animating, .. } => animating,
            Anim::Pot { started, negative } => now - started < pot_duration(negative),
        }
    }
}

/// How many ticks a decorated pot wobbles.
pub(super) fn pot_duration(negative: bool) -> u64 {
    if negative { 10 } else { 7 }
}

/// `Mth.wrapDegrees` for radians.
pub(super) fn wrap_radians(mut angle: f32) -> f32 {
    while angle >= PI {
        angle -= TAU;
    }
    while angle < -PI {
        angle += TAU;
    }
    angle
}
