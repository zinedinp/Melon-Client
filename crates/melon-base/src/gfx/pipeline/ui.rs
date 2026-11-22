//! 2D pass: alpha-blended textured quads in GUI pixel coords.

use ash::vk;

use super::{Pass, PipelineDesc, depth_disabled};
use crate::renderer::quad::{BlendMode, Program, Vertex};

const VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui.vert.spv"));
const FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui.frag.spv"));
const GLINT_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui_glint.frag.spv"));

/// Every blend mode, in [`BlendMode`] order.
pub const BLENDS: [BlendMode; 4] = [
    BlendMode::Alpha,
    BlendMode::Invert,
    BlendMode::Additive,
    BlendMode::Vignette,
];

/// A vertex and a fragment shader, as SPIR-V bytes.
#[derive(Clone, Copy)]
pub struct Shaders<'a> {
    pub vert: &'a [u8],
    pub frag: &'a [u8],
    /// The vertex attribute locations the vertex shader reads; `None` for Melon's own shaders.
    pub inputs: Option<&'a [u32]>,
}

/// Melon's own UI shaders.
pub const BUILT_IN: Shaders<'static> = Shaders {
    vert: VERT_SPV,
    frag: FRAG_SPV,
    inputs: None,
};

/// Item icons with the enchantment glint, alpha-blended.
pub const GLINT: Shaders<'static> = Shaders {
    vert: VERT_SPV,
    frag: GLINT_FRAG_SPV,
    inputs: None,
};

/// The UI's pipelines.
pub struct UiPipelines {
    pub built_in: [Pass; 4],
    pub pack: [Option<[Pass; 4]>; 3],
    /// Item icons with the enchantment glint (`Program::Glint`).
    pub glint: Pass,
}

impl UiPipelines {
    pub fn pass(&self, program: Program, blend: BlendMode) -> &Pass {
        if program == Program::Glint {
            return &self.glint;
        }
        let passes = self.pack[program.index()]
            .as_ref()
            .unwrap_or(&self.built_in);
        &passes[blend as usize]
    }

    pub fn layout(&self) -> vk::PipelineLayout {
        self.built_in[0].layout
    }

    /// Drops the pack's pipelines; the GPU must be done with them.
    pub unsafe fn clear_pack(&mut self, device: &ash::Device) {
        for passes in self.pack.iter_mut().filter_map(Option::take) {
            for pass in &passes {
                pass.destroy(device);
            }
        }
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        self.clear_pack(device);
        for pass in &self.built_in {
            pass.destroy(device);
        }
        self.glint.destroy(device);
    }
}

/// `shaders` once per blend mode.
pub unsafe fn create_all(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    gui_layout: vk::DescriptorSetLayout,
    shaders: Shaders,
) -> anyhow::Result<[Pass; 4]> {
    let mut built = Vec::with_capacity(BLENDS.len());
    for blend in BLENDS {
        match create(
            device,
            render_pass,
            descriptor_set_layout,
            gui_layout,
            blend,
            shaders,
        ) {
            Ok(pass) => built.push(pass),
            Err(e) => {
                for pass in &built {
                    pass.destroy(device);
                }
                return Err(e);
            }
        }
    }
    Ok(built
        .try_into()
        .unwrap_or_else(|_| unreachable!("one pass per blend mode")))
}

/// `vec2 viewport_size` (GUI coordinate system of the vertices).
pub const PUSH_CONSTANT_SIZE: u32 = 8;

/// Same vertex format, shaders and layout for every [`BlendMode`]; only the blend differs.
pub unsafe fn create(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    gui_layout: vk::DescriptorSetLayout,
    blend: BlendMode,
    shaders: Shaders,
) -> anyhow::Result<Pass> {
    let attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .color_blend_op(vk::BlendOp::ADD)
        .alpha_blend_op(vk::BlendOp::ADD);
    let blend = match blend {
        BlendMode::Alpha => attachment
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA),
        BlendMode::Invert => attachment
            .src_color_blend_factor(vk::BlendFactor::ONE_MINUS_DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_COLOR)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO),
        BlendMode::Additive => attachment
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE),
        BlendMode::Vignette => attachment
            .src_color_blend_factor(vk::BlendFactor::ZERO)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_COLOR)
            .src_alpha_blend_factor(vk::BlendFactor::ZERO)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE),
    };
    PipelineDesc {
        vert: shaders.vert,
        frag: shaders.frag,
        push_constant_size: PUSH_CONSTANT_SIZE,
        push_stages: vk::ShaderStageFlags::VERTEX,
        vertex_stride: std::mem::size_of::<Vertex>() as u32,
        attributes: &[
            (vk::Format::R32G32_SFLOAT, 0),
            (vk::Format::R32G32_SFLOAT, 8),
            (vk::Format::R32G32B32A32_SFLOAT, 16),
        ],
        instance: None,
        used_locations: shaders.inputs,
        cull: vk::CullModeFlags::NONE,
        depth: depth_disabled(),
        blend,
        extra_sets: &[gui_layout],
        depth_bias: None,
    }
    .build(device, render_pass, descriptor_set_layout)
}
