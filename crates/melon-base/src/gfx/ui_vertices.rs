//! The UI's vertices: a mapped buffer per frame in flight, and the quads' shared indices.

use ash::vk;

use super::mapped::{self, MappedBuffer};
use super::quad::Vertex;
use super::vk_util::{self, Place};

/// Vertices a frame's UI holds at most.
pub const VERTEX_CAPACITY: usize = 1 << 16;

pub struct UiVertices {
    buffers: Vec<MappedBuffer>,
    /// Six indices per quad, shared by every frame.
    pub indices: vk::Buffer,
    index_memory: vk::DeviceMemory,
}

impl UiVertices {
    /// `frames` vertex buffers and the indices: vanilla's `QUADS` pattern (0, 1, 2, 2, 3, 0).
    ///
    /// # Safety
    ///
    /// `device` must be live, `mem_props` its physical device's.
    pub unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frames: usize,
    ) -> anyhow::Result<Self> {
        let indices: Vec<u32> = (0..(VERTEX_CAPACITY / 4) as u32)
            .flat_map(|quad| [0, 1, 2, 2, 3, 0].map(|corner| quad * 4 + corner))
            .collect();
        let usage = vk::BufferUsageFlags::INDEX_BUFFER;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let (indices, index_memory) =
            unsafe { mapped::upload(device, mem_props, usage, &indices) }?;
        let mut made = Self {
            buffers: Vec::with_capacity(frames),
            indices,
            index_memory,
        };
        let size = (VERTEX_CAPACITY * size_of::<Vertex>()) as u64;
        for _ in 0..frames {
            let usage = vk::BufferUsageFlags::VERTEX_BUFFER;
            // SAFETY: `device` is live, and the infos name only its own live handles.
            match unsafe { MappedBuffer::new(device, mem_props, size, usage, Place::UPLOAD) } {
                Ok(buffer) => made.buffers.push(buffer),
                Err(e) => {
                    // SAFETY: made by this device, and nothing uses it yet.
                    unsafe { made.destroy(device) };
                    return Err(e);
                }
            }
        }
        Ok(made)
    }

    /// Frame `frame`'s vertex buffer.
    pub fn buffer(&self, frame: usize) -> vk::Buffer {
        self.buffers[frame].buffer
    }

    /// Writes frame `frame`'s vertices, at most [`VERTEX_CAPACITY`] of them.
    ///
    /// # Safety
    ///
    /// Frame `frame`'s fence was waited on, so the GPU no longer reads its buffer.
    pub unsafe fn write(&self, frame: usize, vertices: &[Vertex]) {
        let vertices = &vertices[..vertices.len().min(VERTEX_CAPACITY)];
        // SAFETY: the caller's promise, passed on.
        unsafe { self.buffers[frame].write(0, vertices) };
    }

    /// Frees the buffers and the indices.
    ///
    /// # Safety
    ///
    /// `device` made them, and no frame in flight uses them any more.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            for buffer in self.buffers.drain(..) {
                buffer.destroy(device);
            }
            device.destroy_buffer(self.indices, None);
            vk_util::free_memory(device, self.index_memory);
        }
        self.indices = vk::Buffer::null();
        self.index_memory = vk::DeviceMemory::null();
    }
}
