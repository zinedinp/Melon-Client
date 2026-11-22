//! Cloud boxes from `textures/environment/clouds.png` (vanilla `CloudRenderer`).

use std::path::Path;

use crate::cycle::Cycle;
use crate::renderer::WorldVertex;
use crate::world::block::direction::Direction;

/// `CloudRenderer.CELL_SIZE_IN_BLOCKS`.
pub const CELL_SIZE: f32 = 12.0;
/// `CloudRenderer` extruded height.
pub const CELL_HEIGHT: f32 = 4.0;
/// `CloudRenderer.TICKS_PER_CELL`.
const TICKS_PER_CELL: i64 = 400;
/// `CloudRenderer.BLOCKS_PER_SECOND` as the 0.03 blocks/tick factor.
const DRIFT: f32 = 0.030_000_001;
const Z_BIAS: f32 = 3.96;
const EMPTY_ALPHA: u8 = 10;

const SHADE_TOP: f32 = 1.0;
const SHADE_BOTTOM: f32 = 0.7;
/// Vanilla `clouds.vsh` `faceColors`: north/south 0.8, west/east 0.9.
const SHADE_NORTH_SOUTH: f32 = 0.8;
const SHADE_EAST_WEST: f32 = 0.9;

/// How the clouds are drawn (`CloudStatus`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloudStatus {
    Off,
    Fast,
    #[default]
    Fancy,
}

impl CloudStatus {
    pub fn from_options(value: &str) -> Option<Self> {
        match value.trim_matches('"') {
            "false" | "off" => Some(Self::Off),
            "fast" => Some(Self::Fast),
            "true" | "fancy" => Some(Self::Fancy),
            _ => None,
        }
    }

    pub fn options_id(self) -> &'static str {
        match self {
            Self::Off => "false",
            Self::Fast => "fast",
            Self::Fancy => "true",
        }
    }

    /// The value half of `options.renderClouds`.
    pub fn caption_key(self) -> &'static str {
        match self {
            Self::Off => "options.off",
            Self::Fast => "options.clouds.fast",
            Self::Fancy => "options.clouds.fancy",
        }
    }
}

impl Cycle for CloudStatus {
    const ALL: &'static [Self] = &[Self::Off, Self::Fast, Self::Fancy];
}

/// Packed cells of `clouds.png`.
#[derive(Clone, Debug)]
pub struct CloudTexture {
    cells: Box<[u32]>,
    width: i32,
    height: i32,
}

impl CloudTexture {
    /// Empty: no clouds.
    pub fn empty() -> Self {
        Self {
            cells: Box::new([]),
            width: 0,
            height: 0,
        }
    }

    pub fn load(assets_root: &Path) -> Self {
        let Some(rgba) = crate::assets::texture_image(assets_root, "environment/clouds") else {
            return Self::empty();
        };
        let width = rgba.width() as i32;
        let height = rgba.height() as i32;
        if width == 0 || height == 0 {
            return Self::empty();
        }
        let mut cells = vec![0u32; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let p = rgba.get_pixel(x as u32, y as u32).0;
                if p[3] < EMPTY_ALPHA {
                    continue;
                }
                cells[(x + y * width) as usize] = u32::from(p[3]) << 24
                    | u32::from(p[0]) << 16
                    | u32::from(p[1]) << 8
                    | u32::from(p[2]);
            }
        }
        Self {
            cells: cells.into_boxed_slice(),
            width,
            height,
        }
    }

    fn cell(&self, x: i32, z: i32) -> u32 {
        if self.width == 0 {
            return 0;
        }
        let x = x.rem_euclid(self.width);
        let z = z.rem_euclid(self.height);
        self.cells[(x + z * self.width) as usize]
    }

    fn empty_at(&self, x: i32, z: i32) -> bool {
        self.cell(x, z) == 0
    }
}

