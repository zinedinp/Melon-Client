//! What the server said of the level the player is in: its dimension and
//! the registries that describe it.

use melon_world::biome::Biomes;
use melon_world::dimension::{DimensionType, DimensionTypes};

pub(super) struct LevelInfo {
    /// `Level.dimension().identifier()`, for the F3 overlay and the
    /// integrated server's world.
    pub(super) dimension: String,
    /// The dimension types the server registered, by registry index.
    pub(super) dimension_types: DimensionTypes,
    /// The level's `DimensionType`: its height, light and environment.
    pub(super) dimension_type: DimensionType,
    /// The server's biomes, for the colours of fog and sky.
    pub(super) biomes: Biomes,
    /// The login's hashed seed: `BiomeManager`'s zoom seed.
    pub(super) hashed_seed: i64,
}
