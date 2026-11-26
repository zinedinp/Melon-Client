//! A host-visible staging buffer per frame in flight, mapped for its whole life.

use ash::vk;

use super::mapped::MappedBuffer;
use super::vk_util::Place;

#[derive(Default)]
pub struct Staging {
    mapped: Option<MappedBuffer>,
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
    ///
    /// # Safety
    ///
    /// `device` must be live; no frame in flight may read the buffer.
    pub unsafe fn trim(&mut self, device: &ash::Device, size: u64, keep: u64) {
        let capacity = self.capacity();
        if capacity <= keep || size.saturating_mul(4) > capacity {
            self.slack = 0;
            return;
        }
        self.slack += 1;
        if self.slack >= TRIM_AFTER_FRAMES {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.destroy(device) };
        }
    }

    /// Room for `size` bytes, at least `min_capacity` when it has to grow.
    ///
    /// # Safety
    ///
    /// `device` must be live; no frame in flight may read the buffer.
    pub unsafe fn ensure(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        min_capacity: u64,
    ) -> anyhow::Result<()> {
        if size <= self.capacity() {
            return Ok(());
        }
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.destroy(device) };
        let capacity = size.next_power_of_two().max(min_capacity);
        let usage = vk::BufferUsageFlags::TRANSFER_SRC;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let mapped =
            unsafe { MappedBuffer::new(device, mem_props, capacity, usage, Place::STAGING) }?;
        *self = Self {
            mapped: Some(mapped),
            slack: 0,
        };
        Ok(())
    }

    /// Bytes of the buffer; 0 before [`Self::ensure`].
    fn capacity(&self) -> u64 {
        self.mapped
            .as_ref()
            .map_or(0, |mapped| mapped.size() as u64)
    }

    /// The buffer, once [`Self::ensure`] made one.
    pub fn mapped(&self) -> Option<&MappedBuffer> {
        self.mapped.as_ref()
    }

    /// Frees the buffer.
    ///
    /// # Safety
    ///
    /// `device` must be live and done with it.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        if let Some(mapped) = self.mapped.take() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { mapped.destroy(device) };
        }
        *self = Self::default();
    }
}
