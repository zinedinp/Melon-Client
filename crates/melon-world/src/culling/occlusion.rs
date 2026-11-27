//! Vanilla `SectionOcclusionGraph` (cave culling).

use glam::DVec3;

use super::visgraph::VisibilitySet;
use crate::block::direction::{Direction, Directions};
use crate::math::{SECTION_SIZE, section_coord};

/// A section position.
pub type SectionPos = (i32, i32, i32);

/// What the walk needs to know about one section.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SectionState {
    /// Its chunk hasn't arrived: the walk doesn't enter it.
    Missing,
    /// Arrived, with what can be seen through it.
    Loaded(VisibilitySet),
}

/// `ChunkTrackingView.isWithinDistance` (the one-chunk buffer variant).
#[inline]
pub fn within_view_distance(dx: i32, dz: i32, view_distance: i32) -> bool {
    let dx = i64::from((dx.abs() - 1).max(0));
    let dz = i64::from((dz.abs() - 1).max(0));
    dx * dx + dz * dz < i64::from(view_distance) * i64::from(view_distance)
}

/// What a chunk column offers the walk.
pub enum Column<'a> {
    /// Not arrived: the walk doesn't enter it.
    Missing,
    /// Arrived but not meshed yet: open, so it hides nothing.
    Open,
    /// Each section's set, bottom to top.
    Sets(&'a [VisibilitySet]),
}

/// Every section's [`SectionState`] in the window a walk around a camera covers.
pub struct SectionGrid {
    center: (i32, i32),
    radius: i32,
    height: i32,
    /// Layer after layer, bottom to top, in the [`OcclusionMask`]'s slot order.
    cells: Vec<u64>,
}

impl SectionGrid {
    /// Above any set's bits.
    const MISSING: u64 = u64::MAX;

    /// The window [`walk`] covers for `view_distance` around chunk `center`.
    pub fn new<'a>(
        center: (i32, i32),
        view_distance: i32,
        height: i32,
        mut column: impl FnMut(i32, i32) -> Column<'a>,
    ) -> Self {
        let radius = view_distance + 1;
        let side = (2 * radius + 1) as usize;
        let area = side * side;
        let h = height.max(0) as usize;
        let mut cells = vec![Self::MISSING; area * h];
        for i in 0..area {
            let (dx, dz) = ((i % side) as i32 - radius, (i / side) as i32 - radius);
            if h == 0 || !within_view_distance(dx, dz, view_distance) {
                continue;
            }
            let mut fill = |set: &dyn Fn(usize) -> u64| {
                for y in 0..h {
                    cells[y * area + i] = set(y);
                }
            };
            match column(center.0 + dx, center.1 + dz) {
                Column::Missing => {}
                Column::Open => fill(&|_| VisibilitySet::ALL.bits()),
                Column::Sets(sets) => {
                    fill(&|y| sets.get(y).copied().unwrap_or(VisibilitySet::ALL).bits())
                }
            }
        }
        Self {
            center,
            radius,
            height,
            cells,
        }
    }

    /// The window for `view_distance` around `center`, asking `section` for each state.
    pub fn from_fn(
        center: (i32, i32),
        view_distance: i32,
        height: i32,
        section: impl Fn(SectionPos) -> SectionState,
    ) -> Self {
        let mut grid = Self::new(center, view_distance, height, |_, _| Column::Missing);
        let side = 2 * grid.radius + 1;
        for y in 0..grid.height {
            for dz in 0..side {
                for dx in 0..side {
                    let (x, z) = (dx - grid.radius, dz - grid.radius);
                    if !within_view_distance(x, z, view_distance) {
                        continue;
                    }
                    let cell = &mut grid.cells[((y * side + dz) * side + dx) as usize];
                    if let SectionState::Loaded(set) = section((center.0 + x, y, center.1 + z)) {
                        *cell = set.bits();
                    }
                }
            }
        }
        grid
    }

    /// The section's state; [`SectionState::Missing`] outside the window.
    #[inline]
    pub fn state(&self, (x, y, z): SectionPos) -> SectionState {
        let side = 2 * self.radius + 1;
        let (dx, dz) = (
            x - self.center.0 + self.radius,
            z - self.center.1 + self.radius,
        );
        if !(0..side).contains(&dx) || !(0..side).contains(&dz) || !(0..self.height).contains(&y) {
            return SectionState::Missing;
        }
        let cell = self.cells[((y * side + dz) * side + dx) as usize];
        if cell == Self::MISSING {
            SectionState::Missing
        } else {
            SectionState::Loaded(VisibilitySet::from_bits(cell))
        }
    }
}

