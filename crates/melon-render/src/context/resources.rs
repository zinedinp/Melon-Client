//! The long-lived pools and buffers: samplers, per-frame sync, UI vertices,
//! the sky cube and the empty texture and swapchain state.

use ash::vk;

use crate::{
    DepthImage, FrameWaits, Frames, MAX_FRAMES_IN_FLIGHT, SkyCube, Swapchain, Textures,
    create_sampler, descriptor_pools::DescriptorPools, pipeline, staging::Staging,
};
use melon_base::gfx::TextureId;
use melon_base::gfx::mapped;

/// The samplers every texture picks from.
pub(super) struct Samplers {
    pub nearest: vk::Sampler,
    pub mipmap: vk::Sampler,
    pub linear: vk::Sampler,
    pub repeat: vk::Sampler,
    pub linear_repeat: vk::Sampler,
}

pub(super) unsafe fn create_samplers(device: &ash::Device) -> anyhow::Result<Samplers> {
    const NEAREST: vk::Filter = vk::Filter::NEAREST;
    const LINEAR: vk::Filter = vk::Filter::LINEAR;
    use vk::SamplerAddressMode as Address;
    let clamp = Address::CLAMP_TO_EDGE;
    Ok(Samplers {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        nearest: unsafe { create_sampler(device, NEAREST, false, clamp) }?,
        // SAFETY: `device` is live, and the infos name only its own live handles.
        mipmap: unsafe { create_sampler(device, NEAREST, true, clamp) }?,
        // SAFETY: `device` is live, and the infos name only its own live handles.
        linear: unsafe { create_sampler(device, LINEAR, false, clamp) }?,
        // SAFETY: `device` is live, and the infos name only its own live handles.
        repeat: unsafe { create_sampler(device, NEAREST, false, Address::REPEAT) }?,
        // SAFETY: `device` is live, and the infos name only its own live handles.
        linear_repeat: unsafe { create_sampler(device, LINEAR, false, Address::REPEAT) }?,
    })
}

/// No textures yet; [`Textures::white`] is set once the renderer exists.
pub(super) fn textures(descriptor_pools: DescriptorPools, samplers: Samplers) -> Textures {
    Textures {
        list: Vec::new(),
        cubemaps: Vec::new(),
        pending_inits: Vec::new(),
        pending_init_bytes: 0,
        pending_updates: Vec::new(),
        update_staging: (0..MAX_FRAMES_IN_FLIGHT)
            .map(|_| Staging::default())
            .collect(),
        white: TextureId(0),
        descriptor_pools,
        nearest_sampler: samplers.nearest,
        mipmap_sampler: samplers.mipmap,
        linear_sampler: samplers.linear,
        repeat_sampler: samplers.repeat,
        linear_repeat_sampler: samplers.linear_repeat,
        free: Vec::new(),
        reuse: Default::default(),
        tracked: None,
        retired: Vec::new(),
    }
}

/// The frames' bookkeeping, and the pool of one-off uploads.
pub(super) unsafe fn create_frames(
    device: &ash::Device,
    queue_family: u32,
) -> anyhow::Result<Frames> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let command_pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )
    }?;
    Ok(Frames {
        command_pool,
        current: 0,
        number: 0,
        submitted: [0; MAX_FRAMES_IN_FLIGHT],
        last_waits: FrameWaits::default(),
        applied_post_effects: Vec::new(),
    })
}

/// The skybox's cube, uploaded once.
pub(super) unsafe fn create_sky_cube(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
) -> anyhow::Result<SkyCube> {
    let usage = vk::BufferUsageFlags::VERTEX_BUFFER;
    let cube = &pipeline::sky::CUBE_VERTICES;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let (buffer, memory) = unsafe { mapped::upload(device, mem_props, usage, cube) }?;
    Ok(SkyCube { buffer, memory })
}

/// No swapchain yet: the first frame's acquire makes it, for `window`.
pub(super) fn swapchain(
    window: vk::Extent2D,
    format: vk::Format,
    depth_format: vk::Format,
) -> Swapchain {
    Swapchain {
        window,
        extent: window,
        vsync: true,
        requested: None,
        present_mode: numerobis::vulkan::swapchain::PresentMode::Fifo,
        images: Vec::new(),
        views: Vec::new(),
        format,
        can_copy: false,
        depth_format,
        depth_sampled: false,
        depth: DepthImage::default(),
        framebuffers: Vec::new(),
    }
}
