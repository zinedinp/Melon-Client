//! The other entities in the world.

use glam::{DVec3, IVec3, Mat4, Vec3};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use super::InWorld;
use super::entity_models::{DrawStyle, PlayerLook};
use super::entity_sprites;
use super::held_items::Held;
use super::shadows::OWN;
use super::world_resources::WorldResources;
use crate::math::angle::Rotation;
use crate::network::play::game::EntityPacket;
use crate::renderer::{EntityDraw, Glint, MeshId, RenderLayer, Renderer, TextureId};
use crate::world::block_tint::TintKind;
use crate::world::culling::Culler;
use crate::world::culling::occlusion::SectionPos;
use crate::world::entity::humanoid::{self, ArmPose, HumanoidParts};
use crate::world::entity::item_drop;
use crate::world::entity::kind::EntityKind;
use crate::world::entity::mobs::{self, LayerSpec, MobModel, ModelId, Rig, tnt, villager};
use crate::world::entity::model::{Model, ModelPose};
use crate::world::entity::player::{self as player_model, Arm, AvatarRenderState, PlayerModel};
use crate::world::entity::pose_cache::PoseCache;
use crate::world::entity::tracker::{ClientEntity, Entities};
use crate::world::item_model::HandPose;
use crate::world::item_model::{HeldUse, StackKey};
use crate::world::lightmap::{self, LightmapParams};
use crate::world::player::ArmPoses;
use crate::world::player::hands::{Hand, ItemUse};