/// The sections the walk reached, as one bit each on a grid centred on the camera.
pub struct OcclusionMask {
    center: (i32, i32),
    radius: i32,
    height: i32,
    bits: Vec<u64>,
    /// The lowest layer of the open sky the walk left out, `height` when none.
    sky_from: i32,
    sky: Sky,
    /// What F3's `chunk_section_paths` draws, kept only when asked for.
    trace: Option<Box<Trace>>,
}

/// Per grid slot: the walk's steps from the camera's section
/// (`SectionOcclusionGraph.Node.step`) and the directions it came in by
/// (`sourceDirections`), as step directions.
struct Trace {
    step: Vec<u16>,
    entered: Vec<u8>,
}

/// How the open sky above a walk counts (see [`walk_grid`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sky {
    /// Above a reached section of the walk's top layer.
    OverReached,
    /// All of it in view, or none.
    All(bool),
}

impl OcclusionMask {
    fn new(center: (i32, i32), radius: i32, height: i32) -> Self {
        let side = (2 * radius + 1) as usize;
        let cells = side * side * height.max(0) as usize;
        Self {
            center,
            radius,
            height,
            bits: vec![0; cells.div_ceil(64)],
            sky_from: height,
            sky: Sky::All(false),
            trace: None,
        }
    }

    /// Grid slot of a section, `None` outside the grid.
    #[inline]
    fn slot(&self, (x, y, z): SectionPos) -> Option<usize> {
        let dx = x - self.center.0 + self.radius;
        let dz = z - self.center.1 + self.radius;
        let side = 2 * self.radius + 1;
        if !(0..side).contains(&dx) || !(0..side).contains(&dz) || !(0..self.height).contains(&y) {
            return None;
        }
        Some(((y * side + dz) * side + dx) as usize)
    }

    #[inline]
    fn has(&self, slot: usize) -> bool {
        self.bits[slot / 64] >> (slot % 64) & 1 != 0
    }

    #[inline]
    fn set(&mut self, slot: usize) {
        self.bits[slot / 64] |= 1 << (slot % 64);
    }

    /// Whether the walk reached the section, i.e. it may be visible.
    #[inline]
    /// TODO(perf): asked once per section by `Culler::reached_of`. With the
    /// bits laid out column by column (y fastest) a column's sections would
    /// be one shift; the walk (`walk_grid`) and the trace index the same
    /// slots, so all three change together. Small win (~3% of culling).
    pub fn contains(&self, (x, y, z): SectionPos) -> bool {
        if y < self.sky_from {
            return self.slot((x, y, z)).is_some_and(|slot| self.has(slot));
        }
        if y >= self.height {
            return false;
        }
        match self.sky {
            Sky::OverReached => self
                .slot((x, self.sky_from - 1, z))
                .is_some_and(|slot| self.has(slot)),
            Sky::All(seen) => {
                seen && within_view_distance(x - self.center.0, z - self.center.1, self.radius - 1)
            }
        }
    }

    /// The walk's steps to a reached section and the directions it stepped
    /// in by (bit per [`Direction`]), if the walk was traced.
    pub fn trace(&self, pos: SectionPos) -> Option<(u16, u8)> {
        let trace = self.trace.as_ref()?;
        let slot = self.slot(pos)?;
        (pos.1 < self.sky_from && self.has(slot)).then(|| (trace.step[slot], trace.entered[slot]))
    }

