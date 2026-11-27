//! Vanilla `BlockBehaviour.OffsetType`.

use std::sync::LazyLock;

use pumpkin_data::{Block, BlockStateId};

use crate::world::block_models::variants::seed;

/// How a block is moved.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Offset {
    /// `OffsetType.XYZ`: also down, by up to `max_vertical`.
    vertical: bool,
    max_horizontal: f32,
    max_vertical: f32,
}

/// `getMaxHorizontalOffset`, `getMaxVerticalOffset` defaults.
const PLANT_XZ: Offset = Offset {
    vertical: false,
    max_horizontal: 0.25,
    max_vertical: 0.2,
};
const PLANT_XYZ: Offset = Offset {
    vertical: true,
    ..PLANT_XZ
};
/// `SpeleothemBlock`: as far as its base column (12 wide) still fits.
const SPELEOTHEM: Offset = Offset {
    max_horizontal: 0.125,
    ..PLANT_XZ
};
/// `SmallDripleafBlock.getMaxVerticalOffset`.
const SMALL_DRIPLEAF: Offset = Offset {
    max_vertical: 0.1,
    ..PLANT_XYZ
};

/// The blocks with `.offsetType(...)` in `Blocks` (or copying one that has).
fn offset_blocks() -> [(&'static Block, Offset); 39] {
    [
        (&Block::MANGROVE_PROPAGULE, PLANT_XZ),
        (&Block::SHORT_GRASS, PLANT_XYZ),
        (&Block::FERN, PLANT_XYZ),
        (&Block::SHORT_DRY_GRASS, PLANT_XYZ),
        (&Block::TALL_DRY_GRASS, PLANT_XYZ),
        (&Block::TALL_SEAGRASS, PLANT_XZ),
        (&Block::DANDELION, PLANT_XZ),
        (&Block::GOLDEN_DANDELION, PLANT_XZ),
        (&Block::TORCHFLOWER, PLANT_XZ),
        (&Block::POPPY, PLANT_XZ),
        (&Block::BLUE_ORCHID, PLANT_XZ),
        (&Block::ALLIUM, PLANT_XZ),
        (&Block::AZURE_BLUET, PLANT_XZ),
        (&Block::RED_TULIP, PLANT_XZ),
        (&Block::ORANGE_TULIP, PLANT_XZ),
        (&Block::WHITE_TULIP, PLANT_XZ),
        (&Block::PINK_TULIP, PLANT_XZ),
        (&Block::OXEYE_DAISY, PLANT_XZ),
        (&Block::CORNFLOWER, PLANT_XZ),
        (&Block::WITHER_ROSE, PLANT_XZ),
        (&Block::LILY_OF_THE_VALLEY, PLANT_XZ),
        (&Block::SUNFLOWER, PLANT_XZ),
        (&Block::LILAC, PLANT_XZ),
        (&Block::ROSE_BUSH, PLANT_XZ),
        (&Block::PEONY, PLANT_XZ),
        (&Block::TALL_GRASS, PLANT_XZ),
        (&Block::LARGE_FERN, PLANT_XZ),
        (&Block::PITCHER_PLANT, PLANT_XZ),
        (&Block::BAMBOO_SAPLING, PLANT_XZ),
        (&Block::BAMBOO, PLANT_XZ),
        (&Block::WARPED_ROOTS, PLANT_XZ),
        (&Block::NETHER_SPROUTS, PLANT_XZ),
        (&Block::CRIMSON_ROOTS, PLANT_XZ),
        (&Block::POINTED_DRIPSTONE, SPELEOTHEM),
        (&Block::SULFUR_SPIKE, SPELEOTHEM),
        (&Block::SMALL_DRIPLEAF, SMALL_DRIPLEAF),
        (&Block::HANGING_ROOTS, PLANT_XZ),
        (&Block::OPEN_EYEBLOSSOM, PLANT_XZ),
        (&Block::CLOSED_EYEBLOSSOM, PLANT_XZ),
    ]
}

/// Per block state: index into [`KINDS`] plus one, 0 for none.
static BY_STATE: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut table = vec![0u8; usize::from(BlockStateId::COUNT)];
    for (block, offset) in offset_blocks() {
        let kind = KINDS
            .iter()
            .position(|&k| k == offset)
            .expect("listed below") as u8
            + 1;
        for state in block.states {
            table[usize::from(state.id.as_u16())] = kind;
        }
    }
    table
});

/// The kinds by number minus one.
const KINDS: [Offset; 4] = [PLANT_XZ, PLANT_XYZ, SPELEOTHEM, SMALL_DRIPLEAF];

fn kind_of(state: u16) -> Option<Offset> {
    let kind = *BY_STATE.get(usize::from(state))?;
    (kind != 0).then(|| KINDS[usize::from(kind - 1)])
}

/// Whether blocks of `state` are moved off centre.
pub fn has_offset(state: u16) -> bool {
    kind_of(state).is_some()
}

/// `BlockState.getOffset` for `state` in column `(x, z)`.
pub fn offset(state: u16, x: i32, z: i32) -> [f32; 3] {
    kind_of(state).map_or([0.0; 3], |kind| column_offset(kind, x, z))
}

