//! The mesh worker threads (vanilla `SectionRenderDispatcher`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use glam::Vec3;

use crate::math::SECTION_SIZE;
use crate::world::block_models::BlockModels;
use crate::world::block_tint::TintSettings;
use crate::world::chunk::Chunk;
use crate::world::culling::occlusion::SectionPos;
use crate::world::fluid::FluidSprites;
use crate::world::terrain::{self, MeshContext, Neighborhood, SectionMesh};

/// A section to mesh, as of `version`.
pub(super) struct Job {
    pub pos: SectionPos,
    pub version: u64,
    /// The section's chunk at the centre (index 4) and the eight around it.
    pub chunks: [Option<Arc<Chunk>>; 9],
    /// The "Smooth Lighting" option.
    pub smooth_lighting: bool,
    /// The dimension's face shading, relative to the default.
    pub cardinal: [f32; 6],
    /// Where the camera was, to sort the translucent faces from.
    pub camera: Vec3,
    /// Biome colours and the blend radius.
    pub tints: TintSettings,
}

/// A [`Job`] for [`Mesher::mesh_now`], and where its mesh goes.
type Urgent = (Job, Sender<Option<Done>>);

pub(super) struct Done {
    pub pos: SectionPos,
    pub version: u64,
    pub mesh: SectionMesh,
}

/// Meshes section `s` of the chunk at the centre of `chunks`.
pub(super) fn mesh(
    chunks: &[Option<Arc<Chunk>>; 9],
    (x, s, z): SectionPos,
    context: &MeshContext,
    camera: Vec3,
) -> Option<SectionMesh> {
    let center = chunks[4].as_deref()?;
    let context = &MeshContext {
        min_y: center.min_y(),
        ..*context
    };
    let mut neighborhood = Neighborhood::alone(center);
    for (slot, chunk) in neighborhood.chunks.iter_mut().zip(chunks) {
        *slot = chunk.as_deref();
    }
    let mut mesh = terrain::mesh_section(&neighborhood, usize::try_from(s).ok()?, context);
    let corner = Vec3::new((x * SECTION_SIZE) as f32, 0.0, (z * SECTION_SIZE) as f32);
    mesh.translucent.sort(camera - corner);
    Some(mesh)
}

/// Half the hardware threads, at most eight.
pub(super) struct Mesher {
    jobs: Option<Sender<Job>>,
    results: Receiver<Done>,
    /// [`Mesher::mesh_now`]'s jobs, taken by the helpers and the caller.
    urgent: Option<Sender<Urgent>>,
    urgent_rx: Receiver<Urgent>,
    models: Arc<BlockModels>,
    fluids: FluidSprites,
    workers: Vec<JoinHandle<()>>,
    /// Set when the mesher is dropped.
    stop: Arc<AtomicBool>,
    pub in_flight: usize,
    /// Jobs handed out at a time.
    pub limit: usize,
}

impl Mesher {
    pub fn new(models: Arc<BlockModels>, fluids: FluidSprites) -> Self {
        let count = std::thread::available_parallelism()
            .map_or(2, |n| n.get() / 2)
            .clamp(1, 8);
        let (jobs, job_rx) = crossbeam_channel::unbounded::<Job>();
        let (done_tx, results) = crossbeam_channel::unbounded();
        let (urgent, urgent_rx) = crossbeam_channel::unbounded::<Urgent>();
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = (0..count)
            .filter_map(|i| {
                let job_rx = job_rx.clone();
                let stop = Arc::clone(&stop);
                let done_tx = done_tx.clone();
                let models = Arc::clone(&models);
                std::thread::Builder::new()
                    .name(format!("chunk-mesher-{i}"))
                    .spawn(move || work(&job_rx, &done_tx, &models, fluids, &stop))
                    .ok()
            })
            .collect::<Vec<_>>();
        let limit = workers.len() * 32;
        // The caller meshes too, so one thread fewer than there are cores.
        let helpers = std::thread::available_parallelism()
            .map_or(1, |n| n.get() - 1)
            .clamp(1, 7);
        workers.extend((0..helpers).filter_map(|i| {
            let urgent_rx = urgent_rx.clone();
            let models = Arc::clone(&models);
            std::thread::Builder::new()
                .name(format!("chunk-mesher-now-{i}"))
                .spawn(move || {
                    // The frame waits for these.
                    crate::platform::raise_frame_thread_priority();
                    for (job, reply) in &urgent_rx {
                        let _ = reply.send(build(&job, &models, fluids));
                    }
                })
                .ok()
        }));
        Self {
            jobs: Some(jobs),
            results,
            urgent: Some(urgent),
            urgent_rx,
            models,
            fluids,
            workers,
            stop,
            in_flight: 0,
            limit,
        }
    }

    /// Room for more jobs.
    pub fn room(&self) -> usize {
        self.limit.saturating_sub(self.in_flight)
    }

    /// Hands out a job. `false` once the workers are gone.
    pub fn send(&mut self, job: Job) -> bool {
        let Some(jobs) = &self.jobs else {
            return false;
        };
        if jobs.send(job).is_err() {
            return false;
        }
        self.in_flight += 1;
        true
    }

    /// Meshes `jobs` before returning (`compileSync`), spread over the
    /// helpers and this thread.
    pub fn mesh_now(&self, jobs: Vec<Job>) -> Vec<Done> {
        let Some(urgent) = &self.urgent else {
            return Vec::new();
        };
        let (reply, replies) = crossbeam_channel::unbounded();
        for job in jobs {
            let _ = urgent.send((job, reply.clone()));
        }
        drop(reply);
        while let Ok((job, reply)) = self.urgent_rx.try_recv() {
            let _ = reply.send(build(&job, &self.models, self.fluids));
        }
        replies.iter().flatten().collect()
    }

    /// The meshes finished since the last call.
    pub fn finished(&mut self) -> Vec<Done> {
        let done: Vec<Done> = self.results.try_iter().collect();
        self.in_flight -= done.len();
        done
    }
}

/// One worker: takes jobs until the channel closes or `stop` is set.
fn work(
    jobs: &Receiver<Job>,
    done: &Sender<Done>,
    models: &BlockModels,
    fluids: FluidSprites,
    stop: &AtomicBool,
) {
    crate::platform::lower_thread_priority();
    for job in jobs {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Some(result) = build(&job, models, fluids) else {
            continue;
        };
        if done.send(result).is_err() {
            return;
        }
    }
}

/// Meshes one job; `None` without its centre chunk.
fn build(job: &Job, models: &BlockModels, fluids: FluidSprites) -> Option<Done> {
    let context = MeshContext {
        smooth_lighting: job.smooth_lighting,
        models,
        fluids: &fluids,
        min_y: 0,
        cardinal: job.cardinal,
        tints: &job.tints,
    };
    Some(Done {
        pos: job.pos,
        version: job.version,
        mesh: mesh(&job.chunks, job.pos, &context, job.camera)?,
    })
}

impl Drop for Mesher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.jobs = None;
        self.urgent = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
