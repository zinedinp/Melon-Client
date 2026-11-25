//! The local player.

use crate::world::game_mode::Abilities;
use crate::world::physics::Aabb;
use glam::DVec3;

mod attributes;
mod block_effects;
mod climb;
pub mod first_person;
mod glide;
pub mod hands;
pub mod mining;
mod mode;
mod movement;
pub use movement::{MoveState, Travel, priority as move_priority};
mod suffocate;
pub use mode::MoveMode;
mod render_state;
pub mod status_effects;

pub use attributes::PlayerAttributes;
pub use render_state::{ArmPoses, forward_vector_for};

/// `Avatar.STANDING_DIMENSIONS.withEyeHeight(1.62F)`.
pub const EYE_HEIGHT: f32 = 1.62;
/// `Avatar.POSES[Pose.CROUCHING]`: a `0.6 x 1.5` box with eye height `1.27`
pub const CROUCH_EYE_HEIGHT: f32 = 1.27;
/// `Avatar.POSES[Pose.SWIMMING]`: a `0.6 x 0.6` box with eye height `0.4`.
pub const SWIM_EYE_HEIGHT: f32 = 0.4;
const WIDTH: f64 = 0.6_f32 as f64;
const HEIGHT: f64 = 1.8_f32 as f64;
const CROUCH_HEIGHT: f64 = 1.5;
const SWIM_HEIGHT: f64 = 0.6_f32 as f64;
/// Seconds per tick.
pub const TICK: f32 = 0.05;

/// `Attributes.MOVEMENT_SPEED`'s player default.
const MOVEMENT_SPEED: f32 = 0.1;
/// `LivingEntity.SPEED_MODIFIER_SPRINTING`: +30%.
const SPRINT_SPEED_MODIFIER: f32 = 0.3;
/// `LocalPlayer.modifyInput`'s `0.98F` on both input axes.
const INPUT_DAMPING: f32 = 0.98;
const VERTICAL_DRAG: f64 = 0.98;
const HORIZONTAL_DRAG: f32 = 0.91;
/// `LivingEntity.getFrictionInfluencedSpeed`: `speed * 0.21600002F / f^3`
const GROUND_ACCELERATION: f32 = 0.216_000_02;
const AIR_ACCELERATION: f32 = 0.02;
/// `Player.getFlyingSpeed` while sprinting.
const SPRINT_AIR_ACCELERATION: f32 = 0.025_999_999;
/// `LivingEntity.jumpFromGround`'s sprint boost.
const SPRINT_JUMP_BOOST: f64 = 0.2;
/// `LivingEntity.noJumpDelay`, set after each jump while the key is held.
const JUMP_DELAY_TICKS: u32 = 10;
const WATER_ACCELERATION: f32 = 0.02;
const WATER_DRAG: f64 = 0.8;
const WATER_SPRINT_DRAG: f64 = 0.9;
const LAVA_DRAG: f64 = 0.5;
/// `LivingEntity.travelInWater`'s water drag under `DOLPHINS_GRACE`.
const DOLPHINS_GRACE_DRAG: f32 = 0.96;
/// `LivingEntity.jumpInLiquid` / `goDownInWater`.
const FLUID_JUMP: f64 = 0.04;
/// `FoodConstants.MAX_FOOD * 0.3`: below this the player can't sprint.
const SPRINT_FOOD_LEVEL: i32 = 6;
/// `Options.sprintWindow`'s default.
pub const SPRINT_WINDOW_DEFAULT: u32 = 7;
/// `Player.jumpTriggerTime`.
const FLY_TOGGLE_WINDOW_TICKS: u32 = 7;
/// `Player.travel`: what is left of the vertical speed each tick in flight.
const FLYING_VERTICAL_DRAG: f64 = 0.6;

/// Movement keys for one tick, gathered by the caller from window events.
#[derive(Default, Clone, Copy)]
pub struct MoveInput {
    /// `+1` forward, `-1` back.
    pub forward: f32,
    /// `+1` right, `-1` left.
    pub strafe: f32,
    pub jump: bool,
    pub sprint: bool,
    /// `Input.shift()`: the sneak key is held.
    pub sneak: bool,
}

