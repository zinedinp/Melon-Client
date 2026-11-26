//! Small Vulkan helpers.

use ash::vk;
use numerobis::vulkan::alloc::{AllocationCount, Usage};
use numerobis::vulkan::format::{self, Depth};
use numerobis::vulkan::memory::Memory;
use numerobis::vulkan::swapchain::{self, Encoding, Extent, SurfaceFormat, result};
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
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .mipmap_mode(mode)
                .address_mode_u(address)
                .address_mode_v(address)
                .address_mode_w(address)
                .max_lod(max_lod),
            None,
        )
    }?)
}

/// Where a resource's memory goes: numerobis's waterfall for `usage`, host
/// memory first when `host_first` (VRAM past its budget, or a staging copy
/// the BAR window is better kept for).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub usage: Usage,
    pub host_first: bool,
}

impl Place {
    /// Written by the CPU every frame, read by the GPU: VRAM it maps, else host memory.
    pub const UPLOAD: Self = Self::new(Usage::Upload);
    /// Written by the CPU once, copied by the GPU: host memory, so the BAR stays for [`Self::UPLOAD`].
    pub const STAGING: Self = Self {
        usage: Usage::Upload,
        host_first: true,
    };
    /// Written by the GPU, read back by the CPU: cached host memory.
    pub const READBACK: Self = Self::new(Usage::Readback);
    /// Only the GPU touches it, a copy fills it: VRAM, else host memory.
    pub const DEVICE: Self = Self::new(Usage::Device);

    pub const fn new(usage: Usage) -> Self {
        Self {
            usage,
            host_first: false,
        }
    }
}

/// A buffer with its own memory from `place`, bound.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn create_buffer(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
    place: Place,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    unsafe { create_buffer_placed(device, mem_props, size, usage, place) }
        .map(|(buffer, memory, _)| (buffer, memory))
}

/// [`create_buffer`], and the flags of the memory type it took.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn create_buffer_placed(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
    place: Place,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory, vk::MemoryPropertyFlags)> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let buffer = unsafe {
        device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(size)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )
    }?;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let bound = unsafe { allocate_for(device, mem_props, requirements, place) }.and_then(
        |(memory, flags)| {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            match unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
                Ok(()) => Ok((memory, flags)),
                Err(e) => {
                    // SAFETY: made by this device, and nothing uses it yet.
                    unsafe { free_memory(device, memory) };
                    Err(e.into())
                }
            }
        },
    );
    match bound {
        Ok((memory, flags)) => Ok((buffer, memory, flags)),
        Err(e) => {
            // SAFETY: made by this device, and nothing uses it yet.
            unsafe { device.destroy_buffer(buffer, None) };
            Err(e)
        }
    }
}

/// Memory for `requirements` from the first of `place`'s memory types that
/// has room, and that type's flags: a full heap (the VRAM, the BAR window)
/// falls through to the next type.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn allocate_for(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    requirements: vk::MemoryRequirements,
    place: Place,
) -> anyhow::Result<(vk::DeviceMemory, vk::MemoryPropertyFlags)> {
    let mut last = anyhow::anyhow!("no suitable Vulkan memory type found");
    let mut out_of_memory = false;
    for index in memory_types(mem_props, requirements.memory_type_bits, place) {
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(index);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        match unsafe { allocate_memory(device, &info) } {
            Ok(memory) => {
                let flags = mem_props.memory_types[index as usize].property_flags;
                if out_of_memory {
                    note_memory_fallback(flags);
                }
                return Ok((memory, flags));
            }
            Err(e) => {
                out_of_memory |= is_out_of_memory(e);
                last = e.into();
            }
        }
    }
    Err(last)
}

/// The memory types among `type_bits` for `place`, in the order to try them.
pub fn memory_types(
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    place: Place,
) -> Vec<u32> {
    // SAFETY: ash's `PhysicalDeviceMemoryProperties` is the C struct numerobis reads.
    let memory = unsafe { Memory::from_properties(std::ptr::from_ref(mem_props).cast()) };
    memory.map_or_else(Vec::new, |memory| {
        memory.types_for(type_bits, place.usage, place.host_first)
    })
}

/// Whether `e` says a heap is full, so the next memory type may still have room.
pub fn is_out_of_memory(e: vk::Result) -> bool {
    matches!(
        e,
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY | vk::Result::ERROR_OUT_OF_HOST_MEMORY
    )
}

