//! Vanilla `Entity.collide`.

use glam::DVec3;
use pumpkin_data::BlockStateId;

use crate::world::block::state_flags;
use crate::world::fluid::{self, FluidKind};
use crate::world::level::Level;

/// `Shapes.EPSILON`, below which movement counts as none.
const EPSILON: f64 = 1.0e-7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    /// `EntityDimensions.makeBoundingBox`.
    pub fn at_feet(feet: DVec3, width: f64, height: f64) -> Self {
        let half = width / 2.0;
        Self {
            min: DVec3::new(feet.x - half, feet.y, feet.z - half),
            max: DVec3::new(feet.x + half, feet.y + height, feet.z + half),
        }
    }

    pub fn offset(self, by: DVec3) -> Self {
        Self {
            min: self.min + by,
            max: self.max + by,
        }
    }

    /// `AABB.expandTowards`: grown in the direction of `delta` only.
    pub fn expand_towards(self, delta: DVec3) -> Self {
        let mut out = self;
        for axis in 0..3 {
            if delta[axis] < 0.0 {
                out.min[axis] += delta[axis];
            } else {
                out.max[axis] += delta[axis];
            }
        }
        out
    }

    pub fn deflate(self, by: f64) -> Self {
        Self {
            min: self.min + DVec3::splat(by),
            max: self.max - DVec3::splat(by),
        }
    }

    pub fn intersects(&self, other: &Aabb) -> bool {
        (0..3).all(|axis| self.min[axis] < other.max[axis] && self.max[axis] > other.min[axis])
    }

    /// `VoxelShape.collide` for one box: how far `mover` may travel `delta`
    fn clip(&self, mover: &Aabb, axis: usize, mut delta: f64) -> f64 {
        if delta.abs() < EPSILON {
            return 0.0;
        }
        for cross in [(axis + 1) % 3, (axis + 2) % 3] {
            if self.max[cross] - EPSILON <= mover.min[cross]
                || self.min[cross] + EPSILON >= mover.max[cross]
            {
                return delta;
            }
        }
        if delta > 0.0 {
            let gap = self.min[axis] - mover.max[axis];
            if gap >= -EPSILON && gap < delta {
                delta = gap;
            }
        } else {
            let gap = self.max[axis] - mover.min[axis];
            if gap <= EPSILON && gap > delta {
                delta = gap;
            }
        }
        delta
    }
}

/// The block-state ID at a block position, or air outside loaded chunks.
pub fn block_at(level: &Level, x: i32, y: i32, z: i32) -> u16 {
    level.block_state(x, y, z).unwrap_or(0)
}

/// The block cells `region` reaches into, grown by `grow` on every side.
pub(crate) fn cells(
    region: &Aabb,
    grow: f64,
    extra_below: i32,
) -> impl Iterator<Item = (i32, i32, i32)> {
    let lo = |v: f64| (v - grow).floor() as i32;
    let hi = |v: f64| (v + grow).floor() as i32;
    let (x0, x1) = (lo(region.min.x), hi(region.max.x));
    let (y0, y1) = (lo(region.min.y) - extra_below, hi(region.max.y));
    let (z0, z1) = (lo(region.min.z), hi(region.max.z));
    (y0..=y1).flat_map(move |y| (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| (x, y, z))))
}

/// The collision boxes of the block at `(x, y, z)`, in world space.
pub fn block_shapes(level: &Level, (x, y, z): (i32, i32, i32)) -> impl Iterator<Item = Aabb> {
    let id = block_at(level, x, y, z);
    let state = BlockStateId::new(id).filter(|id| id.as_u16() != 0);
    let origin = DVec3::new(f64::from(x), f64::from(y), f64::from(z))
        + crate::world::block::offset::shape_delta(id, x, z);
    state
        .into_iter()
        .flat_map(|state| state.to_state().get_block_collision_shapes())
        .map(move |shape| Aabb {
            min: origin + DVec3::new(shape.min.x, shape.min.y, shape.min.z),
            max: origin + DVec3::new(shape.max.x, shape.max.y, shape.max.z),
        })
}

/// Every block collision box that intersects `region`.
pub fn block_boxes(level: &Level, region: &Aabb) -> Vec<Aabb> {
    let mut out = Vec::new();
    block_boxes_into(level, region, &mut out);
    out
}

