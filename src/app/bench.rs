//! `--bench` in the app.

use std::path::{Path, PathBuf};

use crate::game::InWorld;
use crate::game::bench::nvml::GpuMonitor;
use crate::game::bench::report::{self, FrameSample, Recorder, Setup};
use crate::game::bench::{BenchOptions, Phase, Scene, WORLD_SEED};
use crate::game::subsystems::Snapshot;
use melon_audio::stats::AudioStats;
use melon_gui::hud::debug::RenderStats;
use melon_net::join::Target;
use melon_net::singleplayer::{
    self, Difficulty, WorldCreationMode, WorldSettings, rules, rules::Value,
};
use melon_render::Renderer;
use melon_settings::options::{self, Config};

/// Where the runs' CSV and summaries go without `--out`, from the working
/// directory.
const OUT_DIR: &str = "target/melon-bench";
/// Every how many frames the shading is counted: reading it back costs the
/// CPU time the run measures.
const SHADING_EVERY: u32 = 16;

pub(super) struct BenchRun {
    pub options: BenchOptions,
    recorder: Option<Recorder>,
    phase: Option<Phase>,
    /// NVML, while recording on an NVIDIA GPU.
    gpu_monitor: Option<GpuMonitor>,
    /// The shading frames already recorded.
    shading_read: u64,
}

impl BenchRun {
    pub fn new(options: BenchOptions) -> Self {
        Self {
            options,
            recorder: None,
            phase: None,
            gpu_monitor: None,
            shading_read: 0,
        }
    }

    /// The world was entered: recording starts.
    pub fn start(&mut self, renderer: &mut Renderer, config: &Config) {
        renderer.count_shading(SHADING_EVERY);
        let device = renderer.device_info();
        // NVML only knows NVIDIA's GPUs.
        let nvidia = device.gpu.vendor == numerobis::pci::Vendor::Nvidia;
        self.gpu_monitor = nvidia.then(|| GpuMonitor::start(&device.name)).flatten();
        if self.gpu_monitor.is_none() {
            println!("Bench: no NVML (no NVIDIA GPU, or no driver library), no GPU load");
        }
        self.recorder = Some(Recorder::new(Setup {
            label: self.options.label.clone(),
            scene: self.options.scene.name().to_owned(),
            device: match device.gpu.architecture.name() {
                Some(arch) => format!("{}{} [{arch}]", device.name, device.type_suffix),
                None => format!("{}{}", device.name, device.type_suffix),
            },
            resolution: renderer.framebuffer_size(),
            render_distance: config.render_distance,
            fps_cap: self.options.fps_cap,
            present_mode: renderer.present_mode_name().to_owned(),
        }));
        println!(
            "Bench '{}' ({}): {}{}, {}x{}, render distance {}, FPS limit {}",
            self.options.label,
            self.options.scene.name(),
            device.name,
            device.type_suffix,
            renderer.framebuffer_size().0,
            renderer.framebuffer_size().1,
            config.render_distance,
            self.options
                .fps_cap
                .map_or_else(|| "none".to_owned(), |fps| fps.to_string()),
        );
    }

    /// One frame drawn, `dt_ms` after the last. Whether the run is done.
    pub fn frame(
        &mut self,
        world: &InWorld,
        renderer: &Renderer,
        audio: &AudioStats,
        dt_ms: f32,
        render: RenderStats,
    ) -> bool {
        let (Some(recorder), Some(phase)) = (&mut self.recorder, world.bench_phase()) else {
            return false;
        };
        let net = world.net_stats();
        if self.phase != Some(phase) {
            let size = renderer.framebuffer_size();
            if size != recorder.resolution() {
                println!(
                    "Bench: resolution now {}x{} (was {}x{})",
                    size.0,
                    size.1,
                    recorder.resolution().0,
                    recorder.resolution().1
                );
                recorder.set_resolution(size);
            }
            if let Some(monitor) = &self.gpu_monitor {
                recorder.add_gpu_samples(monitor.take());
            }
            recorder.enter(phase, Snapshot::take(net, audio));
            self.phase = Some(phase);
            println!(
                "Bench: phase {} ({} entities)",
                phase.name(),
                world.entity_count()
            );
            if self.options.hold == Some(phase) {
                println!(
                    "Bench: --hold {}: the phase repeats until the window closes (nothing is written)",
                    phase.name()
                );
            }
        }
        if phase == Phase::Done {
            return true;
        }
        let (cpu_ms, cpu) = world.profiler.last_frame();
        let (gpu_ms, gpu) = renderer.gpu_last_frame();
        let (read, shading) = renderer.gpu_last_stats();
        let shading = if read == self.shading_read {
            Vec::new()
        } else {
            self.shading_read = read;
            shading.to_vec()
        };
        recorder.frame(
            FrameSample {
                frame_ms: dt_ms,
                cpu_ms,
                gpu_ms,
                cpu: cpu.to_vec(),
                gpu: gpu.to_vec(),
                terrain_draws: render.terrain_draws,
                sections_drawn: render.sections_drawn,
                entities: world.entity_count(),
                shading,
            },
            world.server_mspt(),
            net.totals().backlog,
            renderer.mesh_memory().1,
            world.chunk_count(),
        );
        false
    }

