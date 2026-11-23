//! Small Vulkan helpers.

use ash::vk;
use std::ffi::CStr;

/// `mipmapped`.
///
/// # Safety
///
/// `device` must be live.
pub unsafe fn create_sampler(
    device: &ash::Device,
    filter: vk::Filter,
    mipmapped: bool,
    address: vk::SamplerAddressMode,
) -> anyhow::Result<vk::Sampler> {
    let (mode, max_lod) = if mipmapped {
        (vk::SamplerMipmapMode::LINEAR, vk::LOD_CLAMP_NONE)
    } else {
        (vk::SamplerMipmapMode::NEAREST, 0.0)
    };
    Ok(device.create_sampler(
        &vk::SamplerCreateInfo::default()
            .mag_filter(filter)
            .min_filter(filter)
            .mipmap_mode(mode)
            .address_mode_u(address)
            .address_mode_v(address)
            .address_mode_w(address)
            .max_lod(max_lod),
        None,
    )?)
}

/// Memory for buffers the CPU writes through a mapping and the GPU reads
/// every frame: VRAM the CPU can map (resizable BAR, or the 256 MiB window
/// most GPUs expose) where there is one, else host memory over PCIe. The
/// mapping is write-combined either way, so it's only ever written.
pub const MAPPED_FOR_GPU: [vk::MemoryPropertyFlags; 2] = [
    vk::MemoryPropertyFlags::from_raw(
        vk::MemoryPropertyFlags::DEVICE_LOCAL.as_raw()
            | vk::MemoryPropertyFlags::HOST_VISIBLE.as_raw()
            | vk::MemoryPropertyFlags::HOST_COHERENT.as_raw(),
    ),
    vk::MemoryPropertyFlags::from_raw(
        vk::MemoryPropertyFlags::HOST_VISIBLE.as_raw()
            | vk::MemoryPropertyFlags::HOST_COHERENT.as_raw(),
    ),
];

/// Memory for what the GPU reads and only a copy writes (meshes): VRAM,
/// else, while another program holds the VRAM, host memory over PCIe:
/// slower to draw from, but drawn.
pub const DEVICE_OR_HOST: [vk::MemoryPropertyFlags; 2] = [
    vk::MemoryPropertyFlags::DEVICE_LOCAL,
    vk::MemoryPropertyFlags::from_raw(
        vk::MemoryPropertyFlags::HOST_VISIBLE.as_raw()
            | vk::MemoryPropertyFlags::HOST_COHERENT.as_raw(),
    ),
];

/// Memory for buffers the GPU writes and the CPU reads back: cached, as
/// reading uncached (write-combined) memory costs the CPU many times more.
pub const READBACK: [vk::MemoryPropertyFlags; 2] = [
    vk::MemoryPropertyFlags::from_raw(
        vk::MemoryPropertyFlags::HOST_VISIBLE.as_raw()
            | vk::MemoryPropertyFlags::HOST_COHERENT.as_raw()
            | vk::MemoryPropertyFlags::HOST_CACHED.as_raw(),
    ),
    vk::MemoryPropertyFlags::from_raw(
        vk::MemoryPropertyFlags::HOST_VISIBLE.as_raw()
            | vk::MemoryPropertyFlags::HOST_COHERENT.as_raw(),
    ),
];

/// A buffer with its own memory, bound.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn create_buffer(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
    flags: vk::MemoryPropertyFlags,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    create_buffer_in(device, mem_props, size, usage, &[flags])
}

/// A buffer with its own memory, bound, from the first of `choices` that a
/// memory type has and whose heap has room: a small heap such as the BAR
/// window can run out where host memory still has plenty.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn create_buffer_in(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
    choices: &[vk::MemoryPropertyFlags],
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let buffer = device.create_buffer(
        &vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE),
        None,
    )?;
    let requirements = device.get_buffer_memory_requirements(buffer);
    let mut last = anyhow::anyhow!("no suitable Vulkan memory type found");
    let mut out_of_memory = false;
    for &flags in choices {
        // Every type with the flags, not only the first: a full heap (the
        // VRAM, the BAR window) can have a type the next heap shares flags with.
        for index in memory_types(mem_props, requirements.memory_type_bits, flags) {
            match device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(index),
                None,
            ) {
                Ok(memory) => {
                    if let Err(e) = device.bind_buffer_memory(buffer, memory, 0) {
                        device.free_memory(memory, None);
                        device.destroy_buffer(buffer, None);
                        return Err(e.into());
                    }
                    if out_of_memory {
                        note_memory_fallback(flags);
                    }
                    return Ok((buffer, memory));
                }
                Err(e) => {
                    out_of_memory |= matches!(
                        e,
                        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY
                            | vk::Result::ERROR_OUT_OF_HOST_MEMORY
                    );
                    last = e.into();
                }
            }
        }
    }
    device.destroy_buffer(buffer, None);
    Err(last)
}

pub fn choose_surface_format(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> anyhow::Result<vk::SurfaceFormatKHR> {
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };
    Ok(formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .copied()
        .unwrap_or(formats[0]))
}