/// `state`'s offset in column `(x, z)`, packed.
pub fn packed(state: u16, x: i32, z: i32) -> u16 {
    let kind = *BY_STATE.get(usize::from(state)).unwrap_or(&0);
    if kind == 0 {
        return 0;
    }
    u16::from(kind) << 12 | (seed(x, 0, z) & 0xFFF) as u16
}

/// The offset [`packed`] stands for, as the vertex shader works it out.
pub fn unpack(packed: u16) -> [f32; 3] {
    let kind = packed >> 12;
    if kind == 0 {
        return [0.0; 3];
    }
    nibble_offset(KINDS[usize::from(kind - 1)], i64::from(packed & 0xFFF))
}

fn column_offset(kind: Offset, x: i32, z: i32) -> [f32; 3] {
    nibble_offset(kind, seed(x, 0, z))
}

fn nibble_offset(kind: Offset, seed: i64) -> [f32; 3] {
    let nibble = |shift: u32| f64::from(((seed >> shift) & 15) as f32 / 15.0);
    let max = f64::from(kind.max_horizontal);
    let horizontal = |shift| ((nibble(shift) - 0.5) * 0.5).clamp(-max, max) as f32;
    let y = if kind.vertical {
        ((nibble(4) - 1.0) * f64::from(kind.max_vertical)) as f32
    } else {
        0.0
    };
    [horizontal(0), y, horizontal(8)]
}

/// [`offset`] minus the one at column `(0, 0)`.
pub fn shape_delta(state: u16, x: i32, z: i32) -> glam::DVec3 {
    let Some(kind) = kind_of(state) else {
        return glam::DVec3::ZERO;
    };
    let here = column_offset(kind, x, z);
    let origin = column_offset(kind, 0, 0);
    glam::DVec3::from_array(std::array::from_fn(|i| {
        f64::from(here[i]) - f64::from(origin[i])
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_of(block: &Block) -> u16 {
        block.default_state.id.as_u16()
    }

    #[test]
    fn plants_move_within_their_limits_and_blocks_stay() {
        let grass = state_of(&Block::SHORT_GRASS);
        let dripstone = state_of(&Block::POINTED_DRIPSTONE);
        assert_eq!(offset(state_of(&Block::STONE), 5, 9), [0.0; 3]);
        let mut moved = false;
        for x in -20..20 {
            for z in -20..20 {
                let [ox, oy, oz] = offset(grass, x, z);
                assert!(ox.abs() <= 0.25 && oz.abs() <= 0.25 && (-0.2..=0.0).contains(&oy));
                moved |= ox != 0.0 || oz != 0.0;
                let [dx, dy, dz] = offset(dripstone, x, z);
                assert!(dx.abs() <= 0.125 && dz.abs() <= 0.125 && dy == 0.0);
            }
        }
        assert!(moved);
        assert_eq!(offset(grass, 0, 0), [-0.25, -0.2, -0.25]);
        assert_eq!(shape_delta(grass, 0, 0), glam::DVec3::ZERO);
    }

    #[test]
    fn shapes_move_like_pumpkins() {
        use pumpkin_util::math::position::BlockPos;
        for block in [
            &Block::SHORT_GRASS,
            &Block::BAMBOO,
            &Block::POINTED_DRIPSTONE,
            &Block::SMALL_DRIPLEAF,
            &Block::POPPY,
        ] {
            let state = block.default_state;
            let id = state.id.as_u16();
            let plain: Vec<_> = state.get_block_outline_shapes().collect();
            for (x, z) in [(0, 0), (3, -8), (-1234, 98765), (77, 13), (-5, -5)] {
                let moved: Vec<_> = state
                    .get_block_outline_shapes_at(&BlockPos::new(x, 64, z))
                    .collect();
                let delta = shape_delta(id, x, z);
                for (a, b) in plain.iter().zip(&moved) {
                    let ours = a.min.x + delta.x;
                    assert!((ours - b.min.x).abs() < 1e-6, "{} at {x} {z}", block.name);
                    assert!((a.min.y + delta.y - b.min.y).abs() < 1e-6, "{}", block.name);
                    assert!((a.min.z + delta.z - b.min.z).abs() < 1e-6, "{}", block.name);
                }
            }
        }
    }

    #[test]
    fn packed_offsets_unpack_to_the_same() {
        for block in [
            &Block::SHORT_GRASS,
            &Block::POINTED_DRIPSTONE,
            &Block::SMALL_DRIPLEAF,
            &Block::POPPY,
        ] {
            let id = state_of(block);
            for (x, z) in [(0, 0), (3, -8), (-1234, 98765), (77, 13)] {
                assert_eq!(unpack(packed(id, x, z)), offset(id, x, z), "{}", block.name);
            }
        }
        assert_eq!(packed(state_of(&Block::STONE), 5, 9), 0);
    }

    #[test]
    fn both_halves_of_a_tall_plant_share_the_offset() {
        let sunflower = &Block::SUNFLOWER;
        let [lower, upper] = [sunflower.states[0].id, sunflower.states[1].id].map(|id| id.as_u16());
        assert_eq!(offset(lower, 13, -7), offset(upper, 13, -7));
    }
}
