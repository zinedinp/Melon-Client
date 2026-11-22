//! The game's window and frame loop.

mod bench;
pub mod cli;
mod dialogs;
mod frame;
pub mod input;
mod reload;
pub mod screen_stack;
mod screenshot;
mod settings;
mod world_icon;

use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::assets::Assets;
use crate::audio::AudioEngine;
use crate::game::InWorld;
use crate::game::world_resources::WorldResources;
use crate::gui::assets::GuiAssets;
use crate::gui::panorama::Panorama;
use crate::gui::screen::{GuiScale, ScreenAction, ScreenContext};
use crate::gui::screens::ingame::death::DeathScreen;
use crate::gui::screens::ingame::loading::LevelLoadingScreen;
use crate::gui::screens::ingame::pause::PauseScreen;
use crate::gui::screens::menu::connecting::{ConnectingScreen, disconnected_screen};
use crate::gui::screens::menu::title::TitleScreen;
use crate::i18n::Lang;
use crate::join::{Joined, Target};
use crate::platform::{copy_to_clipboard, open_url};
use crate::renderer::Renderer;
use crate::renderer::quad::DrawList;
use crate::settings::options::Config;
use cli::LaunchOptions;
use input::InputState;
use screen_stack::ScreenStack;

/// Runs the game until its window closes.
pub fn run(options: LaunchOptions, assets: Assets, mut config: Config) -> anyhow::Result<()> {
    if options.bench.is_some() {
        bench::override_config(&mut config);
    }
    let event_loop = EventLoop::new()?;
    let mut app = App {
        state: None,
        startup: Some(Startup {
            assets,
            config,
            options,
        }),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// What the window is opened with, used up once it is.
struct Startup {
    assets: Assets,
    config: Config,
    options: LaunchOptions,
}

struct App {
    state: Option<RunningState>,
    startup: Option<Startup>,
}

/// What every screen and frame reads.
struct Env {
    assets: Assets,
    gui_assets: GuiAssets,
    /// The texture slots `gui_assets` was uploaded to, reused by a reload.
    gui_textures: Vec<crate::renderer::TextureId>,
    lang: Lang,
    audio: AudioEngine,
    /// `options.txt`, as the options screens last changed it.
    config: Config,
    /// Shift, Alt and Control as they are held now.
    modifiers: crate::gui::screen::Modifiers,
}

impl Env {
    /// What input and ticks in the world read from the rest of the client.
    fn world_context(&self, size: (f32, f32)) -> crate::game::controls::WorldContext<'_> {
        crate::game::controls::WorldContext {
            lang: &self.lang,
            config: &self.config,
            font: &self.gui_assets.font,
            size,
        }
    }

    fn ctx(&self, scale: GuiScale) -> ScreenContext<'_> {
        ScreenContext {
            assets: &self.gui_assets,
            lang: &self.lang,
            audio: &self.audio,
            config: &self.config,
            assets_root: self.assets.root(),
            game_dir: self.assets.game_dir(),
            account_dir: self.assets.launcher_dir(),
            scale,
            modifiers: self.modifiers,
        }
    }
}

struct RunningState {
    renderer: Renderer,
    window: Window,
    env: Env,
    screens: ScreenStack,
    /// 3D scene behind the menus. `None` if cubemap textures are missing:
    background: Option<Panorama>,
    /// Uploaded on the first join and kept, since the renderer can't free all of it.
    world_resources: Option<WorldResources>,
    world: Option<InWorld>,
    /// What outlives the worlds, as vanilla's `Camera` and `Hud` do.
    lasting: crate::game::Lasting,
    input: InputState,
    last_frame: Instant,
    /// When the framerate limit lets the next frame start.
    next_frame: Option<Instant>,
    /// The GPU dropped the device (`VK_ERROR_DEVICE_LOST`): nothing can be
    /// drawn any more, the game ends after this frame.
    device_lost: bool,
    /// The UI's quads, cleared and refilled every frame.
    draw_list: DrawList,
    /// `--bench`: the run being recorded.
    bench: Option<bench::BenchRun>,
    /// The singleplayer world's picture for the world list.
    world_icon: world_icon::WorldIcon,
    /// F2's pictures: the one asked for and the ones being written.
    screenshots: screenshot::Screenshots,
    /// The volume sliders' sounds outside a world.
    sound_preview: crate::audio::preview::SoundPreview,
}