/// A player to draw: the local one in third person, or another.
pub(super) struct Avatar<'a> {
    /// Its position relative to the draw's origin.
    pub state: AvatarRenderState,
    pub look: PlayerLook<'a>,
    pub main_arm: Arm,
    /// By [`Hand::ALL`]: the stack, and whether it is being used.
    pub hands: [(&'a ItemStack, HeldUse); 2],
    /// By [`ArmorSlot::ALL`].
    pub armor: [&'a ItemStack; 4],
    pub light: [f32; 3],
    pub overlay: f32,
}

/// The entities' own meshes beside their models, refilled every frame.
#[derive(Default)]
pub(super) struct EntityMeshes {
    /// Blob shadows under the player and the entities.
    pub(super) shadows: super::shadows::ShadowMeshes,
    /// The leads drawn, one mesh each.
    pub(super) leads: super::leads::LeadMeshes,
    pub(super) fishing: super::fishing::FishingMeshes,
    /// The orbs' and fireballs' camera-facing quads.
    pub(super) sprites: entity_sprites::SpriteMeshes,
    pub(super) beams: super::crystal_beams::BeamMeshes,
    pub(super) paintings: super::paintings::PaintingMeshes,
    pub(super) text_displays: super::text_displays::TextDisplayMeshes,
    /// The mobs' part matrices from the frame before, for those standing
    /// still.
    pub(super) poses: PoseCache,
}

impl EntityMeshes {
    /// Frees the meshes not drawn this frame.
    pub(super) fn end_frame(&mut self, renderer: &mut Renderer) {
        self.shadows.end_frame(renderer);
        self.leads.end_frame(renderer);
        self.fishing.end_frame(renderer);
        self.paintings.end_frame(renderer);
        self.sprites.end_frame(renderer);
        self.beams.end_frame(renderer);
        self.poses.end_frame();
    }

    /// Frees every mesh: the world is left or changes dimension.
    pub(super) fn clear(&mut self, renderer: &mut Renderer) {
        self.shadows.clear(renderer);
        self.leads.clear(renderer);
        self.fishing.clear(renderer);
        self.sprites.clear(renderer);
        self.beams.clear(renderer);
        self.paintings.clear(renderer);
        self.text_displays.clear(renderer);
        self.poses.clear();
    }
}

/// Where one entity's draws go.
#[derive(Clone, Copy)]
pub(super) struct Placed {
    pub origin: IVec3,
    pub layer: RenderLayer,
    pub light: [f32; 3],
}

impl InWorld {
    /// One entity packet.
    pub(super) fn receive_entity(&mut self, packet: EntityPacket) {
        let own = self.joined.session.info.entity_id;
        if let EntityPacket::Data { id, values } = &packet
            && *id == own
        {
            self.own_entity_data(values);
            self.own_vitals_data(values);
            return;
        }
        if let EntityPacket::Attributes { id, snapshots, .. } = &packet
            && *id == own
        {
            self.own_attributes(snapshots);
            return;
        }
        if let EntityPacket::Swing { id, hand, swing } = packet
            && id == own
        {
            let mut swing = swing;
            swing.duration = self.mining_effects.swing_duration(swing.duration);
            self.hands.swing.start_if_able(hand, swing);
            return;
        }
        if let EntityPacket::Hurt { id, yaw } = &packet
            && *id == own
        {
            self.body.hurt_dir = *yaw;
        }
        if let EntityPacket::Damage { id, damage } = &packet
            && *id == own
        {
            self.play_own_hurt(*damage);
        }
        if let EntityPacket::Hurt { id, .. } | EntityPacket::Damage { id, .. } = &packet
            && *id == own
        {
            self.body.hurt = crate::world::entity::tracker::HURT_DURATION;
            return;
        }
        self.entities.apply(packet, own);
    }

    /// The sound of a damage type's `DamageEffects`, at the player.
    fn play_own_hurt(&mut self, damage: i32) {
        let effects = usize::try_from(damage)
            .ok()
            .and_then(|i| self.joined.session.info.damage_effects.get(i))
            .map_or("hurt", String::as_str);
        let event = match effects {
            "drowning" => "entity.player.hurt_drown",
            "burning" => "entity.player.hurt_on_fire",
            "poking" => "entity.player.hurt_sweet_berry_bush",
            "freezing" => "entity.player.hurt_freeze",
            _ => "entity.player.hurt",
        };
        self.play_own_voice(event);
    }

    /// A sound of the local player's own voice (`getVoicePitch`).
    pub(super) fn play_own_voice(&mut self, event: &'static str) {
        let at = self.player.tick_eye();
        let mut sound = crate::audio::SoundInstance::of(event).at([at.x, at.y, at.z]);
        sound.pitch = (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0;
        self.audio.push(sound);
    }

    /// The items flying into hands move on, and their pickup sounds play.
    pub(super) fn tick_pickups(&mut self) {
        let own = self.joined.session.info.entity_id;
        let middle = (self.player.feet() + self.player.tick_eye()) / 2.0;
        let entities = &self.entities;
        let places: Vec<(i32, glam::DVec3)> = entities
            .pickups()
            .iter()
            .filter_map(|p| entities.body_middle(p.taker()).map(|at| (p.taker(), at)))
            .collect();
        self.entities.tick_pickups(|id| {
            if id == own {
                Some(middle)
            } else {
                places.iter().find(|(i, _)| *i == id).map(|&(_, at)| at)
            }
        });
        // `Warden.tick`'s heartbeat: volume 5, `getVoicePitch`.
        for sound in self.entities.take_own_sounds() {
            let mut instance = crate::audio::SoundInstance::of(sound.event)
                .on(crate::audio::source::SoundSource::Hostile)
                .at([sound.at.x, sound.at.y, sound.at.z]);
            instance.volume = 5.0;
            instance.pitch = (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0;
            self.audio.push(instance);
        }
        for sound in self.entities.take_pickup_sounds() {
            let mut instance = crate::audio::SoundInstance::of("entity.item.pickup")
                .at([sound.at.x, sound.at.y, sound.at.z]);
            instance.volume = 0.2;
            instance.pitch = (sound.jitter - 0.5) * 1.4 + 2.0;
            self.audio.push(instance);
        }
    }

    /// `LivingEntityRenderState.deathTime` of the local player.
    pub(super) fn own_death_time(&self, partial: f32) -> f32 {
        if self.body.death > 0 {
            self.body.death as f32 + partial
        } else {
            0.0
        }
    }

    /// Before the frame is drawn: the textures, items and skins the
    /// TODO(perf): runs every frame for every entity: layer lists, texture
    /// and held-item lookups whose answer only changes with equipment, kind
    /// or display. Cache per entity behind a generation counter, or run per
    /// tick.
    pub fn prepare_entities(&self, renderer: &mut Renderer, assets: &mut WorldResources) {
        assets
            .held
            .animate(renderer, self.clocks.game_time().max(0) as u64);
        if self.faces_shown() {
            for (id, player) in self.listed_players() {
                assets.players.request(id, player.textures.as_deref());
            }
        }
        let mut layers = Vec::new();
        for entity in self.entities.iter() {
            if entity.kind.is_avatar() {
                match entity.mannequin.as_deref() {
                    Some(mannequin) => {
                        if let Some(profile) = &mannequin.profile.profile {
                            assets.players.request_head(profile);
                        }
                    }
                    None => assets
                        .players
                        .request(entity.uuid, self.profile_textures(entity.uuid)),
                }
                layers.clear();
                mobs::equipment_layers(entity, &mut layers);
                for layer in &layers {
                    assets.mobs.prepare_texture(renderer, &layer.texture);
                }
            } else {
                layers.clear();
                if !entity.kind.is_block() {
                    mobs::layers(entity, 0.0, &|id| assets.mobs.hat(id), &mut layers);
                }
                for layer in &layers {
                    assets.mobs.prepare_texture(renderer, &layer.texture);
                }
            }
            let fishing = entity.kind == EntityKind::Player && self.entities.is_fishing(entity.id);
            for hand in Hand::ALL {
                let stack = entity.equipment.hand(hand);
                assets.prepare_held(renderer, stack, false);
                if entity.appearance.using_hand() == Some(hand) {
                    assets.prepare_held(renderer, stack, true);
                }
                let held = remote_held_use(entity, hand, false, fishing);
                if held == HeldUse::Cast {
                    assets.prepare_held(renderer, stack, held);
                }
            }
            if entity.kind == EntityKind::Item {
                assets.prepare_held(renderer, &entity.item, false);
            }
            if let Some(stack) = entity_sprites::shown_item(entity) {
                assets.prepare_held(renderer, &*stack, false);
            }
            match entity.kind {
                EntityKind::ExperienceOrb => {
                    assets
                        .mobs
                        .prepare_texture(renderer, entity_sprites::ORB_TEXTURE);
                }
                EntityKind::DragonFireball => {
                    assets
                        .mobs
                        .prepare_texture(renderer, entity_sprites::DRAGON_FIREBALL_TEXTURE);
                }
                EntityKind::EndCrystal | EntityKind::EnderDragon => {
                    assets
                        .mobs
                        .prepare_texture(renderer, crate::world::entity::crystal_beam::TEXTURE);
                }
                EntityKind::Minecart => {
                    if let Some((state, _)) = minecart_block(entity) {
                        let models = assets.models.clone();
                        let at = block_at(entity);
                        assets.blocks.prepare(renderer, &models, state, |kind| {
                            self.level.tint_at(kind, at)
                        });
                    }
                }
                EntityKind::Painting => {
                    use crate::world::entity::painting;
                    if let Some(variant) = painting::variant(entity.appearance.thing.variant) {
                        assets
                            .mobs
                            .prepare_texture(renderer, &painting::front_texture(variant));
                        assets
                            .mobs
                            .prepare_texture(renderer, painting::BACK_TEXTURE);
                    }
                }
                EntityKind::SulfurCube => {
                    if let Some(state) = mobs::sulfur_cube::swallowed_block(entity) {
                        let models = assets.models.clone();
                        let at = block_at(entity);
                        assets.blocks.prepare(renderer, &models, state, |kind| {
                            self.level.tint_at(kind, at)
                        });
                    }
                }
                _ => {}
            }
            if entity.kind.is_item_frame() {
                for model in FRAME_MODELS {
                    assets.held.prepare_block(renderer, model);
                }
                assets.prepare_held(renderer, &entity.item, false);
            }
            if let Some(display) = entity.display.as_deref() {
                match entity.kind {
                    EntityKind::ItemDisplay => assets.prepare_held(renderer, &display.item, false),
                    EntityKind::BlockDisplay if display.block_state != 0 => {
                        let models = assets.models.clone();
                        let at = block_at(entity);
                        assets
                            .blocks
                            .prepare(renderer, &models, display.block_state, |kind| {
                                self.level.tint_at(kind, at)
                            });
                    }
                    _ => {}
                }
            }
            if let Some(model) = mobs::cow::mushroom_model(entity) {
                assets.held.prepare_block(renderer, model);
            }
            if let Some(state) = shown_block(entity) {
                let models = assets.models.clone();
                let at = block_at(entity);
                assets.blocks.prepare(renderer, &models, state, |kind| {
                    self.level.tint_at(kind, at)
                });
            }
            if entity.kind == EntityKind::SnowGolem && entity.appearance.pumpkin {
                assets.held.prepare_block(renderer, mobs::snow_golem::HEAD);
            }
            if entity.kind == EntityKind::IronGolem && entity.golem_pose(0.0).offer_flower > 0 {
                assets
                    .held
                    .prepare_block(renderer, mobs::golem::FLOWER_MODEL);
            }
        }
        self.prepare_fishing(renderer, assets);
        self.prepare_heads(renderer, assets);
        let mut own = Vec::new();
        mobs::push_humanoid_armor(EntityKind::Player, false, self.own_armor(), &mut own);
        for layer in &own {
            assets.mobs.prepare_texture(renderer, &layer.texture);
        }
    }

    /// The own armor, by [`ArmorSlot::ALL`] (`InventoryMenu`'s slots 5-8).
    pub(super) fn own_armor(&self) -> [&ItemStack; 4] {
        let slots = &self.inventory.slots()[crate::world::inventory::ARMOR_SLOTS];
        std::array::from_fn(|i| &*slots[i])
    }

    /// What a shadow's light reads: the time of day's darkening and the
    /// dimension's ambient light.
    pub(super) fn shadow_light(&self) -> crate::world::shadow::ShadowLight {
        crate::world::shadow::ShadowLight {
            sky_light_level: self.effects.sky_light_level(),
            ambient: self.level_info.dimension_type.ambient_light,
        }
    }

    /// `EntityRenderer.getPackedLightCoords`.
    pub(super) fn light_at_eye(&self, eye: Vec3, glow: u8, params: &LightmapParams) -> [f32; 3] {
        let at = eye.floor().as_ivec3();
        let (block, sky) = self.level.light_at(at.x, at.y, at.z);
        lightmap::sample(block.max(glow), sky, params)
    }

    /// The draws and shadows of the entities in `visible`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn entity_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        visible: &[i32],
        camera: DVec3,
        partial: f32,
        params: &LightmapParams,
        view: (f32, f32),
        out: &mut Vec<EntityDraw>,
        glowing: &mut Vec<(usize, [u8; 3])>,
    ) {
        let mut layers = Vec::new();
        for entity in visible.iter().filter_map(|&id| self.entities.get(id)) {
            // `LevelRenderer`: a frozen entity's partial tick is 1.
            let partial = if self.entities.is_frozen(entity.id) {
                1.0
            } else {
                partial
            };
            let first = out.len();
            let outline = self.outline_color(entity);
            if entity.kind.is_display() {
                self.submit_display(assets, entity, camera, partial, view, params, out);
                mark_glowing(glowing, first..out.len(), outline);
                if let Some(state) = entity.display.as_deref()
                    && !entity.appearance.invisible()
                {
                    let tick = entity.age_in_ticks(0.0) as i32;
                    self.entity_meshes.shadows.draw(
                        renderer,
                        &self.level,
                        entity.id,
                        entity.pos(partial).as_vec3(),
                        camera.as_vec3(),
                        state.shadow(tick, partial),
                        self.shadow_light(),
                        assets.shadow,
                        out,
                    );
                }
                continue;
            }
            if entity.kind.is_item_frame() {
                self.submit_item_frame(assets, entity, partial, params, out);
                mark_glowing(glowing, first..out.len(), outline);
                continue;
            }
            if matches!(
                entity.kind,
                EntityKind::LeashKnot | EntityKind::FishingBobber
            ) {
                continue;
            }
            if entity.kind == EntityKind::Painting {
                self.entity_meshes
                    .paintings
                    .draw(renderer, assets, &self.level, entity, out);
                mark_glowing(glowing, first..out.len(), outline);
                continue;
            }
            let glow = mobs::block_glow(entity);
            let light = if entity.kind == EntityKind::ExperienceOrb {
                // `ExperienceOrbRenderer.getBlockLightLevel`: the block's + 7.
                let at = entity.eye(partial).floor().as_ivec3();
                let (block, sky) = self.level.light_at(at.x, at.y, at.z);
                lightmap::sample((block + 7).min(15), sky, params)
            } else {
                self.light_at_eye(entity.eye(partial), glow, params)
            };
            let at = entity.pos(partial);
            let origin = at.floor().as_ivec3();
            let feet = at.as_vec3();
            if entity.kind.is_avatar() {
                let spectator = entity.kind == EntityKind::Player
                    && self
                        .players
                        .get(&entity.uuid)
                        .is_some_and(|p| p.game_mode.is_spectator());
                // `AvatarRenderer.isEntityUpsideDown`: a player by its
                // profile's name, a mannequin by its custom name.
                let upside_down = match &entity.mannequin {
                    Some(_) => entity.custom_name.as_deref().is_some_and(|name| {
                        AvatarRenderState::wants_upside_down(
                            &name.plain_text(),
                            entity.appearance.shown,
                        )
                    }),
                    None => self.players.get(&entity.uuid).is_some_and(|p| {
                        AvatarRenderState::wants_upside_down(&p.name, entity.appearance.shown)
                    }),
                };
                let look = match &entity.mannequin {
                    Some(mannequin) => assets.players.for_avatar(&mannequin.profile),
                    None => assets.players.for_profile(entity.uuid),
                };
                let avatar = remote_avatar(
                    entity,
                    look,
                    origin,
                    partial,
                    light,
                    spectator,
                    upside_down,
                    self.entities.is_fishing(entity.id),
                );
                submit_avatar(assets, &avatar, origin, RenderLayer::Entities, out);
                if spectator {
                    mark_glowing(glowing, first..out.len(), outline);
                    continue;
                }
            } else {
                let placed = Placed {
                    origin,
                    layer: RenderLayer::Entities,
                    light,
                };
                layers.clear();
                if !entity.kind.is_block() {
                    mobs::layers(entity, partial, &|id| assets.mobs.hat(id), &mut layers);
                }
                let at = block_at(entity);
                if entity.kind.is_sprite() {
                    entity_sprites::submit(
                        &mut self.entity_meshes.sprites,
                        renderer,
                        assets,
                        entity,
                        from_block(entity.pos(partial), origin),
                        placed,
                        partial,
                        view,
                        out,
                    );
                } else if entity.kind.is_block() {
                    submit_block_entity(
                        assets,
                        entity,
                        placed,
                        partial,
                        |kind| self.level.tint_at(kind, at),
                        out,
                    );
                } else if entity.kind == EntityKind::Item {
                    submit_item_entity(assets, entity, placed, partial, out);
                } else {
                    submit_mob(
                        assets,
                        &mut self.entity_meshes.poses,
                        entity,
                        &layers,
                        placed,
                        partial,
                        |kind| self.level.tint_at(kind, at),
                        out,
                    );
                    if entity.kind == EntityKind::Minecart {
                        submit_minecart_block(
                            assets,
                            entity,
                            placed,
                            partial,
                            |kind| self.level.tint_at(kind, at),
                            out,
                        );
                    }
                    if entity.kind == EntityKind::SulfurCube {
                        submit_swallowed_block(
                            assets,
                            &mut self.entity_meshes.poses,
                            entity,
                            placed,
                            partial,
                            |kind| self.level.tint_at(kind, at),
                            out,
                        );
                    }
                    if let Some((from, delta, time)) = self.crystal_beam(entity, partial) {
                        self.entity_meshes
                            .beams
                            .draw(renderer, assets, entity.id, from, delta, time, light, out);
                    }
                }
            }
            mark_glowing(glowing, first..out.len(), outline);
            // `extractShadow`: none for an invisible entity, even one whose
            // armor still shows.
            if !entity.appearance.invisible() {
                self.entity_meshes.shadows.draw(
                    renderer,
                    &self.level,
                    entity.id,
                    feet,
                    camera.as_vec3(),
                    (mobs::shadow_radius(entity), mobs::shadow_strength(entity)),
                    self.shadow_light(),
                    assets.shadow,
                    out,
                );
            }
        }
    }

    /// The crystal beam `entity` casts, if any: where it starts, toward
    /// where (in blocks) and the time it scrolls by. An end crystal's runs
    /// from its target to itself (`EndCrystalRenderer.submit`), a dragon's
    /// from itself to the crystal healing it (`EnderDragonRenderer`).
    fn crystal_beam(&self, entity: &ClientEntity, partial: f32) -> Option<(DVec3, Vec3, f32)> {
        use crate::world::entity::mobs::projectile::crystal_y;
        let at = entity.pos(partial);
        match entity.kind {
            EntityKind::EndCrystal => {
                let [x, y, z] = entity.appearance.thing.beam_target?;
                let target = DVec3::new(f64::from(x), f64::from(y), f64::from(z)) + 0.5;
                let age = entity.age_in_ticks(partial);
                let mut delta = (at - target).as_vec3();
                delta.y += crystal_y(age);
                Some((target, delta, age))
            }
            EntityKind::EnderDragon => {
                let crystal = self.entities.get(entity.nearest_crystal?)?;
                let age = crystal.age_in_ticks(partial);
                let mut delta = (crystal.pos(partial) - at).as_vec3();
                delta.y += crystal_y(age);
                Some((at, delta, entity.age_in_ticks(partial)))
            }
            _ => None,
        }
    }

    /// A display entity's model (`DisplayRenderer`).
    #[allow(clippy::too_many_arguments)]
    fn submit_display(
        &self,
        assets: &WorldResources,
        entity: &ClientEntity,
        camera: DVec3,
        partial: f32,
        view: (f32, f32),
        params: &LightmapParams,
        out: &mut Vec<EntityDraw>,
    ) {
        use crate::world::entity::display;
        let Some(state) = entity.display.as_deref() else {
            return;
        };
        let at = entity.pos(partial);
        if at.distance_squared(camera) as f32 >= (state.view_range * 64.0).powi(2) {
            return;
        }
        let origin = at.floor().as_ivec3();
        let feet = at.as_vec3();
        let tick = entity.age_in_ticks(0.0) as i32;
        let facing = display::orientation(
            state.billboard,
            (entity.yaw_degrees(partial), entity.pitch_degrees(partial)),
            view,
        );
        let model = Mat4::from_translation(from_block(at, origin))
            * Mat4::from_quat(facing)
            * state.transform(tick, partial).matrix();
        let light = match state.light_override() {
            Some((block, sky)) => lightmap::sample(block, sky, params),
            None => self.light_at_eye(feet + Vec3::Y * state.height * 0.5, 0, params),
        };
        let placed = Placed {
            origin,
            layer: RenderLayer::Entities,
            light,
        };
        match entity.kind {
            EntityKind::ItemDisplay => {
                let pose = HandPose::from_display_context(state.item_context);
                submit_item(
                    assets,
                    &state.item,
                    pose,
                    false,
                    model * Mat4::from_rotation_y(std::f32::consts::PI),
                    placed,
                    out,
                );
            }
            EntityKind::BlockDisplay if state.block_state != 0 => {
                let at = block_at(entity);
                if let Some(mesh) = assets
                    .blocks
                    .mesh(state.block_state, |kind| self.level.tint_at(kind, at))
                {
                    out.push(EntityDraw {
                        scroll: None,
                        alpha: 1.0,
                        mesh,
                        atlas: assets.atlas,
                        model,
                        origin,
                        layer: RenderLayer::Entities,
                        blend: crate::renderer::Blending::Opaque,
                        light: Some(light),
                        overlay: 0.0,
                        glint: Glint::None,
                    });
                }
            }
            _ => {}
        }
    }

    /// `ItemFrameRenderer`.
    fn submit_item_frame(
        &self,
        assets: &WorldResources,
        entity: &ClientEntity,
        partial: f32,
        params: &LightmapParams,
        out: &mut Vec<EntityDraw>,
    ) {
        let at = entity.pos(partial);
        let origin = at.floor().as_ivec3();
        let feet = at.as_vec3();
        let glow = entity.kind == EntityKind::GlowItemFrame;
        let light = if glow {
            self.light_at_eye(feet, 5, params)
        } else {
            self.light_at_eye(feet, 0, params)
        };
        let placed = Placed {
            origin,
            layer: RenderLayer::Entities,
            light,
        };
        let (step, y_rot, x_rot) = match entity.hang {
            0 => (Vec3::NEG_Y, 180.0, 90.0),
            1 => (Vec3::Y, 180.0, -90.0),
            2 => (Vec3::NEG_Z, 0.0, 0.0),
            4 => (Vec3::NEG_X, 90.0, 0.0),
            5 => (Vec3::X, -90.0, 0.0),
            _ => (Vec3::Z, 180.0, 0.0),
        };
        let base = Mat4::from_translation(from_block(at, origin) + step * 0.46875)
            * Mat4::rot_x_deg(x_rot)
            * Mat4::rot_y_deg(y_rot);
        let map = entity.item.item.registry_key == "filled_map";
        let model = FRAME_MODELS[usize::from(glow) * 2 + usize::from(map)];
        let frame = assets.held.block(model);
        for (mesh, atlas, translucent) in frame.filter(|_| !entity.appearance.invisible()) {
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas,
                model: base,
                origin,
                layer: RenderLayer::Entities,
                blend: crate::renderer::Blending::of(translucent),
                light: Some(light),
                overlay: 0.0,
                glint: Glint::None,
            });
        }
        if entity.item.is_empty() {
            return;
        }
        let depth = if entity.appearance.invisible() {
            0.5
        } else {
            0.4375
        };
        let turn = (entity.frame_turn % 8) as f32 * 45.0;
        let at = base
            * Mat4::from_translation(Vec3::new(0.0, 0.0, depth))
            * Mat4::rot_z_deg(turn)
            * Mat4::from_scale(Vec3::splat(0.5));
        let item_light = if glow {
            self.light_at_eye(feet, 15, params)
        } else {
            light
        };
        submit_item(
            assets,
            &entity.item,
            HandPose::Fixed,
            false,
            at,
            Placed {
                light: item_light,
                ..placed
            },
            out,
        );
    }

    /// `Entity.getTeamColor` of a glowing entity.
    fn outline_color(
        &self,
        entity: &crate::world::entity::tracker::ClientEntity,
    ) -> Option<[u8; 3]> {
        if !entity.appearance.glowing() {
            return None;
        }
        // The UUID as text on the stack: no string a frame per glowing entity.
        let mut uuid_text = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let holder: &str = match self.players.get(&entity.uuid) {
            Some(player) if entity.kind == EntityKind::Player => &player.name,
            _ => entity.uuid.hyphenated().encode_lower(&mut uuid_text),
        };
        let color = self
            .scoreboard
            .team_of(holder)
            .and_then(|(_, team)| team.look.color)
            .map_or([255, 255, 255], |c| {
                c.named().rgb().map(|v| (v * 255.0).round() as u8)
            });
        Some(color)
    }

    /// The items flying into hands (`ItemPickupParticle`).
    pub(super) fn pickup_draws(
        &self,
        assets: &WorldResources,
        partial: f32,
        params: &LightmapParams,
        out: &mut Vec<EntityDraw>,
    ) {
        for pickup in self.entities.pickups() {
            let at = pickup.position(partial);
            let placed = Placed {
                origin: at.as_vec3().floor().as_ivec3(),
                layer: RenderLayer::Entities,
                light: self.light_at_eye(at.as_vec3() + Vec3::Y * 0.2125, 0, params),
            };
            submit_dropped(
                assets,
                &pickup.stack,
                at,
                pickup.age,
                pickup.bob_offs,
                placed,
                out,
            );
        }
    }

    /// The own player in third person: its body, armor, items and shadow.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn own_avatar_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        state: AvatarRenderState,
        main_arm: Arm,
        camera: DVec3,
        light: [f32; 3],
        out: &mut Vec<EntityDraw>,
    ) {
        let mut state = state;
        let at = self.player.pos();
        let origin = at.floor().as_ivec3();
        let feet = at.as_vec3();
        state.position = from_block(at, origin);
        let hands = Hand::ALL.map(|hand| {
            let stack = self.hand_stack(hand);
            (
                stack,
                self.held_use(hand, stack, self.hands.using_in(hand).is_some()),
            )
        });
        let spectator = state.spectator;
        let avatar = Avatar {
            state,
            look: assets.players.for_profile(self.uuid),
            main_arm,
            hands,
            armor: self.own_armor(),
            light,
            overlay: mobs::hurt_overlay(self.body.hurt > 0 || self.body.death > 0),
        };
        submit_avatar(assets, &avatar, origin, RenderLayer::Entities, out);
        if !spectator {
            // `LivingEntityRenderer.getShadowRadius`: times the `scale`
            // attribute.
            self.entity_meshes.shadows.draw(
                renderer,
                &self.level,
                OWN,
                feet,
                camera.as_vec3(),
                (
                    crate::world::shadow::PLAYER_RADIUS * self.player.scale() as f32,
                    1.0,
                ),
                self.shadow_light(),
                assets.shadow,
                out,
            );
        }
    }
}

