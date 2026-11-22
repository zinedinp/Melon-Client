//! Leads and the knots on fences (`LeashFeatureRenderer`,

use std::collections::HashMap;

use glam::{IVec3, Mat4, Vec3};

use super::InWorld;
use super::entity_models::DrawStyle;
use super::world_resources::WorldResources;
use crate::math::ChunkHash;
use crate::renderer::{EntityDraw, Glint, MeshId, RenderLayer, Renderer};
use crate::world::culling::Culler;
use crate::world::entity::kind::EntityKind;
use crate::world::entity::leash::{self, Lead};
use crate::world::entity::mobs::{ModelId, leash_knot};
use crate::world::entity::player::Arm;
use crate::world::entity::tracker::{ClientEntity, Entities};
use crate::world::level::Level;
use crate::world::lightmap::LightmapParams;

struct Kept {
    mesh: Option<MeshId>,
    origin: IVec3,
    lead: Lead,
    /// Drawn this frame; the others are freed by [`LeadMeshes::end_frame`].
    seen: bool,
}

/// The leads drawn last frame, by the leashed entity's id: about 60 bytes
#[derive(Default)]
pub struct LeadMeshes {
    by_entity: HashMap<i32, Kept, ChunkHash>,
}

impl LeadMeshes {
    fn draw(
        &mut self,
        renderer: &mut Renderer,
        entity: i32,
        lead: Lead,
        white: crate::renderer::TextureId,
        out: &mut Vec<EntityDraw>,
    ) {
        let kept = self.by_entity.entry(entity).or_insert_with(|| Kept {
            mesh: None,
            origin: IVec3::ZERO,
            lead,
            seen: false,
        });
        kept.seen = true;
        if kept.mesh.is_none() || kept.lead != lead {
            if let Some(old) = kept.mesh.take() {
                renderer.free_mesh(old);
            }
            kept.lead = lead;
            kept.origin = lead.start.floor().as_ivec3();
            match renderer.upload_mesh(&lead.quads(kept.origin.as_vec3())) {
                Ok(mesh) => kept.mesh = Some(mesh),
                Err(e) => eprintln!("Leinen-Mesh fehlgeschlagen: {e:#}"),
            }
        }
        if let Some(mesh) = kept.mesh {
            out.push(EntityDraw {
                alpha: 1.0,
                mesh,
                atlas: white,
                model: Mat4::IDENTITY,
                origin: kept.origin,
                layer: RenderLayer::Entities,
                blend: crate::renderer::Blending::Opaque,
                light: None,
                overlay: 0.0,
                glint: Glint::None,
            });
        }
    }

    /// Frees the leads not drawn since the last call.
    pub fn end_frame(&mut self, renderer: &mut Renderer) {
        self.by_entity.retain(|_, kept| {
            let keep = std::mem::take(&mut kept.seen);
            if !keep && let Some(mesh) = kept.mesh.take() {
                renderer.free_mesh(mesh);
            }
            keep
        });
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        for kept in self.by_entity.drain().map(|(_, k)| k) {
            if let Some(mesh) = kept.mesh {
                renderer.free_mesh(mesh);
            }
        }
    }
}

impl InWorld {
    /// The knots in `visible` and the leads from [`visible_leads`].
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lead_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        visible: &[i32],
        leads: Vec<(i32, Lead)>,
        partial: f32,
        params: &LightmapParams,
        out: &mut Vec<EntityDraw>,
    ) {
        for knot in visible
            .iter()
            .filter_map(|&id| self.entities.get(id))
            .filter(|e| e.kind == EntityKind::LeashKnot)
        {
            let light = self.light_at_eye(knot.eye(partial), 0, params);
            submit_knot(assets, knot, partial, light, out);
        }
        let white = renderer.white_texture();
        for (entity, lead) in leads {
            self.entity_meshes
                .leads
                .draw(renderer, entity, lead, white, out);
        }
    }
}

/// Every lead whose ends, or the space between, the camera may see
pub(super) fn visible_leads(
    entities: &Entities,
    level: &Level,
    own: i32,
    own_hold: Vec3,
    culler: &Culler,
    partial: f32,
) -> Vec<(i32, Lead)> {
    let light = |at: Vec3| {
        let at = at.floor().as_ivec3();
        level.light_at(at.x, at.y, at.z)
    };
    let mut out = Vec::new();
    for entity in entities.iter() {
        let Some(holder) = entity.leash_holder else {
            continue;
        };
        let end = if holder == own {
            own_hold
        } else if let Some(holder) = entities.get(holder) {
            rope_hold(holder, partial)
        } else {
            continue;
        };
        let feet = entity.pos(partial).as_vec3();
        let (width, _) = entity.size();
        let scale = if entity.appearance.baby { 0.5 } else { 1.0 };
        let start = leash::tied_at(
            feet,
            entity.body_yaw(partial),
            entity.ty.eye_height * scale,
            width,
        );
        if !culler.sees_entity(start.min(end).as_dvec3(), start.max(end).as_dvec3()) {
            continue;
        }
        let lead = Lead {
            start,
            end,
            start_light: light(entity.eye(partial)),
            end_light: light(end),
        };
        out.push((entity.id, lead));
    }
    out
}

/// `getRopeHoldPosition` of a holder the tracker knows.
fn rope_hold(holder: &ClientEntity, partial: f32) -> Vec3 {
    let feet = holder.pos(partial).as_vec3();
    match holder.kind {
        EntityKind::LeashKnot => leash::knot_hold(feet),
        EntityKind::Player => {
            let crouching = holder.appearance.crouching();
            let (_, height) = holder.size();
            let height = if crouching { 1.5 } else { height };
            let right = holder.appearance.main_arm(EntityKind::Player) == Arm::Right;
            leash::player_hold(feet, holder.body_yaw(partial), right, crouching, height)
        }
        _ => leash::entity_hold(feet, holder.ty.eye_height),
    }
}

/// `LeashKnotRenderer.submit`: the knot flipped upright where it hangs.
fn submit_knot(
    assets: &WorldResources,
    knot: &ClientEntity,
    partial: f32,
    light: [f32; 3],
    out: &mut Vec<EntityDraw>,
) {
    let Some(texture) = assets.mobs.texture(leash_knot::TEXTURE) else {
        return;
    };
    let pos = knot.pos(partial).as_vec3();
    let origin = pos.floor().as_ivec3();
    let root = Mat4::from_translation(pos - origin.as_vec3())
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0));
    let (mob, meshes) = assets.mobs.get(ModelId::LeashKnot);
    let matrices = mob.model.part_matrices(&mob.model.initial_pose(), root);
    meshes.draws(
        &matrices,
        origin,
        DrawStyle {
            texture,
            layer: RenderLayer::Entities,
            light: Some(light),
            overlay: 0.0,
            glint: Glint::None,
        },
        out,
    );
}
