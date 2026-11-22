//! The long-lived pools and buffers: samplers, per-frame sync, UI vertices,
//! the sky cube and the empty texture and swapchain state.

use ash::vk;

use crate::renderer::{
    DepthImage, FrameWaits, Frames, MAX_FRAMES_IN_FLIGHT, SkyCube, Swapchain, TextureId, Textures,
    UiVertices, VERTEX_CAPACITY, create_sampler, descriptor_pools::DescriptorPools, pipeline, quad,
    staging::Staging, vk_util,
};

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
        nearest: create_sampler(device, NEAREST, false, clamp)?,
        mipmap: create_sampler(device, NEAREST, true, clamp)?,
        linear: create_sampler(device, LINEAR, false, clamp)?,
        repeat: create_sampler(device, NEAREST, false, Address::REPEAT)?,
        linear_repeat: create_sampler(device, LINEAR, false, Address::REPEAT)?,
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

/// The command buffers, semaphores and fences of each frame in flight.
pub(super) unsafe fn create_frames(
    device: &ash::Device,
    queue_family: u32,
) -> anyhow::Result<Frames> {
    let command_pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let command_buffers = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32),
    )?;
    let mut image_available = Vec::new();
    let mut in_flight = Vec::new();
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        image_available.push(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
        in_flight.push(device.create_fence(
            &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
            None,
        )?);
    }
    Ok(Frames {
        command_pool,
        command_buffers,
        image_available,
        render_finished: Vec::new(),
        in_flight,
        images_in_flight: Vec::new(),
        current: 0,
        number: 0,
        submitted: [0; MAX_FRAMES_IN_FLIGHT],
        last_waits: FrameWaits::default(),
        applied_post_effects: Vec::new(),
    })
}

/// A mapped UI vertex buffer per frame in flight, and the shared indices.
pub(super) unsafe fn create_ui_vertices(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
) -> anyhow::Result<UiVertices> {
    let mut vertices = UiVertices {
        buffers: Vec::new(),
        memories: Vec::new(),
        mapped: Vec::new(),
        indices: create_quad_indices(device, mem_props)?,
    };
    let size = (VERTEX_CAPACITY * std::mem::size_of::<quad::Vertex>()) as vk::DeviceSize;
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        let (buffer, memory) = vk_util::create_buffer_in(
            device,
            mem_props,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            &vk_util::MAPPED_FOR_GPU,
        )?;
        let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
        vertices.buffers.push(buffer);
        vertices.memories.push(memory);
        vertices.mapped.push(mapped);
    }
    Ok(vertices)
}

/// The UI's index buffer: vanilla's `QUADS` pattern (0, 1, 2, 2, 3, 0).
unsafe fn create_quad_indices(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let indices: Vec<u32> = (0..(VERTEX_CAPACITY / 4) as u32)
        .flat_map(|quad| [0, 1, 2, 2, 3, 0].map(|corner| quad * 4 + corner))
        .collect();
    let size = std::mem::size_of_val(indices.as_slice()) as vk::DeviceSize;
    let (buffer, memory) = vk_util::create_buffer_in(
        device,
        mem_props,
        size,
        vk::BufferUsageFlags::INDEX_BUFFER,
        &vk_util::MAPPED_FOR_GPU,
    )?;
    let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
    std::ptr::copy_nonoverlapping(indices.as_ptr(), mapped.cast::<u32>(), indices.len());
    device.unmap_memory(memory);
    Ok((buffer, memory))
}

/// The skybox's cube, uploaded once.
pub(super) unsafe fn create_sky_cube(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
) -> anyhow::Result<SkyCube> {
    let cube = &pipeline::sky::CUBE_VERTICES;
    let size = std::mem::size_of_val(cube) as vk::DeviceSize;
    let (buffer, memory) = vk_util::create_buffer_in(
        device,
        mem_props,
        size,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        &vk_util::MAPPED_FOR_GPU,
    )?;
    let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
    std::ptr::copy_nonoverlapping(cube.as_ptr() as *const u8, mapped, size as usize);
    device.unmap_memory(memory);
    Ok(SkyCube { buffer, memory })
}

/// No swapchain yet: [`create_swapchain`](crate::renderer::Renderer) makes it.
pub(super) fn swapchain(
    loader: ash::khr::swapchain::Device,
    latest_ready: bool,
    format: vk::Format,
    depth_format: vk::Format,
) -> Swapchain {
    Swapchain {
        loader,
        handle: vk::SwapchainKHR::null(),
        extent: vk::Extent2D::default(),
        vsync: true,
        requested: None,
        latest_ready,
        present_mode: vk::PresentModeKHR::FIFO,
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
