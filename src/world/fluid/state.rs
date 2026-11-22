//! `FluidState` without its block: kind, amount and whether it falls.

use std::sync::LazyLock;

use pumpkin_data::{Block, BlockStateId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FluidKind {
    Water,
    Lava,
}

/// `FluidState` without its block.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FluidState {
    pub kind: FluidKind,
    pub amount: u8,
    pub falling: bool,
}

impl FluidState {
    /// `FluidState.getOwnHeight`.
    pub fn own_height(self) -> f32 {
        f32::from(self.amount) / 9.0
    }
}

/// Packed per block state.
static FLUIDS: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut table = vec![0u8; usize::from(BlockStateId::COUNT)];
    let mut fill = |block: &Block, kind: u8| {
        for (level, state) in block.states.iter().enumerate() {
            let (amount, falling) = match level {
                0 => (8, false),
                1..=7 => (8 - level as u8, false),
                _ => (8, true),
            };
            table[usize::from(state.id.as_u16())] = kind | amount | if falling { 0x80 } else { 0 };
        }
    };
    fill(&Block::WATER, 0x10);
    fill(&Block::LAVA, 0x20);
    for block in [
        &Block::KELP,
        &Block::KELP_PLANT,
        &Block::SEAGRASS,
        &Block::TALL_SEAGRASS,
        &Block::BUBBLE_COLUMN,
    ] {
        for state in block.states {
            table[usize::from(state.id.as_u16())] = 0x10 | 8;
        }
    }
    for id in 0..BlockStateId::COUNT {
        if let Some(state) = BlockStateId::new(id)
            && table[usize::from(id)] == 0
            && state.is_waterlogged()
        {
            table[usize::from(id)] = 0x10 | 8;
        }
    }
    table
});

/// The fluid in a block state, if any.
pub fn fluid_of(state: u16) -> Option<FluidState> {
    let packed = *FLUIDS.get(usize::from(state))?;
    let kind = match packed & 0x30 {
        0x10 => FluidKind::Water,
        0x20 => FluidKind::Lava,
        _ => return None,
    };
    Some(FluidState {
        kind,
        amount: packed & 0x0F,
        falling: packed & 0x80 != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_waterlogging() {
        let source = fluid_of(Block::WATER.default_state.id.as_u16()).unwrap();
        assert_eq!(
            (source.kind, source.amount, source.falling),
            (FluidKind::Water, 8, false)
        );
        let first = Block::WATER.default_state.id.as_u16();
        assert_eq!(fluid_of(first + 3).unwrap().amount, 5);
        assert!(fluid_of(first + 9).unwrap().falling);
        assert_eq!(
            fluid_of(Block::LAVA.default_state.id.as_u16())
                .unwrap()
                .kind,
            FluidKind::Lava
        );
        assert!(fluid_of(Block::STONE.default_state.id.as_u16()).is_none());
        assert!(fluid_of(0).is_none());
        let waterlogged = Block::OAK_STAIRS
            .states
            .iter()
            .find(|s| s.id.is_waterlogged())
            .unwrap();
        assert_eq!(fluid_of(waterlogged.id.as_u16()).unwrap().amount, 8);
    }
}
