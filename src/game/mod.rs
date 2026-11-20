//! A joined world, where the layers meet for play: the connection's events

mod actions;
mod airborne;
mod ambient_sounds;
mod atmosphere;
pub mod bench;
mod block_cracks;
mod block_meshes;
mod border_mesh;
pub mod camera;
pub mod controls;
mod debug;
mod effects;
mod entities;
pub mod entity_models;
mod entity_preview;
mod environment;
mod events;
mod gizmos;
mod hand_render;
mod heads;
pub use heads::sync_head_objects;
mod crystal_beams;
mod entity_sprites;
mod fishing;
pub mod held_items;
mod interaction;
pub mod keys;
mod leads;
pub mod level_meshes;
mod loading;
mod nametags;
mod outline;
mod overlay;
mod paintings;
mod particles;
mod players;
mod position_report;
mod prediction;
pub mod profiler;
mod riding;
mod scene;
mod shadows;
mod sounds;
pub mod subsystems;
mod text_displays;
pub mod thread_cpu;
mod tick;
mod ui;
mod vitals;
pub mod world_resources;

use ui::FpsCounter;

use std::collections::HashMap;
use std::path::Path;

use glam::DVec3;

use crate::gui::hud::debug::{DebugOverlay, RenderStats};
use crate::gui::hud::{Hud, PlayerStatus};
use crate::gui::overlay::chat::Chat;
use crate::gui::text::font::Font;
use crate::join::Joined;
use crate::network::play::game::PlayerInput;
use crate::renderer::{EntityDraw, MeshId, Renderer};
use crate::world::biome::Biomes;
use crate::world::block_entity::BlockEntities;
use crate::world::block_tint::TintSettings;
use crate::world::clock::ClientClocks;
use crate::world::culling::occlusion::OcclusionCache;
use crate::world::dimension::{DimensionType, DimensionTypes};
use crate::world::game_mode::{GameMode, LocalGameMode};
use crate::world::hotbars::Hotbars;
use crate::world::inventory::{Inventory, OpenContainer};
use crate::world::level::Level;
use crate::world::player::PlayerState;
use camera::CameraMode;
use interaction::DestroyTarget;
use keys::Keys;
use level_meshes::{BlockEntityMeshes, LevelMeshes};
use world_resources::WorldResources;

/// `Permissions.COMMANDS_GAMEMASTER`: what switching game modes needs.
pub const GAMEMASTER_LEVEL: u8 = 2;

/// Someone in the player list (`PlayerInfo`).
#[derive(Clone, Debug)]
pub struct PlayerEntry {
    pub name: String,
    pub game_mode: GameMode,
    pub listed: bool,
    /// Milliseconds; negative while unknown.
    pub latency: i32,
    /// `tabListDisplayName`: the server's name for the list.
    pub display_name: Option<crate::component::Component>,
    /// `tabListOrder`: higher comes first.
    pub list_order: i32,
    pub show_hat: bool,
    /// The `textures` property, which names the skin.
    pub textures: Option<Box<str>>,
}

impl Default for PlayerEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            game_mode: GameMode::default(),
            listed: false,
            latency: 0,
            display_name: None,
            list_order: 0,
            show_hat: true,
            textures: None,
        }
    }
}