/// What [`build`] needs besides the texture.
#[derive(Clone, Copy, Debug)]
pub struct CloudFrame {
    pub status: CloudStatus,
    pub color: [f32; 4],
    pub bottom_y: f32,
    pub range_chunks: u8,
    /// In doubles, so the clouds stay put far out.
    pub camera: glam::DVec3,
    pub game_time: i64,
    pub partial_tick: f32,
}

/// Camera-relative quads of the cells around the camera.
pub fn build(texture: &CloudTexture, frame: CloudFrame) -> Vec<WorldVertex> {
    build_with(texture, frame, false)
}

/// What decides the quads [`build_anchored`] makes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudKey {
    status: CloudStatus,
    bottom_y: u32,
    range_chunks: u8,
    cell: (i32, i32),
    /// Whether the layer's bottom is at or under the camera, and its top at or over it.
    inside: (bool, bool),
}

fn cell_of(texture: &CloudTexture, frame: &CloudFrame) -> (i32, i32, f32, f32) {
    let offset =
        (frame.game_time % (i64::from(texture.width) * TICKS_PER_CELL)) as f32 + frame.partial_tick;
    let mut cloud_x = frame.camera.x + f64::from(offset * DRIFT);
    let mut cloud_z = frame.camera.z + f64::from(Z_BIAS);
    let tex_w = f64::from(texture.width) * f64::from(CELL_SIZE);
    let tex_h = f64::from(texture.height) * f64::from(CELL_SIZE);
    cloud_x -= (cloud_x / tex_w).floor() * tex_w;
    cloud_z -= (cloud_z / tex_h).floor() * tex_h;
    let cell_x = (cloud_x / f64::from(CELL_SIZE)).floor() as i32;
    let cell_z = (cloud_z / f64::from(CELL_SIZE)).floor() as i32;
    let x_in_cell = (cloud_x - f64::from(cell_x) * f64::from(CELL_SIZE)) as f32;
    let z_in_cell = (cloud_z - f64::from(cell_z) * f64::from(CELL_SIZE)) as f32;
    (cell_x, cell_z, x_in_cell, z_in_cell)
}

/// The key of this frame's clouds; `None` when there are none.
pub fn key(texture: &CloudTexture, frame: &CloudFrame) -> Option<CloudKey> {
    if matches!(frame.status, CloudStatus::Off) || texture.width == 0 || frame.color[3] <= 0.0 {
        return None;
    }
    let (cell_x, cell_z, ..) = cell_of(texture, frame);
    Some(CloudKey {
        status: frame.status,
        bottom_y: frame.bottom_y.to_bits(),
        range_chunks: frame.range_chunks,
        cell: (cell_x, cell_z),
        inside: (
            f64::from(frame.bottom_y) <= frame.camera.y,
            f64::from(frame.bottom_y + CELL_HEIGHT) >= frame.camera.y,
        ),
    })
}

/// The quads of [`build`] for the camera at the cell's corner.
pub fn build_anchored(texture: &CloudTexture, frame: CloudFrame) -> Vec<WorldVertex> {
    build_with(texture, frame, true)
}

/// Where [`build_anchored`]'s mesh sits: the corner of the camera's cell.
pub fn anchor(texture: &CloudTexture, frame: &CloudFrame) -> glam::DVec3 {
    if texture.width == 0 {
        return glam::DVec3::ZERO;
    }
    let (.., x_in_cell, z_in_cell) = cell_of(texture, frame);
    glam::DVec3::new(
        frame.camera.x - f64::from(x_in_cell),
        0.0,
        frame.camera.z - f64::from(z_in_cell),
    )
}

