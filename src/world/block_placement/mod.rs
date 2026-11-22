//! The state a block item places, as the server picks it (`BlockItem.place`,
//! `getStateForPlacement`), so a predicted block isn't turned a tick later.
//!
//! Pumpkin's `on_place` needs its `World`, so only its extracted property
//! lists are used here ([`Edit`]). Rules are keyed by a block's properties,
//! then by tags or names where vanilla classes differ.
//!
//! Left to the server: redstone power, fence/wall/pane/wire connections,
//! and neighbour shapes other than chests and stairs.

mod attached;
mod chest;
mod look;
mod stairs;

use glam::DVec3;
use pumpkin_data::block_properties::{
    AttachFace, BellAttachment, DoorHinge, DoubleBlockHalf, Half, SlabType,
};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId, Property};

pub use look::Dir;

use crate::world::fluid::{FluidKind, fluid_of};

/// A click with a block item.
pub struct PlaceInput<'a> {
    /// Block state at a position (`0` where unloaded).
    pub blocks: &'a dyn Fn([i32; 3]) -> u16,
    pub hit_pos: [i32; 3],
    pub face: Dir,
    /// World-space hit point.
    pub location: DVec3,
    /// Vanilla yaw and pitch in degrees.
    pub yaw: f32,
    pub pitch: f32,
    /// `isSecondaryUseActive`.
    pub sneaking: bool,
}

/// The placed block, plus what changes with it (door top, chest partner,
/// neighbouring stairs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub pos: [i32; 3],
    pub state: u16,
    pub also: Vec<([i32; 3], u16)>,
}

/// `BlockPlaceContext` with the position settled.
pub(crate) struct Ctx<'a> {
    input: &'a PlaceInput<'a>,
    pub pos: [i32; 3],
    /// `replacingClickedOnBlock`.
    pub replace_clicked: bool,
    /// The block being placed.
    pub block: &'static Block,
}

impl Ctx<'_> {
    pub fn get(&self, pos: [i32; 3]) -> u16 {
        (self.input.blocks)(pos)
    }

    pub fn face(&self) -> Dir {
        self.input.face
    }

    pub fn sneaking(&self) -> bool {
        self.input.sneaking
    }

    pub fn yaw(&self) -> f32 {
        self.input.yaw
    }

    /// `getHorizontalDirection`.
    pub fn horizontal(&self) -> Dir {
        Dir::from_y_rot(f64::from(self.input.yaw))
    }

    /// `getNearestLookingDirection`.
    pub fn nearest_looking(&self) -> Dir {
        look::ordered_by_nearest(self.input.yaw, self.input.pitch)[0]
    }

    /// `getNearestLookingDirections`: the way into the clicked block first,
    /// unless it is replaced.
    pub fn nearest_looking_all(&self) -> [Dir; 6] {
        let mut dirs = look::ordered_by_nearest(self.input.yaw, self.input.pitch);
        if self.replace_clicked {
            return dirs;
        }
        let into = self.input.face.opposite();
        if let Some(index) = dirs.iter().position(|&d| d == into)
            && index > 0
        {
            dirs.copy_within(0..index, 1);
            dirs[0] = into;
        }
        dirs
    }

    /// Click height within the placed position.
    pub fn click_y(&self) -> f64 {
        self.input.location.y - f64::from(self.pos[1])
    }

    pub fn click_xz(&self) -> (f64, f64) {
        (
            self.input.location.x - f64::from(self.pos[0]),
            self.input.location.z - f64::from(self.pos[2]),
        )
    }

    /// `getFluidState(pos).is(Fluids.WATER)`: a water source.
    pub fn water_source(&self) -> bool {
        fluid_of(self.get(self.pos))
            .is_some_and(|f| f.kind == FluidKind::Water && f.amount == 8 && !f.falling)
    }

    /// `canBeReplaced(context)` away from the clicked block.
    pub fn replaceable(&self, state: u16) -> bool {
        can_be_replaced(self.block, state, false, self.input)
    }
}

/// A block state as an editable property list.
#[derive(Clone, Debug)]
pub(crate) struct Edit {
    pub block: &'static Block,
    props: Vec<Property>,
}

impl Edit {
    pub fn default_of(block: &'static Block) -> Edit {
        Edit::of_state(block.default_state.id.as_u16())
    }