/// A joined world: every chunk the server streams, meshed, walkable with
pub struct InWorld {
    /// Dropped when the world is left: disconnects, then saves and stops a
    joined: Joined,
    level: Level,
    /// The level's terrain on the GPU.
    meshes: LevelMeshes,
    /// `Level.dimension().identifier()` from the join, for the F3 overlay.
    dimension: String,
    /// The dimension type's registry index, and the types the server
    dimension_types: DimensionTypes,
    /// The level's `DimensionType`: its height, light and environment.
    dimension_type: DimensionType,
    /// The server's biomes, for the colours of fog and sky.
    biomes: Biomes,
    /// The login's hashed seed: `BiomeManager`'s zoom seed.
    hashed_seed: i64,
    /// `LocalPlayer.waterVisionTime`: ticks the eyes have been under water.
    water_vision_time: i32,
    /// What the ambient sounds remember between ticks.
    ambient: ambient_sounds::Ambient,
    /// What this frame is cleared to: the fog's colour.
    clear_color: [f32; 4],
    /// The new level after a respawn into another dimension, until it is
    loading: Option<loading::LevelLoad>,
    /// The loading finished since the screens were told.
    level_became_ready: bool,
    /// The portal's glow and spin, the vignette's brightness.
    effects: effects::EffectState,
    /// This frame's screen effects, drawn with the HUD.
    effects_frame: crate::gui::effects::ScreenEffects,
    player: PlayerState,
    inventory: Inventory,
    status: PlayerStatus,
    /// The game mode and the one before it.
    game_mode: LocalGameMode,
    /// `EntityEvent.PERMISSION_LEVEL_*`: 0 (none) to 4 (owner).
    permission_level: u8,
    /// The player list, by profile id.
    players: HashMap<uuid::Uuid, PlayerEntry>,
    /// Objectives, scores and teams.
    scoreboard: crate::world::scoreboard::Scoreboard,
    /// The player list's header, footer and hearts.
    tab_list: crate::gui::hud::player_tab::PlayerTabOverlay,
    /// The server checks accounts: skins in the list are real.
    online_mode: bool,
    /// The own profile id, which picks the skin.
    uuid: uuid::Uuid,
    /// The player drawn for the inventory's box.
    preview: entity_preview::PreviewTextures,
    chat: Chat,
    /// The server's commands, for completing them in the chat.
    commands: crate::network::play::chat::commands::CommandTree,
    /// The registries' ids, for completing command arguments.
    registries: crate::network::play::chat::command_suggestions::Registries,
    /// Saved creative hotbars.
    hotbars: Hotbars,
    hud: Hud,
    /// A spectator's hotbar menu.
    spectator_menu: crate::gui::overlay::spectator_menu::SpectatorMenu,
    /// The open inventory, chat or switcher, if any. The world keeps running.
    overlay: Option<Box<dyn crate::gui::overlay::Overlay>>,
    /// The creative inventory's last tab, to reopen on.
    creative_tab: usize,
    /// Real time not yet spent on whole ticks.
    tick_time: f32,
    /// What the server last heard about the movement keys and sprinting.
    sent_input: PlayerInput,
    sent_sprinting: bool,
    /// Send input again even if the keys have not changed: after a teleport
    input_dirty: bool,
    /// What the server last heard of the position and look.
    position_report: position_report::PositionReport,
    /// What riding makes of the keys: the jump charge and the paddles last
    ride: riding::RideState,
    /// `Entity.DATA_TICKS_FROZEN` of the local player.
    frozen_ticks: i32,
    keys: Keys,
    camera_mode: CameraMode,
    debug: DebugOverlay,
    /// F3+B and F3+G: what is drawn into the world.
    pub(crate) debug_view: debug::view::DebugView,
    fps: FpsCounter,
    /// The frame loop's phases, for F3.
    pub profiler: profiler::FrameProfiler,
    /// The last cave-culling walk, rerun when the camera changes section.
    occlusion: OcclusionCache,
    /// Chest lids, bells, books...: what animates on block entities.
    block_entities: BlockEntities,
    /// Mobs, other players and every other entity the server tracks.
    entities: crate::world::entity::tracker::Entities,
    /// Ticks the local player's body still flashes red (`hurtTime`).
    own_hurt: i32,
    /// Ticks since the local player died (`deathTime`), 0 while alive.
    own_death: i32,
    /// `Player.hurtDir`: where the last blow came from, for the camera's
    own_hurt_dir: f32,
    /// `Camera.fovModifier` and the tick before: the smoothed
    fov_modifier: f32,
    fov_modifier_old: f32,
    block_entity_meshes: BlockEntityMeshes,
    /// Last frame's entity draws, refilled by the next (see [`Self::recycle`]).
    spare_entities: Vec<EntityDraw>,
    /// `ClientClockManager`: day/night from `SET_TIME`.
    clocks: ClientClocks,
    /// Blob shadows under the player and the entities.
    shadows: shadows::ShadowMeshes,
    /// The leads drawn, one mesh each.
    leads: leads::LeadMeshes,
    fishing: fishing::FishingMeshes,
    /// The orbs' and fireballs' camera-facing quads.
    sprites: entity_sprites::SpriteMeshes,
    beams: crystal_beams::BeamMeshes,
    paintings: paintings::PaintingMeshes,
    text_displays: text_displays::TextDisplayMeshes,
    /// The mobs' part matrices from the frame before, for those standing
    poses: crate::world::entity::pose_cache::PoseCache,
    /// Cracks on blocks being broken, the own and others'.
    cracks: block_cracks::BlockCracks,
    /// The particles, and what draws them.
    particles: particles::Particles,
    /// The outline of the block looked at.
    outline: outline::BlockOutline,
    /// Ticks since joining, for when cracks were last heard of.
    ticks: u64,
    /// Last `RAIN_LEVEL_CHANGE` / `THUNDER_LEVEL_CHANGE`.
    weather: crate::world::weather::WeatherLevels,
    /// The world border and the other settings the server sends for the level.
    border: crate::world::border::WorldBorder,
    border_mesh: border_mesh::BorderMesh,
    /// What the slower F3 + F6 world renderers keep between frames.
    visualizers: debug::visualizers::VisualizerCache,
    /// `DebugEntryChunkGeneration`'s lines and the block they are for.
    chunk_generation: std::cell::RefCell<Option<([i32; 3], Vec<String>)>>,
    /// The recipes the server unlocked, and what its machines accept.
    recipes: crate::world::recipes::RecipeBook,
    recipe_sets: crate::world::recipes::RecipeSets,
    /// The recipe the server asked the open menu to show (ghost items).
    ghost_recipe: Option<(i32, crate::world::recipes::RecipeDisplay)>,
    level_settings: crate::world::level_settings::LevelSettings,
    /// How far the rain's fog has closed in around the camera, and when it
    rain_fog: f32,
    /// The weather columns around the camera, for what they were made for.
    weather_columns: Option<(
        crate::world::weather::PrepareKey,
        Vec<crate::world::weather::Prepared>,
    )>,
    rain_fog_at: Option<std::time::Instant>,
    cloud_mesh: Option<MeshId>,
    /// What `cloud_mesh` was built for.
    cloud_key: Option<crate::world::clouds::CloudKey>,
    rain_mesh: Option<MeshId>,
    snow_mesh: Option<MeshId>,
    /// Server-opened menu (`OPEN_SCREEN`).
    open_container: Option<OpenContainer>,
    /// Open the container overlay on the next tick, once `WorldContext` exists.
    pending_container_overlay: bool,
    /// Close the container/inventory overlay on the next tick without telling
    pending_container_close: bool,
    /// `BlockStatePredictionHandler` sequence, starting at 1.
    sequence: i32,
    /// `BlockStatePredictionHandler`: what the server said of the blocks the
    predictions: prediction::Predictions,
    /// Where the camera was this frame, for the name tags the HUD layer draws.
    name_view: Option<nametags::NameView>,
    destroying: Option<DestroyTarget>,
    /// `MultiPlayerGameMode.destroyDelay`.
    destroy_delay: i32,
    /// Sounds the player made, for the app to play.
    sounds: Vec<crate::audio::SoundInstance<'static>>,
    footsteps: sounds::Footsteps,
    /// `Hud.isHidden`: F1 hid the HUD and the hand.
    hud_hidden: bool,
    /// The sound channels in use, static and streaming, for F3.
    sound_channels: [u32; 2],
    /// The decoded sound files and their bytes, for F3.
    sound_cache: (usize, usize),
    /// The last rendered camera as the sounds hear it: position, forward, up.
    ears: Option<sounds::Ears>,
    /// What the client played itself lately, to not play it again when
    echoes: sounds::Echoes,
    /// `Minecraft.rightClickDelay`.
    right_click_delay: i32,
    /// The plugin channels the server registered (`melon:swap`).
    server_channels: Vec<String>,
    /// `Minecraft.missTime`: after a punch at nothing, the attack key rests.
    miss_time: i32,
    /// `Minecraft.hitResult`: the block or entity looked at, picked every
    hit_result: Option<crate::world::pick::Hit>,
    /// Haste, Conduit Power and Mining Fatigue on the player.
    mining_effects: crate::world::player::mining::MiningEffects,
    /// Every status effect on the player, for the HUD, the inventory, the
    status_effects: crate::world::player::status_effects::ActiveEffects,
    /// The hands: swing, item in use, first-person heights.
    hands: crate::world::player::hands::Hands,
    /// F3's lines on the connection, sound, server and threads.
    subsystems: subsystems::SubsystemMeter,
    /// `--bench`: plays in the player's place.
    bench: Option<bench::BenchDriver>,
}

