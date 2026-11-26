//! What the CPU reaches of the GPU's memory, read once and kept for the run:
//! numerobis's [`Memory`] from the probe (heaps, types, budget) and its PCI
//! scan (the BAR aperture, resizable BAR). Meshes are written straight into
//! VRAM when the CPU maps all of it, and new buffers stay within the VRAM
//! budget, which is read again once it is a second old.

use std::time::Duration;

use ash::vk;
use numerobis::pci::bar::{self, ResizableBar};
use numerobis::vulkan::Memory;
use numerobis::vulkan::alloc::Usage;
use numerobis::vulkan::memory::{BudgetCache, bytes};

use super::features::Features;
use super::vk_util::Place;

/// How old the budget may get before it is read again.
const BUDGET_AGE: Duration = Duration::from_secs(1);

/// Starts numerobis's PCI scan on its own thread, so it runs beside the
/// instance creation; numerobis keeps the result for the process.
pub fn warm_up() {
    let started = std::thread::Builder::new()
        .name("pci scan".into())
        .spawn(bar::warm_up);
    if let Err(e) = started {
        crate::warn!("pci: scan not started early: {e}");
    }
}

/// The device's memory as the renderer places buffers by it.
pub struct Residency {
    direct: bool,
    budget: Option<BudgetCache>,
    /// VRAM left at the last budget read, less what was allocated since.
    room: Option<u64>,
}

impl Residency {
    /// From what the probe read of `gpu`'s memory; logs the PCI side.
    /// `MELON_NO_DIRECT_UPLOAD` keeps every upload on the staging path.
    pub fn new(features: &Features, gpu: numerobis::pci::Gpu) -> Self {
        let memory = features.memory();
        log_bar(memory, gpu);
        let off = std::env::var_os("MELON_NO_DIRECT_UPLOAD").is_some();
        let room = memory.and_then(Memory::available_vram);
        Self {
            direct: !off && memory.is_some_and(Memory::full_cpu_access),
            budget: room.map(|_| BudgetCache::for_plan(&features.plan, BUDGET_AGE)),
            room,
        }
    }

    /// Without a probe (headless checks): `direct` as given, no budget.
    pub fn without_budget(direct: bool) -> Self {
        Self {
            direct,
            budget: None,
            room: None,
        }
    }

    /// Whether the CPU maps all of VRAM (resizable BAR, a large BAR,
    /// unified memory), so meshes are written there directly.
    pub fn direct(&self) -> bool {
        self.direct
    }

    /// VRAM this process may still allocate; `None` without
    /// `VK_EXT_memory_budget`.
    pub fn available_vram(&self) -> Option<u64> {
        self.room
    }

    /// Where a mesh buffer of `size` bytes goes: VRAM the CPU maps when
    /// [`Self::direct`]; host memory first once VRAM is past the budget, as
    /// the OS would otherwise move other allocations out of VRAM to make room.
    pub fn mesh_memory(&self, size: u64) -> Place {
        Place {
            usage: if self.direct {
                Usage::DeviceMapped
            } else {
                Usage::Device
            },
            host_first: self.room.is_some_and(|room| size > room),
        }
    }

    /// Counts a buffer of `size` bytes that went to memory of `flags`
    /// against the room until the next read.
    pub fn allocated(&mut self, size: u64, flags: vk::MemoryPropertyFlags) {
        if let Some(room) = &mut self.room
            && flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        {
            *room = room.saturating_sub(size);
        }
    }

    /// Reads the budget again once the last read is [`BUDGET_AGE`] old.
    ///
    /// # Safety
    ///
    /// `physical` must be the probed device of `instance`.
    pub unsafe fn refresh(&mut self, instance: &ash::Instance, physical: vk::PhysicalDevice) {
        let Some(cache) = &mut self.budget else {
            return;
        };
        if cache.age().is_some_and(|age| age < BUDGET_AGE) {
            return;
        }
        // A budget is only read through the 1.1 entry point.
        let memory = cache.refresh(|memory2| {
            // SAFETY: the cache lends its own `VkPhysicalDeviceMemoryProperties2` for this call.
            let memory2 = unsafe { &mut *memory2.cast::<vk::PhysicalDeviceMemoryProperties2>() };
            // SAFETY: `instance` is live, and the physical device and surface are its own.
            unsafe { instance.get_physical_device_memory_properties2(physical, memory2) };
        });
        self.room = memory.and_then(Memory::available_vram);
    }
}

/// Logs the card's BAR as the OS reports it, and what to turn on in the
/// firmware when Vulkan lets the CPU map less than all of the VRAM.
fn log_bar(memory: Option<&Memory>, gpu: numerobis::pci::Gpu) {
    let Some(card) = bar::find(gpu.vendor.id(), gpu.device, None) else {
        return;
    };
    crate::info!("pci: {card}");
    if memory.is_none_or(Memory::full_cpu_access) {
        return;
    }
    match card.resizable_bar() {
        ResizableBar::Inactive { max } => crate::warn!(
            "pci: the CPU maps {} of the VRAM, the card can map {}: turn on \
             \"Above 4G Decoding\" and \"Resizable BAR\" in the firmware for \
             meshes written straight into VRAM",
            card.cpu_visible().map_or_else(|| "little".into(), bytes),
            bytes(max)
        ),
        ResizableBar::Active => crate::info!(
            "pci: resizable BAR is on, but the driver lets the CPU map less \
             of the VRAM; meshes go through staging"
        ),
        ResizableBar::NotCapable
        | ResizableBar::Unknown
        | ResizableBar::Hidden
        | ResizableBar::Integrated => {}
    }
}
