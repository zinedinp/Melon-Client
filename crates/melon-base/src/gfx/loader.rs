//! Loading the Vulkan loader, with OBS's capture hook kept out by default.
//!
//! OBS Studio registers `VK_LAYER_OBS_HOOK` as an implicit layer, which the
//! loader puts into every Vulkan process. Its manifest names Vulkan 1.3, so
//! every 1.4 instance gets a loader warning, and the hook sits in front of
//! instance, device and swapchain calls. [`load`] turns it off through the
//! layer's own `DISABLE_VULKAN_OBS_CAPTURE` switch. OBS's Window and Display
//! Capture still see the game; only Game Capture needs the hook.

/// Set (to anything but `0`) to keep OBS's hook in, for OBS Game Capture.
pub const OBS_CAPTURE_ENV: &str = "MELON_OBS_CAPTURE";

/// `ash::Entry::load`, with OBS's hook kept out unless [`OBS_CAPTURE_ENV`] asks for it.
///
/// # Safety
///
/// As `ash::Entry::load`: the library found must be a Vulkan loader.
pub unsafe fn load() -> Result<ash::Entry, ash::LoadingError> {
    keep_obs_out();
    // SAFETY: `device` is live, and the infos name only its own live handles.
    unsafe { ash::Entry::load() }
}

/// Sets OBS's off switch, unless the user opted in or set the switch themselves.
fn keep_obs_out() {
    // Only the Windows hook is implicit; Linux's obs-vkcapture is opt-in already.
    #[cfg(windows)]
    {
        /// The off switch in OBS's layer manifest (`disable_environment`).
        const OBS_DISABLE_ENV: &str = "DISABLE_VULKAN_OBS_CAPTURE";
        let wanted = std::env::var_os(OBS_CAPTURE_ENV).is_some_and(|v| v != "0");
        if !wanted && std::env::var_os(OBS_DISABLE_ENV).is_none() {
            // SAFETY: Windows guards its environment with a lock, so setting it
            // is sound with other threads running.
            unsafe { std::env::set_var(OBS_DISABLE_ENV, "1") };
        }
    }
}
