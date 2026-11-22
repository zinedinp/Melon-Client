//! The terrain's quad draws as indirect multi-draws.

use ash::vk;

use super::mesh_arena::{self, MeshArena};
use super::vk_util::{MAPPED_FOR_GPU, create_buffer_in};
use super::{ChunkDraw, TerrainMesh, WorldDraw};

/// Bytes of one `VkDrawIndexedIndirectCommand`.
pub const COMMAND_SIZE: u32 = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32;
/// Bytes of one section offset: a `vec4`.
const OFFSET_SIZE: u32 = 16;
/// Draws a frame's buffers hold at first.
const MIN_DRAWS: u32 = 4096;

/// A host-visible, mapped buffer.
#[derive(Default)]
struct Mapped {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    ptr: Option<std::ptr::NonNull<u8>>,
}

impl Mapped {
    unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> anyhow::Result<Self> {
        let (buffer, memory) = create_buffer_in(device, mem_props, size, usage, &MAPPED_FOR_GPU)?;
        let ptr = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
        Ok(Self {
            buffer,
            memory,
            ptr: std::ptr::NonNull::new(ptr),
        })
    }

    unsafe fn destroy(&mut self, device: &ash::Device) {
        if self.buffer != vk::Buffer::null() {
            device.unmap_memory(self.memory);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
        *self = Self::default();
    }
}

/// One frame in flight's buffers and the descriptor set binding them.
#[derive(Default)]
struct FrameDraws {
    commands: Mapped,
    offsets: Mapped,
    capacity: u32,
    set: vk::DescriptorSet,
}

pub(super) struct DrawBuffers {
    pool: vk::DescriptorPool,
    frames: Vec<FrameDraws>,
    /// The frame's draws, gathered before its slot is free.
    gathered: Gathered,
}

/// Draws in a row from one mesh buffer: one `cmd_draw_indexed_indirect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Run {
    pub buffer: vk::Buffer,
    pub first: u32,
    pub count: u32,
}

/// The terrain passes that draw indirect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pass {
    Solid,
    Cutout,
}

/// A frame's multi-draws, gathered on the CPU while the GPU may still read
/// the frame slot's buffers, and copied in whole once it can't.
#[derive(Default)]
struct Gathered {
    commands: Vec<vk::DrawIndexedIndirectCommand>,
    offsets: Vec<[f32; 4]>,
    runs: [Vec<Run>; 2],
    /// One pass's draws before they are grouped by buffer: (buffer's
    /// place among `buffers`, command, offset).
    pending: Vec<(u32, vk::DrawIndexedIndirectCommand, [f32; 3])>,
    buffers: Vec<vk::Buffer>,
}

impl Gathered {
    fn clear(&mut self) {
        self.commands.clear();
        self.offsets.clear();
        for runs in &mut self.runs {
            runs.clear();
        }
    }

    /// Queues a draw of the pass being gathered.
    fn queue(
        &mut self,
        buffer: vk::Buffer,
        command: vk::DrawIndexedIndirectCommand,
        offset: [f32; 3],
    ) {
        let place = match self.buffers.iter().position(|&b| b == buffer) {
            Some(place) => place,
            None => {
                self.buffers.push(buffer);
                self.buffers.len() - 1
            }
        };
        self.pending.push((place as u32, command, offset));
    }

    /// Ends pass `pass`: its draws, grouped by buffer (in the order the
    /// buffers first came) so each buffer is one run, each group still in
    /// the order the draws came (near to far).
    fn finish(&mut self, pass: Pass) {
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by_key(|&(place, ..)| place);
        for &(place, command, offset) in &pending {
            self.push(pass, self.buffers[place as usize], command, offset);
        }
        pending.clear();
        self.pending = pending;
        self.buffers.clear();
    }

    fn push(
        &mut self,
        pass: Pass,
        buffer: vk::Buffer,
        command: vk::DrawIndexedIndirectCommand,
        offset: [f32; 3],
    ) {
        let index = self.commands.len() as u32;
        self.commands.push(command);
        self.offsets.push([offset[0], offset[1], offset[2], 0.0]);
        let runs = &mut self.runs[pass as usize];
        match runs.last_mut() {
            Some(run) if run.buffer == buffer && run.first + run.count == index => run.count += 1,
            _ => runs.push(Run {
                buffer,
                first: index,
                count: 1,
            }),
        }
    }
}