    pub fn of_state(state: u16) -> Edit {
        let id = BlockStateId::new_or_air(state);
        let block = Block::from_state_id(id);
        let props = block
            .properties(id)
            .map(|p| p.to_props())
            .unwrap_or_default();
        Edit { block, props }
    }

    /// The first property `pick` matches.
    pub fn find<T>(&self, pick: impl Fn(Property) -> Option<T>) -> Option<T> {
        self.props.iter().copied().find_map(pick)
    }

    pub fn has(&self, pick: impl Fn(Property) -> bool) -> bool {
        self.props.iter().copied().any(pick)
    }

    /// Sets `prop` if the block has that kind of property.
    pub fn set(&mut self, prop: Property) -> &mut Edit {
        let kind = std::mem::discriminant(&prop);
        for p in &mut self.props {
            if std::mem::discriminant(p) == kind {
                *p = prop;
            }
        }
        self
    }

    pub fn facing(&self) -> Option<Dir> {
        self.find(|p| match p {
            Property::Facing(f) => Some(Dir::from_facing(f)),
            _ => None,
        })
    }

    pub fn set_facing(&mut self, dir: Dir) -> &mut Edit {
        self.set(Property::Facing(dir.facing()))
    }

    pub fn state(&self) -> u16 {
        if self.props.is_empty() {
            return self.block.default_state.id.as_u16();
        }
        self.block
            .from_properties(&self.props)
            .to_state_id(self.block)
            .as_u16()
    }
}

/// `BlockItem.place` up to the state: position, state, survival. `None`
/// where vanilla places nothing.
pub fn place(block: &'static Block, input: &PlaceInput<'_>) -> Option<Placement> {
    let clicked = (input.blocks)(input.hit_pos);
    let replace_clicked = can_be_replaced(block, clicked, true, input);
    let pos = if replace_clicked {
        input.hit_pos
    } else {
        input.face.offset(input.hit_pos)
    };
    let ctx = Ctx {
        input,
        pos,
        replace_clicked,
        block,
    };
    if !replace_clicked && !ctx.replaceable(ctx.get(pos)) {
        return None;
    }
    let edit = match attached::wall_variant(block) {
        Some((wall, attachment)) => attached::standing_or_wall(&ctx, wall, attachment)?,
        None => {
            let edit = state_for(&ctx, block)?;
            attached::survives(&ctx, &edit).then_some(edit)?
        }
    };
    let state = edit.state();
    let mut also = Vec::new();
    if edit.block.has_tag(&tag::Block::MINECRAFT_DOORS) || is_double_plant(&edit) {
        let mut upper = edit.clone();
        upper.set(Property::DoubleBlockHalf(DoubleBlockHalf::Upper));
        if is_double_plant(&edit) {
            let above = ctx.get(Dir::Up.offset(pos));
            let water = fluid_of(above)
                .is_some_and(|f| f.kind == FluidKind::Water && f.amount == 8 && !f.falling);
            upper.set(Property::Waterlogged(water));
        }
        also.push((Dir::Up.offset(pos), upper.state()));
    }
    also.extend(chest::partner_update(&ctx, &edit));
    also.extend(stairs::neighbour_updates(&ctx, pos, state));
    Some(Placement { pos, state, also })
}

/// `canBeReplaced(context)`: air, replaceables (not by themselves), and
/// blocks that stack: slabs, snow, candles, pickles, eggs, petals.
fn can_be_replaced(
    block: &'static Block,
    state: u16,
    replace_clicked: bool,
    input: &PlaceInput,
) -> bool {
    let id = BlockStateId::new_or_air(state);
    let here = Block::from_state_id(id);
    if here == block {
        let edit = Edit::of_state(state);
        if let Some(slab) = edit.find(|p| match p {
            Property::SlabType(t) => Some(t),
            _ => None,
        }) {
            if slab == SlabType::Double {
                return false;
            }
            if !replace_clicked {
                return true;
            }
            let upper = input.location.y - f64::from(input.hit_pos[1]) > 0.5;
            let face = input.face;
            return match slab {
                SlabType::Bottom => face == Dir::Up || upper && face.is_horizontal(),
                _ => face == Dir::Down || !upper && face.is_horizontal(),
            };
        }
        if let Some(layers) = edit.find(|p| match p {
            Property::Layers(n) => Some(n),
            _ => None,
        }) {
            if layers >= 8 {
                return false;
            }
            return !replace_clicked || input.face == Dir::Up;
        }
        if let Some(count) = edit.find(stack_count) {
            return !input.sneaking && count < 4;
        }
        return false;
    }
    id.is_air() || id.to_state().replaceable()
}