/// The frame's numbers the F3 overlay shows besides the world's own.
pub struct DebugFrame {
    pub render: RenderStats,
    pub clouds: crate::world::clouds::CloudStatus,
    pub biome_blend: u8,
    pub framerate_limit: Option<u32>,
    /// The frame's time, for the GPU's share of it.
    pub frame_seconds: f32,
    /// The window's logical size and pixel density.
    pub window: (u32, u32, f64),
    /// `VideoMode.refreshRateLabel` of the window's monitor.
    pub refresh_rate: Option<String>,
    /// A screen is open, which shows F3 over a hidden HUD.
    pub screen_open: bool,
    /// The GUI's scale, and the one F3 is drawn at (`debugGuiScale`).
    pub gui_scale: u32,
    pub debug_scale: u32,
}

/// What the server's packets this frame mean for the screens.
#[derive(Default)]
pub struct Received {
    pub died: bool,
    pub revived: bool,
    /// The player was sent to another dimension: the screen to show while
    pub loading: Option<LoadingReason>,
    /// The level the loading screen waited for is ready.
    pub loaded: bool,
    /// The server ended a reconfiguration with a new world: its login and
    pub rejoined: Option<(
        Box<crate::network::JoinInfo>,
        Vec<crate::network::WorldEvent>,
    )>,
}

