//! Vanilla `ClipContext` against block outline shapes, and the entity boxes

use glam::DVec3;
use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::COLLISION_SHAPES;

use crate::world::level::Level;
use crate::world::physics::Aabb;

/// `Attributes.BLOCK_INTERACTION_RANGE`.
pub const SURVIVAL_RANGE: f64 = 4.5;
pub const CREATIVE_RANGE: f64 = 5.0;
/// `Attributes.ENTITY_INTERACTION_RANGE`, creative adding
pub const SURVIVAL_ENTITY_RANGE: f64 = 3.0;
pub const CREATIVE_ENTITY_RANGE: f64 = 5.0;

/// `Direction` on the wire: down, up, north, south, west, east.
pub const FACE_DOWN: u8 = 0;
pub const FACE_UP: u8 = 1;
pub const FACE_NORTH: u8 = 2;
pub const FACE_SOUTH: u8 = 3;
pub const FACE_WEST: u8 = 4;
pub const FACE_EAST: u8 = 5;

/// `BlockHitResult` for a block.
#[derive(Clone, Copy, Debug)]
pub struct BlockHit {
    pub pos: [i32; 3],
    pub face: u8,
    /// World-space hit point (`BlockHitResult.getLocation`).
    pub location: DVec3,
    /// The eye started inside this block.
    pub inside: bool,
}

/// `EntityHitResult`: the entity and where the ray entered its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityHit {
    pub id: i32,
    pub location: DVec3,
}

/// `HitResult`, a miss being `None`.
#[derive(Clone, Copy, Debug)]
pub enum Hit {
    Block(BlockHit),
    Entity(EntityHit),
}

/// `LocalPlayer.pick`: the block or the entity the ray from `eye` along
pub fn pick(
    level: &Level,
    eye: DVec3,
    dir: DVec3,
    block_range: f64,
    entity_range: f64,
    boxes: impl IntoIterator<Item = (i32, Aabb)>,
) -> Option<Hit> {
    let reach = block_range.max(entity_range);
    let block = clip_blocks(level, eye, eye + dir * reach);
    let block_distance_sq = block.map_or(reach * reach, |hit| hit.location.distance_squared(eye));
    let end = eye + dir * block_distance_sq.sqrt();
    match clip_entities(eye, end, boxes, block_distance_sq) {
        Some(entity) if entity.location.distance_squared(eye) < block_distance_sq => {
            (entity.location.distance_squared(eye) < entity_range * entity_range)
                .then_some(Hit::Entity(entity))
        }
        _ => block
            .filter(|hit| hit.location.distance_squared(eye) < block_range * block_range)
            .map(Hit::Block),
    }
}

/// `ProjectileUtil.getEntityHitResult`: the nearest box the segment from
pub fn clip_entities(
    start: DVec3,
    end: DVec3,
    boxes: impl IntoIterator<Item = (i32, Aabb)>,
    max_distance_sq: f64,
) -> Option<EntityHit> {
    let delta = end - start;
    let length = delta.length();
    if length < 1.0e-7 {
        return None;
    }
    let dir = delta / length;
    let mut nearest = max_distance_sq;
    let mut hovered = None;
    for (id, aabb) in boxes {
        let closest = start.clamp(aabb.min, aabb.max);
        if closest.distance_squared(start) >= nearest {
            continue;
        }
        let Some((t, _, inside)) = clip_aabb(&aabb, start, dir, length) else {
            continue;
        };
        if inside {
            nearest = 0.0;
            hovered = Some(EntityHit {
                id,
                location: start,
            });
            continue;
        }
        let location = start + dir * t;
        let distance_sq = start.distance_squared(location);
        if distance_sq < nearest {
            nearest = distance_sq;
            hovered = Some(EntityHit { id, location });
        }
    }
    hovered
}

impl BlockHit {
    /// The neighbouring cell `face` points into.
    pub fn neighbour(&self) -> [i32; 3] {
        let [x, y, z] = self.pos;
        match self.face {
            FACE_DOWN => [x, y - 1, z],
            FACE_UP => [x, y + 1, z],
            FACE_NORTH => [x, y, z - 1],
            FACE_SOUTH => [x, y, z + 1],
            FACE_WEST => [x - 1, y, z],
            FACE_EAST => [x + 1, y, z],
            _ => self.pos,
        }
    }