/// The draws of quad mesh slot `count` quads long that cover `range`, one per
/// [`mesh_arena::QUADS_PER_DRAW`] quads: `(first quad, quads)`.
pub(super) fn quad_draws(
    range: &std::ops::Range<u32>,
    count: u32,
) -> impl Iterator<Item = (u32, u32)> {
    let end = range.end.min(count);
    (range.start..end)
        .step_by(mesh_arena::QUADS_PER_DRAW as usize)
        .map(move |first| (first, (end - first).min(mesh_arena::QUADS_PER_DRAW)))
}

/// The `vertexOffset` that makes `gl_VertexIndex` count corners from quad `quad`.
pub(super) fn corner_offset(quad: u32) -> i32 {
    (quad * 4) as i32
}

/// The set layout of the offsets: one storage buffer for the vertex stage.
pub(super) unsafe fn create_layout(
    device: &ash::Device,
) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX)];
    Ok(device.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
        None,
    )?)
}

impl DrawBuffers {
    pub unsafe fn new(
        device: &ash::Device,
        frames: usize,
        layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<Self> {
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: frames as u32,
        }];
        let pool = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&sizes)
                .max_sets(frames as u32),
            None,
        )?;
        let layouts = vec![layout; frames];
        let sets = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )?;
        Ok(Self {
            pool,
            gathered: Gathered::default(),
            frames: sets
                .into_iter()
                .map(|set| FrameDraws {
                    set,
                    ..FrameDraws::default()
                })
                .collect(),
        })
    }

    /// Room for `draws` draws in frame slot `frame`; call once its fence signalled.
    pub unsafe fn prepare(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
        draws: u32,
    ) -> anyhow::Result<()> {
        let slot = &mut self.frames[frame];
        if draws <= slot.capacity {
            return Ok(());
        }
        let capacity = draws.next_power_of_two().max(MIN_DRAWS);
        slot.commands.destroy(device);
        slot.offsets.destroy(device);
        slot.capacity = 0;
        slot.commands = Mapped::new(
            device,
            mem_props,
            u64::from(capacity) * u64::from(COMMAND_SIZE),
            vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?;
        slot.offsets = Mapped::new(
            device,
            mem_props,
            u64::from(capacity) * u64::from(OFFSET_SIZE),
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        slot.capacity = capacity;
        let info = [vk::DescriptorBufferInfo::default()
            .buffer(slot.offsets.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)];
        device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(slot.set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&info)],
            &[],
        );
        Ok(())
    }

    /// The set binding frame `frame`'s offsets.
    pub fn set(&self, frame: usize) -> vk::DescriptorSet {
        self.frames[frame].set
    }

    /// The commands buffer of frame `frame`, for `cmd_draw_indexed_indirect`.
    pub fn commands(&self, frame: usize) -> vk::Buffer {
        self.frames[frame].commands.buffer
    }

    /// Gathers the quad meshes of `world`'s solid and cutout passes, before
    /// the frame's slot is free: it reads only the CPU's own state.
    pub fn gather(&mut self, world: &WorldDraw, meshes: &MeshArena) {
        let gathered = &mut self.gathered;
        gathered.clear();
        for (pass, chunks) in [(Pass::Solid, &world.chunks), (Pass::Cutout, &world.cutout)] {
            gather_pass(gathered, pass, world, chunks, meshes);
        }
    }

    /// Forgets what was gathered, for a frame without a world.
    pub fn clear(&mut self) {
        self.gathered.clear();
    }

    /// How many draws were gathered, the room [`Self::prepare`] needs.
    pub fn gathered(&self) -> u32 {
        self.gathered.commands.len() as u32
    }

    /// The runs pass `pass` draws.
    pub fn runs(&self, pass: Pass) -> &[Run] {
        &self.gathered.runs[pass as usize]
    }

    /// Copies what was gathered into frame `frame`'s buffers, made big enough
    /// by [`Self::prepare`]; call once its fence signalled.
    pub unsafe fn upload(&self, frame: usize) {
        let slot = &self.frames[frame];
        let gathered = &self.gathered;
        let (Some(commands), Some(offsets)) = (slot.commands.ptr, slot.offsets.ptr) else {
            return;
        };
        let count = gathered.commands.len().min(slot.capacity as usize);
        debug_assert_eq!(count, gathered.commands.len(), "prepared too small");
        std::ptr::copy_nonoverlapping(
            gathered.commands.as_ptr(),
            commands.as_ptr().cast::<vk::DrawIndexedIndirectCommand>(),
            count,
        );
        std::ptr::copy_nonoverlapping(
            gathered.offsets.as_ptr(),
            offsets.as_ptr().cast::<[f32; 4]>(),
            count,
        );
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        self.gathered = Gathered::default();
        for slot in &mut self.frames {
            slot.commands.destroy(device);
            slot.offsets.destroy(device);
        }
        device.destroy_descriptor_pool(self.pool, None);
    }
}

