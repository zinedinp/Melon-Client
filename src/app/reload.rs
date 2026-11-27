//! Reloading every resource when the resource packs change (vanilla
//! `Minecraft.reloadResourcePacks`).

use std::time::Instant;

use super::{RunningState, load_panorama};
use crate::assets::Assets;
use crate::assets::packs::server::{self, Action};
use crate::assets::packs::{self, repository};
use crate::gui::assets::GuiAssets;
use crate::gui::screens::menu::connecting::disconnected_screen;
use crate::gui::screens::packs::PackConfirmScreen;
use crate::i18n::Lang;
use crate::renderer::Renderer;
use crate::settings::options::Config;

/// Reads through the packs `config` selects, with the server's on top.
pub(super) fn activate_packs(assets: &Assets, config: &Config) {
    let available = repository::discover(assets.game_dir());
    let mut stack = repository::stack(&available, &config.resource_packs, &server::active_packs());
    stack
        .packs
        .extend(repository::discover_mods(assets.game_dir()));
    if !stack.packs.is_empty() {
        let ids: Vec<&str> = stack.packs.iter().map(|p| p.id.as_str()).collect();
        crate::info!("Resource packs: {}", ids.join(", "));
    }
    packs::activate(stack);
    crate::world::entity::equipment_asset::install(assets.root());
    crate::world::creative_tabs::apply_from_game_dir(assets.game_dir());
    describe_packs();
}

/// What the active resource packs hold, and which of it Melon has no use for.
fn describe_packs() {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::probe::{enabled, note, section};
    if !enabled() {
        return;
    }
    use crate::assets::packs;
    const USED: &[&str] = &[
        "models",
        "textures",
        "items",
        "blockstates",
        "font",
        "lang",
        "sounds",
        "post_effect",
        "shaders",
        "atlases",
        "particles",
        "texts",
    ];
    let stack = packs::active();
    let mut text = String::new();
    if stack.packs.is_empty() {
        return;
    }
    for pack in &stack.packs {
        text.push_str(&format!("### {} ({})\n\n", pack.title, pack.id));
        let inventory = pack.inventory();
        let mut by_namespace: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
        for ((namespace, folder), n) in &inventory {
            by_namespace
                .entry(namespace)
                .or_default()
                .push((folder, *n));
        }
        for (namespace, folders) in by_namespace {
            let list: Vec<String> = folders.iter().map(|(f, n)| format!("{f} {n}")).collect();
            text.push_str(&format!("- `{namespace}`: {}\n", list.join(", ")));
            for (folder, n) in &folders {
                if !USED.contains(folder) {
                    note("pack.feature", format!("{namespace}/{folder}"), || {
                        format!("{n} files in {}, not read by Melon", pack.id)
                    });
                }
            }
        }
        let mut files = BTreeSet::new();
        pack.list("minecraft", "shaders/core", &mut files);
        for file in &files {
            let name = file.trim_start_matches("shaders/core/");
            let stem = name.split('.').next().unwrap_or(name);
            if !["text", "gui", "position_tex_color"].contains(&stem) {
                note("pack.feature", format!("core shader {name}"), || {
                    format!(
                        "replaces a vanilla world shader in {}; Melon draws the world with its own",
                        pack.id
                    )
                });
            }
        }
        files.clear();
        pack.list("minecraft", "post_effect", &mut files);
        for file in &files {
            let id = file
                .trim_start_matches("post_effect/")
                .trim_end_matches(".json");
            if !crate::renderer::post::load::IDS
                .iter()
                .any(|known| known.trim_start_matches("minecraft:") == id)
            {
                note("pack.feature", format!("post effect {id}"), || {
                    format!("a chain Melon doesn't run, in {}", pack.id)
                });
            }
        }
        text.push('\n');
    }
    section("Packs", text);
}

/// The active packs' replacements for vanilla's GUI shaders, compiled and handed to the
/// renderer.
pub(super) fn apply_gui_shaders(renderer: &mut Renderer, assets: &Assets) {
    let shaders = crate::gui::pack_shaders::compile(assets.root());
    for shader in &shaders {
        crate::info!("GUI shader from a resource pack: {:?}", shader.program);
    }
    for (program, e) in renderer.set_gui_shaders(&shaders) {
        crate::warn!("GUI shader {program:?} unusable: {e:#}");
    }
    match crate::renderer::pack_outline::compile(assets.root()) {
        Ok(Some(shader)) => {
            crate::info!("World shader from a resource pack: rendertype_outline");
            if let Some(e) = renderer.set_outline_pack_shader(Some(&shader)) {
                crate::warn!("World shader rendertype_outline unusable: {e:#}");
            }
        }
        Ok(None) => {
            renderer.set_outline_pack_shader(None);
        }
        Err(e) => {
            crate::warn!("World shader rendertype_outline dropped: {e:#}");
            renderer.set_outline_pack_shader(None);
        }
    }
}

