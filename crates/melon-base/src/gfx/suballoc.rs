//! Device memory for images in shared blocks: one `vkAllocateMemory` per
//! block of many textures, not one per texture, which matters against
//! `maxMemoryAllocationCount` (4096 on Windows) with every baked item icon a
//! texture of its own. Where a range goes is numerobis's [`Block`]; what
//! takes more than half a block gets an allocation of its own.

use std::sync::{Mutex, MutexGuard, PoisonError};

use ash::prelude::VkResult;
use ash::vk;
use numerobis::vulkan::alloc::{self, Block, Tiling};

use super::vk_util::{self, Place};

/// Blocks are an eighth of the heap, at most this: small enough that a
/// block mostly freed holds little VRAM, large enough for an atlas and its mips.
const BLOCK_CAP: u64 = 64 << 20;

/// A range of device memory: of a shared block, or a whole allocation of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Allocation {
    pub memory: vk::DeviceMemory,
    pub offset: u64,
    shared: bool,
}

impl Allocation {
    /// All of `memory`, an allocation of its own that [`release`] frees whole.
    pub fn whole(memory: vk::DeviceMemory) -> Self {
        Self {
            memory,
            offset: 0,
            shared: false,
        }
    }
}

/// One shared `vkAllocateMemory` and its ranges.
struct Shared {
    memory: vk::DeviceMemory,
    ranges: Block,
}

/// A device's blocks of one memory type. Only optimal-tiling images go in,
/// so `bufferImageGranularity` never applies.
struct Pool {
    device: vk::Device,
    type_index: u32,
    blocks: Vec<Shared>,
}

static POOLS: Mutex<Vec<Pool>> = Mutex::new(Vec::new());

fn pools() -> MutexGuard<'static, Vec<Pool>> {
    POOLS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Device-local memory for the optimal-tiling `image`, bound.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's, `image` its
/// own, optimal-tiling and not bound yet.
pub unsafe fn bind_image(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
) -> anyhow::Result<Allocation> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let allocation = unsafe { allocate(device, mem_props, requirements) }?;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let bound = unsafe { device.bind_image_memory(image, allocation.memory, allocation.offset) };
    if let Err(e) = bound {
        // SAFETY: made by this device, and nothing uses it yet.
        unsafe { release(device, allocation) };
        return Err(e.into());
    }
    Ok(allocation)
}

/// A range for `requirements` in the first memory type of [`Place::DEVICE`] with room.
unsafe fn allocate(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    requirements: vk::MemoryRequirements,
) -> anyhow::Result<Allocation> {
    let mut last = anyhow::anyhow!("no suitable Vulkan memory type found");
    let mut out_of_memory = false;
    for index in vk_util::memory_types(mem_props, requirements.memory_type_bits, Place::DEVICE) {
        let kind = mem_props.memory_types[index as usize];
        let heap = mem_props.memory_heaps[kind.heap_index as usize].size;
        let block = alloc::block_size(heap, BLOCK_CAP);
        let made = if alloc::dedicated(requirements.size, block, false) {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { own(device, requirements.size, index) }
        } else {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { in_block(device, requirements, index, block) }
        };
        match made {
            Ok(allocation) => {
                if out_of_memory {
                    vk_util::note_memory_fallback(kind.property_flags);
                }
                return Ok(allocation);
            }
            Err(e) => {
                out_of_memory |= vk_util::is_out_of_memory(e);
                last = e.into();
            }
        }
    }
    Err(last)
}

/// An allocation of `size` bytes of its own.
unsafe fn own(device: &ash::Device, size: u64, type_index: u32) -> VkResult<Allocation> {
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(size)
        .memory_type_index(type_index);
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let memory = unsafe { vk_util::allocate_memory(device, &info) }?;
    Ok(Allocation {
        memory,
        offset: 0,
        shared: false,
    })
}

/// A range of one of `device`'s blocks of `type_index`, in a new block of
/// `block_size` bytes when none has room.
unsafe fn in_block(
    device: &ash::Device,
    requirements: vk::MemoryRequirements,
    type_index: u32,
    block_size: u64,
) -> VkResult<Allocation> {
    let fit = |shared: &mut Shared| {
        let offset =
            shared
                .ranges
                .alloc(requirements.size, requirements.alignment, Tiling::Optimal)?;
        Some(Allocation {
            memory: shared.memory,
            offset,
            shared: true,
        })
    };
    let mut pools = pools();
    let handle = device.handle();
    let at = match pools
        .iter()
        .position(|pool| pool.device == handle && pool.type_index == type_index)
    {
        Some(at) => at,
        None => {
            pools.push(Pool {
                device: handle,
                type_index,
                blocks: Vec::new(),
            });
            pools.len() - 1
        }
    };
    let pool = &mut pools[at];
    if let Some(found) = pool.blocks.iter_mut().find_map(fit) {
        return Ok(found);
    }
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let memory = unsafe { own(device, block_size, type_index) }?.memory;
    let mut shared = Shared {
        memory,
        ranges: Block::new(block_size, 1),
    };
    match fit(&mut shared) {
        Some(found) => {
            pool.blocks.push(shared);
            Ok(found)
        }
        None => {
            // An alignment past the block's size: nothing fits.
            // SAFETY: made by this device, and nothing uses it yet.
            unsafe { vk_util::free_memory(device, memory) };
            Err(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY)
        }
    }
}

/// Gives `allocation` back: its range to the block, the block to the driver
/// once it is empty. A default (null) allocation is skipped.
///
/// # Safety
///
/// `device` made it through this module, and no frame in flight uses it any more.
pub unsafe fn release(device: &ash::Device, allocation: Allocation) {
    if !allocation.shared {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { vk_util::free_memory(device, allocation.memory) };
        return;
    }
    let mut pools = pools();
    let handle = device.handle();
    let found = pools.iter_mut().find_map(|pool| {
        if pool.device != handle {
            return None;
        }
        let at = pool
            .blocks
            .iter()
            .position(|shared| shared.memory == allocation.memory)?;
        Some((pool, at))
    });
    let Some((pool, at)) = found else {
        crate::warn!("vulkan: a memory range freed twice, or of another device");
        return;
    };
    pool.blocks[at].ranges.free(allocation.offset);
    if pool.blocks[at].ranges.is_empty() {
        let empty = pool.blocks.swap_remove(at);
        // SAFETY: made by this device, and no range of it is in use any more.
        unsafe { vk_util::free_memory(device, empty.memory) };
    }
}

/// Frees what is left of `device`'s blocks, before the device is destroyed.
///
/// # Safety
///
/// `device` is waited idle, and nothing will use its images any more.
pub unsafe fn forget(device: &ash::Device) {
    let handle = device.handle();
    let mut left = 0;
    pools().retain_mut(|pool| {
        if pool.device != handle {
            return true;
        }
        for shared in pool.blocks.drain(..) {
            left += 1;
            // SAFETY: the device is waited idle, so nothing uses the block.
            unsafe { vk_util::free_memory(device, shared.memory) };
        }
        false
    });
    if left > 0 {
        crate::warn!("vulkan: {left} shared memory blocks still held images at shutdown");
    }
}