    /// The reached sections below the open sky, if the walk was traced.
    pub fn traced_sections(&self) -> impl Iterator<Item = SectionPos> + '_ {
        let side = 2 * self.radius + 1;
        let traced = self.trace.is_some();
        (0..self.sky_from.min(self.height))
            .flat_map(move |y| (0..side).flat_map(move |z| (0..side).map(move |x| (x, y, z))))
            .filter(move |_| traced)
            .filter(move |&(x, y, z)| self.has(((y * side + z) * side + x) as usize))
            .map(move |(x, y, z)| {
                (
                    x + self.center.0 - self.radius,
                    y,
                    z + self.center.1 - self.radius,
                )
            })
    }

    /// How many sections the walk reached, the open sky above included.
    pub fn count(&self) -> usize {
        let walked: usize = self
            .bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum();
        if self.sky_from >= self.height {
            return walked;
        }
        let side = 2 * self.radius + 1;
        let area = (side * side) as usize;
        let columns = match self.sky {
            Sky::OverReached => {
                let top = (self.sky_from - 1) as usize * area;
                (top..top + area).filter(|&slot| self.has(slot)).count()
            }
            Sky::All(false) => 0,
            Sky::All(true) => (0..side * side)
                .filter(|i| {
                    within_view_distance(
                        i % side - self.radius,
                        i / side - self.radius,
                        self.radius - 1,
                    )
                })
                .count(),
        };
        walked + columns * (self.height - self.sky_from) as usize
    }
}

/// `SectionOcclusionGraph.MINIMUM_ADVANCED_CULLING_DISTANCE`, squared: past it the smart cull applies.
const ADVANCED_CULLING_DISTANCE_SQR: f64 = 60.0 * 60.0;
/// `MINIMUM_ADVANCED_CULLING_SECTION_DISTANCE`: `blockToSectionCoord(60)`.
const ADVANCED_CULLING_SECTIONS: i32 = 60 >> 4;
/// `CEILED_SECTION_DIAGONAL`.
const SECTION_DIAGONAL: f64 = 28.0;

/// Where the camera is exactly, for vanilla's "smart cull" of distant sections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub position: DVec3,
    /// The world's lowest block y: section 0 starts here.
    pub min_y: i32,
}

/// TODO(perf): measure at large view distances. 83 µs at 8 chunks
/// (`perf_world` 2026-11-15); the sections reached grow with the square of
/// the distance (more in open terrain), so 32 chunks may cost over 1 ms a
/// walk. Check how often a frame walks, and whether the frustum test per
/// section (`Frustum::sees_centered_box`) or the queue dominates.
pub fn walk(
    camera: SectionPos,
    height: i32,
    view_distance: i32,
    eye: Option<Eye>,
    section: impl Fn(SectionPos) -> SectionState,
) -> OcclusionMask {
    let grid = SectionGrid::from_fn((camera.0, camera.2), view_distance, height, section);
    walk_grid(camera, view_distance, eye, &grid, true, false)
}

/// A section waiting in [`walk_grid`]'s queue, by its place on the grid.
#[derive(Clone, Copy)]
struct Queued {
    x: u16,
    y: u16,
    z: u16,
    /// Steps from the camera's section.
    step: u16,
}

