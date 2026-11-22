//! A tracked entity's client tick: the timers, turns and animation state

use super::*;

impl ClientEntity {
    /// The client-side tick: interpolation, turning, the walk cycle and
    pub(super) fn tick(&mut self) {
        self.prev_pos = self.pos;
        self.prev_yaw = self.yaw;
        self.prev_pitch = self.pitch;
        self.prev_head = self.head;
        self.prev_body = self.body;
        self.age = self.age.wrapping_add(1);
        self.arrow_shake = (self.arrow_shake - 1).max(0);
        if let Some(life) = &mut self.fangs_life {
            *life -= 1;
        }
        if matches!(self.kind, EntityKind::SulfurCube | EntityKind::Minecart) && self.fuse_ticks > 0
        {
            self.fuse_ticks -= 1;
        }
        if let Some(display) = &mut self.display {
            display.tick(self.age as i32);
        }
        match self.kind {
            EntityKind::Cat => {
                self.feline.0 = self.feline.1;
                self.feline
                    .1
                    .step(self.appearance.lying, self.appearance.relaxed);
            }
            EntityKind::Wolf | EntityKind::Fox => {
                self.interest.0 = self.interest.1;
                let target = if self.appearance.interested { 1.0 } else { 0.0 };
                self.interest.1 += (target - self.interest.1) * 0.4;
                if self.kind == EntityKind::Fox {
                    self.fox_crouch.0 = self.fox_crouch.1;
                    if self.appearance.fox_flags & fox::FLAG_CROUCHING != 0 {
                        self.fox_crouch.1 = (self.fox_crouch.1 + 0.2).min(5.0);
                    } else {
                        self.fox_crouch.1 = 0.0;
                    }
                }
            }
            EntityKind::PolarBear => {
                self.polar_stand.0 = self.polar_stand.1;
                let step = if self.appearance.polar_standing {
                    1.0
                } else {
                    -1.0
                };
                self.polar_stand.1 = (self.polar_stand.1 + step).clamp(0.0, 6.0);
            }
            EntityKind::Armadillo => {
                let state = self.appearance.armadillo_state;
                if state != self.armadillo_seen {
                    self.armadillo_seen = state;
                    self.armadillo_ticks = 0;
                } else {
                    self.armadillo_ticks = self.armadillo_ticks.saturating_add(1);
                }
                armadillo::tick_anim(&mut self.anim, self.age as i32, state, self.armadillo_ticks);
            }
            EntityKind::Camel | EntityKind::CamelHusk => {
                let sitting = self.appearance.camel_pose_tick < 0;
                if self.appearance.camel_pose_tick != self.camel_seen_tick {
                    self.camel_seen_tick = self.appearance.camel_pose_tick;
                    self.camel_pose_age = 0;
                } else {
                    self.camel_pose_age = self.camel_pose_age.saturating_add(1);
                }
                let age = self.camel_pose_age;
                let sitting_down = sitting && age < 40;
                let standing_up = !sitting && self.appearance.camel_pose_tick != 0 && age < 52;
                self.camel.tick(
                    &mut self.anim,
                    self.age as i32,
                    sitting,
                    sitting_down,
                    self.appearance.camel_dashing,
                    standing_up,
                );
            }
            EntityKind::Sniffer => {
                sniffer::tick_anim(
                    &mut self.anim,
                    self.age as i32,
                    self.appearance.sniffer_state,
                );
            }
            EntityKind::Warden => {
                warden::tick_anim(&mut self.anim, self.age as i32, self.appearance.pose);
                let beat = self
                    .warden
                    .tick(self.age as i32, self.appearance.warden_anger);
                if beat && !self.appearance.silent {
                    self.sounds.push("entity.warden.heartbeat");
                }
            }
            EntityKind::Breeze => {
                breeze::tick_anim(&mut self.anim, self.age as i32, self.appearance.pose);
            }
            EntityKind::Creaking => {
                creaking::tick_anim(&mut self.anim, self.age as i32, false, false, false);
            }
            EntityKind::CopperGolem => {
                let tick = self.age as i32;
                let state = self.appearance.copper_state;
                use copper_golem::{
                    CHEST_ITEM_DROP, CHEST_ITEM_NODROP, CHEST_NOITEM_GET, CHEST_NOITEM_NOGET, IDLE,
                    STATE_DROPPING_ITEM, STATE_DROPPING_NO_ITEM, STATE_GETTING_ITEM,
                    STATE_GETTING_NO_ITEM,
                };
                match state {
                    STATE_GETTING_ITEM => {
                        self.anim[CHEST_NOITEM_GET].start_if_stopped(tick);
                    }
                    STATE_GETTING_NO_ITEM => {
                        self.anim[CHEST_NOITEM_NOGET].start_if_stopped(tick);
                    }
                    STATE_DROPPING_ITEM => {
                        self.anim[CHEST_ITEM_DROP].start_if_stopped(tick);
                    }
                    STATE_DROPPING_NO_ITEM => {
                        self.anim[CHEST_ITEM_NODROP].start_if_stopped(tick);
                    }
                    _ => {
                        self.anim[IDLE].stop();
                        self.anim[CHEST_NOITEM_GET].stop();
                        self.anim[CHEST_NOITEM_NOGET].stop();
                        self.anim[CHEST_ITEM_DROP].stop();
                        self.anim[CHEST_ITEM_NODROP].stop();
                    }
                }
            }
            EntityKind::Frog => {
                let tick = self.age as i32;
                let pose = self.appearance.pose;
                self.anim[mobs::frog::JUMP]
                    .animate_when(pose == mobs::frog::POSE_LONG_JUMPING, tick);
                self.anim[mobs::frog::CROAK].animate_when(pose == mobs::frog::POSE_CROAKING, tick);
                self.anim[mobs::frog::TONGUE]
                    .animate_when(pose == mobs::frog::POSE_USING_TONGUE, tick);
                let idle_water = self.in_water && !self.moving;
                self.anim[mobs::frog::IDLE_WATER].animate_when(idle_water, tick);
            }
            EntityKind::Rabbit => {
                self.rabbit
                    .tick(&mut self.anim, self.age as i32, self.leash_holder.is_some());
            }
            EntityKind::Bat => {
                let tick = self.age as i32;
                let resting = self.appearance.resting;
                self.anim[mobs::bat::REST].animate_when(resting, tick);
                self.anim[mobs::bat::FLY].animate_when(!resting, tick);
            }
            EntityKind::Boat | EntityKind::Minecart => {
                let a = &mut self.appearance;
                a.vehicle_hurt = (a.vehicle_hurt - 1).max(0);
                a.vehicle_damage = (a.vehicle_damage - 1.0).max(0.0);
                if self.kind == EntityKind::Boat {
                    self.paddles.tick(a.paddles);
                }
            }
            EntityKind::IronGolem => {
                self.golem.0 = (self.golem.0 - 1).max(0);
                self.golem.1 = (self.golem.1 - 1).max(0);
            }
            k if k.is_equine() => {
                self.equine.0 = self.equine.1;
                let a = &self.appearance;
                let (eating, rearing, mouth) =
                    (a.horse_eating(), a.horse_rearing(), a.horse_mouth_open());
                self.equine.1.step(eating, rearing, mouth);
            }
            _ => {}
        }
        self.using_ticks = if self.appearance.using_hand().is_some() {
            self.using_ticks + 1
        } else {
            0
        };

        if let Some(lerp) = &mut self.lerp {
            let t = 1.0 / lerp.steps as f32;
            self.pos += (lerp.pos - self.pos) / f64::from(lerp.steps);
            self.yaw = rot_lerp(t, self.yaw, lerp.yaw);
            self.pitch += (lerp.pitch - self.pitch) * t;
            lerp.steps -= 1;
            if lerp.steps == 0 {
                self.lerp = None;
            }
        }
        if let Some((target, steps)) = &mut self.head_lerp {
            self.head = rot_lerp(1.0 / *steps as f32, self.head, *target);
            *steps -= 1;
            if *steps == 0 {
                self.head_lerp = None;
            }
        }

        let moved = self.pos - self.prev_pos;
        let side = moved.x * moved.x + moved.z * moved.z;
        self.moving = side > 1.0e-7;
        if self.kind.is_mob() {
            self.body_rotation.tick(
                side > 2.500_000_3e-7,
                self.yaw,
                &mut self.body,
                &mut self.head,
            );
        } else if self.kind.is_avatar() {
            self.body = tick_head_turn(self.body, self.walk_direction(moved), self.head, 50.0);
        } else {
            self.body = self.yaw;
        }

        let dying = self.appearance.dying();
        if dying && self.death_time < DEATH_DURATION {
            self.death_time += 1;
        }
        // `calculateEntityAnimation`: no walk while riding or dead.
        if dying || self.vehicle.is_some() {
            self.walk.stop();
        } else {
            let distance = if self.omnidirectional_air_mover() {
                moved.length()
            } else {
                side.sqrt()
            };
            self.update_walk_animation(distance as f32);
        }
        if self.hurt_time > 0 {
            self.hurt_time -= 1;
        }
        if let Some((_, age)) = &mut self.custom_move {
            *age += 1;
            if *age >= super::CUSTOM_MOVE_TICKS {
                self.custom_move = None;
            }
        }
        self.swing.tick();
        if self.kind == EntityKind::Creeper {
            self.tick_swell();
        }
        if self.kind == EntityKind::Chicken {
            self.tick_flap();
        }
        if self.kind.is_cube() {
            self.tick_squish();
        }
        match self.kind {
            EntityKind::Guardian | EntityKind::ElderGuardian => {
                self.guardian
                    .tick(self.in_water, self.appearance.guardian_moving);
            }
            EntityKind::Axolotl => self.tick_axolotl(),
            _ => {}
        }
        if self.kind.is_squid() {
            self.squid.tick(self.in_water, moved);
            self.body = self.squid.y_body;
        }
        self.prev_swim_amount = self.swim_amount;
        self.swim_amount = Swim::step(self.swim_amount, self.appearance.visually_swimming());
    }