impl App {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let Some(Startup {
            assets,
            config,
            options,
        }) = self.startup.take()
        else {
            return Ok(());
        };
        let mut attributes = Window::default_attributes()
            .with_title("Melon")
            .with_inner_size(LogicalSize::new(854.0, 480.0));
        if options.bench.is_some() {
            // Fullscreen from the first frame: the window manager takes its
            // time with a switch made later, and the run would start small.
            let largest = bench::largest_monitor(event_loop.available_monitors());
            attributes =
                attributes.with_fullscreen(Some(winit::window::Fullscreen::Borderless(largest)));
        }
        let window = crate::platform::create_centered_window(event_loop, attributes)?;

        let mut renderer = Renderer::new(&window, options.validation)?;
        reload::activate_packs(&assets, &config);
        reload::apply_gui_shaders(&mut renderer, &assets);
        reload::apply_post_chains(&mut renderer, &assets);
        renderer.track_textures();
        let gui_assets = match GuiAssets::load(&mut renderer, &assets) {
            Ok(gui_assets) => gui_assets,
            Err(e) => {
                crate::warn!("Resource packs not loaded, using the defaults: {e:#}");
                let mut stack = crate::assets::packs::ResourceStack::default();
                stack
                    .packs
                    .extend(crate::assets::packs::repository::discover_mods(
                        assets.game_dir(),
                    ));
                crate::assets::packs::activate(stack);
                crate::world::creative_tabs::apply_from_game_dir(assets.game_dir());
                reload::apply_gui_shaders(&mut renderer, &assets);
                reload::apply_post_chains(&mut renderer, &assets);
                let (failed, _) = renderer.take_tracked();
                failed.into_iter().for_each(|t| renderer.free_texture(t));
                renderer.track_textures();
                GuiAssets::load(&mut renderer, &assets)?
            }
        };
        let (gui_textures, _) = renderer.take_tracked();
        let lang = Lang::load(&assets, config.lang);
        let splashes = crate::gui::text::splash::load_splashes(&assets);
        let audio = AudioEngine::init(
            &assets,
            &config.sound_device,
            config.speaker_layout.channels(),
            config.directional_audio,
        );
        audio.set_volumes(&config.sound_volumes);
        let background = load_panorama(&mut renderer, &assets);

        let mut state = RunningState {
            renderer,
            window,
            env: Env {
                assets,
                gui_assets,
                gui_textures,
                lang,
                audio,
                config,
                modifiers: Default::default(),
            },
            screens: ScreenStack::default(),
            background,
            world_resources: None,
            world: None,
            lasting: Default::default(),
            input: InputState::new(),
            last_frame: Instant::now(),
            next_frame: None,
            device_lost: false,
            draw_list: DrawList::new(),
            bench: None,
            world_icon: world_icon::WorldIcon::new(),
            screenshots: screenshot::Screenshots::default(),
            sound_preview: Default::default(),
        };
        let scale = state.scale();
        let ctx = state.env.ctx(scale);
        let title = TitleScreen::new(&splashes, &ctx);
        state.screens.push(Box::new(title), &ctx);
        if let Some(address) = options.join {
            let connecting = ConnectingScreen::new(Target::Server(address), &ctx);
            state.screens.push(Box::new(connecting), &ctx);
        } else if let Some(bench_options) = options.bench {
            let target = bench::prepare_world(state.env.assets.game_dir(), bench_options.scene)?;
            let connecting = ConnectingScreen::new(target, &ctx);
            state.screens.push(Box::new(connecting), &ctx);
            state.bench = Some(bench::BenchRun::new(bench_options));
        }
        state.apply_display_options();
        if state.bench.is_some() {
            bench::fullscreen_on_largest(&state.window);
        }
        self.state = Some(state);
        Ok(())
    }
}

