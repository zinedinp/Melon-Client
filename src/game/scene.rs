//! The 3D frame: culling, the visible terrain, block entities and the

use super::camera::{Camera, CameraMode};
use super::entity_models::DrawStyle;
use super::hand_render::HandDraw;
use super::world_resources::WorldResources;
use super::{InWorld, camera};
use crate::gui::hud::debug::RenderStats;
use crate::renderer::{EntityDraw, Glint, RenderLayer, Renderer, SceneFrame, Sky, WorldDraw};
use crate::settings::options::Config;
use crate::world::culling::{Culler, occlusion};
use crate::world::dimension::Skybox;
use crate::world::fog::FogType;
use crate::world::item_model::HandPose;
use crate::world::player::ArmPoses;
use crate::world::{lightmap, sky};
use glam::Mat4;
use std::time::Instant;

/// Whether cave culling uses vanilla's smart cull of distant sections:
fn smart_cull() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("MELON_NO_SMART_CULL").is_none())
}

/// `Camera.calculateHudFov`'s start: the hand's own field of view.
const HAND_FOV: f32 = 70.0;

impl InWorld {
    /// This frame's 3D scene: the chunks and entities the camera can see.
    pub fn scene(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        aspect: f32,
        now: Instant,
        config: &Config,
        font: (&crate::gui::text::font::Font, &[crate::renderer::TextureId]),
    ) -> (SceneFrame, RenderStats) {
        self.fps.tick(now);
        let far = camera::far_plane(config.render_distance);
        let partial = self.partial_tick();
        let level_partial = self.level_partial_tick();
        let mut camera = Camera::follow(
            &self.player,
            &self.level,
            self.camera_mode,
            self.fov_with_effects(f32::from(config.fov), partial),
            far,
        );
        camera.bob = self.hurt_bob(config.damage_tilt_strength as f32, partial);
        if config.bob_view {
            camera.bob *= self.player.view_bob();
        }
        let view_distance = i32::from(config.render_distance);
        let partial_tick = self.partial_tick();
        self.update_rain_fog(&camera, aspect, partial_tick, now);
        let look = self.environment(
            &camera,
            aspect,
            partial_tick,
            config.render_distance,
            config.cloud_range,
        );
        let [r, g, b, _] = look.fog.color;
        self.fog.clear_color = [r, g, b, 1.0];
        self.note_sky_light(&look);
        let dying = self.body.death > 0;
        let death_time = self.own_death_time(self.partial_tick());
        let in_fluid = matches!(look.fog_type, FogType::Water | FogType::Lava);
        let narrowed = |mut fov: f32| {
            if dying {
                fov = camera::dying_fov(fov, death_time);
            }
            if in_fluid {
                let scale = config.fov_effect_scale as f32;
                fov *= 1.0 + (0.857_142_87 - 1.0) * scale;
            }
            fov
        };
        camera.fov = narrowed(camera.fov);
        self.audio.ears = Some(super::sounds::Ears {
            position: camera.position.to_array(),
            forward: camera.forward().to_array(),
            up: camera.up().to_array(),
        });
        self.name_view = Some(super::nametags::NameView::of(&camera, aspect));
        let hand_fov = narrowed(HAND_FOV);
        let first_person = self.camera_mode == CameraMode::FirstPerson;
        self.effects_frame = self.screen_effects(
            assets,
            &look,
            partial_tick,
            config.vignette,
            first_person,
            config.screen_effect_scale,
        );
        let distortion = self.distortion(partial_tick, config.screen_effect_scale);
        let (min_y, height) = self.level.height();
        let spectator = self.game_mode.current.is_spectator();
        let mut cull = config.cull;
        if spectator {
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
        let culler =
            if look.fog_type != FogType::Atmospheric || look.atmosphere.skybox == Skybox::None {
                culler.with_fog(look.fog.environmental_end)
            } else {
                culler
            };
        let section = culler.section_of(camera.position);
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
        let visible = self.meshes.visible_sections(&culler);
        let sections = visible.count;

        let block_entity_meshes = self.block_entity_meshes.update(
            renderer,
            &self.block_entities,
            &self.level,
            &assets.models,
            &culler,
            level_partial,
        );
        let visible_entities =
            super::entities::visible_entities(&self.entities, &culler, partial, |pos| {
                self.meshes.section_compiled(pos)
            });
        let visible_leads = super::leads::visible_leads(
            &self.entities,
            &self.level,
            self.joined.session.info.entity_id,
            self.player.rope_hold(config.main_hand),
            &culler,
            partial,
        );
        let visible_heads = super::heads::visible_heads(&self.level, &culler);
        let own_visible = {
            let (min, max) = self.player.render_bounds();
            culler.sees_entity(min, max)
        };

        let atmosphere = &look.atmosphere;
        let lightmap_params = lightmap::LightmapParams {
            sky: atmosphere.sky_light(),
            gamma: config.gamma as f32,
            ambient: atmosphere.ambient_light_color,
            block_tint: atmosphere.block_light_tint,
        };
        let own = assets.players.for_profile(self.uuid);
        let (skin, body) = (own.texture, own.model);
        let mut entities = std::mem::take(&mut self.spare_entities);
        entities.clear();
        let mut entities_drawn = 0;
        // `Camera.yRot` / `xRot`, which vanilla's billboards turn to.
        let camera_angles = (camera.look().y_rot, camera.look().x_rot);
        let main_arm = config.main_hand;
        if self.camera_mode == CameraMode::FirstPerson {
            // `GameRenderer.renderItemInHand`: not while the HUD is hidden.
            if !spectator && !self.hud_hidden {
                let origin = camera.position.floor().as_ivec3();
                let eye = (camera.position - origin.as_dvec3()).as_vec3();
                let hands = self.first_person_hands(eye, main_arm, config.model_parts, partial);
                for draw in hands {
                    match draw {
                        HandDraw::Arm {
                            arm,
                            matrix,
                            sleeve,
                        } => {
                            let model = &body.model;
                            let pose = model.first_person_arm_pose(arm, sleeve);
                            let matrices =
                                model.model.subtree_matrices(&pose, model.arm(arm), matrix);
                            body.meshes.draws(
                                &matrices,
                                origin,
                                DrawStyle::new(skin, RenderLayer::Hand),
                                &mut entities,
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
                                entities.push(EntityDraw {
                                    alpha: 1.0,
                                    mesh,
                                    atlas,
                                    model: matrix,
                                    origin,
                                    layer: RenderLayer::Hand,
                                    blend: crate::renderer::Blending::of(translucent),
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
        } else {
            if own_visible {
                entities_drawn = 1;
                let (arms, swing) = if spectator {
                    (ArmPoses::default(), None)
                } else {
                    (self.arm_poses(main_arm), self.swing_pose(main_arm, partial))
                };
                let mut state =
                    self.player
                        .avatar_state(arms, swing, spectator, config.model_parts);
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
                let light =
                    self.light_at_eye(self.player.tick_eye().as_vec3(), 0, &lightmap_params);
                self.own_avatar_draws(
                    renderer,
                    assets,
                    state,
                    main_arm,
                    camera.position,
                    light,
                    &mut entities,
                );
            }
        }
        entities_drawn += visible_entities.len();
        let mut glowing = Vec::new();
        self.entity_draws(
            renderer,
            assets,
            &visible_entities,
            camera.position,
            partial,
            &lightmap_params,
            camera_angles,
            &mut entities,
            &mut glowing,
        );
        self.pickup_draws(assets, partial, &lightmap_params, &mut entities);
        self.head_draws(assets, &visible_heads, &lightmap_params, &mut entities);
        self.lead_draws(
            renderer,
            assets,
            &visible_entities,
            visible_leads,
            partial,
            &lightmap_params,
            &mut entities,
        );
        self.fishing_draws(
            renderer,
            assets,
            &visible_entities,
            &super::fishing::View {
                position: camera.position.as_vec3(),
                angles: camera_angles,
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
            &lightmap_params,
            &mut entities,
        );
        self.text_display_draws(
            &visible_entities,
            camera.position.as_vec3(),
            camera_angles,
            partial,
            &lightmap_params,
            &mut entities,
        );
        self.border_mesh.draw(
            renderer,
            &self.border,
            camera.position,
            (f64::from(config.render_distance) * 16.0).max(3.0),
            assets.forcefield,
            &mut entities,
        );
        self.entity_meshes.end_frame(renderer);
        entities.extend(
            block_entity_meshes
                .into_iter()
                .flatten()
                .map(|(mesh, origin)| EntityDraw {
                    alpha: 1.0,
                    mesh,
                    atlas: assets.atlas,
                    model: Mat4::IDENTITY,
                    origin,
                    layer: RenderLayer::Entities,
                    blend: crate::renderer::Blending::Opaque,
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
            &mut entities,
        );
        let particle_frustum = camera.frustum(aspect);
        self.particle_draws(
            renderer,
            assets,
            &camera,
            &particle_frustum,
            // `ParticleEngine` stands with the level: drawn where it is.
            level_partial,
            &mut entities,
        );

        self.hit_result = self.pick_hit(self.player.eye());
        let (width, _) = renderer.framebuffer_size();
        let mut lines = Vec::new();
        self.outline.draw(
            renderer,
            &self.level,
            self.hit_block(),
            (
                |state| assets.models.has_translucent_quads(state),
                config.high_contrast_block_outline,
            ),
            // `Window.getAppropriateLineWidth`.
            (width as f32 / 1920.0 * 2.5).max(2.5),
            &mut lines,
        );

        let debug_frustum = camera.frustum_with_fov(aspect, camera.fov.max(f32::from(config.fov)));
        self.draw_debug_view(
            renderer,
            &camera,
            &debug_frustum,
            &visible_entities,
            partial,
            font,
            &mut entities,
            &mut lines,
        );

        let celestials = self.sky_draws(assets, &look, &camera, renderer.white_texture());
        let (clouds, weather) = self.atmosphere_draws(
            renderer,
            assets,
            &camera,
            config,
            &look,
            &lightmap_params,
            partial_tick,
            aspect,
        );

        let stats = RenderStats {
            sections_drawn: sections.drawn,
            sections_total: sections.total,
            vertices_drawn: sections.vertices,
            terrain_draws: sections.draws,
            entities_drawn,
            entities_total: self.entities.len() + 1,
            particles: {
                let (quads, total) = self.particles.engine.counts();
                (quads, total - quads)
            },
        };
        let scene = SceneFrame {
            sky: Some(Sky {
                cubemap: None,
                view: sky::rotation_view(camera.bob * camera.view),
                fov_y_degrees: camera.fov,
                celestials,
            }),
            world: Some(WorldDraw {
                chunks: visible.solid,
                cutout: visible.cutout,
                translucent: visible.translucent,
                atlas: assets.atlas,
                view: camera.view,
                eye: camera.position,
                fov_y_degrees: camera.fov,
                hand_fov_y_degrees: hand_fov,
                far,
                ambient: {
                    let eye = self.player.tick_eye().floor().as_ivec3();
                    let (block, sky) = self.level.light_at(eye.x, eye.y, eye.z);
                    lightmap::sample(block, sky, &lightmap_params)
                },
                lightmap: lightmap_params,
                fog: look.fog,
                cardinal: self.level_info.dimension_type.cardinal_light.relative(),
                distortion,
                bob: camera.bob,
            }),
            entities,
            clouds,
            weather,
            lines,
            glint_item: Some(assets.glint_item),
            glint_armor: Some(assets.glint_armor),
            glowing,
            post_effects: Vec::new(),
        };
        (scene, stats)
    }

    /// Takes back a drawn [`SceneFrame`] from [`Self::scene`], so the next
    pub fn recycle(&mut self, scene: SceneFrame) {
        if let Some(world) = scene.world {
            self.meshes
                .recycle(world.chunks, world.cutout, world.translucent);
        }
        self.spare_entities = scene.entities;
    }
}
