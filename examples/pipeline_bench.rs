//! Headless bench of every step a chunk takes through the client, on real

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use glam::Vec3;
use serde::{Deserialize, Serialize};

use melon::game::camera;
use melon::game::level_meshes::LevelMeshes;
use melon::math::euler::{Look, Orientation};
use melon::network::{self, Identity, WorldEvent};
use melon::renderer::world_projection;
use melon::singleplayer::{self, server};
use melon::world::block_models::BlockModels;
use melon::world::block_tint::TintSettings;
use melon::world::chunk::{Chunk, ChunkLight, ChunkScan, LightSection, Section};
use melon::world::culling::frustum::Frustum;
use melon::world::culling::{CullSettings, Culler};
use melon::world::fluid::FluidSprites;
use melon::world::level::Level;
use melon::world::terrain::{self, MeshContext, Neighborhood};

/// Chunks around the spawn chunk: 17 x 17.
const RADIUS: i32 = 8;
const BASELINE: &str = "bench/baseline.json";
const OUT_DIR: &str = "target/pipeline-bench";
/// A step is only flagged slower when it also lost this much time, so
const NOISE_FLOOR_US: f64 = 2.0;

#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
struct Machine {
    cpu: String,
    threads: usize,
}

#[derive(Serialize, Deserialize, Clone)]
struct Stage {
    name: String,
    /// What one sample is: a chunk, a call, a whole pass.
    per: String,
    n: usize,
    median_us: f64,
    p90_us: f64,
    max_us: f64,
    total_ms: f64,
}

#[derive(Serialize, Deserialize, Default)]
struct Report {
    machine: Machine,
    git: String,
    /// Hash of the frozen chunks the counts come from.
    #[serde(default)]
    chunks: String,
    stages: Vec<Stage>,
    counts: BTreeMap<String, u64>,
}

impl Report {
    fn samples(&mut self, name: &str, per: &str, mut samples: Vec<Duration>) {
        assert!(!samples.is_empty(), "{name}: no samples");
        samples.sort();
        let us = |d: Duration| d.as_secs_f64() * 1e6;
        let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
        let stage = Stage {
            name: name.to_owned(),
            per: per.to_owned(),
            n: samples.len(),
            median_us: us(at(0.5)),
            p90_us: us(at(0.9)),
            max_us: us(samples[samples.len() - 1]),
            total_ms: samples.iter().sum::<Duration>().as_secs_f64() * 1e3,
        };
        println!(
            "  {:<34} n={:<6} median={:>10} p90={:>10} max={:>10}",
            stage.name,
            stage.n,
            fmt_us(stage.median_us),
            fmt_us(stage.p90_us),
            fmt_us(stage.max_us)
        );
        self.stages.push(stage);
    }

    /// Runs `f` `reps` times as one stage of whole passes.
    fn passes<T>(&mut self, name: &str, reps: usize, mut f: impl FnMut() -> T) -> T {
        let mut samples = Vec::with_capacity(reps);
        let mut last = None;
        for _ in 0..reps {
            let t = Instant::now();
            let out = std::hint::black_box(f());
            samples.push(t.elapsed());
            last = Some(out);
        }
        self.samples(name, "pass", samples);
        last.expect("reps > 0")
    }

    fn count(&mut self, name: &str, value: u64) {
        println!("  {name:<34} = {value}");
        self.counts.insert(name.to_owned(), value);
    }
}

fn fmt_us(us: f64) -> String {
    if us >= 10_000.0 {
        format!("{:.1} ms", us / 1e3)
    } else if us >= 100.0 {
        format!("{us:.0} µs")
    } else {
        format!("{us:.2} µs")
    }
}

struct Args {
    save: bool,
    fresh: bool,
    tolerance: f64,
}

fn args() -> anyhow::Result<Args> {
    let mut args = Args {
        save: false,
        fresh: false,
        tolerance: 0.15,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--save" => args.save = true,
            "--fresh" => args.fresh = true,
            "--tolerance" => {
                args.tolerance = it.next().context("--tolerance <fraction>")?.parse()?;
            }
            other => bail!("unknown argument {other}"),
        }
    }
    Ok(args)
}