/// `LevelLoadingScreen.Reason`: what the loading screen paints behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadingReason {
    NetherPortal,
    EndPortal,
    Other,
}

impl InWorld {
    /// `game_dir` holds `hotbar.nbt` and `command_history.txt`.
    pub fn enter(mut joined: Joined, game_dir: &Path) -> Self {
        let (debug_view, debug_visible) = debug::view::DebugView::load(game_dir);
        let mut debug =
            DebugOverlay::new(crate::gui::hud::debug_entries::DebugEntries::load(game_dir));
        debug.set_visible(debug_visible);
        let mut player = PlayerState::new(DVec3::new(8.0, 80.0, 8.0));
        let chat_types = std::mem::take(&mut joined.session.info.chat_types);
        // The main thread writes item stacks (creative slots) too.
        joined.session.info.registries.install_for_codecs();
        let registries = std::mem::take(&mut joined.session.info.registries);
        let info = &joined.session.info;
        let dimension = info.dimension_name.clone();
        let dimension_types = info.dimension_types.clone();
        let hashed_seed = info.hashed_seed;
        let online_mode = info.online_mode;
        let biomes = info.biomes.clone();
        let dimension_type = dimension_types.get(info.dimension_id);
        let mut level = Level::default();
        level.set_dimension(&dimension_type);
        let mut meshes = LevelMeshes::default();
        meshes.set_min_y(dimension_type.height.min_y);
        let mut game_mode = LocalGameMode::default();
        game_mode.set_both(info.game_mode, info.previous_game_mode);
        player.set_game_mode(game_mode.current);
        let status = PlayerStatus {
            game_mode: game_mode.current,
            hardcore: info.hardcore,
            ..Default::default()
        };
        let chat = Chat::new(chat_types, info.uuid, Some(game_dir.to_path_buf()));
        joined.session.send_register(&[
            pumpkin_protocol::java::melon_movement::CHANNEL,
            crate::network::play::SWAP_CHANNEL,
        ]);
        Self {
            uuid: info.uuid,
            preview: Default::default(),
            joined,
            level,
            meshes,
            dimension,
            dimension_types,
            dimension_type,
            biomes,
            hashed_seed,
            water_vision_time: 0,
            ambient: Default::default(),
            loading: Some(loading::LevelLoad::new()),
            level_became_ready: false,
            effects: effects::EffectState::default(),
            effects_frame: Default::default(),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            player,
            inventory: Inventory::default(),
            status,
            game_mode,
            permission_level: 0,
            players: HashMap::new(),
            scoreboard: Default::default(),
            tab_list: Default::default(),
            online_mode,
            chat,
            commands: Default::default(),
            registries,
            hotbars: Hotbars::load(game_dir),
            hud: Hud::default(),
            spectator_menu: Default::default(),
            overlay: None,
            creative_tab: 0,
            tick_time: 0.0,
            sent_input: PlayerInput::default(),
            sent_sprinting: false,
            input_dirty: false,
            position_report: Default::default(),
            ride: riding::RideState::default(),
            frozen_ticks: 0,
            keys: Keys::default(),
            camera_mode: CameraMode::default(),
            debug,
            debug_view,
            fps: FpsCounter::new(),
            profiler: profiler::FrameProfiler::default(),
            occlusion: OcclusionCache::default(),
            block_entities: BlockEntities::default(),
            entities: Default::default(),
            own_hurt: 0,
            own_death: 0,
            own_hurt_dir: 0.0,
            fov_modifier: 1.0,
            fov_modifier_old: 1.0,
            block_entity_meshes: BlockEntityMeshes::default(),
            spare_entities: Vec::new(),
            clocks: ClientClocks::default(),
            shadows: Default::default(),
            leads: Default::default(),
            fishing: Default::default(),
            sprites: Default::default(),
            beams: Default::default(),
            paintings: Default::default(),
            text_displays: Default::default(),
            poses: Default::default(),
            cracks: Default::default(),
            particles: Default::default(),
            outline: Default::default(),
            ticks: 0,
            weather: crate::world::weather::WeatherLevels::default(),
            border: Default::default(),
            border_mesh: Default::default(),
            visualizers: Default::default(),
            chunk_generation: Default::default(),
            recipes: crate::world::recipes::RecipeBook::with_layer(
                crate::world::recipes::layers::current(),
            ),
            recipe_sets: Default::default(),
            ghost_recipe: None,
            level_settings: Default::default(),
            rain_fog: 0.0,
            weather_columns: None,
            rain_fog_at: None,
            cloud_mesh: None,
            cloud_key: None,
            rain_mesh: None,
            snow_mesh: None,
            open_container: None,
            pending_container_overlay: false,
            pending_container_close: false,
            sequence: 0,
            predictions: Default::default(),
            name_view: None,
            destroying: None,
            destroy_delay: 0,
            sounds: Vec::new(),
            footsteps: sounds::Footsteps::default(),
            hud_hidden: false,
            sound_channels: [0; 2],
            sound_cache: (0, 0),
            ears: None,
            echoes: sounds::Echoes::default(),
            right_click_delay: 0,
            server_channels: Vec::new(),
            miss_time: 0,
            hit_result: None,
            mining_effects: Default::default(),
            status_effects: Default::default(),
            hands: Default::default(),
            subsystems: Default::default(),
            bench: None,
        }
    }

