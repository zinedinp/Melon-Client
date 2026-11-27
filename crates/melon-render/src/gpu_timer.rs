//! GPU time per pass for F3, from timestamp queries.

use ash::vk;
use std::cell::{Cell, RefCell};
use std::ffi::CStr;

/// Timestamps one frame may write.
const MAX_MARKS: u32 = 24;
/// Pipeline statistics queries one frame may use: one per counted pass.
const MAX_COUNTED: u32 = 24;
/// What each counted pass records, in the order the results come in
/// (by bit: vertex shader 2, clipping primitives 6, fragment shader 7).
const COUNTED: vk::QueryPipelineStatisticFlags = vk::QueryPipelineStatisticFlags::from_raw(
    vk::QueryPipelineStatisticFlags::VERTEX_SHADER_INVOCATIONS.as_raw()
        | vk::QueryPipelineStatisticFlags::CLIPPING_PRIMITIVES.as_raw()
        | vk::QueryPipelineStatisticFlags::FRAGMENT_SHADER_INVOCATIONS.as_raw(),
);

/// How much shading one pass did in a frame, from a pipeline statistics
/// query: what the GPU's shader cores were given, not how long it took.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassStats {
    /// Vertex shader invocations (after the post-transform cache).
    pub vertices: u64,
    /// Primitives that came out of clipping, so reached the rasterizer.
    pub primitives: u64,
    /// Fragment shader invocations: fragments early depth testing did not
    /// reject. Divided by the screen's pixels, the pass's overdraw.
    pub fragments: u64,
}

impl std::ops::AddAssign for PassStats {
    fn add_assign(&mut self, other: Self) {
        self.vertices += other.vertices;
        self.primitives += other.primitives;
        self.fragments += other.fragments;
    }
}
/// How much each frame moves the average: about the last 30 frames count.
const SMOOTHING: f32 = 1.0 / 30.0;