    /// The swimming box, which `isInWater` tests: a player in the swimming
    pub(super) fn water_box(&self) -> Aabb {
        let (w, h) = self.size();
        let h = if self.kind.is_avatar() && self.appearance.visually_swimming() {
            h.min(0.6)
        } else {
            h
        };
        Aabb::at_feet(self.pos, f64::from(w), f64::from(h)).deflate(0.001)
    }

    /// How a swimmer's body is laid down, if this kind swims.
    pub(super) fn swim(&self, partial: f32) -> Swim {
        let turn = match self.kind {
            EntityKind::Player | EntityKind::Mannequin => SwimTurn::Avatar {
                in_water: self.in_water,
                visually_swimming: self.appearance.visually_swimming() && self.in_water,
            },
            EntityKind::Drowned => SwimTurn::Drowned {
                height: self.size().1,
            },
            _ => SwimTurn::None,
        };
        Swim {
            amount: self.prev_swim_amount + (self.swim_amount - self.prev_swim_amount) * partial,
            turn,
        }
    }

    /// `LivingEntity.tick`'s `yBodyRotT`: the walking direction, flipped
    pub(super) fn walk_direction(&self, moved: DVec3) -> f32 {
        if self.swing.current().is_some() {
            return self.yaw;
        }
        if moved.x * moved.x + moved.z * moved.z <= 0.002_500_000_2 {
            return self.body;
        }
        let walk = (moved.z.atan2(moved.x) as f32).to_degrees() - 90.0;
        let off = (wrap_degrees(self.yaw) - walk).abs();
        if 95.0 < off && off < 265.0 {
            walk - 180.0
        } else {
            walk
        }
    }

