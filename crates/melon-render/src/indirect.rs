//! The terrain's quad draws as indirect multi-draws, and the translucent
//! sections' sorted index lists the same way.

use ash::vk;
use melon_base::gfx::mapped::MappedBuffer;
use numerobis::elem::Pod;

use super::mesh_arena::{self, MeshArena};
use super::vk_util::Place;
use super::{ChunkDraw, SectionDraw, TerrainMesh, WorldDraw};
use melon_world::mesh::wide_indices;

/// `VkDrawIndexedIndirectCommand` as a `Pod` (ash's own type can't be one here).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawCommand {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}

const _: () = assert!(std::mem::size_of::<DrawCommand>() == 20);
const _: () = assert!(
    std::mem::size_of::<DrawCommand>() == std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for DrawCommand {}

/// Bytes of one `VkDrawIndexedIndirectCommand`.
pub const COMMAND_SIZE: u32 = std::mem::size_of::<DrawCommand>() as u32;
/// Bytes of one section offset: a `vec4`.
const OFFSET_SIZE: u32 = 16;
/// Bytes of one draw's [`Gathered::bounds`]: a `vec4`.
const BOUNDS_SIZE: u32 = 16;
/// A [`Gathered::bounds`] slot for a draw that is never culled.
pub(super) const NO_SLOT: u32 = u32::MAX;
/// Draws a frame's buffers hold at first.
const MIN_DRAWS: u32 = 4096;

/// A host-visible, mapped buffer, once [`Indirect::prepare`] made one.
#[derive(Default)]
struct Mapped(Option<MappedBuffer>);

impl Mapped {
    unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> anyhow::Result<Self> {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { MappedBuffer::new(device, mem_props, size, usage, Place::UPLOAD) }
            .map(|buffer| Self(Some(buffer)))
    }

    /// The buffer; null before the first [`Indirect::prepare`].
    fn buffer(&self) -> vk::Buffer {
        self.0
            .as_ref()
            .map_or(vk::Buffer::null(), |mapped| mapped.buffer)
    }

    unsafe fn destroy(&mut self, device: &ash::Device) {
        if let Some(buffer) = self.0.take() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { buffer.destroy(device) };
        }
    }
}

/// One frame in flight's buffers and the descriptor set binding them.
#[derive(Default)]
struct FrameDraws {
    /// The gathered draws, then room for the occlusion culling to pack
    /// phase 1's and phase 2's (see [`Phase`]).
    commands: Mapped,
    offsets: Mapped,
    bounds: Mapped,
    /// For the occlusion culling, per draw and phase: whether it is drawn.
    keep: Mapped,
    /// Each run's first draw and length, solid runs first.
    run_table: Mapped,
    /// Per run, phase 1's and phase 2's draw counts.
    counts: Mapped,
    /// The layers `expand.comp` expands, and where each one's draws go.
    entries: Mapped,
    capacity: u32,
    set: vk::DescriptorSet,
    /// The translucent sections' commands and offsets, and the set
    /// binding the offsets.
    sorted: Mapped,
    sorted_offsets: Mapped,
    sorted_capacity: u32,
    sorted_set: vk::DescriptorSet,
}

/// The buffers [`DrawBuffers::culling_buffers`] hands out.
#[derive(Clone, Copy)]
pub(super) struct CullingBuffers {
    pub commands: vk::Buffer,
    pub bounds: vk::Buffer,
    pub keep: vk::Buffer,
    pub runs: vk::Buffer,
    pub counts: vk::Buffer,
}

impl FrameDraws {
    unsafe fn destroy(&mut self, device: &ash::Device) {
        for mapped in [
            &mut self.commands,
            &mut self.offsets,
            &mut self.bounds,
            &mut self.keep,
            &mut self.run_table,
            &mut self.counts,
            &mut self.entries,
        ] {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { mapped.destroy(device) };
        }
        self.capacity = 0;
    }

    /// The translucent draws' buffers, which grow on their own.
    unsafe fn destroy_sorted(&mut self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            self.sorted.destroy(device);
            self.sorted_offsets.destroy(device);
        }
        self.sorted_capacity = 0;
    }
}