    /// Hit point relative to the block, for `USE_ITEM_ON`'s cursor.
    pub fn cursor(&self) -> [f32; 3] {
        [
            (self.location.x - f64::from(self.pos[0])) as f32,
            (self.location.y - f64::from(self.pos[1])) as f32,
            (self.location.z - f64::from(self.pos[2])) as f32,
        ]
    }
}

/// `BlockGetter.clip` with `ClipContext.Block.OUTLINE` and no fluid clip.
pub fn clip_blocks(level: &Level, start: DVec3, end: DVec3) -> Option<BlockHit> {
    clip_blocks_where(level, start, end, |_| true, false)
}

/// `BlockGetter.clip` with `ClipContext.Fluid.ANY`: a fluid's own shape
/// (`FluidState.getShape`) stops the ray as well as the block's outline.
pub fn clip_blocks_and_fluids(level: &Level, start: DVec3, end: DVec3) -> Option<BlockHit> {
    clip_blocks_where(level, start, end, |_| true, true)
}

/// `FluidState.getShape`: as high as the fluid, a whole block with the
/// same fluid above.
fn fluid_shape(level: &Level, (x, y, z): (i32, i32, i32), id: u16) -> Option<Aabb> {
    let fluid = crate::world::fluid::fluid_of(id)?;
    let above = level.block_state(x, y + 1, z).unwrap_or(0);
    let same_above = crate::world::fluid::fluid_of(above).is_some_and(|f| f.kind == fluid.kind);
    let height = if same_above {
        1.0
    } else {
        f64::from(fluid.own_height())
    };
    let origin = DVec3::new(f64::from(x), f64::from(y), f64::from(z));
    Some(Aabb {
        min: origin,
        max: origin + DVec3::new(1.0, height, 1.0),
    })
}

/// `Level.clip` with `ClipContext.Block.COLLIDER` and no fluid: where a
/// projectile's move from `start` to `end` first meets a collision box.
pub fn clip_colliders(level: &Level, start: DVec3, end: DVec3) -> Option<DVec3> {
    let delta = end - start;
    let length = delta.length();
    if length < 1.0e-7 {
        return None;
    }
    let dir = delta / length;
    let region = Aabb {
        min: start.min(end),
        max: start.max(end),
    }
    .deflate(-1.0e-7);
    crate::world::physics::block_boxes(level, &region)
        .iter()
        .filter_map(|aabb| clip_aabb(aabb, start, dir, length).map(|(t, _, _)| t))
        .min_by(f64::total_cmp)
        .map(|t| start + dir * t)
}

/// Sight for name tags. Vanilla writes framebuffer depth from drawn
pub fn clip_blocks_for_nametag(level: &Level, start: DVec3, end: DVec3) -> Option<BlockHit> {
    clip_blocks_where(level, start, end, occludes_name_tag, false)
}

fn occludes_name_tag(id: u16) -> bool {
    BlockStateId::new(id).is_none_or(|state| {
        !matches!(
            state.to_block().name,
            "barrier" | "light" | "structure_void"
        )
    })
}

