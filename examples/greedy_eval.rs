//! How much greedy meshing would save on the bench's frozen chunks: meshes

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use anyhow::Context;
use melon_world::block_models::BlockModels;
use melon_world::block_tint::TintSettings;
use melon_world::chunk::Chunk;
use melon_world::fluid::FluidSprites;
use melon_world::mesh::WorldVertex;
use melon_world::terrain::{self, MeshContext, Neighborhood};

/// What two faces must share to merge.
#[derive(Clone, Copy)]
enum Rule {
    /// Same corners' UVs (sprite and turn), colour and light: what a tiling
    Exact,
    /// As [`Rule::Exact`], light ignored: light read per block in the shader.
    NoLight,
    /// Same sprite and colour only: the shader would also redo the random
    SpriteOnly,
    /// Same sprite alone: tint per block in the shader too.
    SpriteNoColor,
}

const RULES: [(Rule, &str); 4] = [
    (Rule::Exact, "exakt (UV-Drehung, Farbe, Licht gleich)"),
    (Rule::NoLight, "ohne Licht"),
    (Rule::SpriteOnly, "nur Sprite + Farbe"),
    (Rule::SpriteNoColor, "nur Sprite"),
];

/// A full block face: its plane, its cell in the plane, and its look.
struct Face {
    plane: (usize, i32),
    cell: (i32, i32),
    keys: [u64; 4],
}

fn main() -> anyhow::Result<()> {
    melon_base::platform::drop_cargo_stack();
    let assets = melon_assets::assets::Assets::discover()?;
    let models = BlockModels::bake(assets.root(), Vec::new(), 16384).0;
    let s = |id: &str| models.sprite(id).unwrap_or([0.0; 4]);
    let fluids = FluidSprites {
        water_still: s(FluidSprites::TEXTURES[0]),
        water_flow: s(FluidSprites::TEXTURES[1]),
        water_overlay: s(FluidSprites::TEXTURES[2]),
        lava_still: s(FluidSprites::TEXTURES[3]),
        lava_flow: s(FluidSprites::TEXTURES[4]),
    };
    let plains_tints = TintSettings::plains();
    let context = MeshContext {
        smooth_lighting: true,
        cutout_leaves: true,
        models: &models,
        fluids: &fluids,
        min_y: -64,
        cardinal: [1.0; 6],
        tints: &plains_tints,
    };
    let chunks = frozen().context("run pipeline_bench once first")?;
    let by_key: HashMap<_, _> = chunks.iter().map(|c| ((c.x, c.z), c)).collect();

    let mut quads = [0u64; 3];
    let mut full = [0u64; 3];
    let mut rects = [[0u64; 4]; 3];
    for chunk in &chunks {
        let mut n = Neighborhood::alone(chunk);
        for dz in -1..=1 {
            for dx in -1..=1 {
                n.chunks[((dx + 1) + (dz + 1) * 3) as usize] =
                    by_key.get(&(chunk.x + dx, chunk.z + dz)).copied();
            }
        }
        for section in terrain::mesh_chunk(&n, &context) {
            quads[0] += (section.lava.items.len() / 4) as u64;
            quads[2] += section.translucent.quad_count() as u64;
            for (l, layer) in [&section.solid, &section.cutout].into_iter().enumerate() {
                quads[l] += layer.items.len() as u64;
                for (dir, range) in layer.groups.iter().take(6).enumerate() {
                    let faces: Vec<Face> = layer.items[range.start as usize..range.end as usize]
                        .iter()
                        .map(|&q| models.quad_vertices(q, [0, section.base_y, 0], &[1.0; 6]))
                        .filter_map(|q| face(dir, &q))
                        .collect();
                    full[l] += faces.len() as u64;
                    for (r, _) in RULES.iter().enumerate() {
                        rects[l][r] += greedy(&faces, r);
                    }
                }
            }
        }
    }

    println!("{} Chunks (eingefrorene Bench-Welt)\n", chunks.len());
    for (l, name) in ["solid", "cutout", "translucent"].iter().enumerate() {
        let other = quads[l] - full[l];
        println!(
            "{name}: {} Quads, davon {} volle Blockflächen ({:.0} %)",
            quads[l],
            full[l],
            pct(full[l], quads[l])
        );
        for (r, (_, rule)) in RULES.iter().enumerate() {
            let after = rects[l][r] + other;
            println!(
                "  {rule:<42} {:>9} Quads  ({:.1} % übrig, Faktor {:.2})",
                after,
                pct(after, quads[l]),
                quads[l] as f64 / after.max(1) as f64
            );
        }
    }
    Ok(())
}

fn pct(a: u64, b: u64) -> f64 {
    100.0 * a as f64 / b.max(1) as f64
}

