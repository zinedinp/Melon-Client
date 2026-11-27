//! The UI's vertices: a mapped ring with a range per frame slot, and the
//! quads' shared indices.

use ash::vk;
use majestix_vk::{Buffer, BufferDesc, Frame, Frames, Mapped, MemoryUse, Queue};

use super::quad::Vertex;

/// Vertices a frame's UI holds at most.
pub const VERTEX_CAPACITY: usize = 1 << 16;

pub struct UiVertices {
    /// majestix-vk's part; given up in [`Self::destroy`], before the device goes.
    own: Option<Own>,
}

struct Own {
    device: majestix_vk::Device,
    ring: Mapped<Vertex>,
    /// Where each slot's range starts in the ring's buffer.
    offsets: Vec<u64>,
    /// Six indices per quad, shared by every frame.
    indices: Buffer,
}

/// What a frame binds: its slot's vertices, and the indices.
#[derive(Clone, Copy)]
pub struct Bound {
    pub vertices: vk::Buffer,
    pub offset: u64,
    pub indices: vk::Buffer,
}

impl UiVertices {
    /// A range of [`VERTEX_CAPACITY`] vertices per slot of `frames`, and the
    /// indices, vanilla's `QUADS` pattern (0, 1, 2, 2, 3, 0), copied in on `queue`.
    ///
    /// # Errors
    ///
    /// Out of memory, or the device is lost.
    pub fn new(
        layer: &majestix_vk::Device,
        queue: &Queue,
        frames: &Frames,
    ) -> anyhow::Result<Self> {
        let pattern: Vec<u32> = (0..(VERTEX_CAPACITY / 4) as u32)
            .flat_map(|quad| [0, 1, 2, 2, 3, 0].map(|corner| quad * 4 + corner))
            .collect();
        let indices = layer.create_buffer_init(
            queue,
            &BufferDesc {
                size: (pattern.len() * size_of::<u32>()) as u64,
                usage: vk::BufferUsageFlags::INDEX_BUFFER,
                memory: MemoryUse::Device,
            },
            &pattern,
        )?;
        let ring = match Mapped::new(
            layer,
            frames,
            VERTEX_CAPACITY,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            MemoryUse::Upload,
        ) {
            Ok(ring) => ring,
            Err(e) => {
                layer.retire(indices);
                return Err(e.into());
            }
        };
        let offsets = frames.slots().map(|slot| ring.offset(slot)).collect();
        Ok(Self {
            own: Some(Own {
                device: layer.clone(),
                ring,
                offsets,
                indices,
            }),
        })
    }

    /// Frame slot `slot`'s vertices and the indices.
    pub fn bound(&self, slot: usize) -> Bound {
        let own = self.own.as_ref().expect("UI vertices used after destroy");
        Bound {
            vertices: own.ring.buffer().raw(),
            offset: own.offsets[slot],
            indices: own.indices.raw(),
        }
    }

    /// Writes the frame's vertices, at most [`VERTEX_CAPACITY`] of them.
    pub fn write(&mut self, frame: &Frame<'_>, vertices: &[Vertex]) {
        let own = self
            .own
            .as_mut()
            .expect("UI vertices written after destroy");
        let vertices = &vertices[..vertices.len().min(VERTEX_CAPACITY)];
        own.ring.get_mut(frame)[..vertices.len()].copy_from_slice(vertices);
    }

    /// Retires the ring and the indices.
    pub fn destroy(&mut self) {
        if let Some(own) = self.own.take() {
            own.device.retire(own.indices);
        }
    }
}