pub struct PlayerState {
    /// The boxes of the entities that stop it near it (boats, ...).
    colliders: Vec<crate::world::physics::Aabb>,
    /// Feet position after the last tick.
    feet: DVec3,
    /// Feet position after the tick before, for interpolation.
    prev_feet: DVec3,
    /// Per tick, like vanilla's `deltaMovement`.
    velocity: DVec3,
    /// Feet position as drawn this frame, between `prev_feet` and `feet`
    /// (`Mth.lerp` in doubles, as `Camera.alignWithEntity`).
    pos: DVec3,
    /// Melon's yaw: 0 looks north, unlike vanilla's south.
    yaw_degrees: f32,
    pitch_degrees: f32,
    on_ground: bool,
    horizontal_collision: bool,
    /// `minorHorizontalCollision`.
    minor_horizontal_collision: bool,
    /// This tick's input as a horizontal world direction.
    intent: DVec3,
    no_jump_delay: u32,
    /// `onClimbable()` as of this tick's start (ladder, vine, scaffolding).
    on_climbable: bool,
    /// What the server set (`Attributes`); defaults are vanilla's.
    attributes: PlayerAttributes,
    /// Effects that change the physics (`SLOW_FALLING`, `JUMP_BOOST` amplifier + 1).
    slow_falling: bool,
    /// `DOLPHINS_GRACE`: water barely slows the player.
    dolphins_grace: bool,
    /// `Entity.stuckSpeedMultiplier`: set by cobwebs and the like, spent by the next move.
    stuck: DVec3,
    jump_boost: u32,
    /// `Entity.fallDistance`, reset while climbing.
    fall_distance: f64,
    /// The fall distance of the last move that ended on the ground
    /// (`Entity.checkFallDamage` hands it to `Block.fallOn`), for the sounds.
    landed_fall: Option<f64>,
    /// `Camera.eyeHeight` and a tick ago.
    camera_eye: f32,
    camera_eye_old: f32,
    /// `Entity.mainSupportingBlockPos`: the block the player stands on.
    support: crate::world::physics::Support,
    /// `LivingEntity.walkAnimation`: the limbs' swing, advanced each tick.
    walk: crate::world::entity::walk::WalkAnimation,
    /// `LocalPlayer.yBob`/`xBob`: a lagging copy of the look angles.
    yaw_bob: f32,
    pitch_bob: f32,
    /// `ageInTicks`.
    age_ticks: f32,
    /// `LocalPlayer.crouching`.
    crouching: bool,
    /// `Entity.isSwimming()`: sprinting in water with the head under it.
    swimming: bool,
    /// `Pose.SWIMMING`.
    swim_pose: bool,
    /// `Entity.isInWater` and `wasEyeInWater`, as of the tick.
    in_water: bool,
    eye_in_water: bool,
    /// `LivingEntity.swimAmount` now and a tick ago.
    swim_amount: f32,
    swim_amount_old: f32,
    /// `Entity.isSprinting()`, which the double-tap and the sprint key both set.
    sprinting: bool,
    /// `LocalPlayer.sprintTriggerTime`, in ticks.
    sprint_trigger_ticks: u32,
    /// `LocalPlayer.aiStep`'s `hasForwardImpulse`, sampled *before* the movement input updates.
    had_forward_impulse: bool,
    /// What the server allows: flying, and how fast.
    abilities: Abilities,
    /// `Entity.noPhysics`, for spectators: no collisions, never on ground.
    spectator: bool,
    /// `Player.jumpTriggerTime`, in ticks.
    jump_trigger_ticks: u32,
    /// Last tick's jump key, for the double-tap's rising edge.
    had_jump: bool,
    /// `Entity.isFallFlying`: gliding on an elytra.
    gliding: bool,
    /// A glider is worn in its slot (`canGlideUsing`).
    glider_worn: bool,
    /// The glide began and the server doesn't know yet.
    glide_start_pending: bool,
    /// `autoSpinAttackTicks`: the riptide spin left.
    spin_ticks: u32,
    /// The box the last tick swept while spinning.
    spin_sweep: Option<Aabb>,
    /// Set when the player started or stopped flying, for the server.
    abilities_changed: bool,
    /// `ClientAvatarState.walkDist` now and a tick ago.
    walk_dist: f32,
    walk_dist_old: f32,
    /// `ClientAvatarState.bob` now and a tick ago.
    bob: f32,
    bob_old: f32,
    /// The tick fraction of the frame being drawn.
    partial: f32,
    /// The item being used, if any.
    item_use: Option<hands::ItemUse>,
    /// What Speed and Slowness make of `MOVEMENT_SPEED`
    speed_effects: f32,
    /// Seated on a vehicle (`isPassenger`), and the turn of its body, last tick's and this
    /// tick's.
    riding: bool,
    ride_body: Option<(f32, f32)>,
    /// The ways of moving, by priority, and the one the last tick used.
    move_states: movement::MoveStates,
    move_mode: MoveMode,
    /// What the server allows (`melon:movement`), the mod state the last
    /// tick ran, and the starts not yet reported.
    allowed_moves: Vec<String>,
    server_move: Option<&'static str>,
    started_moves: Vec<&'static str>,
}

impl PlayerState {
    /// The boxes of the entities near the player that stop it, for this tick.
    pub fn set_colliders(&mut self, colliders: Vec<crate::world::physics::Aabb>) {
        self.colliders = colliders;
    }

    /// The collider list, to refill each tick without a new allocation.
    pub fn colliders_buffer(&mut self) -> &mut Vec<crate::world::physics::Aabb> {
        &mut self.colliders
    }

    /// The player's box, for finding what is near it.
    pub fn collision_box(&self) -> crate::world::physics::Aabb {
        self.bounding_box()
    }

    pub fn new(feet: DVec3) -> Self {
        Self {
            feet,
            prev_feet: feet,
            colliders: Vec::new(),
            velocity: DVec3::ZERO,
            pos: feet,
            yaw_degrees: 0.0,
            pitch_degrees: 0.0,
            on_ground: false,
            horizontal_collision: false,
            minor_horizontal_collision: false,
            intent: DVec3::ZERO,
            no_jump_delay: 0,
            on_climbable: false,
            attributes: PlayerAttributes::default(),
            slow_falling: false,
            dolphins_grace: false,
            stuck: DVec3::ZERO,
            jump_boost: 0,
            fall_distance: 0.0,
            landed_fall: None,
            camera_eye: EYE_HEIGHT,
            camera_eye_old: EYE_HEIGHT,
            support: Default::default(),
            walk: Default::default(),
            yaw_bob: 0.0,
            pitch_bob: 0.0,
            age_ticks: 0.0,
            crouching: false,
            swimming: false,
            swim_pose: false,
            in_water: false,
            eye_in_water: false,
            swim_amount: 0.0,
            swim_amount_old: 0.0,
            sprinting: false,
            sprint_trigger_ticks: 0,
            had_forward_impulse: false,
            abilities: Abilities::default(),
            spectator: false,
            jump_trigger_ticks: 0,
            had_jump: false,
            gliding: false,
            glider_worn: false,
            glide_start_pending: false,
            spin_ticks: 0,
            spin_sweep: None,
            abilities_changed: false,
            walk_dist: 0.0,
            walk_dist_old: 0.0,
            bob: 0.0,
            bob_old: 0.0,
            partial: 1.0,
            item_use: None,
            speed_effects: 1.0,
            riding: false,
            ride_body: None,
            move_states: movement::MoveStates::vanilla(),
            move_mode: MoveMode::Walk,
            allowed_moves: Vec::new(),
            server_move: None,
            started_moves: Vec::new(),
        }
    }

    /// `ClientPacketListener.handleRespawn`: a new `LocalPlayer` takes the
    /// old one's place. It starts with default abilities (the game mode is
    /// set on it next), no item in use, not riding, and the attributes'
    /// base values: their modifiers too under `KEEP_ATTRIBUTE_MODIFIERS`.
    /// Under `KEEP_ENTITY_DATA` it keeps the motion, the look and the
    /// sprinting flag; otherwise it faces north (`setYRot(-180)`).
    ///
    /// The position stays where it was until the server's teleport, which
    /// the loading screen waits for; vanilla's new player is at the origin
    /// until then, behind the same screen. Melon's movement channel (the
    /// server's allowed moves) belongs to the connection and stays.
    pub fn respawned(&mut self, keep_entity_data: bool, keep_attributes: bool) {
        let mut new = Self::new(self.feet);
        new.attributes = if keep_attributes {
            self.attributes.clone()
        } else {
            self.attributes.base_only()
        };
        if keep_entity_data {
            new.velocity = self.velocity;
            new.yaw_degrees = self.yaw_degrees;
            new.pitch_degrees = self.pitch_degrees;
            new.sprinting = self.sprinting;
        }
        new.move_states = std::mem::replace(&mut self.move_states, movement::MoveStates::vanilla());
        new.allowed_moves = std::mem::take(&mut self.allowed_moves);
        *self = new;
    }

