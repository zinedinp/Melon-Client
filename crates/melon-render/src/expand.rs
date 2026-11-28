//! The terrain's solid and cutout multi-draws, expanded per visible section
//! on the GPU (`shaders/expand.comp`).
//!
//! Each section with quads keeps a record on the GPU, written when its
//! meshes change: where its quads are and where each facing group ends.
//! Per frame the CPU culls whole sections and hands over one 8-byte
//! [`crate::SectionDraw`] per visible layer: the record's slot, which facing groups
//! face the eye, and how many draws that makes ([`layer_draws`]).
//! `indirect.rs` places each layer's draws (grouped by quad block, near to
//! far within) and the shader writes them: the same commands, offsets and
//! bounds the CPU gathers from [`crate::ChunkDraw`]s, without building
//! ~10k of those every frame. `MELON_NO_EXPAND=1` gathers on the CPU again.

use ash::vk;
use melon_base::gfx::mapped::MappedBuffer;
use melon_world::terrain::FACINGS;

use super::mesh_arena::{MeshArena, QUADS_PER_DRAW};
use super::occlusion::{compute_pipeline, memory_barrier, pipeline_layout, set_layout};
use super::vk_util::{self, Place, create_buffer};
use super::{QuadMeshId, Renderer, SectionRecord};

const EXPAND_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/expand.comp.spv"));

/// Words of one record: the origin and bottom, then per layer its first
/// quad and its groups' ends.
pub(super) const RECORD_WORDS: usize = 4 + 2 * (1 + FACINGS);
/// `expand.comp`'s threads per group.
const GROUP: u32 = 64;
/// Bytes of `expand.comp`'s push constants: an `ivec4` and a `uvec4`.
const PUSH_SIZE: u32 = 32;
/// Records the buffer has room for at first (2.5 MiB).
const MIN_SLOTS: usize = 1 << 15;

/// Whether the draws are expanded on the GPU: unless `MELON_NO_EXPAND=1`.
pub(super) fn wanted() -> bool {
    std::env::var_os("MELON_NO_EXPAND").is_none_or(|v| v != "1")
}

/// The runs `shown` draws of a layer whose facing groups end at `ends`, in
/// order as `(first quad, end)`: shown groups next to each other, or with
/// only empty groups between, are one run.
pub fn layer_runs(ends: &[u32; FACINGS], shown: u32, mut run: impl FnMut(u32, u32)) {
    let mut start = 0;
    let mut open: Option<(u32, u32)> = None;
    for (group, &end) in ends.iter().enumerate() {
        if end > start {
            if shown >> group & 1 != 0 {
                open = Some((open.map_or(start, |(first, _)| first), end));
            } else if let Some((first, last)) = open.take() {
                run(first, last);
            }
        }
        start = end;
    }
    if let Some((first, last)) = open {
        run(first, last);
    }
}

/// How many draws a layer expands to (a run per [`QUADS_PER_DRAW`] quads),
/// and the quads they draw.
pub fn layer_draws(ends: &[u32; FACINGS], shown: u32) -> (u32, u32) {
    let (mut draws, mut quads) = (0, 0);
    layer_runs(ends, shown, |first, end| {
        draws += (end - first).div_ceil(QUADS_PER_DRAW);
        quads += end - first;
    });
    (draws, quads)
}

/// The GPU's records and the pipeline that expands them.
pub(super) struct Expand {
    set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    pool: vk::DescriptorPool,
    frames: Vec<FrameExpand>,
    records: Option<Records>,
    /// Every record as the CPU last set it, by slot.
    mirror: Vec<[u32; RECORD_WORDS]>,
    /// Slots set since the last frame took them.
    dirty: Vec<u32>,
    /// The records buffer was made anew: every record goes up again.
    all_dirty: bool,
}

/// The records on the GPU.
struct Records {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    slots: usize,
}

/// One frame in flight's set and the records it copies up.
#[derive(Default)]
struct FrameExpand {
    set: vk::DescriptorSet,
    staging: Option<MappedBuffer>,
    copies: Vec<vk::BufferCopy>,
}

/// The per-frame buffers `expand.comp` reads and writes (`indirect.rs`'s).
#[derive(Clone, Copy)]
pub(super) struct ExpandBuffers {
    pub entries: vk::Buffer,
    pub commands: vk::Buffer,
    pub offsets: vk::Buffer,
    pub bounds: vk::Buffer,
}