fn main() -> anyhow::Result<()> {
    melon::platform::drop_cargo_stack();
    let args = args()?;
    let mut report = Report {
        machine: machine(),
        git: git_rev(),
        ..Report::default()
    };
    println!(
        "Pipeline bench: {} ({} threads), {}",
        report.machine.cpu, report.machine.threads, report.git
    );

    println!("\nStartup");
    let assets = melon::assets::Assets::discover()?;
    let models = report.passes("startup.block_models_bake", 3, || {
        BlockModels::bake(assets.root(), Vec::new(), 16384).0
    });
    let models = Arc::new(models);
    let fluids = fluid_sprites(&models);
    icon_bake(&mut report, assets.root());

    println!("\nWorld");
    let (spawn, chunks) = frozen_chunks(&mut report, args.fresh)?;
    report.count("world.chunks", chunks.len() as u64);

    println!("\nParsing (connection thread)");
    let packets: Vec<Vec<u8>> = chunks.iter().map(encode_chunk).collect();
    report.count(
        "parse.packet_bytes",
        packets.iter().map(|p| p.len() as u64).sum(),
    );
    let decoded = network::play::level::chunk::decode(
        &packets[0],
        melon::world::dimension::Height::OVERWORLD,
    )?;
    let roundtrip = decoded
        .sections
        .iter()
        .zip(&chunks[0].sections)
        .all(|(a, b)| a == b);
    if !roundtrip {
        bail!("decode(encode(chunk)) differs from the chunk: the bench's encoder is off");
    }
    let mut samples = Vec::new();
    for _ in 0..3 {
        for packet in &packets {
            let t = Instant::now();
            std::hint::black_box(network::play::level::chunk::decode(
                packet,
                melon::world::dimension::Height::OVERWORLD,
            )?);
            samples.push(t.elapsed());
        }
    }
    report.samples("parse.decode", "chunk", samples);
    let sections: Vec<Vec<Section>> = chunks
        .iter()
        .map(|c| c.sections.iter().map(|s| Section::clone(s)).collect())
        .collect();
    let mut samples = Vec::new();
    let mut block_entities = 0;
    for _ in 0..3 {
        block_entities = 0;
        for (chunk, sections) in chunks.iter().zip(&sections) {
            let t = Instant::now();
            let scan = std::hint::black_box(ChunkScan::of(
                chunk.x,
                chunk.z,
                melon::world::dimension::Height::OVERWORLD.min_y,
                sections,
            ));
            samples.push(t.elapsed());
            block_entities += scan.block_entities.len() as u64;
        }
    }
    report.samples("parse.chunk_scan", "chunk", samples);
    report.count("parse.block_entities", block_entities);

    println!("\nArrival (main thread)");
    let mut level = Level::default();
    let mut samples = Vec::new();
    for chunk in &chunks {
        let chunk = chunk.clone();
        let t = Instant::now();
        level.insert(chunk);
        samples.push(t.elapsed());
    }
    report.samples("level.insert", "chunk", samples);

    println!("\nMeshing");
    let by_key: HashMap<(i32, i32), &Chunk> = chunks.iter().map(|c| ((c.x, c.z), c)).collect();
    let plains_tints = TintSettings::plains();
    let context = MeshContext {
        smooth_lighting: true,
        cutout_leaves: true,
        models: &models,
        fluids: &fluids,
        min_y: melon::world::dimension::Height::OVERWORLD.min_y,
        cardinal: [1.0; 6],
        tints: &plains_tints,
    };
    mesh_single(&mut report, &chunks, &by_key, &context);
    mesh_parallel(&mut report, &chunks, &by_key, &context);
    let finished = mesh_pipeline(&mut report, &mut level, &models, fluids, spawn)?;

    println!("\nCulling and draw lists (main thread, per frame)");
    let mut meshes = LevelMeshes::default();
    let mut next_id = 0;
    for mesh in finished {
        meshes.insert_headless(mesh, || {
            next_id += 1;
            next_id
        });
    }
    let surface = surface_y(&level, spawn.x.floor() as i32, spawn.z.floor() as i32);
    cull(&mut report, &level, &mut meshes, spawn, surface);

    println!("\nBlock changes (light, urgent remesh)");
    edits(&mut report, &mut level, &models, fluids, spawn, surface)?;

    compare_and_save(&report, &args)
}