    pub fn flying(&self) -> bool {
        self.abilities.flying
    }

    /// Feet position after the last tick.
    pub fn feet(&self) -> DVec3 {
        self.feet
    }

    /// Eye position after the last tick (`Entity.getEyePosition`).
    pub fn tick_eye(&self) -> DVec3 {
        self.feet + DVec3::new(0.0, f64::from(self.eye_height()), 0.0)
    }

    /// The pose's eye height (`EntityDimensions.eyeHeight`).
    fn eye_height(&self) -> f32 {
        self.base_eye_height() * self.scale() as f32
    }

    fn base_eye_height(&self) -> f32 {
        if self.swim_pose || self.gliding || self.spin_ticks > 0 {
            SWIM_EYE_HEIGHT
        } else if self.crouching {
            CROUCH_EYE_HEIGHT
        } else {
            EYE_HEIGHT
        }
    }

    /// The pose's box height.
    pub fn box_height(&self) -> f64 {
        self.base_box_height() * self.scale()
    }

    /// The box width (`0.6` scaled).
    pub fn box_width(&self) -> f64 {
        WIDTH * self.scale()
    }

    fn base_box_height(&self) -> f64 {
        if self.swim_pose || self.gliding || self.spin_ticks > 0 {
            SWIM_HEIGHT
        } else if self.crouching {
            CROUCH_HEIGHT
        } else {
            HEIGHT
        }
    }

    /// `Entity.isSwimming`: the swim stroke, sprinting through water.
    pub fn swimming(&self) -> bool {
        self.swimming
    }

    /// `Entity.isVisuallySwimming`: lying flat, swimming or crawling.
    pub fn visually_swimming(&self) -> bool {
        self.swim_pose
    }

    /// Whether the eyes are in water (`isEyeInFluid(WATER)`).
    pub fn eye_in_water(&self) -> bool {
        self.eye_in_water
    }

    /// `Entity.isInWater`.
    pub fn in_water(&self) -> bool {
        self.in_water
    }

    /// Feet position as drawn this frame, between the last two ticks.
    pub fn pos(&self) -> DVec3 {
        self.pos
    }

    /// Where it looks, in vanilla's convention.
    pub fn look_angles(&self) -> crate::math::euler::Look {
        crate::math::euler::Look::from_melon(self.yaw_degrees, self.pitch_degrees)
    }

    /// Melon's yaw in degrees: 0 looks north (vanilla's 0 looks south).
    pub fn yaw(&self) -> f32 {
        self.yaw_degrees
    }

    pub fn pitch(&self) -> f32 {
        self.pitch_degrees
    }

    /// Turns the player to `yaw` (Melon's convention, wrapped to `0..360`)
    pub fn set_rotation(&mut self, yaw: f32, pitch: f32) {
        self.yaw_degrees = yaw.rem_euclid(360.0);
        self.pitch_degrees = pitch.clamp(-90.0, 90.0);
    }

    pub fn abilities(&self) -> Abilities {
        self.abilities
    }

    /// Takes off, if the player may fly (the benchmark's double tap).
    pub fn start_flying(&mut self) {
        if self.abilities.may_fly && !self.abilities.flying {
            self.abilities.flying = true;
            self.abilities_changed = true;
        }
    }

    /// `GameType.updatePlayerAbilities` and `LocalPlayer.onGameModeChanged`.
    pub fn set_game_mode(&mut self, mode: crate::world::game_mode::GameMode) {
        self.abilities = mode.abilities(self.abilities);
        self.set_spectator(mode.is_spectator());
    }

    /// The effects' share of the movement speed, as of this tick.
    pub fn set_speed_effects(&mut self, factor: f32) {
        self.speed_effects = factor;
    }

    /// An attribute update for the local player (`handleUpdateAttributes`).
    pub fn set_attribute(&mut self, attribute: u8, base: f64, modifiers: Vec<(String, f64, i8)>) {
        self.attributes.set(attribute, base, modifiers);
    }

    /// The effects the physics reads besides speed.
    pub fn set_physics_effects(
        &mut self,
        slow_falling: bool,
        jump_boost: u32,
        dolphins_grace: bool,
    ) {
        self.slow_falling = slow_falling;
        self.jump_boost = jump_boost;
        self.dolphins_grace = dolphins_grace;
    }

    /// `LivingEntity.getEffectiveGravity`.
    pub(super) fn effective_gravity(&self) -> f64 {
        let gravity = self.attributes.gravity();
        if self.slow_falling && self.velocity.y <= 0.0 {
            gravity.min(0.01)
        } else {
            gravity
        }
    }

    /// `Entity.getScale` from the `scale` attribute.
    pub fn scale(&self) -> f64 {
        self.attributes.scale()
    }

    /// `Attributes.MOVEMENT_SPEED` as the player's own movement reads it.
    pub fn movement_speed(&self) -> f32 {
        if let Some(speed) = self.attributes.movement_speed(self.sprinting) {
            return speed;
        }
        let base = MOVEMENT_SPEED * self.speed_effects;
        if self.sprinting {
            base * (1.0 + SPRINT_SPEED_MODIFIER)
        } else {
            base
        }
    }

    /// `AbstractClientPlayer.getFieldOfViewModifier` without the item in use.
    pub fn fov_modifier(&self) -> f32 {
        let mut modifier = 1.0;
        if self.abilities.flying {
            modifier *= 1.1;
        }
        let walking = self.abilities.walking_speed;
        if walking != 0.0 {
            modifier *= (self.movement_speed() / walking + 1.0) / 2.0;
        }
        modifier
    }

    /// `ClientboundPlayerAbilitiesPacket`.
    pub fn set_abilities(&mut self, flags: u8, flying_speed: f32, walking_speed: f32) {
        self.abilities = self
            .abilities
            .from_packet(flags, flying_speed, walking_speed);
    }

    /// `Minecraft.handleScroll` for a spectator.
    pub fn scroll_flying_speed(&mut self, lines: f32) {
        let speed = &mut self.abilities.flying_speed;
        *speed = (*speed + lines.signum() * 0.005).clamp(0.0, 0.2);
    }

    /// Whether flight was toggled since the last call.
    pub fn take_abilities_changed(&mut self) -> bool {
        std::mem::take(&mut self.abilities_changed)
    }

