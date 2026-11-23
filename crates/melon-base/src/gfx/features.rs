//! What Melon turns on of a device, and which path each feature takes.
//!
//! numerobis's waterfall picks per capability the newest route the device
//! allows (core version, then extension) and logs it; nothing here is
//! needed for Vulkan 1.0, every feature only enables a faster path beside
//! the 1.0 one. Paths that differ in more than an entry point live in their
//! own files (the terrain's in `renderer/record/scene/terrain.rs`).

use std::ffi::CStr;

use ash::vk;
use numerobis::vulkan::{self, ApiVersion, Op, Plan, Probe, Route};

/// Everything the game renderer turns on when it can.
pub const GAME: &[Op] = &[
    // Indirect terrain draws; `MELON_NO_MULTIDRAW` turns them off. The
    // shaders read the draw's `firstInstance` as `gl_InstanceIndex`, so no
    // `shaderDrawParameters`.
    Op::MultiDrawIndirect,
    // Draw counts the GPU writes, for the terrain's occlusion culling.
    Op::DrawIndirectCount,
    // How much each pass shades, for the benchmark.
    Op::PipelineStatistics,
    // A read past a buffer's end gives zeros instead of a GPU page fault
    // that loses the device (Xid 13 on NVIDIA); `MELON_NO_ROBUST_BUFFERS`
    // turns it off.
    Op::RobustBufferAccess,
    // When another program fills the VRAM, the driver moves allocations to
    // system memory instead of failing them (as DXVK and vkd3d ask too).
    Op::PageableDeviceLocalMemory,
    Op::FifoLatestReady,
];

/// Where `vkCmdDrawIndexedIndirectCount` comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawCount {
    /// Core from 1.2.
    Core,
    /// `VK_KHR_draw_indirect_count`.
    Khr,
}

impl DrawCount {
    /// The entry point, from `device`'s table or the extension's.
    ///
    /// # Safety
    ///
    /// `device` must have been made from `instance` with the route this came
    /// from (API 1.2 for `Core`, the extension enabled for `Khr`).
    pub unsafe fn load(
        self,
        instance: &ash::Instance,
        device: &ash::Device,
    ) -> vk::PFN_vkCmdDrawIndexedIndirectCount {
        match self {
            Self::Core => device.fp_v1_2().cmd_draw_indexed_indirect_count,
            Self::Khr => {
                ash::khr::draw_indirect_count::Device::new(instance, device)
                    .fp()
                    .cmd_draw_indexed_indirect_count_khr
            }
        }
    }
}

/// What the GPU gives of what was asked: the waterfall's plan.
#[derive(Clone, Debug)]
pub struct Features {
    pub plan: Plan,
}

impl Features {
    /// `multiDrawIndirect` and `drawIndirectFirstInstance`.
    pub fn multi_draw(&self) -> bool {
        self.plan.has(Op::MultiDrawIndirect)
    }

    /// With [`multi_draw`](Self::multi_draw) only.
    pub fn draw_count(&self) -> Option<DrawCount> {
        match self.plan.route(Op::DrawIndirectCount)? {
            _ if !self.multi_draw() => None,
            Route::Core { .. } => Some(DrawCount::Core),
            _ => Some(DrawCount::Khr),
        }
    }

    pub fn statistics(&self) -> bool {
        self.plan.has(Op::PipelineStatistics)
    }

    pub fn latest_ready(&self) -> bool {
        self.plan.has(Op::FifoLatestReady)
    }

    /// The device extensions to enable: `VK_KHR_swapchain`, then the plan's.
    pub fn extensions(&self) -> Vec<&'static CStr> {
        let swapchain = ash::khr::swapchain::NAME;
        let planned = self.plan.extensions().iter().copied();
        std::iter::once(swapchain)
            .chain(planned.filter(|&name| name != swapchain))
            .collect()
    }
}

/// Reads which of `wanted` the device has, by the newest route it allows,
/// and logs the result. `api` is the instance's `apiVersion`.
///
/// # Safety
///
/// `physical` must belong to `instance`, made with `api`.
pub unsafe fn probe(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    api: ApiVersion,
    wanted: &[Op],
) -> Features {
    let off = |name: &str| std::env::var_os(name).is_some();
    let wanted: Vec<Op> = wanted
        .iter()
        .copied()
        .filter(|op| match op {
            Op::MultiDrawIndirect | Op::DrawIndirectCount => !off("MELON_NO_MULTIDRAW"),
            Op::RobustBufferAccess => !off("MELON_NO_ROBUST_BUFFERS"),
            _ => true,
        })
        .collect();
    let offered = instance
        .enumerate_device_extension_properties(physical)
        .unwrap_or_default();
    let device_api = instance
        .get_physical_device_properties(physical)
        .api_version;
    let offer = vulkan::Offer::new(
        api,
        ApiVersion::from_raw(device_api),
        offered
            .iter()
            .map(|e| CStr::from_ptr(e.extension_name.as_ptr())),
    );
    let at_1_0 = offer.api() < ApiVersion::V1_1;
    let mut probe = Probe::new(offer, &wanted);
    if at_1_0 {
        // No `pNext` chains on 1.0: the bare 1.0 struct.
        *probe.core_features().cast::<vk::PhysicalDeviceFeatures>() =
            instance.get_physical_device_features(physical);
    } else {
        // The chain is a `VkPhysicalDeviceFeatures2` of the plain C layout.
        let features2 = &mut *probe.features2().cast::<vk::PhysicalDeviceFeatures2>();
        instance.get_physical_device_features2(physical, features2);
    }
    if probe.wants_subgroup() {
        let mut subgroup = vk::PhysicalDeviceSubgroupProperties::default();
        let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut subgroup);
        instance.get_physical_device_properties2(physical, &mut props);
        probe.set_subgroup(vulkan::subgroup::Subgroup {
            size: subgroup.subgroup_size,
            stages: subgroup.supported_stages.as_raw(),
            operations: subgroup.supported_operations.as_raw(),
        });
    }
    let plan = probe.plan();
    crate::info!("vulkan: {plan}");
    Features { plan }
}
