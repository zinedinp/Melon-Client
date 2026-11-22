//! `BiomeManager.getBiome`: which biome a block is in.

/// `LinearCongruentialGenerator.next`.
#[inline]
fn lcg(rval: i64, add: i64) -> i64 {
    rval.wrapping_mul(
        rval.wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407),
    )
    .wrapping_add(add)
}

/// `BiomeManager.getFiddle`.
#[inline]
fn fiddle(rval: i64) -> f64 {
    let uniform = (rval >> 24).rem_euclid(1024) as f64 / 1024.0;
    (uniform - 0.5) * 0.9
}

/// The jitter of quart corner `(x, y, z)`.
pub fn corner_fiddle(seed: i64, [x, y, z]: [i32; 3]) -> [f64; 3] {
    let mut rval = seed;
    for v in [x, y, z, x, y, z] {
        rval = lcg(rval, i64::from(v));
    }
    let fx = fiddle(rval);
    rval = lcg(rval, seed);
    let fy = fiddle(rval);
    rval = lcg(rval, seed);
    [fx, fy, fiddle(rval)]
}

/// The cell, in quart coordinates, block `(x, y, z)` takes its biome from.
pub fn cell(seed: i64, x: i32, y: i32, z: i32) -> [i32; 3] {
    cell_with(x, y, z, |corner| corner_fiddle(seed, corner))
}

/// [`cell`] with the corners' fiddles from `fiddles`.
#[inline]
pub fn cell_with(x: i32, y: i32, z: i32, fiddles: impl FnMut([i32; 3]) -> [f64; 3]) -> [i32; 3] {
    corner(parent(x, y, z), nearest_corner(x, y, z, fiddles))
}

/// The cell `getBiome` looks from for block `(x, y, z)`.
#[inline]
pub fn parent(x: i32, y: i32, z: i32) -> [i32; 3] {
    [(x - 2) >> 2, (y - 2) >> 2, (z - 2) >> 2]
}

/// Corner `i` (`0..8`, bit 4 for x, 2 for y, 1 for z) of `parent`.
#[inline]
pub fn corner(parent: [i32; 3], i: usize) -> [i32; 3] {
    [
        parent[0] + i32::from(i & 4 != 0),
        parent[1] + i32::from(i & 2 != 0),
        parent[2] + i32::from(i & 1 != 0),
    ]
}

/// Which of the eight corners of [`parent`] block `(x, y, z)` takes its biome from.
#[inline]
pub fn nearest_corner(
    x: i32,
    y: i32,
    z: i32,
    mut fiddles: impl FnMut([i32; 3]) -> [f64; 3],
) -> usize {
    let parent = parent(x, y, z);
    let fract = [x - 2, y - 2, z - 2].map(|v| f64::from(v & 3) / 4.0);
    let mut best = 0;
    let mut nearest = f64::INFINITY;
    for i in 0..8 {
        let [dx, dy, dz] = [4, 2, 1].map(|bit| f64::from(u8::from(i & bit != 0)));
        let [dx, dy, dz] = [fract[0] - dx, fract[1] - dy, fract[2] - dz];
        let [fx, fy, fz] = fiddles(corner(parent, i));
        let next = (dz + fz).powi(2) + (dy + fy).powi(2) + (dx + fx).powi(2);
        if nearest > next {
            best = i;
            nearest = next;
        }
    }
    best
}

/// The biome at block `(x, y, z)`, `noise_biome` giving the one stored at a cell.
pub fn biome(seed: i64, x: i32, y: i32, z: i32, noise_biome: &dyn Fn(i32, i32, i32) -> u16) -> u16 {
    let [qx, qy, qz] = cell(seed, x, y, z);
    noise_biome(qx, qy, qz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_takes_a_neighbouring_cell() {
        let seed = 1_234_567_890_123;
        for (x, y, z) in [(0, 0, 0), (5, 70, -9), (-100, -60, 33), (1000, 300, 1000)] {
            let [qx, qy, qz] = cell(seed, x, y, z);
            for (q, b) in [(qx, x), (qy, y), (qz, z)] {
                let parent = (b - 2) >> 2;
                assert!(q == parent || q == parent + 1, "{q} for block {b}");
            }
        }
    }

    #[test]
    fn borders_are_ragged_but_cell_centres_are_their_own() {
        let seed = -4_172_144_997_891_118_834;
        let mut off_cell = 0;
        for x in 0..64 {
            for z in 0..64 {
                let [qx, _, qz] = cell(seed, x, 64, z);
                if [qx, qz] != [x >> 2, z >> 2] {
                    off_cell += 1;
                }
            }
        }
        assert!(off_cell > 64 && off_cell < 64 * 64 / 2, "{off_cell}");
    }

    #[test]
    fn lcg_matches_java() {
        assert_eq!(
            lcg(1, 2),
            6_364_136_223_846_793_005i64
                .wrapping_add(1_442_695_040_888_963_407)
                .wrapping_add(2)
        );
        assert_eq!(fiddle(0), -0.45);
    }
}