pub(super) struct GpuTimer {
    /// One per frame in flight; empty when the queue can't time.
    pools: Vec<vk::QueryPool>,
    /// What each slot's pool holds from its last frame, in order.
    written: Vec<Vec<&'static str>>,
    /// The frame being recorded: its pool, command buffer and labels so far.
    pool: Cell<vk::QueryPool>,
    cmd: Cell<vk::CommandBuffer>,
    labels: RefCell<Vec<&'static str>>,
    /// Nanoseconds per timestamp tick.
    period_ns: f64,
    /// The bits a timestamp holds (`timestampValidBits`).
    valid_mask: u64,
    /// Smoothed milliseconds per label, in the order first seen.
    phases: Vec<(&'static str, f32)>,
    total_ms: f32,
    /// The newest frame read back, unsmoothed, for the benchmark.
    last: Vec<(&'static str, f32)>,
    last_total_ms: f32,
    /// `VK_EXT_debug_utils`, when on: names each pass in the command buffer
    /// for GPU profilers (Nsight Graphics, RenderDoc), see [`Self::pass`].
    pass_labels: Option<ash::ext::debug_utils::Device>,
    /// Whether a pass label is open in the frame being recorded.
    label_open: Cell<bool>,
    /// For the pipeline statistics queries, which are begun and ended
    /// where the pass label is closed (no device at hand there).
    device: Option<ash::Device>,
    /// Pipeline statistics, one pool per frame in flight; empty without
    /// `pipelineStatisticsQuery`. Like the timestamps: what each slot's pool
    /// holds, the frame's pool and passes so far, and whether one is open.
    count_pools: Vec<vk::QueryPool>,
    counted_written: Vec<Vec<&'static str>>,
    count_pool: Cell<vk::QueryPool>,
    counted: RefCell<Vec<&'static str>>,
    count_open: Cell<bool>,
    /// The newest frame's statistics per pass, for the benchmark.
    last_stats: Vec<(&'static str, PassStats)>,
    /// How many times `last_stats` was read back.
    stats_read: u64,
    /// Every how many frames the shading is counted; 0: never. Reading the
    /// statistics back costs some drivers half a millisecond (Intel Arc).
    count_every: u32,
    frames_begun: u64,
}

impl GpuTimer {
    /// `valid_bits` is the queue family's `timestampValidBits`; 0 means no timer.
    pub(super) unsafe fn new(
        device: &ash::Device,
        limits: &vk::PhysicalDeviceLimits,
        valid_bits: u32,
        frames: usize,
        pass_labels: Option<ash::ext::debug_utils::Device>,
        statistics: bool,
    ) -> Self {
        let supported = valid_bits > 0 && limits.timestamp_compute_and_graphics == vk::TRUE;
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(MAX_MARKS);
        let pools = if supported {
            (0..frames)
                // SAFETY: `device` is live, and the infos name only its own live handles.
                .map_while(|_| unsafe { device.create_query_pool(&info, None) }.ok())
                .collect()
        } else {
            Vec::new()
        };
        let count_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::PIPELINE_STATISTICS)
            .query_count(MAX_COUNTED)
            .pipeline_statistics(COUNTED);
        let count_pools: Vec<vk::QueryPool> = if statistics {
            (0..frames)
                // SAFETY: `device` is live, and the infos name only its own live handles.
                .map_while(|_| unsafe { device.create_query_pool(&count_info, None) }.ok())
                .collect()
        } else {
            Vec::new()
        };
        Self {
            written: vec![Vec::new(); pools.len()],
            pools,
            pool: Cell::new(vk::QueryPool::null()),
            cmd: Cell::new(vk::CommandBuffer::null()),
            labels: RefCell::default(),
            period_ns: f64::from(limits.timestamp_period),
            valid_mask: if valid_bits >= 64 {
                u64::MAX
            } else {
                (1u64 << valid_bits) - 1
            },
            phases: Vec::new(),
            total_ms: 0.0,
            last: Vec::new(),
            last_total_ms: 0.0,
            pass_labels,
            label_open: Cell::new(false),
            device: (!count_pools.is_empty()).then(|| device.clone()),
            counted_written: vec![Vec::new(); count_pools.len()],
            count_pools,
            count_pool: Cell::new(vk::QueryPool::null()),
            counted: RefCell::default(),
            count_open: Cell::new(false),
            last_stats: Vec::new(),
            stats_read: 0,
            count_every: 0,
            frames_begun: 0,
        }
    }

    pub(super) unsafe fn destroy(&mut self, device: &ash::Device) {
        for pool in self.pools.drain(..).chain(self.count_pools.drain(..)) {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_query_pool(pool, None) };
        }
    }

    /// Starts frame slot `slot` on `cmd`, outside any render pass.
    pub(super) unsafe fn begin(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        slot: usize,
    ) {
        self.cmd.set(cmd);
        self.label_open.set(false);
        self.count_open.set(false);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.begin_counting(device, cmd, slot) };
        let Some(&pool) = self.pools.get(slot) else {
            return;
        };
        let written = std::mem::take(&mut self.written[slot]);
        if written.len() > 1 {
            let mut ticks = vec![0u64; written.len()];
            // SAFETY: the slot's last frame finished (its fence was waited on), so its queries are written.
            let read = unsafe {
                device.get_query_pool_results(pool, 0, &mut ticks, vk::QueryResultFlags::TYPE_64)
            };
            if read.is_ok() {
                self.take_in(&written, &ticks);
            }
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { device.cmd_reset_query_pool(cmd, pool, 0, MAX_MARKS) };
        self.pool.set(pool);
        self.cmd.set(cmd);
        self.labels.borrow_mut().clear();
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.write(device, vk::PipelineStageFlags::TOP_OF_PIPE, "start") };
    }

