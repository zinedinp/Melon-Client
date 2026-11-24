//! 3D pass: terrain, fluids, entities and the sky bodies, depth-tested.

use ash::vk;

use super::{Pass, PipelineDesc};
use crate::renderer::{ParticleQuad, TerrainQuad, WorldVertex as Vertex};
use crate::world::lightmap::LightmapParams;

const VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world.vert.spv"));
const WEATHER_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/weather.vert.spv"));
const LINES_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/lines.vert.spv"));
const PARTICLE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/particle.vert.spv"));
const TERRAIN_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/terrain.vert.spv"));
const TERRAIN_INDIRECT_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain_indirect.vert.spv"));
const FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world.frag.spv"));
const SCROLL_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world_scroll.frag.spv"));
const SKY_DISC_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/skydisc.vert.spv"));
const SKY_DISC_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/skydisc.frag.spv"));
/// `world.frag` without `discard`.
const SOLID_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world_solid.frag.spv"));
const CUTOUT_TERRAIN_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/world_cutout_terrain.frag.spv"));
const CRUMBLING_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/world_crumbling.frag.spv"));
const WEATHER_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world_weather.frag.spv"));
const CLOUDS_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world_clouds.frag.spv"));
const OUTLINE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world_outline.frag.spv"));
const WATER_MASK_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/world_water_mask.frag.spv"));
const GLINT_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world_glint.frag.spv"));

/// `mat4 view_proj`, `vec4 ambient`, `vec4 params` (sky light RGB, gamma), and the rest of the
/// push block.
pub const PUSH_CONSTANT_SIZE: u32 = VIEW_AT + 16;
/// Where the per-draw mesh offset starts.
pub const OFFSET_AT: u32 = 96;
/// Where how the fog finds a model's distance is.
pub const VIEW_AT: u32 = 112;
/// The size of the offset and of the view constants: a `vec4` each.
pub const OFFSET_SIZE: u32 = 16;

/// How `world.vert` finds a vertex's distance from the camera, for the fog.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FogInput {
    /// Not at all: the vertex is not fogged (sky bodies).
    None,
    /// Its position plus the push constants' offset is relative to the camera's block.
    Chunk,
    /// From where `view_proj` puts it: a model.
    Model([f32; 2]),
}

impl FogInput {
    /// The `vec4 view` constants.
    pub fn bytes(self) -> [f32; 4] {
        match self {
            Self::None => [0.0; 4],
            Self::Chunk => [0.0, 0.0, 1.0, 0.0],
            Self::Model([x, y]) => [x, y, 2.0, 0.0],
        }
    }
}
/// The lightmap's inputs are read where it runs, per fragment.
pub const PUSH_STAGES: vk::ShaderStageFlags = vk::ShaderStageFlags::from_raw(
    vk::ShaderStageFlags::VERTEX.as_raw() | vk::ShaderStageFlags::FRAGMENT.as_raw(),
);

/// How one world/celestial draw is lit and fogged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawLight {
    /// Multiplied onto sentinel-lit vertices (entities, celestials, ...).
    pub ambient: [f32; 4],
    /// The lightmap's sky light and gamma, for vertices that carry their own light.
    pub lightmap: LightmapParams,
    /// How the fog finds the draw's distance from the camera.
    pub fog: FogInput,
    /// Vanilla's entity overlay (`OverlayTexture`) on sentinel-lit vertices.
    pub overlay: f32,
    /// `TextureTransform` scale for the glint overlay pass; 0 skips it.
    pub glint_scale: f32,
    /// `OffsetTextureTransform`, sent in the lightmap's place (unread there).
    pub scroll: Option<[f32; 2]>,
}

impl DrawLight {
    /// Only a tint: for meshes without vertex light (celestials, shadows).
    pub const fn tint(ambient: [f32; 4]) -> Self {
        Self {
            ambient,
            lightmap: LightmapParams::overworld([1.0; 3], 0.0),
            fog: FogInput::None,
            overlay: 0.0,
            glint_scale: 0.0,
            scroll: None,
        }
    }

    /// The same, fogged as `fog` says.
    pub const fn fogged(mut self, fog: FogInput) -> Self {
        self.fog = fog;
        self
    }

    /// The same, with an entity overlay (see [`Self::overlay`]).
    pub const fn overlaid(mut self, overlay: f32) -> Self {
        self.overlay = overlay;
        self
    }