/// `LivingEntityRenderer`.
fn shows_when_invisible(entity: &crate::world::entity::tracker::ClientEntity) -> bool {
    !entity.appearance.invisible()
        || (mobs::wears_armor(entity.kind) && !entity.equipment.is_empty())
}

/// `point` from the block `origin`, worked out in doubles: a model
/// placed a million blocks out is as exact as one at the origin.
#[inline]
fn from_block(point: DVec3, origin: IVec3) -> Vec3 {
    (point - origin.as_dvec3()).as_vec3()
}

/// The tracked entities Melon draws that the camera can see, by id.
pub(super) fn visible_entities(
    entities: &Entities,
    culler: &Culler,
    partial: f32,
    compiled: impl Fn(SectionPos) -> bool,
) -> Vec<i32> {
    entities
        .iter()
        .filter(|e| e.kind != EntityKind::Other && shows_when_invisible(e))
        .filter(|e| {
            // `LightningBoltRenderer.affectedByCulling`: never culled; nor
            // a crystal casting a beam (`EndCrystalRenderer.shouldRender`).
            if e.kind == EntityKind::LightningBolt
                || e.kind == EntityKind::EndCrystal && e.appearance.thing.beam_target.is_some()
            {
                return true;
            }
            if e.kind.is_display()
                && e.display
                    .as_deref()
                    .is_none_or(|d| d.width == 0.0 || d.height == 0.0)
            {
                return true;
            }
            let (min, max) = e.bounds(partial);
            culler.sees_entity(min, max) && culler.in_compiled_section(min, max, &compiled)
        })
        .map(|e| e.id)
        .collect()
}

