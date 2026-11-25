//! How a dropped item is drawn (`ItemEntityRenderer`, `ItemClusterRenderState`).

use glam::Vec3;

use crate::world::java_random::JavaRandom;

/// A model flatter than this is stacked along Z (`submitMultipleFromCount`).
pub const FLAT_DEPTH: f32 = 0.0625;

/// `ItemClusterRenderState.getRenderedAmount`.
pub fn rendered_amount(count: u8) -> usize {
    match count {
        0..=1 => 1,
        2..=16 => 2,
        17..=32 => 3,
        33..=48 => 4,
        _ => 5,
    }
}

/// The height the item floats above its resting place.
pub fn bob(age: f32, bob_offs: f32) -> f32 {
    (age / 10.0 + bob_offs).sin() * 0.1 + 0.1
}

/// `ItemEntity.getSpin`: the turn about the Y axis, in radians.
pub fn spin(age: f32, bob_offs: f32) -> f32 {
    age / 20.0 + bob_offs
}

/// Where each copy of a stack sits, from the first.
pub fn offsets(count: u8, depth: f32, seed: i64) -> Vec<Vec3> {
    let copies = rendered_amount(count);
    let mut random = JavaRandom::with_seed(seed);
    let mut spread = |scale: f32| (random.next_float() * 2.0 - 1.0) * 0.15 * scale;
    let mut out = Vec::with_capacity(copies);
    if depth > FLAT_DEPTH {
        out.push(Vec3::ZERO);
        for _ in 1..copies {
            out.push(Vec3::new(spread(1.0), spread(1.0), spread(1.0)));
        }
    } else {
        let step = depth * 1.5;
        let first = -(step * (copies - 1) as f32 / 2.0);
        out.push(Vec3::new(0.0, 0.0, first));
        for i in 1..copies {
            out.push(Vec3::new(spread(0.5), spread(0.5), first + step * i as f32));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_shows_more_copies_as_it_grows() {
        let shown: Vec<_> = [1, 2, 16, 17, 32, 33, 48, 49, 64]
            .into_iter()
            .map(rendered_amount)
            .collect();
        assert_eq!(shown, [1, 2, 2, 3, 3, 4, 4, 5, 5]);
    }

    #[test]
    fn the_first_solid_copy_sits_at_the_origin_and_the_rest_within_reach() {
        let offsets = offsets(64, 0.5, 42);
        assert_eq!(offsets.len(), 5);
        assert_eq!(offsets[0], Vec3::ZERO);
        assert!(offsets[1..].iter().all(|o| o.abs().max_element() <= 0.15));
    }

    #[test]
    fn flat_copies_are_spaced_along_z_around_the_middle() {
        let offsets = offsets(20, 0.03125, 7);
        assert_eq!(offsets.len(), 3);
        let step = 0.03125 * 1.5;
        assert!((offsets[0].z + step).abs() < 1e-6);
        assert!((offsets[1].z).abs() < 1e-6);
        assert!((offsets[2].z - step).abs() < 1e-6);
    }

    #[test]
    fn the_same_seed_gives_the_same_places() {
        assert_eq!(offsets(40, 0.5, 9), offsets(40, 0.5, 9));
    }

    #[test]
    fn the_bob_stays_between_nothing_and_a_fifth() {
        for age in 0..200 {
            let b = bob(age as f32, 1.3);
            assert!((0.0..=0.2).contains(&b));
        }
    }
}