/// How a frame's render pass starts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PassStart {
    /// Clears colour and depth: the scene.
    Clear,
    /// Paints over every pixel itself (the blur's last draw), so nothing is cleared.
    Overwrite,
    /// The scene's pass when a post chain follows.
    SceneSplit,
    /// What follows a post chain.
    Resume,
    /// terrain's occlusion culling: loads the scene and keeps
    /// its depth for a post chain to read, as [`Self::SceneSplit`] does.
    ResumeKeep,
    /// A [`super::PictureDraw`]'s own target: clears, and leaves the colour
    /// as an attachment for the barrier that hands it to the UI.
    Picture,
}

/// The scene's render pass: one colour and one depth attachment, loaded as `start` says.
///
/// # Safety
///
/// `device` must be live.
pub unsafe fn create_render_pass(
    device: &ash::Device,
    format: vk::Format,
    depth_format: vk::Format,
    start: PassStart,
) -> anyhow::Result<vk::RenderPass> {
    let load_op = match start {
        PassStart::Clear | PassStart::SceneSplit | PassStart::Picture => {
            vk::AttachmentLoadOp::CLEAR
        }
        PassStart::Overwrite => vk::AttachmentLoadOp::DONT_CARE,
        PassStart::Resume | PassStart::ResumeKeep => vk::AttachmentLoadOp::LOAD,
    };
    let resume = matches!(start, PassStart::Resume | PassStart::ResumeKeep);
    let keep_depth = matches!(start, PassStart::SceneSplit | PassStart::ResumeKeep);
    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(if resume {
            vk::ImageLayout::PRESENT_SRC_KHR
        } else {
            vk::ImageLayout::UNDEFINED
        })
        .final_layout(if start == PassStart::Picture {
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        } else {
            vk::ImageLayout::PRESENT_SRC_KHR
        });
    let depth_attachment = vk::AttachmentDescription::default()
        .format(depth_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(load_op)
        .store_op(if keep_depth {
            vk::AttachmentStoreOp::STORE
        } else {
            vk::AttachmentStoreOp::DONT_CARE
        })
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(if resume {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
        } else {
            vk::ImageLayout::UNDEFINED
        })
        .final_layout(if keep_depth {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
        } else {
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        });
    let attachments = [color_attachment, depth_attachment];

    let color_ref = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_ref)
        .depth_stencil_attachment(&depth_ref);
    let subpasses = [subpass];

    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                | vk::PipelineStageFlags::TRANSFER
                | vk::PipelineStageFlags::FRAGMENT_SHADER,
        )
        .dst_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );
    let dependencies = [dependency];

    let info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    Ok(device.create_render_pass(&info, None)?)
}

/// First supported candidate with `DEPTH_STENCIL_ATTACHMENT` optimal-tiling support.
///
/// # Safety
///
/// `physical_device` must belong to `instance`.
pub unsafe fn find_depth_format(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> anyhow::Result<vk::Format> {
    for format in [
        vk::Format::D32_SFLOAT,
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
    ] {
        let props = instance.get_physical_device_format_properties(physical_device, format);
        if props
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            return Ok(format);
        }
    }
    anyhow::bail!("no supported depth buffer format")
}

/// A depth image with its memory and view; `sampled` lets shaders read it.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn create_depth_image(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    format: vk::Format,
    extent: vk::Extent2D,
    sampled: bool,
) -> anyhow::Result<(vk::Image, vk::DeviceMemory, vk::ImageView)> {
    let image = device.create_image(
        &vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(if sampled {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
            } else {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
            })
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED),
        None,
    )?;
    let req = device.get_image_memory_requirements(image);
    let memory = device.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(
                find_memory_type(
                    mem_props,
                    req.memory_type_bits,
                    if sampled {
                        vk::MemoryPropertyFlags::DEVICE_LOCAL
                    } else {
                        vk::MemoryPropertyFlags::LAZILY_ALLOCATED
                    },
                )
                .or_else(|_| {
                    find_memory_type(
                        mem_props,
                        req.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )
                })?,
            ),
        None,
    )?;
    device.bind_image_memory(image, memory, 0)?;

    let has_stencil = matches!(
        format,
        vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT
    );
    let aspect_mask = if has_stencil {
        vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
    } else {
        vk::ImageAspectFlags::DEPTH
    };
    let view = device.create_image_view(
        &vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            }),
        None,
    )?;
    Ok((image, memory, view))
}

/// The memory types with `flags` among `type_bits`, in the device's order.
fn memory_types(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> impl Iterator<Item = u32> + '_ {
    (0..props.memory_type_count).filter(move |&i| {
        (type_bits & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(flags)
    })
}

/// Once per run: a buffer went to a fallback memory because the preferred
/// one was full (another program on the GPU), which can cost frame time.
fn note_memory_fallback(flags: vk::MemoryPropertyFlags) {
    static NOTED: std::sync::Once = std::sync::Once::new();
    NOTED.call_once(|| {
        crate::warn!(
            "vulkan: preferred memory full (another program using the GPU?), \
             falling back to {flags:?}"
        );
    });
}

pub fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> anyhow::Result<u32> {
    for i in 0..props.memory_type_count {
        if (type_bits & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(flags)
        {
            return Ok(i);
        }
    }
    anyhow::bail!("no suitable Vulkan memory type found")
}

/// The debug messenger's callback: the validation layer's warnings and errors to the log.
///
/// # Safety
///
/// Only the Vulkan loader calls it, with a valid `data`.
pub unsafe extern "system" fn debug_callback(
    _severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let message = CStr::from_ptr((*data).p_message).to_string_lossy();
    crate::warn!("[vulkan] {message}");
    vk::FALSE
}
