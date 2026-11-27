//! `--bench`.

pub mod nvml;
pub mod report;

use std::path::PathBuf;

use melon_audio::SoundInstance;
use melon_audio::source::SoundSource;
use melon_core::math::angle::{self, TAU};
use melon_world::player::{MoveInput, PlayerState};

/// The world the benchmark plays in, under `saves/`.
pub const WORLD_FOLDER: &str = "melon-bench";
/// The active scene's world, made anew for every run: the mobs it summons
/// would otherwise pile up from run to run.
pub const ACTIVE_WORLD_FOLDER: &str = "melon-bench-active";
/// Its seed, so every machine generates the same terrain.
pub const WORLD_SEED: &str = "melon-bench";

/// After this many ticks without new chunks or meshes, loading is done.
const SETTLE_TICKS: u32 = 40;
/// The share of the square of chunks around the player that loading waits for.
const EXPECTED_SHARE: f32 = 0.7;
/// Loading gives up waiting after this long (2.5 minutes).
const LOAD_TIMEOUT_TICKS: u32 = 3000;
/// How far from the player the `sound` phase's sounds are, in blocks.
const SOUND_DISTANCE: f64 = 6.0;
/// What the `sound` phase plays, in turn.
const SOUNDS: [&str; 5] = [
    "block.stone.break",
    "block.grass.step",
    "block.wood.place",
    "entity.experience_orb.pickup",
    "block.note_block.harp",
];

/// How many mobs the `spawn` phase summons each tick, and for how many
/// ticks: 150 in all.
const SPAWNS_PER_TICK: u32 = 3;
const SPAWN_TICKS: u32 = 50;
/// What the `spawn` phase summons, in turn: animals that wander, monsters
/// that burn in the daylight, and items that bob.
const SPAWNED: [&str; 12] = [
    "minecraft:cow",
    "minecraft:sheep",
    "minecraft:pig",
    "minecraft:chicken",
    "minecraft:villager",
    "minecraft:zombie",
    "minecraft:skeleton",
    "minecraft:horse",
    "minecraft:wolf",
    "minecraft:rabbit",
    "minecraft:armor_stand",
    "minecraft:item",
];

/// Which world the benchmark plays: `--scene`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scene {
    /// Terrain alone: no mobs, no weather, the time standing still.
    #[default]
    Calm,
    /// Thunder, time running and 150 summoned mobs and items, looked at
    /// from among them before the calm phases.
    Active,
}

impl Scene {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "calm" => Some(Self::Calm),
            "active" => Some(Self::Active),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Calm => "calm",
            Self::Active => "active",
        }
    }

    pub fn world_folder(self) -> &'static str {
        match self {
            Self::Calm => WORLD_FOLDER,
            Self::Active => ACTIVE_WORLD_FOLDER,
        }
    }
}

/// The command line's `--bench` options.
#[derive(Clone, Debug, Default)]
pub struct BenchOptions {
    /// Names the run's files; `bench` without `--label`.
    pub label: String,
    /// `--fps-cap N`: a framerate limit instead of none.
    pub fps_cap: Option<u32>,
    /// `--compare <summary.json>`: an earlier run to hold this one against.
    pub compare: Option<PathBuf>,
    /// `--scene calm|active`.
    pub scene: Scene,
    /// `--out <dir>`: where the CSV and summary go.
    pub out: Option<PathBuf>,
    /// `--render-distance N`: in place of the preset's 32, for both the
    /// render and the simulation distance.
    pub render_distance: Option<u8>,
    /// `--hold <phase>`: repeat that phase until the window is closed
    /// instead of going on, for GPU profilers that need a steady scene
    /// (see `performance/README.md`). Nothing is written then.
    pub hold: Option<Phase>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Load,
    /// Active scene: the mobs are summoned.
    Spawn,
    /// Active scene: the camera turns on the ground among them.
    Crowd,
    Rise,
    Turn,
    Fly,
    Sound,
    Idle,
    /// The calm scene: off to the densest trees nearby, settling there.
    Enter,
    /// Turning at the ground under their leaves: cutout terrain seen through
    /// layers of leaves, much of it hidden behind trunks and canopies.
    Forest,
    Done,
}