    /// `LocalPlayer.onGameModeChanged`: becoming a spectator stops any fall.
    fn set_spectator(&mut self, spectator: bool) {
        if spectator && !self.spectator {
            self.velocity.y = 0.0;
        }
        self.spectator = spectator;
        if spectator {
            self.on_ground = false;
            self.crouching = false;
        }
    }

    /// The server moved the player.
    pub fn teleport(&mut self, feet: DVec3, velocity: DVec3, prev_feet: DVec3) {
        self.feet = feet;
        self.prev_feet = prev_feet;
        self.velocity = velocity;
        self.pos = feet;
    }

    /// After a warp or setback: if a block is under the feet, stand on it.
    pub fn settle_after_teleport(&mut self, level: &crate::world::level::Level) {
        let aabb = self.bounding_box();
        let under = crate::world::physics::Aabb {
            min: DVec3::new(aabb.min.x, aabb.min.y - 1.0e-6, aabb.min.z),
            max: DVec3::new(aabb.max.x, aabb.min.y, aabb.max.z),
        };
        if crate::world::physics::find_supporting_block(level, &under, self.feet).is_none() {
            return;
        }
        self.on_ground = true;
        if self.velocity.y > 0.0 {
            self.velocity.y = 0.0;
        }
        self.update_supporting_block(level, DVec3::ZERO);
    }

    /// `Player.isColliding(pos, state)`.
    pub fn is_colliding_with_block(
        &self,
        level: &crate::world::level::Level,
        pos: [i32; 3],
    ) -> bool {
        let b = self.bounding_box();
        crate::world::physics::block_shapes(level, (pos[0], pos[1], pos[2]))
            .any(|shape| shape.intersects(&b))
    }

    /// `Entity.absSnapTo`: put back at a place, keeping the motion.
    pub fn snap_to(&mut self, feet: DVec3) {
        self.teleport(feet, self.velocity, feet);
    }

    /// `LivingEntity.isPushable`: not a spectator and not climbing.
    pub fn is_pushable(&self) -> bool {
        !self.spectator && !self.on_climbable
    }

    /// Where it stood the tick before (`xo`, `yo`, `zo`).
    pub fn prev_feet(&self) -> DVec3 {
        self.prev_feet
    }

    /// `Entity.isPassenger`.
    pub fn riding(&self) -> bool {
        self.riding
    }

    /// `LocalPlayer.rideTick`.
    pub fn ride_tick(&mut self, seat_feet: DVec3, body: Option<(f32, f32)>) {
        self.riding = true;
        self.ride_body = body;
        self.prev_feet = self.feet;
        self.walk_dist_old = self.walk_dist;
        self.feet = seat_feet;
        self.velocity = DVec3::ZERO;
        self.on_ground = false;
        self.horizontal_collision = false;
        self.minor_horizontal_collision = false;
        self.sprinting = false;
        self.crouching = false;
        self.swimming = false;
        self.swim_pose = false;
        self.gliding = false;
        self.spin_ticks = 0;
        self.bob_old = self.bob;
        self.bob = 0.0;
        self.walk.stop();
    }

    /// Off the vehicle: walking again from where the server puts the player.
    pub fn dismount(&mut self) {
        self.riding = false;
        self.ride_body = None;
    }

    /// Per tick, like vanilla's `getDeltaMovement`.
    pub fn velocity(&self) -> DVec3 {
        self.velocity
    }

    /// `ClientboundSetEntityMotionPacket` for this player.
    pub fn set_velocity(&mut self, velocity: DVec3) {
        self.velocity = velocity;
    }

    /// `ClientboundExplodePacket`'s player knockback, added on top.
    pub fn push(&mut self, impulse: DVec3) {
        self.velocity += impulse;
    }

    pub fn on_ground(&self) -> bool {
        self.on_ground
    }

    /// `Entity.mainSupportingBlockPos`.
    pub fn supporting_block(&self) -> Option<(i32, i32, i32)> {
        self.support.block
    }

    /// `Entity.isCrouching`.
    pub fn crouching(&self) -> bool {
        self.crouching
    }

    pub fn attributes(&self) -> &PlayerAttributes {
        &self.attributes
    }

    /// How far the player fell before the last landing, once.
    pub fn take_landing(&mut self) -> Option<f64> {
        self.landed_fall.take()
    }

    pub fn horizontal_collision(&self) -> bool {
        self.horizontal_collision
    }

    pub fn sprinting(&self) -> bool {
        self.sprinting
    }

    /// `LocalPlayer.aiStep`'s sprint block, transcribed.
    fn update_sprint(&mut self, input: MoveInput, can_sprint: bool, sprint_window: u32) {
        self.sprint_trigger_ticks = self.sprint_trigger_ticks.saturating_sub(1);

        let forward_impulse = input.forward > 1.0e-5;
        if input.sneak || input.forward < 0.0 {
            self.sprint_trigger_ticks = 0;
        }

        let slowed_by_item = self.item_use.is_some_and(|item_use| !item_use.can_sprint);
        if slowed_by_item {
            self.sprint_trigger_ticks = 0;
        }

        let under_water = self.eye_in_water && self.in_water;
        let moving_slowly = self.crouching || (self.swim_pose && !self.in_water);
        if !self.sprinting
            && forward_impulse
            && (!moving_slowly || under_water)
            && can_sprint
            && !slowed_by_item
        {
            if !self.had_forward_impulse {
                if self.sprint_trigger_ticks > 0 {
                    self.sprinting = true;
                } else {
                    self.sprint_trigger_ticks = sprint_window;
                }
            }
            if input.sprint {
                self.sprinting = true;
            }
        }

        let bumped = self.horizontal_collision && !self.minor_horizontal_collision;
        let stop = if self.swimming {
            !self.in_water || (!forward_impulse && !self.on_ground && !input.sneak)
        } else {
            !forward_impulse || bumped || !can_sprint
        };
        if self.sprinting && stop {
            self.sprinting = false;
        }

        self.had_forward_impulse = forward_impulse;
    }