/// [`walk`] over a grid centred on the camera's chunk.
fn walk_grid(
    camera: SectionPos,
    view_distance: i32,
    eye: Option<Eye>,
    grid: &SectionGrid,
    sky_band: bool,
    traced: bool,
) -> OcclusionMask {
    let radius = view_distance + 1;
    let height = grid.height;
    let mut mask = OcclusionMask::new((camera.0, camera.2), radius, height);
    if height <= 0 || grid.radius != radius || grid.center != (camera.0, camera.2) {
        return mask;
    }
    let side = 2 * radius + 1;
    let area = side * side;
    let in_view: Vec<bool> = (0..area)
        .map(|i| within_view_distance(i % side - radius, i / side - radius, view_distance))
        .collect();
    let mut entered = vec![0u8; (area * height) as usize];
    let mut travelled = vec![0u8; (area * height) as usize];
    let mut rays = eye.map(|eye| Rays::new(eye, camera, radius, height));
    let mut queue = Vec::new();
    let start = Queued {
        x: radius as u16,
        y: camera.1.clamp(0, height - 1) as u16,
        z: radius as u16,
        step: 0,
    };
    let mut steps = if traced {
        vec![0u16; (area * height) as usize]
    } else {
        Vec::new()
    };
    let top = if sky_band {
        let open = VisibilitySet::ALL.bits();
        let highest = (0..height).rev().find(|&y| {
            let layer = (y * area) as usize..((y + 1) * area) as usize;
            grid.cells[layer]
                .iter()
                .any(|&cell| cell != open && cell != SectionGrid::MISSING)
        });
        highest
            .map_or(0, |y| y + 1)
            .max(i32::from(start.y))
            .min(height - 1)
    } else {
        height - 1
    };
    mask.set(((i32::from(start.y) * side + radius) * side + radius) as usize);
    queue.push(start);

    let mut next_in_queue = 0;
    while let Some(&at) = queue.get(next_in_queue) {
        next_in_queue += 1;
        let (x, y, z) = (i32::from(at.x), i32::from(at.y), i32::from(at.z));
        let slot = ((y * side + z) * side + x) as usize;
        let cell = grid.cells[slot];
        if cell == SectionGrid::MISSING {
            continue;
        }
        let path = travelled[slot];
        let exits = exits_of(cell, entered[slot], path);
        let distant = (x - radius).abs() > ADVANCED_CULLING_SECTIONS
            || (y - camera.1).abs() > ADVANCED_CULLING_SECTIONS
            || (z - radius).abs() > ADVANCED_CULLING_SECTIONS;
        for exit in Direction::ALL {
            if exits & exit.bit() == 0 {
                continue;
            }
            let [dx, dy, dz] = exit.step();
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            if !(0..side).contains(&nx) || !(0..side).contains(&nz) || !(0..=top).contains(&ny) {
                continue;
            }
            if !in_view[(nz * side + nx) as usize] {
                continue;
            }
            if distant
                && let Some(rays) = &mut rays
                && !rays.sees_camera(&mask, (x, y, z), exit)
            {
                continue;
            }
            let next = ((ny * side + nz) * side + nx) as usize;
            if mask.has(next) {
                entered[next] |= exit.bit();
            } else {
                entered[next] = exit.bit();
                travelled[next] = path | exit.bit();
                mask.set(next);
                let step = at.step.saturating_add(1);
                if traced {
                    steps[next] = step;
                }
                queue.push(Queued {
                    x: nx as u16,
                    y: ny as u16,
                    z: nz as u16,
                    step,
                });
            }
        }
    }
    if traced {
        mask.trace = Some(Box::new(Trace {
            step: steps,
            entered,
        }));
    }
    mask.sky_from = top + 1;
    mask.sky = if eye.is_none() {
        Sky::OverReached
    } else {
        let layer = (top * area) as usize..((top + 1) * area) as usize;
        Sky::All(layer.into_iter().any(|slot| mask.has(slot)))
    };
    mask
}

/// Every direction's bit.
const ALL_DIRECTIONS: u8 = 0x3F;

/// The faces a section with visibility bits `cell` may be left through.
#[inline]
fn exits_of(cell: u64, entered: u8, travelled: u8) -> u8 {
    let exits = if entered == 0 {
        ALL_DIRECTIONS
    } else {
        let through = opposite_bits(Directions(entered));
        let mut exits = 0;
        for from in 0..6 {
            if through >> from & 1 != 0 {
                exits |= (cell >> (from * 6)) as u8 & ALL_DIRECTIONS;
            }
        }
        exits
    };
    exits & !opposite_bits(Directions(travelled)) & ALL_DIRECTIONS
}