fn machine() -> Machine {
    let cpu = melon::platform::cpu_name().unwrap_or_else(|| "unbekannt".to_owned());
    Machine {
        cpu,
        threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
    }
}

fn git_rev() -> String {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let rev = run(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let dirty =
        run(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    if dirty { format!("{rev}+lokal") } else { rev }
}

fn fluid_sprites(models: &BlockModels) -> FluidSprites {
    let s = |id: &str| models.sprite(id).unwrap_or([0.0; 4]);
    FluidSprites {
        water_still: s(FluidSprites::TEXTURES[0]),
        water_flow: s(FluidSprites::TEXTURES[1]),
        water_overlay: s(FluidSprites::TEXTURES[2]),
        lava_still: s(FluidSprites::TEXTURES[3]),
        lava_flow: s(FluidSprites::TEXTURES[4]),
    }
}

/// Item icons, as the creative inventory's search tab bakes them.
fn icon_bake(report: &mut Report, root: &Path) {
    let tabs = &*melon::world::creative_tabs::TABS;
    let Some(search) = tabs
        .iter()
        .find(|t| t.kind == melon::world::creative_tabs::TabType::Search)
    else {
        return;
    };
    let mut seen = std::collections::HashSet::new();
    let mut samples = Vec::new();
    for stack in search
        .items
        .iter()
        .filter(|s| seen.insert(s.item.id))
        .take(300)
    {
        let t = Instant::now();
        std::hint::black_box(melon::gui::hud::item_icon::bake(
            root,
            stack.item.registry_key,
        ));
        samples.push(t.elapsed());
    }
    report.samples("startup.item_icon_bake", "item", samples);
}

/// The chunks of an earlier run, or new ones from the server, frozen for
fn frozen_chunks(report: &mut Report, fresh: bool) -> anyhow::Result<(Vec3, Vec<Chunk>)> {
    let path = Path::new(OUT_DIR).join("chunks.bin");
    if !fresh && let Ok(bytes) = std::fs::read(&path) {
        let (spawn, packets) = read_frozen(&bytes).context("chunks.bin unreadable; try --fresh")?;
        report.chunks = fingerprint(&bytes);
        println!("  {} chunks from {}", packets.len(), path.display());
        let chunks = packets
            .iter()
            .map(|p| {
                network::play::level::chunk::decode(p, melon::world::dimension::Height::OVERWORLD)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        return Ok((spawn, chunks));
    }
    let (spawn, chunks) = receive_chunks(report, fresh)?;
    let mut bytes = Vec::new();
    for v in [spawn.x, spawn.y, spawn.z] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    for chunk in &chunks {
        let packet = encode_chunk(chunk);
        bytes.extend_from_slice(&(packet.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&packet);
    }
    std::fs::create_dir_all(OUT_DIR)?;
    std::fs::write(&path, &bytes)?;
    report.chunks = fingerprint(&bytes);
    println!("  frozen to {}", path.display());
    let (_, packets) = read_frozen(&bytes).expect("just written");
    let chunks = packets
        .iter()
        .map(|p| network::play::level::chunk::decode(p, melon::world::dimension::Height::OVERWORLD))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok((spawn, chunks))
}

fn read_frozen(bytes: &[u8]) -> Option<(Vec3, Vec<&[u8]>)> {
    struct Cursor<'a>(&'a [u8]);
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let (head, tail) = self.0.split_at_checked(n)?;
            self.0 = tail;
            Some(head)
        }
        fn word(&mut self) -> Option<[u8; 4]> {
            self.take(4)?.try_into().ok()
        }
    }
    let mut r = Cursor(bytes);
    let spawn = Vec3::new(
        f32::from_le_bytes(r.word()?),
        f32::from_le_bytes(r.word()?),
        f32::from_le_bytes(r.word()?),
    );
    let count = u32::from_le_bytes(r.word()?) as usize;
    let mut packets = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u32::from_le_bytes(r.word()?) as usize;
        packets.push(r.take(len)?);
    }
    Some((spawn, packets))
}

fn fingerprint(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Starts the integrated server, joins, and waits for the chunks around
fn receive_chunks(report: &mut Report, fresh: bool) -> anyhow::Result<(Vec3, Vec<Chunk>)> {
    let dir = PathBuf::from(OUT_DIR).join("world");
    if fresh && dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    let t = Instant::now();
    let (server, pipe) = server::start(server::Options {
        save_path: dir,
        game_dir: PathBuf::from("."),
        settings: singleplayer::WorldSettings {
            name: "pipeline-bench".into(),
            seed: "12345".into(),
            ..Default::default()
        },
        view_distance: RADIUS as u8 + 2,
        simulation_distance: RADIUS as u8 + 2,
        host: melon::singleplayer::server::Host {
            uuid: Identity::Offline {
                username: "Bench".into(),
            }
            .uuid(),
            name: "Bench".into(),
            online: false,
        },
    })?;
    let mut session = network::join_stream(
        pipe,
        &Identity::Offline {
            username: "Bench".into(),
        },
        &Default::default(),
        Box::new(|| {}),
    )?;
    report.samples("world.server_start_and_join", "pass", vec![t.elapsed()]);

    let t = Instant::now();
    let mut spawn: Option<Vec3> = None;
    let mut chunks: HashMap<(i32, i32), Chunk> = HashMap::new();
    loop {
        for event in session.poll() {
            match event {
                WorldEvent::ChunkLoaded(c) => {
                    chunks.insert((c.x, c.z), c);
                }
                WorldEvent::LightUpdated { x, z, update } => {
                    if let Some(c) = chunks.get_mut(&(x, z)) {
                        Arc::make_mut(&mut c.light).apply(&update);
                    }
                }
                WorldEvent::Teleport(tp) if spawn.is_none() => {
                    spawn = Some(Vec3::new(tp.pos.0 as f32, tp.pos.1 as f32, tp.pos.2 as f32));
                }
                _ => {}
            }
        }
        if let Some(e) = session.ended() {
            bail!("connection closed: {e}");
        }
        if let Some(spawn) = spawn {
            let (cx, cz) = (spawn.x.floor() as i32 >> 4, spawn.z.floor() as i32 >> 4);
            let all = (-RADIUS..=RADIUS)
                .all(|dx| (-RADIUS..=RADIUS).all(|dz| chunks.contains_key(&(cx + dx, cz + dz))));
            if all {
                std::thread::sleep(Duration::from_millis(500));
                for event in session.poll() {
                    if let WorldEvent::LightUpdated { x, z, update } = event
                        && let Some(c) = chunks.get_mut(&(x, z))
                    {
                        Arc::make_mut(&mut c.light).apply(&update);
                    }
                }
                report.samples("world.receive_chunks", "pass", vec![t.elapsed()]);
                drop(session);
                drop(server);
                let mut chunks: Vec<Chunk> = chunks
                    .into_values()
                    .filter(|c| (c.x - cx).abs() <= RADIUS && (c.z - cz).abs() <= RADIUS)
                    .collect();
                chunks.sort_by_key(|c| (c.x, c.z));
                return Ok((spawn, chunks));
            }
        }
        if t.elapsed() > Duration::from_secs(180) {
            bail!("only {} chunks after 180 s", chunks.len());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn neighborhood<'a>(chunk: &'a Chunk, by_key: &HashMap<(i32, i32), &'a Chunk>) -> Neighborhood<'a> {
    let mut n = Neighborhood::alone(chunk);
    for dz in -1..=1 {
        for dx in -1..=1 {
            if (dx, dz) != (0, 0) {
                n.chunks[((dx + 1) + (dz + 1) * 3) as usize] =
                    by_key.get(&(chunk.x + dx, chunk.z + dz)).copied();
            }
        }
    }
    n
}

/// One chunk at a time on this thread: what a mesh worker spends per chunk.
fn mesh_single(
    report: &mut Report,
    chunks: &[Chunk],
    by_key: &HashMap<(i32, i32), &Chunk>,
    context: &MeshContext,
) {
    let mut mesh_samples = Vec::new();
    let mut vis_samples = Vec::new();
    let mut vertices = [0u64; 3];
    let mut bytes = 0u64;
    let mut runs = 0u64;
    for rep in 0..2 {
        for chunk in chunks {
            let n = neighborhood(chunk, by_key);
            let t = Instant::now();
            let mesh = terrain::mesh_chunk(&n, context);
            mesh_samples.push(t.elapsed());
            let t = Instant::now();
            std::hint::black_box(terrain::section_visibility(chunk));
            vis_samples.push(t.elapsed());
            if rep == 0 {
                for section in &mesh {
                    for (total, count) in vertices.iter_mut().zip(section.vertex_counts()) {
                        *total += count as u64;
                    }
                    bytes += section.bytes() as u64;
                    let groups = [
                        &section.solid.groups,
                        &section.cutout.groups,
                        &section.lava.groups,
                    ];
                    for group in groups {
                        runs += group.iter().filter(|r| !r.is_empty()).count() as u64;
                    }
                    runs += u64::from(!section.translucent.vertices.is_empty());
                }
            }
        }
    }
    report.samples("mesh.mesh_chunk", "chunk", mesh_samples);
    report.samples("mesh.section_visibility", "chunk", vis_samples);
    report.count("mesh.gpu_bytes", bytes);
    report.count("mesh.vertices_solid", vertices[0]);
    report.count("mesh.vertices_cutout", vertices[1]);
    report.count("mesh.vertices_translucent", vertices[2]);
    report.count("mesh.section_slices", runs);
}

/// Every chunk on 1, 2, 4... threads at once: how well meshing scales, and
fn mesh_parallel(
    report: &mut Report,
    chunks: &[Chunk],
    by_key: &HashMap<(i32, i32), &Chunk>,
    context: &MeshContext,
) {
    let max = report.machine.threads;
    let mut counts: Vec<usize> = std::iter::successors(Some(1usize), |n| Some(n * 2))
        .take_while(|&n| n < max)
        .collect();
    counts.push(max);
    let mut single = None;
    for threads in counts {
        let mut samples = Vec::new();
        for _ in 0..3 {
            let next = AtomicUsize::new(0);
            let t = Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| {
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(chunk) = chunks.get(i) else { break };
                            let n = neighborhood(chunk, by_key);
                            std::hint::black_box(terrain::mesh_chunk(&n, context));
                        }
                    });
                }
            });
            samples.push(t.elapsed());
        }
        samples.sort();
        let best = samples[0].as_secs_f64();
        let single = *single.get_or_insert(best);
        let speedup = single / best;
        println!(
            "  {:<34} {:>3} threads: {:>8.1} chunks/s  speedup {:>4.2}x  efficiency {:>3.0} %",
            "mesh.parallel",
            threads,
            chunks.len() as f64 / best,
            speedup,
            speedup / threads as f64 * 100.0
        );
        report.samples(&format!("mesh.parallel_{threads}t"), "pass", samples);
    }
}

