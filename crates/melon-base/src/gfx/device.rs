//! The physical device and the logical device with the
//! [`features`](super::features) Melon turns on.

use std::ffi::CStr;

use ash::vk;
use numerobis::vulkan::ApiVersion;

use crate::gfx::features::Features;
use crate::gfx::{DeviceInfo, read_device_info};

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

/// The GPU [`device_pick`](crate::gfx::device_pick) chooses for `surface`;
/// `api` is the instance's `apiVersion`.
///
/// # Safety
///
/// `instance` and `surface` must be live, `surface` made from `instance`.
pub unsafe fn pick(
    instance: &ash::Instance,
    api: ApiVersion,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> anyhow::Result<Gpu> {
    let (physical, queue_family) =
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { crate::gfx::device_pick::pick(instance, surface_loader, surface) }?;
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let timestamp_bits = unsafe { instance.get_physical_device_queue_family_properties(physical) }
        .get(queue_family as usize)
        .map_or(0, |family| family.timestamp_valid_bits);
    Ok(Gpu {
        physical,
        queue_family,
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        mem_props: unsafe { instance.get_physical_device_memory_properties(physical) },
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        limits: unsafe { instance.get_physical_device_properties(physical) }.limits,
        timestamp_bits,
        // SAFETY: `instance` is live, and the physical device is its own.
        info: unsafe { read_device_info(instance, physical, api) },
    })
}

/// The logical device with `features` on, and its one queue.
///
/// # Safety
///
/// `gpu` must come from [`pick`] on `instance`, `features` from probing it.
pub unsafe fn create(
    instance: &ash::Instance,
    gpu: &Gpu,
    features: &Features,
) -> anyhow::Result<(ash::Device, vk::Queue)> {
    let queue_priorities = [1.0f32];
    let queue_infos = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(gpu.queue_family)
        .queue_priorities(&queue_priorities)];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let device = unsafe { create_with(instance, gpu.physical, &queue_infos, features) }?;
    crate::gfx::vk_util::set_allocation_limit(gpu.limits.max_memory_allocation_count);
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let queue = unsafe { device.get_device_queue(gpu.queue_family, 0) };
    Ok((device, queue))
}

/// `vkCreateDevice` with `features`' extensions and bits: a `pNext` chain
/// from 1.1 on, the bare 1.0 struct below.
///
/// # Safety
///
/// `physical` must belong to `instance`, `features` must be probed on it.
pub unsafe fn create_with(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    queues: &[vk::DeviceQueueCreateInfo<'_>],
    features: &Features,
) -> anyhow::Result<ash::Device> {
    let extensions: Vec<_> = features
        .extensions()
        .into_iter()
        .map(CStr::as_ptr)
        .collect();
    let mut chain = features.plan.device_chain();
    let mut info = vk::DeviceCreateInfo::default()
        .queue_create_infos(queues)
        .enabled_extension_names(&extensions);
    if features.plan.api < ApiVersion::V1_1 {
        info.p_enabled_features = chain.core_features().cast();
    } else {
        info.p_next = chain.features2().cast_const();
    }
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    Ok(unsafe { instance.create_device(physical, &info, None) }?)
}