    /// `Creeper.tick`'s fuse.
    pub(super) fn tick_swell(&mut self) {
        self.prev_swell = self.swell;
        let dir = if self.appearance.ignited {
            1
        } else {
            self.appearance.swell_dir
        };
        self.swell = (self.swell + dir).clamp(0, MAX_SWELL);
    }

    /// `Axolotl.tickAdultAnimations` and `tickBabyAnimations`.
    pub(super) fn tick_axolotl(&mut self) {
        use mobs::axolotl as ax;
        let (dead, wet, ground) = (self.appearance.playing_dead, self.in_water, self.on_ground);
        let moving = self.walk.speed(1.0) > 1.0e-5
            || self.pitch != self.prev_pitch
            || self.yaw != self.prev_yaw;
        self.axolotl.moving.tick(moving);
        if self.appearance.baby {
            let tick = self.age as i32;
            let target = if dead {
                ax::PLAY_DEAD
            } else if moving {
                if wet && !ground { ax::SWIM } else { ax::WALK }
            } else if wet && !ground {
                ax::IDLE_UNDERWATER
            } else if wet {
                ax::IDLE_UNDERWATER_FLOOR
            } else {
                ax::IDLE_FLOOR
            };
            for (slot, anim) in self.anim.iter_mut().enumerate().take(ax::BABY_SLOTS) {
                if slot == target {
                    anim.start_if_stopped(tick);
                } else {
                    anim.stop();
                }
            }
        } else {
            self.axolotl.playing_dead.tick(dead);
            self.axolotl.in_water.tick(!dead && wet);
            self.axolotl.on_ground.tick(!dead && !wet && ground);
        }
    }

    /// `GuardianRenderState`'s spikes and tail.
    pub fn guardian_pose(&self, partial: f32) -> mobs::guardian::GuardianPose {
        self.guardian.pose(partial)
    }