/// Another player as the tracker knows it.
#[allow(clippy::too_many_arguments)]
fn remote_avatar<'a>(
    entity: &'a ClientEntity,
    look: PlayerLook<'a>,
    origin: IVec3,
    partial: f32,
    light: [f32; 3],
    spectator: bool,
    upside_down: bool,
    fishing: bool,
) -> Avatar<'a> {
    let input = mobs::pose_input(entity, partial);
    let main_arm = entity.appearance.main_arm(EntityKind::Player);
    let mut humanoid = input.humanoid;
    let using = entity.appearance.using_hand();
    let pose = |hand: Hand| {
        let stack = entity.equipment.hand(hand);
        if stack.is_empty() {
            return ArmPose::Empty;
        }
        if using == Some(hand)
            && let Some(item_use) = ItemUse::of(stack)
        {
            return item_use.animation.arm_pose();
        }
        ArmPose::Item
    };
    let arms = ArmPoses::new(main_arm, pose(Hand::Main), pose(Hand::Off));
    humanoid.right_arm_pose = arms.right;
    humanoid.left_arm_pose = arms.left;
    let mut state = AvatarRenderState {
        humanoid,
        position: from_block(entity.pos(partial), origin),
        body_yaw: input.living.body_rot,
        spectator,
        shown: entity.appearance.shown,
        death_time: input.living.death_time,
        swim: input.living.swim,
        upside_down: None,
        scale: input.living.scale,
    };
    if upside_down {
        state.turn_upside_down(entity.size().1);
    }
    Avatar {
        state,
        look,
        main_arm,
        hands: Hand::ALL.map(|hand| {
            let stack = entity.equipment.hand(hand);
            (
                stack,
                remote_held_use(entity, hand, using == Some(hand), fishing),
            )
        }),
        armor: entity.equipment.armor(),
        light,
        overlay: mobs::overlay(entity, &input.living, partial),
    }
}