/// Which of the occlusion culling's packed copies of the commands a
/// multi-draw reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    First,
    Second,
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

/// Translucent draws in a row from one vertex buffer and one index buffer:
/// one `cmd_draw_indexed_indirect`, in their far to near order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SortedRun {
    pub vertices: vk::Buffer,
    pub indices: vk::Buffer,
    pub kind: vk::IndexType,
    pub first: u32,
    pub count: u32,
}

/// The arenas the gathered draws are found in: quads, vertices, index lists.
pub(super) type Arenas<'a> = (&'a MeshArena, &'a MeshArena, &'a MeshArena);

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
    commands: Vec<DrawCommand>,
    offsets: Vec<[f32; 4]>,
    /// Per draw: its section's lowest corner relative to the camera's block,
    /// and its occlusion slot as `f32` bits ([`NO_SLOT`] for none).
    bounds: Vec<[f32; 4]>,
    runs: [Vec<Run>; 2],
    /// One pass's draws before they are grouped by buffer: (buffer's
    /// place among `buffers`, command, offset, bounds).
    pending: Vec<(u32, DrawCommand, [f32; 3], [f32; 4])>,
    buffers: Vec<vk::Buffer>,
    /// [`Self::finish`]'s draws per buffer, then where each buffer's go.
    places: Vec<u32>,
    /// The world's [`super::WorldDraw::section_slots`].
    slots: u32,
    /// The [`SectionDraw`]s as `expand.comp` reads them: the word, and
    /// where its draws start. They follow the gathered `commands`.
    entries: Vec<[u32; 2]>,
    /// The draws, gathered and expanded.
    total: u32,
    /// The camera's block.
    eye: glam::IVec3,
    /// The translucent draws as multi-draws; `sorted_ready` once they hold
    /// the whole translucent list.
    sorted: Vec<DrawCommand>,
    sorted_offsets: Vec<[f32; 4]>,
    sorted_runs: Vec<SortedRun>,
    sorted_ready: bool,
}

impl Gathered {
    fn clear(&mut self) {
        self.commands.clear();
        self.offsets.clear();
        self.bounds.clear();
        self.slots = 0;
        self.entries.clear();
        self.total = 0;
        self.sorted.clear();
        self.sorted_offsets.clear();
        self.sorted_runs.clear();
        self.sorted_ready = false;
        for runs in &mut self.runs {
            runs.clear();
        }
    }

    /// Queues a draw of the pass being gathered.
    fn queue(
        &mut self,
        buffer: vk::Buffer,
        command: DrawCommand,
        offset: [f32; 3],
        bounds: [f32; 4],
    ) {
        let place = match self.buffers.iter().position(|&b| b == buffer) {
            Some(place) => place,
            None => {
                self.buffers.push(buffer);
                self.buffers.len() - 1
            }
        };
        self.pending.push((place as u32, command, offset, bounds));
    }

    /// Ends pass `pass`: its draws, grouped by buffer (in the order the
    /// buffers first came) so each buffer is one run, each group still in
    /// the order the draws came (near to far). A counting sort: each draw is
    /// written once, straight to its place.
    fn finish(&mut self, pass: Pass) {
        let places = &mut self.places;
        places.clear();
        places.resize(self.buffers.len(), 0);
        for &(place, ..) in &self.pending {
            places[place as usize] += 1;
        }
        let base = self.commands.len() as u32;
        let mut first = base;
        for (place, buffer) in places.iter_mut().zip(&self.buffers) {
            let count = std::mem::replace(place, first);
            if count > 0 {
                self.runs[pass as usize].push(Run {
                    buffer: *buffer,
                    first,
                    count,
                });
            }
            first += count;
        }
        let total = self.pending.len();
        let new_len = self.commands.len() + total;
        self.commands.resize(new_len, DrawCommand::default());
        self.offsets.resize(new_len, [0.0; 4]);
        self.bounds.resize(new_len, [0.0; 4]);
        for &(place, command, offset, bounds) in &self.pending {
            let at = &mut places[place as usize];
            let index = *at;
            *at += 1;
            // `terrain.vert` finds the draw's offset by it.
            self.commands[index as usize] = DrawCommand {
                first_instance: index,
                ..command
            };
            self.offsets[index as usize] = [offset[0], offset[1], offset[2], 0.0];
            self.bounds[index as usize] = bounds;
        }
        self.pending.clear();
        self.buffers.clear();
    }

