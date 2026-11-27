//! Bench of every pipeline the circle functions run in, alone and with

use std::hint::black_box;
use std::time::Instant;

use glam::{Mat4, Vec3};
use melon_core::math::angle::{self, Rotation};
use melon_core::math::euler::{Look, Orientation};
use melon_world::entity::humanoid::HumanoidRenderState;
use melon_world::entity::living::{self, LivingRenderState, Placement};
use melon_world::entity::mobs::{MobModel, ModelId, PoseInput};
use melon_world::entity::player::{AvatarRenderState, PlayerModel, PlayerModelType};
use melon_world::entity::pose_cache::PoseCache;
use melon_world::player::forward_vector_for;

/// Unrelated memory walked between calls: bigger than L2's share per core.
const TRAFFIC_BYTES: usize = 1 << 20;

struct Pipeline {
    name: &'static str,
    /// Calls per run.
    calls: usize,
    /// Operations per call: the small functions run [`BATCH`] times a call
    ops: usize,
    run: Box<dyn FnMut(usize) -> f32>,
}

/// Small operations per call.
const BATCH: usize = 16;

/// `f` over [`BATCH`] consecutive angles from the `k`th call's.
#[inline(always)]
fn batch(angles: &[f32], k: usize, f: impl Fn(f32) -> f32) -> f32 {
    let mut acc = 0.0;
    for j in 0..BATCH {
        acc += f(angles[(k * BATCH + j) & (angles.len() - 1)]);
    }
    acc
}

