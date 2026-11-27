//! Headless timing of the client's per-chunk and per-frame CPU work on real

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use melon_net::singleplayer::{self, server};
use melon_net::{Identity, WorldEvent};
use melon_world::block_models::BlockModels;
use melon_world::block_tint::TintSettings;
use melon_world::chunk::Chunk;
use melon_world::culling::occlusion::{self, SectionState};
use melon_world::culling::visgraph::VisibilitySet;
use melon_world::fluid::FluidSprites;
use melon_world::terrain::{self, MeshContext, Neighborhood};

fn main() -> anyhow::Result<()> {
    melon_base::platform::drop_cargo_stack();
    let dir = std::env::args().nth(1).expect("save dir");
    let assets = melon_assets::assets::Assets::discover()?;

    let t = Instant::now();
    let (models, _atlas) = BlockModels::bake(assets.root(), Vec::new(), 16384);
    println!("BlockModels::bake: {:?}", t.elapsed());
    let s = |id: &str| models.sprite(id).unwrap_or([0.0; 4]);
    let fluids = FluidSprites {
        water_still: s(FluidSprites::TEXTURES[0]),
        water_flow: s(FluidSprites::TEXTURES[1]),
        water_overlay: s(FluidSprites::TEXTURES[2]),
        lava_still: s(FluidSprites::TEXTURES[3]),
        lava_flow: s(FluidSprites::TEXTURES[4]),
    };

    let tabs = &*melon_world::creative_tabs::TABS;
    let search = tabs
        .iter()
        .find(|t| t.kind == melon_world::creative_tabs::TabType::Search)
        .unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut icon_times = Vec::new();
    for stack in search
        .items
        .iter()
        .filter(|s| seen.insert(s.item.id))
        .take(300)
    {
        let t = Instant::now();
        std::hint::black_box(melon_gui::hud::item_icon::bake(
            assets.root(),
            stack.item.registry_key,
        ));
        icon_times.push(t.elapsed());
    }
    icon_times.sort();
    let sum: Duration = icon_times.iter().sum();
    println!(
        "icon bake: n={} avg={:?} median={:?} max={:?} total={:?}",
        icon_times.len(),
        sum / icon_times.len() as u32,
        icon_times[icon_times.len() / 2],
        icon_times[icon_times.len() - 1],
        sum
    );

    let t = Instant::now();
    let (server, pipe) = server::start(server::Options {
        save_path: dir.into(),
        game_dir: PathBuf::from("."),
        settings: singleplayer::WorldSettings {
            name: "perf".into(),
            seed: "12345".into(),
            ..Default::default()
        },
        view_distance: 8,
        simulation_distance: 8,
        host: melon_net::singleplayer::server::Host {
            uuid: Identity::Offline {
                username: "Perf".into(),
            }
            .uuid(),
            name: "Perf".into(),
            online: false,
        },
    })?;
    let mut session = melon_net::join_stream(
        pipe,
        &Identity::Offline {
            username: "Perf".into(),
        },
        &Default::default(),
        Box::new(|| {}),
    )?;
    println!("server start + join: {:?}", t.elapsed());

    let t = Instant::now();
    let mut chunks: HashMap<(i32, i32), Chunk> = HashMap::new();
    let mut events = 0usize;
    while t.elapsed() < Duration::from_secs(90) && chunks.len() < 289 {
        for event in session.poll() {
            events += 1;
            if let WorldEvent::ChunkLoaded(c) = event {
                chunks.insert((c.x, c.z), c);
            }
        }
        if let Some(e) = session.ended() {
            anyhow::bail!("ended: {e}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!(
        "received {} chunks ({events} events) in {:?}",
        chunks.len(),
        t.elapsed()
    );

    let (mut sections, mut uniform, mut heap) = (0usize, 0usize, 0usize);
    for chunk in chunks.values() {
        for section in &chunk.sections {
            sections += 1;
            uniform += usize::from(section.uniform().is_some());
            heap += section.heap_bytes();
        }
    }
    println!(
        "block states: {sections} sections, {uniform} uniform, {:.1} MiB (flat: {:.1} MiB)",
        heap as f64 / 1048576.0,
        (sections * 8192) as f64 / 1048576.0
    );

    let plains_tints = TintSettings::new(
        &session.info.biomes,
        melon_world::colormap::ColorMaps::cached(),
        melon_settings::options::BIOME_BLEND_DEFAULT,
        std::env::var_os("MELON_PERF_NO_ZOOM")
            .is_none()
            .then_some(session.info.hashed_seed),
    );
    let context = MeshContext {
        smooth_lighting: true,
        cutout_leaves: true,
        models: &models,
        fluids: &fluids,
        min_y: -64,
        cardinal: [1.0; 6],
        tints: &plains_tints,
    };
    let mut times = Vec::new();
    let mut vis_times = Vec::new();
    let mut scan_times = Vec::new();
    let (mut solid_v, mut cutout_v, mut trans_v) = (0usize, 0usize, 0usize);
    let mut max_v = 0usize;
    let mut gpu_bytes = 0usize;
    let mut visibility: HashMap<(i32, i32), Vec<VisibilitySet>> = HashMap::new();
    for (&key, chunk) in &chunks {
        let mut n = Neighborhood::alone(chunk);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if (dx, dz) != (0, 0) {
                    n.chunks[((dx + 1) + (dz + 1) * 3) as usize] =
                        chunks.get(&(key.0 + dx, key.1 + dz));
                }
            }
        }
        let t = Instant::now();
        let mesh = terrain::mesh_chunk(&n, &context);
        times.push(t.elapsed());
        let t = Instant::now();
        let v = terrain::section_visibility(chunk);
        vis_times.push(t.elapsed());
        visibility.insert(key, v);
        let t = Instant::now();
        let mut found = 0;
        let kinds = &*melon_world::block_entity::KIND_STATES;
        for section in &chunk.sections {
            for id in section.iter() {
                if kinds.contains(id) {
                    found += 1;
                }
            }
        }
        std::hint::black_box(found);
        scan_times.push(t.elapsed());
        let [solid, cutout, translucent] = mesh.iter().fold([0; 3], |sum, section| {
            let counts = section.vertex_counts();
            std::array::from_fn(|i| sum[i] + counts[i])
        });
        solid_v += solid;
        cutout_v += cutout;
        trans_v += translucent;
        max_v = max_v.max(solid + cutout + translucent);
        gpu_bytes += mesh.iter().map(|section| section.bytes()).sum::<usize>();
    }
    let stats = |name: &str, v: &mut Vec<Duration>| {
        v.sort();
        let sum: Duration = v.iter().sum();
        println!(
            "{name}: n={} avg={:?} median={:?} max={:?} total={:?}",
            v.len(),
            sum / v.len().max(1) as u32,
            v[v.len() / 2],
            v[v.len() - 1],
            sum
        );
    };
    stats("mesh_chunk", &mut times);
    stats("section_visibility", &mut vis_times);
    stats("ChunkScan (connection thread)", &mut scan_times);

    // The section decode kernels alone, on the byte tier `MELON_SIMD` caps.
    let sections: Vec<_> = chunks.values().flat_map(|c| &c.sections).collect();
    let packed: Vec<_> = sections
        .iter()
        .filter(|s| s.packed_bits().is_some())
        .collect();
    let mut by_bits = [0usize; 9];
    for s in &packed {
        by_bits[usize::from(s.packed_bits().unwrap())] += 1;
    }
    let flat = sections.len() - packed.len() - uniform;
    println!(
        "byte tier {}: packed sections by bits 1={} 2={} 4={} 8={}, flat={flat}",
        melon_core::math::simd::byte_tier().name(),
        by_bits[1],
        by_bits[2],
        by_bits[4],
        by_bits[8],
    );
    let reps = 50u32;
    let per = |t: Instant| t.elapsed() / (reps * packed.len() as u32);
    let mut blocks = Box::new([0u16; melon_world::section::BLOCKS_PER_SECTION]);
    let t = Instant::now();
    for _ in 0..reps {
        for s in &packed {
            s.copy_to(&mut blocks);
            std::hint::black_box(&blocks);
        }
    }
    let decode = per(t);
    let t = Instant::now();
    for _ in 0..reps {
        for s in &packed {
            std::hint::black_box(s.flag_rows::<3>(|id| {
                let f = models.mesh_flags(id);
                u8::from(f.solid()) | u8::from(f.draws()) << 1 | u8::from(f.buried_hidden()) << 2
            }));
        }
    }
    let rows = per(t);
    println!("per packed section: copy_to={decode:?} flag_rows::<3>={rows:?}");

    // Where meshing goes: the visibility graph alone, and meshing again
    // with flat light, against the smooth `mesh_chunk` above.
    let solids: Vec<[u16; 256]> = packed
        .iter()
        .map(|s| s.flag_rows::<1>(|id| u8::from(models.mesh_flags(id).solid()))[0])
        .collect();
    let t = Instant::now();
    for _ in 0..reps {
        for solid in &solids {
            std::hint::black_box(melon_world::culling::visgraph::compute_rows(solid));
        }
    }
    println!("per packed section: visgraph::compute_rows={:?}", per(t));
    let flat_context = MeshContext {
        smooth_lighting: false,
        cutout_leaves: true,
        ..context
    };
    let mut flat_times = Vec::new();
    for (&key, chunk) in &chunks {
        let mut n = Neighborhood::alone(chunk);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if (dx, dz) != (0, 0) {
                    n.chunks[((dx + 1) + (dz + 1) * 3) as usize] =
                        chunks.get(&(key.0 + dx, key.1 + dz));
                }
            }
        }
        let t = Instant::now();
        std::hint::black_box(terrain::mesh_chunk(&n, &flat_context));
        flat_times.push(t.elapsed());
    }
    stats("mesh_chunk (flat light)", &mut flat_times);

    // Tints alone: one sampler per section, one lookup per tinted quad.
    let mut tinted = Vec::new();
    for (&key, chunk) in &chunks {
        let mut n = Neighborhood::alone(chunk);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if (dx, dz) != (0, 0) {
                    n.chunks[((dx + 1) + (dz + 1) * 3) as usize] =
                        chunks.get(&(key.0 + dx, key.1 + dz));
                }
            }
        }
        let mut by_section = Vec::new();
        for (s, section) in chunk.sections.iter().enumerate() {
            let mut lookups = Vec::new();
            for i in 0..4096usize {
                let id = section.get(i);
                let (x, y, z) = ((i & 15) as i32, (i >> 8) as i32, ((i >> 4) & 15) as i32);
                let pos = [chunk.x * 16 + x, -64 + s as i32 * 16 + y, chunk.z * 16 + z];
                models.quads_at(id, pos, |quads| {
                    for quad in quads {
                        if quad.tint != melon_world::block_tint::TintKind::None {
                            lookups.push((quad.tint, pos));
                        }
                    }
                });
            }
            if !lookups.is_empty() {
                by_section.push(lookups);
            }
        }
        tinted.push((n, by_section));
    }
    let quads: usize = tinted.iter().flat_map(|c| &c.1).map(Vec::len).sum();
    for blend in [0u8, 2, 7] {
        let settings = TintSettings::new(
            &session.info.biomes,
            melon_world::colormap::ColorMaps::cached(),
            blend,
            std::env::var_os("MELON_PERF_NO_ZOOM")
                .is_none()
                .then_some(session.info.hashed_seed),
        );
        let mut tint_times = Vec::new();
        for (n, by_section) in &tinted {
            let t = Instant::now();
            for lookups in by_section {
                let mut sampler = settings.sampler(|qx, qy, qz| n.biome_at(qx * 4, qy * 4, qz * 4));
                for &(kind, pos) in lookups {
                    std::hint::black_box(sampler.color(kind, pos));
                }
            }
            tint_times.push(t.elapsed());
        }
        stats(
            &format!("tints per chunk (blend {blend}, {quads} quads)"),
            &mut tint_times,
        );
    }
    println!(
        "vertices: solid={solid_v} cutout={cutout_v} translucent={trans_v} max/chunk={max_v} gpu={:.1} MiB",
        gpu_bytes as f64 / (1024.0 * 1024.0)
    );

    let mut by_block: HashMap<u16, usize> = HashMap::new();
    let mut nocull = 0usize;
    for (&key, chunk) in &chunks {
        let mut n = Neighborhood::alone(chunk);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if (dx, dz) != (0, 0) {
                    n.chunks[((dx + 1) + (dz + 1) * 3) as usize] =
                        chunks.get(&(key.0 + dx, key.1 + dz));
                }
            }
        }
        for (s, section) in chunk.sections.iter().enumerate() {
            for i in 0..4096usize {
                let id = section.get(i);
                let (x, y, z) = ((i & 15) as i32, (i >> 8) as i32, ((i >> 4) & 15) as i32);
                let pos = [chunk.x * 16 + x, s as i32 * 16 + y, chunk.z * 16 + z];
                models.quads_at(id, pos, |quads| {
                    for quad in quads {
                        let hidden = quad.cullface.is_some_and(|side| {
                            let [dx, dy, dz] = side.step();
                            n.block(s, x + dx, y + dy, z + dz)
                                .is_some_and(melon_world::block::state_flags::is_solid)
                        });
                        if !hidden {
                            *by_block.entry(id).or_default() += 1;
                            if quad.cullface.is_none() {
                                nocull += 1;
                            }
                        }
                    }
                });
            }
        }
    }
    let mut top: Vec<_> = by_block.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    let total: usize = top.iter().map(|e| e.1).sum();
    println!("quads total={total} without cullface={nocull}");
    for (id, n) in top.iter().take(15) {
        let name = pumpkin_data::BlockStateId::new(*id)
            .map(|s| pumpkin_data::Block::from_state_id(s).name);
        println!(
            "  {:>8} {:5.1}%  state {id} {:?}",
            n,
            *n as f64 * 100.0 / total as f64,
            name
        );
    }

    let height = 24;
    let mut walk_times = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        let mask = occlusion::walk((0, 4, 0), height, 8, None, |(x, y, z)| {
            match visibility.get(&(x, z)) {
                None => SectionState::Missing,
                Some(v) => {
                    SectionState::Loaded(v.get(y as usize).copied().unwrap_or(VisibilitySet::ALL))
                }
            }
        });
        std::hint::black_box(&mask);
        walk_times.push(t.elapsed());
    }
    stats("occlusion::walk (r=8)", &mut walk_times);

    drop(session);
    drop(server);
    Ok(())
}