impl Phase {
    pub const MEASURED: [Phase; 9] = [
        Phase::Load,
        Phase::Spawn,
        Phase::Crowd,
        Phase::Rise,
        Phase::Turn,
        Phase::Fly,
        Phase::Sound,
        Phase::Idle,
        Phase::Forest,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Phase::Load => "load",
            Phase::Spawn => "spawn",
            Phase::Crowd => "crowd",
            Phase::Rise => "rise",
            Phase::Turn => "turn",
            Phase::Fly => "fly",
            Phase::Sound => "sound",
            Phase::Idle => "idle",
            Phase::Enter => "enter",
            Phase::Forest => "forest",
            Phase::Done => "done",
        }
    }

    /// `--hold`'s phases: the ones that can repeat. Loading and summoning
    /// happen once.
    pub fn parse_holdable(name: &str) -> Option<Phase> {
        [
            Phase::Crowd,
            Phase::Rise,
            Phase::Turn,
            Phase::Fly,
            Phase::Sound,
            Phase::Idle,
            Phase::Forest,
        ]
        .into_iter()
        .find(|phase| phase.name() == name)
    }

    /// How long the phase runs; `None` for loading, which runs until the chunks are there.
    fn ticks(self) -> Option<u32> {
        match self {
            Phase::Load | Phase::Done => None,
            Phase::Spawn => Some(SPAWN_TICKS),
            Phase::Crowd => Some(300),
            Phase::Rise => Some(40),
            Phase::Turn => Some(200),
            Phase::Fly => Some(400),
            Phase::Sound => Some(200),
            Phase::Idle => Some(100),
            Phase::Enter => Some(ENTER_TICKS),
            Phase::Forest => Some(200),
        }
    }

    fn next(self, scene: Scene) -> Phase {
        match self {
            Phase::Load if scene == Scene::Active => Phase::Spawn,
            Phase::Load | Phase::Crowd => Phase::Rise,
            Phase::Spawn => Phase::Crowd,
            Phase::Rise => Phase::Turn,
            Phase::Turn => Phase::Fly,
            Phase::Fly => Phase::Sound,
            Phase::Sound => Phase::Idle,
            // Last, so the phases before compare with older runs.
            Phase::Idle if scene == Scene::Calm => Phase::Enter,
            Phase::Enter => Phase::Forest,
            Phase::Idle | Phase::Forest | Phase::Done => Phase::Done,
        }
    }
}

/// How far the world has come in, for when loading is done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadState {
    pub chunks: usize,
    /// Sections waiting for a mesh or being meshed.
    pub meshing: usize,
}

/// What the benchmark does in a tick in the player's place.
#[derive(Default)]
pub struct BenchStep {
    pub input: MoveInput,
    pub sound: Option<SoundInstance<'static>>,
    /// Commands for the server, without their slash.
    pub commands: Vec<String>,
    /// Send the player to [`forest_spot`] (`tp`).
    pub find_forest: bool,
}

pub struct BenchDriver {
    scene: Scene,
    /// Mobs summoned so far.
    spawned: u32,
    phase: Phase,
    /// Ticks into the phase.
    ticks: u32,
    /// The yaw the player had when loading was done.
    yaw: f32,
    /// Ticks the [`LoadState`] stayed the same, while loading.
    settled: u32,
    last_load: LoadState,
    /// Loading waits for at least this many chunks.
    expected_chunks: usize,
    /// `--hold`: the phase that starts over instead of ending.
    hold: Option<Phase>,
}

