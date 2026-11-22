//! `FluidRenderer.tesselate`: the faces of one fluid block.

use std::sync::LazyLock;

use pumpkin_data::BlockStateId;

use crate::renderer::WorldVertex as Vertex;
use crate::world::block::direction::Direction;
use crate::world::block::{ao_flags, skip_rendering, state_flags};
use crate::world::geometry::{
    self, SHADE_DOWN, SHADE_EAST_WEST, SHADE_NORTH_SOUTH, SHADE_UP, atlas_uv,
};
use crate::world::terrain::Neighborhood;

use super::state::{FluidKind, FluidState, fluid_of};
use super::{FluidSprites, MAX_FLUID_HEIGHT, OFFSET};
use crate::math::angle::{self, HALF_PI};

/// What the fluid mesher looks up around one block.
struct Around<'a> {
    neighborhood: &'a Neighborhood<'a>,
    section: usize,
    pos: [i32; 3],
}

impl Around<'_> {
    fn state(&self, dx: i32, dy: i32, dz: i32) -> u16 {
        let [x, y, z] = self.pos;
        self.neighborhood
            .block(self.section, x + dx, y + dy, z + dz)
            .unwrap_or(0)
    }

    /// `LightCoordsUtil.getLightCoords` there.
    fn light_coords(&self, dx: i32, dy: i32, dz: i32) -> [u8; 2] {
        let flags = ao_flags::of(self.state(dx, dy, dz));
        if flags.emissive_rendering() {
            return [15, 15];
        }
        let [x, y, z] = self.pos;
        let (block, sky) = self
            .neighborhood
            .light(self.section, x + dx, y + dy, z + dz);
        [block.max(flags.emission()), sky]
    }

    /// `FluidRenderer.getLightCoords`: the brighter of the block at `dy`
    fn fluid_light(&self, dy: i32) -> [u8; 2] {
        let [b0, s0] = self.light_coords(0, dy, 0);
        let [b1, s1] = self.light_coords(0, dy + 1, 0);
        [b0.max(b1), s0.max(s1)]
    }

    fn fluid(&self, dx: i32, dy: i32, dz: i32) -> Option<FluidState> {
        fluid_of(self.state(dx, dy, dz))
    }

    /// `FluidRenderer.getHeight`.
    fn height(&self, kind: FluidKind, dx: i32, dz: i32) -> f32 {
        match self.fluid(dx, 0, dz) {
            Some(fluid) if fluid.kind == kind => {
                if self
                    .fluid(dx, 1, dz)
                    .is_some_and(|above| above.kind == kind)
                {
                    1.0
                } else {
                    fluid.own_height()
                }
            }
            _ => {
                let state = self.state(dx, 0, dz);
                let solid = BlockStateId::new(state).is_some_and(|s| s.to_state().is_solid());
                if solid { -1.0 } else { 0.0 }
            }
        }
    }

    /// `FluidRenderer.calculateAverageHeight` for the corner between the neighbours.
    fn corner_height(&self, kind: FluidKind, own: f32, ax: i32, bz: i32) -> f32 {
        let (h1, h2) = (self.height(kind, ax, 0), self.height(kind, 0, bz));
        if h1 >= 1.0 || h2 >= 1.0 {
            return 1.0;
        }
        let mut weighted = [0.0f32; 2];
        let mut add = |height: f32| {
            if height >= 0.8 {
                weighted[0] += height * 10.0;
                weighted[1] += 10.0;
            } else if height >= 0.0 {
                weighted[0] += height;
                weighted[1] += 1.0;
            }
        };
        if h1 > 0.0 || h2 > 0.0 {
            let corner = self.height(kind, ax, bz);
            if corner >= 1.0 {
                return 1.0;
            }
            add(corner);
        }
        add(own);
        add(h1);
        add(h2);
        weighted[0] / weighted[1]
    }

    /// `FlowingFluid.getFlow` (without the falling push down).
    fn flow(&self, fluid: FluidState) -> (f32, f32) {
        let own = fluid.own_height();
        let (mut fx, mut fz) = (0.0f32, 0.0f32);
        for direction in Direction::HORIZONTAL {
            let [dx, _, dz] = direction.step();
            let neighbor = self.fluid(dx, 0, dz);
            if neighbor.is_some_and(|n| n.kind != fluid.kind) {
                continue;
            }
            let height = neighbor.map_or(0.0, FluidState::own_height);
            let diff = if height == 0.0 {
                let open = !state_flags::is_solid(self.state(dx, 0, dz));
                match self.fluid(dx, -1, dz) {
                    Some(below) if open && below.kind == fluid.kind && below.own_height() > 0.0 => {
                        own - (below.own_height() - MAX_FLUID_HEIGHT)
                    }
                    _ => 0.0,
                }
            } else {
                own - height
            };
            fx += dx as f32 * diff;
            fz += dz as f32 * diff;
        }
        let length = (fx * fx + fz * fz).sqrt();
        if length < 1e-6 {
            (0.0, 0.0)
        } else {
            (fx / length, fz / length)
        }
    }
}