/// A UNORM format, as vanilla's GL framebuffer stores colour (an `_SRGB` one
/// would encode it a second time), by numerobis's order.
pub fn choose_surface_format(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> anyhow::Result<vk::SurfaceFormatKHR> {
    let formats =
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };
    let offered: Vec<SurfaceFormat> = formats
        .iter()
        .map(|f| SurfaceFormat {
            format: f.format.as_raw() as u32,
            color_space: f.color_space.as_raw() as u32,
        })
        .collect();
    let chosen = swapchain::surface_format(&offered, Encoding::Unorm)
        .ok_or_else(|| anyhow::anyhow!("the surface offers no format"))?;
    Ok(vk::SurfaceFormatKHR {
        format: vk::Format::from_raw(chosen.format as i32),
        color_space: vk::ColorSpaceKHR::from_raw(chosen.color_space as i32),
    })
}

/// What a new swapchain takes from the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapchainShape {
    pub extent: vk::Extent2D,
    pub image_count: u32,
    pub composite_alpha: vk::CompositeAlphaFlagsKHR,
}

/// The shape for a `window` of that size and at least `images` images.
/// `None` while the surface has no pixels (minimized) and an old swapchain
/// can stay (`has_old`); without one, 1x1.
pub fn swapchain_shape(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    window: vk::Extent2D,
    images: u32,
    has_old: bool,
) -> Option<SwapchainShape> {
    let capabilities = surface_capabilities(capabilities);
    let extent = match capabilities.extent(extent_in(window)) {
        Some(extent) => extent,
        None if has_old => return None,
        None => Extent::new(1, 1),
    };
    Some(SwapchainShape {
        extent: extent_out(extent),
        image_count: capabilities.image_count(images),
        composite_alpha: vk::CompositeAlphaFlagsKHR::from_raw(capabilities.composite_alpha()),
    })
}

fn surface_capabilities(c: &vk::SurfaceCapabilitiesKHR) -> swapchain::SurfaceCapabilities {
    swapchain::SurfaceCapabilities {
        min_image_count: c.min_image_count,
        max_image_count: c.max_image_count,
        current_extent: extent_in(c.current_extent),
        min_image_extent: extent_in(c.min_image_extent),
        max_image_extent: extent_in(c.max_image_extent),
        max_image_array_layers: c.max_image_array_layers,
        supported_transforms: c.supported_transforms.as_raw(),
        current_transform: c.current_transform.as_raw(),
        supported_composite_alpha: c.supported_composite_alpha.as_raw(),
        supported_usage_flags: c.supported_usage_flags.as_raw(),
    }
}

fn extent_in(e: vk::Extent2D) -> Extent {
    Extent::new(e.width, e.height)
}

fn extent_out(e: Extent) -> vk::Extent2D {
    vk::Extent2D {
        width: e.width,
        height: e.height,
    }
}

/// Whether a `vkQueuePresentKHR` result asks for a new swapchain, by
/// numerobis's policy: always when out of date, when suboptimal only if the
/// surface's size (`surface_extent`) differs from `extent`.
pub fn recreate_after_present(
    presented: ash::prelude::VkResult<bool>,
    extent: vk::Extent2D,
    surface_extent: impl FnOnce() -> Option<vk::Extent2D>,
) -> ash::prelude::VkResult<bool> {
    let raw = match presented {
        Ok(false) => result::SUCCESS,
        Ok(true) => result::SUBOPTIMAL,
        Err(e) => e.as_raw(),
    };
    swapchain::recreate(raw, extent_in(extent), || surface_extent().map(extent_in))
        .map_err(vk::Result::from_raw)
}

/// The surface's `currentExtent`; `None` where the swapchain picks it (Wayland) or it can't be read.
///
/// # Safety
///
/// `surface` must be live and made from the instance `loader` is for.
pub unsafe fn surface_extent(
    loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Option<vk::Extent2D> {
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let capabilities =
        unsafe { loader.get_physical_device_surface_capabilities(physical_device, surface) }
            .ok()?;
    surface_capabilities(&capabilities)
        .current()
        .map(extent_out)
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
        .src_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );
    let dependencies = [dependency];

    let info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe { device.create_render_pass(&info, None) }?)
}

/// The depth format: numerobis's first candidate shaders can also sample,
/// else the first the device can attach at all.
///
/// # Safety
///
/// `physical_device` must belong to `instance`.
pub unsafe fn find_depth_format(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> anyhow::Result<vk::Format> {
    let optimal = |raw: u32| {
        let format = vk::Format::from_raw(raw as i32);
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        let props =
            unsafe { instance.get_physical_device_format_properties(physical_device, format) };
        props.optimal_tiling_features.as_raw()
    };
    let sampled = Depth {
        stencil: false,
        sampled: true,
    };
    format::depth(sampled, optimal)
        .or_else(|| format::depth(Depth::default(), optimal))
        .map(|raw| vk::Format::from_raw(raw as i32))
        .ok_or_else(|| anyhow::anyhow!("no supported depth buffer format"))
}

/// The aspects of a view of all of `format`: depth, and stencil when it has one.
pub fn aspect_mask(format: vk::Format) -> vk::ImageAspectFlags {
    vk::ImageAspectFlags::from_raw(format::aspect_mask(format.as_raw() as u32))
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
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let image = unsafe {
        device.create_image(
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
        )
    }?;
    let mut memory = vk::DeviceMemory::null();
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let view =
        unsafe { depth_memory_and_view(device, mem_props, image, format, sampled, &mut memory) };
    match view {
        Ok(view) => Ok((image, memory, view)),
        Err(e) => {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                free_memory(device, memory);
                device.destroy_image(image, None);
            }
            Err(e)
        }
    }
}