/// Count of a stacking block.
fn stack_count(p: Property) -> Option<u8> {
    match p {
        Property::Candles(n)
        | Property::Pickles(n)
        | Property::Eggs(n)
        | Property::FlowerAmount(n)
        | Property::SegmentAmount(n) => Some(n),
        _ => None,
    }
}

fn with_count(p: Property, n: u8) -> Property {
    match p {
        Property::Candles(_) => Property::Candles(n),
        Property::Pickles(_) => Property::Pickles(n),
        Property::Eggs(_) => Property::Eggs(n),
        Property::FlowerAmount(_) => Property::FlowerAmount(n),
        Property::SegmentAmount(_) => Property::SegmentAmount(n),
        p => p,
    }
}

/// `DoublePlantBlock`: two halves, but no door.
fn is_double_plant(edit: &Edit) -> bool {
    edit.has(|p| matches!(p, Property::DoubleBlockHalf(_)))
        && !edit.has(|p| matches!(p, Property::Hinge(_)))
}

/// `getStateForPlacement` of `block`'s class at `ctx.pos`.
pub(crate) fn state_for(ctx: &Ctx, block: &'static Block) -> Option<Edit> {
    let here = ctx.get(ctx.pos);
    if Block::from_state_id(BlockStateId::new_or_air(here)) == block
        && let Some(edit) = merge_into(here)
    {
        return Some(edit);
    }
    let mut edit = Edit::default_of(block);
    edit.set(Property::Waterlogged(ctx.water_source()));
    let name = block.name;
    let has = |pick: fn(Property) -> bool| edit.has(pick);

    if has(|p| matches!(p, Property::ChestType(_))) {
        chest::place(ctx, &mut edit);
    } else if has(|p| matches!(p, Property::StairsShape(_))) {
        stairs::place(ctx, &mut edit);
    } else if has(|p| matches!(p, Property::SlabType(_))) {
        let top = ctx.face() == Dir::Down || ctx.face() != Dir::Up && ctx.click_y() > 0.5;
        edit.set(Property::SlabType(if top {
            SlabType::Top
        } else {
            SlabType::Bottom
        }));
    } else if has(|p| matches!(p, Property::Hinge(_))) {
        if !ctx.replaceable(ctx.get(Dir::Up.offset(ctx.pos))) {
            return None;
        }
        let hinge = door_hinge(ctx);
        edit.set_facing(ctx.horizontal())
            .set(Property::Hinge(hinge))
            .set(Property::DoubleBlockHalf(DoubleBlockHalf::Lower));
    } else if has(|p| matches!(p, Property::DoubleBlockHalf(_))) {
        if !ctx.replaceable(ctx.get(Dir::Up.offset(ctx.pos))) {
            return None;
        }
        if edit.facing().is_some() {
            edit.set_facing(ctx.horizontal().opposite());
        }
    } else if has(|p| matches!(p, Property::Face(_))) {
        return attached::face_attached(ctx, edit);
    } else if has(|p| matches!(p, Property::Attachment(_))) {
        return bell(ctx, edit);
    } else if block.has_tag(&tag::Block::MINECRAFT_TRAPDOORS) {
        if !ctx.replace_clicked && ctx.face().is_horizontal() {
            let half = if ctx.click_y() > 0.5 {
                Half::Top
            } else {
                Half::Bottom
            };
            edit.set_facing(ctx.face()).set(Property::Half(half));
        } else {
            let half = if ctx.face() == Dir::Up {
                Half::Bottom
            } else {
                Half::Top
            };
            edit.set_facing(ctx.horizontal().opposite())
                .set(Property::Half(half));
        }
    } else if has(|p| matches!(p, Property::InWall(_))) {
        let dir = ctx.horizontal();
        let is_wall = |d: Dir| {
            Block::from_state_id(BlockStateId::new_or_air(ctx.get(d.offset(ctx.pos))))
                .has_tag(&tag::Block::MINECRAFT_WALLS)
        };
        let in_wall = if dir.axis() == pumpkin_data::block_properties::Axis::Z {
            is_wall(Dir::West) || is_wall(Dir::East)
        } else {
            is_wall(Dir::North) || is_wall(Dir::South)
        };
        edit.set_facing(dir).set(Property::InWall(in_wall));
    } else if has(|p| matches!(p, Property::Part(_))) {
        let dir = ctx.horizontal();
        if !ctx.replaceable(ctx.get(dir.offset(ctx.pos))) {
            return None;
        }
        edit.set_facing(dir);
    } else if has(|p| matches!(p, Property::Hanging(_))) && name.ends_with("lantern") {
        return attached::lantern(ctx, edit);
    } else if has(|p| matches!(p, Property::Rotation(_))) {
        rotation(ctx, &mut edit);
    } else if has(|p| matches!(p, Property::Axis(_))) {
        edit.set(Property::Axis(ctx.face().axis()));
    } else if has(|p| matches!(p, Property::RailShape(_))) {
        use pumpkin_data::block_properties::RailShape;
        let shape = if ctx.horizontal().axis() == pumpkin_data::block_properties::Axis::X {
            RailShape::EastWest
        } else {
            RailShape::NorthSouth
        };
        edit.set(Property::RailShape(shape));
    } else if edit.facing().is_some() {
        return facing(ctx, edit);
    }
    if block.has_tag(&tag::Block::MINECRAFT_LEAVES) {
        edit.set(Property::Persistent(true));
    }
    Some(edit)
}

