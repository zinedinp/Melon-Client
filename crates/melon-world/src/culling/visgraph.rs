//! Vanilla `VisGraph` / `VisibilitySet`.

use crate::block::direction::{Direction, Directions};

const SIZE: usize = 16;
const CELLS: usize = SIZE * SIZE * SIZE;
/// `VisGraph.LEN`-style shortcut.
const SPARSE_LIMIT: usize = SIZE * SIZE;

/// Symmetric 6×6 face-to-face bits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VisibilitySet(u64);

impl VisibilitySet {
    pub const NONE: VisibilitySet = VisibilitySet(0);
    pub const ALL: VisibilitySet = VisibilitySet((1 << 36) - 1);

    #[inline]
    pub fn visible_between(self, from: Direction, to: Direction) -> bool {
        self.0 >> (from.index() * 6 + to.index()) & 1 != 0
    }

    /// Whether a line of sight entering through any of `entered` can leave through `exit`.
    #[inline]
    pub fn any_visible(self, entered: Directions, exit: Direction) -> bool {
        entered.iter().any(|from| self.visible_between(from, exit))
    }

    /// The face pairs as bits, for a compact grid of sets.
    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// The set [`Self::bits`] returned.
    #[inline]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits & Self::ALL.0)
    }

    /// `VisibilitySet.add`: every face in `faces` sees every other.
    fn connect(&mut self, faces: Directions) {
        for a in faces.iter() {
            for b in faces.iter() {
                self.0 |= 1 << (a.index() * 6 + b.index());
            }
        }
    }
}

/// `VisGraph.resolve`.
pub fn compute(solid: impl Fn(usize) -> bool) -> VisibilitySet {
    let mut rows = [0u16; ROWS];
    for (row, bits) in rows.iter_mut().enumerate() {
        for x in 0..SIZE {
            *bits |= u16::from(solid(row * SIZE + x)) << x;
        }
    }
    compute_rows(&rows)
}

/// A section as rows of x: index `y * 16 + z`, bit `x`.
const ROWS: usize = SIZE * SIZE;

/// TODO(perf): reinvestigate. 0.9 µs a section (`perf_world`, 2026-11-15),
/// ~1% of meshing, so low priority. Cheap first step: the `stack` below is
/// a heap allocation on every call; a fixed array (or one kept per thread)
/// removes it. Bigger idea, measure before: flood-fill the whole section as
/// one 4096-bit set (eight AVX-512 registers, 16 AVX2) by dilating (`±x`
/// within the 16-bit rows, `±z` a row, `±y` 16 rows) and masking with
/// `open` until nothing changes. Steps grow with the longest path, so a
/// winding cave can take hundreds; keep the row walk as the fallback.
pub fn compute_rows(solid: &[u16; ROWS]) -> VisibilitySet {
    let solid_count: u32 = solid.iter().map(|row| row.count_ones()).sum();
    if (solid_count as usize) < SPARSE_LIMIT {
        return VisibilitySet::ALL;
    }
    if solid_count as usize == CELLS {
        return VisibilitySet::NONE;
    }
    let open = solid.map(|row| !row);
    let mut seen = [0u16; ROWS];
    let mut set = VisibilitySet::NONE;
    let mut stack: Vec<(usize, u16)> = Vec::with_capacity(64);
    for row in 0..ROWS {
        let (y, z) = (row / SIZE, row % SIZE);
        let surface = if y == 0 || y == SIZE - 1 || z == 0 || z == SIZE - 1 {
            u16::MAX
        } else {
            1 | 1 << (SIZE - 1)
        };
        loop {
            let seeds = open[row] & !seen[row] & surface;
            if seeds == 0 {
                break;
            }
            let mut reached = 0u8;
            stack.push((row, seeds.isolate_lowest_one()));
            while let Some((row, cells)) = stack.pop() {
                let fresh = cells & !seen[row];
                if fresh == 0 {
                    continue;
                }
                let filled = grow_in_row(fresh, open[row] & !seen[row]);
                seen[row] |= filled;
                let (y, z) = (row / SIZE, row % SIZE);
                reached |= faces_of_row(filled, y, z);
                let mut spread = |next: usize| {
                    let cells = filled & open[next] & !seen[next];
                    if cells != 0 {
                        stack.push((next, cells));
                    }
                };
                if y > 0 {
                    spread(row - SIZE);
                }
                if y < SIZE - 1 {
                    spread(row + SIZE);
                }
                if z > 0 {
                    spread(row - 1);
                }
                if z < SIZE - 1 {
                    spread(row + 1);
                }
            }
            set.connect(Directions(reached));
        }
    }
    set
}

/// `cells` (all of them in `open`) grown along the row through `open`.
#[inline]
fn grow_in_row(cells: u16, open: u16) -> u16 {
    let (mut up, mut through_up) = (cells, open);
    let (mut down, mut through_down) = (cells, open);
    for step in [1, 2, 4, 8] {
        up |= through_up & up << step;
        through_up &= through_up << step;
        down |= through_down & down >> step;
        through_down &= through_down >> step;
    }
    up | down
}

