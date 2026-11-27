//! `Direction` and the player's view of it: horizontal facing and
//! `orderedByNearest`.

use pumpkin_data::block_properties::{Axis, Facing};

use crate::math::angle;

/// `Direction` in 3D data order (a hit's `face`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Dir {
    pub const HORIZONTAL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

    /// The face of a [`crate::pick::BlockHit`].
    pub fn from_face(face: u8) -> Option<Dir> {
        Some(match face {
            0 => Dir::Down,
            1 => Dir::Up,
            2 => Dir::North,
            3 => Dir::South,
            4 => Dir::West,
            5 => Dir::East,
            _ => return None,
        })
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::Down => Dir::Up,
            Dir::Up => Dir::Down,
            Dir::North => Dir::South,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
            Dir::East => Dir::West,
        }
    }

    /// `getClockWise` from above; up/down unchanged.
    pub fn clockwise(self) -> Dir {
        match self {
            Dir::North => Dir::East,
            Dir::East => Dir::South,
            Dir::South => Dir::West,
            Dir::West => Dir::North,
            d => d,
        }
    }

    pub fn counter_clockwise(self) -> Dir {
        match self {
            Dir::North => Dir::West,
            Dir::West => Dir::South,
            Dir::South => Dir::East,
            Dir::East => Dir::North,
            d => d,
        }
    }

    pub fn is_horizontal(self) -> bool {
        !matches!(self, Dir::Up | Dir::Down)
    }

    pub fn axis(self) -> Axis {
        match self {
            Dir::West | Dir::East => Axis::X,
            Dir::Up | Dir::Down => Axis::Y,
            Dir::North | Dir::South => Axis::Z,
        }
    }

    pub fn step(self) -> [i32; 3] {
        match self {
            Dir::Down => [0, -1, 0],
            Dir::Up => [0, 1, 0],
            Dir::North => [0, 0, -1],
            Dir::South => [0, 0, 1],
            Dir::West => [-1, 0, 0],
            Dir::East => [1, 0, 0],
        }
    }

    pub fn offset(self, [x, y, z]: [i32; 3]) -> [i32; 3] {
        let [dx, dy, dz] = self.step();
        [x + dx, y + dy, z + dz]
    }

    pub fn facing(self) -> Facing {
        match self {
            Dir::Down => Facing::Down,
            Dir::Up => Facing::Up,
            Dir::North => Facing::North,
            Dir::South => Facing::South,
            Dir::West => Facing::West,
            Dir::East => Facing::East,
        }
    }

    pub fn from_facing(facing: Facing) -> Dir {
        match facing {
            Facing::Down => Dir::Down,
            Facing::Up => Dir::Up,
            Facing::North => Dir::North,
            Facing::South => Dir::South,
            Facing::West => Dir::West,
            Facing::East => Dir::East,
        }
    }

    pub fn block_direction(self) -> pumpkin_data::BlockDirection {
        use pumpkin_data::BlockDirection as B;
        match self {
            Dir::Down => B::Down,
            Dir::Up => B::Up,
            Dir::North => B::North,
            Dir::South => B::South,
            Dir::West => B::West,
            Dir::East => B::East,
        }
    }

    /// `get2DDataValue`: south, west, north, east.
    pub fn data_2d(self) -> i32 {
        match self {
            Dir::South => 0,
            Dir::West => 1,
            Dir::North => 2,
            Dir::East => 3,
            _ => -1,
        }
    }

    fn from_2d(value: i32) -> Dir {
        [Dir::South, Dir::West, Dir::North, Dir::East][(value & 3) as usize]
    }

    /// `Direction.fromYRot`.
    pub fn from_y_rot(yaw: f64) -> Dir {
        Dir::from_2d((yaw / 90.0 + 0.5).floor() as i32)
    }
}

/// `RotationSegment.convertToSegment(float)`, halves rounded up.
pub fn rotation_segment(degrees: f32) -> u8 {
    ((degrees * (16.0 / 360.0_f32) + 0.5).floor() as i32 & 15) as u8
}

/// `RotationSegment.convertToSegment(Direction)`.
pub fn direction_segment(dir: Dir) -> u8 {
    if dir.is_horizontal() {
        (dir.data_2d() << 2) as u8
    } else {
        0
    }
}

/// `RotationSegment.convertToDirection`.
pub fn segment_direction(segment: u8) -> Option<Dir> {
    match segment {
        0 => Some(Dir::North),
        4 => Some(Dir::East),
        8 => Some(Dir::South),
        12 => Some(Dir::West),
        _ => None,
    }
}

/// `Direction.orderedByNearest` (vanilla degrees).
pub fn ordered_by_nearest(yaw: f32, pitch: f32) -> [Dir; 6] {
    let to_radians = (std::f64::consts::PI / 180.0) as f32;
    let pitch = pitch * to_radians;
    let yaw = -yaw * to_radians;
    let (pitch_sin, pitch_cos) = (angle::sin(pitch), angle::cos(pitch));
    let (yaw_sin, yaw_cos) = (angle::sin(yaw), angle::cos(yaw));
    let x_pos = yaw_sin > 0.0;
    let y_pos = pitch_sin < 0.0;
    let z_pos = yaw_cos > 0.0;
    let x_yaw = if x_pos { yaw_sin } else { -yaw_sin };
    let y_mag = if y_pos { -pitch_sin } else { pitch_sin };
    let z_yaw = if z_pos { yaw_cos } else { -yaw_cos };
    let x_mag = x_yaw * pitch_cos;
    let z_mag = z_yaw * pitch_cos;
    let ax = if x_pos { Dir::East } else { Dir::West };
    let ay = if y_pos { Dir::Up } else { Dir::Down };
    let az = if z_pos { Dir::South } else { Dir::North };
    let make = |a: Dir, b: Dir, c: Dir| [a, b, c, c.opposite(), b.opposite(), a.opposite()];
    if x_yaw > z_yaw {
        if y_mag > x_mag {
            make(ay, ax, az)
        } else if z_mag > y_mag {
            make(ax, az, ay)
        } else {
            make(ax, ay, az)
        }
    } else if y_mag > z_mag {
        make(ay, az, ax)
    } else if x_mag > y_mag {
        make(az, ax, ay)
    } else {
        make(az, ay, ax)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_horizontal_direction_follows_the_yaw() {
        assert_eq!(Dir::from_y_rot(0.0), Dir::South);
        assert_eq!(Dir::from_y_rot(90.0), Dir::West);
        assert_eq!(Dir::from_y_rot(180.0), Dir::North);
        assert_eq!(Dir::from_y_rot(-90.0), Dir::East);
        assert_eq!(Dir::from_y_rot(44.0), Dir::South);
    }

    #[test]
    fn looking_down_puts_down_first_and_up_last() {
        let dirs = ordered_by_nearest(0.0, 80.0);
        assert_eq!(dirs[0], Dir::Down);
        assert_eq!(dirs[5], Dir::Up);
        let flat = ordered_by_nearest(180.0, 0.0);
        assert_eq!(flat[0], Dir::North);
    }

    #[test]
    fn rotation_segments_wrap_at_sixteen() {
        assert_eq!(rotation_segment(0.0), 0);
        assert_eq!(rotation_segment(180.0), 8);
        assert_eq!(rotation_segment(359.0), 0);
        assert_eq!(rotation_segment(-90.0), 12);
    }
}
