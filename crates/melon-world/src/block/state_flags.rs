//! Per-block-state flags the mesher and the culling ask for every block.

use pumpkin_data::BlockStateId;

const STATES: usize = BlockStateId::COUNT as usize;
const WORDS: usize = STATES.div_ceil(64);

/// A set of block states, one bit each.
struct StateBits([u64; WORDS]);

/// Builds a [`StateBits`] holding every state `rule` accepts.
macro_rules! state_bits {
    ($rule:path) => {{
        let mut words = [0u64; WORDS];
        let mut id = 0;
        while id < STATES {
            if let Some(state) = BlockStateId::new(id as u16)
                && $rule(state)
            {
                words[id / 64] |= 1 << (id % 64);
            }
            id += 1;
        }
        StateBits(words)
    }};
}

impl StateBits {
    #[inline]
    fn contains(&self, id: u16) -> bool {
        let id = id as usize;
        self.0
            .get(id / 64)
            .is_some_and(|word| word >> (id % 64) & 1 != 0)
    }
}

/// Vanilla `BlockState.isSolidRender`: a full, opaque cube.
const fn rule_solid(state: BlockStateId) -> bool {
    state.is_solid_render()
}

/// Whether a state may have anything to draw: everything but air.
const fn rule_drawn(state: BlockStateId) -> bool {
    !state.is_air()
}

static SOLID: StateBits = state_bits!(rule_solid);
static DRAWN: StateBits = state_bits!(rule_drawn);

/// Vanilla `instanceof LeavesBlock`: every `*_leaves` block.
fn rule_leaves(state: BlockStateId) -> bool {
    pumpkin_data::Block::from_state_id(state)
        .name
        .ends_with("_leaves")
}

static LEAVES: std::sync::LazyLock<StateBits> =
    std::sync::LazyLock::new(|| state_bits!(rule_leaves));

fn extra_state(id: u16) -> Option<&'static pumpkin_data::BlockState> {
    if id < BlockStateId::COUNT {
        return None;
    }
    BlockStateId::new(id).map(pumpkin_data::BlockStateId::to_state)
}

/// See [`rule_solid`].
#[inline]
pub fn is_solid(id: u16) -> bool {
    SOLID.contains(id) || extra_state(id).is_some_and(pumpkin_data::BlockState::is_solid_render)
}

/// See [`rule_drawn`].
#[inline]
pub fn is_drawn(id: u16) -> bool {
    DRAWN.contains(id) || extra_state(id).is_some_and(|state| !state.is_air())
}

/// See [`rule_leaves`].
#[inline]
pub fn is_leaves(id: u16) -> bool {
    LEAVES.contains(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_opaque_cubes_are_solid() {
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        let glass = pumpkin_data::Block::GLASS.default_state.id.as_u16();
        let ice = pumpkin_data::Block::ICE.default_state.id.as_u16();
        let short_grass = pumpkin_data::Block::SHORT_GRASS.default_state.id.as_u16();
        assert!(is_solid(stone) && is_drawn(stone));
        assert!(!is_solid(0) && !is_drawn(0));
        assert!(!is_solid(glass) && is_drawn(glass));
        assert!(!is_solid(ice) && is_drawn(ice));
        assert!(!is_solid(short_grass) && is_drawn(short_grass));
        assert!(!is_solid(u16::MAX));
    }

    #[test]
    fn leaves_are_the_leaves_blocks() {
        use pumpkin_data::Block;
        for leaves in [
            &Block::OAK_LEAVES,
            &Block::CHERRY_LEAVES,
            &Block::AZALEA_LEAVES,
        ] {
            for state in leaves.states {
                assert!(is_leaves(state.id.as_u16()), "{}", leaves.name);
            }
        }
        for other in [&Block::OAK_LOG, &Block::GRASS_BLOCK, &Block::AIR] {
            assert!(
                !is_leaves(other.default_state.id.as_u16()),
                "{}",
                other.name
            );
        }
        assert!(!is_leaves(u16::MAX));
    }

    #[test]
    fn mod_blocks_are_solid_as_their_spec_says() {
        use pumpkin_data::mod_registry::{BlockSpec, register_block};
        let cube = register_block("melon_test:cube", &BlockSpec::default()).unwrap();
        let cube = cube.default_state.id.as_u16();
        assert!(cube >= BlockStateId::COUNT);
        assert!(is_solid(cube) && is_drawn(cube));
        let pane: BlockSpec =
            serde_json::from_str(r#"{"render": {"solid": false, "layer": "cutout"}}"#).unwrap();
        let pane = register_block("melon_test:pane", &pane).unwrap();
        let pane = pane.default_state.id.as_u16();
        assert!(!is_solid(pane) && is_drawn(pane));
        let unregistered = BlockStateId::COUNT + pumpkin_data::mod_registry::STATE_CAP - 1;
        assert!(!is_solid(unregistered) && !is_drawn(unregistered));
    }
}
