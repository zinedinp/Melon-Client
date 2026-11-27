//! The particles in the world.

use glam::{DVec3, IVec3, Vec3};

use super::InWorld;
use super::camera::Camera;
use super::controls::WorldContext;
use super::particle_meshes::ParticleMeshes;
use super::world_resources::WorldResources;
use melon_net::play::game::EquipmentSlot;
use melon_net::play::game::sound::{Explosion, LevelEvent};
use melon_render::{ParticleDraw, Renderer};
use melon_world::entity::tracker::Entities;
use melon_world::inventory::Inventory;
use melon_world::particle::client::{self, entity as own};
use melon_world::particle::mesh::View;
use melon_world::particle::server::{LevelParticles, entity_event, level_event};
use melon_world::particle::{EntityBox, Kind, ParticleEngine, Spawn, World};
use melon_world::player::PlayerState;

/// A level event 2001 for a block this client broke itself within this many ticks is its own.
const OWN_BREAK_TICKS: u64 = 20;

/// What the world keeps of its particles.
#[derive(Default)]
pub(super) struct Particles {
    pub engine: ParticleEngine,
    pub meshes: ParticleMeshes,
    /// Blocks broken here lately, and when.
    own_breaks: Vec<([i32; 3], u64)>,
    /// `wasTouchingWater`, for the splash going in.
    was_in_water: bool,
}

/// `getNearestPlayer(pos, distance, false)`: the local player or another.
fn nearest_player(
    entities: &Entities,
    player: &PlayerState,
    own: i32,
    at: DVec3,
    distance: f64,
) -> Option<EntityBox> {
    let others = entities
        .iter()
        .filter(|e| e.ty.resource_name == "player" && e.id != own)
        .map(|e| e.id);
    std::iter::once(own)
        .chain(others)
        .filter_map(|id| entity_box(entities, player, own, id))
        .map(|b| (b.pos.distance_squared(at), b))
        .filter(|(d, _)| *d < distance * distance)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, b)| b)
}

/// The item in an entity's `slot`, the local player's from the inventory.
fn equipment_item(
    entities: &Entities,
    inventory: &Inventory,
    own: i32,
    id: i32,
    slot: EquipmentSlot,
) -> Option<u16> {
    use melon_world::inventory::{ARMOR_SLOTS, OFFHAND_SLOT};
    let stack = if id == own {
        let armor = |i: usize| inventory.slots().get(ARMOR_SLOTS.start + i).map(|s| &**s);
        match slot {
            EquipmentSlot::MainHand => Some(&**inventory.selected_stack()),
            EquipmentSlot::OffHand => inventory.slots().get(OFFHAND_SLOT).map(|s| &**s),
            EquipmentSlot::Head => armor(0),
            EquipmentSlot::Chest => armor(1),
            EquipmentSlot::Legs => armor(2),
            EquipmentSlot::Feet => armor(3),
            EquipmentSlot::Body | EquipmentSlot::Saddle => None,
        }
    } else {
        entities.get(id).map(|e| e.equipment.get(slot))
    }?;
    (!stack.is_empty()).then_some(stack.item.id)
}

/// An entity's box for the particles, the local player's included.
fn entity_box(entities: &Entities, player: &PlayerState, own: i32, id: i32) -> Option<EntityBox> {
    if id == own {
        return Some(EntityBox {
            pos: player.feet(),
            prev: player.feet(),
            width: 0.6,
            height: player.box_height() as f32,
            vel: player.velocity(),
            name: "player",
            yaw: super::minecraft_yaw(player.yaw()),
            pitch: player.pitch(),
        });
    }
    let entity = entities.get(id)?;
    let (width, height) = entity.size();
    Some(EntityBox {
        pos: entity.tick_pos(),
        prev: entity.prev_tick_pos(),
        width,
        height,
        vel: entity.velocity(),
        name: entity.ty.resource_name,
        yaw: entity.yaw_degrees(1.0),
        pitch: entity.pitch_degrees(1.0),
    })
}

impl InWorld {
    /// Runs `f` with the engine and the world it spawns into.
    fn with_particles<R>(&mut self, f: impl FnOnce(&mut ParticleEngine, &World) -> R) -> R {
        let own = self.joined.session.info.entity_id;
        let (entities, player) = (&self.entities, &self.player);
        let lookup = |id: i32| entity_box(entities, player, own, id);
        let nearest = |at: DVec3, d: f64| nearest_player(entities, player, own, at, d);
        let inventory = &self.inventory;
        let equipment = |id: i32, slot| equipment_item(entities, inventory, own, id, slot);
        let world = World {
            nearest_player: &nearest,
            sky_light: self.effects.sky_light_level(),
            equipment: &equipment,
            scoping_eye: self.scoping_eye(),
            ..World::plain(&self.level, &lookup)
        };
        f(&mut self.particles.engine, &world)
    }

