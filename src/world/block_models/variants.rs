//! Weighted random variants (vanilla `WeightedVariants`, `MultiPartModel`):

use std::sync::Arc;

use crate::world::java_random::JavaRandom;

/// A state's models.
pub enum StateModel<Q> {
    Fixed(Arc<[Q]>),
    Random(Arc<RandomModel<Q>>),
}

impl<Q> Clone for StateModel<Q> {
    fn clone(&self) -> Self {
        match self {
            Self::Fixed(quads) => Self::Fixed(Arc::clone(quads)),
            Self::Random(model) => Self::Random(Arc::clone(model)),
        }
    }
}

/// The parts of a state with a random choice somewhere.
pub struct RandomModel<Q> {
    /// From `multipart`: every part picks from the same seed. A `variants`
    pub multipart: bool,
    pub parts: Vec<Part<Q>>,
}

/// One weighted list of options (a single option draws nothing).
pub struct Part<Q> {
    pub total: u32,
    pub options: Vec<(u32, Arc<[Q]>)>,
}

impl<Q> StateModel<Q> {
    /// Every quad any position could show.
    pub fn all_quads(&self) -> impl Iterator<Item = &Q> {
        let (fixed, random) = match self {
            Self::Fixed(quads) => (Some(quads.iter()), None),
            Self::Random(model) => (None, Some(model)),
        };
        let random = random.into_iter().flat_map(|model| {
            model
                .parts
                .iter()
                .flat_map(|part| part.options.iter().flat_map(|(_, quads)| quads.iter()))
        });
        fixed.into_iter().flatten().chain(random)
    }

    /// The same model with each option's quads turned into `R`s by `map`.
    pub fn map<R>(&self, map: &mut impl FnMut(&Arc<[Q]>) -> Arc<[R]>) -> StateModel<R> {
        match self {
            Self::Fixed(quads) => StateModel::Fixed(map(quads)),
            Self::Random(model) => StateModel::Random(Arc::new(RandomModel {
                multipart: model.multipart,
                parts: model
                    .parts
                    .iter()
                    .map(|part| Part {
                        total: part.total,
                        options: part
                            .options
                            .iter()
                            .map(|(weight, quads)| (*weight, map(quads)))
                            .collect(),
                    })
                    .collect(),
            })),
        }
    }

    /// Hands `out` the quads the block at world position `pos` shows.
    #[inline]
    pub fn quads_at(&self, pos: [i32; 3], mut out: impl FnMut(&[Q])) {
        match self {
            Self::Fixed(quads) => out(quads),
            Self::Random(model) => model.pick(pos, &mut out),
        }
    }
}

impl<Q> RandomModel<Q> {
    fn pick(&self, [x, y, z]: [i32; 3], out: &mut impl FnMut(&[Q])) {
        let mut random = JavaRandom::new(seed(x, y, z));
        if self.multipart {
            let shared = random.next_long();
            for part in &self.parts {
                random.set_seed(shared);
                part.pick(&mut random, out);
            }
        } else {
            for part in &self.parts {
                part.pick(&mut random, out);
            }
        }
    }
}

impl<Q> Part<Q> {
    /// `WeightedList.getRandomOrThrow`: subtract weights in order.
    fn pick(&self, random: &mut JavaRandom, out: &mut impl FnMut(&[Q])) {
        if let [(_, only)] = self.options.as_slice() {
            out(only);
            return;
        }
        let mut selection = random.next_int(self.total.max(1) as i32) as i64;
        for (weight, quads) in &self.options {
            selection -= i64::from(*weight);
            if selection < 0 {
                out(quads);
                return;
            }
        }
    }
}

/// `Mth.getSeed`: a block position's seed.
pub fn seed(x: i32, y: i32, z: i32) -> i64 {
    let mut seed = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42_317_861)
        .wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_and_draws_match_vanilla() {
        let expected: [([i32; 3], i64, i32, i32, i64); 5] = [
            ([0, 64, 0], 2_644_866, 3, 2, -548_323_436_128_114_709),
            (
                [1_000_000, 64, -1_000_000],
                -62_637_068_855_065,
                2,
                0,
                -6_572_928_785_117_712_810,
            ),
            (
                [-37, -12, 99],
                29_578_937_877_413,
                2,
                1,
                -7_141_844_355_776_176_936,
            ),
            (
                [123, 70, -456],
                -89_555_873_183_627,
                0,
                0,
                1_540_833_443_792_610_054,
            ),
            (
                [5, 5, 5],
                139_374_462_679_507,
                3,
                2,
                -4_492_860_117_517_264_228,
            ),
        ];
        for ([x, y, z], want, four, three, long) in expected {
            let s = seed(x, y, z);
            assert_eq!(s, want, "seed at {x} {y} {z}");
            assert_eq!(JavaRandom::new(s).next_int(4), four);
            assert_eq!(JavaRandom::new(s).next_int(3), three);
            assert_eq!(JavaRandom::new(s).next_long(), long);
        }
    }

    #[test]
    fn weights_pick_in_order() {
        let part = Part {
            total: 3,
            options: vec![(1, Arc::from([1u8])), (2, Arc::from([2u8]))],
        };
        let model = StateModel::Random(Arc::new(RandomModel {
            multipart: false,
            parts: vec![part],
        }));
        let mut seen = [0usize; 3];
        for x in 0..300 {
            model.quads_at([x, 0, 0], |q| seen[usize::from(q[0])] += 1);
        }
        assert_eq!(seen[0], 0);
        assert!(seen[2] > seen[1] && seen[1] > 50, "{seen:?}");
    }
}
