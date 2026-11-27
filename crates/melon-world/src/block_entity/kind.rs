//! Which block states draw a block entity, and as what.

use super::chest_texture;
use super::models::{ChestHalf, SkullType};
use crate::world::block::direction::Direction;
use pumpkin_data::{Block, BlockStateId};
use std::sync::LazyLock;

/// What a block entity state looks like.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Chest {
        /// `entity/chest/<texture>[_left|_right]`.
        texture: &'static str,
        half: ChestHalf,
        /// The full atlas id of `texture` and `half`.
        sprite: String,
        facing: Direction,
    },
    Shulker {
        /// Atlas id, `entity/shulker/shulker[_<color>]`.
        texture: String,
        facing: Direction,
    },
    Banner {
        /// `DyeColor` id.
        color: usize,
        placement: Placement,
    },
    Skull {
        skull: SkullType,
        placement: Placement,
        powered: bool,
    },
    Bell,
    EnchantingTable,
    DecoratedPot {
        facing: Direction,
    },
    Conduit,
    EndPortal,
    EndGateway,
}

/// Standing at one of 16 rotations, or hung on a wall facing away from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    Ground(u8),
    Wall(Direction),
}

/// `DyeColor`, by id.
pub(super) const DYES: [(&str, u32); 16] = [
    ("white", 0xF9FFFE),
    ("orange", 0xF9801D),
    ("magenta", 0xC74EBD),
    ("light_blue", 0x3AB3DA),
    ("yellow", 0xFED83D),
    ("lime", 0x80C71F),
    ("pink", 0xF38BAA),
    ("gray", 0x474F52),
    ("light_gray", 0x9D9D97),
    ("cyan", 0x169C9C),
    ("purple", 0x8932B8),
    ("blue", 0x3C44AA),
    ("brown", 0x835432),
    ("green", 0x5E7C16),
    ("red", 0xB02E26),
    ("black", 0x1D1D21),
];

/// Reads the kind of one state from its block's name and properties.
pub(super) fn classify(state: BlockStateId) -> Option<Kind> {
    let block = Block::from_state_id(state);
    let name = block.name;
    let props: Vec<(&str, &str)> = block
        .properties(state)
        .map(|p| {
            p.to_props()
                .into_iter()
                .map(|prop| (prop.key_str(), prop.value_str()))
                .collect()
        })
        .unwrap_or_default();
    let prop = |key: &str| props.iter().find(|&&(k, _)| k == key).map(|&(_, v)| v);
    let facing = || prop("facing").and_then(Direction::from_name);
    let placement = || match prop("rotation") {
        Some(rotation) => Some(Placement::Ground(rotation.parse().ok()?)),
        None => Some(Placement::Wall(facing()?)),
    };

    let chest_texture = match name {
        "chest" => Some("normal"),
        "trapped_chest" => Some("trapped"),
        "ender_chest" => Some("ender"),
        "copper_chest" | "waxed_copper_chest" => Some("copper"),
        "exposed_copper_chest" | "waxed_exposed_copper_chest" => Some("copper_exposed"),
        "weathered_copper_chest" | "waxed_weathered_copper_chest" => Some("copper_weathered"),
        "oxidized_copper_chest" | "waxed_oxidized_copper_chest" => Some("copper_oxidized"),
        _ => None,
    };
    if let Some(texture) = chest_texture {
        let half = match prop("type") {
            Some("left") => ChestHalf::Left,
            Some("right") => ChestHalf::Right,
            _ => ChestHalf::Single,
        };
        return Some(Kind::Chest {
            texture,
            half,
            sprite: self::chest_texture(texture, half),
            facing: facing()?,
        });
    }
    if name == "shulker_box" || name.ends_with("_shulker_box") {
        let texture = match name.strip_suffix("_shulker_box") {
            Some(color) => format!("minecraft:entity/shulker/shulker_{color}"),
            None => "minecraft:entity/shulker/shulker".to_owned(),
        };
        return Some(Kind::Shulker {
            texture,
            facing: facing()?,
        });
    }
    if let Some(color) = name
        .strip_suffix("_wall_banner")
        .or_else(|| name.strip_suffix("_banner"))
    {
        let color = DYES.iter().position(|&(dye, _)| dye == color)?;
        return Some(Kind::Banner {
            color,
            placement: placement()?,
        });
    }
    let skull = match name {
        "skeleton_skull" | "skeleton_wall_skull" => Some(SkullType::Skeleton),
        "wither_skeleton_skull" | "wither_skeleton_wall_skull" => Some(SkullType::WitherSkeleton),
        "zombie_head" | "zombie_wall_head" => Some(SkullType::Zombie),
        "creeper_head" | "creeper_wall_head" => Some(SkullType::Creeper),
        "dragon_head" | "dragon_wall_head" => Some(SkullType::Dragon),
        "piglin_head" | "piglin_wall_head" => Some(SkullType::Piglin),
        "player_head" | "player_wall_head" => Some(SkullType::Player),
        _ => None,
    };
    if let Some(skull) = skull {
        return Some(Kind::Skull {
            skull,
            placement: placement()?,
            powered: prop("powered") == Some("true"),
        });
    }
    Some(match name {
        "bell" => Kind::Bell,
        "enchanting_table" => Kind::EnchantingTable,
        "decorated_pot" => Kind::DecoratedPot { facing: facing()? },
        "conduit" => Kind::Conduit,
        "end_portal" => Kind::EndPortal,
        "end_gateway" => Kind::EndGateway,
        _ => return None,
    })
}

pub(super) static KINDS: LazyLock<Vec<Option<Kind>>> = LazyLock::new(|| {
    (0..BlockStateId::COUNT)
        .map(|id| BlockStateId::new(id).and_then(classify))
        .collect()
});

/// The block entity drawn for a state, if any.
pub fn kind_of(state: u16) -> Option<&'static Kind> {
    KINDS.get(usize::from(state))?.as_ref()
}

/// One bit per block state: whether [`kind_of`] has a block entity for it.
pub struct KindStates(Box<[u64]>);

impl KindStates {
    #[inline]
    pub fn contains(&self, state: u16) -> bool {
        let i = usize::from(state);
        self.0
            .get(i / 64)
            .is_some_and(|word| word >> (i % 64) & 1 != 0)
    }
}

pub static KIND_STATES: LazyLock<KindStates> = LazyLock::new(|| {
    let mut bits = vec![0u64; KINDS.len().div_ceil(64)];
    for (i, kind) in KINDS.iter().enumerate() {
        if kind.is_some() {
            bits[i / 64] |= 1 << (i % 64);
        }
    }
    KindStates(bits.into())
});
