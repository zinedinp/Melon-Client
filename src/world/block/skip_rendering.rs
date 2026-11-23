//! `Block.skipRendering`.

use super::direction::{Direction, Directions};
use pumpkin_data::{Block, BlockStateId};
use std::sync::LazyLock;

/// Which `skipRendering` a block has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    /// `Block`'s: none.
    None,
    /// `HalfTransparentBlock`, `PowderSnowBlock`.
    SameBlock,
    /// `IronBarsBlock` (panes and bars).
    Bars {
        /// In `#bars`: joins up with the other bars too.
        tagged: bool,
    },
}

#[derive(Clone, Copy)]
struct Entry {
    /// The block's id.
    block: u16,
    rule: Rule,
    /// Panes and bars: the sides it connects to.
    connects: Directions,
}

const NONE: Entry = Entry {
    block: u16::MAX,
    rule: Rule::None,
    connects: Directions::NONE,
};

static TABLE: LazyLock<Box<[Entry]>> =
    LazyLock::new(|| (0..BlockStateId::COUNT).map(entry).collect());

fn entry(id: u16) -> Entry {
    let Some(state) = BlockStateId::new(id) else {
        return NONE;
    };
    let block = Block::from_state_id(state);
    let rule = if half_transparent(block.name) || block.name == "powder_snow" {
        Rule::SameBlock
    } else if block.name.ends_with("glass_pane") || block.name.ends_with("_bars") {
        Rule::Bars {
            tagged: block.name.ends_with("_bars"),
        }
    } else {
        return NONE;
    };
    let mut connects = Directions::NONE;
    if let Some(props) = block.properties(state) {
        for prop in props.to_props() {
            if prop.value_str() == "true"
                && let Some(side) = Direction::from_name(prop.key_str())
            {
                connects = connects.with(side);
            }
        }
    }
    Entry {
        block: block.id.as_u16(),
        rule,
        connects,
    }
}

/// `HalfTransparentBlock` and its subclasses.
pub fn half_transparent(name: &str) -> bool {
    matches!(
        name,
        "glass"
            | "tinted_glass"
            | "ice"
            | "frosted_ice"
            | "blue_ice"
            | "slime_block"
            | "honey_block"
    ) || name.ends_with("_stained_glass")
        || name.ends_with("copper_grate")
}

/// Whether [`hides`] can be true for `state` at all; `Block`'s never hides.
#[inline]
pub fn has_rule(state: u16) -> bool {
    TABLE
        .get(usize::from(state))
        .is_some_and(|entry| entry.rule != Rule::None)
}

/// Whether `state`'s face towards `side` isn't drawn against `neighbor`.
#[inline]
pub fn hides(state: u16, neighbor: u16, side: Direction) -> bool {
    let (Some(here), Some(next)) = (
        TABLE.get(usize::from(state)),
        TABLE.get(usize::from(neighbor)),
    ) else {
        return false;
    };
    match here.rule {
        Rule::None => false,
        Rule::SameBlock => next.block == here.block,
        Rule::Bars { tagged } => {
            let joins = next.block == here.block
                || tagged && matches!(next.rule, Rule::Bars { tagged: true });
            joins
                && (side.axis() == 1
                    || here.connects.contains(side) && next.connects.contains(side.opposite()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(block: &Block) -> u16 {
        block.default_state.id.as_u16()
    }

    #[test]
    fn ice_hides_against_ice_only() {
        let ice = id(&Block::ICE);
        assert!(hides(ice, ice, Direction::North));
        assert!(!hides(ice, id(&Block::GLASS), Direction::North));
        assert!(!hides(ice, 0, Direction::North));
        assert!(!hides(id(&Block::STONE), id(&Block::STONE), Direction::Up));
        assert!(!hides(
            id(&Block::PACKED_ICE),
            id(&Block::PACKED_ICE),
            Direction::Up
        ));
    }

    #[test]
    fn glass_kinds_stay_apart() {
        let red = id(&Block::RED_STAINED_GLASS);
        assert!(hides(id(&Block::GLASS), id(&Block::GLASS), Direction::Up));
        assert!(hides(red, red, Direction::West));
        assert!(!hides(red, id(&Block::BLUE_STAINED_GLASS), Direction::West));
    }

    /// The pane state connected on exactly `sides`.
    fn pane(block: &Block, sides: &[Direction]) -> u16 {
        block
            .states
            .iter()
            .map(|s| s.id.as_u16())
            .find(|&s| {
                let entry = entry(s);
                Direction::ALL[2..]
                    .iter()
                    .all(|d| entry.connects.contains(*d) == sides.contains(d))
            })
            .expect("pane state")
    }

    #[test]
    fn panes_hide_where_they_join() {
        let east = pane(&Block::GLASS_PANE, &[Direction::East]);
        let west = pane(&Block::GLASS_PANE, &[Direction::West]);
        let lone = pane(&Block::GLASS_PANE, &[]);
        assert!(hides(east, west, Direction::East));
        assert!(!hides(east, lone, Direction::East));
        assert!(hides(lone, lone, Direction::Up));
        let red = pane(&Block::RED_STAINED_GLASS_PANE, &[Direction::West]);
        assert!(!hides(east, red, Direction::East));
    }

    #[test]
    fn bars_join_across_kinds() {
        let iron = pane(&Block::IRON_BARS, &[Direction::East]);
        let copper = pane(&Block::COPPER_BARS, &[Direction::West]);
        assert!(hides(iron, copper, Direction::East));
        assert!(!hides(
            iron,
            pane(&Block::GLASS_PANE, &[Direction::West]),
            Direction::East
        ));
    }
}
