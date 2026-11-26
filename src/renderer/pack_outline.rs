//! A resource pack's `core/rendertype_outline`: Lumiere (and the like)

use std::path::Path;

use ash::vk;
use glam::Mat4;

use super::gui_uniforms::{BLOCKS, DynamicTransforms, Globals, IDENTITY, STRIDE};
use super::pipeline::{Pass, PipelineDesc, depth_disabled};
use super::vk_util::Place;
use super::{Renderer, WorldVertex};
use crate::assets::shaders::{self, Interface, Stage};
use melon_base::gfx::mapped::MappedBuffer;

/// Glowing entities that write pack-outline markers this frame.
const SLOTS: usize = 64;
const BLOCKS_N: u64 = BLOCKS.len() as u64;

/// Compiled pack shaders for the outline pass.
pub struct Shader {
    pub vert: Vec<u32>,
    pub frag: Vec<u32>,
    pub inputs: Vec<u32>,
}

/// The pack's outline pipeline and the uniform blocks it reads.
pub(super) struct PackOutline {
    pub pass: Pass,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    buffer: MappedBuffer,
    /// One set per slot.
    sets: Vec<vk::DescriptorSet>,
}

/// `core/rendertype_outline` from the active packs, when a pack ships it.
pub fn compile(assets_root: &Path) -> anyhow::Result<Option<Shader>> {
    let vert = shaders::core_source(assets_root, "rendertype_outline", Stage::Vertex);
    let frag = shaders::core_source(assets_root, "rendertype_outline", Stage::Fragment);
    let from_pack =
        |source: &Option<(String, bool)>| source.as_ref().is_some_and(|(_, pack)| *pack);
    if !from_pack(&vert) && !from_pack(&frag) {
        return Ok(None);
    }
    let missing = |stage: &str| anyhow::anyhow!("core/rendertype_outline has no {stage}");
    let (vert, _) = vert.ok_or_else(|| missing("vertex shader"))?;
    let (frag, _) = frag.ok_or_else(|| missing("fragment shader"))?;
    let interface = Interface {
        inputs: &[("Position", 0), ("UV0", 1), ("Color", 2)],
        resources: &[
            ("Sampler0", 0, 0),
            ("DynamicTransforms", 1, 0),
            ("Projection", 1, 1),
            ("Globals", 1, 2),
        ],
    };
    let compile = |stage, source: &str| {
        shaders::compile(
            assets_root,
            "core/rendertype_outline",
            stage,
            source,
            &[],
            &interface,
        )
    };
    let vert = compile(Stage::Vertex, &vert)?;
    Ok(Some(Shader {
        inputs: shaders::input_locations(&vert),
        vert,
        frag: compile(Stage::Fragment, &frag)?,
    }))
}

impl PackOutline {
    pub unsafe fn create(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        color_pass: vk::RenderPass,
        texture_layout: vk::DescriptorSetLayout,
        shader: &Shader,
    ) -> anyhow::Result<Self> {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let layout = unsafe { blocks_layout(device) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pass = unsafe { create_pass(device, color_pass, texture_layout, layout, shader) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let (buffer, pool, sets) = unsafe { uniforms(device, mem_props, layout) }?;
        Ok(Self {
            pass,
            layout,
            pool,
            buffer,
            sets,
        })
    }

    /// Projection, screen size and the day's fraction, once a frame.
    pub unsafe fn write_frame(&self, proj: Mat4, screen: (f32, f32)) {
        let game_time = super::gui_uniforms::game_time();
        let globals = Globals::gui(crate::world::glint::millis(), game_time, screen);
        let stride = STRIDE as usize;
        // SAFETY: the caller keeps the GPU off the shared blocks (every frame's fence was waited on).
        unsafe {
            self.buffer.write_one(stride, &proj.to_cols_array());
            self.buffer.write_one(2 * stride, &globals);
        }
    }

    /// Model-view and colour modulator of glowing entity `slot`.
    pub unsafe fn write_entity(&self, slot: usize, model_view: Mat4, colour: [f32; 4]) {
        let transforms = DynamicTransforms {
            model_view: model_view.to_cols_array(),
            texture: IDENTITY,
            color_modulator: colour,
            model_offset: [0.0; 4],
        };
        let at = (BLOCKS_N + slot as u64) as usize * STRIDE as usize;
        // SAFETY: the caller keeps the GPU off this slot's block (its frame's fence was waited on).
        unsafe { self.buffer.write_one(at, &transforms) };
    }

    pub fn set(&self, slot: usize) -> vk::DescriptorSet {
        self.sets[slot]
    }

    pub fn slots() -> usize {
        SLOTS
    }

    pub unsafe fn destroy(self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            self.pass.destroy(device);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout, None);
            self.buffer.destroy(device);
        }
    }
}