/// In [`Rays`]: the line from this corner stays in reached sections.
const SEES: u8 = u8::MAX;

/// `floor(v) as i32`, without a call into libm.
#[inline]
fn floor_i32(v: f64) -> i32 {
    let t = v as i32;
    if f64::from(t) > v { t - 1 } else { t }
}

/// The smart cull's lines towards the camera during one walk.
struct Rays {
    camera: SectionPos,
    radius: i32,
    height: i32,
    /// The eye, exactly.
    cam: [f64; 3],
    min_y: f64,
    /// `ViewArea.maxY`: `Level.getMaxY`.
    top: f64,
    /// Per section corner.
    known: Vec<u8>,
}

impl Rays {
    fn new(eye: Eye, camera: SectionPos, radius: i32, height: i32) -> Self {
        let size = f64::from(SECTION_SIZE);
        let min_y = f64::from(eye.min_y);
        Self {
            camera,
            radius,
            height,
            cam: [eye.position.x, eye.position.y, eye.position.z],
            min_y,
            top: min_y + f64::from(height) * size - 1.0,
            known: vec![0; ((2 * radius + 2) * (2 * radius + 2) * (height + 1)) as usize],
        }
    }

    /// Vanilla's distant step test.
    fn sees_camera(
        &mut self,
        mask: &OcclusionMask,
        (x, y, z): SectionPos,
        exit: Direction,
    ) -> bool {
        let camera = [self.radius, self.camera.1, self.radius];
        let at = [x, y, z];
        let step_axis = exit.step().iter().position(|&s| s != 0);
        let far = |axis: usize| {
            i32::from(if step_axis == Some(axis) {
                camera[axis] >= at[axis]
            } else {
                camera[axis] < at[axis]
            })
        };
        let corner = [x + far(0), y + far(1), z + far(2)];
        let side = 2 * self.radius + 2;
        let slot = ((corner[2] * side + corner[0]) * (self.height + 1) + corner[1]) as usize;
        let known = self.known[slot];
        if known == SEES {
            return true;
        }

        let size = f64::from(SECTION_SIZE);
        let min_y = self.min_y;
        let cam = self.cam;
        let mut check = [
            f64::from(corner[0] - self.radius + self.camera.0) * size,
            min_y + f64::from(corner[1]) * size,
            f64::from(corner[2] - self.radius + self.camera.2) * size,
        ];
        let to = [cam[0] - check[0], cam[1] - check[1], cam[2] - check[2]];
        let scale = SECTION_DIAGONAL / (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
        let step = to.map(|t| t * scale);
        let dist_sqr = |p: &[f64; 3]| {
            let (x, y, z) = (p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]);
            x * x + y * y + z * z
        };
        let mut sample = 0u8;
        while sample + 1 < known {
            for (c, s) in check.iter_mut().zip(step) {
                *c += s;
            }
            sample += 1;
        }
        let mut sees = true;
        while dist_sqr(&check) > ADVANCED_CULLING_DISTANCE_SQR {
            for (c, s) in check.iter_mut().zip(step) {
                *c += s;
            }
            sample = sample.saturating_add(1);
            if check[1] > self.top || check[1] < min_y {
                break;
            }
            let section = (
                section_coord(floor_i32(check[0])),
                section_coord(floor_i32(check[1]) - min_y as i32),
                section_coord(floor_i32(check[2])),
            );
            if !mask.contains(section) {
                sees = false;
                break;
            }
        }
        self.known[slot] = if sees {
            SEES
        } else if sample == SEES {
            0
        } else {
            sample
        };
        sees
    }
}

