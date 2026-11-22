//! `SulfurCubeRenderer`: a translucent shell round a core (or round the
//! block it swallowed), sized like a slime and swelling like lit TNT once
//! primed. Babies have their own smaller models.

use glam::Vec3;

use pumpkin_data::Block;

use super::tnt;
use super::{LayerSpec, ModelId as M};
use crate::network::play::game::EquipmentSlot;
use crate::world::entity::model::{LayerDefinition, PartDefinition};
use crate::world::entity::tracker::ClientEntity;
use crate::world::model_part::{Cube, Pose};

pub const OUTER: &str = "minecraft:entity/sulfur_cube/sulfur_cube_outer";
pub const OUTER_SMALL: &str = "minecraft:entity/sulfur_cube/sulfur_cube_outer_small";
pub const INNER: &str = "minecraft:entity/sulfur_cube/sulfur_cube_inner";
pub const INNER_SMALL: &str = "minecraft:entity/sulfur_cube/sulfur_cube_inner_small";

fn cube(uv: (f32, f32), half: f32, texture: f32) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "cube",
        vec![Cube::new(
            uv,
            (-half, -half, -half),
            (half * 2.0, half * 2.0, half * 2.0),
        )],
        Pose::ZERO,
    );
    LayerDefinition::new(root, texture, texture)
}

/// `SulfurCubeModel.createOuterBodyLayer`.
pub fn outer() -> LayerDefinition {
    cube((0.0, 0.0), 9.0, 128.0)
}

/// `SulfurCubeModel.createInnerBodyLayer`.
pub fn inner() -> LayerDefinition {
    cube((0.0, 36.0), 8.0, 128.0)
}

/// `SmallSulfurCubeModel.createOuterBodyLayer`.
pub fn outer_small() -> LayerDefinition {
    cube((0.0, 0.0), 5.0, 64.0)
}

/// `SmallSulfurCubeModel.createInnerBodyLayer`.
pub fn inner_small() -> LayerDefinition {
    cube((0.0, 20.0), 4.0, 64.0)
}

/// The block state a sulfur cube swallowed (its body armor slot, a
/// block item, `BlockItemStateProperties` aside), if it is not a head.
pub fn swallowed_block(entity: &ClientEntity) -> Option<u16> {
    let stack = entity.equipment.get(EquipmentSlot::Body);
    if stack.is_empty() {
        return None;
    }
    let block = Block::from_item_id(stack.item.id)?;
    // `AbstractSkullBlock`: the heads and skulls are worn, not swallowed.
    if block.name.ends_with("_head") || block.name.ends_with("_skull") {
        return None;
    }
    Some(block.default_state.id.as_u16())
}

/// `SulfurCubeRenderState.fuseRemainingTicks`: `getFuse() - partial + 1`
/// while primed, 0 otherwise.
pub fn fuse_remaining(fuse: i32, partial: f32) -> f32 {
    if fuse >= 0 {
        fuse as f32 - partial + 1.0
    } else {
        0.0
    }
}

/// `SulfurCubeRenderer.scale` after the body's flip: the scale, and the
/// offset taken in the scaled space before the model's 1.501 drop.
///
/// Vanilla goes `scale(0.999)`, `translate(0, 0.001, 0)`, the size and
/// squish, the fuse's swell, a half for adults, then up by `vOffset`
/// less a pixel when visible; the early translate is carried through the
/// later scales so only one offset is left.
pub fn placement(
    size: f32,
    squish: f32,
    holds_block: bool,
    fuse: f32,
    baby: bool,
    invisible: bool,
) -> (Vec3, Vec3) {
    let ss = if holds_block {
        0.0
    } else {
        squish / (size * 0.5 + 1.0)
    };
    let w = 1.0 / (ss + 1.0);
    let swell = if fuse < 10.0 && fuse > 0.0 {
        1.0 + tnt::swell_amount(fuse)
    } else {
        1.0
    };
    let extra = if baby { 1.0 } else { 0.5 };
    let after = Vec3::new(w * size, size / w, w * size) * swell * extra;
    let v_offset = if baby { 1.24 } else { 0.98 };
    let pixel = if invisible { 0.0 } else { 1.0 / 16.0 };
    let offset = Vec3::new(0.0, 0.001 / after.y + v_offset - pixel, 0.0);
    (after * 0.999, offset)
}

/// `SulfurCubeInnerLayer`: the core flashes white while the fuse is lit.
pub fn core_flashes(fuse: f32) -> bool {
    fuse > 0.0 && tnt::is_lit(fuse)
}

/// `SulfurCubeRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    let fuse = fuse_remaining(entity.fuse_ticks, 0.0);
    let holds_block = swallowed_block(entity).is_some();
    if !holds_block && !a.invisible() {
        let inner = if baby {
            plain(M::SulfurCubeSmallInner, INNER_SMALL)
        } else {
            plain(M::SulfurCubeInner, INNER)
        }
        .translucent();
        out.push(if core_flashes(fuse) {
            inner.with_overlay(-1.0)
        } else {
            inner
        });
    }
    out.push(
        if baby {
            plain(M::SulfurCubeSmallOuter, OUTER_SMALL)
        } else {
            plain(M::SulfurCubeOuter, OUTER)
        }
        .translucent(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unlit_adult_is_halved_and_raised() {
        let (scale, offset) = placement(2.0, 0.0, false, 0.0, false, false);
        assert!((scale.y - 0.999).abs() < 1e-6);
        assert!((offset.y - (0.001 + 0.98 - 1.0 / 16.0)).abs() < 1e-6);
    }

    #[test]
    fn the_fuse_counts_down_from_the_synced_one() {
        assert_eq!(fuse_remaining(-1, 0.5), 0.0);
        assert_eq!(fuse_remaining(5, 0.5), 5.5);
    }
}