/// The title screen's panorama, or `None` for the gradient.
fn load_panorama(renderer: &mut Renderer, assets: &Assets) -> Option<Panorama> {
    match Panorama::load(renderer, assets) {
        Ok(Some(panorama)) => Some(panorama),
        Ok(None) => {
            crate::warn!(
                "No panorama (assets/.../title/background/panorama_0..5.png): using the gradient, see notes/Assets.md"
            );
            None
        }
        Err(e) => {
            crate::warn!("Panorama not loaded: {e:#}");
            None
        }
    }
}

impl RunningState {
    /// The window's size and the GUI scale option.
    fn scale(&self) -> GuiScale {
        let size = self.window.inner_size();
        GuiScale::new(self.env.config.gui_scale, size.width, size.height)
    }

    fn gui_size(&self) -> (f32, f32) {
        self.scale().virtual_size()
    }

    fn paused(&self) -> bool {
        self.world.is_some() && !self.screens.is_empty()
    }

    fn overlay_open(&self) -> bool {
        self.world.as_ref().is_some_and(InWorld::overlay_open)
    }

    /// Carries out what a screen asked for.
    fn apply(&mut self, action: ScreenAction, event_loop: &ActiveEventLoop) {
        match action {
            ScreenAction::None => {}
            ScreenAction::Quit => event_loop.exit(),
            ScreenAction::Back => self.close_top_screen(),
            ScreenAction::Open(screen) => {
                let ctx = self.env.ctx(self.scale());
                self.screens.push(screen, &ctx);
            }
            ScreenAction::Replace(screen) => {
                let ctx = self.env.ctx(self.scale());
                self.screens.replace_top(screen, &ctx);
            }
            ScreenAction::SetLanguage(language) => self.set_language(language),
            ScreenAction::ChangeConfig(change) => {
                let mut config = self.env.config.clone();
                change(&mut config);
                self.set_config(config);
            }
            ScreenAction::EnterWorld(joined) => self.enter_world(*joined),
            ScreenAction::LeaveWorld => self.leave_world(None),
            ScreenAction::Respawn => {
                if let Some(world) = &self.world {
                    world.respawn();
                }
            }
            ScreenAction::OpenUrl(url) => {
                open_url(&url);
                self.close_top_screen();
            }
            ScreenAction::Clipboard(text) => copy_to_clipboard(&text),
            ScreenAction::Copy(text) => {
                copy_to_clipboard(&text);
                self.close_top_screen();
            }
            ScreenAction::SetResourcePacks {
                selected,
                incompatible,
            } => {
                self.close_top_screen();
                let changed = selected != self.env.config.resource_packs;
                self.set_config(Config {
                    resource_packs: selected,
                    incompatible_resource_packs: incompatible,
                    ..self.env.config.clone()
                });
                if changed {
                    self.reload_resources();
                }
            }
            ScreenAction::ReloadResources => self.reload_resources(),
            ScreenAction::AnswerServerPacks(accept) => {
                self.close_top_screen();
                self.answer_server_packs(accept);
            }
            ScreenAction::Dialog(request) => self.apply_dialog(request),
        }
    }

    /// Pops one screen.
    fn close_top_screen(&mut self) {
        let scale = self.scale();
        let ctx = self.env.ctx(scale);
        self.screens.close_top(self.world.is_some(), &ctx);
    }

    /// The server's packets this frame changed whether the player is alive:
    fn after_receive(&mut self, mut received: crate::game::Received) {
        if let Some((info, events)) = received.rejoined.take()
            && let Some(world) = self.world.take()
        {
            self.lasting = world.lasting();
            let joined = world.rejoin(&mut self.renderer, *info, events);
            self.enter_world(joined);
        }
        let scale = self.scale();
        let ui = self.env.world_context(scale.virtual_size());
        let Some(world) = &mut self.world else { return };
        if received.died {
            world.close_overlay(&ui);
            let hardcore = world.hardcore();
            let death = DeathScreen::new(hardcore);
            self.screens.clear();
            self.screens.push(Box::new(death), &self.env.ctx(scale));
        } else if received.revived {
            self.screens.clear();
        }
        if let Some(reason) = received.loading {
            let portal = self
                .world_resources
                .as_ref()
                .and_then(|resources| resources.nether_portal_sprite());
            world.close_overlay(&ui);
            self.screens.clear();
            let screen = LevelLoadingScreen::new(reason, portal);
            self.screens.push(Box::new(screen), &self.env.ctx(scale));
        } else if received.loaded && self.screens.top().is_some_and(|s| s.is_level_loading()) {
            self.screens.clear();
        }
    }

