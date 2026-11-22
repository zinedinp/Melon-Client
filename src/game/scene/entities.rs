//! The entity layer: hands or the own body, entities, block entities and
//! everything else drawn as an [`EntityDraw`].

use glam::{IVec3, Mat4};

use super::view::FrameView;
use crate::game::InWorld;
use crate::game::camera::CameraMode;
use crate::game::entity_models::DrawStyle;
use crate::game::hand_render::HandDraw;
use crate::game::world_resources::WorldResources;
use crate::renderer::{Blending, EntityDraw, Glint, MeshId, RenderLayer, Renderer};
use crate::settings::options::Config;
use crate::world::entity::leash::Lead;
use crate::world::item_model::HandPose;
use crate::world::player::ArmPoses;

/// What the entity layer draws from: the view and what culling left.
pub(super) struct EntityInput<'a> {
    pub view: &'a FrameView,
    pub visible: &'a [i32],
    pub leads: Vec<(i32, Lead)>,
    pub heads: Vec<[i32; 3]>,
    pub block_entities: [Option<(MeshId, IVec3)>; 2],
    pub own_visible: bool,
    pub aspect: f32,
    pub config: &'a Config,
}

/// The entity layer's draws.
pub(super) struct EntityLayer {
    pub draws: Vec<EntityDraw>,
    /// Glowing entities' draws, by index into `draws`, with their outline colour.
    pub glowing: Vec<(usize, [u8; 3])>,
    /// Entities drawn, the own body included.
    pub drawn: usize,
}

impl InWorld {
    pub(super) fn entity_layer(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        input: EntityInput<'_>,
    ) -> EntityLayer {
        let EntityInput {
            view,
            visible,
            leads,
            heads,
            block_entities,
            own_visible,
            aspect,
            config,
        } = input;
        let camera = &view.camera;
        let (partial, params) = (view.partial, &view.lightmap);
        let main_arm = config.main_hand;
        // `Camera.yRot` / `xRot`, which vanilla's billboards turn to.
        let angles = (camera.look().y_rot, camera.look().x_rot);
        let mut draws = std::mem::take(&mut self.spare_entities);
        draws.clear();
        let mut drawn = visible.len();

        if self.camera_mode == CameraMode::FirstPerson {
            // `GameRenderer.renderItemInHand`: not while the HUD is hidden.
            if !view.spectator && !self.hud_hidden {
                self.hand_draws(assets, view, config, &mut draws);
            }
        } else if own_visible {
            drawn += 1;
            self.own_body_draws(renderer, assets, view, config, &mut draws);
        }
        let mut glowing = Vec::new();
        self.entity_draws(
            renderer,
            assets,
            visible,
            camera.position,
            partial,
            params,
            angles,
            &mut draws,
            &mut glowing,
        );
        self.pickup_draws(assets, partial, params, &mut draws);
        self.head_draws(assets, &heads, params, &mut draws);
        self.lead_draws(
            renderer, assets, visible, leads, partial, params, &mut draws,
        );
        self.fishing_draws(
            renderer,
            assets,
            visible,
            &crate::game::fishing::View {
                position: camera.position.as_vec3(),
                angles,
                plane: crate::world::entity::fishing::NearPlane {
                    forward: camera.forward(),
                    left: camera.left(),
                    up: camera.up(),
                    near: crate::renderer::WORLD_Z_NEAR,
                    fov: f32::from(config.fov),
                    aspect,
                },
            },
            main_arm,
            partial,
            params,
            &mut draws,
        );
        self.text_display_draws(
            visible,
            camera.position.as_vec3(),
            angles,
            partial,
            params,
            &mut draws,
        );
        self.border_mesh.draw(
            renderer,
            &self.border,
            camera.position,
            (f64::from(config.render_distance) * 16.0).max(3.0),
            assets.forcefield,
            &mut draws,
        );
        self.entity_meshes.end_frame(renderer);
        draws.extend(
            block_entities
                .into_iter()
                .flatten()
                .map(|(mesh, origin)| EntityDraw {
                    scroll: None,
                    alpha: 1.0,
                    mesh,
                    atlas: assets.atlas,
                    model: Mat4::IDENTITY,
                    origin,
                    layer: RenderLayer::Entities,
                    blend: Blending::Opaque,
                    light: None,
                    overlay: 0.0,
                    glint: Glint::None,
                }),
        );
        self.cracks.draws(
            renderer,
            &self.level,
            &assets.models,
            &assets.destroy_stages,
            &mut draws,
        );
        EntityLayer {
            draws,
            glowing,
            drawn,
        }
    }

    /// First person: the arms and the items they hold.
    fn hand_draws(
        &self,
        assets: &WorldResources,
        view: &FrameView,
        config: &Config,
        out: &mut Vec<EntityDraw>,
    ) {
        let own = assets.players.for_profile(self.uuid);
        let (skin, body) = (own.texture, own.model);
        let camera = &view.camera;
        let origin = camera.position.floor().as_ivec3();
        let eye = (camera.position - origin.as_dvec3()).as_vec3();
        let hands =
            self.first_person_hands(eye, config.main_hand, config.model_parts, view.partial);
        for draw in hands {
            match draw {
                HandDraw::Arm {
                    arm,
                    matrix,
                    sleeve,
                } => {
                    let model = &body.model;
                    let pose = model.first_person_arm_pose(arm, sleeve);
                    let matrices = model.model.subtree_matrices(&pose, model.arm(arm), matrix);
                    body.meshes.draws(
                        &matrices,
                        origin,
                        DrawStyle::new(skin, RenderLayer::Hand),
                        out,
                    );
                }
                HandDraw::Item {
                    stack,
                    arm,
                    matrix,
                    using,
                } => {
                    let pose = HandPose::first_person(arm);
                    for (mesh, atlas, translucent) in assets.held.get(stack, pose, using) {
                        out.push(EntityDraw {
                            scroll: None,
                            alpha: 1.0,
                            mesh,
                            atlas,
                            model: matrix,
                            origin,
                            layer: RenderLayer::Hand,
                            blend: Blending::of(translucent),
                            light: None,
                            overlay: 0.0,
                            glint: Glint::item(
                                crate::world::glint::has_foil(stack),
                                assets.held.glint_texels(atlas),
                            ),
                        });
                    }
                }
            }
        }
    }

    /// Third person: the own body.
    fn own_body_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        view: &FrameView,
        config: &Config,
        out: &mut Vec<EntityDraw>,
    ) {
        let (main_arm, partial) = (config.main_hand, view.partial);
        let (arms, swing) = if view.spectator {
            (ArmPoses::default(), None)
        } else {
            (self.arm_poses(main_arm), self.swing_pose(main_arm, partial))
        };
        let mut state = self
            .player
            .avatar_state(arms, swing, view.spectator, config.model_parts);
        state.death_time = self.own_death_time(partial);
        if self.players.get(&self.uuid).is_some_and(|p| {
            crate::world::entity::player::AvatarRenderState::wants_upside_down(
                &p.name,
                config.model_parts,
            )
        }) {
            state.turn_upside_down(self.player.box_height() as f32);
        }
        if state.death_time > 0.0 {
            state.humanoid.walk_speed = 0.0;
        }
        let light = self.light_at_eye(self.player.tick_eye().as_vec3(), 0, &view.lightmap);
        self.own_avatar_draws(
            renderer,
            assets,
            state,
            main_arm,
            view.camera.position,
            light,
            out,
        );
    }
}