fn build_with(texture: &CloudTexture, frame: CloudFrame, anchored: bool) -> Vec<WorldVertex> {
    if matches!(frame.status, CloudStatus::Off) || texture.width == 0 || frame.color[3] <= 0.0 {
        return Vec::new();
    }
    let radius_blocks = f32::from(frame.range_chunks) * 16.0;
    let radius_cells = (radius_blocks / CELL_SIZE).ceil() as i32;
    let (cell_x, cell_z, x_in_cell, z_in_cell) = cell_of(texture, &frame);
    let (x_in_cell, z_in_cell) = if anchored {
        (0.0, 0.0)
    } else {
        (x_in_cell, z_in_cell)
    };
    let relative_bottom = (f64::from(frame.bottom_y) - frame.camera.y) as f32;
    let shift = if anchored { frame.camera.y as f32 } else { 0.0 };
    let fancy = frame.status == CloudStatus::Fancy;
    let mut out = Vec::new();
    for ring in 0..=2 * radius_cells {
        for rel_x in -ring..=ring {
            let rel_z = ring - rel_x.abs();
            if rel_z < 0
                || rel_z > radius_cells
                || rel_x * rel_x + rel_z * rel_z > radius_cells * radius_cells
            {
                continue;
            }
            if rel_z != 0 {
                push_cell(
                    &mut out,
                    texture,
                    cell_x,
                    cell_z,
                    rel_x,
                    -rel_z,
                    x_in_cell,
                    z_in_cell,
                    relative_bottom,
                    shift,
                    fancy,
                );
            }
            push_cell(
                &mut out,
                texture,
                cell_x,
                cell_z,
                rel_x,
                rel_z,
                x_in_cell,
                z_in_cell,
                relative_bottom,
                shift,
                fancy,
            );
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn push_cell(
    out: &mut Vec<WorldVertex>,
    texture: &CloudTexture,
    cell_x: i32,
    cell_z: i32,
    rel_x: i32,
    rel_z: i32,
    x_in_cell: f32,
    z_in_cell: f32,
    bottom: f32,
    shift: f32,
    fancy: bool,
) {
    let color = texture.cell(cell_x + rel_x, cell_z + rel_z);
    if color == 0 {
        return;
    }
    let x0 = rel_x as f32 * CELL_SIZE - x_in_cell;
    let z0 = rel_z as f32 * CELL_SIZE - z_in_cell;
    let x1 = x0 + CELL_SIZE;
    let z1 = z0 + CELL_SIZE;
    let y0 = bottom + shift;
    let y1 = y0 + CELL_HEIGHT;
    if !fancy {
        push_face(
            out,
            [x0, y0, z0],
            [x1, y0, z1],
            Direction::Down,
            color,
            SHADE_TOP,
            false,
        );
        push_face(
            out,
            [x0, y0, z0],
            [x1, y0, z1],
            Direction::Up,
            color,
            SHADE_TOP,
            false,
        );
        return;
    }
    if bottom <= 0.0 {
        push_face(
            out,
            [x0, y1, z0],
            [x1, y1, z1],
            Direction::Up,
            color,
            SHADE_TOP,
            false,
        );
    }
    if bottom + CELL_HEIGHT >= 0.0 {
        push_face(
            out,
            [x0, y0, z0],
            [x1, y0, z1],
            Direction::Down,
            color,
            SHADE_BOTTOM,
            false,
        );
    }
    let north = texture.empty_at(cell_x + rel_x, cell_z + rel_z - 1);
    let south = texture.empty_at(cell_x + rel_x, cell_z + rel_z + 1);
    let west = texture.empty_at(cell_x + rel_x - 1, cell_z + rel_z);
    let east = texture.empty_at(cell_x + rel_x + 1, cell_z + rel_z);
    if north && rel_z > 0 {
        push_face(
            out,
            [x0, y0, z0],
            [x1, y1, z0],
            Direction::North,
            color,
            SHADE_NORTH_SOUTH,
            false,
        );
    }
    if south && rel_z < 0 {
        push_face(
            out,
            [x0, y0, z1],
            [x1, y1, z1],
            Direction::South,
            color,
            SHADE_NORTH_SOUTH,
            false,
        );
    }
    if west && rel_x > 0 {
        push_face(
            out,
            [x0, y0, z0],
            [x0, y1, z1],
            Direction::West,
            color,
            SHADE_EAST_WEST,
            false,
        );
    }
    if east && rel_x < 0 {
        push_face(
            out,
            [x1, y0, z0],
            [x1, y1, z1],
            Direction::East,
            color,
            SHADE_EAST_WEST,
            false,
        );
    }
    if rel_x.abs() <= 1 && rel_z.abs() <= 1 {
        for dir in Direction::ALL {
            let shade = match dir {
                Direction::Up => SHADE_TOP,
                Direction::Down => SHADE_BOTTOM,
                Direction::North | Direction::South => SHADE_NORTH_SOUTH,
                Direction::West | Direction::East => SHADE_EAST_WEST,
            };
            let (min, max) = match dir {
                Direction::Up => ([x0, y1, z0], [x1, y1, z1]),
                Direction::Down => ([x0, y0, z0], [x1, y0, z1]),
                Direction::North => ([x0, y0, z0], [x1, y1, z0]),
                Direction::South => ([x0, y0, z1], [x1, y1, z1]),
                Direction::West => ([x0, y0, z0], [x0, y1, z1]),
                Direction::East => ([x1, y0, z0], [x1, y1, z1]),
            };
            push_face(out, min, max, dir, color, shade, true);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_face(
    out: &mut Vec<WorldVertex>,
    min: [f32; 3],
    max: [f32; 3],
    dir: Direction,
    cell: u32,
    shade: f32,
    inside: bool,
) {
    // The cell's colour and the face's shade only: the clouds' colour of
    // the moment (time of day, weather) comes per draw, as vanilla's
    // `CloudColor` uniform does, so the mesh outlives it.
    let [r, g, b, a] = crate::math::argb(cell);
    let color = [r * shade, g * shade, b * shade, a];
    let mut corners = match dir {
        Direction::Up => [
            [min[0], min[1], min[2]],
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [max[0], min[1], min[2]],
        ],
        Direction::Down => [
            [min[0], min[1], max[2]],
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [max[0], min[1], max[2]],
        ],
        Direction::North => [
            [max[0], min[1], min[2]],
            [min[0], min[1], min[2]],
            [min[0], max[1], min[2]],
            [max[0], max[1], min[2]],
        ],
        Direction::South => [
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [max[0], max[1], min[2]],
            [min[0], max[1], min[2]],
        ],
        Direction::West => [
            [min[0], min[1], min[2]],
            [min[0], min[1], max[2]],
            [min[0], max[1], max[2]],
            [min[0], max[1], min[2]],
        ],
        Direction::East => [
            [min[0], min[1], max[2]],
            [min[0], min[1], min[2]],
            [min[0], max[1], min[2]],
            [min[0], max[1], max[2]],
        ],
    };
    if inside {
        corners.reverse();
    }
    for pos in corners {
        out.push(WorldVertex::rgba(pos, [0.0, 0.0], color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_cell() -> CloudTexture {
        CloudTexture {
            cells: vec![0xCCFF_FFFF].into_boxed_slice(),
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn the_same_time_and_camera_rebuild_the_same_mesh() {
        let texture = solid_cell();
        let frame = CloudFrame {
            status: CloudStatus::Fast,
            color: [1.0, 1.0, 1.0, 0.8],
            bottom_y: 192.0,
            range_chunks: 2,
            camera: glam::DVec3::new(10.0, 64.0, 20.0),
            game_time: 1200,
            partial_tick: 0.3,
        };
        let a = build(&texture, frame);
        let b = build(&texture, frame);
        assert!(!a.is_empty());
        assert_eq!(a, b);
    }

    #[test]
    fn an_anchored_mesh_moved_to_its_anchor_is_the_camera_relative_one() {
        let texture = solid_cell();
        let frame = CloudFrame {
            status: CloudStatus::Fancy,
            color: [1.0, 1.0, 1.0, 0.8],
            bottom_y: 192.0,
            range_chunks: 2,
            camera: glam::DVec3::new(10.3, 64.0, 20.7),
            game_time: 1200,
            partial_tick: 0.3,
        };
        let relative = build(&texture, frame);
        let anchored = build_anchored(&texture, frame);
        let shift = (anchor(&texture, &frame) - frame.camera).as_vec3();
        assert_eq!(relative.len(), anchored.len());
        for (a, b) in relative.iter().zip(&anchored) {
            let moved = glam::Vec3::from_array(b.pos) + shift;
            assert!((moved - glam::Vec3::from_array(a.pos)).length() < 1e-3);
        }
        let nearby = CloudFrame {
            camera: frame.camera + glam::DVec3::new(0.01, 0.5, 0.0),
            ..frame
        };
        assert_eq!(key(&texture, &frame), key(&texture, &nearby));
    }

    #[test]
    fn the_clouds_colour_changes_neither_key_nor_mesh() {
        let texture = solid_cell();
        let noon = CloudFrame {
            status: CloudStatus::Fancy,
            color: [1.0, 1.0, 1.0, 0.8],
            bottom_y: 192.0,
            range_chunks: 2,
            camera: glam::DVec3::new(10.0, 64.0, 20.0),
            game_time: 1200,
            partial_tick: 0.3,
        };
        let dusk_in_rain = CloudFrame {
            color: [0.4, 0.3, 0.35, 0.6],
            ..noon
        };
        assert_eq!(key(&texture, &noon), key(&texture, &dusk_in_rain));
        assert_eq!(
            build_anchored(&texture, noon),
            build_anchored(&texture, dusk_in_rain)
        );
        // The cell's own colour (0xCCFFFFFF) and the top's shade stay.
        let top = build(&texture, noon);
        assert!(top.iter().any(|v| v.color == [255, 255, 255, 0xCC]));
    }

    #[test]
    fn clear_clouds_draw_nothing() {
        let frame = CloudFrame {
            status: CloudStatus::Fancy,
            color: [1.0, 1.0, 1.0, 0.0],
            bottom_y: 192.0,
            range_chunks: 2,
            camera: glam::DVec3::ZERO,
            game_time: 0,
            partial_tick: 0.0,
        };
        assert!(key(&solid_cell(), &frame).is_none());
        assert!(build(&solid_cell(), frame).is_empty());
    }

    #[test]
    fn off_draws_nothing() {
        let frame = CloudFrame {
            status: CloudStatus::Off,
            color: [1.0; 4],
            bottom_y: 192.0,
            range_chunks: 8,
            camera: glam::DVec3::ZERO,
            game_time: 0,
            partial_tick: 0.0,
        };
        assert!(build(&solid_cell(), frame).is_empty());
    }

    fn shade_of(v: &WorldVertex) -> f32 {
        f32::from(v.color[0]) / 255.0
    }

    fn fancy_at(camera_y: f32) -> Vec<WorldVertex> {
        build(
            &solid_cell(),
            CloudFrame {
                status: CloudStatus::Fancy,
                color: [1.0, 1.0, 1.0, 1.0],
                bottom_y: 192.0,
                range_chunks: 2,
                camera: glam::DVec3::new(6.0, f64::from(camera_y), f64::from(6.0 - Z_BIAS)),
                game_time: 0,
                partial_tick: 0.0,
            },
        )
    }

    fn quad_normal(q: &[WorldVertex]) -> glam::Vec3 {
        let a = glam::Vec3::from_array(q[0].pos);
        let b = glam::Vec3::from_array(q[1].pos);
        let c = glam::Vec3::from_array(q[2].pos);
        (b - a).cross(c - a).normalize_or_zero()
    }

    #[test]
    fn from_above_the_top_wins_over_the_underside() {
        let mesh = fancy_at(220.0);
        let bottom_y = 192.0 - 220.0;
        let top_y = bottom_y + CELL_HEIGHT;
        let far: Vec<_> = mesh
            .iter()
            .filter(|v| v.pos[0].abs() > CELL_SIZE * 2.0 || v.pos[2].abs() > CELL_SIZE * 2.0)
            .collect();
        assert!(far.iter().any(|v| (v.pos[1] - top_y).abs() < 1e-4));
        assert!(far.iter().all(|v| (v.pos[1] - bottom_y).abs() > 1e-4));
        assert!(far.iter().any(|v| (shade_of(v) - SHADE_TOP).abs() < 0.02));
        assert!(
            far.iter()
                .all(|v| (shade_of(v) - SHADE_BOTTOM).abs() > 0.02)
        );
        for q in mesh.chunks(4).filter(|q| {
            q.iter().all(|v| {
                (v.pos[1] - top_y).abs() < 1e-4
                    && (v.pos[0].abs() > CELL_SIZE * 2.0 || v.pos[2].abs() > CELL_SIZE * 2.0)
            })
        }) {
            assert!(quad_normal(q).y > 0.5, "far top faces the sky");
        }
    }

    #[test]
    fn from_below_the_underside_wins_over_the_top() {
        let mesh = fancy_at(64.0);
        let bottom_y = 192.0 - 64.0;
        let top_y = bottom_y + CELL_HEIGHT;
        let far: Vec<_> = mesh
            .iter()
            .filter(|v| v.pos[0].abs() > CELL_SIZE * 2.0 || v.pos[2].abs() > CELL_SIZE * 2.0)
            .collect();
        assert!(far.iter().any(|v| (v.pos[1] - bottom_y).abs() < 1e-4));
        assert!(far.iter().all(|v| (v.pos[1] - top_y).abs() > 1e-4));
        assert!(
            far.iter()
                .any(|v| (shade_of(v) - SHADE_BOTTOM).abs() < 0.02)
        );
        assert!(far.iter().all(|v| (shade_of(v) - SHADE_TOP).abs() > 0.02));
        for q in mesh.chunks(4).filter(|q| {
            q.iter().all(|v| {
                (v.pos[1] - bottom_y).abs() < 1e-4
                    && (v.pos[0].abs() > CELL_SIZE * 2.0 || v.pos[2].abs() > CELL_SIZE * 2.0)
            })
        }) {
            assert!(quad_normal(q).y < -0.5, "far underside faces the ground");
        }
    }

    #[test]
    fn interior_faces_reverse_winding() {
        let mesh = fancy_at(194.0);
        let top_y = (192.0 + CELL_HEIGHT) - 194.0;
        let origin: Vec<_> = mesh
            .chunks(4)
            .filter(|q| {
                q.iter().all(|v| {
                    (v.pos[1] - top_y).abs() < 1e-4
                        && v.pos[0] >= -6.0 - 1e-4
                        && v.pos[0] <= 6.0 + 1e-4
                        && v.pos[2] >= -6.0 - 1e-4
                        && v.pos[2] <= 6.0 + 1e-4
                })
            })
            .collect();
        assert_eq!(origin.len(), 2);
        let order = |q: &[WorldVertex]| q.iter().map(|v| (v.pos[0], v.pos[2])).collect::<Vec<_>>();
        let mut rev = order(origin[0]);
        rev.reverse();
        assert_eq!(rev, order(origin[1]));
    }

    #[test]
    fn options_round_trip() {
        for status in CloudStatus::ALL {
            assert_eq!(
                CloudStatus::from_options(status.options_id()),
                Some(*status)
            );
        }
        assert_eq!(
            CloudStatus::from_options("\"true\""),
            Some(CloudStatus::Fancy)
        );
        assert_eq!(CloudStatus::from_options("off"), Some(CloudStatus::Off));
    }
}