/// [`block_boxes`] into a buffer that is emptied first, so a caller can reuse it.
pub fn block_boxes_into(level: &Level, region: &Aabb, out: &mut Vec<Aabb>) {
    out.clear();
    out.extend(
        cells(region, EPSILON, 1)
            .flat_map(|cell| block_shapes(level, cell))
            .filter(|aabb| aabb.intersects(region)),
    );
}

thread_local! {
    /// The boxes `collide` gathers, kept between calls.
    static SCRATCH: std::cell::RefCell<(Vec<Aabb>, Vec<Aabb>)> = const {
        std::cell::RefCell::new((Vec::new(), Vec::new()))
    };
}

/// `Entity.mainSupportingBlockPos` with `onGroundNoBlocks`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Support {
    pub block: Option<(i32, i32, i32)>,
    no_blocks: bool,
}

/// `Entity.checkSupportingBlock`.
pub fn update_support(
    level: &Level,
    aabb: &Aabb,
    position: DVec3,
    on_ground: bool,
    moved: DVec3,
    previous: Support,
) -> Support {
    if !on_ground {
        return Support::default();
    }
    let test = Aabb {
        min: DVec3::new(aabb.min.x, aabb.min.y - 1.0e-6, aabb.min.z),
        max: DVec3::new(aabb.max.x, aabb.min.y, aabb.max.z),
    };
    let mut found = find_supporting_block(level, &test, position);
    let block = if found.is_some() || previous.no_blocks {
        found
    } else {
        let back = test.offset(DVec3::new(-moved.x, 0.0, -moved.z));
        found = find_supporting_block(level, &back, position);
        found
    };
    Support {
        block,
        no_blocks: found.is_none(),
    }
}

/// `CollisionGetter.findSupportingBlock`.
pub fn find_supporting_block(
    level: &Level,
    region: &Aabb,
    position: DVec3,
) -> Option<(i32, i32, i32)> {
    let mut best: Option<((i32, i32, i32), f64)> = None;
    for cell in cells(region, EPSILON, 1) {
        if !block_shapes(level, cell).any(|b| b.intersects(region)) {
            continue;
        }
        let centre = DVec3::new(
            f64::from(cell.0) + 0.5,
            f64::from(cell.1) + 0.5,
            f64::from(cell.2) + 0.5,
        );
        let distance = centre.distance_squared(position);
        let later = |a: (i32, i32, i32), b: (i32, i32, i32)| (a.1, a.2, a.0) > (b.1, b.2, b.0);
        let better = match best {
            None => true,
            Some((at, d)) => distance < d || (distance == d && later(cell, at)),
        };
        if better {
            best = Some((cell, distance));
        }
    }
    best.map(|(cell, _)| cell)
}

/// `Level.collidesWithSuffocatingBlock`: a suffocating block (`isSuffocating`)
pub fn collides_with_suffocating(level: &Level, region: &Aabb) -> bool {
    cells(region, EPSILON, 0).any(|cell| {
        let id = block_at(level, cell.0, cell.1, cell.2);
        BlockStateId::new(id).is_some_and(|state| {
            id != 0
                && state.to_state().is_suffocating()
                && block_shapes(level, cell).any(|b| b.intersects(region))
        })
    })
}

/// No block box overlaps `region` (`Level.noCollision`).
pub fn no_collision(level: &Level, region: &Aabb) -> bool {
    !cells(region, EPSILON, 1)
        .flat_map(|cell| block_shapes(level, cell))
        .any(|aabb| aabb.intersects(region))
}

/// `Entity.collideWithShapes`.
fn collide_with_boxes(boxes: &[Aabb], mover: Aabb, mut delta: DVec3) -> DVec3 {
    for b in boxes {
        delta.y = b.clip(&mover, 1, delta.y);
    }
    let mut moved = mover.offset(DVec3::new(0.0, delta.y, 0.0));
    let (first, second) = if delta.x.abs() < delta.z.abs() {
        (2, 0)
    } else {
        (0, 2)
    };
    for b in boxes {
        delta[first] = b.clip(&moved, first, delta[first]);
    }
    let mut step = DVec3::ZERO;
    step[first] = delta[first];
    moved = moved.offset(step);
    for b in boxes {
        delta[second] = b.clip(&moved, second, delta[second]);
    }
    delta
}

