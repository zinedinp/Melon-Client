//! Two-phase occlusion culling of the terrain's multi-draws.
//!
//! Phase 1 draws the sections that were visible last frame. Then a depth
//! pyramid (Hi-Z) is built from what phase 1 left in the depth buffer, and
//! `shaders/occlusion.comp` tests every draw's section box against it.
//! Phase 2 draws the ones phase 1 left out that may be visible, and each
//! section's result is kept for the next frame. Only sections the pyramid
//! proves hidden are skipped, so the picture is the same as drawing all.
//! `shaders/compact.comp` packs each phase's draws, keeping each run's
//! order, and `vkCmdDrawIndexedIndirectCount` draws just those.
//! Off unless `MELON_OCCLUSION=1`: so far its overhead (a render pass split
//! and every draw issued twice) costs more than it saves in open terrain.

use ash::vk;
use numerobis::elem::as_bytes;

use super::pipeline::create_shader_module;
use super::vk_util::{self, create_buffer, create_sampler};

const HIZ_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hiz.comp.spv"));
const SELECT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/occlusion_select.comp.spv"));
const CULL_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/occlusion.comp.spv"));
const COMPACT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/compact.comp.spv"));

/// `occlusion.comp`'s threads per group.
const DRAWS_PER_GROUP: u32 = 64;
/// `hiz.comp`'s threads per group side.
const HIZ_GROUP: u32 = 8;
/// Pyramid levels at most: down to one texel for screens up to 32768 pixels wide.
const MAX_LEVELS: u32 = 15;
/// Sections the visibility buffer has room for at first (1 MiB).
const MIN_SLOTS: u32 = 1 << 18;
/// Bytes of `hiz.comp`'s push constants: two `ivec2`.
const HIZ_PUSH_SIZE: u32 = 16;
/// Bytes of `occlusion.comp`'s: a `mat4`, a `vec2` and two `uint`.
const CULL_PUSH_SIZE: u32 = 80;

/// Whether `MELON_OCCLUSION=1` turns the culling on.
pub(super) fn wanted() -> bool {
    std::env::var_os("MELON_OCCLUSION").is_some_and(|v| v == "1")
}

/// Each section's visibility from the last frame that tested it, by slot.
struct Visible {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    slots: u32,
    /// Whether it was zeroed since it was made.
    cleared: bool,
}

/// The depth pyramid and the views it is built through, sized with the screen.
struct Pyramid {
    image: vk::Image,
    memory: vk::DeviceMemory,
    /// One per level, for `hiz.comp` to read and write.
    levels: Vec<vk::ImageView>,
    /// Every level, for `occlusion.comp`.
    whole: vk::ImageView,
    /// The depth buffer's depth aspect, which level 0 is built from.
    depth: vk::ImageView,
    depth_image: vk::Image,
    depth_aspect: vk::ImageAspectFlags,
    screen: vk::Extent2D,
}

impl Pyramid {
    /// Level `level`'s size, as Vulkan sizes the mip levels.
    fn size(&self, level: u32) -> vk::Extent2D {
        let base = base_size(self.screen);
        vk::Extent2D {
            width: (base.width >> level).max(1),
            height: (base.height >> level).max(1),
        }
    }

    unsafe fn destroy(&self, device: &ash::Device) {
        for &view in self.levels.iter().chain([&self.whole, &self.depth]) {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_image_view(view, None) };
        }
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_image(self.image, None);
            vk_util::free_memory(device, self.memory);
        }
    }
}

/// Level 0's size: half the screen's, rounded down.
fn base_size(screen: vk::Extent2D) -> vk::Extent2D {
    vk::Extent2D {
        width: (screen.width / 2).max(1),
        height: (screen.height / 2).max(1),
    }
}

/// Levels from level 0 down to one texel, at most [`MAX_LEVELS`].
fn level_count(screen: vk::Extent2D) -> u32 {
    let base = base_size(screen);
    let side = base.width.max(base.height);
    (u32::BITS - side.leading_zeros()).min(MAX_LEVELS)
}

pub(super) struct Occlusion {
    reduce_layout: vk::DescriptorSetLayout,
    cull_layout: vk::DescriptorSetLayout,
    reduce_pipeline_layout: vk::PipelineLayout,
    cull_pipeline_layout: vk::PipelineLayout,
    hiz: vk::Pipeline,
    select: vk::Pipeline,
    cull: vk::Pipeline,
    compact: vk::Pipeline,
    pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    /// One per pyramid level.
    reduce_sets: Vec<vk::DescriptorSet>,
    /// One per frame in flight.
    cull_sets: Vec<vk::DescriptorSet>,
    visible: Visible,
    pyramid: Option<Pyramid>,
    /// `vkCmdDrawIndexedIndirectCount` (core or `KHR`), which draws what
    /// the culling packed.
    pub draw_count: Option<vk::PFN_vkCmdDrawIndexedIndirectCount>,
}

