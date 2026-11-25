//! What F3 shows about the GPU.

use super::DeviceInfo;
use ash::vk;
use numerobis::vulkan::ApiVersion;
use std::ffi::CStr;

/// `vkGetPhysicalDeviceProperties` mapped onto the fields blaze3d's `GpuDevice` reports;
/// `api` is the instance's `apiVersion`.
///
/// # Safety
///
/// `physical_device` must belong to `instance`, made with `api`.
pub unsafe fn read_device_info(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    api: ApiVersion,
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
    let driver_detail = if driver_properties(instance, physical_device, api) {
        let mut driver = vk::PhysicalDeviceDriverProperties::default();
        let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
        instance.get_physical_device_properties2(physical_device, &mut props2);
        let text = |raw: &[std::ffi::c_char]| {
            CStr::from_ptr(raw.as_ptr())
                .to_string_lossy()
                .trim()
                .to_owned()
        };
        let (name, info) = (text(&driver.driver_name), text(&driver.driver_info));
        // Mesa's info already starts with "Mesa"; NVIDIA's is just the version.
        [name, info]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
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
        )
        .to_string(),
        gpu,
    }
}

/// Whether `VkPhysicalDeviceDriverProperties` can be read: core from 1.2,
/// `VK_KHR_driver_properties` on a 1.1 device that offers it (a physical
/// device query needs no enabled extension), not at all on 1.0.
unsafe fn driver_properties(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    api: ApiVersion,
) -> bool {
    let device = instance.get_physical_device_properties(physical_device);
    let api = api.min(super::api::device_version(device.api_version));
    api >= ApiVersion::V1_2
        || api >= ApiVersion::V1_1
            && instance
                .enumerate_device_extension_properties(physical_device)
                .unwrap_or_default()
                .iter()
                .any(|e| {
                    CStr::from_ptr(e.extension_name.as_ptr()) == ash::khr::driver_properties::NAME
                })
}

#[cfg(test)]
mod tests {
    use ash::vk;

    /// Prints what the log would say about each GPU; passes without Vulkan too.
    #[test]
    fn reads_the_driver() {
        let Ok(entry) = (unsafe { crate::gfx::loader::load() }) else {
            return;
        };
        let api = crate::gfx::api::instance_version(&entry);
        let app = vk::ApplicationInfo::default().api_version(api.raw());
        let create = vk::InstanceCreateInfo::default().application_info(&app);
        let Ok(instance) = (unsafe { entry.create_instance(&create, None) }) else {
            return;
        };
        for physical in unsafe { instance.enumerate_physical_devices() }.unwrap_or_default() {
            let info = unsafe { super::read_device_info(&instance, physical, api) };
            println!(
                "{}{} | {} | {} | {} | {}",
                info.name,
                info.type_suffix,
                info.vendor,
                info.backend,
                info.driver,
                info.driver_detail
            );
        }
        unsafe { instance.destroy_instance(None) };
    }
}
