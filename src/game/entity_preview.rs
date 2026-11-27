//! The player in an inventory's box (or the mount in its screen's),
//! turned toward the mouse. Drawn on the GPU into a picture of its own,
//! as vanilla's `GuiEntityRenderer` does; on the CPU only where that
//! picture can't be made.

use std::collections::HashMap;

use glam::{IVec3, Mat4, Vec3};
use image::RgbaImage;

use super::InWorld;
use super::entities::{Avatar, submit_avatar};
use super::entity_models::DrawStyle;
use super::world_resources::WorldResources;
use melon_base::gfx::TextureId;
use melon_gui::entity_preview::{self, EntityPreview, PreviewLayer};
use melon_render::{EntityDraw, Glint, PictureDraw, RenderLayer, Renderer, pixel_projection};
use melon_settings::options::Config;
use melon_world::block_entity::models::SkullType;
use melon_world::entity::kind::EntityKind;
use melon_world::entity::mobs;
use melon_world::entity::mobs::custom_head::HeadTransforms;
use melon_world::entity::model::PartId;
use melon_world::entity::player::{AvatarRenderState, PlayerModel};
use melon_world::head_profile::HeadProfile;
use melon_world::player::hands::Hand;

/// `Avatar.STANDING_DIMENSIONS`' and `POSES[CROUCHING]`'s heights.
const STANDING_HEIGHT: f32 = 1.8;
const CROUCHING_HEIGHT: f32 = 1.5;
/// How far before and behind the box's middle a picture reaches, in blocks.
const PICTURE_DEPTH: f32 = 4.0;

#[derive(Default)]
pub struct PreviewTextures {
    /// The GPU picture the preview is drawn into.
    picture: Option<TextureId>,
    /// This frame's draws into it, for the renderer.
    picture_draw: Option<PictureDraw>,
    /// No picture could be made: drawn on the CPU from then on.
    cpu_only: bool,
    /// The CPU pictures' textures, by size.
    by_size: HashMap<(u32, u32), TextureId>,
    /// The CPU picture last written, and where.
    last: Option<(TextureId, RgbaImage)>,
    /// What it was drawn from ([`entity_preview::signature`]).
    signature: Option<u64>,
    /// This frame's, if the overlay shows one.
    pub current: Option<TextureId>,
    /// The mob textures drawn so far, decoded (`None`: not there).
    mob_images: HashMap<String, Option<RgbaImage>>,
}

impl PreviewTextures {
    /// The pictures the renderer draws this frame.
    pub fn pictures(&self) -> &[PictureDraw] {
        self.picture_draw.as_slice()
    }

    /// Frees the textures, on leaving the world.
    pub fn release(&mut self, renderer: &mut Renderer) {
        for texture in self
            .picture
            .into_iter()
            .chain(self.by_size.values().copied())
        {
            renderer.free_texture(texture);
        }
        *self = Self::default();
    }
}

/// A CPU picture: unchanged since the last, or drawn anew with what it was
/// drawn from.
enum Drawn {
    Same,
    New(RgbaImage, Option<u64>),
}

impl InWorld {
    /// The own profile id.
    pub fn profile_id(&self) -> uuid::Uuid {
        self.uuid
    }

    /// The pictures the renderer draws this frame.
    pub fn pictures(&self) -> &[PictureDraw] {
        self.preview.pictures()
    }

    /// Draws the player for the open overlay's box, if it has one.
    pub fn prepare_entity_preview(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        config: &Config,
        gui_scale: u32,
    ) {
        self.preview.picture_draw = None;
        let Some(preview) = self.overlay.as_ref().and_then(|o| o.entity_preview()) else {
            self.preview.current = None;
            return;
        };
        let scale = gui_scale.max(1) as f32;
        if !self.preview.cpu_only && self.prepare_picture(renderer, assets, config, &preview, scale)
        {
            return;
        }
        self.prepare_cpu_preview(renderer, assets, config, &preview, scale);
    }

    /// The preview as a GPU picture. `false` when no picture can be made.
    fn prepare_picture(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        config: &Config,
        preview: &EntityPreview,
        scale: f32,
    ) -> bool {
        let mut entities = Vec::new();
        let view = match preview.entity {
            Some(id) => self.mob_draws(assets, preview, id, scale, &mut entities),
            None => Some(self.player_draws(assets, config, preview, scale, &mut entities)),
        };
        let Some(view) = view else {
            self.preview.current = None;
            return true;
        };
        let (width, height) = preview.pixels(scale);
        let kept = self
            .preview
            .picture
            .filter(|&t| renderer.picture_size(t) == Some((width, height)));
        let target = match kept {
            Some(target) => target,
            None => {
                if let Some(old) = self.preview.picture.take() {
                    renderer.free_texture(old);
                }
                match renderer.create_picture(width, height) {
                    Ok(target) => {
                        self.preview.picture = Some(target);
                        target
                    }
                    Err(e) => {
                        melon_base::warn!(
                            "Player preview not on the GPU, drawing it on the CPU: {e:#}"
                        );
                        self.preview.cpu_only = true;
                        return false;
                    }
                }
            }
        };
        let depth = scale * preview.size * PICTURE_DEPTH;
        self.preview.picture_draw = Some(PictureDraw {
            target,
            view_proj: pixel_projection(width, height, depth) * view,
            entities,
            glint_armor: Some(assets.glint.armor),
            glint_item: Some(assets.glint.item),
        });
        self.preview.current = Some(target);
        true
    }

