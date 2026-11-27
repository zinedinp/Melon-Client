//! What vanilla's smooth lighting (`BlockModelLighter`) asks of each block.

use std::sync::LazyLock;

use pumpkin_data::{Block, BlockStateId};

/// One state's smooth-lighting facts, 16 bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AoFlags(u16);

impl AoFlags {
    const EMISSION: u16 = 15;
    const SHADE_DARK: u16 = 1 << 4;
    const LIGHT_PERMEABLE: u16 = 1 << 5;
    const COLLISION_FULL: u16 = 1 << 6;
    const SOLID_RENDER: u16 = 1 << 7;
    const EMISSIVE_RENDERING: u16 = 1 << 8;

    /// Air, and states a newer server knows but Melon doesn't.
    pub const AIR: Self = Self(Self::LIGHT_PERMEABLE);

    /// `BlockState.getLightEmission`, `0..=15`.
    #[inline]
    pub fn emission(self) -> u8 {
        (self.0 & Self::EMISSION) as u8
    }

    /// `getShadeBrightness`: 0.2 when true, else 1.0.
    #[inline]
    pub fn shade_dark(self) -> bool {
        self.0 & Self::SHADE_DARK != 0
    }

    /// `BlockState.isLightPermeable`: `!isSolidRender || lightDampening == 0`.
    #[inline]
    pub fn light_permeable(self) -> bool {
        self.0 & Self::LIGHT_PERMEABLE != 0
    }

    /// `BlockState.isCollisionShapeFullBlock`.
    #[inline]
    pub fn collision_full(self) -> bool {
        self.0 & Self::COLLISION_FULL != 0
    }

    /// `BlockState.isSolidRender`.
    #[inline]
    pub fn solid_render(self) -> bool {
        self.0 & Self::SOLID_RENDER != 0
    }

    /// `BlockState.emissiveRendering`: drawn fully lit.
    #[inline]
    pub fn emissive_rendering(self) -> bool {
        self.0 & Self::EMISSIVE_RENDERING != 0
    }

    fn of(id: u16) -> Self {
        let Some(state_id) = BlockStateId::new(id) else {
            return Self::AIR;
        };
        let state = state_id.to_state();
        let block = Block::from_state_id(state_id);
        let props = |key: &str| {
            block.properties(state_id).and_then(|p| {
                p.to_props()
                    .into_iter()
                    .find(|prop| prop.key_str() == key)
                    .map(|prop| prop.value_str())
            })
        };
        let collision_full = collision_full(state);
        let mut bits = u16::from(state.luminance.min(15));
        if shade_dark(block.name, collision_full, || props("layers")) {
            bits |= Self::SHADE_DARK;
        }
        if !state.is_solid_render() || state.opacity == 0 {
            bits |= Self::LIGHT_PERMEABLE;
        }
        if collision_full {
            bits |= Self::COLLISION_FULL;
        }
        if state.is_solid_render() {
            bits |= Self::SOLID_RENDER;
        }
        let emissive = match block.name {
            "magma_block" => true,
            "sculk_sensor" | "calibrated_sculk_sensor" => {
                props("sculk_sensor_phase") == Some("active")
            }
            _ => false,
        };
        if emissive {
            bits |= Self::EMISSIVE_RENDERING;
        }
        Self(bits)
    }
}

/// `Block.isShapeFullBlock(getCollisionShape)`: the boxes fill the cube.
fn collision_full(state: &pumpkin_data::BlockState) -> bool {
    let mut volume = 0.0;
    for shape in state.get_block_collision_shapes() {
        let (min, max) = (shape.min, shape.max);
        let inside = [min.x, min.y, min.z].iter().all(|&v| v >= -1e-7)
            && [max.x, max.y, max.z].iter().all(|&v| v <= 1.0 + 1e-7);
        if !inside {
            return false;
        }
        volume += (max.x - min.x) * (max.y - min.y) * (max.z - min.z);
    }
    volume >= 1.0 - 1e-6
}

/// `BlockBehaviour.getShadeBrightness` is 0.2 for a full collision shape.
fn shade_dark<'a>(name: &str, collision_full: bool, layers: impl Fn() -> Option<&'a str>) -> bool {
    match name {
        "glass" | "tinted_glass" | "barrier" | "light" | "structure_void" => false,
        _ if name.ends_with("_stained_glass") || name.ends_with("copper_grate") => false,
        "mud" | "soul_sand" => true,
        "snow" => layers() == Some("8"),
        _ => collision_full,
    }
}

static TABLE: LazyLock<Box<[AoFlags]>> =
    LazyLock::new(|| (0..BlockStateId::COUNT).map(AoFlags::of).collect());

/// Every state's flags, indexed by state ID.
pub fn table() -> &'static [AoFlags] {
    &TABLE
}

/// The flags of `state`: from the table for vanilla states, worked out
/// for zip-mod states past it; [`AoFlags::AIR`] for unknown states.
pub fn of(state: u16) -> AoFlags {
    TABLE
        .get(usize::from(state))
        .copied()
        .unwrap_or_else(|| AoFlags::of(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(block: &Block) -> AoFlags {
        of(block.default_state.id.as_u16())
    }

    #[test]
    fn full_opaque_blocks_shade_and_block_light() {
        let stone = flags(&Block::STONE);
        assert!(stone.shade_dark() && stone.collision_full() && stone.solid_render());
        assert!(!stone.light_permeable());
        assert_eq!(stone.emission(), 0);
    }

    #[test]
    fn air_lets_everything_through() {
        let air = of(0);
        assert!(!air.shade_dark() && !air.collision_full() && !air.solid_render());
        assert!(air.light_permeable());
        assert_eq!(of(u16::MAX), AoFlags::AIR);
    }

    #[test]
    fn glass_is_full_but_never_darkens() {
        let glass = flags(&Block::GLASS);
        assert!(glass.collision_full() && !glass.shade_dark() && glass.light_permeable());
        assert!(!flags(&Block::RED_STAINED_GLASS).shade_dark());
        assert!(!flags(&Block::TINTED_GLASS).shade_dark());
    }

    #[test]
    fn overrides_follow_vanilla() {
        let mud = flags(&Block::MUD);
        assert!(!mud.collision_full() && mud.shade_dark());
        assert!(flags(&Block::SOUL_SAND).shade_dark());
        let snow = |layers: &str| {
            Block::SNOW
                .states
                .iter()
                .map(|s| s.id)
                .find(|&id| {
                    Block::SNOW.properties(id).is_some_and(|p| {
                        p.to_props()
                            .iter()
                            .any(|prop| prop.key_str() == "layers" && prop.value_str() == layers)
                    })
                })
                .map(|id| of(id.as_u16()))
                .expect("snow has 1 to 8 layers")
        };
        assert!(!snow("1").shade_dark());
        assert!(snow("8").shade_dark());
    }

    #[test]
    fn leaves_and_lights() {
        let leaves = flags(&Block::OAK_LEAVES);
        assert!(leaves.collision_full() && leaves.shade_dark() && leaves.light_permeable());
        assert_eq!(flags(&Block::GLOWSTONE).emission(), 15);
        assert!(flags(&Block::MAGMA_BLOCK).emissive_rendering());
        assert!(!flags(&Block::STONE).emissive_rendering());
    }
}
