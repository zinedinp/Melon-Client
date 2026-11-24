//! A resource pack's `core/rendertype_outline`: Lumiere (and the like)

use std::path::Path;

use ash::vk;
use glam::Mat4;

use super::gui_uniforms::BLOCKS;
use super::pipeline::{Pass, PipelineDesc, depth_disabled};
use super::vk_util::{MAPPED_FOR_GPU, create_buffer_in};
use super::{Renderer, WorldVertex};
use crate::assets::shaders::{self, Interface, Stage};

/// Glowing entities that write pack-outline markers this frame.
const SLOTS: usize = 64;
const STRIDE: u64 = 256;
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
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    ptr: *mut u8,
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
        let layout = blocks_layout(device)?;
        let pass = create_pass(device, color_pass, texture_layout, layout, shader)?;
        let (buffer, memory, ptr, pool, sets) = uniforms(device, mem_props, layout)?;
        Ok(Self {
            pass,
            layout,
            pool,
            buffer,
            memory,
            ptr,
            sets,
        })
    }

    /// Projection, screen size and the day's fraction, once a frame.
    pub unsafe fn write_frame(&self, proj: Mat4, screen: (f32, f32)) {
        let game_time = super::gui_uniforms::game_time();
        let proj = proj.to_cols_array();
        std::ptr::copy_nonoverlapping(proj.as_ptr(), self.ptr.add(STRIDE as usize).cast(), 16);
        #[repr(C)]
        struct Globals {
            camera_block_pos: [i32; 3],
            glint_alpha: f32,
            camera_offset: [f32; 3],
            game_time: f32,
            screen_size: [f32; 2],
            menu_blur_radius: i32,
            use_rgss: i32,
        }
        self.ptr
            .add(2 * STRIDE as usize)
            .cast::<Globals>()
            .write(Globals {
                camera_block_pos: [0; 3],
                glint_alpha: 1.0,
                camera_offset: [crate::world::glint::millis(), 0.0, 0.0],
                game_time,
                screen_size: [screen.0, screen.1],
                menu_blur_radius: 5,
                use_rgss: 0,
            });
    }

    /// Model-view and colour modulator of glowing entity `slot`.
    pub unsafe fn write_entity(&self, slot: usize, model_view: Mat4, colour: [f32; 4]) {
        #[repr(C)]
        struct DynamicTransforms {
            model_view: [f32; 16],
            texture: [f32; 16],
            color_modulator: [f32; 4],
            model_offset: [f32; 4],
        }
        const IDENTITY: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        self.ptr
            .add((BLOCKS_N + slot as u64) as usize * STRIDE as usize)
            .cast::<DynamicTransforms>()
            .write(DynamicTransforms {
                model_view: model_view.to_cols_array(),
                texture: IDENTITY,
                color_modulator: colour,
                model_offset: [0.0; 4],
            });
    }

    pub fn set(&self, slot: usize) -> vk::DescriptorSet {
        self.sets[slot]
    }

    pub fn slots() -> usize {
        SLOTS
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        self.pass.destroy(device);
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_descriptor_set_layout(self.layout, None);
        device.unmap_memory(self.memory);
        device.destroy_buffer(self.buffer, None);
        device.free_memory(self.memory, None);
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
    Ok(device.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
        None,
    )?)
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

unsafe fn uniforms(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    layout: vk::DescriptorSetLayout,
) -> anyhow::Result<(
    vk::Buffer,
    vk::DeviceMemory,
    *mut u8,
    vk::DescriptorPool,
    Vec<vk::DescriptorSet>,
)> {
    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: (SLOTS as u32) * BLOCKS_N as u32,
    }];
    let pool = device.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&sizes)
            .max_sets(SLOTS as u32),
        None,
    )?;
    let layouts = vec![layout; SLOTS];
    let sets = match device.allocate_descriptor_sets(
        &vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts),
    ) {
        Ok(sets) => sets,
        Err(e) => {
            device.destroy_descriptor_pool(pool, None);
            return Err(e.into());
        }
    };
    let size = STRIDE * (BLOCKS_N + SLOTS as u64);
    let (buffer, memory) = create_buffer_in(
        device,
        mem_props,
        size,
        vk::BufferUsageFlags::UNIFORM_BUFFER,
        &MAPPED_FOR_GPU,
    )?;
    let ptr = match device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty()) {
        Ok(ptr) => ptr as *mut u8,
        Err(e) => {
            device.destroy_buffer(buffer, None);
            device.free_memory(memory, None);
            device.destroy_descriptor_pool(pool, None);
            return Err(e.into());
        }
    };
    for (i, &set) in sets.iter().enumerate() {
        let infos = [
            vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(STRIDE * (BLOCKS_N + i as u64))
                .range(STRIDE),
            vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(STRIDE)
                .range(STRIDE),
            vk::DescriptorBufferInfo::default()
                .buffer(buffer)
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
        device.update_descriptor_sets(&writes, &[]);
    }
    Ok((buffer, memory, ptr, pool, sets))
}

impl Renderer {
    /// The pack's outline shaders from now on, or Melon's flat colour when `None`.
    pub fn set_outline_pack_shader(&mut self, shader: Option<&Shader>) -> Option<anyhow::Error> {
        unsafe {
            if let Err(e) = self.device.device_wait_idle() {
                crate::warn!("Waiting for the GPU failed: {e}");
            }
            if let Some(mut pack) = self.pack_outline.take() {
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
