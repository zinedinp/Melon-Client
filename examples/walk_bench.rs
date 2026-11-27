//! Times the occlusion walk on real terrain at a high view distance.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glam::DVec3;
use melon_net::{self, ClientInfo, Identity, WorldEvent};
use melon_net::singleplayer::{self, server};
use melon_world::culling::occlusion::{self, Column, Eye, SectionGrid, WalkJob};
use melon_world::culling::visgraph::VisibilitySet;
use melon_world::terrain;

const VIEW_DISTANCE: i32 = 32;
const HEIGHT: i32 = 24;
const MIN_Y: i32 = -64;

type Sets = HashMap<(i32, i32), Vec<VisibilitySet>>;

fn main() -> anyhow::Result<()> {
    melon_base::platform::drop_cargo_stack();
    let dir = std::env::args().nth(1).expect("save dir");
    let cache = std::env::args()
        .nth(2)
        .unwrap_or_else(|| format!("{dir}/walk_sets.bin"));
    let sets = match read_cache(Path::new(&cache)) {
        Some(sets) => sets,
        None => {
            let sets = gather(&dir)?;
            write_cache(Path::new(&cache), &sets)?;
            sets
        }
    };
    println!("{} chunks", sets.len());

    let cameras = [
        ("surface y=80", DVec3::new(8.5, 80.0, 8.5)),
        ("air y=200", DVec3::new(8.5, 200.0, 8.5)),
        ("underground y=0", DVec3::new(8.5, 0.0, 8.5)),
        ("off-centre", DVec3::new(100.3, 95.7, -37.2)),
        ("hillside", DVec3::new(-203.9, 71.2, 155.6)),
    ];
    let runs = 15;
    for (name, eye) in cameras {
        let section = (
            (eye.x as i32) >> 4,
            ((eye.y as i32) - MIN_Y) >> 4,
            (eye.z as i32) >> 4,
        );
        for smart in [false, true] {
            let mut grid_times = Vec::new();
            let mut walk_times = Vec::new();
            let mut reached = 0;
            let mut hash = 0;
            for _ in 0..runs {
                let t = Instant::now();
                let grid =
                    SectionGrid::new((section.0, section.2), VIEW_DISTANCE, HEIGHT, |x, z| {
                        match sets.get(&(x, z)) {
                            None => Column::Missing,
                            Some(sets) => Column::Sets(sets),
                        }
                    });
                grid_times.push(t.elapsed());
                let eye = smart.then_some(Eye {
                    position: eye,
                    min_y: MIN_Y,
                });
                let job = WalkJob::new(section, HEIGHT, VIEW_DISTANCE, eye, grid);
                let t = Instant::now();
                let mask = job.run();
                walk_times.push(t.elapsed());
                reached = mask.count();
                hash = mask_hash(&mask, section);
            }
            println!(
                "{name:<16} smart={:<5} reached={reached:>6} hash={hash:016x}  grid {}  walk {}",
                smart,
                summary(&mut grid_times),
                summary(&mut walk_times),
            );
        }
    }
    Ok(())
}

/// FNV-1a over which sections the mask holds, to tell two walks apart.
fn mask_hash(mask: &occlusion::OcclusionMask, camera: (i32, i32, i32)) -> u64 {
    let r = VIEW_DISTANCE + 1;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for y in 0..HEIGHT {
        for z in camera.2 - r..=camera.2 + r {
            for x in camera.0 - r..=camera.0 + r {
                hash ^= u64::from(mask.contains((x, y, z)));
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    hash
}

/// Median and fastest, in ms.
fn summary(times: &mut [Duration]) -> String {
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    format!(
        "median {:6.3} ms  min {:6.3} ms",
        ms(times[times.len() / 2]),
        ms(times[0])
    )
}

fn gather(dir: &str) -> anyhow::Result<Sets> {
    let (server, pipe) = server::start(server::Options {
        save_path: dir.into(),
        game_dir: PathBuf::from("."),
        settings: singleplayer::WorldSettings {
            name: "walk".into(),
            seed: "melon-bench".into(),
            ..Default::default()
        },
        view_distance: VIEW_DISTANCE as u8,
        simulation_distance: 8,
        host: melon_net::singleplayer::server::Host {
            uuid: Identity::Offline {
                username: "Walk".into(),
            }
            .uuid(),
            name: "Walk".into(),
            online: false,
        },
    })?;
    let client = ClientInfo {
        view_distance: VIEW_DISTANCE as u8,
        ..Default::default()
    };
    let mut session = network::join_stream(
        pipe,
        &Identity::Offline {
            username: "Walk".into(),
        },
        &client,
        Box::new(|| {}),
    )?;
    let t = Instant::now();
    let mut sets = Sets::new();
    let mut last_new = Instant::now();
    while t.elapsed() < Duration::from_secs(900) && last_new.elapsed() < Duration::from_secs(20) {
        for event in session.poll() {
            if let WorldEvent::ChunkLoaded(c) = event {
                sets.insert((c.x, c.z), terrain::section_visibility(&c));
                last_new = Instant::now();
            }
        }
        if let Some(e) = session.ended() {
            anyhow::bail!("ended: {e}");
        }
        std::thread::sleep(Duration::from_millis(5));
        if sets.len().is_multiple_of(500) && last_new.elapsed() < Duration::from_millis(6) {
            println!("{} chunks after {:?}", sets.len(), t.elapsed());
        }
    }
    println!("gathered {} chunks in {:?}", sets.len(), t.elapsed());
    drop(session);
    drop(server);
    Ok(sets)
}

fn write_cache(path: &Path, sets: &Sets) -> anyhow::Result<()> {
    let mut out = Vec::new();
    for (&(x, z), column) in sets {
        out.extend(x.to_le_bytes());
        out.extend(z.to_le_bytes());
        out.extend((column.len() as u32).to_le_bytes());
        for set in column {
            out.extend(set.bits().to_le_bytes());
        }
    }
    std::fs::File::create(path)?.write_all(&out)?;
    Ok(())
}

fn read_cache(path: &Path) -> Option<Sets> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .read_to_end(&mut bytes)
        .ok()?;
    let mut sets = Sets::new();
    let mut at = 0;
    let mut take = |n: usize| {
        let slice = bytes.get(at..at + n);
        at += n;
        slice
    };
    while let Some(head) = take(12) {
        let x = i32::from_le_bytes(head[0..4].try_into().ok()?);
        let z = i32::from_le_bytes(head[4..8].try_into().ok()?);
        let n = u32::from_le_bytes(head[8..12].try_into().ok()?) as usize;
        let mut column = Vec::with_capacity(n);
        for _ in 0..n {
            let bits = u64::from_le_bytes(take(8)?.try_into().ok()?);
            column.push(VisibilitySet::from_bits(bits));
        }
        sets.insert((x, z), column);
    }
    Some(sets)
}