impl BenchDriver {
    /// A run at this render distance, which says how many chunks loading waits for.
    pub fn new(render_distance: u8, scene: Scene, hold: Option<Phase>) -> Self {
        let side = f32::from(render_distance) * 2.0 + 1.0;
        Self {
            scene,
            spawned: 0,
            phase: Phase::Load,
            ticks: 0,
            yaw: 0.0,
            settled: 0,
            last_load: LoadState::default(),
            expected_chunks: (side * side * EXPECTED_SHARE) as usize,
            hold,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// One tick.
    pub fn tick(&mut self, player: &mut PlayerState, load: LoadState) -> BenchStep {
        self.ticks += 1;
        let done = match self.phase.ticks() {
            Some(ticks) => self.ticks >= ticks,
            None => self.phase == Phase::Load && self.loaded(load),
        };
        if done && self.hold == Some(self.phase) {
            self.ticks = 0;
        } else if done {
            let from = self.phase;
            self.phase = self.phase.next(self.scene);
            self.ticks = 0;
            if from == Phase::Load {
                self.yaw = player.yaw();
            }
        }
        let t = self.ticks as f32 / self.phase.ticks().unwrap_or(1) as f32;
        let mut step = BenchStep::default();
        let BenchStep {
            input,
            sound,
            commands,
            find_forest,
        } = &mut step;
        match self.phase {
            Phase::Load | Phase::Done => {}
            Phase::Spawn => {
                if self.ticks == 0 {
                    commands.push("weather thunder 6000".to_owned());
                    commands.push("time set 11500".to_owned());
                }
                for _ in 0..SPAWNS_PER_TICK {
                    commands.push(summon_command(self.spawned));
                    self.spawned += 1;
                }
            }
            Phase::Crowd => {
                // Looking a little down, around once: the crowd, the
                // ground it stands on and the sky over it.
                player.set_rotation(self.yaw + 360.0 * t, 15.0);
            }
            Phase::Rise => {
                if self.ticks == 0 {
                    player.start_flying();
                }
                player.set_rotation(self.yaw, 0.0);
                input.jump = true;
            }
            Phase::Turn => {
                let pitch = 30.0 * angle::sin(t * TAU * 2.0);
                player.set_rotation(self.yaw + 360.0 * t, pitch);
            }
            Phase::Fly => {
                player.set_rotation(self.yaw, 0.0);
                input.forward = 1.0;
                input.sprint = true;
            }
            Phase::Sound => {
                let event = SOUNDS[self.ticks as usize % SOUNDS.len()];
                let turn = f64::from(self.ticks) * 0.7;
                let feet = player.feet();
                let at = [
                    feet.x + turn.cos() * SOUND_DISTANCE,
                    feet.y + 1.0,
                    feet.z + turn.sin() * SOUND_DISTANCE,
                ];
                *sound = Some(
                    SoundInstance::of(event)
                        .on(SoundSource::Blocks)
                        .volume(0.5)
                        .at(at),
                );
            }
            Phase::Idle => {}
            Phase::Enter => {
                *find_forest = self.ticks == 0;
                player.set_rotation(self.yaw, 0.0);
            }
            Phase::Forest => {
                let pitch = 10.0 * angle::sin(t * TAU);
                player.set_rotation(self.yaw + 360.0 * t, pitch);
            }
        }
        step
    }

    /// Loading is done once the chunks around have come and nothing changed for a while.
    fn loaded(&mut self, load: LoadState) -> bool {
        let enough = load.chunks >= self.expected_chunks.max(1);
        if load == self.last_load && enough && load.meshing == 0 {
            self.settled += 1;
        } else {
            self.settled = 0;
        }
        self.last_load = load;
        self.settled >= SETTLE_TICKS || self.ticks >= LOAD_TIMEOUT_TICKS
    }
}

/// Ticks the `enter` phase gives the teleport and the meshes around to land.
const ENTER_TICKS: u32 = 60;
/// How far from the player [`forest_spot`] looks, in blocks.
const FOREST_SEARCH: i32 = 96;
/// The square around a candidate whose tree tops count, in blocks either way.
const FOREST_REACH: i32 = 12;

/// Where to stand under the densest trees within [`FOREST_SEARCH`] of
/// `around`: the feet on the ground below a leafy column with room to stand,
/// or on the canopy if no such ground is found. `None` without leaves.
/// `top(x, z)` is the motion-blocking height, `block(x, y, z)` the state there.
pub fn forest_spot(
    around: glam::DVec3,
    top: impl Fn(i32, i32) -> Option<i32>,
    block: impl Fn(i32, i32, i32) -> u16,
) -> Option<glam::DVec3> {
    let leafy = |x: i32, z: i32| {
        top(x, z).filter(|&y| melon_world::block::state_flags::is_leaves(block(x, y - 1, z)))
    };
    let (cx, cz) = (around.x.floor() as i32, around.z.floor() as i32);
    let mut best = (0, cx, cz);
    for x in (cx - FOREST_SEARCH..=cx + FOREST_SEARCH).step_by(8) {
        for z in (cz - FOREST_SEARCH..=cz + FOREST_SEARCH).step_by(8) {
            let mut score = 0;
            for dx in (-FOREST_REACH..=FOREST_REACH).step_by(3) {
                for dz in (-FOREST_REACH..=FOREST_REACH).step_by(3) {
                    score += i32::from(leafy(x + dx, z + dz).is_some());
                }
            }
            if score > best.0 {
                best = (score, x, z);
            }
        }
    }
    let (score, x, z) = best;
    if score == 0 {
        return None;
    }
    let feet = |x: i32, z: i32, y: i32| {
        glam::DVec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5)
    };
    // A column under the canopy: leaves, air below them, then the ground
    // with two blocks of air on it.
    for dx in -FOREST_REACH..=FOREST_REACH {
        for dz in -FOREST_REACH..=FOREST_REACH {
            let (x, z) = (x + dx, z + dz);
            let Some(top) = leafy(x, z) else { continue };
            let mut y = top - 1;
            while melon_world::block::state_flags::is_leaves(block(x, y, z)) {
                y -= 1;
            }
            let mut air = 0;
            while !melon_world::block::state_flags::is_drawn(block(x, y, z)) && y > top - 48 {
                air += 1;
                y -= 1;
            }
            if air >= 2 && !melon_world::block::state_flags::is_leaves(block(x, y, z)) {
                return Some(feet(x, z, y + 1));
            }
        }
    }
    leafy(x, z).map(|y| feet(x, z, y))
}

/// The `index`th mob of the `spawn` phase, a block up so it lands on the
/// ground: on a golden-angle spiral, 4 to 19 blocks from the player.
fn summon_command(index: u32) -> String {
    let entity = SPAWNED[index as usize % SPAWNED.len()];
    let (x, z) = around(index, 4.0 + (index % 16) as f32);
    let nbt = if entity == "minecraft:item" {
        r#" {Item:{id:"minecraft:diamond",count:1}}"#
    } else {
        ""
    };
    format!("summon {entity} ~{x:.1} ~1 ~{z:.1}{nbt}")
}

/// The `index`th point of a golden-angle spiral, `radius` from the centre.
fn around(index: u32, radius: f32) -> (f32, f32) {
    const GOLDEN_ANGLE: f32 = 2.399_963;
    let angle = index as f32 * GOLDEN_ANGLE;
    (angle.cos() * radius, angle.sin() * radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use pumpkin_data::Block;

    fn run_until(driver: &mut BenchDriver, player: &mut PlayerState, phase: Phase) -> u32 {
        let load = LoadState {
            chunks: 9,
            meshing: 0,
        };
        let mut ticks = 0;
        while driver.phase() != phase {
            driver.tick(player, load);
            ticks += 1;
            assert!(ticks < 10_000, "stuck in {:?}", driver.phase());
        }
        ticks
    }

    #[test]
    fn phases_run_in_order_for_their_ticks() {
        let mut driver = BenchDriver::new(1, Scene::Calm, None);
        let mut player = PlayerState::new(DVec3::ZERO);
        assert_eq!(
            run_until(&mut driver, &mut player, Phase::Rise),
            SETTLE_TICKS + 1
        );
        assert_eq!(run_until(&mut driver, &mut player, Phase::Turn), 40);
        assert_eq!(run_until(&mut driver, &mut player, Phase::Fly), 200);
        assert_eq!(run_until(&mut driver, &mut player, Phase::Sound), 400);
        assert_eq!(run_until(&mut driver, &mut player, Phase::Idle), 200);
        assert_eq!(run_until(&mut driver, &mut player, Phase::Enter), 100);
        assert_eq!(
            run_until(&mut driver, &mut player, Phase::Forest),
            ENTER_TICKS
        );
        assert_eq!(run_until(&mut driver, &mut player, Phase::Done), 200);
    }

    #[test]
    fn the_forest_is_entered_once() {
        let mut driver = BenchDriver::new(1, Scene::Calm, None);
        let mut player = PlayerState::new(DVec3::ZERO);
        let load = LoadState {
            chunks: 9,
            meshing: 0,
        };
        let mut asked = 0;
        while driver.phase() != Phase::Done {
            let step = driver.tick(&mut player, load);
            if step.find_forest {
                assert_eq!(driver.phase(), Phase::Enter);
                asked += 1;
            }
        }
        assert_eq!(asked, 1);
    }

    #[test]
    fn the_forest_spot_is_on_the_ground_under_the_leaves() {
        let leaves = Block::OAK_LEAVES.default_state.id.as_u16();
        let grass = Block::GRASS_BLOCK.default_state.id.as_u16();
        // A canopy at y 70..72 over x 100..120, ground at 63 everywhere.
        let block = |x: i32, y: i32, _z: i32| match y {
            ..=63 => grass,
            70..=72 if (100..120).contains(&x) => leaves,
            _ => 0,
        };
        let top = |x: i32, _z: i32| Some(if (100..120).contains(&x) { 73 } else { 64 });
        let spot = forest_spot(DVec3::new(40.0, 64.0, 0.0), top, block).expect("trees");
        assert!((100.0..120.0).contains(&spot.x), "{spot}");
        assert_eq!(spot.y, 64.0);
        let bare = |_x: i32, _y: i32, _z: i32| grass;
        assert_eq!(forest_spot(DVec3::ZERO, |_, _| Some(64), bare), None);
    }

    #[test]
    fn loading_waits_while_chunks_keep_coming() {
        let mut driver = BenchDriver::new(1, Scene::Calm, None);
        let mut player = PlayerState::new(DVec3::ZERO);
        for chunks in 1..100 {
            let load = LoadState { chunks, meshing: 0 };
            driver.tick(&mut player, load);
        }
        assert_eq!(driver.phase(), Phase::Load);
    }

    #[test]
    fn flying_holds_forward_and_sprint() {
        let mut driver = BenchDriver::new(1, Scene::Calm, None);
        let mut player = PlayerState::new(DVec3::ZERO);
        run_until(&mut driver, &mut player, Phase::Fly);
        let step = driver.tick(&mut player, LoadState::default());
        assert_eq!(step.input.forward, 1.0);
        assert!(step.input.sprint);
        assert!(step.sound.is_none());
        run_until(&mut driver, &mut player, Phase::Sound);
        let step = driver.tick(&mut player, LoadState::default());
        assert!(step.sound.is_some_and(|s| s.position.is_some()));
    }

    #[test]
    fn the_active_scene_spawns_then_looks_around_then_flies() {
        let mut driver = BenchDriver::new(1, Scene::Active, None);
        let mut player = PlayerState::new(DVec3::ZERO);
        let load = LoadState {
            chunks: 9,
            meshing: 0,
        };
        let mut commands = Vec::new();
        while driver.phase() != Phase::Crowd {
            commands.extend(driver.tick(&mut player, load).commands);
        }
        let summons = commands.iter().filter(|c| c.starts_with("summon ")).count();
        assert_eq!(summons as u32, SPAWNS_PER_TICK * SPAWN_TICKS);
        assert!(commands.iter().any(|c| c.starts_with("weather thunder")));
        assert_eq!(driver.phase(), Phase::Crowd);
        assert_eq!(run_until(&mut driver, &mut player, Phase::Rise), 300);
    }

    #[test]
    fn a_held_phase_starts_over_instead_of_ending() {
        let mut driver = BenchDriver::new(1, Scene::Calm, Some(Phase::Turn));
        let mut player = PlayerState::new(DVec3::ZERO);
        run_until(&mut driver, &mut player, Phase::Turn);
        let load = LoadState::default();
        let mut yaws = Vec::new();
        for _ in 0..Phase::Turn.ticks().unwrap() * 3 {
            driver.tick(&mut player, load);
            assert_eq!(driver.phase(), Phase::Turn);
            yaws.push(player.yaw());
        }
        let turn = Phase::Turn.ticks().unwrap() as usize;
        assert_eq!(yaws[1], yaws[turn + 1], "each round turns the same way");
    }

    #[test]
    fn only_repeatable_phases_can_be_held() {
        assert_eq!(Phase::parse_holdable("turn"), Some(Phase::Turn));
        assert_eq!(Phase::parse_holdable("idle"), Some(Phase::Idle));
        for once in ["load", "spawn", "done", "nope"] {
            assert_eq!(Phase::parse_holdable(once), None, "{once}");
        }
    }

    #[test]
    fn spawn_commands_read_as_vanilla_syntax() {
        assert_eq!(summon_command(0), "summon minecraft:cow ~4.0 ~1 ~0.0");
        assert!(summon_command(11).ends_with(r#"{Item:{id:"minecraft:diamond",count:1}}"#));
    }
}