    /// Reads back what slot `slot` counted the last time, then readies its
    /// pool for this frame if it is one of the counted.
    unsafe fn begin_counting(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, slot: usize) {
        let Some(&pool) = self.count_pools.get(slot) else {
            return;
        };
        let written = std::mem::take(&mut self.counted_written[slot]);
        if !written.is_empty() {
            let mut results = vec![[0u64; 3]; written.len()];
            // SAFETY: the slot's last frame finished (its fence was waited on), so its queries are written.
            let read = unsafe {
                device.get_query_pool_results(pool, 0, &mut results, vk::QueryResultFlags::TYPE_64)
            };
            if read.is_ok() {
                self.last_stats = sum_by_pass(&written, &results);
                self.stats_read += 1;
            }
        }
        self.counted.borrow_mut().clear();
        self.frames_begun += 1;
        let counts = self.count_every > 0
            && self
                .frames_begun
                .is_multiple_of(u64::from(self.count_every));
        if !counts {
            self.count_pool.set(vk::QueryPool::null());
            return;
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { device.cmd_reset_query_pool(cmd, pool, 0, MAX_COUNTED) };
        self.count_pool.set(pool);
    }

    /// Ends the GPU work named `label`, which ran since the last mark.
    pub(super) unsafe fn mark(&self, device: &ash::Device, label: &'static str) {
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.close_pass() };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.write(device, vk::PipelineStageFlags::BOTTOM_OF_PIPE, label) };
    }

    /// Opens a debug label `label` for the work recorded from here on, up to
    /// the next [`Self::mark`] (or the next `pass`, or the frame's end). GPU
    /// profilers show the pass under that name; without a profiler attached
    /// the driver ignores it. Names past 31 bytes are cut.
    pub(super) unsafe fn pass(&self, label: &'static str) {
        let Some(api) = &self.pass_labels else {
            return;
        };
        let cmd = self.cmd.get();
        if cmd == vk::CommandBuffer::null() {
            return;
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.close_pass() };
        let mut name = [0u8; 32];
        let len = label.len().min(name.len() - 1);
        name[..len].copy_from_slice(&label.as_bytes()[..len]);
        let name = CStr::from_bytes_until_nul(&name).unwrap_or_default();
        let info = vk::DebugUtilsLabelEXT::default().label_name(name);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { api.cmd_begin_debug_utils_label(cmd, &info) };
        self.label_open.set(true);
    }

    /// [`Self::pass`], and counts the pass's shading as well (see
    /// [`PassStats`]) up to the next [`Self::mark`] or `pass`. Only inside a
    /// render pass that lasts until then: a pipeline statistics query must
    /// end in the subpass it began in.
    pub(super) unsafe fn counted_pass(&self, label: &'static str) {
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.pass(label) };
        let (Some(device), pool) = (&self.device, self.count_pool.get()) else {
            return;
        };
        let mut counted = self.counted.borrow_mut();
        if pool == vk::QueryPool::null() || counted.len() >= MAX_COUNTED as usize {
            return;
        }
        let cmd = self.cmd.get();
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_begin_query(
                cmd,
                pool,
                counted.len() as u32,
                vk::QueryControlFlags::empty(),
            );
        }
        counted.push(label);
        self.count_open.set(true);
    }

    /// Closes the open pass label and pipeline statistics query, if any.
    pub(super) unsafe fn close_pass(&self) {
        if let Some(device) = &self.device
            && self.count_open.replace(false)
        {
            let index = self.counted.borrow().len() as u32 - 1;
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { device.cmd_end_query(self.cmd.get(), self.count_pool.get(), index) };
        }
        if let Some(api) = &self.pass_labels
            && self.label_open.replace(false)
        {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { api.cmd_end_debug_utils_label(self.cmd.get()) };
        }
    }

    /// Ends the frame's recording; its times are read when its slot comes round again.
    pub(super) fn end(&mut self, slot: usize) {
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.close_pass() };
        self.cmd.set(vk::CommandBuffer::null());
        if let Some(written) = self.written.get_mut(slot) {
            *written = std::mem::take(&mut *self.labels.borrow_mut());
        }
        self.pool.set(vk::QueryPool::null());
        if let Some(written) = self.counted_written.get_mut(slot) {
            *written = std::mem::take(&mut *self.counted.borrow_mut());
        }
        self.count_pool.set(vk::QueryPool::null());
    }

    unsafe fn write(
        &self,
        device: &ash::Device,
        stage: vk::PipelineStageFlags,
        label: &'static str,
    ) {
        let pool = self.pool.get();
        let mut labels = self.labels.borrow_mut();
        if pool == vk::QueryPool::null() || labels.len() >= MAX_MARKS as usize {
            return;
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { device.cmd_write_timestamp(self.cmd.get(), stage, pool, labels.len() as u32) };
        labels.push(label);
    }

    /// Folds one frame's timestamps into the averages.
    fn take_in(&mut self, labels: &[&'static str], ticks: &[u64]) {
        let ms = |from: u64, to: u64| {
            let delta = to.wrapping_sub(from) & self.valid_mask;
            (delta as f64 * self.period_ns / 1_000_000.0) as f32
        };
        let mut frame: Vec<(&'static str, f32)> = Vec::new();
        for (pair, &label) in ticks.windows(2).zip(&labels[1..]) {
            let spent = ms(pair[0], pair[1]);
            match frame.iter_mut().find(|(name, _)| *name == label) {
                Some((_, sum)) => *sum += spent,
                None => frame.push((label, spent)),
            }
        }
        for (name, average) in &mut self.phases {
            let spent = frame
                .iter()
                .find(|(n, _)| n == name)
                .map_or(0.0, |&(_, ms)| ms);
            *average += (spent - *average) * SMOOTHING;
        }
        for &(name, spent) in &frame {
            if !self.phases.iter().any(|(n, _)| *n == name) {
                self.phases.push((name, spent));
            }
        }
        let total = ms(ticks[0], ticks[ticks.len() - 1]);
        self.total_ms += (total - self.total_ms) * SMOOTHING;
        self.last = frame;
        self.last_total_ms = total;
    }

    /// The newest frame the GPU finished: its time and each pass's.
    pub(super) fn last_frame(&self) -> (f32, &[(&'static str, f32)]) {
        (self.last_total_ms, &self.last)
    }

    /// The newest counted frame's shading per pass, and how many were read
    /// so far (a new count means a new frame).
    pub(super) fn last_stats(&self) -> (u64, &[(&'static str, PassStats)]) {
        (self.stats_read, &self.last_stats)
    }

    /// Counts the shading of every `every`th frame; 0 stops it.
    pub(super) fn count_every(&mut self, every: u32) {
        self.count_every = every;
    }

    /// F3's lines: the frame's GPU time, then each pass with its share.
    pub(super) fn lines(&self) -> Vec<String> {
        if self.pools.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![format!("Frame: {:.2} ms (GPU)", self.total_ms)];
        let total = self.total_ms.max(f32::EPSILON);
        lines.extend(
            self.phases
                .iter()
                .map(|(name, ms)| format!("  {name}: {ms:.2} ms ({:.0}%)", ms / total * 100.0)),
        );
        lines
    }
}

/// One frame's pipeline statistics, `[vertex, primitive, fragment]` per
/// query in `results`, summed per pass name in the order first seen.
fn sum_by_pass(labels: &[&'static str], results: &[[u64; 3]]) -> Vec<(&'static str, PassStats)> {
    let mut frame: Vec<(&'static str, PassStats)> = Vec::new();
    for (&label, &[vertices, primitives, fragments]) in labels.iter().zip(results) {
        let stats = PassStats {
            vertices,
            primitives,
            fragments,
        };
        match frame.iter_mut().find(|(name, _)| *name == label) {
            Some((_, sum)) => *sum += stats,
            None => frame.push((label, stats)),
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer() -> GpuTimer {
        GpuTimer {
            pools: vec![vk::QueryPool::null()],
            written: vec![Vec::new()],
            pool: Cell::new(vk::QueryPool::null()),
            cmd: Cell::new(vk::CommandBuffer::null()),
            labels: RefCell::default(),
            period_ns: 1.0,
            valid_mask: u64::MAX,
            phases: Vec::new(),
            total_ms: 0.0,
            last: Vec::new(),
            last_total_ms: 0.0,
            pass_labels: None,
            label_open: Cell::new(false),
            device: None,
            count_pools: Vec::new(),
            counted_written: Vec::new(),
            count_pool: Cell::new(vk::QueryPool::null()),
            counted: RefCell::default(),
            count_open: Cell::new(false),
            last_stats: Vec::new(),
            stats_read: 0,
            count_every: 0,
            frames_begun: 0,
        }
    }

    #[test]
    fn statistics_add_up_per_pass_in_first_seen_order() {
        let stats = sum_by_pass(
            &["solid", "cutout", "solid"],
            &[[4, 2, 100], [8, 4, 50], [1, 1, 10]],
        );
        assert_eq!(
            stats,
            vec![
                (
                    "solid",
                    PassStats {
                        vertices: 5,
                        primitives: 3,
                        fragments: 110
                    }
                ),
                (
                    "cutout",
                    PassStats {
                        vertices: 8,
                        primitives: 4,
                        fragments: 50
                    }
                ),
            ]
        );
    }

    #[test]
    fn each_label_gets_the_time_since_the_mark_before() {
        let mut timer = timer();
        let labels = ["start", "upload", "solid", "ui"];
        let ticks = [10_000_000, 11_000_000, 14_000_000, 16_000_000];
        timer.take_in(&labels, &ticks);
        assert_eq!(
            timer.phases,
            vec![("upload", 1.0), ("solid", 3.0), ("ui", 2.0)]
        );
        assert!((timer.total_ms - 6.0 * SMOOTHING).abs() < 1e-6);
    }

    #[test]
    fn a_wrapped_counter_still_measures_forward() {
        let mut timer = timer();
        timer.valid_mask = (1 << 36) - 1;
        timer.take_in(&["start", "solid"], &[(1 << 36) - 500, 1_500]);
        assert!((timer.phases[0].1 - 0.002).abs() < 1e-6);
    }

    #[test]
    fn a_pass_missing_this_frame_fades_out() {
        let mut timer = timer();
        timer.take_in(&["start", "solid"], &[0, 1_000_000]);
        timer.take_in(&["start", "ui"], &[0, 1_000_000]);
        assert!((timer.phases[0].1 - (1.0 - SMOOTHING)).abs() < 1e-6);
        assert_eq!(timer.phases[1], ("ui", 1.0));
    }
}
