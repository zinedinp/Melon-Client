//! Buffers in host-visible, coherent memory, mapped from creation to destruction.

use std::ptr::NonNull;

use anyhow::Context;
use ash::vk;
use numerobis::elem::{Pod, as_bytes};

use super::vk_util::{self, Place};

/// A buffer mapped for its whole life; every read and write is bounds-checked.
pub struct MappedBuffer {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    ptr: NonNull<u8>,
    size: usize,
}

impl MappedBuffer {
    /// `size` bytes for `usage`, in host-visible memory from `place`.
    ///
    /// # Safety
    ///
    /// `device` must be live, `mem_props` its physical device's.
    pub unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
        place: Place,
    ) -> anyhow::Result<Self> {
        let len = usize::try_from(size)?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let (buffer, memory, flags) =
            unsafe { vk_util::create_buffer_placed(device, mem_props, size, usage, place) }?;
        let host = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let mapped = if flags.contains(host) {
            // SAFETY: `memory` is this device's, host visible, and not mapped yet.
            unsafe { device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty()) }
                .context("mapping a buffer")
                .and_then(|at| NonNull::new(at.cast::<u8>()).context("vkMapMemory gave null"))
        } else {
            Err(anyhow::anyhow!(
                "{place:?} gave memory the CPU can't map ({flags:?})"
            ))
        };
        match mapped {
            Ok(ptr) => Ok(Self {
                buffer,
                memory,
                ptr,
                size: len,
            }),
            Err(e) => {
                // SAFETY: made by this device, and nothing uses it yet.
                unsafe {
                    device.destroy_buffer(buffer, None);
                    vk_util::free_memory(device, memory);
                }
                Err(e)
            }
        }
    }

    /// Bytes mapped.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Copies `data` to byte `offset`.
    ///
    /// # Safety
    ///
    /// The GPU must not be using these bytes: their frame's fence was waited
    /// on, or no submit names the buffer yet.
    ///
    /// # Panics
    ///
    /// When `data` runs past the end of the buffer.
    pub unsafe fn write<T: Pod>(&self, offset: usize, data: &[T]) {
        let bytes = as_bytes(data);
        self.check(offset, bytes.len());
        // SAFETY: in bounds (checked above) of a mapping as old as `self`; the caller keeps the GPU off it.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.ptr.as_ptr().add(offset),
                bytes.len(),
            )
        };
    }

    /// [`Self::write`] of one value.
    ///
    /// # Safety
    ///
    /// As for [`Self::write`].
    pub unsafe fn write_one<T: Pod>(&self, offset: usize, value: &T) {
        // SAFETY: the caller's promise, passed on.
        unsafe { self.write(offset, std::slice::from_ref(value)) };
    }

    /// The `len` bytes at `offset`.
    ///
    /// # Safety
    ///
    /// The GPU's writes there are done: their frame's fence was waited on.
    ///
    /// # Panics
    ///
    /// When the range runs past the end of the buffer.
    pub unsafe fn read(&self, offset: usize, len: usize) -> Vec<u8> {
        self.check(offset, len);
        let mut out = vec![0; len];
        // SAFETY: in bounds (checked above) of a mapping as old as `self`; the GPU is done writing.
        unsafe {
            std::ptr::copy_nonoverlapping(self.ptr.as_ptr().add(offset), out.as_mut_ptr(), len)
        };
        out
    }

    fn check(&self, offset: usize, len: usize) {
        let end = offset.checked_add(len);
        assert!(
            end.is_some_and(|end| end <= self.size),
            "{len} bytes at {offset} run past a {}-byte mapped buffer",
            self.size
        );
    }

    /// Unmaps it and gives the buffer and its memory to the caller.
    ///
    /// # Safety
    ///
    /// `device` made it.
    pub unsafe fn unmap(self, device: &ash::Device) -> (vk::Buffer, vk::DeviceMemory) {
        // SAFETY: the memory is this device's and mapped.
        unsafe { device.unmap_memory(self.memory) };
        (self.buffer, self.memory)
    }

    /// Frees the buffer and its memory.
    ///
    /// # Safety
    ///
    /// `device` made it, and no frame in flight uses it any more.
    pub unsafe fn destroy(self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            let (buffer, memory) = self.unmap(device);
            device.destroy_buffer(buffer, None);
            vk_util::free_memory(device, memory);
        }
    }
}

/// A buffer holding `data`, written once through a mapping.
///
/// # Safety
///
/// `device` must be live, `mem_props` its physical device's.
pub unsafe fn upload<T: Pod>(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    usage: vk::BufferUsageFlags,
    data: &[T],
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let size = (std::mem::size_of_val(data) as u64).max(1);
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let buffer = unsafe { MappedBuffer::new(device, mem_props, size, usage, Place::UPLOAD) }?;
    // SAFETY: the buffer is new, so no submit names it yet; `device` made it.
    unsafe {
        buffer.write(0, data);
        Ok(buffer.unmap(device))
    }
}
