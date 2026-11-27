//! One frame.

use std::time::{Duration, Instant};

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use super::RunningState;
use crate::game::{DebugFrame, InWorld};
use melon_gui::hud::item_slot::ItemPainter;
use melon_gui::widgets::Ui;
use melon_render::{Blur, Frame};

/// `Options.menuBackgroundBlurriness`'s default.
const MENU_BACKGROUND_BLURRINESS: f32 = 5.0;

impl RunningState {
    pub(super) fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        melon_core::probe::flush_if_due();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame);
        self.last_frame = now;
        self.env.gui_assets.sync_font(&mut self.renderer);
        if let Some(resources) = &mut self.world_resources {
            crate::game::sync_head_objects(&self.env.gui_assets.font, &mut resources.players);
        }

        if let Some(action) = self.screens.poll() {
            self.apply(action, event_loop);
        }
        if let Some(reason) = self.world.as_mut().and_then(InWorld::ended) {
            let reason = if reason == melon_net::play::TIMEOUT_REASON {
                self.env.lang.get("disconnect.timeout").to_owned()
            } else {
                reason
            };
            self.leave_world(Some(reason));
        }
        self.poll_server_packs();
        self.poll_dialogs();
        self.report_screenshots();
        self.pause_if_inactive();

        if self.world.is_some() {
            if self.draw_world(now, dt.as_secs_f32()) {
                self.finish_bench(event_loop);
                return;
            }
        } else {
            self.draw_menus(dt);
        }
        if self.device_lost {
            // Leaving saves a singleplayer world; a renderer on a lost
            // device would only fail again every frame.
            self.leave_world(None);
            event_loop.exit();
            return;
        }
        self.sync_cursor();
        self.sync_ime();
        self.sync_ime_area();
        match self.framerate_limit() {
            None => self.window.request_redraw(),
            Some(fps) => {
                let next = now + Duration::from_secs_f64(1.0 / f64::from(fps));
                self.next_frame = Some(next);
                event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            }
        }
    }

    /// `Screenshot.grab`'s callback: each finished save in the chat
    /// (`showDebugChat`), the file name a link to it.
    fn report_screenshots(&mut self) {
        for saved in self.screenshots.finished() {
            let lang = &self.env.lang;
            let text = match &saved {
                Ok(file) => super::screenshot::saved_message(lang, file),
                Err(e) => {
                    melon_gui::text::StyledText::plain(&lang.format("screenshot.failure", &[e]))
                }
            };
            match &mut self.world {
                Some(world) => world.show_client_message(text, &self.env.gui_assets.font),
                None => match saved {
                    Ok(file) => melon_base::info!("Screenshot: {}", file.display()),
                    Err(e) => melon_base::warn!("Screenshot not saved: {e}"),
                },
            }
        }
    }

    /// The top screen over the panorama, or over the gradient without one.
    fn draw_menus(&mut self, dt: Duration) {
        if let Some(background) = &mut self.background {
            background.tick(dt);
        }
        let scene = self.background.as_ref().map(|b| b.scene());
        if let Some(screen) = self.screens.top_mut() {
            screen.tick();
            let renderer = &mut self.renderer;
            screen.upload_images(&mut |image| renderer.load_texture(image).ok());
        }

        let scale = self.scale();
        let size = scale.virtual_size();
        self.draw_list.clear();
        if scene.is_none() {
            melon_gui::background::push_gradient(
                &mut self.draw_list,
                self.env.gui_assets.white,
                size.0,
                size.1,
            );
        }
        let mut blur = None;
        if let Some(screen) = self.screens.top() {
            let env = &self.env;
            blur = menu_blur(screen, &mut self.draw_list, scene.is_some());
            let mut ui = Ui::new(&mut self.draw_list, &env.gui_assets, &env.lang, size);
            screen.draw(&mut ui, &env.ctx(scale));
        }

        if self.screenshots.begin_frame(true) {
            self.renderer.request_capture();
        }
        let frame = Frame {
            scene: scene.as_ref(),
            ui: &self.draw_list,
            ui_size: scale.projection_size(),
            clear_color: melon_gui::background::CLEAR_COLOR,
            blur,
            pictures: &[],
        };
        if let Err(e) = self.renderer.render(&frame) {
            self.device_lost |= render_failed(&e);
        }
        let renderer = &mut self.renderer;
        self.screenshots
            .end_frame(self.env.assets.game_dir(), || renderer.take_capture());
    }

    /// The benchmark is done: its results, the world saved, the game ends.
    fn finish_bench(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(run) = self.bench.take() {
            run.finish();
        }
        if let Some(mut world) = self.world.take() {
            world.release(&mut self.renderer);
            drop(world);
        }
        event_loop.exit();
    }

    /// Draws a frame of the world. Whether the benchmark has finished.
    fn draw_world(&mut self, now: Instant, dt: f32) -> bool {
        self.update_pause();
        let blocked = self.paused() || self.overlay_open();
        let framerate_limit = self.framerate_limit();
        let scale = self.scale();
        let size = scale.virtual_size();
        let env = &self.env;
        let received = match &mut self.world {
            Some(world) => {
                world.profiler.begin_at(now);
                world.receive(&mut self.renderer, &env.lang, &env.gui_assets.font)
            }
            None => return false,
        };
        self.after_receive(received);
        let env = &self.env;
        let (Some(world), Some(world_resources)) = (&mut self.world, &mut self.world_resources)
        else {
            return false;
        };
        world.profiler.mark("receive");
        let paused = self.game_paused;
        let ticks = world.advance(dt, blocked, paused, &env.world_context(size));
        let ears = world.listener();
        env.audio.set_listener(ears.position, ears.forward, ears.up);
        world.sample_subsystems(env.audio.stats());
        world.set_sound_channels(env.audio.channels_used());
        if world.debug_extended() {
            world.set_sound_cache(env.audio.cache_stats());
        }
        env.audio.set_subtitles(env.config.show_subtitles);
        if world.take_stop_all() {
            env.audio.stop_all();
        }
        for sound in world.take_sounds() {
            env.audio.play(&sound);
        }
        world.hear(env.audio.take_heard(), &env.lang);
        world_resources.animate(&mut self.renderer, ticks);
        if let Some(screen) = self.screens.top_mut() {
            screen.tick();
            let renderer = &mut self.renderer;
            screen.upload_images(&mut |image| renderer.load_texture(image).ok());
        }
        world.profiler.mark("tick");
        world.mesh(&mut self.renderer, world_resources);
        world.profiler.mark("mesh");
        let screens = &self.screens;
        world_resources.prepare_icons(&mut self.renderer, |visit| {
            world.visit_shown_stacks(visit);
            if let Some(screen) = screens.top() {
                screen.visit_stacks(visit);
            }
        });
        for (stack, using) in world.held_stacks() {
            world_resources.prepare_held(&mut self.renderer, stack, using);
        }
        world.debug_view.gui_scale = scale.scale() as f32;
        world.prepare_entities(&mut self.renderer, world_resources);
        let own = world.profile_id();
        world_resources
            .players
            .request(own, world.profile_textures(own));
        world_resources.players.poll(&mut self.renderer);
        world.prepare_entity_preview(
            &mut self.renderer,
            world_resources,
            &env.config,
            scale.scale(),
        );
        world.profiler.mark("icons");
        let aspect = self.renderer.aspect_ratio();
        {
            let gui = &env.gui_assets;
            world.prepare_text_displays(
                &mut self.renderer,
                ItemPainter {
                    icons: &world_resources.icons,
                    font: &gui.font,
                    font_textures: &gui.font_textures,
                    white: gui.white,
                    tooltip: gui.tooltip,
                },
                &env.lang,
            );
        }
        let (scene, render) = world.scene(
            &mut self.renderer,
            world_resources,
            aspect,
            now,
            &env.config,
            (&env.gui_assets.font, &env.gui_assets.font_textures),
        );
        world.profiler.mark("scene");

        let gui = &env.gui_assets;
        let painter = ItemPainter {
            icons: &world_resources.icons,
            font: &gui.font,
            font_textures: &gui.font_textures,
            white: gui.white,
            tooltip: gui.tooltip,
        };
        let icon_path = if blocked {
            None
        } else {
            self.world_icon.due(world, now)
        };
        self.draw_list.clear();
        world.draw_ui(
            &mut self.draw_list,
            world_resources,
            painter,
            &env.lang,
            &env.config,
            size,
        );
        let physical = self.window.inner_size();
        let density = self.window.scale_factor();
        let refresh_rate = if world.debug_shown() {
            self.window
                .current_monitor()
                .and_then(|m| m.refresh_rate_millihertz())
                .map(melon_gui::hud::debug::refresh_rate_label)
        } else {
            None
        };
        let debug = DebugFrame {
            render,
            clouds: env.config.clouds,
            biome_blend: env.config.biome_blend,
            framerate_limit,
            frame_seconds: dt,
            window: (
                (f64::from(physical.width) / density).round() as u32,
                (f64::from(physical.height) / density).round() as u32,
                density,
            ),
            refresh_rate,
            screen_open: self.screens.top().is_some(),
            gui_scale: scale.scale(),
            debug_scale: {
                let (w, h) = self.renderer.framebuffer_size();
                match env.config.debug_gui_scale {
                    -1 => scale.scale(),
                    0 => melon_gui::screen::calculate_scale(0, w, h) / 2,
                    n => melon_gui::screen::calculate_scale(n as u32, w, h),
                }
            },
        };
        world.draw_debug(&mut self.draw_list, gui, &self.renderer, debug, size.0);
        let mut blur = None;
        if let Some(screen) = self.screens.top() {
            blur = menu_blur(screen, &mut self.draw_list, true);
            let mut ui = Ui::new(&mut self.draw_list, gui, &env.lang, size).with_items(painter);
            screen.draw(&mut ui, &env.ctx(scale));
        }
        world.profiler.mark("ui");
        if icon_path.is_some() {
            self.draw_list.clear();
            self.renderer.request_capture();
        }
        let probe_shot = if icon_path.is_none() {
            melon_core::probe::screenshot_due()
        } else {
            None
        };
        if probe_shot.is_some() {
            self.renderer.request_capture();
        }
        let screenshot_free = icon_path.is_none() && probe_shot.is_none();
        if self.screenshots.begin_frame(screenshot_free) {
            self.renderer.request_capture();
        }

        let frame = Frame {
            scene: Some(&scene),
            ui: &self.draw_list,
            ui_size: scale.projection_size(),
            clear_color: world.sky_clear_color(),
            blur,
            pictures: world.pictures(),
        };
        if let Err(e) = self.renderer.render(&frame) {
            self.device_lost |= render_failed(&e);
        }
        world.recycle(scene);
        if let Some(path) = probe_shot {
            if let Some(image) = self.renderer.take_capture()
                && let Err(e) = image.save(&path)
            {
                melon_base::warn!("Tester: {} not written: {e}", path.display());
            }
        } else if let Some(path) = icon_path
            && let Some(image) = self.renderer.take_capture()
        {
            // Only when the icon asked for the frame: otherwise it is a
            // screenshot's, taken below.
            super::world_icon::save(&path, &image);
        }
        let renderer = &mut self.renderer;
        self.screenshots
            .end_frame(self.env.assets.game_dir(), || renderer.take_capture());
        let waits = self.renderer.last_waits();
        world.profiler.mark_split(
            "render",
            &[
                ("gpu wait", waits.gpu),
                ("acquire wait", waits.acquire),
                ("present", waits.present),
                ("record", waits.record),
                ("record scene", waits.record_scene),
                ("record ui", waits.record_ui),
                ("submit", waits.submit),
                ("gather", waits.gather),
                ("prepare", waits.prepare),
            ],
        );
        world.profiler.end();
        match &mut self.bench {
            Some(run) => run.frame(
                world,
                &self.renderer,
                env.audio.stats(),
                dt * 1000.0,
                render,
            ),
            None => false,
        }
    }
}

/// Reports a frame that failed; whether the device is lost for good.
fn render_failed(e: &anyhow::Error) -> bool {
    if melon_render::is_device_lost(e) {
        melon_base::warn!(
            "Render error: the GPU lost the Vulkan device ({e:#}). \
             Melon leaves the world and quits; `journalctl -k` (Xid) has the cause."
        );
        return true;
    }
    melon_base::warn!("Render error, frame skipped: {e:#}");
    false
}

/// The blur behind `screen`, if it has one.
fn menu_blur(
    screen: &dyn melon_gui::screen::Screen,
    draw_list: &mut melon_render::quad::DrawList,
    has_background: bool,
) -> Option<Blur> {
    (has_background && screen.blurs_background()).then(|| Blur {
        radius: MENU_BACKGROUND_BLURRINESS,
        ui_split: draw_list.seal(),
    })
}