/// `AvatarRenderer.submit`: the body, then the armor and items in hand.
pub(super) fn submit_avatar(
    assets: &WorldResources,
    avatar: &Avatar,
    origin: IVec3,
    layer: RenderLayer,
    out: &mut Vec<EntityDraw>,
) {
    let player = &avatar.look.model.model;
    let pose = player.pose(&avatar.state);
    let root = PlayerModel::root_transform(&avatar.state);
    let matrices = player.model.part_matrices(&pose, root);
    let placed = Placed {
        origin,
        layer,
        light: avatar.light,
    };
    let overlay_parts = player.overlay_parts();
    let style = DrawStyle {
        texture: avatar.look.texture,
        layer,
        light: Some(avatar.light),
        overlay: avatar.overlay,
        glint: Glint::None,
    };
    avatar
        .look
        .model
        .meshes
        .draws_inner(&matrices, origin, style, &overlay_parts, out);
    avatar
        .look
        .model
        .meshes
        .draws_overlay(&matrices, origin, style, &overlay_parts, out);
    if avatar.state.spectator {
        return;
    }
    let wearer = (&player.model, &player.humanoid, &pose);
    let mut worn = Vec::new();
    mobs::push_humanoid_armor(EntityKind::Player, false, avatar.armor, &mut worn);
    submit_worn(assets, wearer, root, &worn, placed, out);
    super::heads::submit_worn_head(
        assets,
        avatar.armor[0],
        &super::heads::Wearer {
            model: &player.model,
            pose: &pose,
            head: player.humanoid.head,
            transforms: mobs::custom_head::HeadTransforms::DEFAULT,
            walk_position: avatar.state.humanoid.walk_position,
        },
        root,
        origin,
        layer,
        avatar.light,
        out,
    );
    for (hand, (stack, using)) in Hand::ALL.into_iter().zip(avatar.hands) {
        if stack.is_empty() {
            continue;
        }
        let arm = hand.arm(avatar.main_arm);
        let at = player_model::item_in_hand(player.hand_transform(&pose, arm, root), arm);
        submit_item(
            assets,
            stack,
            HandPose::third_person(arm),
            using,
            at,
            placed,
            out,
        );
    }
}

/// The block an entity shows as one: an enderman's, a falling one.
fn shown_block(entity: &ClientEntity) -> Option<u16> {
    let state = entity.appearance.block_state;
    let shows = match entity.kind {
        EntityKind::Enderman => entity.appearance.carrying(),
        EntityKind::FallingBlock | EntityKind::PrimedTnt => state != 0,
        _ => false,
    };
    shows.then_some(state)
}

