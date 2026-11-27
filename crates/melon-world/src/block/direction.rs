//! Vanilla `Direction`, in its ordinal order (down, up, north, south, west, east).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Direction {
    Down = 0,
    Up = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

/// Unit step towards each direction, `(x, y, z)`, indexed by ordinal.
const STEPS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// The axis each direction runs along: 0 = X, 1 = Y, 2 = Z.
const AXES: [usize; 6] = [1, 1, 2, 2, 0, 0];

impl Direction {
    pub const ALL: [Direction; 6] = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];
    /// `Direction.Plane.HORIZONTAL`.
    pub const HORIZONTAL: [Direction; 4] = [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// `Direction.getUnitVec3i`.
    #[inline]
    pub const fn step(self) -> [i32; 3] {
        STEPS[self.index()]
    }

    /// `Direction.getOpposite`.
    #[inline]
    pub const fn opposite(self) -> Direction {
        Direction::ALL[self.index() ^ 1]
    }

    #[inline]
    pub const fn axis(self) -> usize {
        AXES[self.index()]
    }

    /// Whether this direction points along its axis' positive side.
    #[inline]
    pub const fn is_positive(self) -> bool {
        self.index() & 1 == 1
    }

    /// This direction's bit in a 6-bit set of directions.
    #[inline]
    pub const fn bit(self) -> u8 {
        1 << self as u8
    }

    /// `Direction.byName`, for block-state `facing` values and model face names.
    pub fn from_name(name: &str) -> Option<Direction> {
        Some(match name {
            "down" | "bottom" => Direction::Down,
            "up" | "top" => Direction::Up,
            "north" => Direction::North,
            "south" => Direction::South,
            "west" => Direction::West,
            "east" => Direction::East,
            _ => return None,
        })
    }

    /// `Direction.toYRot`.
    pub fn y_rot(self) -> f32 {
        match self {
            Direction::South => 0.0,
            Direction::West => 90.0,
            Direction::North => 180.0,
            Direction::East => 270.0,
            Direction::Up | Direction::Down => 0.0,
        }
    }

    /// `Direction.getClockWise` seen from above; up and down stay.
    pub fn clockwise(self) -> Direction {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            other => other,
        }
    }
}

/// A set of directions as 6 bits, one per ordinal.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Directions(pub u8);

impl Directions {
    pub const NONE: Directions = Directions(0);

    #[inline]
    pub const fn contains(self, direction: Direction) -> bool {
        self.0 & direction.bit() != 0
    }

    #[inline]
    pub const fn with(self, direction: Direction) -> Directions {
        Directions(self.0 | direction.bit())
    }

    pub fn iter(self) -> impl Iterator<Item = Direction> {
        Direction::ALL
            .into_iter()
            .filter(move |&d| self.contains(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposites_cancel_and_share_an_axis() {
        for direction in Direction::ALL {
            let [x, y, z] = direction.step();
            let [ox, oy, oz] = direction.opposite().step();
            assert_eq!([x + ox, y + oy, z + oz], [0, 0, 0]);
            assert_eq!(direction.axis(), direction.opposite().axis());
            assert_eq!(
                direction.step()[direction.axis()] > 0,
                direction.is_positive()
            );
        }
    }

    #[test]
    fn four_clockwise_turns_return_and_names_round_trip() {
        for direction in Direction::ALL {
            let turned = direction.clockwise().clockwise().clockwise().clockwise();
            assert_eq!(turned, direction);
        }
        for (name, direction) in ["down", "up", "north", "south", "west", "east"]
            .into_iter()
            .zip(Direction::ALL)
        {
            assert_eq!(Direction::from_name(name), Some(direction));
        }
        assert_eq!(Direction::from_name("top"), Some(Direction::Up));
        assert_eq!(Direction::from_name("sideways"), None);
    }
}
