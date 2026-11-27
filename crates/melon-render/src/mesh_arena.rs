//! Where terrain and entity meshes live on the GPU.
//!
//! A block the CPU can map (VRAM with resizable BAR, unified memory, host
//! memory) is mapped for its life and uploads are written into it directly;
//! into any other block they are copied from a staging buffer at the start
//! of the next frame. Either way a mesh only goes where no frame in flight
//! reads, so the write needs no wait.

use std::ptr::NonNull;

use ash::vk;
use melon_base::gfx::residency::Residency;
use numerobis::elem::{Pod, as_bytes};

use super::create_buffer;
use super::staging::Staging;
use super::vk_util::{self, create_buffer_placed};

/// Bytes per arena block.
const BLOCK_BYTES: u64 = 64 << 20;
/// A staging buffer up to this size is kept however little of it is used.
const STAGING_KEPT: u64 = 4 << 20;
/// Quads one indexed draw can reach with `u16` indices.
pub const QUADS_PER_DRAW: u32 = 1 << 14;
/// `0, 1, 2, 0, 2, 3` per quad: both triangles counter-clockwise.
const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

/// Whether a sorted mesh of `vertices` takes `u32` indices, not `u16`s.
pub fn wide_indices(vertices: u32) -> bool {
    vertices > 1 << 16
}

/// `MeshData.SortState.buildSortedIndexBuffer`: the index list that draws
/// `quads` (by index, in order) of a mesh of `vertices`, [`QUAD_INDICES`]
/// each. `u16`s two to a word while they reach every vertex
/// (`IndexType.least`), else `u32`s.
pub fn quad_index_list(
    quads: impl ExactSizeIterator<Item = u32>,
    vertices: u32,
    out: &mut Vec<u32>,
) {
    out.clear();
    let corners = |quad: u32| QUAD_INDICES.map(|i| quad * 4 + u32::from(i));
    if wide_indices(vertices) {
        out.reserve(quads.len() * 6);
        out.extend(quads.flat_map(corners));
    } else {
        out.reserve(quads.len() * 3);
        out.extend(quads.flat_map(|quad| {
            let [a, b, c, d, e, f] = corners(quad);
            [a | b << 16, c | d << 16, e | f << 16]
        }));
    }
}

/// The quads [`quad_index_list`] put in `list`, in its order.
#[cfg(test)]
pub fn quads_of_index_list(list: &[u32], vertices: u32) -> Vec<u32> {
    let (step, mask) = if wide_indices(vertices) {
        (6, u32::MAX)
    } else {
        (3, 0xFFFF)
    };
    list.iter()
        .step_by(step)
        .map(|&word| (word & mask) / 4)
        .collect()
}

/// First-fit free list over a block's vertex range, coalescing on free.
struct FreeList {
    /// Free `(offset, len)` regions, sorted by offset, never adjacent.
    free: Vec<(u32, u32)>,
}

impl FreeList {
    fn new(capacity: u32) -> Self {
        Self {
            free: vec![(0, capacity)],
        }
    }

    fn alloc(&mut self, n: u32) -> Option<u32> {
        let i = self.free.iter().position(|&(_, len)| len >= n)?;
        let (offset, len) = self.free[i];
        if len == n {
            self.free.remove(i);
        } else {
            self.free[i] = (offset + n, len - n);
        }
        Some(offset)
    }

    fn release(&mut self, offset: u32, n: u32) {
        let at = self.free.partition_point(|&(o, _)| o < offset);
        self.free.insert(at, (offset, n));
        if at + 1 < self.free.len() {
            let (o, l) = self.free[at];
            let (next, next_len) = self.free[at + 1];
            if o + l == next {
                self.free[at].1 += next_len;
                self.free.remove(at + 1);
            }
        }
        if at > 0 {
            let (prev, prev_len) = self.free[at - 1];
            let (o, l) = self.free[at];
            if prev + prev_len == o {
                self.free[at - 1].1 += l;
                self.free.remove(at);
            }
        }
    }
}

/// What an arena's elements are.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Vertices, read as vertex input.
    Vertices,
    /// Terrain quads, read by the shader as a storage buffer.
    Quads,
    /// Read as vertex input per instance.
    Instances,
    /// Index lists of sorted vertex meshes ([`quad_index_list`]).
    Indices,
}