    /// The same, its texture slid by `scroll`.
    pub const fn scrolled(mut self, scroll: Option<[f32; 2]>) -> Self {
        self.scroll = scroll;
        self
    }

    /// The same, with a glint overlay of this scale (see [`Self::glint_scale`]).
    pub const fn glinted(mut self, scale: f32) -> Self {
        self.glint_scale = scale;
        self
    }
}

/// Push constants as the renderer remembers the last ones sent, 16-byte
/// aligned: unaligned (after an `Option`'s tag), each draw's compare read
/// what the last draw's store had not finished writing, and stalled on it.
#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq)]
pub struct PushBlock<const N: usize = 24>(pub [f32; N]);

/// Bytes for one world/celestial draw.
pub fn push_bytes(view_proj: &glam::Mat4, light: &DrawLight) -> [f32; 24] {
    let mut data = [0.0; 24];
    data[..16].copy_from_slice(&view_proj.to_cols_array());
    data[16..20].copy_from_slice(&light.ambient);
    data[20..23].copy_from_slice(&light.lightmap.sky);
    data[23] = light.lightmap.gamma;
    if let Some([u, v]) = light.scroll {
        data[20] = u;
        data[21] = v;
    }
    data
}

/// Which world-shaped pipeline to build.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fully opaque terrain (vanilla's `SOLID` chunk layer).
    Solid,
    /// Terrain with holes (leaves, plants; vanilla `CUTOUT`) and entities.
    Opaque,
    /// Blob shadows and see-through items.
    Translucent,
    /// Text displays (`RenderPipelines.TEXT_POLYGON_OFFSET`): blended,
    /// depth-tested and depth written, clear texels dropped.
    Text,
    /// See-through models (`ENTITY_TRANSLUCENT`, `ITEM_TRANSLUCENT`):
    /// [`Self::Text`] without its depth bias.
    EntityTranslucent,
    /// Water, glass and ice (vanilla `TRANSLUCENT_TERRAIN`).
    TranslucentTerrain,
    /// Fancy clouds (`RenderPipelines.CLOUDS`).
    Clouds,
    /// Rain and snow (`RenderPipelines.WEATHER`).
    Weather,
    /// Sun, moon and stars: no depth, vanilla `BlendFunction.OVERLAY`.
    Celestial,
    /// The sky's flat colour (`RenderPipelines.SKY`).
    SkyDisc,
    /// What lies over the sky and under the sun.
    SkyGlow,
    /// The cracks on a block being broken (`RenderPipelines.CRUMBLING`):
    Crumbling,
    /// The boat's water patch (`RenderPipelines.WATER_MASK`).
    WaterMask,
    /// A glowing entity's flat outline colour, into the entity outline target.
    Outline,
    /// Enchantment glint overlay (`RenderPipelines.GLINT`).
    Glint,
    /// Lines and points of a fixed width in pixels (`RenderPipelines.LINES`):
    /// depth written, not blended.
    Lines,
    /// `RenderPipelines.LINES_DEPTH_BIAS`: [`Self::Lines`], nearer by a
    /// depth bias.
    LinesDepthBias,
    /// `RenderPipelines.LINES_TRANSLUCENT`: blended, depth written.
    LinesTranslucent,
    /// `RenderPipelines.LINES_TRANSLUCENT_NO_DEPTH_WRITE`.
    LinesNoDepthWrite,
    /// `RenderPipelines.LIGHTNING`: added on (`BlendFunction.LIGHTNING`),
    /// the default depth test and write.
    Lightning,
    /// `RenderPipelines.ENERGY_SWIRL`.
    EnergySwirl,
    /// `RenderPipelines.BREEZE_WIND`.
    Scrolled,
}

impl Mode {
    fn is_lines(self) -> bool {
        matches!(
            self,
            Self::Lines | Self::LinesDepthBias | Self::LinesTranslucent | Self::LinesNoDepthWrite
        )
    }
}

/// What a pipeline draws.
#[derive(Clone, Copy)]
pub enum Geometry {
    /// [`Vertex`] quads through the shared index buffer.
    Vertices,
    /// [`TerrainQuad`]s, read from the mesh buffer (set 4) by the terrain shader.
    Quads { indirect: bool },
    /// [`ParticleQuad`]s, one per instance of the shared index buffer's first quad.
    Particles,
}