impl ExpandBuffers {
    fn ready(&self) -> bool {
        [self.entries, self.commands, self.offsets, self.bounds]
            .iter()
            .all(|&b| b != vk::Buffer::null())
    }
}

impl Expand {
    pub unsafe fn new(device: &ash::Device, frames: usize) -> anyhow::Result<Self> {
        let storage = [vk::DescriptorType::STORAGE_BUFFER; 5];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let set_layout = unsafe { set_layout(device, &storage) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pipeline_layout = unsafe { pipeline_layout(device, set_layout, PUSH_SIZE) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pipeline = unsafe { compute_pipeline(device, pipeline_layout, EXPAND_SPV) }?;
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: storage.len() as u32 * frames as u32,
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
        let layouts = vec![set_layout; frames];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
        }?;
        Ok(Self {
            set_layout,
            pipeline_layout,
            pipeline,
            pool,
            frames: sets
                .into_iter()
                .map(|set| FrameExpand {
                    set,
                    ..FrameExpand::default()
                })
                .collect(),
            records: None,
            mirror: Vec::new(),
            dirty: Vec::new(),
            all_dirty: false,
        })
    }

    /// Keeps `words` as slot `slot`'s record, sent up with the next frame.
    pub fn set(&mut self, slot: u32, words: [u32; RECORD_WORDS]) {
        let at = slot as usize;
        if self.mirror.len() <= at {
            self.mirror.resize(at + 1, [0; RECORD_WORDS]);
        }
        self.mirror[at] = words;
        self.dirty.push(slot);
    }

    /// Makes room for every record, stages the ones set since the last
    /// frame and points frame `frame`'s set at `buffers`. Call once its
    /// fence signalled.
    pub unsafe fn prepare(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
        buffers: ExpandBuffers,
    ) -> anyhow::Result<()> {
        let needed = self.mirror.len().max(1);
        if self.records.as_ref().is_none_or(|r| r.slots < needed) {
            // Only while a world loads, a few times at most.
            // SAFETY: nothing else submits while the renderer waits.
            unsafe { device.device_wait_idle() }?;
            let slots = needed.next_power_of_two().max(MIN_SLOTS);
            let size = (slots * RECORD_WORDS * 4) as u64;
            // SAFETY: `device` is live, and the infos name only its own live handles.
            let (buffer, memory) = unsafe {
                create_buffer(
                    device,
                    mem_props,
                    size,
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                    Place::DEVICE,
                )
            }?;
            if let Some(old) = self.records.replace(Records {
                buffer,
                memory,
                slots,
            }) {
                // SAFETY: the device is idle, so no frame uses it any more.
                unsafe {
                    device.destroy_buffer(old.buffer, None);
                    vk_util::free_memory(device, old.memory);
                }
            }
            self.all_dirty = true;
        }
        // SAFETY: the caller's promise, passed on.
        unsafe { self.stage(device, mem_props, frame) }?;
        let Some(records) = &self.records else {
            return Ok(());
        };
        if !buffers.ready() {
            return Ok(());
        }
        let info = |buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)]
        };
        let infos = [
            info(records.buffer),
            info(buffers.entries),
            info(buffers.commands),
            info(buffers.offsets),
            info(buffers.bounds),
        ];
        let set = self.frames[frame].set;
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(binding, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(binding as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(info)
            })
            .collect();
        // SAFETY: frame `frame`'s fence signalled, so no command buffer in flight binds its set.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        Ok(())
    }

    /// Copies the dirty records into frame `frame`'s staging buffer and
    /// lists the copies, runs of slots in a row as one.
    unsafe fn stage(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
    ) -> anyhow::Result<()> {
        let slot = &mut self.frames[frame];
        slot.copies.clear();
        if self.all_dirty {
            self.dirty.clear();
            self.dirty.extend(0..self.mirror.len() as u32);
            self.all_dirty = false;
        } else {
            self.dirty.sort_unstable();
            self.dirty.dedup();
        }
        if self.dirty.is_empty() {
            return Ok(());
        }
        let record = (RECORD_WORDS * 4) as u64;
        let bytes = self.dirty.len() as u64 * record;
        if slot.staging.as_ref().is_none_or(|s| (s.size() as u64) < bytes) {
            if let Some(old) = slot.staging.take() {
                // SAFETY: frame `frame`'s fence signalled, so the GPU is done copying from it.
                unsafe { old.destroy(device) };
            }
            // SAFETY: `device` is live, and the infos name only its own live handles.
            slot.staging = Some(unsafe {
                MappedBuffer::new(
                    device,
                    mem_props,
                    bytes.next_power_of_two(),
                    vk::BufferUsageFlags::TRANSFER_SRC,
                    Place::STAGING,
                )
            }?);
        }
        let staging = slot.staging.as_ref().expect("made just above");
        for (i, &dirty) in self.dirty.iter().enumerate() {
            let src = i as u64 * record;
            let dst = u64::from(dirty) * record;
            // SAFETY: frame `frame`'s fence signalled, so the GPU is off its staging buffer.
            unsafe { staging.write(src as usize, &self.mirror[dirty as usize]) };
            match slot.copies.last_mut() {
                Some(copy) if copy.dst_offset + copy.size == dst => copy.size += record,
                _ => slot.copies.push(vk::BufferCopy {
                    src_offset: src,
                    dst_offset: dst,
                    size: record,
                }),
            }
        }
        self.dirty.clear();
        Ok(())
    }

    /// Copies the staged records up, then expands `entries` entries with
    /// the camera in block `eye`; with `bounds`, also writes the occlusion
    /// culling's boxes. Outside a render pass, before the draws.
    pub unsafe fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        (entries, eye, bounds): (u32, glam::IVec3, bool),
    ) {
        let Some(records) = &self.records else {
            return;
        };
        let slot = &self.frames[frame];
        if let Some(staging) = &slot.staging
            && !slot.copies.is_empty()
        {
            let compute = vk::PipelineStageFlags::COMPUTE_SHADER;
            let transfer = vk::PipelineStageFlags::TRANSFER;
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                // Earlier frames' expansions read the records first.
                memory_barrier(
                    device,
                    cmd,
                    (compute, vk::AccessFlags::empty()),
                    (transfer, vk::AccessFlags::TRANSFER_WRITE),
                );
                device.cmd_copy_buffer(cmd, staging.buffer, records.buffer, &slot.copies);
                memory_barrier(
                    device,
                    cmd,
                    (transfer, vk::AccessFlags::TRANSFER_WRITE),
                    (compute, vk::AccessFlags::SHADER_READ),
                );
            }
        }
        if entries == 0 {
            return;
        }
        let mut push = [0u32; 8];
        push[..3].copy_from_slice(&eye.to_array().map(i32::cast_unsigned));
        push[4] = entries;
        push[5] = u32::from(bounds);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[slot.set],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                numerobis::elem::as_bytes(&push),
            );
            device.cmd_dispatch(cmd, entries.div_ceil(GROUP), 1, 1);
            memory_barrier(
                device,
                cmd,
                (
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                ),
                (
                    vk::PipelineStageFlags::DRAW_INDIRECT
                        | vk::PipelineStageFlags::VERTEX_SHADER
                        | vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::INDIRECT_COMMAND_READ | vk::AccessFlags::SHADER_READ,
                ),
            );
        }
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            for frame in &mut self.frames {
                if let Some(staging) = frame.staging.take() {
                    staging.destroy(device);
                }
            }
            if let Some(records) = self.records.take() {
                device.destroy_buffer(records.buffer, None);
                vk_util::free_memory(device, records.memory);
            }
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