/// The real thing: every chunk through `Level`'s queue and its mesh
fn mesh_pipeline(
    report: &mut Report,
    level: &mut Level,
    models: &Arc<BlockModels>,
    fluids: FluidSprites,
    eye: Vec3,
) -> anyhow::Result<Vec<melon::world::level::FinishedMesh>> {
    let mut finished = Vec::new();
    let mut frames = Vec::new();
    let t = Instant::now();
    loop {
        let frame = Instant::now();
        finished.extend(level.mesh_pending(models, fluids, eye.as_dvec3()));
        frames.push(frame.elapsed());
        let stats = level.mesh_stats();
        if stats.waiting == 0 && stats.in_flight == 0 && !finished.is_empty() {
            break;
        }
        if t.elapsed() > Duration::from_secs(60) {
            bail!("mesh pipeline not done after 60 s");
        }
        std::thread::sleep(Duration::from_micros(250));
    }
    report.samples("mesh.pipeline_until_done", "pass", vec![t.elapsed()]);
    report.samples("mesh.pipeline_main_per_frame", "frame", frames);
    report.count("mesh.pipeline_meshed_sections", finished.len() as u64);
    Ok(finished)
}

fn surface_y(level: &Level, x: i32, z: i32) -> i32 {
    (-64..320)
        .rev()
        .find(|&y| level.block_state(x, y, z).is_some_and(|s| s != 0))
        .unwrap_or(64)
}