/// `quad` as a full face of direction `dir`, or `None` if it isn't one
fn face(dir: usize, quad: &[WorldVertex]) -> Option<Face> {
    let normal = [1, 1, 2, 2, 0, 0][dir];
    let (a, b) = match normal {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    };
    let plane = quad[0].pos[normal];
    if plane.fract() != 0.0 || quad.iter().any(|v| v.pos[normal] != plane) {
        return None;
    }
    let min_a = quad.iter().map(|v| v.pos[a]).fold(f32::MAX, f32::min);
    let min_b = quad.iter().map(|v| v.pos[b]).fold(f32::MAX, f32::min);
    if min_a.fract() != 0.0 || min_b.fract() != 0.0 {
        return None;
    }
    let mut uvs = [[0.0_f32; 2]; 4];
    let mut seen = 0u8;
    for v in quad {
        let (da, db) = (v.pos[a] - min_a, v.pos[b] - min_b);
        let corner = match (da, db) {
            (0.0, 0.0) => 0,
            (1.0, 0.0) => 1,
            (0.0, 1.0) => 2,
            (1.0, 1.0) => 3,
            _ => return None,
        };
        seen |= 1 << corner;
        uvs[corner] = v.uv;
    }
    if seen != 0b1111 {
        return None;
    }
    let colors: Vec<[u8; 4]> = quad.iter().map(|v| v.color).collect();
    let lights: Vec<[u8; 2]> = quad.iter().map(|v| v.light).collect();
    let u_min = uvs.iter().map(|uv| uv[0]).fold(f32::MAX, f32::min);
    let v_min = uvs.iter().map(|uv| uv[1]).fold(f32::MAX, f32::min);
    let u_max = uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
    let v_max = uvs.iter().map(|uv| uv[1]).fold(f32::MIN, f32::max);
    let sprite = (u_min, v_min, u_max, v_max);
    let sprite = [sprite.0, sprite.1, sprite.2, sprite.3].map(f32::to_bits);
    let uvs = uvs.map(|uv| uv.map(f32::to_bits));
    let hash = |value: &dyn Fn(&mut DefaultHasher)| {
        let mut h = DefaultHasher::new();
        value(&mut h);
        h.finish()
    };
    Some(Face {
        plane: (dir, plane as i32),
        cell: (min_a as i32, min_b as i32),
        keys: [
            hash(&|h| (uvs, &colors, &lights).hash(h)),
            hash(&|h| (uvs, &colors).hash(h)),
            hash(&|h| (sprite, &colors).hash(h)),
            hash(&|h| sprite.hash(h)),
        ],
    })
}

/// Rectangles left after merging `faces` (one section, one facing) under
fn greedy(faces: &[Face], r: usize) -> u64 {
    type Cells = HashMap<(i32, i32), Vec<u64>>;
    let mut planes: HashMap<(usize, i32), Cells> = HashMap::new();
    for f in faces {
        planes
            .entry(f.plane)
            .or_default()
            .entry(f.cell)
            .or_default()
            .push(f.keys[r]);
    }
    let mut total = 0;
    for cells in planes.values() {
        let mut stacks: HashMap<(i32, i32), u64> = HashMap::new();
        for (&cell, keys) in cells {
            let mut keys = keys.clone();
            keys.sort_unstable();
            let mut h = DefaultHasher::new();
            keys.hash(&mut h);
            stacks.insert(cell, h.finish());
        }
        let mut left = stacks.clone();
        let mut order: Vec<_> = stacks.keys().copied().collect();
        order.sort_unstable_by_key(|&(a, b)| (b, a));
        for (a, b) in order {
            let Some(&key) = left.get(&(a, b)) else {
                continue;
            };
            let mut w = 1;
            while left.get(&(a + w, b)) == Some(&key) {
                w += 1;
            }
            let mut h = 1;
            while (0..w).all(|i| left.get(&(a + i, b + h)) == Some(&key)) {
                h += 1;
            }
            for j in 0..h {
                for i in 0..w {
                    left.remove(&(a + i, b + j));
                }
            }
            total += 1;
        }
    }
    total
}

/// The chunks `pipeline_bench` froze.
fn frozen() -> anyhow::Result<Vec<Chunk>> {
    let bytes = std::fs::read("target/pipeline-bench/chunks.bin")?;
    let mut at = 12;
    let word = |at: &mut usize| -> anyhow::Result<usize> {
        let w = bytes.get(*at..*at + 4).context("short file")?;
        *at += 4;
        Ok(u32::from_le_bytes(w.try_into()?) as usize)
    };
    let count = word(&mut at)?;
    (0..count)
        .map(|_| {
            let len = word(&mut at)?;
            let packet = bytes.get(at..at + len).context("short file")?;
            at += len;
            melon_net::play::level::chunk::decode(packet, melon_world::dimension::Height::OVERWORLD)
        })
        .collect()
}
