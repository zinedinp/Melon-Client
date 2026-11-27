//! CPU time per subsystem, from the kernel's own count of each thread's time.

use std::collections::BTreeMap;

/// Nanoseconds on a core per thread group, since each thread started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadTimes(pub BTreeMap<String, u64>);

/// A thread's group: its name without a trailing number (`chunk-mesher-3` is `chunk-mesher`).
pub fn group_of(name: &str) -> &str {
    let name = name.trim();
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let trimmed = trimmed.trim_end_matches(['-', '_', ' ', '#']);
    if trimmed.is_empty() { name } else { trimmed }
}

impl ThreadTimes {
    /// All threads of this process right now.
    #[cfg(windows)]
    pub fn sample() -> Self {
        let mut groups = BTreeMap::new();
        for (name, nanos) in melon_base::platform::thread_cpu_times() {
            *groups.entry(group_of(&name).to_owned()).or_default() += nanos;
        }
        Self(groups)
    }

    /// All threads of this process right now.
    #[cfg(not(windows))]
    pub fn sample() -> Self {
        let mut groups = BTreeMap::new();
        let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
            return Self(groups);
        };
        for task in tasks.flatten() {
            let dir = task.path();
            let (Ok(name), Ok(sched)) = (
                std::fs::read_to_string(dir.join("comm")),
                std::fs::read_to_string(dir.join("schedstat")),
            ) else {
                continue;
            };
            let Some(nanos) = sched
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue;
            };
            *groups.entry(group_of(&name).to_owned()).or_default() += nanos;
        }
        Self(groups)
    }

    /// Cores busy per group between `earlier` and `self`, `seconds` apart.
    pub fn load_since(&self, earlier: &ThreadTimes, seconds: f64) -> Vec<(String, f64)> {
        let mut load: Vec<(String, f64)> = self
            .0
            .iter()
            .map(|(name, &now)| {
                let then = earlier.0.get(name).copied().unwrap_or(0);
                let busy = now.saturating_sub(then) as f64 / 1e9;
                (name.clone(), busy / seconds.max(1e-9))
            })
            .collect();
        load.sort_by(|a, b| b.1.total_cmp(&a.1));
        load
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_drop_the_thread_number() {
        assert_eq!(group_of("chunk-mesher-12\n"), "chunk-mesher");
        assert_eq!(group_of("server-worker"), "server-worker");
        assert_eq!(group_of("cpal_alsa_out"), "cpal_alsa_out");
        assert_eq!(group_of("42"), "42");
    }

    #[test]
    fn load_is_busy_time_over_wall_time() {
        let then = ThreadTimes([("melon".to_owned(), 1_000_000_000)].into());
        let now = ThreadTimes(
            [
                ("melon".to_owned(), 1_500_000_000),
                ("cpal_alsa_out".to_owned(), 10_000_000),
            ]
            .into(),
        );
        let load = now.load_since(&then, 1.0);
        assert_eq!(load[0].0, "melon");
        assert!((load[0].1 - 0.5).abs() < 1e-9);
        assert!((load[1].1 - 0.01).abs() < 1e-9);
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn this_process_has_threads() {
        assert!(!ThreadTimes::sample().0.is_empty());
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn a_named_thread_is_found_by_its_group() {
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let (started, ready) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("cpu-probe-7".to_owned())
            .spawn(move || {
                let _ = started.send(());
                let _ = wait.recv();
            })
            .unwrap();
        ready.recv().unwrap();
        let sampled = ThreadTimes::sample();
        drop(stop);
        thread.join().unwrap();
        assert!(
            sampled.0.contains_key("cpu-probe"),
            "{:?}",
            sampled.0.keys()
        );
    }
}