    /// The eyes looking through a spyglass in first person (`isScoping`).
    fn scoping_eye(&self) -> Option<DVec3> {
        let using = self.hands.using?;
        let first_person = self.camera_mode == super::camera::CameraMode::FirstPerson;
        let spyglass = self.hand_stack(using.hand).item.registry_key == "spyglass";
        (first_person && spyglass).then(|| self.player.tick_eye())
    }

    /// `ClientboundLevelParticlesPacket`.
    pub(super) fn on_level_particles(&mut self, packet: &LevelParticles) {
        let mut out = Vec::new();
        packet.spawns(self.particles.engine.rng(), &mut out);
        self.particles.engine.request_all(out);
    }

    /// `ClientboundAnimatePacket`'s sparks.
    pub(super) fn on_animate(&mut self, entity: i32, action: u8) {
        entity_event::animate(&mut self.particles.engine, entity, action);
    }

    /// `ClientboundEntityEventPacket`'s particles.
    pub(super) fn on_entity_event_particles(&mut self, entity: i32, event: u8) {
        self.with_particles(|engine, world| entity_event::spawn(engine, world, entity, event));
    }

    /// The particles of a level event.
    pub(super) fn level_event_particles(&mut self, event: &LevelEvent) {
        if event.event == level_event::id::DESTROY_BLOCK {
            let tick = self.ticks;
            let own =
                self.particles.own_breaks.iter().any(|(pos, at)| {
                    *pos == event.pos && tick.saturating_sub(*at) <= OWN_BREAK_TICKS
                });
            if own {
                return;
            }
        }
        let (id, pos, data) = (event.event, event.pos, event.data);
        self.with_particles(|engine, world| level_event::spawn(engine, world, id, pos, data));
    }

    /// The explosion packet's blast and debris (`handleExplosion`).
    pub(super) fn explosion_particles(&mut self, explosion: &Explosion) {
        let center = DVec3::from_array(explosion.center);
        let engine = &mut self.particles.engine;
        engine.request(Spawn::new(explosion.particle, center, DVec3::X));
        engine.explosions.track(
            center,
            explosion.radius,
            explosion.block_count,
            explosion.debris.clone(),
        );
    }

    /// The crumbs of a block this client broke (`spawnDestroyParticles`).
    pub(super) fn own_break_particles(&mut self, pos: [i32; 3], state: u16) {
        let tick = self.ticks;
        self.particles
            .own_breaks
            .retain(|(_, at)| tick.saturating_sub(*at) <= OWN_BREAK_TICKS);
        self.particles.own_breaks.push((pos, tick));
        self.with_particles(|engine, world| client::terrain::destroy(engine, world, pos, state));
    }

    /// A crumb off the face being mined (`addBreakingBlockEffects`).
    pub(super) fn mining_particles(
        &mut self,
        pos: [i32; 3],
        face: melon_world::block::direction::Direction,
    ) {
        self.with_particles(|engine, world| client::terrain::crack(engine, world, pos, face));
    }

    /// `spawnItemParticles` of eating or drinking `item`, `count` crumbs.
    pub(super) fn eating_particles(&mut self, item: u16, count: u32) {
        let eye = self.player.tick_eye();
        let yaw = super::minecraft_yaw(self.player.yaw());
        let pitch = self.player.pitch();
        let mut out = Vec::new();
        own::item_crumbs(
            item,
            eye,
            yaw,
            pitch,
            count,
            self.particles.engine.rng(),
            &mut out,
        );
        self.particles.engine.request_all(out);
    }