/// `Entity.collide`.
pub fn collide(
    level: &Level,
    colliders: &[Aabb],
    mover: Aabb,
    delta: DVec3,
    step_height: f64,
    on_ground: bool,
) -> DVec3 {
    SCRATCH.with_borrow_mut(|(boxes, step_boxes)| {
        let region = mover.expand_towards(delta);
        block_boxes_into(level, &region, boxes);
        boxes.extend(colliders.iter().filter(|b| b.intersects(&region)));
        let moved = collide_with_boxes(boxes, mover, delta);

        let blocked_x = delta.x != moved.x;
        let blocked_z = delta.z != moved.z;
        let landed = delta.y != moved.y && delta.y < 0.0;
        if step_height <= 0.0 || !(on_ground || landed) || !(blocked_x || blocked_z) {
            return moved;
        }

        let grounded = if landed {
            mover.offset(DVec3::new(0.0, moved.y, 0.0))
        } else {
            mover
        };
        let mut step_region = grounded.expand_towards(DVec3::new(delta.x, step_height, delta.z));
        if !landed {
            step_region = step_region.expand_towards(DVec3::new(0.0, -f64::from(1.0e-5_f32), 0.0));
        }
        block_boxes_into(level, &step_region, step_boxes);
        step_boxes.extend(colliders.iter().filter(|b| b.intersects(&step_region)));
        let mut heights: Vec<f32> = Vec::new();
        for b in step_boxes.iter() {
            for y in [b.min.y, b.max.y] {
                let h = (y - grounded.min.y) as f32;
                if h >= 0.0
                    && h != moved.y as f32
                    && h <= step_height as f32
                    && !heights.contains(&h)
                {
                    heights.push(h);
                }
            }
        }
        heights.sort_by(f32::total_cmp);

        let flat = moved.x * moved.x + moved.z * moved.z;
        for h in heights {
            let stepped = collide_with_boxes(
                step_boxes,
                grounded,
                DVec3::new(delta.x, f64::from(h), delta.z),
            );
            if stepped.x * stepped.x + stepped.z * stepped.z > flat {
                let drop = mover.min.y - grounded.min.y;
                return stepped - DVec3::new(0.0, drop, 0.0);
            }
        }
        moved
    })
}

/// `Player.canFallAtLeast`.
pub fn can_fall_at_least(
    level: &Level,
    colliders: &[Aabb],
    mover: Aabb,
    (dx, dz): (f64, f64),
    min_height: f64,
) -> bool {
    let probe = Aabb {
        min: DVec3::new(
            mover.min.x + EPSILON + dx,
            mover.min.y - min_height - EPSILON,
            mover.min.z + EPSILON + dz,
        ),
        max: DVec3::new(
            mover.max.x - EPSILON + dx,
            mover.min.y,
            mover.max.z - EPSILON + dz,
        ),
    };
    no_collision(level, &probe) && !colliders.iter().any(|b| b.intersects(&probe))
}

/// `Player.maybeBackOffFromEdge`.
pub fn back_off_from_edge(
    level: &Level,
    colliders: &[Aabb],
    mover: Aabb,
    mut delta: DVec3,
    max_drop: f64,
) -> DVec3 {
    let can_fall =
        |dx: f64, dz: f64| can_fall_at_least(level, colliders, mover, (dx, dz), max_drop);
    const STEP: f64 = 0.05;
    let step_x = delta.x.signum() * STEP;
    let step_z = delta.z.signum() * STEP;
    while delta.x != 0.0 && can_fall(delta.x, 0.0) {
        if delta.x.abs() <= STEP {
            delta.x = 0.0;
            break;
        }
        delta.x -= step_x;
    }
    while delta.z != 0.0 && can_fall(0.0, delta.z) {
        if delta.z.abs() <= STEP {
            delta.z = 0.0;
            break;
        }
        delta.z -= step_z;
    }
    while delta.x != 0.0 && delta.z != 0.0 && can_fall(delta.x, delta.z) {
        delta.x = if delta.x.abs() <= STEP {
            0.0
        } else {
            delta.x - step_x
        };
        if delta.z.abs() <= STEP {
            delta.z = 0.0;
        } else {
            delta.z -= step_z;
        }
    }
    delta
}

/// Whether any block overlapping `region` holds a fluid, and which.
pub fn touches_fluid(level: &Level, region: &Aabb) -> Option<FluidKind> {
    let mut found = None;
    for (x, y, z) in cells(region, -EPSILON, 0) {
        match fluid_at(level, x, y, z) {
            Some(FluidKind::Lava) => return Some(FluidKind::Lava),
            Some(FluidKind::Water) => found = Some(FluidKind::Water),
            None => {}
        }
    }
    found
}