struct Block {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    /// The whole block, where its memory is host visible and coherent.
    mapped: Option<NonNull<u8>>,
    capacity: u32,
    free: FreeList,
}

impl Block {
    /// No mesh in it, live or retired.
    fn is_empty(&self) -> bool {
        self.free.free == [(0, self.capacity)]
    }
}

/// Where one mesh sits. Small, since gathering the terrain's draws reads
/// one per draw from all over the slot table.
#[derive(Clone, Copy)]
pub struct Slot {
    block: u32,
    /// First element in the block.
    offset: u32,
    /// Elements: vertices or quads.
    pub count: u32,
}

/// One queued copy: `bytes` of [`MeshArena::pending_bytes`] into a buffer.
struct PendingCopy {
    dst: vk::Buffer,
    src_offset: u64,
    dst_offset: u64,
    size: u64,
}

pub struct MeshArena {
    /// Bytes per element.
    element_size: u64,
    /// Elements per block.
    block_elements: u32,
    /// By [`Slot::block`]; `None` where an empty block was destroyed, so
    /// the indices of the others stay put.
    blocks: Vec<Option<Block>>,
    /// Buffers of blocks [`Self::record`] destroyed, for the caller to drop
    /// what it keeps per buffer (descriptor sets).
    destroyed: Vec<vk::Buffer>,
    /// By `MeshId`; `None` for a free id.
    slots: Vec<Option<Slot>>,
    free_ids: Vec<usize>,
    /// Freed slots with the frame they were freed in.
    retired: Vec<(u64, Slot)>,
    pending: Vec<PendingCopy>,
    pending_bytes: Vec<u8>,
    staging: Vec<Staging>,
    /// [`QUAD_INDICES`] for [`QUADS_PER_DRAW`] quads, for vertex meshes.
    index: Option<(vk::Buffer, vk::DeviceMemory)>,
    /// How blocks are bound.
    usage: vk::BufferUsageFlags,
    /// The queued copies are in the frame being recorded.
    recorded: bool,
}

impl MeshArena {
    /// An arena for `T`s of `kind`.
    pub unsafe fn new<T: Pod>(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frames_in_flight: usize,
        kind: Kind,
    ) -> anyhow::Result<Self> {
        let element_size = std::mem::size_of::<T>() as u64;
        let mut arena = Self {
            element_size,
            block_elements: (BLOCK_BYTES / element_size) as u32,
            blocks: Vec::new(),
            destroyed: Vec::new(),
            slots: Vec::new(),
            free_ids: Vec::new(),
            retired: Vec::new(),
            pending: Vec::new(),
            pending_bytes: Vec::new(),
            staging: (0..frames_in_flight).map(|_| Staging::default()).collect(),
            index: None,
            usage: match kind {
                Kind::Vertices | Kind::Instances => vk::BufferUsageFlags::VERTEX_BUFFER,
                Kind::Quads => vk::BufferUsageFlags::STORAGE_BUFFER,
                Kind::Indices => vk::BufferUsageFlags::INDEX_BUFFER,
            },
            recorded: false,
        };
        if kind == Kind::Vertices {
            let indices: Vec<u16> = (0..QUADS_PER_DRAW)
                .flat_map(|q| QUAD_INDICES.map(|i| (q * 4) as u16 + i))
                .collect();
            let size = std::mem::size_of_val(indices.as_slice()) as u64;
            // SAFETY: `device` is live, and the infos name only its own live handles.
            let (buffer, memory) = unsafe {
                create_buffer(
                    device,
                    mem_props,
                    size,
                    vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                    super::Place::DEVICE,
                )
            }?;
            arena.index = Some((buffer, memory));
            arena.queue_copy(buffer, 0, as_bytes(&indices));
        }
        Ok(arena)
    }

    /// The shared quad index buffer; null for an arena without one.
    pub fn index_buffer(&self) -> vk::Buffer {
        self.index.map_or(vk::Buffer::null(), |(buffer, _)| buffer)
    }