/// `FluidRenderer.isFaceOccludedByState`.
fn is_face_occluded_by_state(direction: Direction, height: f32, state: u16) -> bool {
    let Some(id) = BlockStateId::new(state) else {
        return false;
    };
    let s = id.to_state();
    if !s.can_occlude() {
        return false;
    }
    if s.is_solid_render() || s.is_full_cube() {
        return direction != Direction::Up || height >= 1.0;
    }
    false
}

fn is_face_occluded_by_neighbor(direction: Direction, height: f32, neighbor: u16) -> bool {
    is_face_occluded_by_state(direction, height, neighbor)
}

fn is_face_occluded_by_self(state: u16, direction: Direction) -> bool {
    is_face_occluded_by_state(direction.opposite(), 1.0, state)
}

/// `FluidRenderer.shouldRenderFace`.
fn should_render_face(
    fluid: FluidState,
    block: u16,
    direction: Direction,
    neighbor_fluid: Option<FluidState>,
) -> bool {
    let same = neighbor_fluid.is_some_and(|n| n.kind == fluid.kind);
    !same && !is_face_occluded_by_self(block, direction)
}

/// `FluidState.shouldRenderBackwardUpFace`.
fn should_render_backward_up_face(around: &Around, kind: FluidKind) -> bool {
    (-1..=1).any(|dx| {
        (-1..=1).any(|dz| {
            let above = around.state(dx, 1, dz);
            let same = fluid_of(above).is_some_and(|f| f.kind == kind);
            !same && !state_flags::is_solid(above)
        })
    })
}

/// Per block state: whether its sides get `water_overlay`
static OVERLAY: LazyLock<Vec<bool>> = LazyLock::new(|| {
    (0..BlockStateId::COUNT)
        .map(|id| {
            BlockStateId::new(id).is_some_and(|id| {
                let name = pumpkin_data::Block::from_state_id(id).name;
                skip_rendering::half_transparent(name) || name.ends_with("leaves")
            })
        })
        .collect()
});

fn shows_overlay(state: u16) -> bool {
    OVERLAY.get(usize::from(state)).copied().unwrap_or(false)
}

/// `FluidRenderer.addFace`: a quad, and if `both_sides` its back too.
fn add_face(
    out: &mut Vec<Vertex>,
    corners: [([f32; 3], [f32; 2]); 4],
    color: [f32; 3],
    both_sides: bool,
    light: [u8; 2],
) {
    geometry::push_quad_lit(out, corners, color, false, light);
    if both_sides {
        geometry::push_quad_lit(out, corners, color, true, light);
    }
}

fn scaled(color: [f32; 3], shade: f32) -> [f32; 3] {
    color.map(|c| c * shade)
}

