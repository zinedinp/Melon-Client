//! What vanilla's smooth lighting reads off a quad's geometry.

use glam::Vec3;

use crate::world::block::direction::Direction;
use crate::world::geometry::closest_direction;

/// The facts above for one quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AoShape {
    /// `BakedQuad.direction`.
    pub direction: Direction,
    /// `facePartial`: it doesn't cover its side of the block.
    pub partial: bool,
    /// Flat along its own axis (min == max), the first half of `faceCubic`.
    pub flat: bool,
    /// On the block's side along its axis.
    pub boundary: bool,
    /// Per vertex, the corner group (vanilla's `vert0..vert3`) it takes.
    pub groups: [u8; 4],
    /// Per vertex, its four group weights for partial quads.
    pub weights: [[f32; 4]; 4],
}

const D: usize = 0;
const U: usize = 1;
const N: usize = 2;
const S: usize = 3;
const W: usize = 4;
const E: usize = 5;
const FD: usize = 6;
const FU: usize = 7;
const FN: usize = 8;
const FS: usize = 9;
const FW: usize = 10;
const FE: usize = 11;

/// Four `SizeInfo` products: each pair is one term of a group weight.
macro_rules! vert {
    ($($a:ident $b:ident),+ $(,)?) => {
        [$($a, $b),+]
    };
}

/// `AdjacencyInfo.vert0Weights..vert3Weights` by direction.
const WEIGHTS: [[[usize; 8]; 4]; 6] = [
    [
        vert!(FW S, FW FS, W FS, W S),
        vert!(FW N, FW FN, W FN, W N),
        vert!(FE N, FE FN, E FN, E N),
        vert!(FE S, FE FS, E FS, E S),
    ],
    [
        vert!(E S, E FS, FE FS, FE S),
        vert!(E N, E FN, FE FN, FE N),
        vert!(W N, W FN, FW FN, FW N),
        vert!(W S, W FS, FW FS, FW S),
    ],
    [
        vert!(U FW, U W, FU W, FU FW),
        vert!(U FE, U E, FU E, FU FE),
        vert!(D FE, D E, FD E, FD FE),
        vert!(D FW, D W, FD W, FD FW),
    ],
    [
        vert!(U FW, FU FW, FU W, U W),
        vert!(D FW, FD FW, FD W, D W),
        vert!(D FE, FD FE, FD E, D E),
        vert!(U FE, FU FE, FU E, U E),
    ],
    [
        vert!(U S, U FS, FU FS, FU S),
        vert!(U N, U FN, FU FN, FU N),
        vert!(D N, D FN, FD FN, FD N),
        vert!(D S, D FS, FD FS, FD S),
    ],
    [
        vert!(FD S, FD FS, D FS, D S),
        vert!(FD N, FD FN, D FN, D N),
        vert!(FU N, FU FN, U FN, U N),
        vert!(FU S, FU FS, U FS, U S),
    ],
];

/// `AmbientVertexRemap`: the `FaceInfo` slot each group `vert0..vert3`
const REMAP: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [2, 3, 0, 1],
    [3, 0, 1, 2],
    [0, 1, 2, 3],
    [3, 0, 1, 2],
    [1, 2, 3, 0],
];

macro_rules! max {
    (min) => {
        false
    };
    (max) => {
        true
    };
}

/// `FaceInfo` winding: four corners as min/max on x, y, z.
macro_rules! face {
    ($(($x:ident, $y:ident, $z:ident)),+ $(,)?) => {
        [$( [max!($x), max!($y), max!($z)] ),+]
    };
}

/// `FaceInfo`: each slot's corner as max (true) or min per axis x, y, z.
const FACE_INFO: [[[bool; 3]; 4]; 6] = [
    face!(
        (min, min, max),
        (min, min, min),
        (max, min, min),
        (max, min, max)
    ),
    face!(
        (min, max, min),
        (min, max, max),
        (max, max, max),
        (max, max, min)
    ),
    face!(
        (max, max, min),
        (max, min, min),
        (min, min, min),
        (min, max, min)
    ),
    face!(
        (min, max, max),
        (min, min, max),
        (max, min, max),
        (max, max, max)
    ),
    face!(
        (min, max, min),
        (min, min, min),
        (min, min, max),
        (min, max, max)
    ),
    face!(
        (max, max, max),
        (max, min, max),
        (max, min, min),
        (max, max, min)
    ),
];

const fn plane_axes(d: usize) -> [usize; 2] {
    match d {
        0 | 1 => [0, 2],
        2 | 3 => [0, 1],
        _ => [1, 2],
    }
}

const fn remap_inverse() -> [[u8; 4]; 6] {
    let mut out = [[0u8; 4]; 6];
    let mut d = 0;
    while d < 6 {
        let mut k = 0;
        while k < 4 {
            out[d][REMAP[d][k]] = k as u8;
            k += 1;
        }
        d += 1;
    }
    out
}

const fn remap_group(d: usize, slot: usize) -> u8 {
    let mut g = 0;
    while g < 4 {
        if REMAP[d][g] == slot {
            return g as u8;
        }
        g += 1;
    }
    0
}

