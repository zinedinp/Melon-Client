//! 3D pass: world skybox as a cubemap cube around the camera.

use ash::vk;

use super::{Pass, PipelineDesc, depth_disabled};

const VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sky.vert.spv"));
const FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sky.frag.spv"));

/// `mat4 view_proj`.
pub const PUSH_CONSTANT_SIZE: u32 = 64;

/// Vanilla `CubeMap.PROJECTION_FOV` / `PROJECTION_Z_NEAR` / `PROJECTION_Z_FAR`.
pub const Z_NEAR: f32 = 0.05;
pub const Z_FAR: f32 = 10.0;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 3],
}

const _: () = assert!(std::mem::size_of::<Vertex>() == 12);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl numerobis::elem::Pod for Vertex {}

/// Vanilla `CubeMap.initializeVertices()`: six quads as a triangle list.
pub const CUBE_VERTICES: [Vertex; 36] = {
    const fn v(x: f32, y: f32, z: f32) -> Vertex {
        Vertex { pos: [x, y, z] }
    }
    [
        v(-1.0, -1.0, 1.0),
        v(-1.0, 1.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(-1.0, -1.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(1.0, -1.0, 1.0),
        v(1.0, -1.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(1.0, 1.0, -1.0),
        v(1.0, -1.0, 1.0),
        v(1.0, 1.0, -1.0),
        v(1.0, -1.0, -1.0),
        v(1.0, -1.0, -1.0),
        v(1.0, 1.0, -1.0),
        v(-1.0, 1.0, -1.0),
        v(1.0, -1.0, -1.0),
        v(-1.0, 1.0, -1.0),
        v(-1.0, -1.0, -1.0),
        v(-1.0, -1.0, -1.0),
        v(-1.0, 1.0, -1.0),
        v(-1.0, 1.0, 1.0),
        v(-1.0, -1.0, -1.0),
        v(-1.0, 1.0, 1.0),
        v(-1.0, -1.0, 1.0),
        v(-1.0, -1.0, -1.0),
        v(-1.0, -1.0, 1.0),
        v(1.0, -1.0, 1.0),
        v(-1.0, -1.0, -1.0),
        v(1.0, -1.0, 1.0),
        v(1.0, -1.0, -1.0),
        v(-1.0, 1.0, 1.0),
        v(-1.0, 1.0, -1.0),
        v(1.0, 1.0, -1.0),
        v(-1.0, 1.0, 1.0),
        v(1.0, 1.0, -1.0),
        v(1.0, 1.0, 1.0),
    ]
};

pub unsafe fn create(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> anyhow::Result<Pass> {
    // SAFETY: `device` is live, and the layout, pass and shaders named are its own.
    unsafe {
        PipelineDesc {
            vert: VERT_SPV,
            frag: FRAG_SPV,
            push_constant_size: PUSH_CONSTANT_SIZE,
            push_stages: vk::ShaderStageFlags::VERTEX,
            vertex_stride: std::mem::size_of::<Vertex>() as u32,
            attributes: &[(vk::Format::R32G32B32_SFLOAT, 0)],
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
}