/// `FluidRenderer.tesselate` for the fluid in section `section`'s block.
#[allow(clippy::too_many_arguments)]
pub fn mesh(
    neighborhood: &Neighborhood,
    section: usize,
    pos: [i32; 3],
    origin: [f32; 3],
    state: u16,
    fluid: FluidState,
    sprites: &FluidSprites,
    cardinal: &[f32; 6],
    water_tint: [f32; 3],
    solid: &mut Vec<Vertex>,
    translucent: &mut Vec<Vertex>,
) {
    let around = Around {
        neighborhood,
        section,
        pos,
    };
    let kind = fluid.kind;
    let same = |f: Option<FluidState>| f.is_some_and(|f| f.kind == kind);
    let render_up = !same(around.fluid(0, 1, 0));
    let render_down = should_render_face(fluid, state, Direction::Down, around.fluid(0, -1, 0))
        && !is_face_occluded_by_neighbor(Direction::Down, MAX_FLUID_HEIGHT, around.state(0, -1, 0));
    let render_north = should_render_face(fluid, state, Direction::North, around.fluid(0, 0, -1));
    let render_south = should_render_face(fluid, state, Direction::South, around.fluid(0, 0, 1));
    let render_west = should_render_face(fluid, state, Direction::West, around.fluid(-1, 0, 0));
    let render_east = should_render_face(fluid, state, Direction::East, around.fluid(1, 0, 0));
    if !(render_up || render_down || render_north || render_south || render_west || render_east) {
        return;
    }

    let (out, tint, still, flowing, overlay) = match kind {
        FluidKind::Water => (
            translucent,
            water_tint,
            sprites.water_still,
            sprites.water_flow,
            Some(sprites.water_overlay),
        ),
        FluidKind::Lava => (solid, [1.0; 3], sprites.lava_still, sprites.lava_flow, None),
    };

    let light = around.fluid_light(0);

    let own = around.height(kind, 0, 0);
    let [mut nw, mut sw, mut se, mut ne] = if own >= 1.0 {
        [1.0; 4]
    } else {
        [
            around.corner_height(kind, own, -1, -1),
            around.corner_height(kind, own, -1, 1),
            around.corner_height(kind, own, 1, 1),
            around.corner_height(kind, own, 1, -1),
        ]
    };
    let [x, y, z] = origin;
    let bottom = if render_down { OFFSET } else { 0.0 };

    let lowest = nw.min(sw).min(se.min(ne));
    if render_up && !is_face_occluded_by_neighbor(Direction::Up, lowest, around.state(0, 1, 0)) {
        for h in [&mut nw, &mut sw, &mut se, &mut ne] {
            *h -= OFFSET;
        }
        let (fx, fz) = around.flow(fluid);
        let uvs = if fx == 0.0 && fz == 0.0 {
            [
                atlas_uv(still, 0.0, 0.0),
                atlas_uv(still, 0.0, 1.0),
                atlas_uv(still, 1.0, 1.0),
                atlas_uv(still, 1.0, 0.0),
            ]
        } else {
            let angle = fz.atan2(fx) - HALF_PI;
            let (s, c) = (angle::sin(angle) * 0.25, angle::cos(angle) * 0.25);
            [
                atlas_uv(flowing, 0.5 + (-c - s), 0.5 + (-c + s)),
                atlas_uv(flowing, 0.5 + (-c + s), 0.5 + (c + s)),
                atlas_uv(flowing, 0.5 + (c + s), 0.5 + (c - s)),
                atlas_uv(flowing, 0.5 + (c - s), 0.5 + (-c - s)),
            ]
        };
        add_face(
            out,
            [
                ([x, y + nw, z], uvs[0]),
                ([x, y + sw, z + 1.0], uvs[1]),
                ([x + 1.0, y + se, z + 1.0], uvs[2]),
                ([x + 1.0, y + ne, z], uvs[3]),
            ],
            scaled(tint, SHADE_UP * cardinal[1]),
            should_render_backward_up_face(&around, kind),
            light,
        );
    }

    if render_down {
        add_face(
            out,
            [
                ([x, y + bottom, z], atlas_uv(still, 0.0, 0.0)),
                ([x + 1.0, y + bottom, z], atlas_uv(still, 1.0, 0.0)),
                ([x + 1.0, y + bottom, z + 1.0], atlas_uv(still, 1.0, 1.0)),
                ([x, y + bottom, z + 1.0], atlas_uv(still, 0.0, 1.0)),
            ],
            scaled(tint, SHADE_DOWN * cardinal[0]),
            false,
            around.fluid_light(-1),
        );
    }

    let sides = [
        (
            Direction::North,
            render_north,
            nw,
            ne,
            [x, z + OFFSET],
            [x + 1.0, z + OFFSET],
            (0, -1),
        ),
        (
            Direction::South,
            render_south,
            se,
            sw,
            [x + 1.0, z + 1.0 - OFFSET],
            [x, z + 1.0 - OFFSET],
            (0, 1),
        ),
        (
            Direction::West,
            render_west,
            sw,
            nw,
            [x + OFFSET, z + 1.0],
            [x + OFFSET, z],
            (-1, 0),
        ),
        (
            Direction::East,
            render_east,
            ne,
            se,
            [x + 1.0 - OFFSET, z],
            [x + 1.0 - OFFSET, z + 1.0],
            (1, 0),
        ),
    ];
    for (direction, render, h0, h1, [x0, z0], [x1, z1], (dx, dz)) in sides {
        let neighbor = around.state(dx, 0, dz);
        if !render || is_face_occluded_by_neighbor(direction, h0.max(h1), neighbor) {
            continue;
        }
        let (sprite, is_overlay) = match overlay {
            Some(overlay) if shows_overlay(neighbor) => (overlay, true),
            _ => (flowing, false),
        };
        let shade = if direction.axis() == 2 {
            SHADE_NORTH_SOUTH * cardinal[2]
        } else {
            SHADE_EAST_WEST * cardinal[4]
        };
        add_face(
            out,
            [
                ([x0, y + h0, z0], atlas_uv(sprite, 0.0, (1.0 - h0) * 0.5)),
                ([x1, y + h1, z1], atlas_uv(sprite, 0.5, (1.0 - h1) * 0.5)),
                ([x1, y + bottom, z1], atlas_uv(sprite, 0.5, 0.5)),
                ([x0, y + bottom, z0], atlas_uv(sprite, 0.0, 0.5)),
            ],
            scaled(tint, SHADE_UP * shade),
            !is_overlay,
            light,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{Chunk, Section};
    use pumpkin_data::Block;

    fn sprites() -> FluidSprites {
        let r = [0.0, 0.0, 1.0, 1.0];
        FluidSprites {
            water_still: r,
            water_flow: r,
            water_overlay: r,
            lava_still: r,
            lava_flow: r,
        }
    }

    fn chunk_with(blocks: &[(i32, i32, i32, u16)]) -> Chunk {
        let mut sections: Vec<Section> = (0..2).map(|_| Section::empty()).collect();
        for &(x, y, z, id) in blocks {
            sections[(y / 16) as usize].set(
                Section::index(x as usize, (y % 16) as usize, z as usize),
                id,
            );
        }
        Chunk::new(0, 0, sections)
    }

    fn mesh_water(blocks: &[(i32, i32, i32, u16)]) -> Vec<Vertex> {
        let chunk = chunk_with(blocks);
        let neighborhood = Neighborhood::alone(&chunk);
        let water = Block::WATER.default_state.id.as_u16();
        let fluid = fluid_of(water).unwrap();
        let mut translucent = Vec::new();
        let mut solid = Vec::new();
        mesh(
            &neighborhood,
            0,
            [4, 4, 4],
            [4.0, 4.0, 4.0],
            water,
            fluid,
            &sprites(),
            &[1.0; 6],
            crate::world::fluid::WATER_TINT,
            &mut solid,
            &mut translucent,
        );
        translucent
    }

    /// Faces of the water at (4,4,4) that sit at its surface.
    fn surface_quads(vertices: &[Vertex]) -> usize {
        vertices
            .chunks(4)
            .filter(|q| {
                q.iter().all(|v| {
                    v.pos[1] > 4.5
                        && (4.0..=5.0).contains(&v.pos[0])
                        && (4.0..=5.0).contains(&v.pos[2])
                })
            })
            .count()
    }

    #[test]
    fn ice_and_glass_do_not_hide_the_underside_of_water() {
        let water = Block::WATER.default_state.id.as_u16();
        let ice = Block::ICE.default_state.id.as_u16();
        let glass = Block::GLASS.default_state.id.as_u16();
        let dirt = Block::DIRT.default_state.id.as_u16();
        assert_eq!(surface_quads(&mesh_water(&[(4, 4, 4, water)])), 2);
        let cover = |block: u16| {
            let mut blocks = vec![(4, 4, 4, water)];
            for x in 3..6 {
                for z in 3..6 {
                    blocks.push((x, 5, z, block));
                }
            }
            surface_quads(&mesh_water(&blocks))
        };
        assert_eq!(cover(ice), 2);
        assert_eq!(cover(glass), 2);
        assert_eq!(cover(dirt), 1);
        assert_eq!(
            surface_quads(&mesh_water(&[(4, 4, 4, water), (4, 5, 4, dirt)])),
            2
        );
    }

    #[test]
    fn the_water_surface_stays_inside_its_sprite() {
        let vertices = mesh_water(&[(4, 4, 4, Block::WATER.default_state.id.as_u16())]);
        let inset = crate::world::geometry::UV_INSET;
        for v in &vertices {
            assert!(
                v.uv[0] >= inset
                    && v.uv[0] <= 1.0 - inset
                    && v.uv[1] >= inset
                    && v.uv[1] <= 1.0 - inset,
                "water UV {:?} sits on the sprite edge",
                v.uv
            );
        }
    }

    #[test]
    fn ice_does_not_occlude_the_water_top() {
        let water = Block::WATER.default_state.id.as_u16();
        let ice = Block::ICE.default_state.id.as_u16();
        let vertices = mesh_water(&[(4, 4, 4, water), (4, 5, 4, ice)]);
        assert!(
            surface_quads(&vertices) >= 2,
            "water under ice must keep its surface, got {}",
            surface_quads(&vertices)
        );
        assert!(!is_face_occluded_by_neighbor(Direction::Up, 8.0 / 9.0, ice));
        assert!(state_flags::is_solid(Block::DIRT.default_state.id.as_u16()));
        assert!(!state_flags::is_solid(ice));
        assert!(!state_flags::is_solid(
            Block::GLASS.default_state.id.as_u16()
        ));
    }

    #[test]
    fn the_nether_shades_a_water_surface_by_its_own_table() {
        let chunk = chunk_with(&[]);
        let neighborhood = Neighborhood::alone(&chunk);
        let water = Block::WATER.default_state.id.as_u16();
        let fluid = fluid_of(water).unwrap();
        let top_shade = |cardinal: &[f32; 6]| {
            let (mut translucent, mut solid) = (Vec::new(), Vec::new());
            mesh(
                &neighborhood,
                0,
                [4, 4, 4],
                [4.0, 4.0, 4.0],
                water,
                fluid,
                &sprites(),
                cardinal,
                crate::world::fluid::WATER_TINT,
                &mut solid,
                &mut translucent,
            );
            translucent
                .iter()
                .map(|v| v.color[0])
                .max()
                .unwrap_or_default()
        };
        let default = top_shade(&[1.0; 6]);
        let nether = top_shade(&crate::world::dimension::CardinalLight::Nether.relative());
        assert!(nether < default);
    }
}
