//! A frame's phases timed on the CPU, for F3.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// How much each frame moves the average: about the last 30 frames count.
const SMOOTHING: f32 = 1.0 / 30.0;
/// The kernel's counts are read this often.
const KERNEL_PERIOD: Duration = Duration::from_secs(1);
/// The frame's phases spent waiting off the CPU, on the GPU or the compositor.
pub const WAITS: [&str; 3] = ["gpu wait", "acquire wait", "present"];

pub struct FrameProfiler {
    /// Each phase's smoothed milliseconds, in the order first seen.
    phases: Vec<(&'static str, f32)>,
    frame_ms: f32,
    started: Instant,
    last: Instant,
    /// The current (after [`Self::end`].
    frame: Vec<(&'static str, f32)>,
    frame_total_ms: f32,
    /// When the last frame ended.
    ended: Option<Instant>,
    /// Since then: time in Melon's event handlers, and how many ran.
    gap_events: Duration,
    gap_count: u32,
    /// Smoothed: Melon's event handlers, the rest of the event loop, and the events per frame.
    events_ms: f32,
    event_loop_ms: f32,
    events_per_frame: f32,
    interval_ms: f32,
    kernel: KernelClock,
    /// Since the kernel's last count: the frames' time not spent in [`WAITS`], and the events'.
    working: Duration,
}

impl Default for FrameProfiler {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            phases: Vec::new(),
            frame_ms: 0.0,
            started: now,
            last: now,
            frame: Vec::new(),
            frame_total_ms: 0.0,
            ended: None,
            gap_events: Duration::ZERO,
            gap_count: 0,
            events_ms: 0.0,
            event_loop_ms: 0.0,
            events_per_frame: 0.0,
            interval_ms: 0.0,
            kernel: KernelClock::default(),
            working: Duration::ZERO,
        }
    }
}

impl FrameProfiler {
    /// Starts a frame whose work began at `now`.
    pub fn begin_at(&mut self, now: Instant) {
        if let Some(ended) = self.ended.take() {
            let between = now.saturating_duration_since(ended);
            let events = std::mem::take(&mut self.gap_events).min(between);
            let count = std::mem::take(&mut self.gap_count);
            let ms = |d: Duration| d.as_secs_f32() * 1000.0;
            let interval = ms(now.saturating_duration_since(self.started));
            self.events_ms += (ms(events) - self.events_ms) * SMOOTHING;
            self.event_loop_ms += (ms(between - events) - self.event_loop_ms) * SMOOTHING;
            self.events_per_frame += (count as f32 - self.events_per_frame) * SMOOTHING;
            self.interval_ms += (interval - self.interval_ms) * SMOOTHING;
        }
        self.kernel.sample(now, std::mem::take(&mut self.working));
        self.started = now;
        self.last = now;
        self.frame.clear();
    }

    /// One window or device event handled between frames, in `spent`.
    pub fn event(&mut self, spent: Duration) {
        self.working += spent;
        if self.ended.is_some() {
            self.gap_events += spent;
            self.gap_count += 1;
        }
    }

    /// Ends the phase `name`, which ran since the last mark.
    pub fn mark(&mut self, name: &'static str) {
        let now = Instant::now();
        let spent = now - self.last;
        self.last = now;
        self.add(name, spent);
    }

    /// Ends the phase `name`, of which `parts` were spent elsewhere, each counted on its own.
    pub fn mark_split(&mut self, name: &'static str, parts: &[(&'static str, Duration)]) {
        let now = Instant::now();
        let spent = now - self.last;
        self.last = now;
        let own = parts
            .iter()
            .fold(spent, |left, &(_, part)| left.saturating_sub(part));
        self.add(name, own);
        for &(part_name, part) in parts {
            self.add(part_name, part);
        }
    }

    /// Ends the frame.
    pub fn end(&mut self) {
        let now = Instant::now();
        let total = (now - self.started).as_secs_f32() * 1000.0;
        self.frame_ms += (total - self.frame_ms) * SMOOTHING;
        self.frame_total_ms = total;
        let waited: f32 = self
            .frame
            .iter()
            .filter(|(name, _)| WAITS.contains(name))
            .map(|&(_, ms)| ms)
            .sum();
        self.working += Duration::from_secs_f32(((total - waited) / 1000.0).max(0.0));
        self.ended = Some(now);
    }

    /// The last whole frame: its CPU time and each phase's, unsmoothed.
    pub fn last_frame(&self) -> (f32, &[(&'static str, f32)]) {
        (self.frame_total_ms, &self.frame)
    }

    fn add(&mut self, name: &'static str, spent: Duration) {
        let ms = spent.as_secs_f32() * 1000.0;
        self.frame.push((name, ms));
        match self.phases.iter_mut().find(|(n, _)| *n == name) {
            Some((_, average)) => *average += (ms - *average) * SMOOTHING,
            None => self.phases.push((name, ms)),
        }
    }

    fn phase(&self, name: &str) -> f32 {
        self.phases
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(0.0, |&(_, ms)| ms)
    }

    /// F3's lines.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Frame: {:.2} ms ({:.2} ms CPU in the frame)",
            self.interval_ms, self.frame_ms
        )];
        let total = self.interval_ms.max(self.frame_ms).max(f32::EPSILON);
        let share = |ms: f32| ms / total * 100.0;
        let line = |name: &str, ms: f32| format!("  {name}: {ms:.2} ms ({:.0}%)", share(ms));
        lines.extend(self.phases.iter().map(|&(name, ms)| line(name, ms)));
        lines.push(format!(
            "  input events: {:.2} ms ({:.0}%, {:.1} a frame)",
            self.events_ms,
            share(self.events_ms),
            self.events_per_frame
        ));
        lines.push(line("event loop", self.event_loop_ms));

        let present = self.phase("present");
        let acquire = self.phase("acquire wait");
        let os = self.event_loop_ms + present + acquire;
        lines.push(format!(
            "OS overhead: {os:.2} ms ({:.0}%): loop {:.2}, present {present:.2}, acquire {acquire:.2}",
            share(os),
            self.event_loop_ms
        ));
        if let Some(line) = self.kernel.line() {
            lines.push(line);
        }
        lines
    }
}

