//! Blocks that need support: standing/wall pairs (`StandingAndWallBlockItem`),
//! ladders, buttons, levers, lanterns, hooks, cocoa. Each takes the first
//! look direction where `canSurvive` holds.

use pumpkin_data::block_properties::AttachFace;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId, Property};

use super::look::{self, Dir};
use super::{Ctx, Edit, attach_face_dir, state_for};

/// `isFaceSturdy`, full face.
pub(super) fn sturdy(state: u16, side: Dir) -> bool {
    BlockStateId::new(state).is_some_and(|id| id.to_state().is_side_solid(side.block_direction()))
}

/// `canSupportCenter`.
fn center(state: u16, side: Dir) -> bool {
    BlockStateId::new(state).is_some_and(|id| id.to_state().is_center_solid(side.block_direction()))
}

/// `BlockState.isSolid`.
fn solid(state: u16) -> bool {
    BlockStateId::new(state).is_some_and(|id| id.to_state().is_solid())
}

/// Wall form of a standing item, and where the standing form attaches.
pub(super) fn wall_variant(block: &'static Block) -> Option<(&'static Block, Dir)> {
    let name = block.name;
    if let Some(prefix) = name.strip_suffix("hanging_sign") {
        let wall = Block::from_registry_key(&format!("{prefix}wall_hanging_sign"))?;
        return Some((wall, Dir::Up));
    }
    let (prefix, last) = match name.rsplit_once('_') {
        Some((prefix, last)) => (Some(prefix), last),
        None => (None, name),
    };
    if !matches!(last, "torch" | "sign" | "banner" | "skull" | "head" | "fan") {
        return None;
    }
    let wall = match prefix {
        Some(prefix) => format!("{prefix}_wall_{last}"),
        None => format!("wall_{last}"),
    };
    Some((Block::from_registry_key(&wall)?, Dir::Down))
}

/// `StandingAndWallBlockItem.getPlacementState`.
pub(super) fn standing_or_wall(ctx: &Ctx, wall: &'static Block, attachment: Dir) -> Option<Edit> {
    let wall_state = state_for(ctx, wall);
    for dir in ctx.nearest_looking_all() {
        if dir == attachment.opposite() {
            continue;
        }
        let possible = if dir == attachment {
            state_for(ctx, ctx.block)
        } else {
            wall_state.clone()
        };
        if let Some(edit) = possible
            && survives(ctx, &edit)
        {
            return Some(edit);
        }
    }
    None
}

/// Blocks held by the block behind them.
pub(super) fn is_wall_attached(block: &'static Block) -> bool {
    let name = block.name;
    name.ends_with("wall_torch")
        || name.ends_with("_wall_banner")
        || name.ends_with("coral_wall_fan")
        || name.ends_with("_wall_skull")
        || name.ends_with("_wall_head")
        || matches!(name, "ladder" | "tripwire_hook" | "cocoa")
        || block.has_tag(&tag::Block::MINECRAFT_WALL_SIGNS)
        || block.has_tag(&tag::Block::MINECRAFT_WALL_HANGING_SIGNS)
}

/// `WallTorchBlock`, `LadderBlock`, `WallSignBlock` & co.: first horizontal
/// look direction with support.
pub(super) fn on_wall(ctx: &Ctx, mut edit: Edit) -> Option<Edit> {
    let block = edit.block;
    let name = block.name;
    if name == "ladder" && !ctx.replace_clicked {
        let clicked = Edit::of_state(ctx.get(ctx.face().opposite().offset(ctx.pos)));
        if clicked.block == block && clicked.facing() == Some(ctx.face()) {
            return None;
        }
    }
    let hanging = block.has_tag(&tag::Block::MINECRAFT_WALL_HANGING_SIGNS);
    let skull = name.ends_with("_wall_skull") || name.ends_with("_wall_head");
    for dir in ctx.nearest_looking_all() {
        if !dir.is_horizontal() || hanging && dir.axis() == ctx.face().axis() {
            continue;
        }
        edit.set_facing(if name == "cocoa" { dir } else { dir.opposite() });
        let holds = if skull {
            !ctx.replaceable(ctx.get(dir.offset(ctx.pos)))
        } else if hanging {
            hanging_sign_holds(ctx, &edit)
        } else {
            survives(ctx, &edit)
        };
        if holds {
            return Some(edit);
        }
    }
    None
}

/// `WallHangingSignBlock.canPlace`: support or a parallel sign at a side.
fn hanging_sign_holds(ctx: &Ctx, edit: &Edit) -> bool {
    let Some(facing) = edit.facing() else {
        return false;
    };
    let attaches = |side: Dir, face: Dir| {
        let state = ctx.get(side.offset(ctx.pos));
        let other = Edit::of_state(state);
        if other
            .block
            .has_tag(&tag::Block::MINECRAFT_WALL_HANGING_SIGNS)
        {
            other.facing().is_some_and(|f| f.axis() == facing.axis())
        } else {
            sturdy(state, face)
        }
    };
    let (cw, ccw) = (facing.clockwise(), facing.counter_clockwise());
    attaches(cw, ccw) || attaches(ccw, cw)
}