    /// The preview drawn on the CPU, uploaded when it changed.
    fn prepare_cpu_preview(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        config: &Config,
        preview: &EntityPreview,
        scale: f32,
    ) {
        let drawn = match preview.entity {
            Some(id) => self.mob_preview(assets, preview, id, scale),
            None => Some(self.player_preview(assets, config, preview, scale)),
        };
        let (image, signature) = match drawn {
            None => {
                self.preview.current = None;
                return;
            }
            Some(Drawn::Same) => {
                self.preview.current = self.preview.last.as_ref().map(|(at, _)| *at);
                return;
            }
            Some(Drawn::New(image, signature)) => (image, signature),
        };
        let (width, height) = (image.width(), image.height());
        let previews = &mut self.preview;
        let texture = match previews.by_size.get(&(width, height)) {
            Some(&texture) => {
                let same = previews
                    .last
                    .as_ref()
                    .is_some_and(|(at, last)| *at == texture && *last == image);
                if !same {
                    renderer.update_texture(texture, 0, 0, 0, image.clone());
                }
                texture
            }
            None => match renderer.load_texture(&image) {
                Ok(texture) => {
                    previews.by_size.insert((width, height), texture);
                    texture
                }
                Err(e) => {
                    melon_base::warn!("Player preview not uploaded: {e:#}");
                    previews.current = None;
                    return;
                }
            },
        };
        previews.last = Some((texture, image));
        previews.signature = signature;
        previews.current = Some(texture);
    }

    /// Whether the CPU picture of `layers` is the one shown last, and what
    /// it is drawn from.
    fn cpu_picture(&self, layers: &[PreviewLayer], width: u32, height: u32) -> Drawn {
        let signature = entity_preview::signature(layers, width, height);
        if signature.is_some() && signature == self.preview.signature && self.preview.last.is_some()
        {
            return Drawn::Same;
        }
        Drawn::New(
            entity_preview::rasterize_layers(layers, width, height),
            signature,
        )
    }

    /// The own player as the preview shows it: turned toward the mouse,
    /// and its height.
    fn preview_avatar_state(
        &self,
        config: &Config,
        preview: &EntityPreview,
    ) -> (AvatarRenderState, f32) {
        let partial = self.partial_tick();
        let arms = self.arm_poses(config.main_hand);
        let swing = self.swing_pose(config.main_hand, partial);
        let mut state = self
            .player
            .avatar_state(arms, swing, false, config.model_parts);
        let (x_angle, y_angle) = preview.angles();
        state.position = Vec3::ZERO;
        // `InventoryScreen.renderEntityInInventory` divides the scale
        // attribute back out: the doll is always the same size.
        state.scale = 1.0;
        state.body_yaw = x_angle * 20.0;
        state.humanoid.y_rot = x_angle * 20.0;
        state.humanoid.x_rot = -y_angle * 20.0;
        let height = if state.humanoid.crouching {
            CROUCHING_HEIGHT
        } else {
            STANDING_HEIGHT
        };
        (state, height)
    }

    /// The own player's draws, as the world draws it (`submit_avatar`):
    /// skin, armor, worn head and the items in hand, at full light. The
    /// preview's view.
    fn player_draws(
        &self,
        assets: &WorldResources,
        config: &Config,
        preview: &EntityPreview,
        scale: f32,
        out: &mut Vec<EntityDraw>,
    ) -> Mat4 {
        let (state, height) = self.preview_avatar_state(config, preview);
        let hands = Hand::ALL.map(|hand| {
            let stack = self.hand_stack(hand);
            let using = self.hands.using_in(hand).is_some();
            (stack, self.held_use(hand, stack, using))
        });
        let avatar = Avatar {
            state,
            look: assets.players.for_profile(self.uuid),
            main_arm: config.main_hand,
            hands,
            armor: self.own_armor(),
            light: [1.0; 3],
            overlay: mobs::hurt_overlay(self.body.hurt > 0 || self.body.death > 0),
        };
        submit_avatar(assets, &avatar, IVec3::ZERO, RenderLayer::Entities, out);
        preview.view(scale, height)
    }