fn pipelines(angles: &'static [f32]) -> Vec<Pipeline> {
    let mobs: Vec<MobModel> = ModelId::ALL.into_iter().map(MobModel::new).collect();
    let animated: Vec<MobModel> = ModelId::ALL.into_iter().map(MobModel::new).collect();
    let with_ids = || -> Vec<(ModelId, MobModel)> {
        ModelId::ALL
            .into_iter()
            .map(|id| (id, MobModel::new(id)))
            .collect()
    };
    let (cached_walking, cached_still) = (with_ids(), with_ids());
    let (mut walking_cache, mut still_cache) = (PoseCache::default(), PoseCache::default());
    let player = PlayerModel::new(PlayerModelType::Wide);
    let walking = |k: usize| {
        let a = angles[k];
        let living = LivingRenderState {
            body_rot: a * 40.0,
            y_rot: a * 10.0,
            x_rot: a * 5.0,
            walk_position: a * 3.0 + 20.0,
            walk_speed: 0.8,
            age_in_ticks: k as f32,
            ..Default::default()
        };
        PoseInput {
            living,
            humanoid: HumanoidRenderState::from_living(&living),
            flap: (a, 0.5),
            ..Default::default()
        }
    };
    vec![
        Pipeline {
            name: "baseline (no math)",
            calls: 4_000_000,
            ops: 1,
            run: Box::new(|k| angles[k]),
        },
        Pipeline {
            name: "sin",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| batch(angles, k, angle::sin)),
        },
        Pipeline {
            name: "cos",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| batch(angles, k, angle::cos)),
        },
        Pipeline {
            name: "sin_cos",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    let (s, c) = angle::sin_cos(a);
                    s + c
                })
            }),
        },
        Pipeline {
            name: "sin_cos_exact (JOML)",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    let (s, c) = angle::sin_cos_exact(a);
                    s + c
                })
            }),
        },
        Pipeline {
            name: "sin_cos_exact_slice (16)",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                let mut x = [0.0f32; BATCH];
                for (j, v) in x.iter_mut().enumerate() {
                    *v = angles[(k * BATCH + j) & (angles.len() - 1)];
                }
                let (mut s, mut c) = ([0.0f32; BATCH], [0.0f32; BATCH]);
                angle::sin_cos_exact_slice(&x, &mut s, &mut c);
                s.iter().zip(&c).map(|(s, c)| s + c).sum()
            }),
        },
        Pipeline {
            name: "libm sin + cos (f64)",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    let a = f64::from(a);
                    (a.sin() + a.cos()) as f32
                })
            }),
        },
        Pipeline {
            name: "movement sin_cos_deg",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    let (s, c) = angle::sin_cos_deg(a * 40.0 + 180.0);
                    s * 0.98 + c * 0.02
                })
            }),
        },
        Pipeline {
            name: "rotation rot_x * rot_y * rot_z",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    (Mat4::rot_x(a) * Mat4::rot_y_deg(a * 30.0) * Mat4::rot_z(-a))
                        .x_axis
                        .y
                })
            }),
        },
        Pipeline {
            name: "rotation euler_zyx",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    Mat4::euler_zyx(Vec3::new(a, -a * 0.5, a * 2.0)).y_axis.z
                })
            }),
        },
        Pipeline {
            name: "camera orientation + look",
            calls: 400_000,
            ops: BATCH,
            run: Box::new(|k| {
                batch(angles, k, |a| {
                    let (yaw, pitch) = (a * 40.0, a * 8.0);
                    Orientation::new(Look::from_melon(yaw, pitch)).view.z_axis.x
                        + forward_vector_for(yaw, pitch).y
                })
            }),
        },
        Pipeline {
            name: "mob pose, every model",
            calls: 4_000,
            ops: 1,
            run: Box::new(move |k| {
                let input = walking(k);
                let root = living::root_transform(&input.living, Placement::default());
                mobs.iter()
                    .map(|mob| {
                        let pose = mob.pose(&input);
                        let matrices = mob.model.part_matrices(&pose, root);
                        matrices.iter().flatten().map(|m| m.w_axis.x).sum::<f32>()
                    })
                    .sum()
            }),
        },
        Pipeline {
            name: "mob pose, every model, cache: walking",
            calls: 4_000,
            ops: 1,
            run: Box::new(move |k| {
                let input = walking(k);
                let root = living::root_transform(&input.living, Placement::default());
                cached_walking
                    .iter()
                    .enumerate()
                    .map(|(entity, (id, mob))| {
                        let pose = mob.pose(&input);
                        let matrices =
                            walking_cache.matrices(entity as i32, *id, &mob.model, &pose, root);
                        matrices.iter().flatten().map(|m| m.w_axis.x).sum::<f32>()
                    })
                    .sum()
            }),
        },
        Pipeline {
            name: "mob pose, every model, cache: standing",
            calls: 4_000,
            ops: 1,
            run: Box::new(move |_| {
                let input = walking(0);
                let root = living::root_transform(&input.living, Placement::default());
                cached_still
                    .iter()
                    .enumerate()
                    .map(|(entity, (id, mob))| {
                        let pose = mob.pose(&input);
                        let matrices =
                            still_cache.matrices(entity as i32, *id, &mob.model, &pose, root);
                        matrices.iter().flatten().map(|m| m.w_axis.x).sum::<f32>()
                    })
                    .sum()
            }),
        },
        Pipeline {
            name: "mob pose, animation only",
            calls: 4_000,
            ops: 1,
            run: Box::new(move |k| {
                let input = walking(k);
                animated
                    .iter()
                    .map(|mob| mob.pose(&input)[mob.model.root()].pose.offset.y)
                    .sum()
            }),
        },
        Pipeline {
            name: "player pose",
            calls: 40_000,
            ops: 1,
            run: Box::new(move |k| {
                let input = walking(k);
                let state = AvatarRenderState {
                    humanoid: input.humanoid,
                    body_yaw: input.living.body_rot,
                    ..Default::default()
                };
                let pose = player.pose(&state);
                let matrices = player
                    .model
                    .part_matrices(&pose, PlayerModel::root_transform(&state));
                matrices.iter().flatten().map(|m| m.w_axis.y).sum()
            }),
        },
    ]
}

/// One run: ns per call.
fn time(pipeline: &mut Pipeline, traffic: Option<&[u64]>, angles: usize) -> f64 {
    let mut acc = 0.0f32;
    let mut at = 0usize;
    let start = Instant::now();
    for i in 0..pipeline.calls {
        if let Some(junk) = traffic
            && i % 4 == 0
        {
            for _ in 0..48 {
                at = (at + 521) & (junk.len() - 1);
                acc += junk[at] as f32;
            }
        }
        acc += (pipeline.run)(black_box(i % angles));
    }
    black_box(acc);
    start.elapsed().as_secs_f64() * 1e9 / pipeline.calls as f64
}