/// Draw lists from a few cameras: above ground in four directions, looking
fn cull(report: &mut Report, level: &Level, meshes: &mut LevelMeshes, spawn: Vec3, surface: i32) {
    let eye_y = surface as f32 + 1.0 + melon::world::player::EYE_HEIGHT;
    let views = [
        ("north", Vec3::new(spawn.x, eye_y, spawn.z), 180.0, 0.0),
        ("east", Vec3::new(spawn.x, eye_y, spawn.z), 270.0, 0.0),
        ("south", Vec3::new(spawn.x, eye_y, spawn.z), 0.0, 0.0),
        ("west", Vec3::new(spawn.x, eye_y, spawn.z), 90.0, 0.0),
        ("down", Vec3::new(spawn.x, eye_y + 30.0, spawn.z), 0.0, 60.0),
        ("underground", Vec3::new(spawn.x, -20.0, spawn.z), 0.0, 0.0),
    ];
    let (min_y, height) = level.height();
    let far = camera::far_plane(RADIUS as u8);
    let projection = world_projection(70.0, 16.0 / 9.0, far);
    let mut walks = Vec::new();
    let mut lists = Vec::new();
    for (name, eye, yaw, pitch) in views {
        let eye = eye.as_dvec3();
        let view = Orientation::new(Look::from_melon(yaw, pitch)).view;
        let culler = Culler::new(
            Frustum::from_view_projection(projection * view),
            None,
            eye,
            (min_y, height),
            CullSettings::default(),
        );
        let section = culler.section_of(eye);
        let mut mask = None;
        for _ in 0..20 {
            let t = Instant::now();
            mask = Some(level.occlusion_walk(section, RADIUS, None));
            walks.push(t.elapsed());
        }
        let mask = mask.expect("walked");
        let culler = culler.with_occlusion(&mask);
        let mut count = None;
        for _ in 0..50 {
            let t = Instant::now();
            let visible = meshes.visible_sections(&culler);
            lists.push(t.elapsed());
            count = Some(visible.count);
            meshes.recycle(visible.solid, visible.cutout, visible.translucent);
        }
        let count = count.expect("listed");
        report.count(&format!("cull.{name}.reached"), mask.count() as u64);
        report.count(&format!("cull.{name}.sections_drawn"), count.drawn as u64);
        report.count(&format!("cull.{name}.draws"), count.draws as u64);
        report.count(&format!("cull.{name}.vertices"), count.vertices as u64);
    }
    report.samples("cull.occlusion_walk", "walk", walks);
    report.samples("cull.visible_sections", "frame", lists);
}