unsafe fn set_layout(
    device: &ash::Device,
    types: &[vk::DescriptorType],
) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings: Vec<_> = types
        .iter()
        .enumerate()
        .map(|(i, &ty)| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(i as u32)
                .descriptor_type(ty)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
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

unsafe fn pipeline_layout(
    device: &ash::Device,
    set: vk::DescriptorSetLayout,
    push_size: u32,
) -> anyhow::Result<vk::PipelineLayout> {
    let ranges = [vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::COMPUTE)
        .size(push_size)];
    let sets = [set];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&sets)
                .push_constant_ranges(&ranges),
            None,
        )
    }?)
}

unsafe fn compute_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    spirv: &[u8],
) -> anyhow::Result<vk::Pipeline> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let module = unsafe { create_shader_module(device, spirv) }?;
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(c"main");
    let info = vk::ComputePipelineCreateInfo::default()
        .stage(stage)
        .layout(layout);
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let result =
        unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) };
    // SAFETY: made by this device, and no frame in flight uses it any more.
    unsafe { device.destroy_shader_module(module, None) };
    match result {
        Ok(pipelines) => Ok(pipelines[0]),
        Err((_, e)) => Err(e.into()),
    }
}

unsafe fn create_visible(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    slots: u32,
) -> anyhow::Result<Visible> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let (buffer, memory) = unsafe {
        create_buffer(
            device,
            mem_props,
            u64::from(slots) * 4,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            super::Place::DEVICE,
        )
    }?;
    Ok(Visible {
        buffer,
        memory,
        slots,
        cleared: false,
    })
}

/// A barrier from `src` to `dst` on every memory.
unsafe fn memory_barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    (src_stage, src_access): (vk::PipelineStageFlags, vk::AccessFlags),
    (dst_stage, dst_access): (vk::PipelineStageFlags, vk::AccessFlags),
) {
    let barrier = vk::MemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access);
    // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        )
    };
}