    /// A tracked mob's draws, layer by layer as [`Self::mob_preview`] has
    /// them, and the preview's view. `None` once it is gone.
    fn mob_draws(
        &self,
        assets: &WorldResources,
        preview: &EntityPreview,
        id: i32,
        scale: f32,
        out: &mut Vec<EntityDraw>,
    ) -> Option<Mat4> {
        let entity = self.entities.get(id)?;
        let (specs, input) = mob_specs(assets, entity, preview, true);
        let root = mobs::root(entity, &input, 1.0);
        for (layer, pose) in specs.iter().zip(pose_layers(assets, &specs, &input)) {
            let (mob, meshes) = assets.mobs.get(layer.model);
            let Some(texture) = assets.mobs.texture(&layer.texture) else {
                continue;
            };
            let matrices = mob.model.part_matrices(&pose, root);
            let first = out.len();
            meshes.draws(
                &matrices,
                IVec3::ZERO,
                DrawStyle {
                    texture,
                    layer: RenderLayer::Entities,
                    light: Some(layer.tint),
                    overlay: 0.0,
                    glint: Glint::armor(layer.foil),
                },
                out,
            );
            if layer.translucent {
                for draw in &mut out[first..] {
                    draw.blend = melon_render::Blending::Translucent;
                }
            }
        }
        Some(preview.view(scale, entity.size().1))
    }
}

impl InWorld {
    /// The own player turned toward the mouse, skin then worn armor.
    fn player_preview(
        &mut self,
        assets: &WorldResources,
        config: &Config,
        preview: &EntityPreview,
        scale: f32,
    ) -> Drawn {
        let look = assets.players.for_profile(self.uuid);
        let player = &look.model.model;
        let (state, height) = self.preview_avatar_state(config, preview);
        let root = preview.view(scale, height) * PlayerModel::root_transform(&state);
        let pose = player.pose(&state);
        let overlay = player.overlay_parts();
        let mut worn = Vec::new();
        mobs::push_humanoid_armor(EntityKind::Player, false, self.own_armor(), &mut worn);
        for layer in &worn {
            let images = &mut self.preview.mob_images;
            images
                .entry(layer.texture.to_string())
                .or_insert_with(|| assets.mobs.image(&layer.texture));
        }
        let armor_posed: Vec<_> = worn
            .iter()
            .map(|layer| {
                let (mob, _) = assets.mobs.get(layer.model);
                (
                    mob,
                    mobs::pose_worn((&player.model, &player.humanoid, &pose), mob),
                )
            })
            .collect();
        let worn_head = self.own_armor()[0];
        let skull = SkullType::of_item(worn_head.item.registry_key);
        let head_profile = HeadProfile::of_stack(worn_head);
        if let Some(kind) = skull.filter(|&k| k != SkullType::Player) {
            let images = &mut self.preview.mob_images;
            images
                .entry(kind.texture().to_owned())
                .or_insert_with(|| assets.mobs.image(kind.texture()));
        }
        let head_skull = skull.map(|kind| {
            let model = assets.players.head.model(kind);
            let head = player
                .model
                .part_transform(&pose, player.humanoid.head, root);
            (
                model,
                model.pose(state.humanoid.walk_position),
                super::heads::worn_skull_transform(head, HeadTransforms::DEFAULT),
            )
        });
        let head_texture = match skull {
            Some(SkullType::Player) => Some(assets.players.for_head(head_profile.as_ref()).image),
            Some(kind) => self
                .preview
                .mob_images
                .get(kind.texture())
                .and_then(|t| t.as_ref()),
            None => None,
        };
        let head_overlay: Vec<PartId> = head_skull
            .as_ref()
            .and_then(|(model, ..)| model.overlay())
            .into_iter()
            .collect();
        let mut layers = vec![
            PreviewLayer {
                model: &player.model,
                pose: &pose,
                root,
                texture: look.image,
                tint: [255; 3],
                skip: &overlay,
                only: None,
                share_depth: false,
                glint: None,
            },
            PreviewLayer {
                model: &player.model,
                pose: &pose,
                root,
                texture: look.image,
                tint: [255; 3],
                skip: &[],
                only: Some(&overlay),
                share_depth: true,
                glint: None,
            },
        ];
        for (layer, (mob, armor_pose)) in worn.iter().zip(&armor_posed) {
            let Some(texture) = self
                .preview
                .mob_images
                .get(layer.texture.as_ref())
                .and_then(|t| t.as_ref())
            else {
                continue;
            };
            layers.push(PreviewLayer {
                model: &mob.model,
                pose: armor_pose,
                root,
                texture,
                tint: layer
                    .tint
                    .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
                skip: &[],
                only: None,
                share_depth: true,
                glint: layer.foil.then_some(&assets.glint.armor_image),
            });
        }
        if let (Some((model, pose, at)), Some(texture)) = (&head_skull, head_texture) {
            layers.push(PreviewLayer {
                model: model.model(),
                pose,
                root: *at,
                texture,
                tint: [255; 3],
                skip: &head_overlay,
                only: None,
                share_depth: true,
                glint: None,
            });
            if !head_overlay.is_empty() {
                layers.push(PreviewLayer {
                    model: model.model(),
                    pose,
                    root: *at,
                    texture,
                    tint: [255; 3],
                    skip: &[],
                    only: Some(&head_overlay),
                    share_depth: true,
                    glint: None,
                });
            }
        }
        let (width, height) = preview.pixels(scale);
        self.cpu_picture(&layers, width, height)
    }