/// What the kernel counts for the main thread.
#[derive(Default)]
struct KernelClock {
    last: Option<(Instant, ThreadCounts)>,
    /// The working time since `last`.
    working: Duration,
    /// Over the last period.
    kernel_share: Option<f32>,
    /// Its time on a core over its working time: below 100% it was taken
    /// off the core, or blocked, while it had work.
    on_core: Option<f32>,
    /// Linux only (`schedstat`).
    queued_ms_per_s: Option<f32>,
}

/// Nanoseconds.
#[derive(Clone, Copy)]
struct ThreadCounts {
    user: u64,
    system: u64,
    queued: Option<u64>,
}

impl KernelClock {
    fn sample(&mut self, now: Instant, working: Duration) {
        self.working += working;
        if self
            .last
            .is_some_and(|(at, _)| now.saturating_duration_since(at) < KERNEL_PERIOD)
        {
            return;
        }
        let Some(counts) = ThreadCounts::read() else {
            return;
        };
        if let Some((at, before)) = self.last {
            let seconds = now.saturating_duration_since(at).as_secs_f32().max(1e-6);
            let user = counts.user.saturating_sub(before.user);
            let system = counts.system.saturating_sub(before.system);
            self.kernel_share =
                (user + system > 0).then(|| system as f32 / (user + system) as f32 * 100.0);
            let working = self.working.as_nanos() as f32;
            self.on_core = (working > 0.0).then(|| (user + system) as f32 / working * 100.0);
            self.queued_ms_per_s = counts
                .queued
                .zip(before.queued)
                .map(|(now, then)| now.saturating_sub(then) as f32 / 1e6 / seconds);
        }
        self.working = Duration::ZERO;
        self.last = Some((now, counts));
    }

    fn line(&self) -> Option<String> {
        let kernel = self.kernel_share?;
        let mut line = format!("  main thread: {kernel:.0}% of its CPU in the kernel");
        if let Some(on_core) = self.on_core {
            let _ = write!(line, ", on a core {on_core:.0}% of its working time");
        }
        if let Some(queued) = self.queued_ms_per_s {
            let _ = write!(line, ", waited {queued:.1} ms/s for a core");
        }
        Some(line)
    }
}

impl ThreadCounts {
    #[cfg(target_os = "linux")]
    fn read() -> Option<Self> {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").ok()?;
        let sched = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
        Self::parse(&stat, &sched)
    }

    #[cfg(windows)]
    fn read() -> Option<Self> {
        let (user, system) = crate::platform::current_thread_cpu()?;
        Some(Self {
            user,
            system,
            queued: None,
        })
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn read() -> Option<Self> {
        None
    }

    /// `stat`'s fields 14 and 15 (`utime`, `stime`, in `USER_HZ` = 100
    /// ticks a second) and `schedstat`'s wait time.
    #[cfg(any(target_os = "linux", test))]
    fn parse(stat: &str, sched: &str) -> Option<Self> {
        const TICK_NS: u64 = 10_000_000;
        let rest = stat.get(stat.rfind(')')? + 1..)?;
        let mut fields = rest.split_whitespace().skip(11);
        let user = fields.next()?.parse::<u64>().ok()? * TICK_NS;
        let system = fields.next()?.parse::<u64>().ok()? * TICK_NS;
        let queued = sched.split_whitespace().nth(1)?.parse().ok()?;
        Some(Self {
            user,
            system,
            queued: Some(queued),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_loop_and_the_events_between_frames_are_shown() {
        let mut profiler = FrameProfiler::default();
        for _ in 0..3 {
            profiler.begin_at(Instant::now());
            profiler.mark("work");
            profiler.end();
            profiler.event(Duration::from_millis(1));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(profiler.events_ms > 0.0);
        assert!(profiler.event_loop_ms > 0.0);
        assert!(profiler.interval_ms >= profiler.event_loop_ms + profiler.events_ms);
        let lines = profiler.lines();
        assert!(
            lines.iter().any(|l| l.starts_with("  input events:")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("OS overhead:")),
            "{lines:?}"
        );
        assert!(profiler.last_frame().0 < 5.0);
    }

    #[test]
    fn the_kernel_counts_are_read_after_the_name() {
        let stat = "123 (my (odd) name) S 1 2 3 4 5 6 7 8 9 10 250 40 0 0";
        let counts = ThreadCounts::parse(stat, "9000 777 12\n").unwrap();
        assert_eq!(counts.user, 2_500_000_000);
        assert_eq!(counts.system, 400_000_000);
        assert_eq!(counts.queued, Some(777));
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn this_thread_can_be_read() {
        assert!(ThreadCounts::read().is_some());
    }
}