/// Where its biome tint is read, stable within a tick so the mesh isn't rebuilt.
fn block_at(entity: &ClientEntity) -> [i32; 3] {
    let at = entity.tick_pos().floor();
    [at.x as i32, at.y as i32, at.z as i32]
}

/// `FallingBlockRenderer.submit`.
fn submit_block_entity(
    assets: &WorldResources,
    entity: &ClientEntity,
    placed: Placed,
    partial: f32,
    tint: impl FnOnce(TintKind) -> [u8; 3],
    out: &mut Vec<EntityDraw>,
) {
    let Some(state) = shown_block(entity) else {
        return;
    };
    let Some(mesh) = assets.blocks.mesh(state, tint) else {
        return;
    };
    let feet = from_block(entity.pos(partial), placed.origin);
    let (model, overlay) = if entity.kind == EntityKind::PrimedTnt {
        let fuse = entity.fuse_remaining(partial);
        let scale = if fuse < 10.0 {
            1.0 + tnt::swell_amount(fuse)
        } else {
            1.0
        };
        (
            Mat4::from_translation(feet + Vec3::Y * 0.5)
                * Mat4::from_scale(Vec3::splat(scale))
                * Mat4::from_translation(Vec3::splat(-0.5)),
            if tnt::is_lit(fuse) { -1.0 } else { 0.0 },
        )
    } else {
        (
            Mat4::from_translation(feet + Vec3::new(-0.5, 0.0, -0.5)),
            0.0,
        )
    };
    out.push(EntityDraw {
        scroll: None,
        alpha: 1.0,
        mesh,
        atlas: assets.atlas,
        model,
        origin: placed.origin,
        layer: placed.layer,
        blend: crate::renderer::Blending::Opaque,
        light: Some(placed.light),
        overlay,
        glint: Glint::None,
    });
}

/// What a minecart carries, and how high.
fn minecart_block(entity: &ClientEntity) -> Option<(u16, i32)> {
    let thing = &entity.appearance.thing;
    mobs::vehicle::minecart_block(
        entity.ty.resource_name,
        thing.display_block,
        thing.display_offset,
        thing.fuel,
    )
}

/// `AbstractMinecartRenderer.submit`'s contents: the carried block, and
/// a TNT cart's swelling, flashing fuse (`TntMinecartRenderer`).
fn submit_minecart_block(
    assets: &WorldResources,
    entity: &ClientEntity,
    placed: Placed,
    partial: f32,
    tint: impl FnOnce(TintKind) -> [u8; 3],
    out: &mut Vec<EntityDraw>,
) {
    let Some((state, offset)) = minecart_block(entity) else {
        return;
    };
    let Some(mesh) = assets.blocks.mesh(state, tint) else {
        return;
    };
    let mut input = mobs::pose_input(entity, partial);
    input.living.position = from_block(entity.pos(partial), placed.origin);
    let root = mobs::root(entity, &input, partial);
    let tnt = entity.ty.resource_name == "tnt_minecart";
    let fuse = if entity.fuse_ticks > -1 {
        entity.fuse_ticks as f32 - partial + 1.0
    } else {
        -1.0
    };
    let swell = (tnt && fuse > -1.0 && fuse < 10.0).then(|| tnt::swell_amount(fuse));
    out.push(EntityDraw {
        scroll: None,
        alpha: 1.0,
        mesh,
        atlas: assets.atlas,
        model: mobs::vehicle::minecart_block_matrix(root, offset, swell),
        origin: placed.origin,
        layer: placed.layer,
        blend: crate::renderer::Blending::Opaque,
        light: Some(placed.light),
        overlay: if tnt && tnt::is_lit(fuse) { -1.0 } else { 0.0 },
        glint: Glint::None,
    });
}

/// `SulfurCubeInnerLayer`: the block a sulfur cube swallowed, in place of
/// its core, in the model's space (flipped back up, and halved for a
/// baby), flashing white with the fuse.
fn submit_swallowed_block(
    assets: &WorldResources,
    _poses: &mut PoseCache,
    entity: &ClientEntity,
    placed: Placed,
    partial: f32,
    tint: impl FnOnce(TintKind) -> [u8; 3],
    out: &mut Vec<EntityDraw>,
) {
    let Some(state) = mobs::sulfur_cube::swallowed_block(entity) else {
        return;
    };
    let Some(mesh) = assets.blocks.mesh(state, tint) else {
        return;
    };
    let mut input = mobs::pose_input(entity, partial);
    input.living.position = from_block(entity.pos(partial), placed.origin);
    let mut model = mobs::root(entity, &input, partial) * Mat4::rot_x_deg(180.0);
    if entity.appearance.baby {
        model *= Mat4::from_scale(Vec3::splat(0.5));
    }
    model *= Mat4::from_translation(Vec3::new(-0.5, -0.518, -0.5));
    let fuse = mobs::sulfur_cube::fuse_remaining(entity.fuse_ticks, partial);
    let overlay = if mobs::sulfur_cube::core_flashes(fuse) {
        -1.0
    } else {
        mobs::overlay(entity, &input.living, partial)
    };
    out.push(EntityDraw {
        scroll: None,
        alpha: 1.0,
        mesh,
        atlas: assets.atlas,
        model,
        origin: placed.origin,
        layer: placed.layer,
        blend: crate::renderer::Blending::Opaque,
        light: Some(placed.light),
        overlay,
        glint: Glint::None,
    });
}

/// `ItemEntityRenderer.submit`.
fn submit_item_entity(
    assets: &WorldResources,
    entity: &ClientEntity,
    placed: Placed,
    partial: f32,
    out: &mut Vec<EntityDraw>,
) {
    let feet = entity.pos(partial);
    let age = entity.age_in_ticks(partial);
    submit_dropped(
        assets,
        &entity.item,
        feet,
        age,
        entity.bob_offs,
        placed,
        out,
    );
}

/// A dropped `stack` at `feet`, `age` ticks old.
fn submit_dropped(
    assets: &WorldResources,
    stack: &impl StackKey,
    feet: glam::DVec3,
    age: f32,
    bob_offs: f32,
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    let Some(held) = assets.held.held(stack, false) else {
        return;
    };
    let feet = from_block(feet, placed.origin);
    let lift = item_drop::bob(age, bob_offs) - held.ground().0 + 0.0625;
    let base = Mat4::from_translation(feet + Vec3::Y * lift)
        * Mat4::from_rotation_y(item_drop::spin(age, bob_offs));
    submit_held_cluster(
        assets,
        (stack.stack(), stack.foil()),
        held,
        base,
        placed,
        out,
    );
}

/// `ItemEntityRenderer.submitMultipleFromCount`: the stack drawn once per
/// few items, spread a little, from `base`.
pub(super) fn submit_item_cluster(
    assets: &WorldResources,
    stack: &ItemStack,
    base: Mat4,
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    if let Some(held) = assets.held.held(stack, false) {
        let foil = crate::world::glint::has_foil(stack);
        submit_held_cluster(assets, (stack, foil), held, base, placed, out);
    }
}

fn submit_held_cluster(
    assets: &WorldResources,
    (stack, foil): (&ItemStack, bool),
    held: Held<'_>,
    base: Mat4,
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    let seed = i64::from(stack.item.id);
    for offset in item_drop::offsets(stack.item_count, held.ground().1, seed) {
        let at = base * Mat4::from_translation(offset);
        submit_held(assets, foil, held.draws(HandPose::Ground), at, placed, out);
    }
}