/// One more of what's there.
fn merge_into(here: u16) -> Option<Edit> {
    let mut edit = Edit::of_state(here);
    if edit.has(|p| matches!(p, Property::SlabType(_))) {
        edit.set(Property::SlabType(SlabType::Double))
            .set(Property::Waterlogged(false));
        return Some(edit);
    }
    if let Some(n) = edit.find(|p| match p {
        Property::Layers(n) => Some(n),
        _ => None,
    }) {
        edit.set(Property::Layers((n + 1).min(8)));
        return Some(edit);
    }
    let prop = edit.find(|p| stack_count(p).map(|_| p))?;
    let n = stack_count(prop)?;
    edit.set(with_count(prop, (n + 1).min(4)));
    Some(edit)
}

/// `facing` for blocks with no other rule.
fn facing(ctx: &Ctx, mut edit: Edit) -> Option<Edit> {
    let block = edit.block;
    let name = block.name;
    // Horizontal-only facings ignore up/down.
    let vertical = {
        let (mut up, mut down) = (edit.clone(), edit.clone());
        up.set_facing(Dir::Up).state() != down.set_facing(Dir::Down).state()
    };
    if attached::is_wall_attached(block) {
        return attached::on_wall(ctx, edit);
    }
    let dir = if vertical {
        if name == "observer" {
            ctx.nearest_looking()
        } else if name == "hopper" {
            let d = ctx.face().opposite();
            if d.is_horizontal() { d } else { Dir::Down }
        } else if name == "end_rod" {
            let behind = Edit::of_state(ctx.get(ctx.face().opposite().offset(ctx.pos)));
            if behind.block == block && behind.facing() == Some(ctx.face()) {
                ctx.face().opposite()
            } else {
                ctx.face()
            }
        } else if block.has_tag(&tag::Block::MINECRAFT_SHULKER_BOXES)
            || name.ends_with("lightning_rod")
            || name.contains("amethyst_bud")
            || name == "amethyst_cluster"
        {
            ctx.face()
        } else {
            // Pistons, dispensers, droppers, barrels, command blocks.
            ctx.nearest_looking().opposite()
        }
    } else if block.has_tag(&tag::Block::MINECRAFT_ANVIL) {
        ctx.horizontal().clockwise()
    } else if block.has_tag(&tag::Block::MINECRAFT_CAMPFIRES)
        || name == "decorated_pot"
        || name == "calibrated_sculk_sensor"
    {
        ctx.horizontal()
    } else if name == "big_dripleaf" {
        let below = Edit::of_state(ctx.get(Dir::Down.offset(ctx.pos)));
        match below.block.name {
            "big_dripleaf" | "big_dripleaf_stem" => below.facing()?,
            _ => ctx.horizontal().opposite(),
        }
    } else {
        ctx.horizontal().opposite()
    };
    edit.set_facing(dir);
    Some(edit)
}