/// `record`'s words, its layers' quads placed by `place`; a layer
/// without a mesh draws nothing.
fn record_words(
    record: &SectionRecord,
    place: impl Fn(QuadMeshId) -> Option<u32>,
) -> [u32; RECORD_WORDS] {
    let mut words = [0; RECORD_WORDS];
    let origin = record.origin.to_array().map(i32::cast_unsigned);
    words[..3].copy_from_slice(&origin);
    words[3] = record.bottom.cast_unsigned();
    for (layer, at) in record.layers.iter().zip([4, 5 + FACINGS]) {
        if let Some((mesh, ends)) = layer
            && let Some(base) = place(*mesh)
        {
            words[at] = base;
            words[at + 1..at + 1 + FACINGS].copy_from_slice(ends);
        }
    }
    words
}

impl Renderer {
    /// Whether the terrain's solid and cutout draws are expanded on the
    /// GPU: then [`crate::WorldDraw::sections`] holds them, from the
    /// records [`Self::set_section_record`] keeps.
    pub fn expands_terrain(&self) -> bool {
        self.expand.is_some()
    }

    /// The quad block `quads` lies in, for its [`crate::SectionDraw`]s.
    pub fn quad_block(&self, quads: QuadMeshId) -> Option<u16> {
        let slot = self.quad_meshes.slot(quads.0)?;
        u16::try_from(MeshArena::place(slot).0).ok()
    }

