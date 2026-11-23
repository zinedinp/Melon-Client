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
    let driver_detail = if props.api_version >= vk::API_VERSION_1_2 {
        let mut driver = vk::PhysicalDeviceDriverProperties::default();
        let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
        instance.get_physical_device_properties2(physical_device, &mut props2);
        let text = |raw: &[std::ffi::c_char]| CStr::from_ptr(raw.as_ptr()).to_string_lossy().trim().to_owned();
        let (name, info) = (text(&driver.driver_name), text(&driver.driver_info));
        // Mesa's info already starts with "Mesa"; NVIDIA's is just the version.
        [name, info].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    };
    DeviceInfo {
        driver_detail,
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

#[cfg(test)]
mod tests {
    use ash::vk;

    /// Prints what the log would say about each GPU; passes without Vulkan too.
    #[test]
    fn reads_the_driver() {
        let Ok(entry) = (unsafe { ash::Entry::load() }) else {
            return;
        };
        let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
        let create = vk::InstanceCreateInfo::default().application_info(&app);
        let Ok(instance) = (unsafe { entry.create_instance(&create, None) }) else {
            return;
        };
        for physical in unsafe { instance.enumerate_physical_devices() }.unwrap_or_default() {
            let info = unsafe { super::read_device_info(&instance, physical) };
            println!("{}{} | {} | {} | {} | {}", info.name, info.type_suffix, info.vendor, info.backend, info.driver, info.driver_detail);
        }
        unsafe { instance.destroy_instance(None) };
    }
}