    /// Whether this is a singleplayer world on the integrated server.
    pub fn is_integrated(&self) -> bool {
        self.joined.is_integrated()
    }

    /// `--bench`: from now on the benchmark plays, at this render distance.
    pub fn start_bench(&mut self, render_distance: u8, options: &bench::BenchOptions) {
        self.bench = Some(bench::BenchDriver::new(
            render_distance,
            options.scene,
            options.hold,
        ));
    }

    /// The benchmark's phase, when it plays.
    pub fn bench_phase(&self) -> Option<bench::Phase> {
        self.bench.as_ref().map(bench::BenchDriver::phase)
    }

    /// The integrated world's `icon.png` path while the world has none and
    pub fn missing_icon(&self) -> Option<std::path::PathBuf> {
        if self.loading.is_some() {
            return None;
        }
        let path = self.joined.server.as_ref()?.save_path().join("icon.png");
        (!path.exists()).then_some(path)
    }

    /// Entities the client tracks, for the benchmark.
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Chunks loaded, for the benchmark.
    pub fn chunk_count(&self) -> usize {
        self.level.chunk_count()
    }

    /// What the connection thread took in so far.
    pub fn net_stats(&self) -> &crate::network::play::stats::NetStats {
        self.joined.session.stats()
    }

    /// Pumpkin's world of the dimension the player is in, in singleplayer.
    pub(super) fn server_world(&self) -> Option<std::sync::Arc<pumpkin::world::World>> {
        self.joined.server.as_ref()?.world(&self.dimension)
    }

