//! The physical device, the features Melon turns on, and the logical device.

use std::ffi::CStr;

use ash::vk;
use numerobis::vulkan::{self, ApiVersion, Op, Plan, Probe, Route};

use crate::gfx::{DeviceInfo, read_device_info};

/// The picked GPU and what Melon reads of it once.
pub struct Gpu {
    pub physical: vk::PhysicalDevice,
    /// The one family Melon draws, computes and presents with.
    pub queue_family: u32,
    pub mem_props: vk::PhysicalDeviceMemoryProperties,
    pub limits: vk::PhysicalDeviceLimits,
    pub timestamp_bits: u32,
    pub info: DeviceInfo,
}

/// The GPU [`device_pick`](crate::gfx::device_pick) chooses for `surface`.
pub unsafe fn pick(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> anyhow::Result<Gpu> {
    let (physical, queue_family) =
        crate::gfx::device_pick::pick(instance, surface_loader, surface)?;
    let timestamp_bits = instance
        .get_physical_device_queue_family_properties(physical)
        .get(queue_family as usize)
        .map_or(0, |family| family.timestamp_valid_bits);
    Ok(Gpu {
        physical,
        queue_family,
        mem_props: instance.get_physical_device_memory_properties(physical),
        limits: instance.get_physical_device_properties(physical).limits,
        timestamp_bits,
        info: read_device_info(instance, physical),
    })
}

/// Everything the game renderer turns on when it can.
pub const GAME: &[Op] = &[
    // Indirect terrain draws; `MELON_NO_MULTIDRAW` turns them off.
    Op::MultiDrawIndirect,
    Op::ShaderDrawParameters,
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
    pub fn multi_draw(&self) -> bool {
        self.plan.has(Op::MultiDrawIndirect) && self.plan.has(Op::ShaderDrawParameters)
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
            Op::MultiDrawIndirect | Op::ShaderDrawParameters | Op::DrawIndirectCount => {
                !off("MELON_NO_MULTIDRAW")
            }
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

/// The logical device with `features` on, and its one queue.
pub unsafe fn create(
    instance: &ash::Instance,
    gpu: &Gpu,
    features: &Features,
) -> anyhow::Result<(ash::Device, vk::Queue)> {
    let queue_priorities = [1.0f32];
    let queue_infos = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(gpu.queue_family)
        .queue_priorities(&queue_priorities)];
    let extensions: Vec<_> = features
        .extensions()
        .into_iter()
        .map(CStr::as_ptr)
        .collect();
    let mut chain = features.plan.device_chain();
    let mut info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extensions);
    if features.plan.api < ApiVersion::V1_1 {
        info.p_enabled_features = chain.core_features().cast();
    } else {
        info.p_next = chain.features2().cast_const();
    }
    let device = instance.create_device(gpu.physical, &info, None)?;
    let queue = device.get_device_queue(gpu.queue_family, 0);
    Ok((device, queue))
}