    /// The mesh's slot; `None` for a free id or an empty mesh, which has
    /// nothing to draw and no block.
    pub fn slot(&self, id: usize) -> Option<Slot> {
        self.slots
            .get(id)
            .copied()
            .flatten()
            .filter(|slot| slot.count > 0)
    }

    /// Every block's buffer.
    pub fn buffers(&self) -> impl Iterator<Item = vk::Buffer> + '_ {
        self.blocks.iter().flatten().map(|block| block.buffer)
    }

    /// Buffers of the blocks destroyed since the last call.
    pub fn take_destroyed(&mut self) -> Vec<vk::Buffer> {
        std::mem::take(&mut self.destroyed)
    }

    /// The buffer a slot is in and its first element there.
    pub fn location(&self, slot: Slot) -> (vk::Buffer, u32) {
        let block = self.blocks[slot.block as usize]
            .as_ref()
            .expect("a live mesh's block is kept");
        (block.buffer, slot.offset)
    }

    /// Whether the slot's block is mapped, so uploads into it are written
    /// directly.
    #[cfg(test)]
    pub(super) fn written_directly(&self, slot: Slot) -> bool {
        self.blocks[slot.block as usize]
            .as_ref()
            .is_some_and(|block| block.mapped.is_some())
    }

    /// Bytes of a block of the usual size.
    #[cfg(test)]
    pub(super) fn block_bytes(&self) -> u64 {
        u64::from(self.block_elements) * self.element_size
    }

    /// Total and used bytes of the blocks, for F3.
    pub fn usage(&self) -> (u64, u64) {
        let total: u64 = self
            .blocks
            .iter()
            .flatten()
            .map(|b| u64::from(b.capacity))
            .sum();
        let free: u64 = self
            .blocks
            .iter()
            .flatten()
            .flat_map(|b| b.free.free.iter())
            .map(|&(_, len)| u64::from(len))
            .sum();
        (
            total * self.element_size,
            (total - free) * self.element_size,
        )
    }

    /// Queues `bytes` to be copied into `dst` at `dst_offset` at the start of the next frame.
    pub fn queue_copy(&mut self, dst: vk::Buffer, dst_offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.pending.push(PendingCopy {
            dst,
            src_offset: self.pending_bytes.len() as u64,
            dst_offset,
            size: bytes.len() as u64,
        });
        self.pending_bytes.extend_from_slice(bytes);
    }

    /// Drops the queued copies into `dst`, a buffer about to be destroyed.
    pub fn cancel_copies(&mut self, dst: vk::Buffer) {
        self.pending.retain(|copy| copy.dst != dst);
    }

    /// Places `elements` in a block and writes them there, or queues their
    /// upload; returns the mesh's id. A new block goes where `residency`
    /// says, and counts against its VRAM room.
    pub unsafe fn upload<T: Pod>(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        residency: &mut Residency,
        elements: &[T],
    ) -> anyhow::Result<usize> {
        debug_assert_eq!(std::mem::size_of::<T>() as u64, self.element_size);
        debug_assert!(
            self.index.is_none() || elements.len().is_multiple_of(4),
            "vertex meshes are lists of quads"
        );
        let count = u32::try_from(elements.len())?;
        let slot = if count == 0 {
            Slot {
                block: 0,
                offset: 0,
                count: 0,
            }
        } else {
            let found = self.blocks.iter_mut().enumerate().find_map(|(i, block)| {
                let offset = block.as_mut()?.free.alloc(count)?;
                Some((i, offset))
            });
            let (block, offset) = match found {
                Some(found) => found,
                None => {
                    let capacity = count.max(self.block_elements);
                    let size = u64::from(capacity) * self.element_size;
                    // SAFETY: `device` is live, and the infos name only its own live handles.
                    let (buffer, memory, flags) = unsafe {
                        create_buffer_placed(
                            device,
                            mem_props,
                            size,
                            // `TRANSFER_SRC` so a block can be read back.
                            self.usage
                                | vk::BufferUsageFlags::TRANSFER_DST
                                | vk::BufferUsageFlags::TRANSFER_SRC,
                            residency.mesh_memory(size),
                        )
                    }?;
                    residency.allocated(size, flags);
                    // A block that fails to map takes copies from staging.
                    let mapped = flags
                        .contains(
                            vk::MemoryPropertyFlags::HOST_VISIBLE
                                | vk::MemoryPropertyFlags::HOST_COHERENT,
                        )
                        // SAFETY: `device` is live, and the infos name only its own live handles.
                        .then(|| unsafe {
                            device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())
                        })
                        .and_then(Result::ok)
                        .and_then(|at| NonNull::new(at.cast::<u8>()));
                    let mut free = FreeList::new(capacity);
                    let offset = free.alloc(count).expect("a new block fits its mesh");
                    let block = Some(Block {
                        buffer,
                        memory,
                        mapped,
                        capacity,
                        free,
                    });
                    let index = match self.blocks.iter().position(Option::is_none) {
                        Some(index) => {
                            self.blocks[index] = block;
                            index
                        }
                        None => {
                            self.blocks.push(block);
                            self.blocks.len() - 1
                        }
                    };
                    (index, offset)
                }
            };
            let block = u32::try_from(block)?;
            let (dst, mapped) = self.blocks[block as usize]
                .as_ref()
                .map(|block| (block.buffer, block.mapped))
                .expect("a mesh's block is kept");
            let at = u64::from(offset) * self.element_size;
            let bytes = as_bytes(elements);
            match mapped {
                // Sequential, as the memory is write-combined.
                // SAFETY: the block is mapped, the mesh's range lies inside it, and no frame in flight reads it.
                Some(base) => unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        base.as_ptr().add(at as usize),
                        bytes.len(),
                    )
                },
                None => self.queue_copy(dst, at, bytes),
            }
            Slot {
                block,
                offset,
                count,
            }
        };
        let id = match self.free_ids.pop() {
            Some(id) => {
                self.slots[id] = Some(slot);
                id
            }
            None => {
                self.slots.push(Some(slot));
                self.slots.len() - 1
            }
        };
        Ok(id)
    }

    /// Frees a mesh; its space is reused once no frame in flight can draw it.
    pub fn free(&mut self, id: usize, frame_number: u64) {
        if let Some(slot) = self.slots.get_mut(id).and_then(Option::take) {
            self.free_ids.push(id);
            if slot.count > 0 {
                self.retired.push((frame_number, slot));
            }
        }
    }

    /// Room in this frame's staging buffer for the queued copies: done
    /// before the frame acquires an image, so running out of memory skips
    /// the frame instead of breaking it halfway.
    pub unsafe fn prepare(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
    ) -> anyhow::Result<()> {
        let needed = self.pending_bytes.len() as u64;
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.staging[frame].trim(device, needed, STAGING_KEPT) };
        if self.pending.is_empty() {
            return Ok(());
        }
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { self.staging[frame].ensure(device, mem_props, needed, 1 << 20) }
    }

    /// The frame with the copies [`Self::record`] recorded was submitted:
    /// they are done. Until then they stay queued, so a frame that fails
    /// after recording them doesn't leave meshes drawn that were never
    /// filled (whose garbage quads index past the quad table).
    pub fn submitted(&mut self) {
        if !std::mem::take(&mut self.recorded) {
            return;
        }
        self.pending.clear();
        self.pending_bytes.clear();
        if self.pending_bytes.capacity() > 64 << 20 {
            self.pending_bytes = Vec::new();
        }
    }

    /// Releases retired slots no frame in flight can still draw and destroys blocks left empty.
    pub unsafe fn record(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        cmd: vk::CommandBuffer,
        frame: usize,
        oldest_in_flight: u64,
    ) -> anyhow::Result<()> {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.release_retired(device, oldest_in_flight) };
        if self.pending.is_empty() {
            return Ok(());
        }
        let staging = &mut self.staging[frame];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { staging.ensure(device, mem_props, self.pending_bytes.len() as u64, 1 << 20) }?;
        let Some(mapped) = staging.mapped() else {
            return Ok(());
        };
        // SAFETY: this frame's staging buffer; its fence was waited on before recording.
        unsafe { mapped.write(0, &self.pending_bytes) };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::VERTEX_INPUT | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[],
            )
        };
        for copy in &self.pending {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_copy_buffer(
                    cmd,
                    mapped.buffer,
                    copy.dst,
                    &[vk::BufferCopy {
                        src_offset: copy.src_offset,
                        dst_offset: copy.dst_offset,
                        size: copy.size,
                    }],
                )
            };
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::VERTEX_INPUT | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(
                        vk::AccessFlags::VERTEX_ATTRIBUTE_READ
                            | vk::AccessFlags::INDEX_READ
                            | vk::AccessFlags::SHADER_READ,
                    )],
                &[],
                &[],
            )
        };
        self.recorded = true;
        Ok(())
    }

    /// Gives the space of slots freed before `oldest_in_flight` back to
    /// their blocks and destroys the blocks that leaves empty, all but one
    /// of the usual size: a spare, so meshes that come and go (entities,
    /// a column at the edge of the render distance) don't allocate and
    /// free 64 MiB over and over.
    pub(super) unsafe fn release_retired(&mut self, device: &ash::Device, oldest_in_flight: u64) {
        let blocks = &mut self.blocks;
        let mut released = false;
        self.retired.retain(|&(freed_at, slot)| {
            let done = freed_at < oldest_in_flight;
            if done {
                if let Some(block) = &mut blocks[slot.block as usize] {
                    block.free.release(slot.offset, slot.count);
                }
                released = true;
            }
            !done
        });
        if !released {
            return;
        }
        let mut spare = false;
        let first_destroyed = self.destroyed.len();
        for entry in &mut self.blocks {
            let Some(block) = entry.as_ref().filter(|block| block.is_empty()) else {
                continue;
            };
            if !spare && block.capacity == self.block_elements {
                spare = true;
                continue;
            }
            // Every mesh in it was last drawn by a frame that is done:
            // nothing in flight reads the buffer any more.
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                device.destroy_buffer(block.buffer, None);
                vk_util::free_memory(device, block.memory);
            }
            self.destroyed.push(block.buffer);
            *entry = None;
        }
        let gone = &self.destroyed[first_destroyed..];
        if !gone.is_empty() {
            // Uploads of meshes freed before a frame recorded them.
            self.pending.retain(|copy| !gone.contains(&copy.dst));
        }
        while self.blocks.last().is_some_and(Option::is_none) {
            self.blocks.pop();
        }
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        for block in self.blocks.drain(..).flatten() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                device.destroy_buffer(block.buffer, None);
                vk_util::free_memory(device, block.memory);
            }
        }
        for staging in &mut self.staging {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { staging.destroy(device) };
        }
        if let Some((buffer, memory)) = self.index.take() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                device.destroy_buffer(buffer, None);
                vk_util::free_memory(device, memory);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_list_reuses_and_coalesces() {
        let mut list = FreeList::new(100);
        let a = list.alloc(10).unwrap();
        let b = list.alloc(20).unwrap();
        let c = list.alloc(30).unwrap();
        assert_eq!((a, b, c), (0, 10, 30));
        list.release(b, 20);
        assert_eq!(list.alloc(15), Some(10));
        list.release(10, 15);
        list.release(a, 10);
        list.release(c, 30);
        assert_eq!(list.free, vec![(0, 100)]);
        assert_eq!(list.alloc(101), None);
    }

    /// The `u16`s read back as the shared quad indices would draw those
    /// quads; past 65536 vertices the list holds `u32`s.
    #[test]
    fn index_lists_draw_quads_in_the_order_given() {
        let mut list = Vec::new();
        quad_index_list([2, 0].into_iter(), 12, &mut list);
        let narrow: Vec<u16> = as_bytes(&list)
            .chunks(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(narrow, [8, 9, 10, 8, 10, 11, 0, 1, 2, 0, 2, 3]);
        assert_eq!(quads_of_index_list(&list, 12), [2, 0]);
        let last = (1 << 16) / 4;
        quad_index_list([last].into_iter(), (1 << 16) + 4, &mut list);
        let corner = last * 4;
        assert_eq!(
            list,
            [
                corner,
                corner + 1,
                corner + 2,
                corner,
                corner + 2,
                corner + 3
            ]
        );
        assert_eq!(quads_of_index_list(&list, (1 << 16) + 4), [last]);
    }
}