    /// Writes the files and prints the summary, and the comparison when there is an earlier
    /// report.
    pub fn finish(&self) {
        let Some(recorder) = &self.recorder else {
            melon_base::warn!("Bench: the world was never entered, nothing measured");
            return;
        };
        let summary = recorder.summary();
        let dir = self.options.out.as_deref().unwrap_or(Path::new(OUT_DIR));
        match recorder.write(dir) {
            Ok(path) => println!("Bench: {} (and .csv)", path.display()),
            Err(e) => melon_base::warn!("Bench: not written: {e:#}"),
        }
        print!("{}", report::table(&summary));
        if let Some(path) = &self.options.compare {
            match std::fs::read_to_string(path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
            {
                Some(old) => {
                    println!("Compared with {}:", path.display());
                    print!("{}", report::compare(&old, &summary));
                }
                None => melon_base::warn!("Bench: {} unreadable", path.display()),
            }
        }
    }
}

/// What the benchmark holds still for the run, in memory only.
pub(super) fn override_config(config: &mut Config, render_distance: Option<u8>) {
    // The heaviest preset, whatever the player chose: render distance 32,
    // fancy clouds out to 128, every particle.
    config.graphics_preset = options::GraphicsPreset::Fabulous;
    options::GraphicsPreset::Fabulous.apply(config);
    if let Some(chunks) = render_distance {
        config.render_distance = chunks;
        config.simulation_distance = chunks;
    }
    config.vsync = false;
    config.fullscreen = true;
    config.max_fps = options::MAX_FPS_RANGE.1;
}

/// Borderless fullscreen on the monitor with the most pixels, so a run
/// measures the largest resolution the machine has, whichever monitor the
/// window opened on.
pub(super) fn fullscreen_on_largest(window: &winit::window::Window) {
    let largest = largest_monitor(window.available_monitors());
    if let Some(monitor) = &largest {
        println!(
            "Bench: full screen on {} ({}x{})",
            monitor.name().unwrap_or_default(),
            monitor.size().width,
            monitor.size().height
        );
    }
    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(largest)));
}

/// The monitor with the most pixels.
pub(super) fn largest_monitor(
    monitors: impl Iterator<Item = winit::monitor::MonitorHandle>,
) -> Option<winit::monitor::MonitorHandle> {
    monitors.max_by_key(|monitor| {
        let size = monitor.size();
        u64::from(size.width) * u64::from(size.height)
    })
}

/// The benchmark's world, made the first time.
pub(super) fn prepare_world(game_dir: &Path, scene: Scene) -> anyhow::Result<Target> {
    let folder = scene.world_folder();
    let active = scene == Scene::Active;
    if active {
        // Made anew: what the last run summoned would still be there.
        let old = singleplayer::world_dir(game_dir, folder);
        if old.is_dir() {
            std::fs::remove_dir_all(&old)?;
        }
    }
    let settings = WorldSettings {
        name: "Melon Bench".to_owned(),
        game_mode: WorldCreationMode::Creative,
        seed: WORLD_SEED.to_owned(),
        // Monsters stay in the active world (and burn in its daylight).
        difficulty: if active {
            Difficulty::Normal
        } else {
            Difficulty::Peaceful
        },
        allow_cheats: true,
        ..WorldSettings::default()
    };
    singleplayer::save_world(game_dir, folder, &settings)?;
    let world_dir = singleplayer::world_dir(game_dir, folder);
    // The active scene lets the time run into the dusk; the weather it sets
    // itself, and only its own mobs come, so runs stay comparable.
    let rules: rules::Changes = [
        ("advance_time", Value::Bool(active)),
        ("advance_weather", Value::Bool(false)),
        ("spawn_mobs", Value::Bool(false)),
        ("send_command_feedback", Value::Bool(false)),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect();
    rules::write_changes(&world_dir, &rules)?;
    let players: PathBuf = world_dir.join("players").join("data");
    if players.is_dir() {
        std::fs::remove_dir_all(&players)?;
    }
    Ok(Target::World {
        save_path: world_dir.clone(),
        game_dir: game_dir.to_path_buf(),
        settings,
    })
}