const fn group_of_bits() -> [[u8; 4]; 6] {
    let mut out = [[0u8; 4]; 6];
    let mut d = 0;
    while d < 6 {
        let plane = plane_axes(d);
        let mut slot = 0;
        while slot < 4 {
            let info = FACE_INFO[d][slot];
            let bits = (info[plane[0]] as usize) | (info[plane[1]] as usize) << 1;
            out[d][bits] = remap_group(d, slot);
            slot += 1;
        }
        d += 1;
    }
    out
}

const GROUPS_BY_INDEX: [[u8; 4]; 6] = remap_inverse();
const GROUP_OF: [[u8; 4]; 6] = group_of_bits();

fn corner_groups(d: usize, corners: [Vec3; 4], min: Vec3, max: Vec3) -> [u8; 4] {
    let by_index = GROUPS_BY_INDEX[d];
    let plane = plane_axes(d);
    if max[plane[0]] <= min[plane[0]] || max[plane[1]] <= min[plane[1]] {
        return by_index;
    }
    let mid = (min + max) * 0.5;
    let mut groups = [0u8; 4];
    let mut seen = 0u8;
    for (i, c) in corners.iter().enumerate() {
        let bits = usize::from(c[plane[0]] > mid[plane[0]])
            | usize::from(c[plane[1]] > mid[plane[1]]) << 1;
        let g = GROUP_OF[d][bits];
        if seen & (1 << g) != 0 {
            return by_index;
        }
        seen |= 1 << g;
        groups[i] = g;
    }
    groups
}

impl AoShape {
    /// The shape of a quad with block-space `corners` (`0..1`).
    pub fn of(corners: [Vec3; 4]) -> Self {
        let normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        let direction = closest_direction(normal).unwrap_or(Direction::Up);
        let min = corners.iter().fold(Vec3::splat(32.0), |m, &c| m.min(c));
        let max = corners.iter().fold(Vec3::splat(-32.0), |m, &c| m.max(c));
        let size = [
            min.y,
            max.y,
            min.z,
            max.z,
            min.x,
            max.x,
            1.0 - min.y,
            1.0 - max.y,
            1.0 - min.z,
            1.0 - max.z,
            1.0 - min.x,
            1.0 - max.x,
        ];
        let (lo, hi) = (1.0e-4, 0.9999);
        let d = direction.index();
        let axis = direction.axis();
        let plane = plane_axes(d);
        let partial = plane.iter().any(|&a| min[a] >= lo || max[a] <= hi);
        let flat = min[axis] == max[axis];
        let boundary = if direction.is_positive() {
            max[axis] > hi
        } else {
            min[axis] < lo
        };

        let table = &WEIGHTS[d];
        let group_weights: [[f32; 4]; 4] = std::array::from_fn(|k| {
            std::array::from_fn(|j| size[table[k][2 * j]] * size[table[k][2 * j + 1]])
        });
        let groups = corner_groups(d, corners, min, max);
        let weights = std::array::from_fn(|i| group_weights[usize::from(groups[i])]);
        Self {
            direction,
            partial,
            flat,
            boundary,
            groups,
            weights,
        }
    }

    /// `faceCubic`: the light and shade come from the block in front.
    pub fn cubic(&self, collision_full: bool) -> bool {
        self.flat && (self.boundary || collision_full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    #[test]
    fn a_full_top_face_maps_its_corners_like_vanilla() {
        let shape = AoShape::of([v(0., 1., 0.), v(0., 1., 1.), v(1., 1., 1.), v(1., 1., 0.)]);
        assert!(!shape.partial && shape.flat && shape.boundary);
        assert!(shape.cubic(false));
        assert_eq!(shape.groups, [2, 3, 0, 1]);
    }

    #[test]
    fn vertex_order_doesnt_matter() {
        let shape = AoShape::of([v(1., 1., 1.), v(1., 1., 0.), v(0., 1., 0.), v(0., 1., 1.)]);
        assert_eq!(shape.groups, [0, 1, 2, 3]);
    }

    #[test]
    fn unmatched_faces_still_take_the_vanilla_remap() {
        let shape = AoShape::of([v(0.5, 1., 0.5); 4]);
        assert_eq!(shape.groups, [2, 3, 0, 1]);
    }

    #[test]
    fn a_slab_top_is_whole_but_not_on_the_side() {
        let shape = AoShape::of([
            v(0., 0.5, 0.),
            v(0., 0.5, 1.),
            v(1., 0.5, 1.),
            v(1., 0.5, 0.),
        ]);
        assert!(!shape.partial && shape.flat && !shape.boundary);
        assert!(!shape.cubic(false) && shape.cubic(true));
    }

    #[test]
    fn a_small_face_is_partial_and_weighs_its_corners() {
        let (a, b) = (7.0 / 16.0, 9.0 / 16.0);
        let shape = AoShape::of([
            v(a, 0.625, a),
            v(a, 0.625, b),
            v(b, 0.625, b),
            v(b, 0.625, a),
        ]);
        assert!(shape.partial);
        for weights in shape.weights {
            assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn rotated_quads_find_their_nearest_axis() {
        let shape = AoShape::of([v(0., 0., 0.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 0.)]);
        assert_eq!(shape.direction, Direction::South);
    }
}