    /// The factors an adult axolotl is posed by.
    pub fn axolotl_factors(&self, partial: f32) -> mobs::axolotl::Factors {
        self.axolotl.factors(partial)
    }

    /// `AbstractCubeMob.tick`'s squish: it follows `targetSquish`, which a
    pub(super) fn tick_squish(&mut self) {
        self.prev_squish = self.squish;
        self.squish += (self.target_squish - self.squish) * 0.5;
        if self.on_ground && !self.was_on_ground {
            self.target_squish = -0.5;
        } else if !self.on_ground && self.was_on_ground {
            self.target_squish = 1.0;
        }
        self.was_on_ground = self.on_ground;
        self.target_squish *= 0.6;
    }

    /// What `IronGolemRenderer.extractRenderState` reads.
    pub fn golem_pose(&self, partial: f32) -> golem::GolemPose {
        golem::GolemPose {
            attack: if self.golem.0 > 0 {
                self.golem.0 as f32 - partial
            } else {
                0.0
            },
            offer_flower: self.golem.1,
        }
    }

    /// Works out what the custom name makes renderers do, with the text
    pub fn resolve_name(&mut self, text_of: &impl Fn(&Component) -> String) {
        if !std::mem::take(&mut self.name_pending) {
            return;
        }
        self.appearance.magic = self
            .custom_name
            .as_deref()
            .map_or(Magic::None, |name| Magic::of(&text_of(name)));
    }

    /// `TntRenderState.fuseRemainingInTicks`: `getFuse - partial + 1`.
    pub fn fuse_remaining(&self, partial: f32) -> f32 {
        self.appearance.fuse as f32 - partial + 1.0
    }

    /// `ageInTicks`: the entity's ticks with the partial one.
    pub fn age_in_ticks(&self, partial: f32) -> f32 {
        self.age as f32 + partial
    }

    /// `LivingEntityRenderer.isEntityUpsideDown`: a `Dinnerbone`. A player's
    pub fn upside_down(&self) -> bool {
        !self.kind.is_avatar() && self.appearance.magic == Magic::UpsideDown
    }

    /// `omnidirectionalAirMover`: whose walk counts the climb too.
    fn omnidirectional_air_mover(&self) -> bool {
        match self.kind {
            EntityKind::Bee | EntityKind::Parrot => true,
            EntityKind::SulfurCube => !self
                .equipment
                .get(crate::network::play::game::EquipmentSlot::Body)
                .is_empty(),
            _ => false,
        }
    }

    /// `updateWalkAnimation`, with the overrides of the mobs that walk
    /// their own way.
    fn update_walk_animation(&mut self, distance: f32) {
        let scale = if self.appearance.baby { 3.0 } else { 1.0 };
        match self.kind {
            // `Creaking`: quick to swing, no baby.
            EntityKind::Creaking => self.walk.update((distance * 25.0).min(3.0), 0.4, 1.0),
            // `Camel`: only standing and not dashing, and slow to ease.
            EntityKind::Camel | EntityKind::CamelHusk => {
                let standing = self.appearance.pose == 0 && !self.anim[camel::DASH].is_started();
                let target = if standing {
                    (distance * 6.0).min(1.0)
                } else {
                    0.0
                };
                self.walk.update(target, 0.2, scale);
            }
            // `Frog`: not while it leaps.
            EntityKind::Frog => {
                let target = if self.anim[mobs::frog::JUMP].is_started() {
                    0.0
                } else {
                    (distance * 25.0).min(1.0)
                };
                self.walk.update(target, 0.4, scale);
            }
            _ => self.walk.tick(distance, self.appearance.baby),
        }
    }

    /// `SquidRenderState`: the tentacle angle, and how the body lies.
    pub fn squid_pose(&self, partial: f32) -> (f32, mobs::squid::Tilt) {
        (
            self.squid.tentacle_angle(partial),
            self.squid.tilt(partial, self.appearance.baby),
        )
    }

    /// `SlimeRenderState.squish`.
    pub fn squish(&self, partial: f32) -> f32 {
        self.prev_squish + (self.squish - self.prev_squish) * partial
    }

    /// `Chicken.aiStep`'s wing flapping.
    pub(super) fn tick_flap(&mut self) {
        self.prev_flap = self.flap;
        self.prev_flap_speed = self.flap_speed;
        let up = if self.on_ground { -1.0 } else { 4.0 };
        self.flap_speed = (self.flap_speed + up * 0.3).clamp(0.0, 1.0);
        if !self.on_ground && self.flapping < 1.0 {
            self.flapping = 1.0;
        }
        self.flapping *= 0.9;
        self.flap += self.flapping * 2.0;
    }
}
