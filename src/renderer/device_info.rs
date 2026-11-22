//! What F3 shows about the GPU.

use super::DeviceInfo;
use ash::vk;
use std::ffi::CStr;

/// `vkGetPhysicalDeviceProperties` mapped onto the fields blaze3d's `GpuDevice` reports.
pub(super) unsafe fn read_device_info(
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
    let vendor = match props.vendor_id {
        0x1002 => "AMD".to_string(),
        0x10DE => "NVIDIA".to_string(),
        0x8086 => "Intel".to_string(),
        0x13B5 => "ARM".to_string(),
        0x5143 => "Qualcomm".to_string(),
        0x1010 => "ImgTec".to_string(),
        id => format!("0x{id:04X}"),
    };
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
        driver: unpack(props.driver_version),
    }
}
