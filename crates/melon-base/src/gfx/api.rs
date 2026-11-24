//! Which Vulkan version Melon runs at, decided once at instance creation.
//!
//! Vulkan 1.0 is the floor: every path has a 1.0 way, and a newer version or
//! an extension only adds a faster one beside it (see [`super::features`]
//! for the choice, `renderer/record/scene/terrain.rs` for an example ladder). The
//! instance asks for the newest version the loader and numerobis's tables
//! both know; each device then works at the lower of that and its own.
//!
//! `MELON_VULKAN` (`1.0` ... `1.4`) caps it, so a new GPU runs the paths of
//! an older one, as `MELON_SIMD` does for the CPU's vector unit.

use numerobis::vulkan::{ApiVersion, app_version};

/// The `VkApplicationInfo::apiVersion` to create the instance with.
pub fn instance_version(entry: &ash::Entry) -> ApiVersion {
    let loader = unsafe { entry.try_enumerate_instance_version() }
        .ok()
        .flatten()
        .and_then(ApiVersion::from_raw);
    let wanted = cap().map_or(ApiVersion::NEWEST, |cap| cap.min(ApiVersion::NEWEST));
    app_version(loader, wanted)
}

/// A device's `apiVersion`; another variant (Vulkan SC) counts as the 1.0 floor.
pub fn device_version(raw: u32) -> ApiVersion {
    ApiVersion::from_raw(raw).unwrap_or(ApiVersion::V1_0)
}

/// The version `MELON_VULKAN` names, if it names one.
fn cap() -> Option<ApiVersion> {
    let text = std::env::var("MELON_VULKAN").ok()?;
    let cap = parse(&text);
    if cap.is_none() {
        crate::warn!("vulkan: MELON_VULKAN={text} is no version like 1.0, ignored");
    }
    cap
}

/// `"1.2"` as a version.
fn parse(text: &str) -> Option<ApiVersion> {
    let (major, minor) = text.trim().split_once('.')?;
    Some(ApiVersion::new(major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_as_major_dot_minor() {
        assert_eq!(parse("1.0"), Some(ApiVersion::V1_0));
        assert_eq!(parse(" 1.3 "), Some(ApiVersion::V1_3));
        assert_eq!(parse("1"), None);
        assert_eq!(parse("one.two"), None);
    }
}