    fn enter_world(&mut self, joined: Joined) {
        if self.world_resources.is_none() {
            match WorldResources::load(&mut self.renderer, &self.env.assets) {
                Ok(loaded) => self.world_resources = Some(loaded),
                Err(e) => {
                    self.show_failure(format!("{e:#}"));
                    return;
                }
            }
        }
        let mut world = InWorld::enter(joined, self.env.assets.game_dir(), self.lasting);
        world.set_chat_settings(&self.env.config.chat, &self.env.gui_assets.font);
        world.set_smooth_lighting(self.env.config.smooth_lighting);
        world.set_chunk_builder(self.env.config.chunk_builder);
        if let Some(assets) = &self.world_resources {
            world.set_tints(
                std::sync::Arc::clone(&assets.colormaps),
                self.env.config.biome_blend,
            );
        }
        if let Some(run) = &mut self.bench {
            world.start_bench(self.env.config.render_distance, &run.options);
            run.start(&self.renderer, &self.env.config);
        }
        self.screens.enter_world();
        self.world = Some(world);
        let ctx = self.env.ctx(self.scale());
        let screen = LevelLoadingScreen::new(crate::game::LoadingReason::Other, None);
        self.screens.push(Box::new(screen), &ctx);
    }

    /// Leaves the world for the menus it was joined from, dropping the connection.
    fn leave_world(&mut self, reason: Option<String>) {
        let Some(mut world) = self.world.take() else {
            return;
        };
        crate::assets::packs::server::end();
        crate::dialog::connection::end();
        self.lasting = world.lasting();
        // `Minecraft.disconnect` -> `updateLevelInEngines(null)`: silence.
        self.env.audio.stop_all();
        world.release(&mut self.renderer);
        let integrated = world.is_integrated();
        drop(world);
        self.screens.leave_world(integrated);
        self.resize_gui();
        self.input.release_cursor(&self.window);
        if let Some(reason) = reason {
            self.show_failure(reason);
        } else {
            let scale = self.scale();
            let ctx = self.env.ctx(scale);
            if let Some(top) = self.screens.top_mut() {
                top.on_resume(&ctx);
            }
        }
    }

    fn show_failure(&mut self, reason: String) {
        crate::warn!("Connection lost: {reason}");
        if self.screens.is_empty() {
            return;
        }
        let ctx = self.env.ctx(self.scale());
        let screen = disconnected_screen("disconnect.lost", reason, "gui.toTitle");
        self.screens.push(Box::new(screen), &ctx);
    }

    pub(super) fn open_pause_menu(&mut self) {
        let local_server = self.world.as_ref().is_some_and(InWorld::is_integrated);
        let scale = self.scale();
        let addition = crate::dialog::connection::pause_screen_addition();
        let pause = PauseScreen::new(local_server, addition, &self.env.lang);
        self.screens.push(Box::new(pause), &self.env.ctx(scale));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none()
            && let Err(e) = self.init(event_loop)
        {
            crate::warn!("Start-up failed: {e:#}");
            event_loop.exit();
        }
    }

    /// The framerate limit's wait: the next frame once its time has come.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else { return };
        if let Some(next) = state.next_frame
            && Instant::now() >= next
        {
            state.next_frame = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            state.window.request_redraw();
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let Some(state) = &mut self.state {
            let started = Instant::now();
            state.device_event(event);
            state.count_event(started);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };
        if state.window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => state.redraw(event_loop),
            WindowEvent::Resized(size) => {
                let started = Instant::now();
                if let Err(e) = state.renderer.resize(size.width, size.height) {
                    crate::warn!("Resize failed: {e:#}");
                }
                state.resize_gui();
                state.count_event(started);
            }
            event => {
                let started = Instant::now();
                state.input_event(event, event_loop);
                state.count_event(started);
            }
        }
    }
}
