//! `StairBlock`: half by click height, corners with neighbours (both ways).

use pumpkin_data::Property;
use pumpkin_data::block_properties::{Half, StairsShape};

use super::look::Dir;
use super::{Ctx, Edit};

/// Facing and half of a stairs state.
fn stairs(state: u16) -> Option<(Dir, Half)> {
    let edit = Edit::of_state(state);
    let half = edit.find(|p| match p {
        Property::Half(h) => Some(h),
        _ => None,
    })?;
    edit.has(|p| matches!(p, Property::StairsShape(_)))
        .then_some(())?;
    Some((edit.facing()?, half))
}

/// `getStairsShape` for stairs at `pos` facing `facing` with `half`.
fn shape(get: &dyn Fn([i32; 3]) -> u16, pos: [i32; 3], facing: Dir, half: Half) -> StairsShape {
    let can_take =
        |toward: Dir| stairs(get(toward.offset(pos))).is_none_or(|(f, h)| f != facing || h != half);
    if let Some((behind, h)) = stairs(get(facing.offset(pos)))
        && h == half
        && behind.axis() != facing.axis()
        && can_take(behind.opposite())
    {
        return if behind == facing.counter_clockwise() {
            StairsShape::OuterLeft
        } else {
            StairsShape::OuterRight
        };
    }
    if let Some((front, h)) = stairs(get(facing.opposite().offset(pos)))
        && h == half
        && front.axis() != facing.axis()
        && can_take(front)
    {
        return if front == facing.counter_clockwise() {
            StairsShape::InnerLeft
        } else {
            StairsShape::InnerRight
        };
    }
    StairsShape::Straight
}

/// `StairBlock.getStateForPlacement`.
pub(super) fn place(ctx: &Ctx, edit: &mut Edit) {
    let facing = ctx.horizontal();
    let top = ctx.face() == Dir::Down || ctx.face() != Dir::Up && ctx.click_y() > 0.5;
    let half = if top { Half::Top } else { Half::Bottom };
    let get = |p| ctx.get(p);
    let shape = shape(&get, ctx.pos, facing, half);
    edit.set_facing(facing)
        .set(Property::Half(half))
        .set(Property::StairsShape(shape));
}

/// Neighbouring stairs reshaped around the new `state` (`updateShape`).
pub(super) fn neighbour_updates(ctx: &Ctx, pos: [i32; 3], state: u16) -> Vec<([i32; 3], u16)> {
    let get = |p: [i32; 3]| if p == pos { state } else { ctx.get(p) };
    Dir::HORIZONTAL
        .iter()
        .filter_map(|d| {
            let at = d.offset(pos);
            let old = ctx.get(at);
            let (facing, half) = stairs(old)?;
            let mut edit = Edit::of_state(old);
            edit.set(Property::StairsShape(shape(&get, at, facing, half)));
            let new = edit.state();
            (new != old).then_some((at, new))
        })
        .collect()
}