fn clip_blocks_where(
    level: &Level,
    start: DVec3,
    end: DVec3,
    keep: impl Fn(u16) -> bool,
    fluids: bool,
) -> Option<BlockHit> {
    let delta = end - start;
    let length = delta.length();
    if length < 1.0e-7 {
        return None;
    }
    let dir = delta / length;
    let mut cell = start.floor().as_ivec3();
    let step = dir.signum().as_ivec3();
    let t_delta = DVec3::new(axis_delta(dir.x), axis_delta(dir.y), axis_delta(dir.z));
    let next_boundary = |cell: i32, start: f64, dir: f64| {
        if dir > 0.0 {
            (f64::from(cell) + 1.0 - start) / dir
        } else if dir < 0.0 {
            (f64::from(cell) - start) / dir
        } else {
            f64::INFINITY
        }
    };
    let mut t_max = DVec3::new(
        next_boundary(cell.x, start.x, dir.x),
        next_boundary(cell.y, start.y, dir.y),
        next_boundary(cell.z, start.z, dir.z),
    );
    loop {
        let mut best: Option<(f64, BlockHit)> = None;
        let id = level.block_state(cell.x, cell.y, cell.z).unwrap_or(0);
        if keep(id) {
            let fluid = fluids
                .then(|| fluid_shape(level, (cell.x, cell.y, cell.z), id))
                .flatten();
            for aabb in outline_shapes(level, (cell.x, cell.y, cell.z)).chain(fluid) {
                if let Some((hit_t, face, inside)) = clip_aabb(&aabb, start, dir, length)
                    && best.is_none_or(|(best_t, _)| hit_t < best_t)
                {
                    best = Some((
                        hit_t,
                        BlockHit {
                            pos: cell.to_array(),
                            face,
                            location: start + dir * hit_t,
                            inside,
                        },
                    ));
                }
            }
        }
        if let Some((_, hit)) = best {
            return Some(hit);
        }
        let t = t_max.min_element();
        if t > length {
            return None;
        }
        if t_max.x == t {
            cell.x += step.x;
            t_max.x += t_delta.x;
        } else if t_max.y == t {
            cell.y += step.y;
            t_max.y += t_delta.y;
        } else {
            cell.z += step.z;
            t_max.z += t_delta.z;
        }
    }
}

/// How far along the ray one whole cell is on an axis.
fn axis_delta(dir: f64) -> f64 {
    if dir == 0.0 {
        f64::INFINITY
    } else {
        1.0 / dir.abs()
    }
}

/// The boxes of the block at `(x, y, z)` that the crosshair hits and the
pub fn outline_shapes(level: &Level, (x, y, z): (i32, i32, i32)) -> impl Iterator<Item = Aabb> {
    let origin = DVec3::new(f64::from(x), f64::from(y), f64::from(z));
    let id = level.block_state(x, y, z).unwrap_or(0);
    let origin = origin + crate::world::block::offset::shape_delta(id, x, z);
    BlockStateId::new(id)
        .filter(|id| id.as_u16() != 0)
        .into_iter()
        .flat_map(|state| state.to_state().outline_shapes.iter())
        .map(move |&shape| {
            let shape = COLLISION_SHAPES[usize::from(shape)];
            Aabb {
                min: origin + DVec3::new(shape.min.x, shape.min.y, shape.min.z),
                max: origin + DVec3::new(shape.max.x, shape.max.y, shape.max.z),
            }
        })
}

