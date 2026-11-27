//! The 3D frame, gathered step by step: the view, what it sees, then the
//! entity, particle, overlay and sky layers.

mod cull;
mod entities;
mod overlays;
mod particles;
mod sky;
mod view;

pub(super) use overlays::OverlayInput;
pub(super) use view::FrameView;

use super::InWorld;
use super::world_resources::WorldResources;
use melon_gui::hud::debug::RenderStats;
use melon_render::{Renderer, SceneFrame, Sky, WorldDraw};
use melon_settings::options::Config;
use melon_world::lightmap;
use std::time::Instant;

/// The font debug labels are written with, and its pages.
pub type DebugFont<'a> = (
    &'a melon_gui::text::font::Font,
    &'a [melon_render::TextureId],
);

impl InWorld {
    /// This frame's 3D scene: the chunks and entities the camera can see.
    pub fn scene(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        aspect: f32,
        now: Instant,
        config: &Config,
        font: DebugFont<'_>,
    ) -> (SceneFrame, RenderStats) {
        let view = self.frame_view(assets, aspect, now, config);
        self.profiler.mark("view");
        let cull::Culled {
            chunks,
            block_entities,
            entities: visible,
            leads,
            heads,
            own_visible,
        } = self.cull(renderer, assets, &view, aspect, config);
        self.profiler.mark("cull");

        let mut layer = self.entity_layer(
            renderer,
            assets,
            entities::EntityInput {
                view: &view,
                visible: &visible,
                leads,
                heads,
                block_entities,
                own_visible,
                aspect,
                config,
            },
        );
        self.profiler.mark("entities");
        let mut particle_draws = std::mem::take(&mut self.spare_particles);
        particle_draws.clear();
        let particles = self.particle_layer(
            renderer,
            assets,
            &particles::ParticleInput {
                camera: &view.camera,
                aspect,
                partial: view.level_partial,
            },
            &mut particle_draws,
        );
        self.profiler.mark("particles");
        let lines = self.overlay_layer(
            renderer,
            assets,
            &overlays::OverlayInput {
                camera: &view.camera,
                aspect,
                fov_setting: f32::from(config.fov),
                visible: &visible,
                partial: view.partial,
                high_contrast: config.high_contrast_block_outline,
                font,
            },
            &mut layer.draws,
        );
        self.profiler.mark("overlays");
        let sky = self.sky_layer(renderer, assets, &view, config, aspect);
        self.profiler.mark("sky");

        let stats = RenderStats {
            sections_drawn: chunks.count.drawn,
            sections_total: chunks.count.total,
            vertices_drawn: chunks.count.vertices,
            terrain_draws: chunks.count.draws,
            entities_drawn: layer.drawn,
            entities_total: self.entities.len() + 1,
            particles,
        };
        let camera = &view.camera;
        let scene = SceneFrame {
            sky: Some(Sky {
                cubemap: None,
                view: melon_world::sky::rotation_view(camera.bob * camera.view),
                fov_y_degrees: camera.fov,
                celestials: sky.celestials,
            }),
            world: Some(WorldDraw {
                chunks: chunks.solid,
                cutout: chunks.cutout,
                translucent: chunks.translucent,
                atlas: assets.atlas,
                view: camera.view,
                eye: camera.position,
                fov_y_degrees: camera.fov,
                hand_fov_y_degrees: view.hand_fov,
                far: camera.far,
                ambient: {
                    let eye = self.player.tick_eye().floor().as_ivec3();
                    let (block, sky) = self.level.light_at(eye.x, eye.y, eye.z);
                    lightmap::sample(block, sky, &view.lightmap)
                },
                lightmap: view.lightmap,
                fog: view.look.fog,
                cardinal: self.level_info.dimension_type.cardinal_light.relative(),
                distortion: view.distortion,
                bob: camera.bob,
                section_slots: chunks.slots,
            }),
            entities: layer.draws,
            particles: particle_draws,
            clouds: sky.clouds,
            weather: sky.weather,
            lines,
            glint_item: Some(assets.glint.item),
            glint_armor: Some(assets.glint.armor),
            glowing: layer.glowing,
            post_effects: Vec::new(),
        };
        (scene, stats)
    }

    /// Takes back a drawn [`SceneFrame`], so the next frame reuses its buffers.
    pub fn recycle(&mut self, scene: SceneFrame) {
        if let Some(world) = scene.world {
            self.meshes
                .recycle(world.chunks, world.cutout, world.translucent);
        }
        self.spare_entities = scene.entities;
        self.spare_particles = scene.particles;
    }
}