/// Signs and banners face the player, skulls away; hanging signs snap to
/// the one above.
fn rotation(ctx: &Ctx, edit: &mut Edit) {
    let name = edit.block.name;
    let yaw = ctx.yaw();
    let segment = if name.ends_with("_skull") || name.ends_with("_head") {
        look::rotation_segment(yaw)
    } else if name.ends_with("_hanging_sign") {
        return attached::ceiling_hanging_sign(ctx, edit);
    } else {
        look::rotation_segment(yaw + 180.0)
    };
    edit.set(Property::Rotation(segment));
}

/// `BellBlock.getStateForPlacement`.
fn bell(ctx: &Ctx, mut edit: Edit) -> Option<Edit> {
    let face = ctx.face();
    let sturdy = |d: Dir, side: Dir| attached::sturdy(ctx.get(d.offset(ctx.pos)), side);
    if face.axis() == pumpkin_data::block_properties::Axis::Y {
        let attach = if face == Dir::Down {
            BellAttachment::Ceiling
        } else {
            BellAttachment::Floor
        };
        edit.set(Property::Attachment(attach))
            .set_facing(ctx.horizontal());
        return attached::survives(ctx, &edit).then_some(edit);
    }
    let double = match face.axis() {
        pumpkin_data::block_properties::Axis::X => {
            sturdy(Dir::West, Dir::East) && sturdy(Dir::East, Dir::West)
        }
        _ => sturdy(Dir::North, Dir::South) && sturdy(Dir::South, Dir::North),
    };
    let attach = if double {
        BellAttachment::DoubleWall
    } else {
        BellAttachment::SingleWall
    };
    edit.set_facing(face.opposite())
        .set(Property::Attachment(attach));
    if attached::survives(ctx, &edit) {
        return Some(edit);
    }
    let attach = if sturdy(Dir::Down, Dir::Up) {
        BellAttachment::Floor
    } else {
        BellAttachment::Ceiling
    };
    edit.set(Property::Attachment(attach));
    attached::survives(ctx, &edit).then_some(edit)
}

/// `DoorBlock.getHinge`: away from solid blocks, toward a door, else by
/// click position.
fn door_hinge(ctx: &Ctx) -> DoorHinge {
    let dir = ctx.horizontal();
    let pos = ctx.pos;
    let above = Dir::Up.offset(pos);
    let (left, right) = (dir.counter_clockwise(), dir.clockwise());
    let full =
        |p: [i32; 3]| BlockStateId::new(ctx.get(p)).is_some_and(|id| id.to_state().is_full_cube());
    let lower_door = |p: [i32; 3]| {
        let edit = Edit::of_state(ctx.get(p));
        edit.block.has_tag(&tag::Block::MINECRAFT_DOORS)
            && edit.has(|p| p == Property::DoubleBlockHalf(DoubleBlockHalf::Lower))
    };
    let balance = -(i32::from(full(left.offset(pos))) + i32::from(full(left.offset(above))))
        + i32::from(full(right.offset(pos)))
        + i32::from(full(right.offset(above)));
    let door_left = lower_door(left.offset(pos));
    let door_right = lower_door(right.offset(pos));
    if door_left && !door_right || balance > 0 {
        return DoorHinge::Right;
    }
    if door_right && !door_left || balance < 0 {
        return DoorHinge::Left;
    }
    let [sx, _, sz] = dir.step();
    let (cx, cz) = ctx.click_xz();
    if (sx >= 0 || cz >= 0.5)
        && (sx <= 0 || cz <= 0.5)
        && (sz >= 0 || cx <= 0.5)
        && (sz <= 0 || cx >= 0.5)
    {
        DoorHinge::Left
    } else {
        DoorHinge::Right
    }
}

/// The direction an `AttachFace` points (`getConnectedDirection`).
pub(crate) fn attach_face_dir(face: AttachFace, facing: Dir) -> Dir {
    match face {
        AttachFace::Ceiling => Dir::Down,
        AttachFace::Floor => Dir::Up,
        AttachFace::Wall => facing,
    }
}

#[cfg(test)]
mod tests;