/// The faces a section was entered *through*.
#[inline]
fn opposite_bits(steps: Directions) -> u8 {
    steps.iter().fold(0, |bits, d| bits | d.opposite().bit())
}

/// One walk to run.
pub struct WalkJob {
    camera: SectionPos,
    height: i32,
    view_distance: i32,
    eye: Option<Eye>,
    grid: SectionGrid,
    traced: bool,
}

impl WalkJob {
    /// Keeps what F3's `chunk_section_paths` draws ([`OcclusionMask::trace`]).
    pub fn traced(self, traced: bool) -> Self {
        Self { traced, ..self }
    }

    pub fn new(
        camera: SectionPos,
        height: i32,
        view_distance: i32,
        eye: Option<Eye>,
        grid: SectionGrid,
    ) -> Self {
        Self {
            camera,
            height,
            view_distance,
            eye,
            grid,
            traced: false,
        }
    }

    pub fn run(self) -> OcclusionMask {
        if self.grid.height != self.height {
            return OcclusionMask::new(
                (self.camera.0, self.camera.2),
                self.view_distance + 1,
                self.height,
            );
        }
        walk_grid(
            self.camera,
            self.view_distance,
            self.eye,
            &self.grid,
            true,
            self.traced,
        )
    }
}

/// The camera's section, the level's revision, the view distance and
/// whether the walk is traced.
type Key = (SectionPos, u64, i32, bool);

/// The thread walks run on, and the walk in flight.
struct Worker {
    jobs: crossbeam_channel::Sender<(Key, WalkJob)>,
    done: crossbeam_channel::Receiver<(Key, OcclusionMask)>,
    /// The key being walked, if a job is out.
    in_flight: Option<Key>,
}

impl Worker {
    fn spawn() -> Option<Self> {
        let (jobs, job_rx) = crossbeam_channel::unbounded::<(Key, WalkJob)>();
        let (done_tx, done) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("occlusion".into())
            .spawn(move || {
                for (key, job) in job_rx {
                    if done_tx.send((key, job.run())).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            jobs,
            done,
            in_flight: None,
        })
    }
}

/// The last walk, kept until the camera changes section or the level changes.
#[derive(Default)]
pub struct OcclusionCache {
    /// What the mask in `mask` was walked for.
    walked: Option<Key>,
    mask: Option<OcclusionMask>,
    worker: Option<Worker>,
}

impl OcclusionCache {
    /// The mask the last [`Self::update`] gave.
    pub fn mask(&self) -> Option<&OcclusionMask> {
        self.mask.as_ref()
    }

