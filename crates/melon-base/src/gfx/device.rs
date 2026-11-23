//! The physical device, the features Melon turns on, and the logical device.

use std::ffi::CStr;

use ash::vk;

use crate::gfx::{DeviceInfo, read_device_info};

/// `VkPhysicalDevicePresentModeFifoLatestReadyFeaturesKHR`, chained by hand.
#[repr(C)]
pub struct FifoLatestReadyFeatures {
    pub s_type: vk::StructureType,
    pub p_next: *mut std::ffi::c_void,
    pub present_mode_fifo_latest_ready: vk::Bool32,
}

impl FifoLatestReadyFeatures {
    pub fn new(enabled: bool) -> Self {
        Self {
            s_type: vk::StructureType::from_raw(1000361000),
            p_next: std::ptr::null_mut(),
            present_mode_fifo_latest_ready: vk::Bool32::from(enabled),
        }
    }
}

/// The picked GPU and what Melon reads of it once.
pub struct Gpu {
    pub physical: vk::PhysicalDevice,
    /// The one family Melon draws, computes and presents with.
    pub queue_family: u32,
    pub mem_props: vk::PhysicalDeviceMemoryProperties,
    pub limits: vk::PhysicalDeviceLimits,
    pub timestamp_bits: u32,
    pub info: DeviceInfo,
}

/// The GPU [`device_pick`](crate::gfx::device_pick) chooses for `surface`.
pub unsafe fn pick(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> anyhow::Result<Gpu> {
    let (physical, queue_family) =
        crate::gfx::device_pick::pick(instance, surface_loader, surface)?;
    let timestamp_bits = instance
        .get_physical_device_queue_family_properties(physical)
        .get(queue_family as usize)
        .map_or(0, |family| family.timestamp_valid_bits);
    Ok(Gpu {
        physical,
        queue_family,
        mem_props: instance.get_physical_device_memory_properties(physical),
        limits: instance.get_physical_device_properties(physical).limits,
        timestamp_bits,
        info: read_device_info(instance, physical),
    })
}

/// What the GPU offers that Melon turns on when it can.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    /// `VK_KHR`/`VK_EXT_present_mode_fifo_latest_ready`, by the name offered.
    pub latest_ready: Option<&'static CStr>,
    /// `VK_EXT_pageable_device_local_memory` (with `memory_priority`).
    pub pageable: bool,
    /// Indirect terrain draws; `MELON_NO_MULTIDRAW` turns them off.
    pub multi_draw: bool,
    /// `VK_KHR_draw_indirect_count`, with `multi_draw`: draw counts the GPU
    /// writes, for the terrain's occlusion culling.
    pub draw_count: bool,
    /// Pipeline statistics: how much each pass shades, for the benchmark.
    pub statistics: bool,
    /// `robustBufferAccess`; `MELON_NO_ROBUST_BUFFERS` turns it off.
    pub robust: bool,
}

pub unsafe fn probe(instance: &ash::Instance, physical: vk::PhysicalDevice) -> Features {
    let offered = instance
        .enumerate_device_extension_properties(physical)
        .unwrap_or_default();
    let offers = |name: &CStr| {
        offered
            .iter()
            .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name)
    };
    let latest_ready_extension = [
        c"VK_KHR_present_mode_fifo_latest_ready",
        c"VK_EXT_present_mode_fifo_latest_ready",
    ]
    .into_iter()
    .find(|name| offers(name));
    let supported = instance.get_physical_device_features(physical);
    let mut supported_11 = vk::PhysicalDeviceVulkan11Features::default();
    let mut supported_latest = FifoLatestReadyFeatures::new(false);
    if latest_ready_extension.is_some() {
        supported_11.p_next = std::ptr::from_mut(&mut supported_latest).cast();
    }
    let mut supported_2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut supported_11);
    instance.get_physical_device_features2(physical, &mut supported_2);
    let latest_ready = latest_ready_extension
        .filter(|_| supported_latest.present_mode_fifo_latest_ready == vk::TRUE);
    // `VK_EXT_pageable_device_local_memory`: when another program fills
    // the VRAM, the driver moves allocations to system memory instead of
    // failing them (as DXVK and vkd3d ask too), so the game keeps running
    // slower rather than losing its swapchain and textures.
    let pageable = offers(ash::ext::pageable_device_local_memory::NAME)
        && offers(ash::ext::memory_priority::NAME)
        && {
            let mut query = vk::PhysicalDevicePageableDeviceLocalMemoryFeaturesEXT::default();
            let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut query);
            instance.get_physical_device_features2(physical, &mut features);
            query.pageable_device_local_memory == vk::TRUE
        };
    let multi_draw = std::env::var_os("MELON_NO_MULTIDRAW").is_none()
        && supported.multi_draw_indirect == vk::TRUE
        && supported.draw_indirect_first_instance == vk::TRUE
        && supported_11.shader_draw_parameters == vk::TRUE;
    // `robustBufferAccess`: a read past a buffer's end gives zeros instead
    // of a GPU page fault that loses the device (Xid 13 on NVIDIA). A
    // safety net for broken data.
    let robust = std::env::var_os("MELON_NO_ROBUST_BUFFERS").is_none()
        && supported.robust_buffer_access == vk::TRUE;
    let draw_count = multi_draw && offers(ash::khr::draw_indirect_count::NAME);
    Features {
        latest_ready,
        pageable,
        multi_draw,
        draw_count,
        statistics: supported.pipeline_statistics_query == vk::TRUE,
        robust,
    }
}

/// The logical device with `features` on, and its one queue.
pub unsafe fn create(
    instance: &ash::Instance,
    gpu: &Gpu,
    features: &Features,
) -> anyhow::Result<(ash::Device, vk::Queue)> {
    let queue_priorities = [1.0f32];
    let queue_infos = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(gpu.queue_family)
        .queue_priorities(&queue_priorities)];
    let mut extensions = vec![ash::khr::swapchain::NAME.as_ptr()];
    if let Some(name) = features.latest_ready {
        extensions.push(name.as_ptr());
    }
    if features.draw_count {
        extensions.push(ash::khr::draw_indirect_count::NAME.as_ptr());
    }
    if features.pageable {
        extensions.push(ash::ext::memory_priority::NAME.as_ptr());
        extensions.push(ash::ext::pageable_device_local_memory::NAME.as_ptr());
    }
    let core = vk::PhysicalDeviceFeatures::default()
        .robust_buffer_access(features.robust)
        .multi_draw_indirect(features.multi_draw)
        .draw_indirect_first_instance(features.multi_draw)
        .pipeline_statistics_query(features.statistics);
    let mut features_11 =
        vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(features.multi_draw);
    let mut latest_ready = FifoLatestReadyFeatures::new(true);
    if features.latest_ready.is_some() {
        features_11.p_next = std::ptr::from_mut(&mut latest_ready).cast();
    }
    let mut pageable = vk::PhysicalDevicePageableDeviceLocalMemoryFeaturesEXT::default()
        .pageable_device_local_memory(true);
    let mut info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_features(&core)
        .push_next(&mut features_11)
        .enabled_extension_names(&extensions);
    if features.pageable {
        info = info.push_next(&mut pageable);
    }
    let device = instance.create_device(gpu.physical, &info, None)?;
    let queue = device.get_device_queue(gpu.queue_family, 0);
    Ok((device, queue))
}
