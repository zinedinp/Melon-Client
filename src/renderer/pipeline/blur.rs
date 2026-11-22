//! The menu background blur.

use ash::vk;

use super::{Pass, PipelineDesc, depth_disabled};

const VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blur.vert.spv"));
const FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blur.frag.spv"));

/// `vec2 texel_step`, `float radius`, padded to a `vec4`.
pub const PUSH_CONSTANT_SIZE: u32 = 16;

/// The blur for whatever render pass `render_pass` is compatible with.
pub unsafe fn create(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> anyhow::Result<Pass> {
    PipelineDesc {
        vert: VERT_SPV,
        frag: FRAG_SPV,
        push_constant_size: PUSH_CONSTANT_SIZE,
        push_stages: vk::ShaderStageFlags::FRAGMENT,
        vertex_stride: 0,
        attributes: &[],
        instance: None,
        used_locations: None,
        cull: vk::CullModeFlags::NONE,
        depth: depth_disabled(),
        blend: vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false),
        extra_sets: &[],
        depth_bias: None,
    }
    .build(device, render_pass, descriptor_set_layout)
}
