//! A frame's phases timed on the CPU, for F3: what the frame loop spends

use std::time::{Duration, Instant};

/// How much each frame moves the average: about the last 30 frames count.
const SMOOTHING: f32 = 1.0 / 30.0;
/// The kernel's counts are read this often.
const KERNEL_PERIOD: Duration = Duration::from_secs(1);

pub struct FrameProfiler {
    /// Each phase's smoothed milliseconds, in the order first seen.
    phases: Vec<(&'static str, f32)>,
    frame_ms: f32,
    started: Instant,
    last: Instant,
    /// The current (after [`Self::end`]: the last) frame's own times, for
    frame: Vec<(&'static str, f32)>,
    frame_total_ms: f32,
    /// When the last frame ended: from then until the next begins, the
    ended: Option<Instant>,
    /// Since then: time in Melon's event handlers, and how many ran.
    gap_events: Duration,
    gap_count: u32,
    /// Smoothed: Melon's event handlers, the rest of the event loop, how
    events_ms: f32,
    event_loop_ms: f32,
    events_per_frame: f32,
    interval_ms: f32,
    kernel: KernelClock,
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
        self.kernel.sample(now);
        self.started = now;
        self.last = now;
        self.frame.clear();
    }

    /// One window or device event handled between frames, in `spent`.
    pub fn event(&mut self, spent: Duration) {
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

    /// Ends the phase `name`, of which `parts` were spent elsewhere, each
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

    /// F3's lines: the whole frame, then each phase and the event loop
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

/// What the kernel counts for the main thread (`/proc/thread-self`): its
#[derive(Default)]
struct KernelClock {
    last: Option<(Instant, ThreadCounts)>,
    /// Over the last period: the share of the thread's CPU time spent in
    kernel_share: Option<f32>,
    queued_ms_per_s: f32,
}

#[derive(Clone, Copy)]
struct ThreadCounts {
    user_ticks: u64,
    system_ticks: u64,
    queued_ns: u64,
}

impl KernelClock {
    fn sample(&mut self, now: Instant) {
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
            let user = counts.user_ticks.saturating_sub(before.user_ticks);
            let system = counts.system_ticks.saturating_sub(before.system_ticks);
            self.kernel_share =
                (user + system > 0).then(|| system as f32 / (user + system) as f32 * 100.0);
            let queued = counts.queued_ns.saturating_sub(before.queued_ns);
            self.queued_ms_per_s = queued as f32 / 1e6 / seconds;
        }
        self.last = Some((now, counts));
    }

    fn line(&self) -> Option<String> {
        let kernel = self.kernel_share?;
        Some(format!(
            "  main thread: {kernel:.0}% of its CPU in the kernel, waited {:.1} ms/s for a core",
            self.queued_ms_per_s
        ))
    }
}

impl ThreadCounts {
    #[cfg(target_os = "linux")]
    fn read() -> Option<Self> {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").ok()?;
        let sched = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
        Self::parse(&stat, &sched)
    }

    #[cfg(not(target_os = "linux"))]
    fn read() -> Option<Self> {
        None
    }

    /// `stat`'s fields 14 and 15 (`utime`, `stime`) and `schedstat`'s
    #[cfg(any(target_os = "linux", test))]
    fn parse(stat: &str, sched: &str) -> Option<Self> {
        let rest = stat.get(stat.rfind(')')? + 1..)?;
        let mut fields = rest.split_whitespace().skip(11);
        let user_ticks = fields.next()?.parse().ok()?;
        let system_ticks = fields.next()?.parse().ok()?;
        let queued_ns = sched.split_whitespace().nth(1)?.parse().ok()?;
        Some(Self {
            user_ticks,
            system_ticks,
            queued_ns,
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
        assert_eq!(counts.user_ticks, 250);
        assert_eq!(counts.system_ticks, 40);
        assert_eq!(counts.queued_ns, 777);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_thread_can_be_read() {
        assert!(ThreadCounts::read().is_some());
    }
}