/// One pass's draws, in the order of `chunks`.
fn gather_pass(
    gathered: &mut Gathered,
    pass: Pass,
    world: &WorldDraw,
    chunks: &[ChunkDraw],
    meshes: &MeshArena,
) {
    for chunk in chunks {
        let TerrainMesh::Quads(id) = chunk.mesh else {
            continue;
        };
        // TODO(perf): a random read per draw from the arena's slot table
        // (~8 MB with every section loaded), a cache miss each. Could the
        // buffer and base go into `ChunkDraw` when culled, kept right across
        // arena moves by a generation number?
        let Some(slot) = meshes.slot(id.0) else {
            continue;
        };
        let (buffer, base) = meshes.location(slot);
        let offset = world.offset(chunk.origin).to_array();
        for (first, quads) in quad_draws(&chunk.range, slot.count) {
            gathered.queue(
                buffer,
                vk::DrawIndexedIndirectCommand {
                    index_count: quads * 6,
                    instance_count: 1,
                    first_index: 0,
                    vertex_offset: corner_offset(base + first),
                    first_instance: 0,
                },
                offset,
            );
        }
    }
    gathered.finish(pass);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    fn command() -> vk::DrawIndexedIndirectCommand {
        vk::DrawIndexedIndirectCommand::default()
    }

    #[test]
    fn draws_from_one_buffer_in_a_row_are_one_run() {
        let (a, b) = (vk::Buffer::from_raw(1), vk::Buffer::from_raw(2));
        let mut gathered = Gathered::default();
        for buffer in [a, a, b, a] {
            gathered.push(Pass::Solid, buffer, command(), [0.0; 3]);
        }
        gathered.push(Pass::Cutout, a, command(), [1.0, 2.0, 3.0]);
        let run = |buffer, first, count| Run {
            buffer,
            first,
            count,
        };
        assert_eq!(
            gathered.runs[Pass::Solid as usize],
            [run(a, 0, 2), run(b, 2, 1), run(a, 3, 1)]
        );
        // The cutout pass numbers on from the solid one, into the same buffers.
        assert_eq!(gathered.runs[Pass::Cutout as usize], [run(a, 4, 1)]);
        assert_eq!(gathered.offsets[4], [1.0, 2.0, 3.0, 0.0]);
    }

    #[test]
    fn a_pass_is_grouped_by_buffer_and_keeps_its_order_within() {
        let (a, b) = (vk::Buffer::from_raw(1), vk::Buffer::from_raw(2));
        let mut gathered = Gathered::default();
        for (buffer, x) in [(a, 0.0), (b, 1.0), (a, 2.0), (b, 3.0), (a, 4.0)] {
            gathered.queue(buffer, command(), [x, 0.0, 0.0]);
        }
        gathered.finish(Pass::Solid);
        let run = |buffer, first, count| Run {
            buffer,
            first,
            count,
        };
        assert_eq!(
            gathered.runs[Pass::Solid as usize],
            [run(a, 0, 3), run(b, 3, 2)]
        );
        let xs: Vec<f32> = gathered.offsets.iter().map(|o| o[0]).collect();
        assert_eq!(xs, [0.0, 2.0, 4.0, 1.0, 3.0]);
    }

    #[test]
    fn a_long_mesh_is_cut_into_draws() {
        let per = mesh_arena::QUADS_PER_DRAW;
        let draws: Vec<_> = quad_draws(&(0..per * 2 + 3), per * 2 + 3).collect();
        assert_eq!(draws, [(0, per), (per, per), (per * 2, 3)]);
        // A range past the mesh stops at its end.
        assert_eq!(quad_draws(&(0..10), 4).collect::<Vec<_>>(), [(0, 4)]);
    }
}