    /// Keeps `record` as slot `slot`'s, which [`crate::SectionDraw`]s expand from
    /// the next frame on; without the GPU expansion, nothing.
    pub fn set_section_record(&mut self, slot: u32, record: &SectionRecord) {
        let Some(expand) = &mut self.expand else {
            return;
        };
        let meshes = &self.quad_meshes;
        let place = |mesh: QuadMeshId| meshes.slot(mesh.0).map(|s| MeshArena::place(s).1);
        expand.set(slot, record_words(record, place));
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::SectionDraw;
    use crate::indirect::{DrawCommand, corner_offset, quad_draws};

    /// What `expand.comp` writes for `entry` at its place `at`: commands,
    /// offsets and bounds, as the CPU path gathers them.
    pub fn expected(
        record: &[u32; RECORD_WORDS],
        entry: SectionDraw,
        at: u32,
        eye: glam::IVec3,
    ) -> Vec<(DrawCommand, [f32; 4], [f32; 4])> {
        let origin = glam::IVec3::from_array([0, 1, 2].map(|i| record[i].cast_signed()));
        let offset = (origin - eye).as_vec3();
        let bound = [
            offset.x,
            offset.y + record[3].cast_signed() as f32,
            offset.z,
            f32::from_bits(entry.slot()),
        ];
        let layer = 4 + usize::from(entry.cutout()) * (1 + FACINGS);
        let base = record[layer];
        let ends: [u32; FACINGS] = record[layer + 1..layer + 1 + FACINGS]
            .try_into()
            .expect("FACINGS ends");
        let mut out = Vec::new();
        layer_runs(&ends, entry.shown(), |first, end| {
            for (from, quads) in quad_draws(&(first..end), ends[FACINGS - 1]) {
                let index = at + out.len() as u32;
                out.push((
                    DrawCommand {
                        index_count: quads * 6,
                        instance_count: 1,
                        first_index: 0,
                        vertex_offset: corner_offset(base + from),
                        first_instance: index,
                    },
                    [offset.x, offset.y, offset.z, 0.0],
                    bound,
                ));
            }
        });
        out
    }

    #[test]
    fn runs_merge_across_empty_groups_and_split_at_hidden_ones() {
        let runs = |ends: [u32; FACINGS], shown: u32| {
            let mut out = Vec::new();
            layer_runs(&ends, shown, |a, b| out.push((a, b)));
            out
        };
        let ends = [2, 2, 5, 7, 7, 9, 12];
        // Group 1 is empty, so 0 and 2 join; 3 is hidden.
        assert_eq!(runs(ends, 0b111_0101), [(0, 5), (7, 12)]);
        assert_eq!(runs(ends, 0), []);
        assert_eq!(runs(ends, 0x7F), [(0, 12)]);
        assert_eq!(layer_draws(&ends, 0b111_0101), (2, 10));
        let big = [0, 0, 0, 0, 0, 0, QUADS_PER_DRAW * 2 + 1];
        assert_eq!(layer_draws(&big, 1 << 6), (3, QUADS_PER_DRAW * 2 + 1));
    }

    #[test]
    fn a_section_draw_keeps_its_fields_apart() {
        let draw = SectionDraw::new(SectionDraw::SLOTS - 1, true, 0x55, 7, 3);
        assert_eq!(draw.slot(), SectionDraw::SLOTS - 1);
        assert!(draw.cutout());
        assert_eq!(draw.shown(), 0x55);
        assert!(!SectionDraw::new(5, false, 0x7F, 0, 1).cutout());
    }

    #[test]
    fn records_place_each_layer_and_skip_missing_meshes() {
        let record = SectionRecord {
            origin: glam::IVec3::new(-32, -64, 48),
            bottom: 80,
            layers: [
                Some((QuadMeshId(1), [1, 2, 3, 4, 5, 6, 7])),
                Some((QuadMeshId(2), [7; FACINGS])),
            ],
        };
        let words = record_words(&record, |mesh| (mesh.0 == 1).then_some(100));
        assert_eq!(words[..4], [(-32i32).cast_unsigned(), (-64i32).cast_unsigned(), 48, 80]);
        assert_eq!(words[4..12], [100, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(words[12..], [0; 8], "the cutout mesh is gone");
    }
}
