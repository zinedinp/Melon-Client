//! 2D pass: alpha-blended textured quads in GUI pixel coords.

use ash::vk;

use super::{Pass, PipelineDesc, depth_disabled};
use crate::gfx::quad::{BlendMode, Program, Vertex};

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
    ///
    /// # Safety
    ///
    /// `device` must be live.
    pub unsafe fn clear_pack(&mut self, device: &ash::Device) {
        for passes in self.pack.iter_mut().filter_map(Option::take) {
            for pass in &passes {
                pass.destroy(device);
            }
        }
    }

    /// Destroys every pipeline.
    ///
    /// # Safety
    ///
    /// `device` must be live and done with them.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        self.clear_pack(device);
        for pass in &self.built_in {
            pass.destroy(device);
        }
        self.glint.destroy(device);
    }
}

/// `shaders` once per blend mode.
///
/// # Safety
///
/// `device` must be live, `render_pass` and the layouts made on it.
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
///
/// # Safety
///
/// `device` must be live, `render_pass` and the layouts made on it.
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
            (vk::Format::R8G8B8A8_UNORM, 16),
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

/// Where UI quads are recorded to, and what they are drawn with.
pub struct Target<'a> {
    pub device: &'a ash::Device,
    pub cmd: vk::CommandBuffer,
    pub pipelines: &'a UiPipelines,
    /// The GUI uniform blocks' set of the frame being recorded.
    pub gui_set: vk::DescriptorSet,
    pub vertices: vk::Buffer,
    pub indices: vk::Buffer,
    /// The size of what is drawn into, in pixels.
    pub extent: vk::Extent2D,
}

/// Records `calls`, in GUI coordinates spanning `ui_size`; `set_of` gives a texture's set.
///
/// # Safety
///
/// `target`'s command buffer must be recording inside its render pass; `set_of`'s sets must be live.
pub unsafe fn record(
    target: &Target,
    ui_size: (f32, f32),
    calls: &[crate::gfx::quad::DrawCall],
    full_scissor: vk::Rect2D,
    set_of: impl Fn(crate::gfx::TextureId) -> vk::DescriptorSet,
) {
    if calls.is_empty() {
        return;
    }
    let (device, cmd, ui) = (target.device, target.cmd, target.pipelines);
    let push = [ui_size.0, ui_size.1];
    let bind = |pass: &Pass| {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
        device.cmd_push_constants(
            cmd,
            pass.layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            super::push_constant_bytes(&push, PUSH_CONSTANT_SIZE),
        );
    };
    let mut applied = (Program::Sprite, BlendMode::Alpha);
    bind(ui.pass(applied.0, applied.1));
    device.cmd_bind_vertex_buffers(cmd, 0, &[target.vertices], &[0]);
    device.cmd_bind_index_buffer(cmd, target.indices, 0, vk::IndexType::UINT32);
    device.cmd_bind_descriptor_sets(
        cmd,
        vk::PipelineBindPoint::GRAPHICS,
        ui.layout(),
        1,
        &[target.gui_set],
        &[],
    );

    let scale_x = target.extent.width as f32 / ui_size.0;
    let scale_y = target.extent.height as f32 / ui_size.1;
    let mut applied_scissor = full_scissor;
    device.cmd_set_scissor(cmd, 0, &[full_scissor]);
    for call in calls {
        let wanted = match call.scissor {
            Some([x, y, w, h]) => vk::Rect2D {
                offset: vk::Offset2D {
                    x: (x * scale_x).round() as i32,
                    y: (y * scale_y).round() as i32,
                },
                extent: vk::Extent2D {
                    width: (w * scale_x).round() as u32,
                    height: (h * scale_y).round() as u32,
                },
            },
            None => full_scissor,
        };
        if wanted != applied_scissor {
            device.cmd_set_scissor(cmd, 0, &[wanted]);
            applied_scissor = wanted;
        }
        let wanted_pipeline = (call.program, call.blend);
        if wanted_pipeline != applied {
            let (from, to) = (
                ui.pass(applied.0, applied.1),
                ui.pass(call.program, call.blend),
            );
            if from.pipeline != to.pipeline {
                bind(to);
            }
            applied = wanted_pipeline;
        }
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            ui.layout(),
            0,
            &[set_of(call.texture)],
            &[],
        );
        device.cmd_draw_indexed(cmd, call.count, 1, call.first, 0, 0);
    }
}
