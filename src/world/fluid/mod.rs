//! Water and lava (vanilla `FlowingFluid` for the state, `FluidRenderer` for the mesh).

mod mesh;
mod state;

pub use mesh::mesh;
pub use state::{FluidKind, FluidState, fluid_of};

/// Atlas rectangles of the fluid textures.
#[derive(Clone, Copy)]
pub struct FluidSprites {
    pub water_still: [f32; 4],
    pub water_flow: [f32; 4],
    /// `water_overlay`: the side facing glass, ice or leaves.
    pub water_overlay: [f32; 4],
    pub lava_still: [f32; 4],
    pub lava_flow: [f32; 4],
}

impl FluidSprites {
    pub const TEXTURES: [&'static str; 5] = [
        "minecraft:block/water_still",
        "minecraft:block/water_flow",
        "minecraft:block/water_overlay",
        "minecraft:block/lava_still",
        "minecraft:block/lava_flow",
    ];
}

/// `BiomeColors.getAverageWaterColor` in plains.
#[cfg(test)]
pub(crate) const WATER_TINT: [f32; 3] = [
    0x3F as f32 / 255.0,
    0x76 as f32 / 255.0,
    0xE4 as f32 / 255.0,
];
/// `FluidRenderer.MAX_FLUID_HEIGHT`.
pub(crate) const MAX_FLUID_HEIGHT: f32 = 0.888_888_9;
/// Sides and the top are pulled this far in, against z-fighting.
pub(crate) const OFFSET: f32 = 0.001;