impl Occlusion {
    /// `draw_count` may be `None` only where nothing is drawn (the tests).
    pub unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frames: usize,
        draw_count: Option<vk::PFN_vkCmdDrawIndexedIndirectCount>,
    ) -> anyhow::Result<Self> {
        use vk::DescriptorType as Ty;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let reduce_layout =
            unsafe { set_layout(device, &[Ty::COMBINED_IMAGE_SAMPLER, Ty::STORAGE_IMAGE]) }?;
        // `occlusion.glsl`'s bindings.
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let cull_layout = unsafe {
            set_layout(
                device,
                &[
                    Ty::STORAGE_BUFFER,
                    Ty::STORAGE_BUFFER,
                    Ty::STORAGE_BUFFER,
                    Ty::COMBINED_IMAGE_SAMPLER,
                    Ty::STORAGE_BUFFER,
                    Ty::STORAGE_BUFFER,
                    Ty::STORAGE_BUFFER,
                ],
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let reduce_pipeline_layout =
            unsafe { pipeline_layout(device, reduce_layout, HIZ_PUSH_SIZE) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let cull_pipeline_layout = unsafe { pipeline_layout(device, cull_layout, CULL_PUSH_SIZE) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let hiz = unsafe { compute_pipeline(device, reduce_pipeline_layout, HIZ_SPV) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let select = unsafe { compute_pipeline(device, cull_pipeline_layout, SELECT_SPV) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let cull = unsafe { compute_pipeline(device, cull_pipeline_layout, CULL_SPV) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let compact = unsafe { compute_pipeline(device, cull_pipeline_layout, COMPACT_SPV) }?;
        let frames = frames as u32;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: Ty::COMBINED_IMAGE_SAMPLER,
                descriptor_count: MAX_LEVELS + frames,
            },
            vk::DescriptorPoolSize {
                ty: Ty::STORAGE_IMAGE,
                descriptor_count: MAX_LEVELS,
            },
            vk::DescriptorPoolSize {
                ty: Ty::STORAGE_BUFFER,
                descriptor_count: 6 * frames,
            },
        ];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&sizes)
                    .max_sets(MAX_LEVELS + frames),
                None,
            )
        }?;
        let allocate = |layout, count: u32| {
            let layouts = vec![layout; count as usize];
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                device.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&layouts),
                )
            }
        };
        let reduce_sets = allocate(reduce_layout, MAX_LEVELS)?;
        let cull_sets = allocate(cull_layout, frames)?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sampler = unsafe {
            create_sampler(
                device,
                vk::Filter::NEAREST,
                true,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
            )
        }?;
        Ok(Self {
            reduce_layout,
            cull_layout,
            reduce_pipeline_layout,
            cull_pipeline_layout,
            hiz,
            select,
            cull,
            compact,
            pool,
            sampler,
            reduce_sets,
            cull_sets,
            // SAFETY: `device` is live, and the infos name only its own live handles.
            visible: unsafe { create_visible(device, mem_props, MIN_SLOTS) }?,
            pyramid: None,
            draw_count,
        })
    }

    /// Whether there is a pyramid to cull against.
    pub fn ready(&self) -> bool {
        self.pyramid.is_some()
    }

    /// Builds the pyramid for a new depth buffer `depth` of `screen`'s size.
    pub unsafe fn resize(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        depth: vk::Image,
        depth_format: vk::Format,
        screen: vk::Extent2D,
    ) -> anyhow::Result<()> {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.release_sized(device) };
        if screen.width == 0 || screen.height == 0 {
            return Ok(());
        }
        let levels = level_count(screen);
        let base = base_size(screen);
        let format = vk::Format::R32_SFLOAT;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let image = unsafe {
            device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: base.width,
                        height: base.height,
                        depth: 1,
                    })
                    .mip_levels(levels)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let req = unsafe { device.get_image_memory_requirements(image) };
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let memory =
            match unsafe { vk_util::allocate_for(device, mem_props, req, super::Place::DEVICE) } {
                Ok((memory, _)) => memory,
                Err(e) => {
                    // SAFETY: made by this device, and no frame in flight uses it any more.
                    unsafe { device.destroy_image(image, None) };
                    return Err(e);
                }
            };
        let mut pyramid = Pyramid {
            image,
            memory,
            levels: Vec::new(),
            whole: vk::ImageView::null(),
            depth: vk::ImageView::null(),
            depth_image: depth,
            depth_aspect: vk_util::aspect_mask(depth_format),
            screen,
        };
        let built = (|| -> anyhow::Result<()> {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { device.bind_image_memory(image, memory, 0) }?;
            let view = |image, format, aspect, base_mip_level, level_count| {
                // SAFETY: `device` is live, and the infos name only its own live handles.
                unsafe {
                    device.create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(image)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(format)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: aspect,
                                base_mip_level,
                                level_count,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                        None,
                    )
                }
            };
            let color = vk::ImageAspectFlags::COLOR;
            for level in 0..levels {
                pyramid.levels.push(view(image, format, color, level, 1)?);
            }
            pyramid.whole = view(image, format, color, 0, levels)?;
            pyramid.depth = view(depth, depth_format, vk::ImageAspectFlags::DEPTH, 0, 1)?;
            Ok(())
        })();
        if let Err(e) = built {
            for &view in pyramid
                .levels
                .iter()
                .chain([&pyramid.whole, &pyramid.depth])
            {
                if view != vk::ImageView::null() {
                    // SAFETY: made by this device, and no frame in flight uses it any more.
                    unsafe { device.destroy_image_view(view, None) };
                }
            }
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                device.destroy_image(image, None);
                vk_util::free_memory(device, memory);
            }
            return Err(e);
        }
        for level in 0..levels {
            let (src, src_layout) = if level == 0 {
                (
                    pyramid.depth,
                    vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                )
            } else {
                (pyramid.levels[level as usize - 1], vk::ImageLayout::GENERAL)
            };
            let src_info = [vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(src)
                .image_layout(src_layout)];
            let dst_info = [vk::DescriptorImageInfo::default()
                .image_view(pyramid.levels[level as usize])
                .image_layout(vk::ImageLayout::GENERAL)];
            let set = self.reduce_sets[level as usize];
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                device.update_descriptor_sets(
                    &[
                        vk::WriteDescriptorSet::default()
                            .dst_set(set)
                            .dst_binding(0)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&src_info),
                        vk::WriteDescriptorSet::default()
                            .dst_set(set)
                            .dst_binding(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .image_info(&dst_info),
                    ],
                    &[],
                )
            };
        }
        self.pyramid = Some(pyramid);
        Ok(())
    }

    /// Drops the pyramid; no frame in flight may use it.
    pub unsafe fn release_sized(&mut self, device: &ash::Device) {
        if let Some(pyramid) = self.pyramid.take() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { pyramid.destroy(device) };
        }
    }

    /// Points frame slot `frame`'s set at its draw buffers, making room for
    /// `slots` sections first. Call once its fence signalled, with buffers
    /// that hold at least one draw.
    pub unsafe fn prepare(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
        slots: u32,
        buffers: super::indirect::CullingBuffers,
    ) -> anyhow::Result<()> {
        if slots > self.visible.slots {
            // Only while a world loads, a few times at most.
            // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
            unsafe { device.device_wait_idle() }?;
            // SAFETY: `device` is live, and the infos name only its own live handles.
            let grown = unsafe { create_visible(device, mem_props, slots.next_power_of_two()) }?;
            let old = std::mem::replace(&mut self.visible, grown);
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                device.destroy_buffer(old.buffer, None);
                vk_util::free_memory(device, old.memory);
            }
        }
        let Some(pyramid) = &self.pyramid else {
            return Ok(());
        };
        let buffer = |buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)]
        };
        let (commands, bounds, visible) = (
            buffer(buffers.commands),
            buffer(buffers.bounds),
            buffer(self.visible.buffer),
        );
        let (keep, runs, counts) = (
            buffer(buffers.keep),
            buffer(buffers.runs),
            buffer(buffers.counts),
        );
        let hiz = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(pyramid.whole)
            .image_layout(vk::ImageLayout::GENERAL)];
        let set = self.cull_sets[frame];
        let write = |binding, ty| {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(ty)
        };
        use vk::DescriptorType as Ty;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            device.update_descriptor_sets(
                &[
                    write(0, Ty::STORAGE_BUFFER).buffer_info(&commands),
                    write(1, Ty::STORAGE_BUFFER).buffer_info(&bounds),
                    write(2, Ty::STORAGE_BUFFER).buffer_info(&visible),
                    write(3, Ty::COMBINED_IMAGE_SAMPLER).image_info(&hiz),
                    write(4, Ty::STORAGE_BUFFER).buffer_info(&keep),
                    write(5, Ty::STORAGE_BUFFER).buffer_info(&runs),
                    write(6, Ty::STORAGE_BUFFER).buffer_info(&counts),
                ],
                &[],
            )
        };
        Ok(())
    }

    /// Picks phase 1's draws among `count` in `runs` runs: the ones visible
    /// last frame. Outside a render pass, before the frame's first draw.
    pub unsafe fn record_select(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        (count, runs): (u32, u32),
    ) {
        if !self.visible.cleared {
            self.visible.cleared = true;
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { device.cmd_fill_buffer(cmd, self.visible.buffer, 0, vk::WHOLE_SIZE, 0) };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                memory_barrier(
                    device,
                    cmd,
                    (
                        vk::PipelineStageFlags::TRANSFER,
                        vk::AccessFlags::TRANSFER_WRITE,
                    ),
                    (
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::AccessFlags::SHADER_READ,
                    ),
                )
            };
        }
        // The last frame's results.
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            memory_barrier(
                device,
                cmd,
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                ),
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_READ,
                ),
            )
        };
        let words = push(Default::default(), count, [0.0; 2], 0);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.dispatch(
                device,
                cmd,
                self.select,
                frame,
                &words,
                count.div_ceil(DRAWS_PER_GROUP),
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.record_compact(device, cmd, frame, (count, runs), 0) };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            memory_barrier(
                device,
                cmd,
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                ),
                (
                    vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::INDIRECT_COMMAND_READ | vk::AccessFlags::SHADER_READ,
                ),
            )
        };
    }

    /// Packs phase `phase`'s kept draws, after a barrier for what picked them.
    unsafe fn record_compact(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        (count, runs): (u32, u32),
        phase: u32,
    ) {
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            memory_barrier(
                device,
                cmd,
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                ),
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_READ,
                ),
            )
        };
        let words = push(Default::default(), count, [0.0; 2], phase);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.dispatch(device, cmd, self.compact, frame, &words, runs) };
    }

    /// Builds the pyramid from the depth phase 1 left and picks phase 2's
    /// draws. Between the render passes of the two phases; the depth buffer
    /// in `DEPTH_STENCIL_READ_ONLY_OPTIMAL`.
    pub unsafe fn record_cull(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        (count, runs): (u32, u32),
        view_proj: &glam::Mat4,
    ) {
        let Some(pyramid) = &self.pyramid else {
            return;
        };
        let levels = pyramid.levels.len() as u32;
        let range = |aspect_mask, base_mip_level, level_count| vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level,
            level_count,
            base_array_layer: 0,
            layer_count: 1,
        };
        let depth_layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
        let barriers = [
            // Phase 1's depth, for level 0.
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(depth_layout)
                .new_layout(depth_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(pyramid.depth_image)
                .subresource_range(range(pyramid.depth_aspect, 0, 1)),
            // Last frame's pyramid is rewritten whole.
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(pyramid.image)
                .subresource_range(range(vk::ImageAspectFlags::COLOR, 0, levels)),
        ];
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_GRAPHICS | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.hiz) };
        for level in 0..levels {
            let dst = pyramid.size(level);
            let src = if level == 0 {
                pyramid.screen
            } else {
                pyramid.size(level - 1)
            };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    self.reduce_pipeline_layout,
                    0,
                    &[self.reduce_sets[level as usize]],
                    &[],
                )
            };
            let sizes = [dst.width, dst.height, src.width, src.height].map(|v| v as i32);
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    self.reduce_pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    as_bytes(&sizes),
                )
            };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_dispatch(
                    cmd,
                    dst.width.div_ceil(HIZ_GROUP),
                    dst.height.div_ceil(HIZ_GROUP),
                    1,
                )
            };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                memory_barrier(
                    device,
                    cmd,
                    (
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::AccessFlags::SHADER_WRITE,
                    ),
                    (
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::AccessFlags::SHADER_READ,
                    ),
                )
            };
        }
        let screen = [pyramid.screen.width as f32, pyramid.screen.height as f32];
        let words = push(*view_proj, count, screen, levels);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.dispatch(
                device,
                cmd,
                self.cull,
                frame,
                &words,
                count.div_ceil(DRAWS_PER_GROUP),
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.record_compact(device, cmd, frame, (count, runs), 1) };
        // Phase 2 reads the commands, and writes the depth this read.
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            memory_barrier(
                device,
                cmd,
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                ),
                (
                    vk::PipelineStageFlags::DRAW_INDIRECT
                        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    vk::AccessFlags::INDIRECT_COMMAND_READ,
                ),
            )
        };
    }

    /// Runs `pipeline` in `groups` workgroups with the push constants `push`.
    unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline: vk::Pipeline,
        frame: usize,
        push: &[u32; 20],
        groups: u32,
    ) {
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline) };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.cull_pipeline_layout,
                0,
                &[self.cull_sets[frame]],
                &[],
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_push_constants(
                cmd,
                self.cull_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                as_bytes(push),
            )
        };
        if groups > 0 {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { device.cmd_dispatch(cmd, groups, 1, 1) };
        }
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            self.release_sized(device);
            device.destroy_buffer(self.visible.buffer, None);
            vk_util::free_memory(device, self.visible.memory);
            device.destroy_sampler(self.sampler, None);
            device.destroy_descriptor_pool(self.pool, None);
        }
        for pipeline in [self.hiz, self.select, self.cull, self.compact] {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_pipeline(pipeline, None) };
        }
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_pipeline_layout(self.reduce_pipeline_layout, None);
            device.destroy_pipeline_layout(self.cull_pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.reduce_layout, None);
            device.destroy_descriptor_set_layout(self.cull_layout, None);
        }
    }
}

