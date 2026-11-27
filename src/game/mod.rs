//! A joined world, where the layers meet for play.

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
pub use melon_gui::screens::ingame::loading::LoadingReason;
mod crystal_beams;
mod entity_sprites;
mod fishing;
pub mod held_items;
mod interaction;
pub mod keys;
mod leads;
mod level_info;
pub mod level_meshes;
mod loading;
mod nametags;
mod outline;
mod overlay;
mod paintings;
mod particle_meshes;
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

use camera::CameraMode;
use keys::Keys;
use level_meshes::{BlockEntityMeshes, LevelMeshes};
use melon_gui::hud::debug::{DebugOverlay, RenderStats};
use melon_gui::hud::{Hud, PlayerStatus};
use melon_gui::overlay::chat::Chat;
use melon_gui::text::font::Font;
use melon_net::join::Joined;
use melon_render::{EntityDraw, Renderer};
use melon_world::block_entity::BlockEntities;
use melon_world::block_tint::TintSettings;
use melon_world::clock::ClientClocks;
use melon_world::culling::occlusion::OcclusionCache;
use melon_world::game_mode::{GameMode, LocalGameMode};
use melon_world::hotbars::Hotbars;
use melon_world::inventory::Inventory;
use melon_world::level::Level;
use melon_world::player::PlayerState;
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
    pub display_name: Option<melon_protocol::component::Component>,
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

/// A joined world.
pub struct InWorld {
    /// Dropped when the world is left.
    joined: Joined,
    level: Level,
    /// The level's terrain on the GPU.
    meshes: LevelMeshes,
    /// The dimension, its type, the biomes and the seed.
    level_info: level_info::LevelInfo,
    /// What the ambient sounds remember between ticks.
    ambient: ambient_sounds::Ambient,
    /// The rain's fog and the colour the frame is cleared to.
    fog: environment::FogState,
    /// The clouds' and the weather's meshes.
    sky: atmosphere::SkyMeshes,
    /// The new level after a respawn into another dimension, until it is ready.
    loading: Option<loading::LevelLoad>,
    /// The loading finished since the screens were told.
    level_became_ready: bool,
    /// The portal's glow and spin, the vignette's brightness.
    effects: effects::EffectState,
    /// This frame's screen effects, drawn with the HUD.
    effects_frame: melon_gui::effects::ScreenEffects,
    player: PlayerState,
    /// What `LocalPlayer` keeps of its body: hurt, death, frost, eyes.
    body: vitals::OwnBody,
    inventory: Inventory,
    status: PlayerStatus,
    /// The game mode and the one before it.
    game_mode: LocalGameMode,
    /// `EntityEvent.PERMISSION_LEVEL_*`: 0 (none) to 4 (owner).
    permission_level: u8,
    /// The player list, by profile id.
    players: HashMap<uuid::Uuid, PlayerEntry>,
    /// Objectives, scores and teams.
    scoreboard: melon_world::scoreboard::Scoreboard,
    /// The player list's header, footer and hearts.
    tab_list: melon_gui::hud::player_tab::PlayerTabOverlay,
    /// The server checks accounts: skins in the list are real.
    online_mode: bool,
    /// The own profile id, which picks the skin.
    uuid: uuid::Uuid,
    /// The player drawn for the inventory's box.
    preview: entity_preview::PreviewTextures,
    chat: Chat,
    /// The server's commands, for completing them in the chat.
    commands: melon_net::play::chat::commands::CommandTree,
    /// The registries' ids, for completing command arguments.
    registries: melon_net::play::chat::command_suggestions::Registries,
    /// Saved creative hotbars.
    hotbars: Hotbars,
    menus: overlay::Menus,
    hud: Hud,
    /// A spectator's hotbar menu.
    spectator_menu: melon_gui::overlay::spectator_menu::SpectatorMenu,
    /// `LocalPlayer.postEffects`: the chains the server asked for, gone with the player.
    post_effects: Vec<String>,
    /// The open inventory, chat or switcher, if any. The world keeps running.
    overlay: Option<Box<dyn melon_gui::overlay::Overlay>>,
    /// Real time not yet spent on whole ticks.
    tick_time: f32,
    /// While the game is paused: the partial tick it was paused at
    /// (`DeltaTracker.pausedDeltaTickResidual`).
    paused_at: Option<f32>,
    /// What the server last heard of the keys, sprinting and position.
    sent: position_report::LastSent,
    /// What riding makes of the keys.
    ride: riding::RideState,
    keys: Keys,
    camera_mode: CameraMode,
    fov: camera::FovState,
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
    entities: melon_world::entity::tracker::Entities,
    block_entity_meshes: BlockEntityMeshes,
    entity_meshes: entities::EntityMeshes,
    /// Last frame's entity draws, refilled by the next (see [`Self::recycle`]).
    spare_entities: Vec<EntityDraw>,
    /// The same for the particle draws.
    spare_particles: Vec<melon_render::ParticleDraw>,
    /// `ClientClockManager`: day/night from `SET_TIME`.
    clocks: ClientClocks,
    /// Cracks on blocks being broken, the own and others'.
    cracks: block_cracks::BlockCracks,
    /// The particles, and what draws them.
    particles: particles::Particles,
    /// The outline of the block looked at.
    outline: outline::BlockOutline,
    /// Ticks since joining, for when cracks were last heard of.
    ticks: u64,
    /// Last `RAIN_LEVEL_CHANGE` / `THUNDER_LEVEL_CHANGE`.
    weather: melon_world::weather::WeatherLevels,
    /// The world border and the other settings the server sends for the level.
    border: melon_world::border::WorldBorder,
    border_mesh: border_mesh::BorderMesh,
    /// What the slower F3 + F6 world renderers keep between frames.
    visualizers: debug::visualizers::VisualizerCache,
    /// `DebugEntryChunkGeneration`'s lines and the block they are for.
    chunk_generation: std::cell::RefCell<Option<([i32; 3], Vec<String>)>>,
    /// The recipes the server unlocked, and what its machines accept.
    recipes: melon_world::recipes::RecipeBook,
    recipe_sets: melon_world::recipes::RecipeSets,
    level_settings: melon_world::level_settings::LevelSettings,
    /// `BlockStatePredictionHandler`.
    predictions: prediction::Predictions,
    /// The attack and use keys' delays and the block being broken.
    interaction: interaction::Interaction,
    /// Where the camera was this frame, for the name tags the HUD layer draws.
    name_view: Option<nametags::NameView>,
    /// `Hud.isHidden`: F1 hid the HUD and the hand.
    hud_hidden: bool,
    /// The sounds made this frame, the footsteps and the ears.
    audio: sounds::SoundState,
    /// The plugin channels the server registered (`melon:swap`).
    server_channels: Vec<String>,
    /// `Minecraft.hitResult`.
    hit_result: Option<melon_world::pick::Hit>,
    /// Haste, Conduit Power and Mining Fatigue on the player.
    mining_effects: melon_world::player::mining::MiningEffects,
    /// Every status effect on the player, for the HUD, the inventory and the movement.
    status_effects: melon_world::player::status_effects::ActiveEffects,
    /// The hands: swing, item in use, first-person heights.
    hands: melon_world::player::hands::Hands,
    /// F3's lines on the connection, sound, server and threads.
    subsystems: subsystems::SubsystemMeter,
    /// `--bench`: plays in the player's place.
    bench: Option<bench::BenchDriver>,
}

