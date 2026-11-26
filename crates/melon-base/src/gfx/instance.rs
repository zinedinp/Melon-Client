//! The Vulkan instance, its debug messenger and the window's surface.

use std::ffi::{CStr, c_char};

use ash::vk;
use numerobis::vulkan::ApiVersion;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::gfx::debug_callback;

/// The instance and what came up with it.
pub struct Instance {
    pub instance: ash::Instance,
    /// The validation messenger, when the layer is on.
    pub debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    /// `VK_EXT_debug_utils` is on: passes get labels for GPU profilers.
    pub labels: bool,
    /// The `apiVersion` it was made with ([`crate::gfx::api`]).
    pub api: ApiVersion,
}

/// The instance, with `VK_LAYER_KHRONOS_validation` if asked for and
/// installed.
///
/// # Safety
///
/// `entry` must be loaded; `display` must outlive the instance's surfaces.
pub unsafe fn create(
    entry: &ash::Entry,
    display: RawDisplayHandle,
    enable_validation: bool,
) -> anyhow::Result<Instance> {
    let mut extension_names = ash_window::enumerate_required_extensions(display)?.to_vec();
    let mut layer_names: Vec<*const c_char> = Vec::new();
    let validation_layer = c"VK_LAYER_KHRONOS_validation";
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let validation_available = unsafe { entry.enumerate_instance_layer_properties() }?
        .iter()
        // SAFETY: Vulkan's fixed-size name arrays are NUL-terminated.
        .any(|l| unsafe { CStr::from_ptr(l.layer_name.as_ptr()) } == validation_layer);
    let use_validation = enable_validation && validation_available;
    if enable_validation && !validation_available {
        crate::warn!("vulkan: VK_LAYER_KHRONOS_validation not found, going on without validation");
    }
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let available = unsafe { entry.enumerate_instance_extension_properties(None) }?;
    let offers = |name: &CStr| {
        available
            .iter()
            // SAFETY: Vulkan's fixed-size name arrays are NUL-terminated.
            .any(|e| unsafe { CStr::from_ptr(e.extension_name.as_ptr()) } == name)
    };
    // Debug utils also name the passes for GPU profilers (Nsight Graphics,
    // RenderDoc), so they are on whenever the driver offers them.
    let debug_utils = offers(ash::ext::debug_utils::NAME);
    // Without it a loader from 1.3.216 on hides MoltenVK. Apple only, so a
    // layered driver (Dozen) never competes with a native one elsewhere.
    let portability =
        cfg!(target_vendor = "apple") && offers(ash::khr::portability_enumeration::NAME);
    if portability {
        extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
    }
    if use_validation {
        layer_names.push(validation_layer.as_ptr());
    }
    let labels = debug_utils || use_validation;
    if labels {
        extension_names.push(ash::ext::debug_utils::NAME.as_ptr());
    }

    let api = crate::gfx::api::instance_version(entry);
    let app_name = c"Melon";
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name)
        .application_version(0)
        .engine_name(app_name)
        .engine_version(0)
        .api_version(api.raw());
    let flags = if portability {
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::empty()
    };
    let instance_info = vk::InstanceCreateInfo::default()
        .flags(flags)
        .application_info(&app_info)
        .enabled_layer_names(&layer_names)
        .enabled_extension_names(&extension_names);
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let instance = unsafe { entry.create_instance(&instance_info, None) }?;

    let debug = if use_validation {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        Some(unsafe { create_messenger(entry, &instance) }?)
    } else {
        None
    };
    Ok(Instance {
        instance,
        debug,
        labels,
        api,
    })
}

/// Warnings and errors of every kind to [`debug_callback`].
unsafe fn create_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> anyhow::Result<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)> {
    let debug_instance = ash::ext::debug_utils::Instance::new(entry, instance);
    let messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback));
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let messenger = unsafe { debug_instance.create_debug_utils_messenger(&messenger_info, None) }?;
    Ok((debug_instance, messenger))
}

/// The window's surface and its loader.
///
/// # Safety
///
/// `instance` must be made from `entry`; `display` and `window` must be live and outlive the surface.
pub unsafe fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    display: RawDisplayHandle,
    window: RawWindowHandle,
) -> anyhow::Result<(ash::khr::surface::Instance, vk::SurfaceKHR)> {
    let loader = ash::khr::surface::Instance::new(entry, instance);
    // SAFETY: `instance` is live, and the physical device and surface are its own.
    let surface = unsafe { ash_window::create_surface(entry, instance, display, window, None) }?;
    Ok((loader, surface))
}