/// The section faces the filled cells of row `(y, z)` touch.
#[inline]
fn faces_of_row(filled: u16, y: usize, z: usize) -> u8 {
    let mut faces = 0;
    if filled & 1 != 0 {
        faces |= Direction::West.bit();
    }
    if filled >> (SIZE - 1) != 0 {
        faces |= Direction::East.bit();
    }
    match y {
        0 => faces |= Direction::Down.bit(),
        15 => faces |= Direction::Up.bit(),
        _ => {}
    }
    match z {
        0 => faces |= Direction::North.bit(),
        15 => faces |= Direction::South.bit(),
        _ => {}
    }
    faces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::xorshift::XorShift;

    /// Index steps in `Section.blocks` order (`y*256 + z*16 + x`), by direction.
    const INDEX_STEP: [isize; 6] = [-256, 256, -16, 16, -1, 1];
    /// The section's faces block `(x, y, z)` touches.
    fn edge_faces(x: usize, y: usize, z: usize) -> Directions {
        let edge = |coordinate: usize, low: Direction, high: Direction| match coordinate {
            0 => low.bit(),
            15 => high.bit(),
            _ => 0,
        };
        Directions(
            edge(x, Direction::West, Direction::East)
                | edge(y, Direction::Down, Direction::Up)
                | edge(z, Direction::North, Direction::South),
        )
    }

    /// `(x, y, z)` of a `Section.blocks` index.
    #[inline]
    fn coords(index: usize) -> (usize, usize, usize) {
        (index & 15, index >> 8, index >> 4 & 15)
    }

    /// The block-by-block fill this replaced, to check against.
    fn reference(solid: impl Fn(usize) -> bool) -> VisibilitySet {
        let mut closed = [false; CELLS];
        let mut solid_count = 0;
        for (index, cell) in closed.iter_mut().enumerate() {
            if solid(index) {
                *cell = true;
                solid_count += 1;
            }
        }
        if solid_count < SPARSE_LIMIT {
            return VisibilitySet::ALL;
        }
        if solid_count == CELLS {
            return VisibilitySet::NONE;
        }
        let mut set = VisibilitySet::NONE;
        let mut stack = Vec::new();
        for seed in 0..CELLS {
            let (x, y, z) = coords(seed);
            if closed[seed] || edge_faces(x, y, z) == Directions::NONE {
                continue;
            }
            closed[seed] = true;
            stack.push(seed);
            let mut reached = Directions::NONE;
            while let Some(index) = stack.pop() {
                let (x, y, z) = coords(index);
                reached.0 |= edge_faces(x, y, z).0;
                let position = [x, y, z];
                for direction in Direction::ALL {
                    let along = position[direction.axis()];
                    if (direction.is_positive() && along == 15)
                        || (!direction.is_positive() && along == 0)
                    {
                        continue;
                    }
                    let next = index.wrapping_add_signed(INDEX_STEP[direction.index()]);
                    if !closed[next] {
                        closed[next] = true;
                        stack.push(next);
                    }
                }
            }
            set.connect(reached);
        }
        set
    }

    #[test]
    fn doubling_grows_a_row_as_far_as_shifting_one_by_one() {
        let one_by_one = |mut cells: u16, open: u16| loop {
            let grown = (cells | cells << 1 | cells >> 1) & open;
            if grown == cells {
                return cells;
            }
            cells = grown;
        };
        for open in 0..=u16::MAX {
            for a in 0..16 {
                for b in [a, (a + 5) % 16, (a + 11) % 16] {
                    let cells = (1 << a | 1 << b) & open;
                    assert_eq!(
                        grow_in_row(cells, open),
                        one_by_one(cells, open),
                        "{open:#b} {cells:#b}"
                    );
                }
            }
        }
    }

    #[test]
    fn row_fill_matches_the_block_fill() {
        let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
        let mut next = move || rng.next_u64();
        for round in 0..400 {
            let density = [10, 30, 45, 55, 60, 65, 70, 80, 90][round % 9];
            let cells: Vec<bool> = (0..CELLS)
                .map(|i| match round % 4 {
                    0 => (next() % 100) < density,
                    1 => (i >> 8) % 3 == 0 && (next() % 100) < 97,
                    2 => (i & 15) % 5 == 2 && (next() % 100) < 98,
                    _ => (next() % 100) < density || (i >> 4 & 15) == 7,
                })
                .collect();
            let solid = |i: usize| cells[i];
            assert_eq!(compute(solid), reference(solid), "round {round}");
        }
    }

    fn index(x: usize, y: usize, z: usize) -> usize {
        crate::chunk::Section::index(x, y, z)
    }

    #[test]
    fn empty_sees_through_and_full_sees_nothing() {
        assert_eq!(compute(|_| false), VisibilitySet::ALL);
        assert_eq!(compute(|_| true), VisibilitySet::NONE);
    }

    #[test]
    fn a_wall_splits_west_from_east() {
        let set = compute(|i| i & 15 == 8);
        assert!(!set.visible_between(Direction::West, Direction::East));
        assert!(set.visible_between(Direction::West, Direction::Up));
        assert!(set.visible_between(Direction::East, Direction::North));
    }

    #[test]
    fn a_tunnel_connects_only_its_ends() {
        let set = compute(|i| {
            let (x, y, _) = coords(i);
            !(x == 5 && y == 7)
        });
        assert!(set.visible_between(Direction::North, Direction::South));
        assert!(!set.visible_between(Direction::North, Direction::Up));
        assert!(!set.visible_between(Direction::West, Direction::East));
        assert_eq!(coords(index(3, 9, 12)), (3, 9, 12));
    }
}
