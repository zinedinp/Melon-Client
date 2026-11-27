//! Which output device to open.

use rodio::cpal::Device;
use rodio::cpal::traits::{DeviceTrait, HostTrait};

/// Drivers that are no use as an output: no sound, or they fight PipeWire.
const BLOCKED: &[&str] = &[
    "null",
    "jack",
    "oss",
    "lavrate",
    "samplerate",
    "speexrate",
    "speex",
    "upmix",
    "vdownmix",
    "usbstream",
];

fn driver_of(device: &Device) -> Option<String> {
    device
        .description()
        .ok()
        .and_then(|desc| desc.driver().map(str::to_string))
}

/// The output devices worth offering.
fn usable_devices() -> Vec<Device> {
    let Ok(devices) = rodio::cpal::default_host().output_devices() else {
        return Vec::new();
    };
    devices
        .filter(|d| driver_of(d).is_none_or(|driver| !BLOCKED.contains(&driver.as_str())))
        .collect()
}

/// Prefer `pipewire`/`pulse` (auto-routed desktop stream).
pub fn pick_output_device() -> Option<Device> {
    let devices = usable_devices();
    if let Some(device) = devices
        .iter()
        .find(|d| matches!(driver_of(d).as_deref(), Some("pipewire") | Some("pulse")))
    {
        return Some(device.clone());
    }
    let default = rodio::cpal::default_host().default_output_device();
    if let Some(default) = default
        && let Some(name) = name_of(&default)
        && devices.iter().any(|d| name_of(d).as_deref() == Some(&name))
    {
        return Some(default);
    }
    devices.first().cloned()
}

/// The name a device goes by in the options.
fn name_of(device: &Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_owned())
}

/// The devices the options offer, by name.
pub fn output_device_names() -> Vec<String> {
    let mut names: Vec<String> = usable_devices().iter().filter_map(name_of).collect();
    let mut seen = std::collections::HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
    names
}

/// The device with this name, if it is still there.
pub fn find_output_device(name: &str) -> Option<Device> {
    usable_devices()
        .into_iter()
        .find(|d| name_of(d).as_deref() == Some(name))
}