/// The post effect chains the packs have (vanilla's entity outline, and any others).
pub(super) fn apply_post_chains(renderer: &mut Renderer, assets: &Assets) {
    let (chains, blit) = crate::renderer::post::load::load(assets.root());
    for chain in &chains {
        crate::info!("Post effect: {}", chain.id);
    }
    for (id, e) in renderer.set_post_chains(chains, blit) {
        crate::warn!("Post effect {id} unusable: {e:#}");
    }
}

impl RunningState {
    /// Builds the pack stack anew and reloads what is read from it.
    pub(super) fn reload_resources(&mut self) {
        let started = Instant::now();
        activate_packs(&self.env.assets, &self.env.config);
        apply_gui_shaders(&mut self.renderer, &self.env.assets);
        apply_post_chains(&mut self.renderer, &self.env.assets);
        let mut ok = true;

        self.renderer
            .begin_texture_reuse(self.env.gui_textures.clone());
        self.renderer.track_textures();
        let gui = GuiAssets::load(&mut self.renderer, &self.env.assets);
        let (textures, _) = self.renderer.take_tracked();
        match gui {
            Ok(gui) => {
                self.renderer.end_texture_reuse(true);
                self.env.gui_assets = gui;
                self.env.gui_textures = textures;
            }
            Err(e) => {
                crate::warn!("GUI textures not reloaded: {e:#}");
                ok = false;
                let mut kept = textures;
                kept.extend(self.renderer.end_texture_reuse(false));
                self.env.gui_textures = kept;
            }
        }

        self.env.lang = Lang::load(&self.env.assets, self.env.config.lang);
        self.screens.refresh_lang(&self.env.lang);
        if let Some(world) = &mut self.world {
            world.refresh_lang(&self.env.lang);
        }
        self.env.audio.reload_sounds(&self.env.assets);

        if let Some(old) = self.background.take() {
            old.release(&mut self.renderer);
        }
        self.background = load_panorama(&mut self.renderer, &self.env.assets);

        if let Some(resources) = self.world_resources.take() {
            if let Some(world) = &mut self.world {
                match resources.reload(&mut self.renderer, &self.env.assets) {
                    Ok(resources) => {
                        world.reload_resources(
                            &mut self.renderer,
                            std::sync::Arc::clone(&resources.colormaps),
                            self.env.config.biome_blend,
                        );
                        self.world_resources = Some(resources);
                    }
                    Err(failed) => {
                        let (old, e) = *failed;
                        crate::warn!("World resources not reloaded: {e:#}");
                        ok = false;
                        self.world_resources = Some(old);
                    }
                }
            } else {
                resources.release(&mut self.renderer);
            }
        }

        if self.world_resources.is_none() {
            // Baked ahead again, from the packs active now.
            self.pending_bake = super::PendingBake::start(
                self.env.assets.root().to_path_buf(),
                self.renderer.max_texture_side(),
            );
        }
        if let Some(world) = &mut self.world {
            world.set_chat_settings(&self.env.config.chat, &self.env.gui_assets.font);
        }
        self.resize_gui();
        server::reload_finished(ok);
        crate::info!("Resources reloaded in {:.2?}", started.elapsed());
    }

    /// Carries out what the server's resource packs need.
    pub(super) fn poll_server_packs(&mut self) {
        let Some(action) = server::poll() else {
            return;
        };
        let ctx = self.env.ctx(self.scale());
        match action {
            Action::Prompt(request) => {
                let screen = PackConfirmScreen::new(request, &self.env.lang);
                self.screens.push(Box::new(screen), &ctx);
            }
            Action::Reload => self.reload_resources(),
            Action::Disconnect(key) => {
                let reason = self.env.lang.get(key).to_owned();
                if self.world.is_some() {
                    self.leave_world(Some(reason));
                } else {
                    server::close_connection();
                    let screen = disconnected_screen("connect.failed", reason, "gui.back");
                    self.screens.replace_top(Box::new(screen), &ctx);
                }
            }
        }
    }

    /// The player answered the server's pack prompt; a saved server remembers the answer.
    pub(super) fn answer_server_packs(&mut self, accept: bool) {
        if let Some((address, policy)) = server::answer_prompt(accept) {
            crate::settings::server_list::set_accept_textures(
                self.env.assets.game_dir(),
                &address,
                policy.accept(),
            );
        }
    }
}