    /// The newest finished mask, after asking for a walk if `camera` or the level changed.
    pub fn update(
        &mut self,
        camera: SectionPos,
        revision: u64,
        view_distance: i32,
        traced: bool,
        job: impl FnOnce() -> WalkJob,
    ) -> &OcclusionMask {
        let key = (camera, revision, view_distance, traced);
        if self.worker.is_none() {
            self.worker = Worker::spawn();
        }
        if let Some(worker) = &mut self.worker {
            while let Ok((done, mask)) = worker.done.try_recv() {
                worker.in_flight = None;
                self.walked = Some(done);
                self.mask = Some(mask);
            }
        }
        if self.walked != Some(key) {
            match &mut self.worker {
                Some(worker) if worker.in_flight.is_some() => {}
                Some(worker) => {
                    if let Err(failed) = worker.jobs.send((key, job())) {
                        let (key, job) = failed.into_inner();
                        self.walked = Some(key);
                        self.mask = Some(job.run());
                        self.worker = None;
                    } else {
                        worker.in_flight = Some(key);
                    }
                }
                None => {
                    self.walked = Some(key);
                    self.mask = Some(job().run());
                }
            }
        }
        if self.mask.is_none()
            && let Some(worker) = &mut self.worker
            && let Ok((done, mask)) = worker.done.recv()
        {
            worker.in_flight = None;
            self.walked = Some(done);
            self.mask = Some(mask);
        }
        self.mask.as_ref().expect("the first walk was waited for")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::xorshift::XorShift;

    const HEIGHT: i32 = 8;

    #[test]
    fn open_space_reaches_everything_in_range() {
        let mask = walk((0, 4, 0), HEIGHT, 4, None, |_| {
            SectionState::Loaded(VisibilitySet::ALL)
        });
        for x in -4..=4 {
            for z in -4..=4 {
                for y in 0..HEIGHT {
                    assert_eq!(
                        mask.contains((x, y, z)),
                        within_view_distance(x, z, 4),
                        "({x}, {y}, {z})"
                    );
                }
            }
        }
    }

    #[test]
    fn a_solid_section_hides_what_is_behind_it() {
        let wall = (0, 4, -1);
        let mask = walk((0, 4, 0), HEIGHT, 4, None, |pos| {
            SectionState::Loaded(if pos == wall {
                VisibilitySet::NONE
            } else {
                VisibilitySet::ALL
            })
        });
        assert!(mask.contains(wall));
        assert!(!mask.contains((0, 4, -2)));
        assert!(mask.contains((1, 4, -2)));
    }

    #[test]
    fn underground_only_the_cave_is_seen() {
        let mask = walk((0, 2, 0), HEIGHT, 4, None, |pos| {
            SectionState::Loaded(if pos == (0, 2, 0) {
                VisibilitySet::ALL
            } else {
                VisibilitySet::NONE
            })
        });
        assert_eq!(mask.count(), 1 + 6);
    }

    #[test]
    fn a_traced_walk_knows_each_sections_steps_and_way_in() {
        let grid = SectionGrid::from_fn((0, 0), 3, HEIGHT, |_| {
            SectionState::Loaded(VisibilitySet::ALL)
        });
        let mask = walk_grid((0, 4, 0), 3, None, &grid, true, true);
        assert_eq!(mask.trace((0, 4, 0)), Some((0, 0)));
        let (step, entered) = mask.trace((2, 4, 0)).unwrap();
        assert_eq!(step, 2);
        assert_eq!(entered, Direction::East.bit());
        assert!(mask.traced_sections().any(|p| p == (2, 4, 0)));
        let untraced = walk_grid((0, 4, 0), 3, None, &grid, true, false);
        assert_eq!(untraced.trace((2, 4, 0)), None);
    }

    #[test]
    fn missing_chunks_stop_the_walk() {
        let mask = walk((0, 0, 0), 1, 3, None, |pos| {
            if pos.0 == 1 {
                SectionState::Missing
            } else {
                SectionState::Loaded(VisibilitySet::ALL)
            }
        });
        assert!(mask.contains((1, 0, 0)));
        assert!(!mask.contains((2, 0, 0)));
    }

    fn eye_at(x: f64, y: f64, z: f64) -> Option<Eye> {
        Some(Eye {
            position: DVec3::new(x, y, z),
            min_y: 0,
        })
    }

    #[test]
    fn a_distant_step_needs_the_line_to_the_camera_reached() {
        let eye = eye_at(8.0, 72.0, 8.0).unwrap();
        let far = (0, 4, -8);
        let mut mask = OcclusionMask::new((0, 0), 12, HEIGHT);
        let mut rays = Rays::new(eye, (0, 4, 0), 12, HEIGHT);
        let slot = mask.slot((0, 4, 0)).unwrap();
        mask.set(slot);
        let at = (far.0 + 12, far.1, far.2 + 12);
        assert!(!rays.sees_camera(&mask, at, Direction::North));
        for z in -8..=0 {
            let slot = mask.slot((0, 4, z)).unwrap();
            mask.set(slot);
        }
        assert!(rays.sees_camera(&mask, at, Direction::North));
    }

    #[test]
    fn the_smart_cull_never_hides_what_is_near() {
        let open = |_| SectionState::Loaded(VisibilitySet::ALL);
        let plain = walk((0, 4, 0), HEIGHT, 8, None, open);
        let smart = walk((0, 4, 0), HEIGHT, 8, eye_at(8.0, 72.0, 8.0), open);
        assert_eq!(plain.count(), smart.count());
    }

    #[test]
    fn the_smart_cull_drops_sections_whose_line_is_walled_off() {
        let wall = |pos: SectionPos| pos.2 == -1 && pos.0.abs() <= 6;
        let section = |pos| {
            SectionState::Loaded(if wall(pos) {
                VisibilitySet::NONE
            } else {
                VisibilitySet::ALL
            })
        };
        let plain = walk((0, 4, 0), HEIGHT, 12, None, section);
        let smart = walk((0, 4, 0), HEIGHT, 12, eye_at(8.0, 72.0, 8.0), section);
        assert!(smart.count() <= plain.count());
        assert!(!smart.contains((0, 4, -9)));
        assert!(!plain.contains((0, 4, -9)) || smart.count() < plain.count());
    }

    #[test]
    fn the_sky_band_keeps_every_section_the_whole_walk_reaches() {
        const H: i32 = 12;
        let mut rng = XorShift(0x2545_f491_4f6c_dd1d);
        let mut random = move || rng.next_u64();
        let r = 9;
        let mut sets = std::collections::HashMap::new();
        for x in -r - 4..=r + 4 {
            for z in -r - 4..=r + 4 {
                let hill = (random() % 6) as i32;
                for y in 0..H {
                    let open = y > hill || random() % 3 == 0;
                    let set = if open {
                        VisibilitySet::ALL
                    } else {
                        VisibilitySet::from_bits(random())
                    };
                    sets.insert((x, y, z), set);
                }
            }
        }
        let section =
            |pos| SectionState::Loaded(sets.get(&pos).copied().unwrap_or(VisibilitySet::ALL));
        for camera in [(0, 3, 0), (2, 7, -1), (0, 11, 0), (-3, 0, 4)] {
            let grid = SectionGrid::from_fn((camera.0, camera.2), r - 1, H, section);
            let eye = eye_at(
                f64::from(camera.0 * 16) + 3.3,
                f64::from(camera.1 * 16) + 9.1,
                f64::from(camera.2 * 16) + 12.7,
            );
            for eye in [None, eye] {
                let whole = walk_grid(camera, r - 1, eye, &grid, false, false);
                let banded = walk_grid(camera, r - 1, eye, &grid, true, false);
                if eye.is_none() {
                    assert_eq!(whole.count(), banded.count());
                }
                for y in 0..H {
                    for x in -r..=r {
                        for z in -r..=r {
                            let pos = (camera.0 + x, y, camera.2 + z);
                            if eye.is_none() || y <= 6.max(camera.1) {
                                assert_eq!(whole.contains(pos), banded.contains(pos), "{pos:?}");
                            } else {
                                assert!(!whole.contains(pos) || banded.contains(pos), "{pos:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    fn open_job(camera: SectionPos) -> WalkJob {
        let grid = SectionGrid::new((camera.0, camera.2), 2, 1, |_, _| Column::Open);
        WalkJob::new(camera, 1, 2, None, grid)
    }

    #[test]
    fn the_cache_walks_once_per_section() {
        let mut cache = OcclusionCache::default();
        let mut walks = 0;
        for _ in 0..3 {
            cache.update((0, 0, 0), 7, 2, false, || {
                walks += 1;
                open_job((0, 0, 0))
            });
        }
        assert_eq!(walks, 1);
        let mut mask_for_new = false;
        for _ in 0..1000 {
            let mask = cache.update((1, 0, 0), 7, 2, false, || {
                walks += 1;
                open_job((1, 0, 0))
            });
            if mask.contains((3, 0, 0)) {
                mask_for_new = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(mask_for_new);
        assert_eq!(walks, 2);
    }
}