/// A mob's layers, and a humanoid's armor and items in hand.
#[allow(clippy::too_many_arguments)]
fn submit_mob(
    assets: &WorldResources,
    poses: &mut PoseCache,
    entity: &ClientEntity,
    layers: &[LayerSpec],
    placed: Placed,
    partial: f32,
    tint: impl FnOnce(TintKind) -> [u8; 3],
    out: &mut Vec<EntityDraw>,
) {
    let hide_body = entity.appearance.invisible();
    let mut input = mobs::pose_input(entity, partial);
    input.living.position = from_block(entity.pos(partial), placed.origin);
    let root = mobs::root(entity, &input, partial);
    let overlay = mobs::overlay(entity, &input.living, partial);
    let mut poses = Some(poses);
    let mut own = Vec::new();
    let mut matrices: &[Option<Mat4>] = &[];
    let mut body = None;
    let mut last: Option<(ModelId, &MobModel, ModelPose)> = None;
    for layer in layers {
        let (mob, meshes) = assets.mobs.get(layer.model);
        let Some(texture) = assets.mobs.texture(&layer.texture) else {
            continue;
        };
        if last.as_ref().is_none_or(|(id, ..)| *id != layer.model) {
            let pose = if layer.copy_humanoid {
                let source = body
                    .as_ref()
                    .map(|(m, p)| (*m, p))
                    .or_else(|| last.as_ref().map(|(_, m, p)| (*m, p)));
                source
                    .and_then(|(from_mob, from_pose)| {
                        let parts = from_mob.humanoid()?;
                        mob.humanoid()?;
                        Some(mobs::pose_worn((&from_mob.model, parts, from_pose), mob))
                    })
                    .unwrap_or_else(|| mob.model.initial_pose())
            } else {
                mob.pose(&input)
            };
            let shared = !layer.copy_humanoid
                && last.as_ref().is_some_and(|(_, before, before_pose)| {
                    *before_pose == pose && mob.model.shares_tree(&before.model)
                });
            if !shared {
                matrices = match poses.take() {
                    Some(cache) if !layer.copy_humanoid => {
                        cache.matrices(entity.id, layer.model, &mob.model, &pose, root)
                    }
                    _ => {
                        mob.model.part_matrices_into(&pose, root, &mut own);
                        &own
                    }
                };
            }
            if let Some((_, before, before_pose)) = last.replace((layer.model, mob, pose))
                && body.is_none()
            {
                body = Some((before, before_pose));
            }
        }
        let light = if layer.full_bright {
            layer.tint
        } else {
            tinted(placed.light, layer.tint)
        };
        if hide_body && !layer.copy_humanoid {
            continue;
        }
        let first = out.len();
        meshes.draws(
            matrices,
            placed.origin,
            DrawStyle {
                texture,
                layer: if layer.water_mask {
                    RenderLayer::WaterMask
                } else {
                    placed.layer
                },
                light: Some(light),
                overlay: match layer.overlay {
                    _ if layer.copy_humanoid => 0.0,
                    Some(own) => own,
                    None => overlay,
                },
                // `entitySolidGlint`: a thrown trident's glint is an
                // entity's, over its own texture; worn things the armor's.
                glint: if entity.kind == EntityKind::Trident {
                    Glint::entity(layer.foil, 16)
                } else {
                    Glint::armor(layer.foil)
                },
            },
            out,
        );
        // `entityTranslucent`: lit and sorted with the other entities.
        if layer.translucent {
            for draw in &mut out[first..] {
                draw.blend = crate::renderer::Blending::Translucent;
            }
        }
        if layer.alpha < 1.0 {
            for draw in &mut out[first..] {
                draw.alpha = layer.alpha;
            }
        }
        if let Some(scroll) = layer.scroll {
            for draw in &mut out[first..] {
                draw.scroll = Some(scroll);
                draw.blend = if layer.additive {
                    crate::renderer::Blending::Additive
                } else {
                    crate::renderer::Blending::Translucent
                };
            }
        }
    }
    if let Some(model) = mobs::cow::mushroom_model(entity)
        && let Some(Rig::Quadruped(parts)) = last.as_ref().map(|(_, mob, _)| &mob.rig)
        && let Some(Some(head)) = matrices.get(parts.head.0)
    {
        submit_mushrooms(assets, model, root, *head, placed, overlay, out);
    }
    let Some((mob, pose)) = body.or(last.map(|(_, mob, pose)| (mob, pose))) else {
        return;
    };
    if let Some((head, transforms)) = mobs::custom_head::custom_head(entity.kind, mob) {
        super::heads::submit_worn_head(
            assets,
            entity.equipment.armor()[0],
            &super::heads::Wearer {
                model: &mob.model,
                pose: &pose,
                head,
                transforms,
                walk_position: input.living.walk_position,
            },
            root,
            placed.origin,
            placed.layer,
            placed.light,
            out,
        );
    }
    if entity.kind == EntityKind::Enderman
        && let Some(state) = shown_block(entity)
        && let Some(mesh) = assets.blocks.mesh(state, tint)
    {
        out.push(EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh,
            atlas: assets.atlas,
            model: mobs::enderman::carried_block_transform(root),
            origin: placed.origin,
            layer: placed.layer,
            blend: crate::renderer::Blending::Opaque,
            light: Some(placed.light),
            overlay: 0.0,
            glint: Glint::None,
        });
    }
    if let Rig::Golem(parts) = &mob.rig
        && entity.golem_pose(partial).offer_flower > 0
        && let Some(Some(arm)) = matrices.get(parts.flower_arm().0)
    {
        submit_flower(assets, *arm, placed, out);
    }
    if let Rig::SnowGolem(parts) = &mob.rig
        && entity.appearance.pumpkin
        && let Some(Some(head)) = matrices.get(parts.head().0)
    {
        submit_pumpkin(assets, *head, placed, overlay, out);
    }
    if entity.kind == EntityKind::Dolphin {
        let stack = entity.equipment.hand(Hand::Main);
        if !stack.is_empty() {
            let at = mobs::mouth_item::dolphin(root, input.living.x_rot);
            submit_item(assets, stack, HandPose::Ground, false, at, placed, out);
        }
        return;
    }
    if entity.kind == EntityKind::Fox
        && let Rig::Fox(parts) = &mob.rig
    {
        let stack = entity.equipment.hand(Hand::Main);
        if !stack.is_empty() {
            let at = mobs::mouth_item::fox(
                root,
                mobs::mouth_item::FoxMouth {
                    head: mobs::fox::head_blocks(parts, &pose),
                    baby: entity.appearance.baby,
                    sleeping: input.fox.sleeping,
                    head_roll: input.fox.head_roll,
                    y_rot: input.living.y_rot,
                    x_rot: input.living.x_rot,
                },
            );
            submit_item(assets, stack, HandPose::Ground, false, at, placed, out);
        }
        return;
    }
    if let Rig::Villager(parts) = &mob.rig {
        let stack = entity.equipment.hand(Hand::Main);
        if !stack.is_empty() {
            let potion = entity.kind == EntityKind::Witch && stack.item.id == Item::POTION.id;
            let at = if potion {
                villager::potion_transform(&mob.model, parts, &pose, root)
            } else {
                villager::crossed_arms_transform(&mob.model, parts, &pose, root)
            };
            submit_item(assets, stack, HandPose::Ground, false, at, placed, out);
        }
        return;
    }
    let Some(parts) = mob.humanoid() else {
        return;
    };
    if !mobs::shows_held_item(entity) {
        return;
    }
    let outward = if entity.kind.is_skeleton() { 1.0 } else { 0.0 };
    let main_arm = entity.appearance.main_arm(entity.kind);
    for hand in Hand::ALL {
        let stack = entity.equipment.hand(hand);
        if stack.is_empty() {
            continue;
        }
        let arm = hand.arm(main_arm);
        let at = player_model::item_in_hand(
            humanoid::hand_transform(&mob.model, parts, &pose, arm, outward, root),
            arm,
        );
        submit_item(
            assets,
            stack,
            HandPose::third_person(arm),
            false,
            at,
            placed,
            out,
        );
    }
}