/// `occlusion.glsl`'s push constants as words: `view_proj`, `screen`, `count`, `extra`.
fn push(view_proj: glam::Mat4, count: u32, screen: [f32; 2], extra: u32) -> [u32; 20] {
    let mut words = [0u32; 20];
    for (word, value) in words.iter_mut().zip(view_proj.to_cols_array()) {
        *word = value.to_bits();
    }
    words[16] = screen[0].to_bits();
    words[17] = screen[1].to_bits();
    words[18] = count;
    words[19] = extra;
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_reach_one_texel() {
        let at = |width, height| level_count(vk::Extent2D { width, height });
        // 1920x1080 at level 0, down to 1x1 at level 10.
        assert_eq!(at(3840, 2160), 11);
        assert_eq!(at(2, 2), 1);
        assert_eq!(at(5, 1), 2);
        assert_eq!(at(1, 1), 1);
    }

    #[test]
    fn push_constants_match_the_shader() {
        assert_eq!(std::mem::size_of::<[u32; 20]>() as u32, CULL_PUSH_SIZE);
        let words = push(glam::Mat4::IDENTITY, 7, [3.0, 4.0], 9);
        assert_eq!(f32::from_bits(words[0]), 1.0);
        assert_eq!(f32::from_bits(words[17]), 4.0);
        assert_eq!((words[18], words[19]), (7, 9));
    }
}
