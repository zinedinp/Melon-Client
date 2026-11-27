//! `ChestBlock` for all `ChestLikeProperties` blocks (wooden, trapped,
//! copper): joins a same-facing neighbour, or the clicked one when sneaking.

use pumpkin_data::block_properties::ChestType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, Property};

use super::look::Dir;
use super::{Ctx, Edit};

fn chest_type(edit: &Edit) -> Option<ChestType> {
    edit.find(|p| match p {
        Property::ChestType(t) => Some(t),
        _ => None,
    })
}

/// `chestCanConnectTo`: same block, or copper with copper.
fn connects(block: &'static Block, other: &'static Block) -> bool {
    let copper = &tag::Block::MINECRAFT_COPPER_CHESTS;
    other == block || block.has_tag(copper) && other.has_tag(copper)
}

/// `candidatePartnerFacing`.
fn partner_facing(ctx: &Ctx, toward: Dir) -> Option<Dir> {
    let other = Edit::of_state(ctx.get(toward.offset(ctx.pos)));
    (connects(ctx.block, other.block) && chest_type(&other) == Some(ChestType::Single))
        .then(|| other.facing())
        .flatten()
}

/// `ChestBlock.getStateForPlacement`.
pub(super) fn place(ctx: &Ctx, edit: &mut Edit) {
    let mut kind = ChestType::Single;
    let mut facing = ctx.horizontal().opposite();
    let face = ctx.face();
    let sneaking = ctx.sneaking();
    if face.is_horizontal()
        && sneaking
        && let Some(partner) = partner_facing(ctx, face.opposite())
        && partner.axis() != face.axis()
    {
        facing = partner;
        kind = if partner.counter_clockwise() == face.opposite() {
            ChestType::Right
        } else {
            ChestType::Left
        };
    }
    if kind == ChestType::Single && !sneaking {
        if partner_facing(ctx, facing.clockwise()) == Some(facing) {
            kind = ChestType::Left;
        } else if partner_facing(ctx, facing.counter_clockwise()) == Some(facing) {
            kind = ChestType::Right;
        }
    }
    edit.set_facing(facing).set(Property::ChestType(kind));
}

/// `ChestBlock.updateShape`: the joined chest becomes the other half.
pub(super) fn partner_update(ctx: &Ctx, edit: &Edit) -> Option<([i32; 3], u16)> {
    let facing = edit.facing()?;
    let toward = match chest_type(edit)? {
        ChestType::Single => return None,
        ChestType::Left => facing.clockwise(),
        ChestType::Right => facing.counter_clockwise(),
    };
    let pos = toward.offset(ctx.pos);
    let mut other = Edit::of_state(ctx.get(pos));
    if !connects(edit.block, other.block)
        || chest_type(&other) != Some(ChestType::Single)
        || other.facing() != Some(facing)
    {
        return None;
    }
    let kind = match chest_type(edit)? {
        ChestType::Left => ChestType::Right,
        _ => ChestType::Left,
    };
    other.set(Property::ChestType(kind));
    Some((pos, other.state()))
}