    /// The integrated server's ms per tick, in a singleplayer world.
    pub fn server_mspt(&self) -> Option<f64> {
        self.joined.server.as_ref().and_then(|server| server.mspt())
    }

    /// Renews F3's subsystem lines while F3 is open.
    /// The sound channels in use (`AudioEngine::channels_used`), for F3.
    pub fn set_sound_channels(&mut self, used: [u32; 2]) {
        self.sound_channels = used;
    }

    /// Whether any F3 entry shows now.
    pub fn debug_shown(&self) -> bool {
        self.debug.entries().shows_anything()
    }

    /// Whether F3 shows the sound cache, which asks the engine for it.
    pub fn debug_extended(&self) -> bool {
        self.debug
            .entries()
            .is_enabled(crate::gui::hud::debug_entries::Entry::SoundCache)
    }

    /// `AudioEngine::cache_stats`, for F3.
    pub fn set_sound_cache(&mut self, cache: (usize, usize)) {
        self.sound_cache = cache;
    }

    pub fn sample_subsystems(&mut self, audio: &crate::audio::stats::AudioStats) {
        if !self.debug.entries().shows_anything() {
            self.subsystems.reset();
            return;
        }
        let mspt = self.server_mspt();
        self.subsystems
            .update(self.joined.session.stats(), audio, mspt);
    }

    /// Whether the world is hardcore: dying ends it.
    pub fn hardcore(&self) -> bool {
        self.status.hardcore
    }

    /// The death screen's Respawn (`ServerboundClientCommandPacket`).
    pub fn respawn(&self) {
        self.joined.session.send_respawn();
    }

    /// `ClientPacketListener.sendUnattendedCommand`: a command a dialog or
    /// a link runs, its `/` dropped (`Commands.trimOptionalPrefix`).
    pub fn run_command(&self, command: &str) {
        let command = command.strip_prefix('/').unwrap_or(command);
        self.joined.session.send_command(command);
    }

    /// `Options.broadcastOptions`: the server hears of changed options.
    pub fn send_client_info(&self, info: &crate::network::ClientInfo) {
        self.joined.session.send_client_information(info);
    }

    /// The mouse moved while the cursor is grabbed: the player looks around.
    pub fn look(&mut self, dx: f32, dy: f32, sensitivity: f64, invert_y: bool) {
        let scale = crate::settings::input::look_scale(sensitivity);
        let dy = if invert_y { -dy } else { dy };
        self.player.look(dx * scale, dy * scale);
    }

    /// The chat options changed.
    pub fn set_chat_settings(
        &mut self,
        settings: &crate::settings::options::ChatSettings,
        font: &Font,
    ) {
        self.chat.set_settings(settings, font);
    }

    /// The "Smooth Lighting" option; changing it remeshes everything.
    pub fn set_smooth_lighting(&mut self, on: bool) {
        self.level.set_smooth_lighting(on);
    }

    /// The "Chunk Builder" option.
    pub fn set_chunk_builder(&mut self, builder: crate::settings::options::ChunkBuilder) {
        self.level.set_chunk_builder(builder);
    }

    /// Colour maps, the biome registry and biome blend, for terrain tints.
    pub fn set_tints(
        &mut self,
        maps: std::sync::Arc<crate::world::colormap::ColorMaps>,
        blend: u8,
    ) {
        self.level.set_tints(TintSettings::new(
            &self.biomes,
            &maps,
            blend,
            Some(self.hashed_seed),
        ));
    }