/// Whether the eye at `eye` is in water (`isEyeInFluid(WATER)`).
pub fn eye_in_water(level: &Level, eye: glam::DVec3) -> bool {
    let block = eye.floor().as_ivec3();
    fluid_height(level, block, FluidKind::Water)
        .is_some_and(|height| eye.y < f64::from(block.y) + f64::from(height))
}

/// `FluidState.getHeightForCamera` of `kind` at `block`, if it holds that.
pub fn fluid_height(level: &Level, block: glam::IVec3, kind: FluidKind) -> Option<f32> {
    let state = level.block_state(block.x, block.y, block.z)?;
    let fluid = fluid::fluid_of(state).filter(|f| f.kind == kind)?;
    let above = level
        .block_state(block.x, block.y + 1, block.z)
        .unwrap_or(0);
    let covered = fluid::fluid_of(above).is_some_and(|f| f.kind == kind)
        || (fluid.amount == 8 && !fluid.falling && state_flags::is_solid(above));
    Some(if covered { 1.0 } else { fluid.own_height() })
}

/// The top of the fluid in a cell as `FluidState.getHeight`.
fn fluid_top(level: &Level, (x, y, z): (i32, i32, i32), fluid: fluid::FluidState) -> f64 {
    let above = block_at(level, x, y + 1, z);
    let full = fluid::fluid_of(above).is_some_and(|f| f.kind == fluid.kind);
    f64::from(y)
        + if full {
            1.0
        } else {
            f64::from(fluid.own_height())
        }
}

/// `FlowingFluid.isSolidFace`.
fn solid_face(level: &Level, (x, y, z): (i32, i32, i32), kind: FluidKind, up: bool) -> bool {
    let id = block_at(level, x, y, z);
    if fluid::fluid_of(id).is_some_and(|f| f.kind == kind) {
        return false;
    }
    if up {
        return true;
    }
    let is_ice = BlockStateId::new(id).is_some_and(|state| {
        pumpkin_data::Block::from_state_id(state) == &pumpkin_data::Block::ICE
    });
    !is_ice && state_flags::is_solid(id)
}

/// `FlowingFluid.getFlow`.
pub fn fluid_flow(level: &Level, (x, y, z): (i32, i32, i32), fluid: fluid::FluidState) -> DVec3 {
    const SIDES: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let own = f64::from(fluid.own_height());
    let affects = |f: Option<fluid::FluidState>| f.is_none_or(|f| f.kind == fluid.kind);
    let height = |f: Option<fluid::FluidState>| f.map_or(0.0, |f| f64::from(f.own_height()));
    let (mut fx, mut fz) = (0.0, 0.0);
    for (dx, dz) in SIDES {
        let id = block_at(level, x + dx, y, z + dz);
        let neighbour = fluid::fluid_of(id);
        if !affects(neighbour) {
            continue;
        }
        let nh = height(neighbour);
        let mut distance = 0.0;
        if nh == 0.0 {
            if !state_flags::is_solid(id) {
                let below = fluid::fluid_of(block_at(level, x + dx, y - 1, z + dz));
                if affects(below) {
                    let bh = height(below);
                    if bh > 0.0 {
                        distance = own - (bh - f64::from(0.888_888_9_f32));
                    }
                }
            }
        } else {
            distance = own - nh;
        }
        if distance != 0.0 {
            fx += f64::from(dx) * distance;
            fz += f64::from(dz) * distance;
        }
    }
    let mut flow = DVec3::new(fx, 0.0, fz);
    if fluid.falling {
        for (dx, dz) in SIDES {
            let (nx, nz) = (x + dx, z + dz);
            if solid_face(level, (nx, y, nz), fluid.kind, false)
                || solid_face(level, (nx, y + 1, nz), fluid.kind, false)
            {
                flow = flow.normalize_or_zero() + DVec3::new(0.0, -6.0, 0.0);
                break;
            }
        }
    }
    flow.normalize_or_zero()
}

/// What `EntityFluidInteraction` gathers of one fluid around an entity.
#[derive(Clone, Copy, Default, Debug)]
pub struct FluidCurrent {
    /// How far the fluid reaches above the entity's feet.
    pub height: f64,
    /// The flows of its cells, summed (scaled while the fluid is shallow).
    pub flow: DVec3,
    pub count: u32,
}

