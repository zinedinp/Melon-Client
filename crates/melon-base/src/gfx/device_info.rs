//! What F3 shows about the GPU.

use super::DeviceInfo;
use ash::vk;
use std::ffi::CStr;

/// `vkGetPhysicalDeviceProperties` mapped onto the fields blaze3d's `GpuDevice` reports.
pub unsafe fn read_device_info(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> DeviceInfo {
    let props = instance.get_physical_device_properties(physical_device);
    let name = CStr::from_ptr(props.device_name.as_ptr())
        .to_string_lossy()
        .into_owned();
    let type_suffix = match props.device_type {
        vk::PhysicalDeviceType::INTEGRATED_GPU => " (iGPU)",
        vk::PhysicalDeviceType::DISCRETE_GPU => " (dGPU)",
        vk::PhysicalDeviceType::VIRTUAL_GPU => " (vGPU)",
        vk::PhysicalDeviceType::CPU => " (software)",
        _ => "",
    };
    let gpu = numerobis::pci::Gpu::from_pci(props.vendor_id, props.device_id);
    let vendor = gpu
        .vendor
        .name()
        .map_or_else(|| format!("0x{:04X}", props.vendor_id), str::to_owned);
    let unpack = |version: u32| {
        format!(
            "{}.{}.{}",
            vk::api_version_major(version),
            vk::api_version_minor(version),
            vk::api_version_patch(version)
        )
    };
    DeviceInfo {
        name,
        type_suffix,
        vendor,
        backend: format!("Vulkan {}", unpack(props.api_version)),
        driver: numerobis::pci::driver_version(
            gpu.vendor,
            props.driver_version,
            numerobis::pci::DriverOs::host(),
        ),
        gpu,
    }
}