/// A player digging a shaft, lighting it, closing it, and breaking a block
fn edits(
    report: &mut Report,
    level: &mut Level,
    models: &Arc<BlockModels>,
    fluids: FluidSprites,
    spawn: Vec3,
    surface: i32,
) -> anyhow::Result<()> {
    level.set_chunk_builder(melon::settings::options::ChunkBuilder::PlayerAffected);
    let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
    let torch = pumpkin_data::Block::TORCH.default_state.id.as_u16();
    let (x, z) = (spawn.x.floor() as i32, spawn.z.floor() as i32);
    let bottom = surface - 15;

    let mut light = Vec::new();
    let mut remesh = Vec::new();
    let mut edit = |level: &mut Level, pos: [i32; 3], state: u16| -> anyhow::Result<()> {
        let t = Instant::now();
        level
            .set_block(pos, state, true)
            .context("edit outside the loaded chunks")?;
        light.push(t.elapsed());
        let t = Instant::now();
        std::hint::black_box(level.mesh_pending(models, fluids, spawn.as_dvec3()));
        remesh.push(t.elapsed());
        Ok(())
    };
    for y in (bottom..=surface).rev() {
        edit(level, [x, y, z], 0)?;
    }
    edit(level, [x, bottom, z], torch)?;
    edit(level, [x, surface + 1, z], stone)?;
    edit(level, [x, surface + 1, z], 0)?;
    let (cx, cz) = ((x & !15) + 15, (z & !15) + 15);
    let corner_y = surface_y(level, cx, cz);
    let corner = level.block_state(cx, corner_y, cz).unwrap_or(0);
    edit(level, [cx, corner_y, cz], 0)?;
    edit(level, [cx, corner_y, cz], corner)?;
    report.samples("edit.set_block_with_light", "edit", light);
    report.samples("edit.urgent_remesh", "edit", remesh);

    let mut sum = 0u64;
    for y in bottom - 2..=surface + 2 {
        for dz in -6..=6 {
            for dx in -6..=6 {
                let (block, sky) = level.light_at(x + dx, y, z + dz);
                sum += u64::from(block) * 16 + u64::from(sky);
            }
        }
    }
    report.count("edit.light_fingerprint", sum);
    Ok(())
}