    /// `Options.biomeBlendRadius`; changing it remeshes everything.
    pub fn set_biome_blend(&mut self, radius: u8) {
        self.level.set_biome_blend(radius);
    }

    /// Builds the sections the Chunk Builder wants for this frame, takes
    pub fn mesh(&mut self, renderer: &mut Renderer, assets: &WorldResources) {
        let eye = self.player.tick_eye().as_vec3();
        let finished = self.level.mesh_pending(&assets.models, assets.fluids, eye);
        self.meshes.upload(renderer, finished);
    }

    /// `ClientPacketListener.handleRespawn`: after death or a portal the
    pub(super) fn respawn_in(
        &mut self,
        renderer: &mut Renderer,
        dimension_id: i32,
        dimension_name: String,
    ) -> Option<LoadingReason> {
        if dimension_name == self.dimension {
            return None;
        }
        let end = "minecraft:the_end";
        let reason = if self.effects.in_portal_effect() {
            LoadingReason::NetherPortal
        } else if dimension_name == end || self.dimension == end {
            LoadingReason::EndPortal
        } else {
            LoadingReason::Other
        };
        self.release(renderer);
        self.dimension_type = self.dimension_types.get(dimension_id);
        self.dimension = dimension_name;
        self.level.set_dimension(&self.dimension_type);
        self.meshes.set_min_y(self.dimension_type.height.min_y);
        self.block_entities = BlockEntities::default();
        self.border = Default::default();
        self.entities.clear();
        self.occlusion = OcclusionCache::default();
        self.loading = Some(loading::LevelLoad::new());
        self.level_became_ready = false;
        Some(reason)
    }

    /// Frees what the world uploaded. Call when leaving it.
    pub fn reload_resources(
        &mut self,
        renderer: &mut Renderer,
        colormaps: std::sync::Arc<crate::world::colormap::ColorMaps>,
        biome_blend: u8,
    ) {
        self.meshes.clear(renderer);
        self.block_entity_meshes.clear(renderer);
        self.cracks.clear(renderer);
        self.clear_particles(renderer);
        self.outline.clear(renderer);
        self.debug_view.clear(renderer);
        self.set_tints(colormaps, biome_blend);
        self.level.remesh_all();
    }

    /// The same connection in a new world (`WorldEvent::Rejoined`): this
    pub fn rejoin(
        mut self,
        renderer: &mut Renderer,
        info: crate::network::JoinInfo,
        events: Vec<crate::network::WorldEvent>,
    ) -> Joined {
        self.release(renderer);
        let InWorld { mut joined, .. } = self;
        joined.session.rejoined(info, events);
        joined
    }

    pub fn release(&mut self, renderer: &mut Renderer) {
        self.level.clear();
        self.meshes.clear(renderer);
        self.block_entity_meshes.clear(renderer);
        self.shadows.clear(renderer);
        self.border_mesh.clear(renderer);
        self.leads.clear(renderer);
        self.fishing.clear(renderer);
        self.sprites.clear(renderer);
        self.beams.clear(renderer);
        self.paintings.clear(renderer);
        self.text_displays.clear(renderer);
        self.poses.clear();
        self.entities.clear();
        self.cracks.clear(renderer);
        self.clear_particles(renderer);
        self.outline.clear(renderer);
        self.preview.release(renderer);
    }

    /// What this frame is cleared to: the fog's colour, as vanilla clears
    pub fn sky_clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Why the world ended on its own, if it did: kicked, connection lost,
    pub fn ended(&mut self) -> Option<String> {
        if let Some(reason) = self.joined.session.ended() {
            return Some(reason.to_owned());
        }
        self.joined
            .server
            .as_ref()
            .is_some_and(crate::singleplayer::server::IntegratedServer::is_finished)
            .then(|| "The integrated server stopped.".to_owned())
    }
}

/// Melon's yaw faces north (-Z) at 0, vanilla's faces south (+Z).
fn minecraft_yaw(melon_yaw: f32) -> f32 {
    (melon_yaw + 180.0).rem_euclid(360.0)
}

fn melon_yaw(minecraft_yaw: f32) -> f32 {
    (minecraft_yaw - 180.0).rem_euclid(360.0)
}