/// The sine kernel on each tier the CPU has, in ns per angle, over
/// `angles` at a time (a model's pose hands it a handful).
fn kernel_tiers() {
    use melon_core::math::simd::Tier;
    use melon_core::math::sin_cos::sin_cos_on;
    for angles in [4usize, 8, 16, 64, 1024] {
        let x: Vec<f32> = (0..angles).map(|k| (k as f32 * 0.37).sin() * 3.0).collect();
        let (mut s, mut c) = (vec![0.0f32; angles], vec![0.0f32; angles]);
        let rounds = 4_000_000 / angles;
        for tier in Tier::available() {
            let t = Instant::now();
            for _ in 0..rounds {
                sin_cos_on(tier, black_box(&x), &mut s, &mut c);
                black_box((&s, &c));
            }
            let per_angle = t.elapsed().as_nanos() as f64 / (rounds * angles) as f64;
            println!(
                "{:<9} {angles:>5} angles: {per_angle:5.2} ns/angle",
                tier.name()
            );
        }
    }
}

fn main() {
    melon_base::platform::drop_cargo_stack();
    if std::env::var_os("MELON_BENCH_TIERS").is_some() {
        return kernel_tiers();
    }
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let only = value("--only");
    let runs: usize = value("--runs").and_then(|r| r.parse().ok()).unwrap_or(15);
    let quiet = args.iter().any(|a| a == "--quiet");

    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let angles: &'static [f32] = Vec::leak(
        (0..4096)
            .map(|_| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                ((seed >> 40) as f32 / 16_777_216.0 - 0.5) * 50.0
            })
            .collect(),
    );
    let junk: Vec<u64> = (0..(TRAFFIC_BYTES / 8) as u64).collect();
    let mut pipelines: Vec<Pipeline> = pipelines(angles)
        .into_iter()
        .filter(|p| {
            p.name.starts_with("baseline") || only.as_deref().is_none_or(|o| p.name.contains(o))
        })
        .collect();

    let mut alone = vec![Vec::with_capacity(runs); pipelines.len()];
    let mut busy = vec![Vec::with_capacity(runs); pipelines.len()];
    for _ in 0..runs {
        for (i, pipeline) in pipelines.iter_mut().enumerate() {
            alone[i].push(time(pipeline, None, angles.len()));
            busy[i].push(time(pipeline, Some(&junk), angles.len()));
        }
    }
    if quiet {
        return;
    }
    let base = pipelines
        .iter()
        .position(|p| p.name.starts_with("baseline"))
        .map(|i| (summary(&mut alone[i]).0, summary(&mut busy[i]).0));
    let (harness, harness_busy) = base.unwrap_or((0.0, 0.0));
    println!(
        "{:32} {:>30}   {:>12}   {:>16}",
        "pipeline (ns per operation)", "alone: median [min..max]", "traffic", "misses add"
    );
    for (i, pipeline) in pipelines.iter().enumerate() {
        if pipeline.name.starts_with("baseline") {
            continue;
        }
        let ops = pipeline.ops as f64;
        let (a, lo, hi) = summary(&mut alone[i]);
        let (b, _, _) = summary(&mut busy[i]);
        let per = |call: f64| (call - harness) / ops;
        let added = (b - a - (harness_busy - harness)) / ops;
        println!(
            "{:32} {:10.2} [{:8.2}..{:8.2}]   {:12.2}   {:+8.2} ns {:+4.0}%",
            pipeline.name,
            per(a),
            per(lo),
            per(hi),
            (b - harness_busy) / ops,
            added,
            added / per(a) * 100.0
        );
    }
    println!(
        "\nThe harness's own {harness:.2} ns a call (and {:.2} ns with the traffic) are taken off.\n\
         misses add: how much slower the operation got with the traffic evicting its lines.",
        harness_busy
    );
}

/// Median, fastest and slowest.
fn summary(runs: &mut [f64]) -> (f64, f64, f64) {
    runs.sort_by(f64::total_cmp);
    (runs[runs.len() / 2], runs[0], runs[runs.len() - 1])
}