/// `IronGolemFlowerLayer`: a poppy held out in the right arm.
fn submit_flower(assets: &WorldResources, arm: Mat4, placed: Placed, out: &mut Vec<EntityDraw>) {
    let parts: Vec<_> = assets.held.block(mobs::golem::FLOWER_MODEL).collect();
    let model = arm
        * Mat4::from_translation(Vec3::new(-1.1875 + 0.5, 1.0625 + 0.5, -0.9375 + 0.5))
        * Mat4::from_scale(Vec3::splat(0.5))
        * Mat4::rot_x_deg(-90.0);
    for &(mesh, atlas, translucent) in &parts {
        out.push(EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh,
            atlas,
            model,
            origin: placed.origin,
            layer: placed.layer,
            blend: crate::renderer::Blending::of(translucent),
            light: Some(placed.light),
            overlay: 0.0,
            glint: Glint::None,
        });
    }
}

/// `SnowGolemHeadLayer`.
fn submit_pumpkin(
    assets: &WorldResources,
    head: Mat4,
    placed: Placed,
    overlay: f32,
    out: &mut Vec<EntityDraw>,
) {
    let parts: Vec<_> = assets.held.block(mobs::snow_golem::HEAD).collect();
    let model = head
        * Mat4::from_translation(Vec3::new(0.0, -0.34375, 0.0))
        * Mat4::rot_y_deg(180.0)
        * Mat4::from_scale(Vec3::new(0.625, -0.625, -0.625));
    for &(mesh, atlas, translucent) in &parts {
        out.push(EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh,
            atlas,
            model,
            origin: placed.origin,
            layer: placed.layer,
            blend: crate::renderer::Blending::of(translucent),
            light: Some(placed.light),
            overlay,
            glint: Glint::None,
        });
    }
}

/// `MushroomCowMushroomLayer`.
fn submit_mushrooms(
    assets: &WorldResources,
    model: &str,
    root: Mat4,
    head: Mat4,
    placed: Placed,
    overlay: f32,
    out: &mut Vec<EntityDraw>,
) {
    let parts: Vec<_> = assets.held.block(model).collect();
    let flip = Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0));
    let back = root * Mat4::from_translation(Vec3::new(0.2, -0.35, 0.5));
    let places = [
        back * Mat4::rot_y_deg(-48.0) * flip,
        back * Mat4::rot_y_deg(42.0)
            * Mat4::from_translation(Vec3::new(0.1, 0.0, -0.6))
            * Mat4::rot_y_deg(-48.0)
            * flip,
        head * Mat4::from_translation(Vec3::new(0.0, -0.7, -0.2)) * Mat4::rot_y_deg(-78.0) * flip,
    ];
    for model in places {
        for &(mesh, atlas, translucent) in &parts {
            out.push(EntityDraw {
                scroll: None,
                alpha: 1.0,
                mesh,
                atlas,
                model,
                origin: placed.origin,
                layer: placed.layer,
                blend: crate::renderer::Blending::of(translucent),
                light: Some(placed.light),
                overlay,
                glint: Glint::None,
            });
        }
    }
}

/// `HumanoidArmorLayer` and every other worn extra layer.
fn submit_worn(
    assets: &WorldResources,
    wearer: (&Model, &HumanoidParts, &ModelPose),
    root: Mat4,
    layers: &[LayerSpec],
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    for layer in layers {
        if !layer.copy_humanoid {
            continue;
        }
        let (mob, meshes) = assets.mobs.get(layer.model);
        let Some(texture) = assets.mobs.texture(&layer.texture) else {
            continue;
        };
        let pose = mobs::pose_worn(wearer, mob);
        let matrices = mob.model.part_matrices(&pose, root);
        meshes.draws(
            &matrices,
            placed.origin,
            DrawStyle {
                texture,
                layer: placed.layer,
                light: Some(tinted(placed.light, layer.tint)),
                overlay: 0.0,
                glint: Glint::armor(layer.foil),
            },
            out,
        );
    }
}

/// `ItemInHandLayer`: `stack` held in `arm`, placed by `at`.
pub(super) fn submit_item(
    assets: &WorldResources,
    stack: &impl StackKey,
    pose: HandPose,
    using: impl Into<HeldUse>,
    at: Mat4,
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    let parts = assets.held.get(stack, pose, using);
    submit_held(assets, stack.foil(), parts, at, placed, out);
}

fn submit_held(
    assets: &WorldResources,
    foil: bool,
    parts: impl Iterator<Item = (MeshId, TextureId, bool)>,
    at: Mat4,
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    for (mesh, atlas, translucent) in parts {
        out.push(EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh,
            atlas,
            model: at,
            origin: placed.origin,
            layer: placed.layer,
            blend: crate::renderer::Blending::of(translucent),
            light: Some(placed.light),
            overlay: 0.0,
            glint: Glint::entity(foil, assets.held.glint_texels(atlas)),
        });
    }
}

fn tinted(light: [f32; 3], tint: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| light[i] * tint[i])
}

/// An item frame's block models.
const FRAME_MODELS: [&str; 4] = [
    "block/item_frame",
    "block/item_frame_map",
    "block/glow_item_frame",
    "block/glow_item_frame_map",
];

/// Marks the draws `range` as glowing in `colour`, for the entity outline.
fn mark_glowing(
    glowing: &mut Vec<(usize, [u8; 3])>,
    range: std::ops::Range<usize>,
    colour: Option<[u8; 3]>,
) {
    if let Some(colour) = colour {
        glowing.extend(range.map(|i| (i, colour)));
    }
}

/// How the stack in `hand` of another entity is used, as its model sees it.
fn remote_held_use(entity: &ClientEntity, hand: Hand, using: bool, fishing: bool) -> HeldUse {
    if using {
        return HeldUse::Using;
    }
    let rod = |h: Hand| entity.equipment.hand(h).item.registry_key == "fishing_rod";
    if rod(hand)
        && crate::world::entity::fishing::rod_looks_cast(
            fishing,
            hand == Hand::Main,
            rod(Hand::Main),
        )
    {
        HeldUse::Cast
    } else {
        HeldUse::Idle
    }
}
