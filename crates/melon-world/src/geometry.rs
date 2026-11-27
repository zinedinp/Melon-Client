//! What every mesher shares.

use glam::Vec3;

use crate::block::direction::Direction;
use crate::mesh::WorldVertex as Vertex;

/// Corners of a quad in the order they are stored, front facing.
pub const QUAD_CORNERS: [usize; 4] = [0, 1, 2, 3];
/// The same corners wound the other way, for a quad seen from behind.
pub const QUAD_CORNERS_FLIPPED: [usize; 4] = [0, 3, 2, 1];

/// `CardinalLighting.DEFAULT`: how bright a face is by where it points.
pub const SHADE_UP: f32 = 1.0;
pub const SHADE_DOWN: f32 = 0.5;
pub const SHADE_NORTH_SOUTH: f32 = 0.8;
pub const SHADE_EAST_WEST: f32 = 0.6;

/// `CardinalLighting.DEFAULT.byFace`.
pub fn cardinal_shade(direction: Direction) -> f32 {
    match direction {
        Direction::Down => SHADE_DOWN,
        Direction::Up => SHADE_UP,
        Direction::North | Direction::South => SHADE_NORTH_SOUTH,
        Direction::West | Direction::East => SHADE_EAST_WEST,
    }
}

/// `FaceBakery.findClosestDirection`.
pub fn closest_direction(normal: Vec3) -> Option<Direction> {
    if !normal.is_finite() {
        return None;
    }
    let mut best = None;
    let mut closest = 0.0;
    for direction in Direction::ALL {
        let [x, y, z] = direction.step().map(|v| v as f32);
        let product = normal.dot(Vec3::new(x, y, z));
        if product >= 0.0 && product > closest {
            closest = product;
            best = Some(direction);
        }
    }
    best
}

/// Vanilla's directional shade (`BlockAndTintGetter.getShade` with the default lighting).
pub fn shade(normal: Vec3) -> f32 {
    let up = if normal.y > 0.0 { SHADE_UP } else { SHADE_DOWN };
    (normal.x * normal.x * SHADE_EAST_WEST
        + normal.y * normal.y * up
        + normal.z * normal.z * SHADE_NORTH_SOUTH)
        .min(1.0)
}

/// `TextureAtlasSprite.uvShrinkRatio`.
pub const UV_INSET: f32 = 1.0 / 512.0;

/// `TextureAtlasSprite.getU`/`getV`: `(u, v)` in `0..=1` across the sprite `rect`.
pub fn atlas_uv(rect: [f32; 4], u: f32, v: f32) -> [f32; 2] {
    let u = u.clamp(UV_INSET, 1.0 - UV_INSET);
    let v = v.clamp(UV_INSET, 1.0 - UV_INSET);
    [
        rect[0] + (rect[2] - rect[0]) * u,
        rect[1] + (rect[3] - rect[1]) * v,
    ]
}

/// One quad's corners, counter-clockwise from the front; `flipped` writes them the other way
/// round.
pub fn push_quad(
    out: &mut Vec<Vertex>,
    corners: [([f32; 3], [f32; 2]); 4],
    color: [f32; 3],
    flipped: bool,
) {
    let order = if flipped {
        QUAD_CORNERS_FLIPPED
    } else {
        QUAD_CORNERS
    };
    for i in order {
        let (pos, uv) = corners[i];
        out.push(Vertex::new(pos, uv, color));
    }
}

/// [`push_quad`] with packed block/sky light on each corner.
pub fn push_quad_lit(
    out: &mut Vec<Vertex>,
    corners: [([f32; 3], [f32; 2]); 4],
    color: [f32; 3],
    flipped: bool,
    light: [u8; 2],
) {
    let order = if flipped {
        QUAD_CORNERS_FLIPPED
    } else {
        QUAD_CORNERS
    };
    for i in order {
        let (pos, uv) = corners[i];
        out.push(Vertex::new(pos, uv, color).with_light(light[0], light[1]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_uv_stays_inside_the_sprite() {
        let rect = [0.25, 0.5, 0.75, 1.0];
        let [u, v] = atlas_uv(rect, 0.0, 0.0);
        assert!(u > rect[0] && v > rect[1]);
        let [u, v] = atlas_uv(rect, 1.0, 1.0);
        assert!(u < rect[2] && v < rect[3]);
        let [u, v] = atlas_uv(rect, 0.5, 0.5);
        assert!((u - 0.5).abs() < 1e-6 && (v - 0.75).abs() < 1e-6);
    }
}
