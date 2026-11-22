//! What the camera sees: sections, block entities, entities, leads, heads.

use glam::IVec3;

use super::view::FrameView;
use crate::game::InWorld;
use crate::game::level_meshes::VisibleChunks;
use crate::game::world_resources::WorldResources;
use crate::renderer::{MeshId, Renderer};
use crate::settings::options::Config;
use crate::world::culling::{Culler, occlusion};
use crate::world::dimension::Skybox;
use crate::world::entity::leash::Lead;
use crate::world::fog::FogType;

/// Whether cave culling uses vanilla's smart cull of distant sections;
/// `MELON_NO_SMART_CULL` turns it off.
fn smart_cull() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("MELON_NO_SMART_CULL").is_none())
}

/// What survived culling this frame.
pub(super) struct Culled {
    pub chunks: VisibleChunks,
    pub block_entities: [Option<(MeshId, IVec3)>; 2],
    pub entities: Vec<i32>,
    pub leads: Vec<(i32, Lead)>,
    pub heads: Vec<[i32; 3]>,
    /// The own body is in the frustum (it is only drawn in third person).
    pub own_visible: bool,
}

impl InWorld {
    /// Culls the level against the camera, fog and cave occlusion.
    pub(super) fn cull(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        view: &FrameView,
        aspect: f32,
        config: &Config,
    ) -> Culled {
        let camera = &view.camera;
        let (min_y, height) = self.level.height();
        let mut cull = config.cull;
        if view.spectator {
            let p = camera.position.floor().as_ivec3();
            let inside = self
                .level
                .block_state(p.x, p.y, p.z)
                .is_some_and(crate::world::block::state_flags::is_drawn);
            cull.occlusion &= !inside;
        }
        let culler = Culler::new(
            camera.frustum_with_fov(aspect, camera.fov.max(f32::from(config.fov))),
            None,
            camera.position,
            (min_y, height),
            cull,
        );
        let look = &view.look;
        let culler =
            if look.fog_type != FogType::Atmospheric || look.atmosphere.skybox == Skybox::None {
                culler.with_fog(look.fog.environmental_end)
            } else {
                culler
            };
        let section = culler.section_of(camera.position);
        let view_distance = i32::from(config.render_distance);
        let level = &self.level;
        let eye = smart_cull().then_some(occlusion::Eye {
            position: camera.position,
            min_y,
        });
        let traced = self
            .debug
            .entries()
            .is_enabled(crate::gui::hud::debug_entries::Entry::ChunkSectionPaths);
        let mask = self
            .occlusion
            .update(section, level.revision(), view_distance, traced, || {
                level
                    .occlusion_job(section, view_distance, eye)
                    .traced(traced)
            });
        let culler = culler.with_occlusion(mask);
        self.meshes.resort(renderer, camera.position);
        let chunks = self.meshes.visible_sections(&culler);

        let block_entities = self.block_entity_meshes.update(
            renderer,
            &self.block_entities,
            &self.level,
            &assets.models,
            &culler,
            view.level_partial,
        );
        let entities = crate::game::entities::visible_entities(
            &self.entities,
            &culler,
            view.partial,
            |pos| self.meshes.section_compiled(pos),
        );
        let leads = crate::game::leads::visible_leads(
            &self.entities,
            &self.level,
            self.joined.session.info.entity_id,
            self.player.rope_hold(config.main_hand),
            &culler,
            view.partial,
        );
        let heads = crate::game::heads::visible_heads(&self.level, &culler);
        let own_visible = {
            let (min, max) = self.player.render_bounds();
            culler.sees_entity(min, max)
        };
        Culled {
            chunks,
            block_entities,
            entities,
            leads,
            heads,
            own_visible,
        }
    }
}