/// `FaceAttachedHorizontalDirectionalBlock`: buttons, levers, grindstones.
pub(super) fn face_attached(ctx: &Ctx, mut edit: Edit) -> Option<Edit> {
    for dir in ctx.nearest_looking_all() {
        if dir.is_horizontal() {
            edit.set(Property::Face(AttachFace::Wall))
                .set_facing(dir.opposite());
        } else {
            let face = if dir == Dir::Up {
                AttachFace::Ceiling
            } else {
                AttachFace::Floor
            };
            edit.set(Property::Face(face)).set_facing(ctx.horizontal());
        }
        if survives(ctx, &edit) {
            return Some(edit);
        }
    }
    None
}

/// `LanternBlock`.
pub(super) fn lantern(ctx: &Ctx, mut edit: Edit) -> Option<Edit> {
    for dir in ctx.nearest_looking_all() {
        if dir.is_horizontal() {
            continue;
        }
        edit.set(Property::Hanging(dir == Dir::Up));
        if survives(ctx, &edit) {
            return Some(edit);
        }
    }
    None
}

/// `CeilingHangingSignBlock.getStateForPlacement`.
pub(super) fn ceiling_hanging_sign(ctx: &Ctx, edit: &mut Edit) {
    let above = ctx.get(Dir::Up.offset(ctx.pos));
    let above_edit = Edit::of_state(above);
    let below_sign = above_edit
        .block
        .has_tag(&tag::Block::MINECRAFT_ALL_HANGING_SIGNS);
    let direction = Dir::from_y_rot(f64::from(ctx.yaw()));
    let mut middle = !sturdy(above, Dir::Down) || ctx.sneaking();
    if below_sign && !ctx.sneaking() {
        let in_line = if above_edit
            .block
            .has_tag(&tag::Block::MINECRAFT_WALL_HANGING_SIGNS)
        {
            above_edit.facing().map(Dir::axis)
        } else {
            above_edit
                .find(|p| match p {
                    Property::Rotation(r) => Some(r),
                    _ => None,
                })
                .and_then(look::segment_direction)
                .map(Dir::axis)
        };
        if in_line == Some(direction.axis()) {
            middle = false;
        }
    }
    let segment = if middle {
        look::rotation_segment(ctx.yaw() + 180.0)
    } else {
        look::direction_segment(direction.opposite())
    };
    edit.set(Property::Attached(middle))
        .set(Property::Rotation(segment));
}

/// `canSurvive` for supported blocks; `true` otherwise.
pub(super) fn survives(ctx: &Ctx, edit: &Edit) -> bool {
    let block = edit.block;
    let name = block.name;
    let at = |d: Dir| ctx.get(d.offset(ctx.pos));
    let facing = edit.facing();
    if let Some(face) = edit.find(|p| match p {
        Property::Face(f) => Some(f),
        _ => None,
    }) {
        let connected = attach_face_dir(face, facing.unwrap_or(Dir::North));
        return sturdy(at(connected.opposite()), connected);
    }
    if let Some(attachment) = edit.find(|p| match p {
        Property::Attachment(a) => Some(a),
        _ => None,
    }) {
        use pumpkin_data::block_properties::BellAttachment;
        let connected = match attachment {
            BellAttachment::Floor => Dir::Up,
            BellAttachment::Ceiling => Dir::Down,
            _ => facing.unwrap_or(Dir::North).opposite(),
        };
        let toward = connected.opposite();
        return if toward == Dir::Up {
            center(at(Dir::Up), Dir::Down)
        } else {
            sturdy(at(toward), toward.opposite())
        };
    }
    if name.ends_with("lantern")
        && let Some(hanging) = edit.find(|p| match p {
            Property::Hanging(h) => Some(h),
            _ => None,
        })
    {
        let connected = if hanging { Dir::Down } else { Dir::Up };
        return center(at(connected.opposite()), connected);
    }
    if is_wall_attached(block) {
        let Some(facing) = facing else {
            return true;
        };
        let behind = at(facing.opposite());
        if name == "cocoa" {
            return Block::from_state_id(BlockStateId::new_or_air(at(facing)))
                .has_tag(&tag::Block::MINECRAFT_SUPPORTS_COCOA);
        }
        if name.ends_with("_wall_banner") || block.has_tag(&tag::Block::MINECRAFT_WALL_SIGNS) {
            return solid(behind);
        }
        if name.ends_with("_wall_skull")
            || name.ends_with("_wall_head")
            || block.has_tag(&tag::Block::MINECRAFT_WALL_HANGING_SIGNS)
        {
            return true;
        }
        return facing.is_horizontal() && sturdy(behind, facing);
    }
    if name.ends_with("torch") {
        return center(at(Dir::Down), Dir::Up);
    }
    if block.has_tag(&tag::Block::MINECRAFT_CEILING_HANGING_SIGNS) {
        return center(at(Dir::Up), Dir::Down);
    }
    if block.has_tag(&tag::Block::MINECRAFT_STANDING_SIGNS) || name.ends_with("_banner") {
        return solid(at(Dir::Down));
    }
    if name.ends_with("_coral") || name.ends_with("_coral_fan") {
        return sturdy(at(Dir::Down), Dir::Up);
    }
    true
}