fn var_int(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    loop {
        if v & !0x7f == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
}

/// A `LevelChunkWithLight` body for `chunk`, as a server would send it.
fn encode_chunk(chunk: &Chunk) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&chunk.x.to_be_bytes());
    out.extend_from_slice(&chunk.z.to_be_bytes());
    var_int(&mut out, 0);
    let mut data = Vec::new();
    for section in &chunk.sections {
        let blocks = section.iter().filter(|&b| b != 0).count() as i16;
        data.extend_from_slice(&blocks.to_be_bytes());
        data.extend_from_slice(&0i16.to_be_bytes());
        encode_blocks(&mut data, &section.to_array()[..]);
        data.push(0);
        var_int(&mut data, 0);
    }
    var_int(&mut out, data.len() as i32);
    out.extend_from_slice(&data);
    var_int(&mut out, 0);
    encode_light(&mut out, &chunk.light);
    out
}

/// A block `PalettedContainer`: one value, a palette of up to 256, or
fn encode_blocks(out: &mut Vec<u8>, blocks: &[u16]) {
    let mut palette: Vec<u16> = Vec::new();
    let mut index: HashMap<u16, u64> = HashMap::new();
    for &b in blocks {
        index.entry(b).or_insert_with(|| {
            palette.push(b);
            palette.len() as u64 - 1
        });
    }
    if palette.len() == 1 {
        out.push(0);
        var_int(out, i32::from(palette[0]));
        return;
    }
    let needed = usize::BITS - (palette.len() - 1).leading_zeros();
    if needed <= 8 {
        let bits = needed.max(4);
        out.push(bits as u8);
        var_int(out, palette.len() as i32);
        for &p in &palette {
            var_int(out, i32::from(p));
        }
        pack(out, blocks.iter().map(|b| index[b]), bits);
    } else {
        out.push(15);
        pack(out, blocks.iter().map(|&b| u64::from(b)), 15);
    }
}

fn pack(out: &mut Vec<u8>, values: impl Iterator<Item = u64>, bits: u32) {
    let per_long = (64 / bits) as usize;
    let values: Vec<u64> = values.collect();
    for group in values.chunks(per_long) {
        let mut word = 0u64;
        for (slot, v) in group.iter().enumerate() {
            word |= v << (slot as u32 * bits);
        }
        out.extend_from_slice(&word.to_be_bytes());
    }
}

fn encode_light(out: &mut Vec<u8>, light: &ChunkLight) {
    let masks = |layers: &[LightSection]| {
        let mut data = 0u64;
        let mut empty = 0u64;
        for (i, layer) in layers.iter().enumerate() {
            match layer {
                LightSection::Data(_) => data |= 1 << i,
                LightSection::Empty => empty |= 1 << i,
            }
        }
        (data, empty)
    };
    let bit_set = |out: &mut Vec<u8>, bits: u64| {
        let bytes = bits.to_le_bytes();
        let len = 8 - bits.leading_zeros() as usize / 8;
        var_int(out, len as i32);
        out.extend_from_slice(&bytes[..len]);
    };
    let arrays = |out: &mut Vec<u8>, layers: &[LightSection]| {
        let data: Vec<_> = layers
            .iter()
            .filter_map(|l| match l {
                LightSection::Data(d) => Some(d),
                LightSection::Empty => None,
            })
            .collect();
        var_int(out, data.len() as i32);
        for d in data {
            var_int(out, d.len() as i32);
            out.extend_from_slice(&d[..]);
        }
    };
    let (sky, sky_empty) = masks(&light.sky);
    let (block, block_empty) = masks(&light.block);
    bit_set(out, sky);
    bit_set(out, block);
    bit_set(out, sky_empty);
    bit_set(out, block_empty);
    arrays(out, &light.sky);
    arrays(out, &light.block);
}