    /// One tick of the particles.
    pub(super) fn tick_particles(&mut self, ui: &WorldContext) {
        let config = ui.config;
        let eye = self.player.tick_eye();
        self.particles.engine.status = config.particles;
        self.particles.engine.camera = eye;
        let center = eye.floor().as_ivec3();
        self.tick_own_particles();
        let biomes = &self.level_info.biomes;
        let level = &self.level;
        let ambient = |pos: IVec3| -> &[(Kind, f32)] {
            level
                .biome(pos.x, pos.y, pos.z)
                .and_then(|id| biomes.biome(id))
                .map_or(&[], |b| b.ambient_particles.as_slice())
        };
        let rains_at = |pos: IVec3| {
            level
                .biome(pos.x, pos.y, pos.z)
                .and_then(|id| biomes.biome(id))
                .is_some_and(|b| b.precipitation() == melon_world::weather::Precipitation::Rain)
        };
        let rain = self.weather.rain;
        let raining_at = |pos: IVec3| {
            rain > 0.2 && level.motion_blocking_height(pos.x, pos.z) <= pos.y && rains_at(pos)
        };
        let own = self.joined.session.info.entity_id;
        let (entities, player) = (&self.entities, &self.player);
        let lookup = |id: i32| entity_box(entities, player, own, id);
        let nearest = |at: DVec3, d: f64| nearest_player(entities, player, own, at, d);
        let held = self.inventory.selected_stack().item;
        let creative = self.game_mode.current == melon_world::game_mode::GameMode::Creative;
        let marker = [
            pumpkin_data::item::Item::BARRIER.id,
            pumpkin_data::item::Item::LIGHT.id,
        ]
        .contains(&held.id)
        .then_some(held.registry_key)
        .filter(|_| creative);
        let world = World {
            nearest_player: &nearest,
            raining_at: &raining_at,
            sky_light: self.effects.sky_light_level(),
            marker,
            scoping_eye: self.scoping_eye(),
            thundering: self.weather.thunder > 0.9,
            game_time: self.clocks.game_time(),
            ..World::plain(level, &lookup)
        };
        let engine = &mut self.particles.engine;
        let mut out = Vec::new();
        client::weather::rain_splashes(
            &world,
            (center, i32::from(config.weather_radius)),
            self.weather.rain,
            config.particles,
            &rains_at,
            engine.rng(),
            &mut out,
        );
        client::blocks::campfires(&world, engine.rng(), center, &mut out);
        for e in entities.iter() {
            if e.id == own || e.tick_pos().distance_squared(eye) > 32.0 * 32.0 {
                continue;
            }
            let Some(body) = entity_box(entities, player, own, e.id) else {
                continue;
            };
            let ticking = client::mobs::Ticking {
                body,
                crouching: e.appearance.crouching(),
                sprinting: e.appearance.sprinting(),
            };
            client::mobs::tick(&world, &ticking, engine.rng(), &mut out);
        }
        engine.request_all(out);
        client::animate_tick(engine, &world, center, &ambient);
        engine.tick(&world);
    }

    /// The player's own.
    fn tick_own_particles(&mut self) {
        let feet = self.player.feet();
        let vel = self.player.velocity();
        let in_water = self.player.in_water();
        let mut out = Vec::new();
        let rng = self.particles.engine.rng();
        let spectator = self.game_mode.current.is_spectator();
        if self.player.sprinting() && !in_water && !spectator && !self.keys.sneak {
            let on = [
                feet.x.floor() as i32,
                (feet.y - 0.2).floor() as i32,
                feet.z.floor() as i32,
            ];
            let state = self.level.block_state(on[0], on[1], on[2]).unwrap_or(0);
            own::sprint_dust(feet, 0.6, vel, (on, state), rng, &mut out);
        }
        if in_water && !self.particles.was_in_water && !spectator && self.ticks > 1 {
            own::water_splash(feet, 0.6, vel, rng, &mut out);
        }
        self.particles.was_in_water = in_water;
        self.particles.engine.request_all(out);
    }

    /// This frame's particle draws.
    pub(super) fn particle_draws(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        camera: &Camera,
        frustum: &melon_world::culling::frustum::Frustum,
        partial: f32,
        out: &mut Vec<ParticleDraw>,
    ) {
        self.particles
            .engine
            .set_looks(&assets.models, &assets.particle_sprites);
        // `-left`, `up` and the yaw (`Camera.yRot`) of vanilla's camera.
        let right = -camera.left();
        let up = camera.up();
        let yaw = melon_core::math::angle::axis_angle(
            Vec3::Y,
            (180.0 - camera.look().y_rot) * melon_core::math::DEG_TO_RAD,
        );
        let view = View {
            origin: camera.position.floor().as_ivec3(),
            camera: camera.position,
            right,
            up,
            yaw,
            partial,
            frustum: Some(frustum),
        };
        self.particles.meshes.draws(
            renderer,
            (&assets.root, assets.atlas),
            self.particles.engine.quads(),
            &self.level,
            &view,
            out,
        );
    }

    /// Everything gone: a new level, a respawn, a dimension.
    pub(super) fn clear_particles(&mut self, renderer: &mut Renderer) {
        self.particles.engine.clear();
        self.particles.meshes.clear(renderer);
        self.particles.own_breaks.clear();
    }
}

/// Whether a stack spawns crumbs when used (`Consumable.hasConsumeParticles`).
pub(super) fn has_consume_particles(stack: &pumpkin_data::item_stack::ItemStack) -> bool {
    use pumpkin_data::data_component_impl::ConsumableImpl;
    stack
        .get_data_component::<ConsumableImpl>()
        .is_some_and(|c| c.consume_particles)
}
