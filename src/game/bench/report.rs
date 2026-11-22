//! The benchmark's records.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::Phase;
use super::nvml::GpuSample;
use crate::game::subsystems::{Rates, Snapshot};
use crate::renderer::PassStats;

/// A change smaller than this share is noise, in the comparison.
pub const TOLERANCE: f64 = 0.10;

/// One frame.
#[derive(Clone, Debug, Default)]
pub struct FrameSample {
    /// Wall time since the frame before.
    pub frame_ms: f32,
    /// The frame loop's own time on the CPU.
    pub cpu_ms: f32,
    /// The newest frame the GPU finished.
    pub gpu_ms: f32,
    pub cpu: Vec<(&'static str, f32)>,
    pub gpu: Vec<(&'static str, f32)>,
    pub terrain_draws: usize,
    pub sections_drawn: usize,
    pub entities: usize,
    /// How much each counted pass shaded (pipeline statistics).
    pub shading: Vec<(&'static str, PassStats)>,
}

struct PhaseRecord {
    phase: Phase,
    frames: Vec<FrameSample>,
    start: Snapshot,
    end: Option<Snapshot>,
    mspt: Vec<f64>,
    backlog_max: u64,
    rss_max: u64,
    mesh_memory_max: u64,
    chunks_end: usize,
    /// NVML's readings while the phase ran.
    gpu_samples: Vec<GpuSample>,
}

/// What the machine and the options were, for reading a summary later.
#[derive(Clone, Debug, Default)]
pub struct Setup {
    pub label: String,
    /// `calm` or `active`.
    pub scene: String,
    pub device: String,
    pub resolution: (u32, u32),
    pub render_distance: u8,
    pub fps_cap: Option<u32>,
    pub present_mode: String,
}

pub struct Recorder {
    setup: Setup,
    phases: Vec<PhaseRecord>,
}

/// The process's resident memory in bytes, from `/proc/self/statm`.
pub fn resident_bytes() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4096)
}

/// Mean, percentiles and max of `values`, in their unit.
pub fn distribution(values: &[f32]) -> Value {
    if values.is_empty() {
        return json!({});
    }
    let mut sorted: Vec<f64> = values.iter().map(|&v| f64::from(v)).collect();
    sorted.sort_by(f64::total_cmp);
    let at = |p: f64| {
        let index = ((sorted.len() - 1) as f64 * p).round() as usize;
        sorted[index]
    };
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    json!({
        "mean": round(mean),
        "p50": round(at(0.50)),
        "p95": round(at(0.95)),
        "p99": round(at(0.99)),
        "max": round(sorted[sorted.len() - 1]),
    })
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn mean_by_name<'a>(rows: impl Iterator<Item = &'a [(&'static str, f32)]>) -> Value {
    let mut sums: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    let mut frames = 0;
    for row in rows {
        frames += 1;
        for &(name, ms) in row {
            let entry = sums.entry(name).or_default();
            entry.0 += f64::from(ms);
            entry.1 += 1;
        }
    }
    let mut out = Map::new();
    for (name, (sum, _)) in sums {
        out.insert(name.to_owned(), json!(round(sum / frames.max(1) as f64)));
    }
    Value::Object(out)
}

/// Per counted pass, the mean per frame of what it shaded, and its
/// fragments per screen pixel (`overdraw`); `total` sums the passes.
fn shading(frames: &[FrameSample], resolution: (u32, u32)) -> Value {
    let mut sums: Vec<(&str, PassStats)> = Vec::new();
    for frame in frames {
        for &(name, stats) in &frame.shading {
            match sums.iter_mut().find(|(n, _)| *n == name) {
                Some((_, sum)) => *sum += stats,
                None => sums.push((name, stats)),
            }
        }
    }
    if sums.is_empty() {
        return Value::Null;
    }
    let total = sums
        .iter()
        .fold(PassStats::default(), |mut all, &(_, stats)| {
            all += stats;
            all
        });
    let frames = frames.len().max(1) as f64;
    let pixels = (f64::from(resolution.0) * f64::from(resolution.1)).max(1.0);
    let entry = |stats: PassStats| {
        let fragments = stats.fragments as f64 / frames;
        json!({
            "vertices": round(stats.vertices as f64 / frames),
            "primitives": round(stats.primitives as f64 / frames),
            "fragments": round(fragments),
            "overdraw": round(fragments / pixels),
        })
    };
    let mut out: Map<String, Value> = sums
        .into_iter()
        .map(|(name, stats)| (name.to_owned(), entry(stats)))
        .collect();
    out.insert("total".to_owned(), entry(total));
    Value::Object(out)
}

/// The means of NVML's readings, and the most VRAM used; `null` without
/// NVML.
fn nvml(samples: &[GpuSample]) -> Value {
    if samples.is_empty() {
        return Value::Null;
    }
    let mean = |value: fn(&GpuSample) -> f64| {
        round(samples.iter().map(value).sum::<f64>() / samples.len() as f64)
    };
    json!({
        "samples": samples.len(),
        "gpu_util": mean(|s| f64::from(s.gpu_util)),
        "memory_util": mean(|s| f64::from(s.memory_util)),
        "sm_mhz": mean(|s| f64::from(s.sm_mhz)),
        "memory_mhz": mean(|s| f64::from(s.memory_mhz)),
        "power_w": mean(|s| f64::from(s.power_mw) / 1000.0),
        "vram_used_mib": round(
            samples.iter().map(|s| s.vram_used).max().unwrap_or(0) as f64 / (1024.0 * 1024.0)
        ),
    })
}

impl Recorder {
    pub fn new(setup: Setup) -> Self {
        Self {
            setup,
            phases: Vec::new(),
        }
    }

    /// The framebuffer size the summary states.
    pub fn resolution(&self) -> (u32, u32) {
        self.setup.resolution
    }

    /// The framebuffer changed size (a fullscreen switch landing late).
    pub fn set_resolution(&mut self, size: (u32, u32)) {
        self.setup.resolution = size;
    }

    /// The phase `phase` begins; the one before ends.
    pub fn enter(&mut self, phase: Phase, now: Snapshot) {
        if let Some(last) = self.phases.last_mut() {
            last.end = Some(now.clone());
        }
        if phase == Phase::Done {
            return;
        }
        self.phases.push(PhaseRecord {
            phase,
            frames: Vec::new(),
            start: now,
            end: None,
            mspt: Vec::new(),
            backlog_max: 0,
            rss_max: 0,
            mesh_memory_max: 0,
            chunks_end: 0,
            gpu_samples: Vec::new(),
        });
    }

    /// NVML readings taken during the current phase.
    pub fn add_gpu_samples(&mut self, samples: Vec<GpuSample>) {
        if let Some(record) = self.phases.last_mut() {
            record.gpu_samples.extend(samples);
        }
    }

    /// One frame of the current phase, and what is worth sampling with it.
    pub fn frame(
        &mut self,
        sample: FrameSample,
        mspt: Option<f64>,
        backlog: u64,
        mesh_memory: u64,
        chunks: usize,
    ) {
        let Some(record) = self.phases.last_mut() else {
            return;
        };
        if record.frames.len() % 64 == 0 {
            record.rss_max = record.rss_max.max(resident_bytes());
            if let Some(mspt) = mspt {
                record.mspt.push(mspt);
            }
        }
        record.backlog_max = record.backlog_max.max(backlog);
        record.mesh_memory_max = record.mesh_memory_max.max(mesh_memory);
        record.chunks_end = chunks;
        record.frames.push(sample);
    }

    /// The summary of every phase, as written to the JSON file.
    pub fn summary(&self) -> Value {
        let mut phases = Map::new();
        for record in &self.phases {
            let Some(end) = &record.end else { continue };
            let rates = Rates::between(&record.start, end);
            let frames = &record.frames;
            let frame_ms: Vec<f32> = frames.iter().map(|f| f.frame_ms).collect();
            let cpu_ms: Vec<f32> = frames.iter().map(|f| f.cpu_ms).collect();
            let gpu_ms: Vec<f32> = frames.iter().map(|f| f.gpu_ms).collect();
            let mean_of = |values: &[f32]| {
                values.iter().map(|&v| f64::from(v)).sum::<f64>() / values.len().max(1) as f64
            };
            let draws: Vec<f32> = frames.iter().map(|f| f.terrain_draws as f32).collect();
            let sections: Vec<f32> = frames.iter().map(|f| f.sections_drawn as f32).collect();
            let entities: Vec<f32> = frames.iter().map(|f| f.entities as f32).collect();
            let threads: Map<String, Value> = rates
                .threads
                .iter()
                .filter(|(_, cores)| *cores >= 0.001)
                .map(|(name, cores)| (name.clone(), json!(round(*cores))))
                .collect();
            let mspt = if record.mspt.is_empty() {
                Value::Null
            } else {
                json!(round(
                    record.mspt.iter().sum::<f64>() / record.mspt.len() as f64
                ))
            };
            phases.insert(
                record.phase.name().to_owned(),
                json!({
                    "seconds": round(rates.seconds),
                    "frames": frames.len(),
                    "fps": round(frames.len() as f64 / rates.seconds),
                    "frame_ms": distribution(&frame_ms),
                    "cpu_ms": distribution(&cpu_ms),
                    "gpu_ms": distribution(&gpu_ms),
                    "cpu": mean_by_name(frames.iter().map(|f| f.cpu.as_slice())),
                    "gpu": mean_by_name(frames.iter().map(|f| f.gpu.as_slice())),
                    "terrain_draws": round(mean_of(&draws)),
                    "sections_drawn": round(mean_of(&sections)),
                    "entities": round(mean_of(&entities)),
                    "net": {
                        "kib_s": round(rates.net_kib),
                        "packets_s": round(rates.net_packets),
                        "chunks_s": round(rates.net_chunks),
                        "decode_cores": round(rates.net_decode_cores),
                        "backlog_max": record.backlog_max,
                        "ms_per_chunk": round(rates.net_ms_per_chunk),
                        "chunks_loaded": record.chunks_end,
                    },
                    "sound": {
                        "started_s": round(rates.sounds_started),
                        "playing_end": rates.sounds_playing,
                        "play_ms": round(rates.sound_play_ms),
                        "decodes": rates.sound_decodes,
                        "decode_ms": round(rates.sound_decode_ms),
                    },
                    "shading": shading(frames, self.setup.resolution),
                    "nvml": nvml(&record.gpu_samples),
                    "server_mspt": mspt,
                    "threads": threads,
                    "rss_mib": round(record.rss_max as f64 / (1024.0 * 1024.0)),
                    "mesh_mib": round(record.mesh_memory_max as f64 / (1024.0 * 1024.0)),
                }),
            );
        }
        let setup = &self.setup;
        json!({
            "label": setup.label,
            "scene": setup.scene,
            "device": setup.device,
            "resolution": [setup.resolution.0, setup.resolution.1],
            "render_distance": setup.render_distance,
            "fps_cap": setup.fps_cap,
            "present_mode": setup.present_mode,
            "phases": Value::Object(phases),
        })
    }

    /// One row per frame.
    pub fn csv(&self) -> String {
        let mut cpu_names: Vec<&str> = Vec::new();
        let mut gpu_names: Vec<&str> = Vec::new();
        for frame in self.phases.iter().flat_map(|r| &r.frames) {
            for &(name, _) in &frame.cpu {
                if !cpu_names.contains(&name) {
                    cpu_names.push(name);
                }
            }
            for &(name, _) in &frame.gpu {
                if !gpu_names.contains(&name) {
                    gpu_names.push(name);
                }
            }
        }
        let mut out = String::from("phase,frame_ms,cpu_ms,gpu_ms,terrain_draws,sections_drawn");
        for name in &cpu_names {
            let _ = write!(out, ",cpu:{name}");
        }
        for name in &gpu_names {
            let _ = write!(out, ",gpu:{name}");
        }
        out.push('\n');
        for record in &self.phases {
            for frame in &record.frames {
                let _ = write!(
                    out,
                    "{},{:.3},{:.3},{:.3},{},{}",
                    record.phase.name(),
                    frame.frame_ms,
                    frame.cpu_ms,
                    frame.gpu_ms,
                    frame.terrain_draws,
                    frame.sections_drawn,
                );
                for (names, values) in [(&cpu_names, &frame.cpu), (&gpu_names, &frame.gpu)] {
                    for name in names.iter() {
                        out.push(',');
                        let sum: f32 = values
                            .iter()
                            .filter(|(n, _)| n == name)
                            .map(|&(_, ms)| ms)
                            .sum();
                        if values.iter().any(|(n, _)| n == name) {
                            let _ = write!(out, "{sum:.3}");
                        }
                    }
                }
                out.push('\n');
            }
        }
        out
    }

    /// Writes `<dir>/<label>-<unix time>.csv` and `.json`; returns the JSON's path.
    pub fn write(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let base = dir.join(format!("{}-{stamp}", self.setup.label));
        std::fs::write(base.with_extension("csv"), self.csv())?;
        let json = base.with_extension("json");
        std::fs::write(&json, serde_json::to_string_pretty(&self.summary())?)?;
        Ok(json)
    }
}

/// The summary as a short table, one row per phase.
pub fn table(summary: &Value) -> String {
    let mut out = String::from(
        "phase   sec    fps     frame ms (mean/p99)   cpu ms  gpu ms  ch/s   KiB/s  mspt\n",
    );
    let Some(phases) = summary["phases"].as_object() else {
        return out;
    };
    let number = |value: &Value| value.as_f64().unwrap_or(f64::NAN);
    for phase in Phase::MEASURED {
        let Some(p) = phases.get(phase.name()) else {
            continue;
        };
        let _ = writeln!(
            out,
            "{:<7} {:>5.1} {:>7.1}  {:>7.2} / {:>7.2}       {:>6.2}  {:>6.2}  {:>5.1}  {:>6.0}  {:>5.2}",
            phase.name(),
            number(&p["seconds"]),
            number(&p["fps"]),
            number(&p["frame_ms"]["mean"]),
            number(&p["frame_ms"]["p99"]),
            number(&p["cpu_ms"]["mean"]),
            number(&p["gpu_ms"]["mean"]),
            number(&p["net"]["chunks_s"]),
            number(&p["net"]["kib_s"]),
            number(&p["server_mspt"]),
        );
    }
    out
}

/// Every number in `value` by its path, `phases.turn.frame_ms.p99`.
fn flatten(prefix: &str, value: &Value, out: &mut BTreeMap<String, f64>) {
    match value {
        Value::Number(n) => {
            if let Some(n) = n.as_f64() {
                out.insert(prefix.to_owned(), n);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, value, out);
            }
        }
        _ => {}
    }
}

/// Whether more of the number at `path` is better (`Some(true)`), worse (`Some(false)`) or
/// neither.
fn more_is_better(path: &str) -> Option<bool> {
    let last = path.rsplit('.').next().unwrap_or(path);
    if last == "max" || last == "backlog_max" {
        return None;
    }
    if path.starts_with("phases.") && !path.contains(".threads.") {
        match last {
            "fps" | "chunks_s" => return Some(true),
            "seconds" if path.starts_with("phases.load.") => return Some(false),
            "seconds" | "frames" | "kib_s" | "packets_s" | "started_s" | "playing_end"
            | "decodes" | "chunks_loaded" | "terrain_draws" | "sections_drawn" => return None,
            _ => {}
        }
    }
    if path.contains(".threads.") {
        return Some(false);
    }
    // Busier, faster clocked or more power is neither better nor worse.
    if path.contains(".nvml.") {
        return None;
    }
    path.starts_with("phases.").then_some(false)
}

/// The comparison of `new` with `old`.
pub fn compare(old: &Value, new: &Value) -> String {
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    flatten("", old, &mut before);
    flatten("", new, &mut after);
    let mut out = String::new();
    for (path, &now) in &after {
        let Some(&then) = before.get(path) else {
            continue;
        };
        let Some(more_better) = more_is_better(path) else {
            continue;
        };
        let change = if then.abs() < 1e-9 {
            if now.abs() < 1e-9 { 0.0 } else { 1.0 }
        } else {
            (now - then) / then.abs()
        };
        let headline = path.ends_with("frame_ms.mean") || path.ends_with("frame_ms.p99");
        if change.abs() < TOLERANCE && !headline {
            continue;
        }
        if then.abs().max(now.abs()) < 0.05 && !headline {
            continue;
        }
        let better = (change > 0.0) == more_better;
        let mark = if change.abs() < TOLERANCE {
            "  "
        } else if better {
            "+ "
        } else {
            "- "
        };
        let _ = writeln!(
            out,
            "{mark}{path}: {then:.3} -> {now:.3} ({:+.1}%)",
            change * 100.0
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::stats::AudioTotals;
    use crate::game::thread_cpu::ThreadTimes;
    use crate::network::play::stats::NetTotals;
    use std::time::{Duration, Instant};

    fn snapshot(at: Instant) -> Snapshot {
        Snapshot {
            at,
            net: NetTotals::default(),
            audio: AudioTotals::default(),
            threads: ThreadTimes::default(),
        }
    }

    #[test]
    fn percentiles_of_a_known_spread() {
        let values: Vec<f32> = (1..=100).map(|v| v as f32).collect();
        let d = distribution(&values);
        assert_eq!(d["p50"], json!(51.0));
        assert_eq!(d["p99"], json!(99.0));
        assert_eq!(d["max"], json!(100.0));
        assert_eq!(d["mean"], json!(50.5));
    }

    #[test]
    fn a_phase_sums_up_its_frames() {
        let start = Instant::now();
        let mut recorder = Recorder::new(Setup {
            label: "t".into(),
            ..Setup::default()
        });
        recorder.enter(Phase::Turn, snapshot(start));
        for ms in [10.0, 20.0] {
            recorder.frame(
                FrameSample {
                    frame_ms: ms,
                    cpu: vec![("tick", ms / 2.0)],
                    ..FrameSample::default()
                },
                None,
                0,
                0,
                0,
            );
        }
        recorder.enter(Phase::Done, snapshot(start + Duration::from_secs(2)));
        let summary = recorder.summary();
        let turn = &summary["phases"]["turn"];
        assert_eq!(turn["frames"], json!(2));
        assert_eq!(turn["fps"], json!(1.0));
        assert_eq!(turn["frame_ms"]["mean"], json!(15.0));
        assert_eq!(turn["cpu"]["tick"], json!(7.5));
        let csv = recorder.csv();
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.starts_with("phase,frame_ms"));
        assert!(csv.lines().nth(1).unwrap().ends_with(",5.000"));
    }

    #[test]
    fn shading_is_a_mean_per_frame_with_overdraw_per_pixel() {
        let start = Instant::now();
        let mut recorder = Recorder::new(Setup {
            resolution: (10, 10),
            ..Setup::default()
        });
        recorder.enter(Phase::Turn, snapshot(start));
        let solid = |fragments| PassStats {
            vertices: 40,
            primitives: 20,
            fragments,
        };
        for fragments in [100, 300] {
            recorder.frame(
                FrameSample {
                    shading: vec![("solid", solid(fragments)), ("ui", solid(0))],
                    ..FrameSample::default()
                },
                None,
                0,
                0,
                0,
            );
        }
        recorder.add_gpu_samples(vec![
            GpuSample {
                gpu_util: 80,
                vram_used: 2 << 20,
                ..GpuSample::default()
            },
            GpuSample {
                gpu_util: 100,
                vram_used: 1 << 20,
                ..GpuSample::default()
            },
        ]);
        recorder.enter(Phase::Done, snapshot(start + Duration::from_secs(1)));
        let turn = &recorder.summary()["phases"]["turn"];
        assert_eq!(turn["shading"]["solid"]["fragments"], json!(200.0));
        assert_eq!(turn["shading"]["solid"]["overdraw"], json!(2.0));
        assert_eq!(turn["shading"]["total"]["vertices"], json!(80.0));
        assert_eq!(turn["nvml"]["gpu_util"], json!(90.0));
        assert_eq!(turn["nvml"]["vram_used_mib"], json!(2.0));
    }

    #[test]
    fn without_statistics_or_nvml_the_phase_says_null() {
        assert_eq!(shading(&[FrameSample::default()], (1, 1)), Value::Null);
        assert_eq!(nvml(&[]), Value::Null);
        assert_eq!(more_is_better("phases.turn.nvml.gpu_util"), None);
    }

    #[test]
    fn the_comparison_marks_what_got_worse() {
        let old = json!({"phases": {"turn": {
            "fps": 100.0,
            "frame_ms": {"mean": 10.0, "p99": 20.0},
            "gpu": {"terrain": 4.0},
            "net": {"kib_s": 100.0},
        }}});
        let new = json!({"phases": {"turn": {
            "fps": 120.0,
            "max": 99.0,
            "frame_ms": {"mean": 8.3, "p99": 20.2},
            "gpu": {"terrain": 5.0},
            "net": {"kib_s": 300.0},
        }}});
        let report = compare(&old, &new);
        assert!(report.contains("+ phases.turn.fps"), "{report}");
        assert!(report.contains("+ phases.turn.frame_ms.mean"), "{report}");
        assert!(report.contains("  phases.turn.frame_ms.p99"), "{report}");
        assert!(report.contains("- phases.turn.gpu.terrain"), "{report}");
        assert!(!report.contains("kib_s"), "{report}");
        assert!(!report.contains(".max"), "{report}");
    }
}