/// The descriptor sets after the texture's that every world pipeline binds.
#[derive(Clone, Copy)]
pub struct Layouts {
    /// Set 1: the quad table.
    pub table: vk::DescriptorSetLayout,
    /// Set 2: the multi-draws' offsets.
    pub draws: vk::DescriptorSetLayout,
    /// Set 3: the frame's globals.
    pub globals: vk::DescriptorSetLayout,
}

impl Layouts {
    fn sets(&self) -> [vk::DescriptorSetLayout; 3] {
        [self.table, self.draws, self.globals]
    }
}

/// `BlendFunction.TRANSLUCENT`: src α, dst 1-α.
fn translucent_blend() -> vk::PipelineColorBlendAttachmentState {
    vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
}

/// The world pipeline.
pub unsafe fn create(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    layouts: &Layouts,
    mode: Mode,
    geometry: Geometry,
) -> anyhow::Result<Pass> {
    let (cull, depth, blend) = match mode {
        Mode::Solid | Mode::Opaque => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false),
        ),
        Mode::Translucent => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            translucent_blend(),
        ),
        Mode::Weather => (
            vk::CullModeFlags::NONE,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            translucent_blend(),
        ),
        // Vanilla's `LINES_SNIPPET`: no cull, the default depth test.
        Mode::Lines | Mode::LinesDepthBias => (
            vk::CullModeFlags::NONE,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false),
        ),
        Mode::LinesTranslucent | Mode::LinesNoDepthWrite => (
            vk::CullModeFlags::NONE,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(mode == Mode::LinesTranslucent)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            translucent_blend(),
        ),
        Mode::Clouds | Mode::TranslucentTerrain | Mode::Text | Mode::EntityTranslucent => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            translucent_blend(),
        ),
        Mode::Lightning => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::ADD),
        ),
        Mode::EnergySwirl => (
            vk::CullModeFlags::NONE,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ONE)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::ADD),
        ),
        Mode::Scrolled => (
            vk::CullModeFlags::NONE,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            translucent_blend(),
        ),
        Mode::Celestial => (
            vk::CullModeFlags::NONE,
            super::depth_disabled(),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD),
        ),
        Mode::SkyDisc => (
            vk::CullModeFlags::NONE,
            super::depth_disabled(),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false),
        ),
        Mode::SkyGlow => (
            vk::CullModeFlags::NONE,
            super::depth_disabled(),
            translucent_blend(),
        ),
        Mode::Outline => (
            vk::CullModeFlags::NONE,
            super::depth_disabled(),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false),
        ),
        Mode::WaterMask => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::empty())
                .blend_enable(false),
        ),
        Mode::Crumbling => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
                .dst_color_blend_factor(vk::BlendFactor::SRC_COLOR)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD),
        ),
        Mode::Glint => (
            vk::CullModeFlags::BACK,
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::EQUAL),
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_COLOR)
                .dst_color_blend_factor(vk::BlendFactor::ONE)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ZERO)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::ADD),
        ),
    };
    let vertex_attributes = [
        (vk::Format::R32G32B32_SFLOAT, 0),
        (
            vk::Format::R32G32_SFLOAT,
            std::mem::offset_of!(Vertex, uv) as u32,
        ),
        (
            vk::Format::R8G8B8A8_UNORM,
            std::mem::offset_of!(Vertex, color) as u32,
        ),
        (
            vk::Format::R8G8_UINT,
            std::mem::offset_of!(Vertex, light) as u32,
        ),
        // Only `lines.vert` reads it: which side or corner a vertex is.
        (
            vk::Format::R8_UINT,
            std::mem::offset_of!(Vertex, face) as u32,
        ),
    ];
    // `world::weather::pieces`: two vertices of data per instance.
    let second = std::mem::size_of::<Vertex>() as u32;
    let weather_pieces = [
        (vk::Format::R32G32B32_SFLOAT, 0),
        (vk::Format::R32G32_SFLOAT, vertex_attributes[1].1),
        (vk::Format::R8G8B8A8_UNORM, vertex_attributes[2].1),
        (vk::Format::R32G32B32_SFLOAT, second),
        (vk::Format::R32G32_SFLOAT, second + vertex_attributes[1].1),
    ];
    let axes = std::mem::offset_of!(ParticleQuad, axes) as u32;
    let particle_quads = [
        (vk::Format::R32G32B32_SFLOAT, 0),
        // `right` and `up`'s x, then `up`'s y and z.
        (vk::Format::R16G16B16A16_SFLOAT, axes),
        (vk::Format::R16G16_SFLOAT, axes + 8),
        (
            vk::Format::R16G16B16A16_UNORM,
            std::mem::offset_of!(ParticleQuad, uv) as u32,
        ),
        (
            vk::Format::R8G8B8A8_UNORM,
            std::mem::offset_of!(ParticleQuad, color) as u32,
        ),
        (
            vk::Format::R8G8_UINT,
            std::mem::offset_of!(ParticleQuad, light) as u32,
        ),
    ];
    let (vert, vertex_stride, attributes): (_, _, &[_]) = match geometry {
        Geometry::Vertices => (
            if mode == Mode::SkyDisc {
                SKY_DISC_VERT_SPV
            } else if mode == Mode::Weather {
                WEATHER_VERT_SPV
            } else if mode.is_lines() {
                LINES_VERT_SPV
            } else {
                VERT_SPV
            },
            std::mem::size_of::<Vertex>(),
            if mode == Mode::SkyDisc {
                &vertex_attributes[..1]
            } else if mode.is_lines() {
                &vertex_attributes
            } else {
                &vertex_attributes[..4]
            },
        ),
        Geometry::Quads { indirect } => (
            if indirect {
                TERRAIN_INDIRECT_VERT_SPV
            } else {
                TERRAIN_VERT_SPV
            },
            std::mem::size_of::<TerrainQuad>(),
            &[],
        ),
        Geometry::Particles => (PARTICLE_VERT_SPV, 0, &[]),
    };
    let instance = match geometry {
        Geometry::Vertices if mode == Mode::Weather => Some((
            2 * std::mem::size_of::<Vertex>() as u32,
            &weather_pieces[..],
        )),
        Geometry::Particles => Some((
            std::mem::size_of::<ParticleQuad>() as u32,
            &particle_quads[..],
        )),
        Geometry::Vertices | Geometry::Quads { .. } => None,
    };
    let quad_sets = [layouts.table, layouts.draws, layouts.globals, layouts.table];
    let glint_sets = [descriptor_set_layout, layouts.draws, layouts.globals];
    let extra_sets: &[vk::DescriptorSetLayout] = match (mode, geometry) {
        (Mode::Glint, Geometry::Vertices) => &glint_sets,
        (_, Geometry::Vertices | Geometry::Particles) => &layouts.sets(),
        (_, Geometry::Quads { .. }) => &quad_sets,
    };
    PipelineDesc {
        vert,
        frag: match mode {
            Mode::Solid => SOLID_FRAG_SPV,
            Mode::Opaque if matches!(geometry, Geometry::Quads { .. }) => CUTOUT_TERRAIN_FRAG_SPV,
            Mode::Clouds => CLOUDS_FRAG_SPV,
            Mode::Weather => WEATHER_FRAG_SPV,
            Mode::SkyDisc => SKY_DISC_FRAG_SPV,
            Mode::Crumbling => CRUMBLING_FRAG_SPV,
            Mode::Outline => OUTLINE_FRAG_SPV,
            Mode::WaterMask => WATER_MASK_FRAG_SPV,
            Mode::Glint => GLINT_FRAG_SPV,
            Mode::EnergySwirl | Mode::Scrolled => SCROLL_FRAG_SPV,
            _ => FRAG_SPV,
        },
        push_constant_size: PUSH_CONSTANT_SIZE,
        push_stages: PUSH_STAGES,
        vertex_stride: vertex_stride as u32,
        attributes,
        instance,
        // `weather.vert` leaves the template's colour unread.
        used_locations: (mode == Mode::Weather).then_some(&[0, 1, 3][..]),
        cull,
        depth,
        blend,
        extra_sets,
        depth_bias: match mode {
            Mode::Crumbling => Some((40.0, 3.0)),
            // Vanilla's `DepthStencilState(GREATER_THAN_OR_EQUAL, true, 1, 1)`.
            Mode::LinesDepthBias => Some((1.0, 1.0)),
            // Only text is offset; entities, cutout terrain and the glint
            // over them have none, as vanilla (`DepthStencilState.DEFAULT`).
            // A slope bias pulled faces seen edge-on (a box's inner sides
            // under armor or leaves) through the faces they meet.
            Mode::Text => Some((16.0, 2.0)),
            _ => None,
        },
    }
    .build(device, render_pass, descriptor_set_layout)
}