/// Binds memory to the depth `image` (left in `memory`, for the caller to free on failure) and makes its view.
unsafe fn depth_memory_and_view(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
    format: vk::Format,
    sampled: bool,
    memory: &mut vk::DeviceMemory,
) -> anyhow::Result<vk::ImageView> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let req = unsafe { device.get_image_memory_requirements(image) };
    // Nothing samples an unsampled depth buffer: tilers may never back it.
    let usage = if sampled {
        Usage::Device
    } else {
        Usage::Transient
    };
    // SAFETY: `device` is live, and the infos name only its own live handles.
    *memory = unsafe { allocate_for(device, mem_props, req, Place::new(usage)) }?.0;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    unsafe { device.bind_image_memory(image, *memory, 0) }?;

    let aspect_mask = aspect_mask(format);
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_image_view(
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
        )
    }?)
}

/// Once per run: a buffer went to a fallback memory because the preferred
/// one was full (another program on the GPU), which can cost frame time.
pub(super) fn note_memory_fallback(flags: vk::MemoryPropertyFlags) {
    static NOTED: std::sync::Once = std::sync::Once::new();
    NOTED.call_once(|| {
        crate::warn!(
            "vulkan: preferred memory full (another program using the GPU?), \
             falling back to {flags:?}"
        );
    });
}

/// Live `vkAllocateMemory` allocations, of every device, against the
/// device's `maxMemoryAllocationCount`.
static ALLOCATIONS: AllocationCount = AllocationCount::new(AllocationCount::DEFAULT_LIMIT);

/// Sets the limit [`allocate_memory`] warns against.
pub fn set_allocation_limit(limit: u32) {
    ALLOCATIONS.set_limit(limit);
}

/// `vkAllocateMemory`, counted; warns once the count nears `maxMemoryAllocationCount`.
///
/// # Safety
///
/// `device` must be live.
pub unsafe fn allocate_memory(
    device: &ash::Device,
    info: &vk::MemoryAllocateInfo<'_>,
) -> ash::prelude::VkResult<vk::DeviceMemory> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let memory = unsafe { device.allocate_memory(info, None) }?;
    if let Some(live) = ALLOCATIONS.allocated() {
        crate::warn!(
            "vulkan: {live} memory allocations live, the device allows {}",
            ALLOCATIONS.limit()
        );
    }
    Ok(memory)
}

/// `vkFreeMemory` of memory from [`allocate_memory`]; null is skipped.
///
/// # Safety
///
/// `memory` must be from `device` and no longer in use.
pub unsafe fn free_memory(device: &ash::Device, memory: vk::DeviceMemory) {
    if memory == vk::DeviceMemory::null() {
        return;
    }
    // SAFETY: made by this device, and no frame in flight uses it any more.
    unsafe { device.free_memory(memory, None) };
    ALLOCATIONS.freed();
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
    // SAFETY: Vulkan's fixed-size name arrays are NUL-terminated.
    let message = unsafe { CStr::from_ptr((*data).p_message) }.to_string_lossy();
    crate::warn!("[vulkan] {message}");
    vk::FALSE
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTENT: vk::Extent2D = vk::Extent2D {
        width: 854,
        height: 480,
    };

    #[test]
    fn suboptimal_recreates_only_when_the_surface_resized() {
        let grown = vk::Extent2D {
            width: 1280,
            height: 720,
        };
        let recreate = |presented, now| recreate_after_present(presented, EXTENT, || now);
        assert_eq!(recreate(Ok(false), Some(grown)), Ok(false));
        assert_eq!(recreate(Ok(true), Some(EXTENT)), Ok(false));
        assert_eq!(recreate(Ok(true), None), Ok(false));
        assert_eq!(recreate(Ok(true), Some(vk::Extent2D::default())), Ok(false));
        assert_eq!(recreate(Ok(true), Some(grown)), Ok(true));
        assert_eq!(
            recreate(Err(vk::Result::ERROR_OUT_OF_DATE_KHR), Some(EXTENT)),
            Ok(true)
        );
        assert_eq!(
            recreate(Err(vk::Result::ERROR_DEVICE_LOST), None),
            Err(vk::Result::ERROR_DEVICE_LOST)
        );
    }
}