    /// A tracked mob turned toward the mouse, layer by layer (a horse's
    /// armor over its body), drawn on the CPU.
    fn mob_preview(
        &mut self,
        assets: &WorldResources,
        preview: &EntityPreview,
        id: i32,
        scale: f32,
    ) -> Option<Drawn> {
        let entity = self.entities.get(id)?;
        let (specs, input) = mob_specs(assets, entity, preview, false);
        let root = preview.view(scale, entity.size().1) * mobs::root(entity, &input, 1.0);
        let posed: Vec<_> = specs
            .iter()
            .zip(pose_layers(assets, &specs, &input))
            .map(|(layer, pose)| (assets.mobs.get(layer.model).0, pose))
            .collect();
        for layer in &specs {
            let images = &mut self.preview.mob_images;
            images
                .entry(layer.texture.to_string())
                .or_insert_with(|| assets.mobs.image(&layer.texture));
        }
        let layers: Vec<_> = specs
            .iter()
            .zip(&posed)
            .enumerate()
            .filter_map(|(i, (layer, (mob, pose)))| {
                let texture = self
                    .preview
                    .mob_images
                    .get(layer.texture.as_ref())?
                    .as_ref()?;
                Some(PreviewLayer {
                    model: &mob.model,
                    pose,
                    root,
                    texture,
                    tint: layer
                        .tint
                        .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
                    skip: &[],
                    only: None,
                    share_depth: i > 0,
                    glint: layer.foil.then_some(&assets.glint.armor_image),
                })
            })
            .collect();
        let (width, height) = preview.pixels(scale);
        Some(self.cpu_picture(&layers, width, height))
    }
}

/// The layers of `entity` the preview shows (the see-through ones only
/// with `translucent`: the CPU can't blend them in order), and its pose
/// turned toward the mouse.
fn mob_specs(
    assets: &WorldResources,
    entity: &melon_world::entity::tracker::ClientEntity,
    preview: &EntityPreview,
    translucent: bool,
) -> (Vec<mobs::LayerSpec>, mobs::PoseInput) {
    let mut specs = Vec::new();
    mobs::layers(entity, 0.0, &|id| assets.mobs.hat(id), &mut specs);
    specs.retain(|layer| translucent || !layer.translucent);
    let mut input = mobs::pose_input(entity, 1.0);
    let (x_angle, y_angle) = preview.angles();
    input.living.position = Vec3::ZERO;
    input.living.body_rot = x_angle * 20.0;
    input.living.y_rot = x_angle * 20.0;
    input.living.x_rot = -y_angle * 20.0;
    input.living.walk_position = 0.0;
    input.living.walk_speed = 0.0;
    input.living.riding = false;
    (specs, input)
}

/// Each layer's pose: its own, or for worn armor the body's it copies.
fn pose_layers(
    assets: &WorldResources,
    specs: &[mobs::LayerSpec],
    input: &mobs::PoseInput,
) -> Vec<melon_world::entity::model::ModelPose> {
    let mut posed = Vec::with_capacity(specs.len());
    let mut body_pose: Option<(&mobs::MobModel, melon_world::entity::model::ModelPose)> = None;
    for layer in specs {
        let (mob, _) = assets.mobs.get(layer.model);
        let pose = if layer.copy_humanoid {
            body_pose
                .as_ref()
                .and_then(|(from_mob, from_pose)| {
                    let parts = from_mob.humanoid()?;
                    mob.humanoid()?;
                    Some(mobs::pose_worn((&from_mob.model, parts, from_pose), mob))
                })
                .unwrap_or_else(|| mob.model.initial_pose())
        } else {
            mob.pose(input)
        };
        if body_pose.is_none() {
            body_pose = Some((mob, pose.clone()));
        }
        posed.push(pose);
    }
    posed
}