    /// One draw at the end of pass `pass`, joining its last run if it can.
    #[cfg(test)]
    fn push(
        &mut self,
        pass: Pass,
        buffer: vk::Buffer,
        command: DrawCommand,
        offset: [f32; 3],
        bounds: [f32; 4],
    ) {
        let index = self.commands.len() as u32;
        // `terrain.vert` finds the draw's offset by it.
        self.commands.push(DrawCommand {
            first_instance: index,
            ..command
        });
        self.offsets.push([offset[0], offset[1], offset[2], 0.0]);
        self.bounds.push(bounds);
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
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }?)
}

impl DrawBuffers {
    pub unsafe fn new(
        device: &ash::Device,
        frames: usize,
        layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<Self> {
        // Per frame: the quads' offsets, the translucent draws'.
        let sets = 2 * frames as u32;
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: sets,
        }];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&sizes)
                    .max_sets(sets),
                None,
            )
        }?;
        let layouts = vec![layout; 2 * frames];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
        }?;
        let (offsets, sorted) = sets.split_at(frames);
        Ok(Self {
            pool,
            gathered: Gathered::default(),
            frames: offsets
                .iter()
                .zip(sorted)
                .map(|(&set, &sorted_set)| FrameDraws {
                    set,
                    sorted_set,
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
        // SAFETY: the caller's promise, passed on.
        unsafe { self.prepare_sorted(device, mem_props, frame) }?;
        let slot = &mut self.frames[frame];
        if draws <= slot.capacity {
            return Ok(());
        }
        let capacity = draws.next_power_of_two().max(MIN_DRAWS);
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { slot.destroy(device) };
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let indirect = vk::BufferUsageFlags::INDIRECT_BUFFER | storage;
        let capacity_64 = u64::from(capacity);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.commands = unsafe {
            Mapped::new(
                device,
                mem_props,
                3 * capacity_64 * u64::from(COMMAND_SIZE),
                indirect,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.keep = unsafe { Mapped::new(device, mem_props, 2 * capacity_64 * 4, storage) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.run_table = unsafe { Mapped::new(device, mem_props, capacity_64 * 8, storage) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.counts = unsafe { Mapped::new(device, mem_props, 2 * capacity_64 * 4, indirect) }?;
        // Each expanded layer has a draw at least.
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.entries = unsafe { Mapped::new(device, mem_props, capacity_64 * 8, storage) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.offsets = unsafe {
            Mapped::new(
                device,
                mem_props,
                u64::from(capacity) * u64::from(OFFSET_SIZE),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.bounds = unsafe {
            Mapped::new(
                device,
                mem_props,
                u64::from(capacity) * u64::from(BOUNDS_SIZE),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
        }?;
        slot.capacity = capacity;
        let info = [vk::DescriptorBufferInfo::default()
            .buffer(slot.offsets.buffer())
            .offset(0)
            .range(vk::WHOLE_SIZE)];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(slot.set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&info)],
                &[],
            )
        };
        Ok(())
    }

    /// Room for the gathered translucent draws in frame slot `frame`.
    unsafe fn prepare_sorted(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
    ) -> anyhow::Result<()> {
        let draws = self.gathered.sorted.len() as u32;
        let slot = &mut self.frames[frame];
        if draws <= slot.sorted_capacity {
            return Ok(());
        }
        let capacity = draws.next_power_of_two().max(MIN_DRAWS);
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { slot.destroy_sorted(device) };
        let capacity_64 = u64::from(capacity);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.sorted = unsafe {
            Mapped::new(
                device,
                mem_props,
                capacity_64 * u64::from(COMMAND_SIZE),
                vk::BufferUsageFlags::INDIRECT_BUFFER,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        slot.sorted_offsets = unsafe {
            Mapped::new(
                device,
                mem_props,
                capacity_64 * u64::from(OFFSET_SIZE),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
        }?;
        slot.sorted_capacity = capacity;
        let info = [vk::DescriptorBufferInfo::default()
            .buffer(slot.sorted_offsets.buffer())
            .offset(0)
            .range(vk::WHOLE_SIZE)];
        // SAFETY: frame `frame`'s fence signalled, so no command buffer in flight binds its set.
        unsafe {
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(slot.sorted_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&info)],
                &[],
            )
        };
        Ok(())
    }

    /// Whether the translucent list was gathered whole into
    /// [`Self::sorted_runs`], so they draw it in its order.
    pub fn sorted_ready(&self) -> bool {
        self.gathered.sorted_ready
    }

    /// The translucent multi-draws, far to near.
    pub fn sorted_runs(&self) -> &[SortedRun] {
        &self.gathered.sorted_runs
    }

    /// Frame `frame`'s translucent commands, and the set binding their offsets.
    pub fn sorted(&self, frame: usize) -> (vk::Buffer, vk::DescriptorSet) {
        let slot = &self.frames[frame];
        (slot.sorted.buffer(), slot.sorted_set)
    }

    /// The set binding frame `frame`'s offsets.
    pub fn set(&self, frame: usize) -> vk::DescriptorSet {
        self.frames[frame].set
    }

    /// The commands buffer of frame `frame`, for `cmd_draw_indexed_indirect`.
    pub fn commands(&self, frame: usize) -> vk::Buffer {
        self.frames[frame].commands.buffer()
    }

    /// Frame `frame`'s buffers the occlusion culling reads and writes.
    pub fn culling_buffers(&self, frame: usize) -> CullingBuffers {
        let slot = &self.frames[frame];
        CullingBuffers {
            commands: slot.commands.buffer(),
            bounds: slot.bounds.buffer(),
            keep: slot.keep.buffer(),
            runs: slot.run_table.buffer(),
            counts: slot.counts.buffer(),
        }
    }

    /// Where `phase`'s packed commands start, in commands.
    pub fn phase_start(&self, phase: Phase) -> u32 {
        self.gathered() * (phase as u32 + 1)
    }

    /// The runs both passes draw.
    pub fn run_count(&self) -> u32 {
        self.gathered.runs.iter().map(Vec::len).sum::<usize>() as u32
    }

    /// Where pass `pass`'s runs start among [`Self::run_count`].
    pub fn run_start(&self, pass: Pass) -> u32 {
        self.gathered.runs[..pass as usize]
            .iter()
            .map(Vec::len)
            .sum::<usize>() as u32
    }

    /// The frame's draw counts, for `cmd_draw_indexed_indirect_count`.
    pub fn counts(&self, frame: usize) -> vk::Buffer {
        self.frames[frame].counts.buffer()
    }

    /// One past the highest occlusion slot the gathered draws use.
    pub fn slots(&self) -> u32 {
        self.gathered.slots
    }

    /// Gathers the quad meshes of `world`'s solid and cutout passes and its
    /// translucent sections, before the frame's slot is free: it reads only
    /// the CPU's own state.
    pub fn gather(&mut self, world: &WorldDraw, (meshes, vertices, indices): Arenas<'_>) {
        let gathered = &mut self.gathered;
        gathered.clear();
        gathered.slots = world.section_slots;
        gathered.eye = world.eye.floor().as_ivec3();
        for (pass, chunks) in [(Pass::Solid, &world.chunks), (Pass::Cutout, &world.cutout)] {
            gather_pass(gathered, pass, world, chunks, meshes);
        }
        gathered.total = gathered.commands.len() as u32;
        gather_sections(gathered, &world.sections, |block| meshes.block_buffer(block));
        gather_sorted(gathered, world, vertices, indices);
    }

    /// Forgets what was gathered, for a frame without a world.
    pub fn clear(&mut self) {
        self.gathered.clear();
    }

    /// How many draws were gathered and are expanded, the room [`Self::prepare`] needs.
    pub fn gathered(&self) -> u32 {
        self.gathered.total
    }

    /// The layers `expand.comp` expands this frame, and the camera's block.
    pub fn expanding(&self) -> (u32, glam::IVec3) {
        (self.gathered.entries.len() as u32, self.gathered.eye)
    }

    /// Frame `frame`'s buffers `expand.comp` reads and writes.
    pub fn expand_buffers(&self, frame: usize) -> super::expand::ExpandBuffers {
        let slot = &self.frames[frame];
        super::expand::ExpandBuffers {
            entries: slot.entries.buffer(),
            commands: slot.commands.buffer(),
            offsets: slot.offsets.buffer(),
            bounds: slot.bounds.buffer(),
        }
    }

    /// The runs pass `pass` draws.
    pub fn runs(&self, pass: Pass) -> &[Run] {
        &self.gathered.runs[pass as usize]
    }

    /// Copies what was gathered into frame `frame`'s buffers, made big enough
    /// by [`Self::prepare`]; call once its fence signalled.
    /// With `culling`, also what the occlusion culling reads of them.
    pub unsafe fn upload(&self, frame: usize, culling: bool) {
        let slot = &self.frames[frame];
        let gathered = &self.gathered;
        let (Some(commands), Some(offsets), Some(bounds), Some(run_table), Some(entries)) = (
            &slot.commands.0,
            &slot.offsets.0,
            &slot.bounds.0,
            &slot.run_table.0,
            &slot.entries.0,
        ) else {
            return;
        };
        let capacity = slot.capacity as usize;
        debug_assert!(gathered.total as usize <= capacity, "prepared too small");
        let count = gathered.commands.len().min(capacity);
        let expanded = gathered.entries.len().min(capacity);
        // SAFETY: this frame's fence was waited on (this fn's contract), so the GPU is off its buffers.
        unsafe {
            commands.write(0, &gathered.commands[..count]);
            offsets.write(0, &gathered.offsets[..count]);
            entries.write(0, &gathered.entries[..expanded]);
        }
        if let (Some(sorted), Some(sorted_offsets)) = (&slot.sorted.0, &slot.sorted_offsets.0) {
            let count = gathered.sorted.len().min(slot.sorted_capacity as usize);
            // SAFETY: as above.
            unsafe {
                sorted.write(0, &gathered.sorted[..count]);
                sorted_offsets.write(0, &gathered.sorted_offsets[..count]);
            }
        }
        if !culling {
            return;
        }
        for (i, run) in gathered.runs.iter().flatten().enumerate().take(capacity) {
            // SAFETY: as above.
            unsafe { run_table.write_one(i * 8, &[run.first, run.count]) };
        }
        // SAFETY: as above.
        unsafe { bounds.write(0, &gathered.bounds[..count]) };
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        self.gathered = Gathered::default();
        for slot in &mut self.frames {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                slot.destroy(device);
                slot.destroy_sorted(device);
            }
        }
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { device.destroy_descriptor_pool(self.pool, None) };
    }
}

/// Each pass's expanded layers after the gathered draws, grouped by quad
/// block (in block order) so each block is one run, near to far within;
/// each entry learns where its draws start.
fn gather_sections(
    gathered: &mut Gathered,
    sections: &[SectionDraw],
    buffer_of: impl Fn(u32) -> Option<vk::Buffer>,
) {
    for (pass, cutout) in [(Pass::Solid, false), (Pass::Cutout, true)] {
        let layer = || sections.iter().filter(move |s| s.cutout() == cutout);
        let places = &mut gathered.places;
        places.clear();
        for section in layer() {
            let block = usize::from(section.block);
            if places.len() <= block {
                places.resize(block + 1, 0);
            }
            places[block] += u32::from(section.draws);
        }
        let mut first = gathered.total;
        for (block, place) in places.iter_mut().enumerate() {
            let count = std::mem::replace(place, first);
            if count == 0 {
                continue;
            }
            let buffer = buffer_of(block as u32);
            debug_assert!(buffer.is_some(), "a drawn layer's block is kept");
            if let Some(buffer) = buffer {
                gathered.runs[pass as usize].push(Run {
                    buffer,
                    first,
                    count,
                });
            }
            first += count;
        }
        for section in layer() {
            let at = &mut places[usize::from(section.block)];
            gathered.entries.push([section.word, *at]);
            *at += u32::from(section.draws);
        }
        gathered.total = first;
    }
}

/// `world`'s translucent sections as multi-draws in their order (far to
/// near), each through its own sorted index list; a run per stretch that
/// shares its buffers. Only a list of sorted meshes alone is gathered.
fn gather_sorted(
    gathered: &mut Gathered,
    world: &WorldDraw,
    vertices: &MeshArena,
    indices: &MeshArena,
) {
    let all_sorted = world
        .translucent
        .iter()
        .all(|chunk| matches!(chunk.mesh, TerrainMesh::Sorted { .. }));
    if !all_sorted {
        return;
    }
    for chunk in &world.translucent {
        let TerrainMesh::Sorted {
            vertices: mesh,
            indices: list,
        } = chunk.mesh
        else {
            continue;
        };
        let (Some(mesh), Some(list)) = (vertices.slot(mesh.0), indices.slot(list.0)) else {
            continue;
        };
        let (vertex_buffer, base) = vertices.location(mesh);
        let (index_buffer, words) = indices.location(list);
        let (kind, first_index) = if wide_indices(mesh.count) {
            (vk::IndexType::UINT32, words)
        } else {
            (vk::IndexType::UINT16, words * 2)
        };
        let index = gathered.sorted.len() as u32;
        gathered.sorted.push(DrawCommand {
            index_count: mesh.count / 4 * 6,
            instance_count: 1,
            first_index,
            vertex_offset: base as i32,
            // `world.vert` finds the draw's offset by it.
            first_instance: index,
        });
        gathered
            .sorted_offsets
            .push(world.offset(chunk.origin).extend(0.0).to_array());
        match gathered.sorted_runs.last_mut() {
            Some(run)
                if (run.vertices, run.indices, run.kind) == (vertex_buffer, index_buffer, kind) =>
            {
                run.count += 1;
            }
            _ => gathered.sorted_runs.push(SortedRun {
                vertices: vertex_buffer,
                indices: index_buffer,
                kind,
                first: index,
                count: 1,
            }),
        }
    }
    gathered.sorted_ready = true;
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
        // arena moves by a generation number? (Prefetching a few draws ahead
        // measured no gain.)
        let Some(slot) = meshes.slot(id.0) else {
            continue;
        };
        let (buffer, base) = meshes.location(slot);
        let offset = world.offset(chunk.origin).to_array();
        let (bottom, cull_slot) = chunk.cull.map_or((0, NO_SLOT), |c| (c.bottom, c.slot));
        let bounds = [
            offset[0],
            offset[1] + bottom as f32,
            offset[2],
            f32::from_bits(cull_slot),
        ];
        for (first, quads) in quad_draws(&chunk.range.into(), slot.count) {
            gathered.queue(
                buffer,
                DrawCommand {
                    index_count: quads * 6,
                    instance_count: 1,
                    first_index: 0,
                    vertex_offset: corner_offset(base + first),
                    first_instance: 0,
                },
                offset,
                bounds,
            );
        }
    }
    gathered.finish(pass);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    fn command() -> DrawCommand {
        DrawCommand::default()
    }

    #[test]
    fn draws_from_one_buffer_in_a_row_are_one_run() {
        let (a, b) = (vk::Buffer::from_raw(1), vk::Buffer::from_raw(2));
        let mut gathered = Gathered::default();
        for buffer in [a, a, b, a] {
            gathered.push(Pass::Solid, buffer, command(), [0.0; 3], [0.0; 4]);
        }
        gathered.push(Pass::Cutout, a, command(), [1.0, 2.0, 3.0], [4.0; 4]);
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
        assert_eq!(gathered.bounds[4], [4.0; 4]);
    }

    #[test]
    fn a_pass_is_grouped_by_buffer_and_keeps_its_order_within() {
        let (a, b) = (vk::Buffer::from_raw(1), vk::Buffer::from_raw(2));
        let mut gathered = Gathered::default();
        for (buffer, x) in [(a, 0.0), (b, 1.0), (a, 2.0), (b, 3.0), (a, 4.0)] {
            gathered.queue(buffer, command(), [x, 0.0, 0.0], [x; 4]);
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
        // The bounds go with their draws.
        let bounds: Vec<f32> = gathered.bounds.iter().map(|b| b[0]).collect();
        assert_eq!(bounds, xs);
    }

    /// The counting sort in `finish` against a stable sort by buffer with
    /// `push`, over two passes of draws from many buffers in random order.
    #[test]
    fn finishing_matches_a_stable_sort_by_buffer() {
        let mut seed = 0x2545_f491_u64;
        let mut random = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut gathered = Gathered::default();
        let mut expected = Gathered::default();
        for pass in [Pass::Solid, Pass::Cutout] {
            let mut draws = Vec::new();
            for i in 0..2000u32 {
                let buffer = vk::Buffer::from_raw(1 + random() % 9);
                let command = DrawCommand {
                    index_count: 6 * (i + 1),
                    instance_count: 1,
                    first_index: 0,
                    vertex_offset: random() as i32 & 0xFFFF,
                    first_instance: 0,
                };
                let x = i as f32;
                draws.push((buffer, command, [x, 1.0, 2.0], [x, 3.0, 4.0, 5.0]));
                gathered.queue(buffer, command, [x, 1.0, 2.0], [x, 3.0, 4.0, 5.0]);
            }
            gathered.finish(pass);
            let mut order: Vec<vk::Buffer> = Vec::new();
            for &(buffer, ..) in &draws {
                if !order.contains(&buffer) {
                    order.push(buffer);
                }
            }
            draws.sort_by_key(|&(buffer, ..)| order.iter().position(|&b| b == buffer));
            for (buffer, command, offset, bounds) in draws {
                expected.push(pass, buffer, command, offset, bounds);
            }
        }
        let fields = |c: &DrawCommand| {
            (
                c.index_count,
                c.instance_count,
                c.first_index,
                c.vertex_offset,
                c.first_instance,
            )
        };
        let commands = |g: &Gathered| g.commands.iter().map(fields).collect::<Vec<_>>();
        assert_eq!(commands(&gathered), commands(&expected));
        assert_eq!(gathered.offsets, expected.offsets);
        assert_eq!(gathered.bounds, expected.bounds);
        assert_eq!(gathered.runs, expected.runs);
        assert!(
            gathered
                .commands
                .iter()
                .enumerate()
                .all(|(i, c)| c.first_instance == i as u32)
        );
    }

    /// Each pass's layers after the gathered draws, a run per block in block
    /// order, near to far within it; each entry knows where its draws start.
    #[test]
    fn expanded_layers_are_placed_by_block_after_the_gathered_draws() {
        let mut gathered = Gathered::default();
        gathered.queue(vk::Buffer::from_raw(9), command(), [0.0; 3], [0.0; 4]);
        gathered.finish(Pass::Solid);
        gathered.total = gathered.commands.len() as u32;
        let draw = |slot, cutout, block, draws| SectionDraw::new(slot, cutout, 1, block, draws);
        let sections = [
            draw(0, false, 2, 3),
            draw(1, true, 0, 1),
            draw(2, false, 0, 2),
            draw(3, false, 2, 1),
            draw(4, true, 0, 4),
        ];
        let buffer = |block: u32| Some(vk::Buffer::from_raw(100 + u64::from(block)));
        gather_sections(&mut gathered, &sections, buffer);
        let entry = |s: SectionDraw, at| [s.word, at];
        // Solid: block 0 (slot 2) at 1, then block 2 (slots 0, 3) at 3;
        // cutout's block 0 (slots 1, 4) after.
        assert_eq!(
            gathered.entries,
            [
                entry(sections[0], 3),
                entry(sections[2], 1),
                entry(sections[3], 6),
                entry(sections[1], 7),
                entry(sections[4], 8),
            ]
        );
        let run = |block: u64, first, count| Run {
            buffer: vk::Buffer::from_raw(100 + block),
            first,
            count,
        };
        assert_eq!(gathered.runs[Pass::Solid as usize][1..], [run(0, 1, 2), run(2, 3, 4)]);
        assert_eq!(gathered.runs[Pass::Cutout as usize], [run(0, 7, 5)]);
        assert_eq!(gathered.total, 12);
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
