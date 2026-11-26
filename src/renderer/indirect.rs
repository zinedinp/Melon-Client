//! The terrain's quad draws as indirect multi-draws.

use ash::vk;
use melon_base::gfx::mapped::MappedBuffer;
use numerobis::elem::Pod;

use super::mesh_arena::{self, MeshArena};
use super::vk_util::Place;
use super::{ChunkDraw, TerrainMesh, WorldDraw};

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
    capacity: u32,
    set: vk::DescriptorSet,
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
        ] {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { mapped.destroy(device) };
        }
        self.capacity = 0;
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
}

impl Gathered {
    fn clear(&mut self) {
        self.commands.clear();
        self.offsets.clear();
        self.bounds.clear();
        self.slots = 0;
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
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: frames as u32,
        }];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&sizes)
                    .max_sets(frames as u32),
                None,
            )
        }?;
        let layouts = vec![layout; frames];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
        }?;
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

    /// Gathers the quad meshes of `world`'s solid and cutout passes, before
    /// the frame's slot is free: it reads only the CPU's own state.
    pub fn gather(&mut self, world: &WorldDraw, meshes: &MeshArena) {
        let gathered = &mut self.gathered;
        gathered.clear();
        gathered.slots = world.section_slots;
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
    /// With `culling`, also what the occlusion culling reads of them.
    pub unsafe fn upload(&self, frame: usize, culling: bool) {
        let slot = &self.frames[frame];
        let gathered = &self.gathered;
        let (Some(commands), Some(offsets), Some(bounds), Some(run_table)) = (
            &slot.commands.0,
            &slot.offsets.0,
            &slot.bounds.0,
            &slot.run_table.0,
        ) else {
            return;
        };
        let count = gathered.commands.len().min(slot.capacity as usize);
        debug_assert_eq!(count, gathered.commands.len(), "prepared too small");
        // SAFETY: this frame's fence was waited on (this fn's contract), so the GPU is off its buffers.
        unsafe {
            commands.write(0, &gathered.commands[..count]);
            offsets.write(0, &gathered.offsets[..count]);
        }
        if !culling {
            return;
        }
        for (i, run) in gathered.runs.iter().flatten().enumerate().take(count) {
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
            unsafe { slot.destroy(device) };
        }
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { device.destroy_descriptor_pool(self.pool, None) };
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

    #[test]
    fn a_long_mesh_is_cut_into_draws() {
        let per = mesh_arena::QUADS_PER_DRAW;
        let draws: Vec<_> = quad_draws(&(0..per * 2 + 3), per * 2 + 3).collect();
        assert_eq!(draws, [(0, per), (per, per), (per * 2, 3)]);
        // A range past the mesh stops at its end.
        assert_eq!(quad_draws(&(0..10), 4).collect::<Vec<_>>(), [(0, 4)]);
    }
}