/// The frame's numbers the F3 overlay shows besides the world's own.
pub struct DebugFrame {
    pub render: RenderStats,
    pub clouds: melon_world::clouds::CloudStatus,
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
    /// The player was sent to another dimension.
    pub loading: Option<LoadingReason>,
    /// The level the loading screen waited for is ready.
    pub loaded: bool,
    /// The server ended a reconfiguration with a new world.
    pub rejoined: Option<(Box<melon_net::JoinInfo>, Vec<melon_net::WorldEvent>)>,
}

/// What vanilla's `Camera` and `Hud` carry from one world into the next,
/// since they outlive worlds. The app keeps it between them.
#[derive(Clone, Copy, Debug)]
pub struct Lasting {
    pub fov: camera::FovState,
    /// `Hud.vignetteBrightness`: `1.0` at game start, so the first world
    /// fades in from a dark vignette.
    pub vignette: f32,
}

impl Default for Lasting {
    fn default() -> Self {
        Self {
            fov: camera::FovState::default(),
            vignette: 1.0,
        }
    }
}

impl InWorld {
    /// `game_dir` holds `hotbar.nbt` and `command_history.txt`.
    /// `lasting` is what the last world left.
    pub fn enter(mut joined: Joined, game_dir: &Path, lasting: Lasting) -> Self {
        let (debug_view, debug_visible) = debug::view::DebugView::load(game_dir);
        let mut debug =
            DebugOverlay::new(melon_gui::hud::debug_entries::DebugEntries::load(game_dir));
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
        meshes.set_height(dimension_type.height);
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
            melon_net::play::SWAP_CHANNEL,
        ]);
        Self {
            uuid: info.uuid,
            preview: Default::default(),
            joined,
            level,
            meshes,
            level_info: level_info::LevelInfo {
                dimension,
                dimension_types,
                dimension_type,
                biomes,
                hashed_seed,
            },
            ambient: Default::default(),
            fog: Default::default(),
            sky: Default::default(),
            body: Default::default(),
            fov: lasting.fov,
            interaction: Default::default(),
            audio: Default::default(),
            loading: Some(loading::LevelLoad::new()),
            level_became_ready: false,
            effects: effects::EffectState::with_vignette(lasting.vignette),
            effects_frame: Default::default(),
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
            menus: Default::default(),
            hud: Hud::default(),
            spectator_menu: Default::default(),
            post_effects: Vec::new(),
            overlay: None,
            tick_time: 0.0,
            paused_at: None,
            sent: Default::default(),
            ride: riding::RideState::default(),
            keys: Keys::default(),
            camera_mode: CameraMode::default(),
            debug,
            debug_view,
            fps: FpsCounter::new(),
            profiler: profiler::FrameProfiler::default(),
            occlusion: OcclusionCache::default(),
            block_entities: BlockEntities::default(),
            entities: Default::default(),
            block_entity_meshes: BlockEntityMeshes::default(),
            spare_entities: Vec::new(),
            spare_particles: Vec::new(),
            entity_meshes: Default::default(),
            clocks: ClientClocks::default(),
            cracks: Default::default(),
            particles: Default::default(),
            outline: Default::default(),
            ticks: 0,
            weather: melon_world::weather::WeatherLevels::default(),
            border: Default::default(),
            border_mesh: Default::default(),
            visualizers: Default::default(),
            chunk_generation: Default::default(),
            recipes: melon_world::recipes::RecipeBook::with_layer(
                melon_world::recipes::layers::current(),
            ),
            recipe_sets: Default::default(),
            level_settings: Default::default(),
            predictions: Default::default(),
            name_view: None,
            hud_hidden: false,
            server_channels: Vec::new(),
            hit_result: None,
            mining_effects: Default::default(),
            status_effects: Default::default(),
            hands: Default::default(),
            subsystems: Default::default(),
            bench: None,
        }
    }

    /// Whether this is a singleplayer world on the integrated server.
    /// What the next world starts from.
    pub fn lasting(&self) -> Lasting {
        Lasting {
            fov: self.fov,
            vignette: self.effects.vignette(),
        }
    }

    pub fn is_integrated(&self) -> bool {
        self.joined.is_integrated()
    }

    /// The integrated server, in a singleplayer world.
    pub fn integrated_server(&self) -> Option<&melon_net::singleplayer::server::IntegratedServer> {
        self.joined.server.as_ref()
    }

    /// [`Self::integrated_server`], to change it.
    pub fn integrated_server_mut(
        &mut self,
    ) -> Option<&mut melon_net::singleplayer::server::IntegratedServer> {
        self.joined.server.as_mut()
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

    /// The integrated world's `icon.png` path while the world has none and the view is ready
    /// for one.
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
    pub fn net_stats(&self) -> &melon_net::play::stats::NetStats {
        self.joined.session.stats()
    }

    /// Pumpkin's world of the dimension the player is in, in singleplayer.
    pub(super) fn server_world(&self) -> Option<std::sync::Arc<pumpkin::world::World>> {
        self.joined
            .server
            .as_ref()?
            .world(&self.level_info.dimension)
    }

    /// The integrated server's ms per tick, in a singleplayer world.
    pub fn server_mspt(&self) -> Option<f64> {
        self.joined.server.as_ref().and_then(|server| server.mspt())
    }

    /// Renews F3's subsystem lines while F3 is open.
    /// The sound channels in use (`AudioEngine::channels_used`), for F3.
    pub fn set_sound_channels(&mut self, used: [u32; 2]) {
        self.audio.channels = used;
    }

    /// Whether any F3 entry shows now.
    pub fn debug_shown(&self) -> bool {
        self.debug.entries().shows_anything()
    }

    /// Whether F3 shows the sound cache, which asks the engine for it.
    pub fn debug_extended(&self) -> bool {
        self.debug
            .entries()
            .is_enabled(melon_gui::hud::debug_entries::Entry::SoundCache)
    }

    /// `AudioEngine::cache_stats`, for F3.
    pub fn set_sound_cache(&mut self, cache: (usize, usize)) {
        self.audio.cache = cache;
    }

    pub fn sample_subsystems(&mut self, audio: &melon_audio::stats::AudioStats) {
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
    pub fn send_client_info(&self, info: &melon_net::ClientInfo) {
        self.joined.session.send_client_information(info);
    }

    /// The mouse moved while the cursor is grabbed: the player looks around.
    pub fn look(&mut self, dx: f32, dy: f32, sensitivity: f64, invert_y: bool) {
        let scale = melon_settings::input::look_scale(sensitivity);
        let dy = if invert_y { -dy } else { dy };
        self.player.look(dx * scale, dy * scale);
    }

    /// The chat options changed.
    pub fn set_chat_settings(
        &mut self,
        settings: &melon_settings::options::ChatSettings,
        font: &Font,
    ) {
        self.chat.set_settings(settings, font);
    }

    /// The "Smooth Lighting" option; changing it remeshes everything.
    pub fn set_smooth_lighting(&mut self, on: bool) {
        self.level.set_smooth_lighting(on);
    }

    /// The "See-Through Leaves" option; changing it remeshes everything.
    pub fn set_cutout_leaves(&mut self, on: bool) {
        self.level.set_cutout_leaves(on);
    }

    /// The "Chunk Builder" option.
    pub fn set_chunk_builder(&mut self, builder: melon_settings::options::ChunkBuilder) {
        self.level.set_chunk_builder(builder);
    }

    /// Colour maps, the biome registry and biome blend, for terrain tints.
    pub fn set_tints(&mut self, maps: std::sync::Arc<melon_world::colormap::ColorMaps>, blend: u8) {
        self.level.set_tints(TintSettings::new(
            &self.level_info.biomes,
            &maps,
            blend,
            Some(self.level_info.hashed_seed),
        ));
    }

    /// `Options.biomeBlendRadius`; changing it remeshes everything.
    pub fn set_biome_blend(&mut self, radius: u8) {
        self.level.set_biome_blend(radius);
    }

    /// Builds the sections the Chunk Builder wants for this frame and takes the finished
    /// meshes.
    pub fn mesh(&mut self, renderer: &mut Renderer, assets: &WorldResources) {
        let eye = self.player.tick_eye();
        let finished = self.level.mesh_pending(&assets.models, assets.fluids, eye);
        self.meshes.upload(renderer, finished);
    }

    /// `ClientPacketListener.handleRespawn`: after death or a portal the
    ///
    /// Every respawn waits for the level again (`setClientLoaded(false)`,
    /// `startWaitingForNewLevel`), so `ServerboundPlayerLoadedPacket` follows
    /// each one, also into the same dimension. Only another dimension makes
    /// a new `ClientLevel`, with the packet's `BiomeManager` seed.
    pub(super) fn respawn_in(
        &mut self,
        renderer: &mut Renderer,
        dimension_id: i32,
        dimension_name: String,
        hashed_seed: i64,
    ) {
        self.loading = Some(loading::LevelLoad::new());
        self.level_became_ready = false;
        if dimension_name == self.level_info.dimension {
            return;
        }
        self.release(renderer);
        self.level_info.dimension_type = self.level_info.dimension_types.get(dimension_id);
        self.level_info.dimension = dimension_name;
        self.level.set_dimension(&self.level_info.dimension_type);
        self.meshes
            .set_height(self.level_info.dimension_type.height);
        self.block_entities = BlockEntities::default();
        self.border = Default::default();
        self.entities.clear();
        self.occlusion = OcclusionCache::default();
        // `Minecraft.setLevel`: the sounds of the old level stop.
        self.audio.stop_all();
        // The new `ClientLevel`'s own `TickRateManager`.
        self.level_settings.new_level();
        // A new `ClientLevel`, with its own prediction handler and rain timer.
        self.predictions = Default::default();
        self.ambient.new_level();
        self.level_info.hashed_seed = hashed_seed;
        self.level.set_zoom_seed(hashed_seed);
    }

    /// Frees what the world uploaded. Call when leaving it.
    pub fn reload_resources(
        &mut self,
        renderer: &mut Renderer,
        colormaps: std::sync::Arc<melon_world::colormap::ColorMaps>,
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

    /// The same connection in a new world (`WorldEvent::Rejoined`).
    pub fn rejoin(
        mut self,
        renderer: &mut Renderer,
        info: melon_net::JoinInfo,
        events: Vec<melon_net::WorldEvent>,
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
        self.entity_meshes.clear(renderer);
        self.border_mesh.clear(renderer);
        self.sky.clear(renderer);
        self.entities.clear();
        self.cracks.clear(renderer);
        self.clear_particles(renderer);
        self.outline.clear(renderer);
        self.preview.release(renderer);
    }

    /// What this frame is cleared to.
    pub fn sky_clear_color(&self) -> [f32; 4] {
        self.fog.clear_color
    }

    /// Why the world ended on its own, if it did.
    pub fn ended(&mut self) -> Option<String> {
        if let Some(reason) = self.joined.session.ended() {
            return Some(reason.to_owned());
        }
        self.joined
            .server
            .as_ref()
            .is_some_and(melon_net::singleplayer::server::IntegratedServer::is_finished)
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