/// `EntityFluidInteraction.update` for one fluid.
pub fn fluid_current(level: &Level, region: &Aabb, feet_y: f64, kind: FluidKind) -> FluidCurrent {
    let mut current = FluidCurrent::default();
    for cell in cells(region, -EPSILON, 0) {
        let Some(fluid) =
            fluid::fluid_of(block_at(level, cell.0, cell.1, cell.2)).filter(|f| f.kind == kind)
        else {
            continue;
        };
        let top = fluid_top(level, cell, fluid);
        if top < region.min.y {
            continue;
        }
        current.height = current.height.max(top - feet_y);
        let mut flow = fluid_flow(level, cell, fluid);
        if current.height < 0.4 {
            flow *= current.height;
        }
        current.flow += flow;
        current.count += 1;
    }
    current
}

pub fn fluid_at(level: &Level, x: i32, y: i32, z: i32) -> Option<FluidKind> {
    fluid::fluid_of(block_at(level, x, y, z)).map(|fluid| fluid.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{Chunk, Section};
    use pumpkin_data::Block;

    #[test]
    fn a_box_falling_onto_another_stops_on_top() {
        let floor = Aabb {
            min: DVec3::new(0.0, 0.0, 0.0),
            max: DVec3::new(1.0, 1.0, 1.0),
        };
        let player = Aabb::at_feet(DVec3::new(0.5, 1.5, 0.5), 0.6, 1.8);
        let moved = collide_with_boxes(&[floor], player, DVec3::new(0.0, -1.0, 0.0));
        assert!((moved.y + 0.5).abs() < 1e-9, "{moved:?}");
    }

    #[test]
    fn a_wall_stops_horizontal_movement() {
        let wall = Aabb {
            min: DVec3::new(1.0, 0.0, 0.0),
            max: DVec3::new(2.0, 2.0, 1.0),
        };
        let player = Aabb::at_feet(DVec3::new(0.5, 0.0, 0.5), 0.6, 1.8);
        let moved = collide_with_boxes(&[wall], player, DVec3::new(0.5, 0.0, 0.0));
        assert!((moved.x - 0.2).abs() < 1e-9, "{moved:?}");
    }

    /// Stone up to y = 63 around the origin (chunks -1..=1).
    fn floor_level() -> Level {
        let mut level = Level::default();
        for cx in -1..=1 {
            for cz in -1..=1 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                for section in &mut sections[..8] {
                    *section = Section::filled(1);
                }
                level.insert(Chunk::new(cx, cz, sections));
            }
        }
        level
    }

    /// The state of `block` whose properties include all of `props`.
    fn state(block: &Block, props: &[(&str, &str)]) -> u16 {
        block
            .states
            .iter()
            .find(|s| {
                let Some(p) = block.properties(s.id) else {
                    return props.is_empty();
                };
                let all = p.to_props();
                props
                    .iter()
                    .all(|(k, v)| all.iter().any(|q| q.key_str() == *k && q.value_str() == *v))
            })
            .map(|s| s.id.as_u16())
            .expect("state")
    }

    fn put(level: &mut Level, at: [i32; 3], id: u16) {
        level.set_block(at, id, false);
    }

    fn walk(level: &Level, feet: DVec3, delta: DVec3) -> DVec3 {
        let b = Aabb::at_feet(feet, 0.6_f32.into(), 1.8_f32.into());
        collide(level, &[], b, delta, 0.6, true)
    }

    #[test]
    fn a_bottom_slab_is_stepped_onto_and_a_top_slab_is_not() {
        let mut level = floor_level();
        put(
            &mut level,
            [1, 64, 0],
            state(&Block::STONE_SLAB, &[("type", "bottom")]),
        );
        put(
            &mut level,
            [1, 64, 3],
            state(&Block::STONE_SLAB, &[("type", "top")]),
        );
        let feet = DVec3::new(0.5, 64.0, 0.5);
        let up = walk(&level, feet, DVec3::new(0.3, -0.0784, 0.0));
        assert!(
            (up.x - 0.3).abs() < 1e-9 && (up.y - 0.5).abs() < 1e-6,
            "{up:?}"
        );
        let feet = DVec3::new(0.5, 64.0, 3.5);
        let blocked = walk(&level, feet, DVec3::new(0.3, -0.0784, 0.0));
        assert!((blocked.x - 0.2).abs() < 1e-6, "{blocked:?}");
    }

    #[test]
    fn stairs_climb_from_the_low_side_and_stop_at_the_high_side() {
        let mut level = floor_level();
        let facing_east = state(
            &Block::OAK_STAIRS,
            &[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
            ],
        );
        put(&mut level, [1, 64, 0], facing_east);
        let up = walk(
            &level,
            DVec3::new(0.5, 64.0, 0.5),
            DVec3::new(0.3, -0.0784, 0.0),
        );
        assert!((up.x - 0.3).abs() < 1e-9 && up.y > 0.49, "{up:?}");
        let back = walk(
            &level,
            DVec3::new(2.5, 64.0, 0.5),
            DVec3::new(-0.3, -0.0784, 0.0),
        );
        assert!((back.x + 0.2).abs() < 1e-6 && back.y <= 0.0, "{back:?}");
    }

    #[test]
    fn a_step_up_still_has_the_ground_under_it_as_a_candidate() {
        let mut level = floor_level();
        put(
            &mut level,
            [1, 64, 0],
            state(&Block::STONE_SLAB, &[("type", "bottom")]),
        );
        let b = Aabb::at_feet(DVec3::new(0.5, 64.0, 0.5), 0.6, 1.8);
        let moved = collide(&level, &[], b, DVec3::new(0.3, 0.0, 0.0), 0.6, true);
        assert!((moved.x - 0.3).abs() < 1e-9, "{moved:?}");
    }

    /// A tiny deterministic generator, so the fuzz needs no crate.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1_u64 << 53) as f64)
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
    }

    #[test]
    fn fast_moves_over_slabs_and_stairs_never_end_inside_a_block() {
        let mut level = floor_level();
        let mut rng = Lcg(0x5EED);
        let kinds = [
            state(&Block::STONE_SLAB, &[("type", "bottom")]),
            state(&Block::STONE_SLAB, &[("type", "top")]),
            state(
                &Block::OAK_STAIRS,
                &[("facing", "north"), ("half", "bottom")],
            ),
            state(&Block::OAK_STAIRS, &[("facing", "east"), ("half", "top")]),
            state(&Block::OAK_FENCE, &[]),
            state(&Block::MOSS_CARPET, &[]),
            state(&Block::WHITE_CARPET, &[]),
            1,
            0,
            0,
            0,
            0,
            0,
        ];
        for x in 0..8 {
            for z in 0..8 {
                for y in 64..66 {
                    let k = (rng.next() * kinds.len() as f64) as usize % kinds.len();
                    put(&mut level, [x, y, z], kinds[k]);
                }
            }
        }
        let mut checked = 0;
        for _ in 0..20000 {
            let feet = DVec3::new(
                rng.range(0.5, 7.5),
                rng.range(63.0, 66.0),
                rng.range(0.5, 7.5),
            );
            let b = Aabb::at_feet(feet, 0.6_f32.into(), 1.8_f32.into());
            if !no_collision(&level, &b.deflate(1.0e-5)) {
                continue;
            }
            let delta = DVec3::new(
                rng.range(-1.5, 1.5),
                rng.range(-1.0, 0.5),
                rng.range(-1.5, 1.5),
            );
            let ground = rng.next() < 0.5;
            let moved = collide(&level, &[], b, delta, 0.6, ground);
            let after = b.offset(moved);
            assert!(
                no_collision(&level, &after.deflate(1.0e-5)),
                "from {feet:?} by {delta:?} (ground {ground}) ended inside: {moved:?}"
            );
            checked += 1;
        }
        assert!(checked > 500, "{checked}");
    }

    #[test]
    fn carpets_are_stepped_onto_and_fallen_onto_and_fences_stop_a_jump() {
        let mut level = floor_level();
        for (z, block) in [(0, &Block::MOSS_CARPET), (3, &Block::WHITE_CARPET)] {
            put(&mut level, [1, 64, z], state(block, &[]));
        }
        put(
            &mut level,
            [1, 64, 6],
            state(
                &Block::OAK_FENCE,
                &[
                    ("north", "false"),
                    ("south", "false"),
                    ("east", "false"),
                    ("west", "false"),
                    ("waterlogged", "false"),
                ],
            ),
        );
        for z in [0, 3] {
            let up = walk(
                &level,
                DVec3::new(0.5, 64.0, f64::from(z) + 0.5),
                DVec3::new(0.3, -0.0784, 0.0),
            );
            assert!(
                (up.x - 0.3).abs() < 1e-9 && (up.y - 0.0625).abs() < 1e-6,
                "{up:?}"
            );
            let b = Aabb::at_feet(DVec3::new(1.5, 66.0, f64::from(z) + 0.5), 0.6, 1.8);
            let down = collide(&level, &[], b, DVec3::new(0.0, -5.0, 0.0), 0.6, false);
            assert!((66.0 + down.y - 64.0625).abs() < 1e-6, "{down:?}");
        }
        let fence = walk(
            &level,
            DVec3::new(0.5, 64.0, 6.5),
            DVec3::new(0.8, -0.0784, 0.0),
        );
        assert!((fence.x - (1.375 - 0.8)).abs() < 1e-6, "{fence:?}");
    }

    #[test]
    fn the_supporting_block_is_the_nearest_centre() {
        let mut level = floor_level();
        put(&mut level, [0, 63, 0], state(&Block::ICE, &[]));
        put(&mut level, [1, 63, 0], 0);
        let b = Aabb::at_feet(DVec3::new(1.02, 64.0, 0.5), 0.6, 1.8);
        let test = Aabb {
            min: DVec3::new(b.min.x, b.min.y - 1.0e-6, b.min.z),
            max: DVec3::new(b.max.x, b.min.y, b.max.z),
        };
        let found = find_supporting_block(&level, &test, DVec3::new(1.02, 64.0, 0.5));
        assert_eq!(found, Some((0, 63, 0)));
    }

    /// The cells a step on snow is heard from: `getOnPos()` (the supporting
    /// block) and `getOnPosLegacy()` (0.2 under the feet in its column).
    fn snow_step_cells(layers: u8) -> ((i32, i32, i32), (i32, i32, i32)) {
        let mut level = floor_level();
        let snow = state(&Block::SNOW, &[("layers", &layers.to_string())]);
        put(&mut level, [0, 64, 0], snow);
        let feet = DVec3::new(0.5, 64.0 + f64::from(layers - 1) * 2.0 / 16.0, 0.5);
        let b = Aabb::at_feet(feet, 0.6_f32.into(), 1.8_f32.into());
        let support = update_support(&level, &b, feet, true, DVec3::ZERO, Support::default());
        let on =
            |offset: f64| crate::world::block_motion::on_pos(&level, feet, support.block, offset);
        (on(f64::from(1.0e-5_f32)), on(f64::from(0.2_f32)))
    }

    #[test]
    fn thin_snow_is_heard_over_the_block_under_it() {
        // One layer has no collision: the stone carries the player, and
        // the snow above it makes the step a combination sound.
        assert_eq!(snow_step_cells(1), ((0, 63, 0), (0, 63, 0)));
        // Two layers carry the player, but 0.2 under the feet is the stone.
        assert_eq!(snow_step_cells(2), ((0, 64, 0), (0, 63, 0)));
        // From three layers on the snow alone is stepped on.
        assert_eq!(snow_step_cells(3), ((0, 64, 0), (0, 64, 0)));
        assert_eq!(snow_step_cells(8), ((0, 64, 0), (0, 64, 0)));
    }

    #[test]
    fn over_an_edge_the_step_is_heard_from_the_supporting_block() {
        let mut level = floor_level();
        put(&mut level, [1, 63, 0], 0);
        // Feet over the hole at x=1, the box still on the block at x=0.
        let feet = DVec3::new(1.2, 64.0, 0.5);
        let b = Aabb::at_feet(feet, 0.6_f32.into(), 1.8_f32.into());
        let support = update_support(&level, &b, feet, true, DVec3::ZERO, Support::default());
        assert_eq!(support.block, Some((0, 63, 0)));
        let on =
            crate::world::block_motion::on_pos(&level, feet, support.block, f64::from(0.2_f32));
        assert_eq!(on, (0, 63, 0));
    }

    #[test]
    fn a_player_sized_box_is_the_float_dimension() {
        let b = Aabb::at_feet(DVec3::ZERO, 0.6_f32.into(), 1.8_f32.into());
        assert_eq!(b.max.x, f64::from(0.6_f32) / 2.0);
        assert_eq!(b.max.y, f64::from(1.8_f32));
    }
}
