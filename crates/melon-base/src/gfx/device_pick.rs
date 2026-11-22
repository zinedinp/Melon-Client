//! Which GPU renders.

use ash::vk;
use std::ffi::CStr;

/// What the ranking looks at.
#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub device_type: vk::PhysicalDeviceType,
    pub name: String,
    /// Bytes in the largest `DEVICE_LOCAL` heap.
    pub local_memory: u64,
}

impl Candidate {
    /// Higher wins. Tuple order is the priority: kind first, memory after.
    fn rank(&self) -> (u32, u64) {
        let kind = match self.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 4,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
            vk::PhysicalDeviceType::CPU => 0,
            _ => 1,
        };
        let name = self.name.to_ascii_lowercase();
        let layered = [
            "d3d12",
            "direct3d12",
            "dozen",
            "llvmpipe",
            "swiftshader",
            "basic render",
        ]
        .iter()
        .any(|needle| name.contains(needle));
        (
            if layered { kind.min(1) } else { kind * 2 },
            self.local_memory,
        )
    }
}

/// Index of the best candidate, the first on a tie; `None` when empty.
pub(super) fn best(candidates: &[Candidate]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .max_by(|(ia, a), (ib, b)| a.rank().cmp(&b.rank()).then(ib.cmp(ia)))
        .map(|(i, _)| i)
}

/// `MELON_GPU`.
fn forced(candidates: &[Candidate], setting: &str) -> Option<usize> {
    let setting = setting.trim();
    if let Ok(index) = setting.parse::<usize>() {
        return (index < candidates.len()).then_some(index);
    }
    let needle = setting.to_ascii_lowercase();
    candidates
        .iter()
        .position(|c| c.name.to_ascii_lowercase().contains(&needle))
}

/// The device to render with and a graphics + present queue family on it.
pub(super) unsafe fn pick(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> anyhow::Result<(vk::PhysicalDevice, u32)> {
    let mut devices = Vec::new();
    let mut candidates = Vec::new();
    for pd in unsafe { instance.enumerate_physical_devices()? } {
        let props = unsafe { instance.get_physical_device_properties(pd) };
        let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let has_swapchain = unsafe { instance.enumerate_device_extension_properties(pd) }
            .unwrap_or_default()
            .iter()
            .any(|e| unsafe { CStr::from_ptr(e.extension_name.as_ptr()) } == ash::khr::swapchain::NAME);
        let family = unsafe { instance.get_physical_device_queue_family_properties(pd) }
            .iter()
            .enumerate()
            .find_map(|(idx, p)| {
                let idx = idx as u32;
                let graphics = p.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                let present =
                    unsafe { surface_loader.get_physical_device_surface_support(pd, idx, surface) }
                        .unwrap_or(false);
                (graphics && present).then_some(idx)
            });
        let usable = has_swapchain && props.api_version >= vk::API_VERSION_1_1;
        let Some(family) = family.filter(|_| usable) else {
            crate::warn!("vulkan: {name} unusable (no present, swapchain or Vulkan 1.1)");
            continue;
        };
        let memory = unsafe { instance.get_physical_device_memory_properties(pd) };
        let local_memory = memory.memory_heaps[..memory.memory_heap_count as usize]
            .iter()
            .filter(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
            .map(|h| h.size)
            .max()
            .unwrap_or(0);
        candidates.push(Candidate {
            device_type: props.device_type,
            name,
            local_memory,
        });
        devices.push((pd, family));
    }
    for (i, c) in candidates.iter().enumerate() {
        crate::info!(
            "vulkan: GPU {i}: {} ({:?}, {} MiB)",
            c.name,
            c.device_type,
            c.local_memory >> 20
        );
    }
    let choice = std::env::var("MELON_GPU")
        .ok()
        .and_then(|setting| {
            let found = forced(&candidates, &setting);
            if found.is_none() {
                crate::warn!("vulkan: MELON_GPU={setting} matches no GPU, choosing one");
            }
            found
        })
        .or_else(|| best(&candidates))
        .ok_or_else(|| anyhow::anyhow!("keine passende Vulkan-GPU gefunden"))?;
    crate::info!("vulkan: using GPU {choice}: {}", candidates[choice].name);
    Ok(devices[choice])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(device_type: vk::PhysicalDeviceType, name: &str, mib: u64) -> Candidate {
        Candidate {
            device_type,
            name: name.into(),
            local_memory: mib << 20,
        }
    }

    const DISCRETE: vk::PhysicalDeviceType = vk::PhysicalDeviceType::DISCRETE_GPU;
    const INTEGRATED: vk::PhysicalDeviceType = vk::PhysicalDeviceType::INTEGRATED_GPU;

    #[test]
    fn single_discrete_wins_whatever_the_order() {
        let igpu = gpu(INTEGRATED, "Intel UHD 770", 128);
        let dgpu = gpu(DISCRETE, "Intel Arc B580", 12288);
        assert_eq!(best(&[igpu.clone(), dgpu.clone()]), Some(1));
        assert_eq!(best(&[dgpu, igpu]), Some(0));
    }

    #[test]
    fn hybrid_laptop_renders_on_the_discrete_gpu() {
        let devices = [
            gpu(INTEGRATED, "AMD Radeon(TM) Graphics", 512),
            gpu(DISCRETE, "NVIDIA GeForce RTX 4060 Laptop GPU", 8192),
        ];
        assert_eq!(best(&devices), Some(1));
    }

    #[test]
    fn more_memory_breaks_a_tie_between_discrete_gpus() {
        let devices = [gpu(DISCRETE, "GPU A", 8192), gpu(DISCRETE, "GPU B", 16384)];
        assert_eq!(best(&devices), Some(1));
    }

    #[test]
    fn first_wins_an_exact_tie() {
        let devices = [gpu(DISCRETE, "GPU A", 8192), gpu(DISCRETE, "GPU B", 8192)];
        assert_eq!(best(&devices), Some(0));
    }

    #[test]
    fn software_and_translation_layers_come_last() {
        let devices = [
            gpu(vk::PhysicalDeviceType::CPU, "llvmpipe", 0),
            gpu(DISCRETE, "Microsoft Direct3D12 (NVIDIA)", 8192),
            gpu(INTEGRATED, "Intel UHD 770", 128),
        ];
        assert_eq!(best(&devices), Some(2));
    }

    #[test]
    fn nothing_to_pick_from() {
        assert_eq!(best(&[]), None);
    }

    #[test]
    fn forced_by_index_or_name() {
        let devices = [
            gpu(INTEGRATED, "Intel UHD 770", 128),
            gpu(DISCRETE, "Intel Arc B580", 12288),
        ];
        assert_eq!(forced(&devices, "0"), Some(0));
        assert_eq!(forced(&devices, "arc"), Some(1));
        assert_eq!(forced(&devices, "7"), None);
        assert_eq!(forced(&devices, "radeon"), None);
    }
}