    /// `dx`/`dy` are already scaled look deltas (`MouseHandler` × `turn`'s 0.15).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw_degrees = (self.yaw_degrees + dx).rem_euclid(360.0);
        self.pitch_degrees = (self.pitch_degrees + dy).clamp(-90.0, 90.0);
    }

    /// The bounding box where the player is drawn this frame, for culling.
    pub fn render_bounds(&self) -> (DVec3, DVec3) {
        let height = self.box_height();
        let half = self.box_width() / 2.0;
        (
            self.pos - DVec3::new(half, 0.0, half),
            self.pos + DVec3::new(half, height, half),
        )
    }

    fn bounding_box(&self) -> Aabb {
        Aabb::at_feet(self.feet, self.box_width(), self.box_height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{Chunk, Section};
    use crate::world::level::Level;

    #[test]
    fn the_field_of_view_widens_with_speed_and_flight() {
        let mut player = PlayerState::new(DVec3::ZERO);
        assert_eq!(player.fov_modifier(), 1.0);
        player.set_speed_effects(1.4);
        assert!((player.fov_modifier() - 1.2).abs() < 1e-6);
        player.set_speed_effects(1.0);
        player.sprinting = true;
        assert!((player.fov_modifier() - 1.15).abs() < 1e-6);
        player.sprinting = false;
        player.abilities.flying = true;
        assert!((player.fov_modifier() - 1.1).abs() < 1e-6);
        player.abilities.flying = false;
        player.set_speed_effects(0.7);
        assert!(player.fov_modifier() < 1.0);
    }

    #[test]
    fn speed_effects_scale_the_movement_speed() {
        let mut player = PlayerState::new(DVec3::ZERO);
        assert!((player.movement_speed() - 0.1).abs() < 1e-7);
        player.set_speed_effects(1.2);
        assert!((player.movement_speed() - 0.12).abs() < 1e-7);
    }

    /// Stone (state 1) from the bottom up to y = 63 around the origin.
    fn flat_world(wall: bool) -> Level {
        let mut level = Level::default();
        for cx in -2..=2 {
            for cz in -2..=2 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                for section in &mut sections[..8] {
                    *section = Section::filled(1);
                }
                if wall && cx == 1 {
                    for y in 0..3 {
                        for z in 0..16 {
                            sections[8].set(y * 256 + z * 16 + 4, 1);
                        }
                    }
                }
                level.insert(Chunk::new(cx, cz, sections));
            }
        }
        level
    }

    #[test]
    fn standing_on_an_edge_takes_friction_from_the_supporting_block() {
        let mut level = flat_world(false);
        let ice = pumpkin_data::Block::ICE.default_state.id.as_u16();
        level.set_block([0, 63, 0], ice, false);
        level.set_block([1, 63, 0], 0, false);
        let mut player = PlayerState::new(DVec3::new(1.02, 64.0, 0.5));
        player.on_ground = true;
        player.update_supporting_block(&level, DVec3::ZERO);
        assert_eq!(player.support.block, Some((0, 63, 0)));
        assert_eq!(player.block_below(&level), &pumpkin_data::Block::ICE);
        player.on_ground = false;
        player.update_supporting_block(&level, DVec3::ZERO);
        assert_eq!(player.support.block, None);
    }

    #[test]
    fn a_teleport_onto_the_floor_settles_and_kills_leftover_jump() {
        let level = flat_world(false);
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        player.velocity = DVec3::new(0.1, 0.42, 0.0);
        player.on_ground = false;
        player.settle_after_teleport(&level);
        assert!(player.on_ground);
        assert_eq!(player.velocity.y, 0.0);
        assert!((player.velocity.x - 0.1).abs() < 1e-9);

        let mut air = PlayerState::new(DVec3::new(0.5, 80.0, 0.5));
        air.velocity = DVec3::new(0.0, 0.42, 0.0);
        air.settle_after_teleport(&level);
        assert!(!air.on_ground);
        assert!((air.velocity.y - 0.42).abs() < 1e-9);
    }

    #[test]
    fn a_player_inside_a_wall_is_pushed_out_sideways() {
        let level = flat_world(true);
        let mut player = PlayerState::new(DVec3::new(19.9, 64.0, 5.5));
        player.push_out_of_blocks(&level);
        assert!(
            (player.velocity.x + 0.1).abs() < 1e-9,
            "{:?}",
            player.velocity
        );
        let mut clear = PlayerState::new(DVec3::new(10.5, 64.0, 5.5));
        clear.push_out_of_blocks(&level);
        assert_eq!(clear.velocity, DVec3::ZERO);
    }

    #[test]
    fn a_sneaking_player_stays_on_a_ledge_and_a_blocked_nudge_is_not_made() {
        let mut level = flat_world(false);
        for x in 1..4 {
            for z in -2..3 {
                for y in 56..64 {
                    level.set_block([x, y, z], 0, false);
                }
            }
        }
        let mut player = PlayerState::new(DVec3::new(1.2, 64.0, 0.5));
        player.on_ground = true;
        player.velocity = DVec3::new(0.2, 0.0, 0.0);
        player.move_colliding(&level, true);
        assert!(
            player.feet.x > 1.2 && player.feet.x <= 1.3 + 1.0e-6,
            "{}",
            player.feet.x
        );
    }

    #[test]
    fn ducking_glides_the_camera_while_the_true_eye_drops_at_once() {
        let level = flat_world(false);
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        for _ in 0..3 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        let sneak = MoveInput {
            sneak: true,
            ..MoveInput::default()
        };
        player.tick(sneak, &level, 20, SPRINT_WINDOW_DEFAULT);
        player.tick(sneak, &level, 20, SPRINT_WINDOW_DEFAULT);
        player.update_render(1.0, 0.05);
        let half = EYE_HEIGHT + (CROUCH_EYE_HEIGHT - EYE_HEIGHT) * 0.5;
        assert!((player.eye().y - player.feet.y - f64::from(CROUCH_EYE_HEIGHT)).abs() < 1e-5);
        assert!((player.camera_eye().y - player.feet.y - f64::from(half)).abs() < 1e-5);
        for _ in 0..12 {
            player.tick(sneak, &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        player.update_render(1.0, 0.05);
        assert!((player.camera_eye().y - player.eye().y).abs() < 1e-3);
    }

    /// A one block high platform over x <= 0 on the flat world (top y = 65).
    fn platform() -> Level {
        let mut level = flat_world(false);
        for x in -10..=0 {
            for z in -3..=4 {
                level.set_block([x, 64, z], 1, false);
            }
        }
        level
    }

    #[test]
    fn sprinting_off_a_ledge_leaves_the_ground_on_the_tick_the_box_clears_it() {
        let level = platform();
        let mut player = PlayerState::new(DVec3::new(-3.5, 65.0, 0.5));
        player.yaw_degrees = 90.0; // east (Melon's yaw faces north at 0)
        for _ in 0..3 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.on_ground);
        let mut fell_at = None;
        let mut trail = Vec::new();
        for tick in 0..40 {
            let from = player.feet.x;
            player.tick(forward(true), &level, 20, SPRINT_WINDOW_DEFAULT);
            trail.push((player.on_ground, player.feet.y, player.feet.x));
            let began_clear = from - f64::from(0.6_f32) / 2.0 >= 1.0;
            if !player.on_ground && fell_at.is_none() {
                fell_at = Some(tick);
                assert!(began_clear, "left the ground early: {trail:?}");
            }
            if player.on_ground && began_clear && fell_at.is_none() {
                panic!("still on the ground past the edge: {trail:?}");
            }
        }
        let first = fell_at.expect("fell");
        assert_eq!(trail[first - 1].1, 65.0);
        assert!((trail[first].1 - (65.0 - 0.0784)).abs() < 1e-6, "{trail:?}");
        let (mut v, mut y) = (-0.0784_f64, trail[first].1);
        for t in &trail[first + 1..first + 4] {
            v = (v - 0.08) * f64::from(0.98_f32);
            y += v;
            assert!((t.1 - y).abs() < 1e-6, "{trail:?}");
        }
    }

    #[test]
    fn a_fast_fall_onto_slime_bounces_and_sneaking_stops_it() {
        for (sneak, bounces) in [(false, true), (true, false)] {
            let mut level = flat_world(false);
            let slime = pumpkin_data::Block::SLIME_BLOCK.default_state.id.as_u16();
            for x in -2..=3 {
                for z in -2..=3 {
                    level.set_block([x, 63, z], slime, false);
                }
            }
            let mut player = PlayerState::new(DVec3::new(0.5, 80.0, 0.5));
            let input = MoveInput {
                sneak,
                ..MoveInput::default()
            };
            let mut went_up = false;
            for _ in 0..60 {
                player.tick(input, &level, 20, SPRINT_WINDOW_DEFAULT);
                went_up |= player.velocity.y > 0.2;
            }
            assert_eq!(went_up, bounces, "sneak {sneak}");
        }
    }

    #[test]
    fn a_block_that_comes_back_over_the_player_is_found_and_the_snap_keeps_motion() {
        let mut level = flat_world(false);
        let mut player = PlayerState::new(DVec3::new(0.5, 63.5, 0.5));
        level.set_block([0, 63, 0], 0, true);
        assert!(!player.is_colliding_with_block(&level, [0, 63, 0]));
        level.set_block([0, 63, 0], 1, false);
        assert!(player.is_colliding_with_block(&level, [0, 63, 0]));
        player.velocity = DVec3::new(0.1, -0.3, 0.0);
        player.snap_to(DVec3::new(0.5, 65.0, 0.5));
        assert_eq!(player.feet(), DVec3::new(0.5, 65.0, 0.5));
        assert_eq!(player.velocity.y, -0.3);
        assert!(!player.is_colliding_with_block(&level, [0, 63, 0]));
    }

    /// A lake: stone up to y = 51, then water to y = 63.
    fn lake() -> Level {
        let mut level = Level::default();
        let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
        for cx in -2..=2 {
            for cz in -2..=2 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                for section in &mut sections[..7] {
                    *section = Section::filled(1);
                }
                for y in 0..16 {
                    for i in 0..256 {
                        sections[7].set(y * 256 + i, if y < 4 { 1 } else { water });
                    }
                }
                level.insert(Chunk::new(cx, cz, sections));
            }
        }
        level
    }

    #[test]
    fn sprinting_under_water_swims_flat_and_follows_the_look() {
        let level = lake();
        let mut player = PlayerState::new(DVec3::new(0.5, 58.0, 0.5));
        player.pitch_degrees = 45.0;
        for _ in 0..4 {
            player.tick(forward(false), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.in_water() && !player.swimming());
        let start = player.feet.y;
        for _ in 0..6 {
            player.tick(forward(true), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.swimming() && player.visually_swimming());
        assert!(player.swim_amount > 0.0);
        assert_eq!(player.box_height(), SWIM_HEIGHT);
        assert!((player.eye_height() - SWIM_EYE_HEIGHT).abs() < 1e-6);
        assert!(!player.crouching);
        assert!(player.feet.y < start, "looking down swims down");
        for _ in 0..30 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(!player.swimming() && !player.visually_swimming());
        assert_eq!(player.swim_amount, 0.0);
    }

    /// A pool one block deep on the floor: `state` at y = 64 around the origin.
    fn pool(state: u16) -> Level {
        let mut level = flat_world(false);
        for x in -3..=3 {
            for z in -3..=3 {
                level.set_block([x, 64, z], state, false);
            }
        }
        level
    }

    fn jump() -> MoveInput {
        MoveInput {
            jump: true,
            ..MoveInput::default()
        }
    }

    /// Water of `amount` (8 a source, 1 the thinnest flowing layer).
    fn water(amount: u16) -> u16 {
        pumpkin_data::Block::WATER.default_state.id.as_u16() + (8 - amount) % 8
    }

    #[test]
    fn jumping_in_water_a_block_deep_swims_up_instead_of_jumping() {
        let level = pool(water(8));
        let mut player = standing(&level, 0.0);
        assert!(player.in_water());
        player.tick(jump(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(player.velocity.y < 0.05, "jumped at {}", player.velocity.y);
        let start = player.feet.y;
        for _ in 0..20 {
            player.tick(jump(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.feet.y > start + 0.2, "rose to {}", player.feet.y);
    }

    #[test]
    fn shallow_water_and_land_are_jumped_out_of() {
        for level in [pool(water(1)), flat_world(false)] {
            let mut player = standing(&level, 0.0);
            player.tick(jump(), &level, 20, SPRINT_WINDOW_DEFAULT);
            assert!(player.velocity.y > 0.3, "jumped at {}", player.velocity.y);
        }
    }

    #[test]
    fn a_thin_layer_below_the_feet_is_no_water() {
        let level = pool(water(1));
        let mut player = PlayerState::new(DVec3::new(0.5, 64.5, 0.5));
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(!player.in_water());
        assert!(player.velocity.y < -0.07, "fell at {}", player.velocity.y);
    }

    #[test]
    fn lava_a_block_deep_is_swum_up_too() {
        let level = pool(pumpkin_data::Block::LAVA.default_state.id.as_u16());
        let mut player = standing(&level, 0.0);
        player.tick(jump(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(player.velocity.y < 0.05, "jumped at {}", player.velocity.y);
    }

    #[test]
    fn water_touching_lava_still_counts_as_water() {
        let mut level = pool(pumpkin_data::Block::LAVA.default_state.id.as_u16());
        level.set_block([0, 65, 0], water(8), false);
        let mut player = standing(&level, 0.0);
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(player.in_water());
    }

    #[test]
    fn dolphins_grace_swims_faster() {
        let level = lake();
        let distance = |grace: bool| {
            let mut player = PlayerState::new(DVec3::new(0.5, 58.0, 0.5));
            player.set_physics_effects(false, 0, grace);
            for _ in 0..20 {
                player.tick(forward(false), &level, 20, SPRINT_WINDOW_DEFAULT);
            }
            let moved = player.feet - DVec3::new(0.5, 58.0, 0.5);
            (moved.x * moved.x + moved.z * moved.z).sqrt()
        };
        assert!(distance(true) > distance(false) * 2.0);
    }

    /// Standing on the floor, facing `yaw` (Melon degrees).
    fn standing(level: &Level, yaw: f32) -> PlayerState {
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        player.yaw_degrees = yaw;
        for _ in 0..5 {
            player.tick(MoveInput::default(), level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.on_ground());
        player
    }

    fn forward(sprint: bool) -> MoveInput {
        MoveInput {
            forward: 1.0,
            sprint,
            ..MoveInput::default()
        }
    }

    #[test]
    fn jumping_in_the_air_with_a_glider_glides_forward_and_landing_ends_it() {
        let level = flat_world(false);
        let mut player = PlayerState::new(DVec3::new(0.5, 90.0, 0.5));
        player.set_glider_worn(true);
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        let jump = MoveInput {
            jump: true,
            ..MoveInput::default()
        };
        player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(player.gliding() && player.take_glide_start());
        assert_eq!(player.move_mode(), MoveMode::Glide);
        assert_eq!(player.box_height(), SWIM_HEIGHT);
        player.set_rotation(0.0, 20.0);
        let start = player.feet.y;
        for _ in 0..15 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(
            player.feet.z < 0.5 - 0.5 && player.feet.y < start,
            "{:?}",
            player.feet
        );
        player.set_rotation(0.0, 90.0);
        for _ in 0..200 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.on_ground() && !player.gliding());
    }

    #[test]
    fn a_riptide_from_the_ground_hops_and_pushes_along_the_look() {
        let level = flat_world(false);
        let mut player = standing(&level, 0.0);
        let before = player.velocity;
        player.start_riptide(&level, 1);
        assert!(player.spin_attacking() && !player.on_ground());
        assert!(player.feet.y > 64.0 + 1.0);
        let push = player.velocity - before;
        assert!((push.length() - 1.5).abs() < 1e-3, "{push}");
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(player.take_spin_sweep().is_some());
    }

    /// Blocks moved horizontally in the last of `ticks` ticks.
    fn speed_after(player: &mut PlayerState, level: &Level, input: MoveInput, ticks: u32) -> f64 {
        let mut last = 0.0;
        for _ in 0..ticks {
            let before = player.feet;
            player.tick(input, level, 20, SPRINT_WINDOW_DEFAULT);
            let moved = player.feet - before;
            last = (moved.x * moved.x + moved.z * moved.z).sqrt();
        }
        last
    }

    /// Flat ground with a ladder column in the block at x 0, z 0, y 64..72.
    fn ladder_world(state: u16) -> Level {
        let mut level = Level::default();
        for cx in -2..=2 {
            for cz in -2..=2 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                for section in &mut sections[..8] {
                    *section = Section::filled(1);
                }
                if cx == 0 && cz == 0 {
                    for y in 0..8 {
                        sections[8].set(y * 256, state);
                    }
                }
                level.insert(Chunk::new(cx, cz, sections));
            }
        }
        level
    }

    #[test]
    fn ladders_carry_up_hold_on_sneak_and_slow_the_fall() {
        let ladder = pumpkin_data::Block::LADDER.default_state.id.as_u16();
        let level = ladder_world(ladder);
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        for _ in 0..5 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.on_climbable);
        assert!(!player.is_pushable());
        let jump = MoveInput {
            jump: true,
            ..MoveInput::default()
        };
        for _ in 0..10 {
            player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.feet.y > 65.2, "climbed to {}", player.feet.y);
        let sneak = MoveInput {
            sneak: true,
            ..MoveInput::default()
        };
        for _ in 0..4 {
            player.tick(sneak, &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        let held = player.feet.y;
        for _ in 0..5 {
            player.tick(sneak, &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!((player.feet.y - held).abs() < 1e-9);
        let before = player.feet.y;
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(before - player.feet.y <= 0.15 + 1e-6);
    }

    #[test]
    fn leaving_the_ladder_top_gets_no_climb_boost_that_tick() {
        let ladder = pumpkin_data::Block::LADDER.default_state.id.as_u16();
        let level = ladder_world(ladder);
        let mut player = PlayerState::new(DVec3::new(0.5, 70.0, 0.5));
        let jump = MoveInput {
            jump: true,
            ..MoveInput::default()
        };
        for _ in 0..40 {
            player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
            if player.feet.y >= 72.0 {
                assert!(!player.on_climbable);
                assert!(player.velocity.y < 0.1, "boosted: {}", player.velocity.y);
                return;
            }
        }
        panic!("never left the ladder at {}", player.feet.y);
    }

    #[test]
    fn server_attributes_change_jump_gravity_scale_and_speed() {
        use pumpkin_data::attributes::Attributes as A;
        let level = flat_world(false);
        let peak = |player: &mut PlayerState| {
            let jump = MoveInput {
                jump: true,
                ..MoveInput::default()
            };
            let mut top = player.feet.y;
            for _ in 0..30 {
                player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
                top = top.max(player.feet.y);
            }
            top - 64.0
        };
        let mut normal = standing(&level, 0.0);
        let normal_peak = peak(&mut normal);
        let mut strong = standing(&level, 0.0);
        strong.set_attribute(A::JUMP_STRENGTH.id, 0.84, vec![]);
        assert!(peak(&mut strong) > normal_peak * 1.5);
        let mut light = standing(&level, 0.0);
        light.set_attribute(A::GRAVITY.id, 0.04, vec![]);
        assert!(peak(&mut light) > normal_peak * 1.5);

        let mut small = standing(&level, 0.0);
        small.set_attribute(A::SCALE.id, 0.5, vec![]);
        assert!((small.box_height() - 0.9).abs() < 1e-6);
        assert!((small.eye_height() - 0.81).abs() < 1e-6);

        let mut fast = standing(&level, 0.0);
        fast.set_attribute(A::MOVEMENT_SPEED.id, 0.2, vec![]);
        let speed = speed_after(&mut fast, &level, forward(false), 40);
        let mut base = standing(&level, 0.0);
        let base_speed = speed_after(&mut base, &level, forward(false), 40);
        assert!((speed / base_speed - 2.0).abs() < 0.01);
    }

    #[test]
    fn slow_falling_caps_gravity_while_falling() {
        let mut player = PlayerState::new(DVec3::ZERO);
        player.velocity.y = -0.5;
        assert_eq!(player.effective_gravity(), 0.08);
        player.set_physics_effects(true, 0, false);
        assert_eq!(player.effective_gravity(), 0.01);
        player.velocity.y = 0.3;
        assert_eq!(player.effective_gravity(), 0.08);
    }

    #[test]
    fn a_cobweb_nearly_stops_the_player() {
        let web = pumpkin_data::Block::COBWEB.default_state.id.as_u16();
        let level = ladder_world(web);
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        for _ in 0..5 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        let speed = speed_after(&mut player, &level, forward(false), 10);
        assert!(speed < 0.06, "speed {speed}");
        let mut faller = PlayerState::new(DVec3::new(0.5, 70.0, 0.5));
        for _ in 0..10 {
            faller.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(70.0 - faller.feet.y < 1.0, "fell {}", 70.0 - faller.feet.y);
    }

    #[test]
    fn flowing_water_carries_the_player_and_a_flier_is_not_carried() {
        let source = pumpkin_data::Block::WATER.default_state.id.as_u16();
        let mut level = Level::default();
        for cx in -1..=1 {
            for cz in -1..=1 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                for section in &mut sections[..8] {
                    *section = Section::filled(1);
                }
                if cx == 0 && cz == 0 {
                    for x in 0..8usize {
                        for y in 0..2usize {
                            sections[8].set(y * 256 + 4 * 16 + x, source + x as u16);
                        }
                    }
                }
                level.insert(Chunk::new(cx, cz, sections));
            }
        }
        let start = DVec3::new(3.5, 64.0, 4.5);
        let mut player = PlayerState::new(start);
        for _ in 0..20 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.feet.x > start.x + 0.05, "moved to {}", player.feet.x);
    }

    #[test]
    fn walking_and_sprinting_reach_vanilla_speeds() {
        let level = flat_world(false);
        let mut player = standing(&level, 0.0);
        let walk = speed_after(&mut player, &level, forward(false), 40);
        assert!((walk - 0.21585).abs() < 1.0e-3, "walking at {walk}/tick");
        let sprint = speed_after(&mut player, &level, forward(true), 40);
        assert!(player.sprinting());
        assert!(
            (sprint - 0.2806).abs() < 1.0e-3,
            "sprinting at {sprint}/tick"
        );
        assert!(
            (player.feet.y - 64.0).abs() < 1.0e-9,
            "sank to {}",
            player.feet.y
        );
    }

    #[test]
    fn letting_go_stops_within_a_second() {
        let level = flat_world(false);
        let mut player = standing(&level, 0.0);
        speed_after(&mut player, &level, forward(false), 40);
        let coasting = speed_after(&mut player, &level, MoveInput::default(), 1);
        assert!(coasting < 0.2 && coasting > 0.05, "{coasting}");
        let stopped = speed_after(&mut player, &level, MoveInput::default(), 20);
        assert_eq!(stopped, 0.0);
        assert!(!player.sprinting());
    }

    #[test]
    fn creative_players_double_tap_jump_to_fly_and_land_to_stop() {
        let level = flat_world(false);
        let mut player = standing(&level, 0.0);
        player.abilities = crate::world::game_mode::GameMode::Creative.abilities(player.abilities);
        let jump = MoveInput {
            jump: true,
            ..MoveInput::default()
        };
        player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(!player.flying());
        player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(player.flying());
        assert!(player.take_abilities_changed());
        for _ in 0..20 {
            player.tick(jump, &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        let height = player.feet.y;
        assert!(height > 66.0, "only reached {height}");
        for _ in 0..40 {
            player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!((player.feet.y - height).abs() < 1.0, "fell while flying");
        let sneak = MoveInput {
            sneak: true,
            ..MoveInput::default()
        };
        for _ in 0..200 {
            player.tick(sneak, &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(!player.flying(), "still flying on the ground");
        assert!(player.on_ground());
    }

    #[test]
    fn spectators_fly_through_walls() {
        let level = flat_world(true);
        let mut player = standing(&level, 90.0);
        player.feet.x = 19.0;
        player.abilities = crate::world::game_mode::GameMode::Spectator.abilities(player.abilities);
        player.set_spectator(true);
        for _ in 0..40 {
            player.tick(forward(false), &level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(
            player.feet.x > 21.0,
            "stopped at the wall: {}",
            player.feet.x
        );
        assert!(player.flying());
        assert!(!player.horizontal_collision());
    }

    #[test]
    fn brushing_a_wall_keeps_the_sprint_but_running_into_it_ends_it() {
        let level = flat_world(true);
        let mut player = standing(&level, 175.0);
        player.feet.x = 19.6;
        speed_after(&mut player, &level, forward(true), 1);
        assert!(player.sprinting());
        speed_after(&mut player, &level, forward(true), 20);
        assert!(player.horizontal_collision(), "never reached the wall");
        assert!(player.sprinting(), "a glancing wall stopped the sprint");
        assert!(player.feet.x <= 20.0 - 0.3 + 1.0e-7);

        player.yaw_degrees = 90.0;
        speed_after(&mut player, &level, forward(true), 5);
        assert!(!player.sprinting(), "running into the wall kept sprinting");
    }

    #[test]
    fn knockback_is_applied_and_fades() {
        let level = flat_world(false);
        let mut player = standing(&level, 0.0);
        player.set_velocity(DVec3::new(0.4, 0.36, 0.0));
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(
            player.feet.x > 0.8 && player.feet.y > 64.0,
            "{:?}",
            player.feet
        );
        speed_after(&mut player, &level, MoveInput::default(), 60);
        assert!(player.on_ground());
        assert_eq!(player.velocity().x, 0.0);
    }
}
