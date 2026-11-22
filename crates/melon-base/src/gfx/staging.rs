//! A host-visible staging buffer per frame in flight, mapped for its whole life.

use ash::vk;

use super::create_buffer;

#[derive(Default)]
pub struct Staging {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    pub mapped: Option<std::ptr::NonNull<u8>>,
    capacity: u64,
    /// Frames in a row that needed a quarter of the buffer at most.
    slack: u32,
}

/// Frames a buffer stays far too large before [`Staging::trim`] frees it:
/// long enough that a world still loading doesn't allocate it over and over.
const TRIM_AFTER_FRAMES: u32 = 240;

impl Staging {
    /// Frees a buffer over `keep` bytes that the last frames used a quarter
    /// of at most, so one burst (joining a world) doesn't hold its peak in
    /// host memory for the rest of the session. `size` is what this frame
    /// needs; call it once per frame, when no frame in flight reads the
    /// buffer, before [`Self::ensure`].
    pub unsafe fn trim(&mut self, device: &ash::Device, size: u64, keep: u64) {
        if self.capacity <= keep || size.saturating_mul(4) > self.capacity {
            self.slack = 0;
            return;
        }
        self.slack += 1;
        if self.slack >= TRIM_AFTER_FRAMES {
            self.destroy(device);
        }
    }

    /// Room for `size` bytes, at least `min_capacity` when it has to grow.
    pub unsafe fn ensure(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        min_capacity: u64,
    ) -> anyhow::Result<()> {
        if size <= self.capacity {
            return Ok(());
        }
        self.destroy(device);
        let capacity = size.next_power_of_two().max(min_capacity);
        let (buffer, memory) = create_buffer(
            device,
            mem_props,
            capacity,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let mapped =
            device.map_memory(memory, 0, capacity, vk::MemoryMapFlags::empty())? as *mut u8;
        *self = Self {
            buffer,
            memory,
            mapped: std::ptr::NonNull::new(mapped),
            capacity,
            slack: 0,
        };
        Ok(())
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        if self.buffer != vk::Buffer::null() {
            device.unmap_memory(self.memory);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
        *self = Self::default();
    }
}