unsafe fn blocks_layout(device: &ash::Device) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings: Vec<_> = (0..BLOCKS.len() as u32)
        .map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        })
        .collect();
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }?)
}

unsafe fn create_pass(
    device: &ash::Device,
    color_pass: vk::RenderPass,
    texture_layout: vk::DescriptorSetLayout,
    blocks_layout: vk::DescriptorSetLayout,
    shader: &Shader,
) -> anyhow::Result<Pass> {
    let bytes = |words: &[u32]| -> Vec<u8> { words.iter().flat_map(|w| w.to_le_bytes()).collect() };
    let vert = bytes(&shader.vert);
    let frag = bytes(&shader.frag);
    let attributes = [
        (vk::Format::R32G32B32_SFLOAT, 0),
        (
            vk::Format::R32G32_SFLOAT,
            std::mem::offset_of!(WorldVertex, uv) as u32,
        ),
        (
            vk::Format::R8G8B8A8_UNORM,
            std::mem::offset_of!(WorldVertex, color) as u32,
        ),
        (
            vk::Format::R8G8_UINT,
            std::mem::offset_of!(WorldVertex, light) as u32,
        ),
    ];
    // SAFETY: `device` is live, and the layout, pass and shaders named are its own.
    unsafe {
        PipelineDesc {
            vert: &vert,
            frag: &frag,
            push_constant_size: 0,
            push_stages: vk::ShaderStageFlags::VERTEX,
            vertex_stride: std::mem::size_of::<WorldVertex>() as u32,
            attributes: &attributes,
            instance: None,
            used_locations: Some(&shader.inputs),
            cull: vk::CullModeFlags::NONE,
            depth: depth_disabled(),
            blend: vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false),
            extra_sets: &[blocks_layout],
            depth_bias: None,
        }
        .build(device, color_pass, texture_layout)
    }
}

unsafe fn uniforms(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    layout: vk::DescriptorSetLayout,
) -> anyhow::Result<(MappedBuffer, vk::DescriptorPool, Vec<vk::DescriptorSet>)> {
    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: (SLOTS as u32) * BLOCKS_N as u32,
    }];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let pool = unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&sizes)
                .max_sets(SLOTS as u32),
            None,
        )
    }?;
    let layouts = vec![layout; SLOTS];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let sets = match unsafe {
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )
    } {
        Ok(sets) => sets,
        Err(e) => {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_descriptor_pool(pool, None) };
            return Err(e.into());
        }
    };
    let size = STRIDE * (BLOCKS_N + SLOTS as u64);
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let made = unsafe {
        MappedBuffer::new(
            device,
            mem_props,
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            Place::UPLOAD,
        )
    };
    let buffer = match made {
        Ok(buffer) => buffer,
        Err(e) => {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_descriptor_pool(pool, None) };
            return Err(e);
        }
    };
    for (i, &set) in sets.iter().enumerate() {
        let infos = [
            vk::DescriptorBufferInfo::default()
                .buffer(buffer.buffer)
                .offset(STRIDE * (BLOCKS_N + i as u64))
                .range(STRIDE),
            vk::DescriptorBufferInfo::default()
                .buffer(buffer.buffer)
                .offset(STRIDE)
                .range(STRIDE),
            vk::DescriptorBufferInfo::default()
                .buffer(buffer.buffer)
                .offset(2 * STRIDE)
                .range(STRIDE),
        ];
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(binding, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(binding as u32)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(std::slice::from_ref(info))
            })
            .collect();
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }
    Ok((buffer, pool, sets))
}

impl Renderer {
    /// The pack's outline shaders from now on, or Melon's flat colour when `None`.
    pub fn set_outline_pack_shader(&mut self, shader: Option<&Shader>) -> Option<anyhow::Error> {
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            if let Err(e) = self.device.device_wait_idle() {
                crate::warn!("Waiting for the GPU failed: {e}");
            }
            if let Some(pack) = self.pack_outline.take() {
                pack.destroy(&self.device);
            }
            let shader = shader?;
            match PackOutline::create(
                &self.device,
                &self.mem_props,
                self.post.color_pass,
                self.pipelines.layout,
                shader,
            ) {
                Ok(pack) => {
                    self.pack_outline = Some(pack);
                    None
                }
                Err(e) => Some(e),
            }
        }
    }
}