/// Ray from `start` along unit `dir`, up to `max_t`. The enter time and
fn clip_aabb(aabb: &Aabb, start: DVec3, dir: DVec3, max_t: f64) -> Option<(f64, u8, bool)> {
    let mut t_enter = f64::NEG_INFINITY;
    let mut t_exit = f64::INFINITY;
    let mut enter_face = FACE_NORTH;
    for axis in 0..3 {
        let min = aabb.min[axis];
        let max = aabb.max[axis];
        let s = start[axis];
        let d = dir[axis];
        if d.abs() < 1.0e-8 {
            if s < min || s > max {
                return None;
            }
            continue;
        }
        let mut t1 = (min - s) / d;
        let mut t2 = (max - s) / d;
        let mut face = match axis {
            0 => FACE_WEST,
            1 => FACE_DOWN,
            _ => FACE_NORTH,
        };
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            face = match axis {
                0 => FACE_EAST,
                1 => FACE_UP,
                _ => FACE_SOUTH,
            };
        }
        if t1 > t_enter {
            t_enter = t1;
            enter_face = face;
        }
        t_exit = t_exit.min(t2);
        if t_enter > t_exit {
            return None;
        }
    }
    let inside = t_enter < 0.0 && t_exit > 0.0;
    let t = if inside { 0.0 } else { t_enter };
    if t >= 0.0 && t <= max_t && t_exit >= 0.0 {
        Some((t, enter_face, inside))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_the_front_of_a_unit_cube() {
        let cube = Aabb {
            min: DVec3::ZERO,
            max: DVec3::ONE,
        };
        let (t, face, inside) =
            clip_aabb(&cube, DVec3::new(0.5, 0.5, -1.0), DVec3::Z, 4.0).unwrap();
        assert!(!inside);
        assert_eq!(face, FACE_NORTH);
        assert!((t - 1.0).abs() < 1e-9);
    }

    #[test]
    fn grazing_a_corner_hits_the_block() {
        use crate::world::chunk::{Chunk, Section};
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        level.set_block([1, 64, 0], 1, false);
        let hit = clip_blocks(
            &level,
            DVec3::new(0.6, 64.5, 0.55),
            DVec3::new(2.6, 64.5, 2.55),
        )
        .expect("the corner is hit");
        assert_eq!(hit.pos, [1, 64, 0]);
        assert_eq!(hit.face, FACE_WEST);
    }

    /// A level with stone at each of `blocks`.
    fn level_with(blocks: &[[i32; 3]]) -> Level {
        use crate::world::chunk::{Chunk, Section};
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        for &pos in blocks {
            level.set_block(pos, 1, false);
        }
        level
    }

    /// A pig-sized box standing at `x, 64, z`.
    fn pig_at(id: i32, x: f64, z: f64) -> (i32, Aabb) {
        (id, Aabb::at_feet(DVec3::new(x, 64.0, z), 0.9, 0.9))
    }

    const EYE: DVec3 = DVec3::new(0.5, 64.5, 0.5);

    fn look_east(level: &Level, boxes: Vec<(i32, Aabb)>, entity_range: f64) -> Option<Hit> {
        pick(level, EYE, DVec3::X, SURVIVAL_RANGE, entity_range, boxes)
    }

    #[test]
    fn a_barrier_does_not_hide_a_name_tag() {
        use crate::world::chunk::{Chunk, Section};
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let barrier = pumpkin_data::Block::BARRIER.default_state.id.as_u16();
        level.set_block([1, 64, 0], barrier, false);
        let start = DVec3::new(0.5, 64.5, 0.5);
        let end = DVec3::new(2.5, 64.5, 0.5);
        assert!(
            clip_blocks(&level, start, end).is_some(),
            "barriers still have an outline for picking"
        );
        assert!(
            clip_blocks_for_nametag(&level, start, end).is_none(),
            "invisible-render blocks write no depth in vanilla"
        );
    }

    #[test]
    fn a_mob_in_front_of_a_block_is_hit_instead() {
        let level = level_with(&[[4, 64, 0]]);
        let hit = look_east(&level, vec![pig_at(7, 2.5, 0.5)], SURVIVAL_ENTITY_RANGE);
        let Some(Hit::Entity(hit)) = hit else {
            panic!("{hit:?}");
        };
        assert_eq!(hit.id, 7);
        assert!((hit.location.x - 2.05).abs() < 1e-9, "{:?}", hit.location);
    }

    #[test]
    fn the_nearest_of_two_mobs_is_hit() {
        let level = level_with(&[]);
        let boxes = vec![pig_at(1, 2.8, 0.5), pig_at(2, 1.8, 0.5)];
        let hit = look_east(&level, boxes, SURVIVAL_ENTITY_RANGE);
        assert!(
            matches!(hit, Some(Hit::Entity(EntityHit { id: 2, .. }))),
            "{hit:?}"
        );
    }

    #[test]
    fn a_mob_behind_a_block_is_not_hit() {
        let level = level_with(&[[2, 64, 0]]);
        let hit = look_east(&level, vec![pig_at(7, 3.5, 0.5)], SURVIVAL_ENTITY_RANGE);
        assert!(
            matches!(
                hit,
                Some(Hit::Block(BlockHit {
                    pos: [2, 64, 0],
                    ..
                }))
            ),
            "{hit:?}"
        );
    }

    #[test]
    fn a_mob_out_of_reach_hides_the_block_behind_it() {
        let level = level_with(&[[4, 64, 0]]);
        let chicken = || vec![(7, Aabb::at_feet(DVec3::new(3.75, 64.0, 0.5), 0.4, 0.7))];
        let survival = look_east(&level, chicken(), SURVIVAL_ENTITY_RANGE);
        assert!(survival.is_none(), "{survival:?}");
        let creative = look_east(&level, chicken(), CREATIVE_ENTITY_RANGE);
        assert!(matches!(creative, Some(Hit::Entity(_))), "{creative:?}");
    }

    #[test]
    fn a_mob_around_the_eye_is_hit() {
        let level = level_with(&[[3, 64, 0]]);
        let hit = look_east(&level, vec![pig_at(7, 0.5, 0.5)], SURVIVAL_ENTITY_RANGE);
        let Some(Hit::Entity(hit)) = hit else {
            panic!("{hit:?}");
        };
        assert_eq!(hit.id, 7);
        assert_eq!(hit.location, EYE);
    }

    #[test]
    fn water_in_a_waterlogged_block_is_not_clipped() {
        use pumpkin_data::Block;
        let slab = Block::OAK_SLAB.default_state.id;
        let wet = slab
            .to_state()
            .set_waterlogged(true)
            .expect("slabs hold water");
        let mut level = level_with(&[]);
        level.set_block([3, 64, 0], wet.id.as_u16(), false);
        let low = clip_blocks(
            &level,
            DVec3::new(0.5, 64.7, 0.5),
            DVec3::new(6.5, 64.7, 0.5),
        );
        assert!(low.is_none(), "{low:?}");
        let high = clip_blocks(
            &level,
            DVec3::new(0.5, 64.3, 0.5),
            DVec3::new(6.5, 64.3, 0.5),
        );
        assert_eq!(high.map(|h| h.pos), Some([3, 64, 0]));
    }

    /// `cargo test --lib -- --ignored pick_cost --nocapture`: what a pick
    #[test]
    #[ignore]
    fn pick_cost() {
        use crate::world::chunk::{Chunk, Section};
        use pumpkin_data::Block;
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let slab = Block::OAK_SLAB.default_state.id.as_u16();
        let grass = Block::SHORT_GRASS.default_state.id.as_u16();
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..64 {
                    level.set_block([x, y, z], 1, false);
                }
                let top = if (x + z) % 3 == 0 { slab } else { grass };
                level.set_block([x, 64, z], top, false);
            }
        }
        let boxes: Vec<(i32, Aabb)> = (0..300)
            .map(|i| {
                pig_at(
                    i,
                    2.0 + f64::from(i % 20) * 3.0,
                    2.0 + f64::from(i / 20) * 3.0,
                )
            })
            .collect();
        let eye = DVec3::new(8.0, 66.0, 8.0);
        let rounds = 200_000;
        let started = std::time::Instant::now();
        let mut hits = 0;
        for i in 0..rounds {
            let yaw = f64::from(i) * 0.37;
            let pitch = (f64::from(i % 90) - 60.0).to_radians();
            let dir = DVec3::new(
                -yaw.sin() * pitch.cos(),
                -pitch.sin(),
                yaw.cos() * pitch.cos(),
            );
            hits += usize::from(
                pick(
                    &level,
                    eye,
                    dir,
                    SURVIVAL_RANGE,
                    SURVIVAL_ENTITY_RANGE,
                    boxes.iter().copied(),
                )
                .is_some(),
            );
        }
        let each = started.elapsed() / rounds;
        println!("pick: {each:?} each, {hits} of {rounds} hit");
    }

    #[test]
    fn a_look_from_inside_an_entity_hits_it() {
        let box_ = Aabb {
            min: DVec3::new(-1.0, 0.0, -1.0),
            max: DVec3::new(1.0, 2.0, 1.0),
        };
        let hit = clip_entities(
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(0.0, 1.0, 4.0),
            [(7, box_)],
            16.0,
        )
        .unwrap();
        assert_eq!(hit.id, 7);
        assert_eq!(hit.location, DVec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn neighbour_follows_the_face() {
        let hit = BlockHit {
            pos: [3, 4, 5],
            face: FACE_UP,
            location: DVec3::new(3.5, 5.0, 5.5),
            inside: false,
        };
        assert_eq!(hit.neighbour(), [3, 5, 5]);
    }
}