fn compare_and_save(report: &Report, args: &Args) -> anyhow::Result<()> {
    std::fs::create_dir_all(OUT_DIR)?;
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(Path::new(OUT_DIR).join("last.json"), &json)?;

    let baseline: Option<Report> = std::fs::read_to_string(BASELINE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let mut problems = 0;
    match &baseline {
        None => println!("\nNo baseline at {BASELINE}."),
        Some(base) => {
            println!(
                "\nCompared with the baseline ({}, {})",
                base.git, base.machine.cpu
            );
            if base.machine != report.machine {
                println!("  ! Other machine: only the counts compare, not the times.");
            }
            let same_machine = base.machine == report.machine;
            let same_chunks = base.chunks == report.chunks;
            if !same_chunks {
                println!("  ! Other frozen chunks: the counts do not compare.");
            }
            let old: HashMap<&str, &Stage> =
                base.stages.iter().map(|s| (s.name.as_str(), s)).collect();
            for stage in &report.stages {
                let Some(before) = old.get(stage.name.as_str()) else {
                    println!("  {:<34} new", stage.name);
                    continue;
                };
                let ratio = stage.median_us / before.median_us.max(1e-9);
                let lost = stage.median_us - before.median_us;
                let checked = same_machine && !stage.name.starts_with("world.");
                let flag = if checked && ratio > 1.0 + args.tolerance && lost > NOISE_FLOOR_US {
                    problems += 1;
                    "SLOWER"
                } else if checked && ratio < 1.0 - args.tolerance && -lost > NOISE_FLOOR_US {
                    "faster"
                } else {
                    ""
                };
                println!(
                    "  {:<34} {:>10} -> {:>10}  {:>+6.1} %  {flag}",
                    stage.name,
                    fmt_us(before.median_us),
                    fmt_us(stage.median_us),
                    (ratio - 1.0) * 100.0
                );
            }
            for (name, &value) in report.counts.iter().filter(|_| same_chunks) {
                match base.counts.get(name) {
                    Some(&before) if before != value => {
                        problems += 1;
                        println!("  {name:<34} {before} -> {value}  CHANGED");
                    }
                    None => println!("  {name:<34} new: {value}"),
                    _ => {}
                }
            }
        }
    }

    println!("\nAnomalies");
    let mut unusual = 0;
    for stage in &report.stages {
        if stage.n >= 20 && stage.max_us > 20.0 * stage.median_us && stage.max_us > 1000.0 {
            unusual += 1;
            println!(
                "  {}: outlier {} at median {}",
                stage.name,
                fmt_us(stage.max_us),
                fmt_us(stage.median_us)
            );
        }
    }
    let parallel: Vec<&Stage> = report
        .stages
        .iter()
        .filter(|s| s.name.starts_with("mesh.parallel_"))
        .collect();
    if let (Some(first), Some(last)) = (parallel.first(), parallel.last()) {
        let threads = report.machine.threads as f64;
        let efficiency = first.median_us / last.median_us / threads;
        if efficiency < 0.33 {
            unusual += 1;
            println!(
                "  Meshing scales poorly: {:.0} % efficiency on {threads} threads",
                efficiency * 100.0
            );
        }
    }
    if unusual == 0 {
        println!("  none");
    }

    if args.save {
        std::fs::create_dir_all(Path::new(BASELINE).parent().expect("has a dir"))?;
        std::fs::write(BASELINE, json)?;
        println!("\nSaved as the baseline: {BASELINE}");
    } else if problems > 0 {
        println!("\n{problems} regression(s) against the baseline.");
        std::process::exit(1);
    } else if baseline.is_some() {
        println!("\nNo regression against the baseline.");
    }
    Ok(())
}
