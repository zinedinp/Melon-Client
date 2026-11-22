//! What a tracked entity looks like this frame.

use super::*;

impl ClientEntity {
    pub fn pos(&self, partial: f32) -> DVec3 {
        self.prev_pos.lerp(self.pos, f64::from(partial))
    }

    /// Width and height (`EntityType.dimensions`, halved for babies).
    pub fn size(&self) -> (f32, f32) {
        if self.kind == EntityKind::ArmorStand && self.appearance.stand_marker() {
            return (0.0, 0.0);
        }
        if let Some(display) = &self.display {
            return (display.width, display.height);
        }
        if self.kind == EntityKind::Interaction {
            let b = self.interaction.unwrap_or_default();
            return (b.width, b.height);
        }
        let scale = self.scale() * self.entity_scale();
        let [w, h] = self.ty.dimension;
        (w * scale, h * scale)
    }

    /// `LivingEntity.getScale`: the `scale` attribute, as `sanitizeScale`
    /// keeps it (a dragon's always 1, a shulker's at most 3); 1 for what
    /// isn't living.
    pub fn entity_scale(&self) -> f32 {
        if !self.is_living() {
            return 1.0;
        }
        match self.kind {
            EntityKind::EnderDragon => 1.0,
            EntityKind::Shulker => self.scale_attribute.min(3.0),
            _ => self.scale_attribute,
        }
    }

    /// What the type's dimensions are multiplied by.
    pub fn scale(&self) -> f32 {
        if self.kind.is_cube() {
            // `AbstractCubeMob.getDefaultDimensions`: by size alone, a baby
            // sulfur cube is simply size 1.
            self.appearance.size as f32
        } else if self.appearance.baby
            || (self.kind == EntityKind::ArmorStand && self.appearance.stand_small())
        {
            0.5
        } else {
            1.0
        }
    }

    /// Whether it is a `LivingEntity`, which has attributes.
    pub fn is_living(&self) -> bool {
        !self.ty.attributes.is_empty()
    }

    /// The bounding box this frame.
    pub fn bounds(&self, partial: f32) -> (DVec3, DVec3) {
        let feet = self.pos(partial);
        let (mut w, mut h) = self.size();
        if self.kind == EntityKind::ArmorStand && self.appearance.stand_marker() {
            [w, h] = self.ty.dimension;
        }
        let half = DVec3::new(f64::from(w) / 2.0, 0.0, f64::from(w) / 2.0);
        (feet - half, feet + half + DVec3::Y * f64::from(h))
    }

    /// Where it stands this tick, for a click's hit point.
    pub fn tick_pos(&self) -> DVec3 {
        self.pos
    }

    /// Where it stood the tick before (`xo`, `yo`, `zo`).
    pub fn prev_tick_pos(&self) -> DVec3 {
        self.prev_pos
    }

    /// `getDeltaMovement`, per tick.
    pub fn velocity(&self) -> DVec3 {
        self.velocity
    }

    /// `Entity.blocksBuilding`.
    pub fn blocks_building(&self) -> bool {
        if self.is_living() {
            return self.appearance.health > 0.0 && self.kind != EntityKind::ArmorStand
                || self.kind == EntityKind::ArmorStand && !self.appearance.stand_marker();
        }
        matches!(self.kind, EntityKind::Boat)
    }

    /// The box that stops what walks into it (`canBeCollidedWith`).
    pub fn collider(&self, other_feet: f64) -> Option<Aabb> {
        if self.appearance.health <= 0.0 && self.is_living() {
            return None;
        }
        let (w, h) = self.size();
        let bounds = Aabb::at_feet(self.pos, f64::from(w), f64::from(h));
        let solid = match self.kind {
            EntityKind::Boat => true,
            EntityKind::HappyGhast => !self.appearance.baby && other_feet >= bounds.max.y,
            _ => false,
        };
        solid.then_some(bounds)
    }

    /// The box the crosshair hits (`getBoundingBox` at this tick's position).
    pub fn pick_box(&self) -> Option<Aabb> {
        if !self.pickable || (self.kind == EntityKind::ArmorStand && self.appearance.stand_marker())
        {
            return None;
        }
        let (w, h) = self.size();
        Some(Aabb::at_feet(self.pos, f64::from(w), f64::from(h)))
    }

    /// This entity as something to ride: what its seats depend on.
    pub fn as_vehicle(&self) -> ride::Vehicle<'_> {
        ride::Vehicle {
            ty: self.ty,
            baby: self.appearance.baby,
            yaw: self.yaw,
            scale: 1.0,
            rearing: if self.kind.is_equine() {
                self.equine.0.stand
            } else {
                0.0
            },
        }
    }

    /// The body turn of a living entity, last tick's and this tick's.
    pub fn body_turn(&self) -> Option<(f32, f32)> {
        self.is_living().then_some((self.prev_body, self.body))
    }

    /// `getYRot(partial)` and `getXRot(partial)`, vanilla's degrees (0 faces
    /// What a projectile's, crystal's or fangs' `setupAnim` reads.
    pub fn thing_pose(&self, partial: f32) -> mobs::projectile::ThingPose {
        mobs::projectile::ThingPose {
            y_rot: self.yaw_degrees(partial),
            x_rot: self.pitch_degrees(partial),
            age_in_ticks: self.age_in_ticks(partial),
            bite: mobs::projectile::bite_progress(self.fangs_life, partial),
            shake: if self.arrow_shake > 0 {
                self.arrow_shake as f32 - partial
            } else {
                0.0
            },
            shows_bottom: self.appearance.thing.show_bottom,
        }
    }

    pub fn yaw_degrees(&self, partial: f32) -> f32 {
        rot_lerp(partial, self.prev_yaw, self.yaw)
    }

    pub fn pitch_degrees(&self, partial: f32) -> f32 {
        self.prev_pitch + (self.pitch - self.prev_pitch) * partial
    }

    /// A hit vehicle's shake (`state.hurtTime`, `damageTime`, `hurtDir`).
    pub fn vehicle_shake(&self, partial: f32) -> mobs::vehicle::Shake {
        let a = &self.appearance;
        mobs::vehicle::Shake {
            hurt: a.vehicle_hurt as f32 - partial,
            damage: (a.vehicle_damage - partial).max(0.0),
            dir: a.vehicle_hurt_dir,
        }
    }

    /// Where a boat's paddles are.
    pub fn rowing(&self, partial: f32) -> mobs::vehicle::Rowing {
        self.paddles.rowing(self.appearance.paddles, partial)
    }

    /// `getPreciseBodyRotation`.
    pub fn body_yaw(&self, partial: f32) -> f32 {
        rot_lerp(partial, self.prev_body, self.body)
    }

    /// `Cat.getLieDownAmount` and the other two, between ticks.
    pub fn feline_amounts(&self, partial: f32) -> feline::Amounts {
        feline::Amounts::lerp(self.feline.0, self.feline.1, partial)
    }

    /// `AbstractHorse.getEatAnim` and the other two, between ticks.
    pub fn equine_amounts(&self, partial: f32) -> horse::Amounts {
        horse::Amounts::lerp(self.equine.0, self.equine.1, partial)
    }

    /// `Wolf.getHeadRollAngle`.
    pub fn head_roll(&self, partial: f32) -> f32 {
        let angle = self.interest.0 + (self.interest.1 - self.interest.0) * partial;
        let factor = if self.kind == EntityKind::Fox {
            0.11
        } else {
            0.15
        };
        angle * factor * crate::math::angle::PI
    }

    /// `Fox.getCrouchAmount`.
    pub fn fox_crouch(&self, partial: f32) -> f32 {
        self.fox_crouch.0 + (self.fox_crouch.1 - self.fox_crouch.0) * partial
    }

    /// `PolarBear.getStandingAnimationScale`.
    pub fn polar_stand(&self, partial: f32) -> f32 {
        (self.polar_stand.0 + (self.polar_stand.1 - self.polar_stand.0) * partial) / 6.0
    }

    /// How long the armadillo has been in its synced state.
    pub fn armadillo_ticks(&self) -> u32 {
        self.armadillo_ticks
    }

    /// `LivingEntity.getTicksUsingItem(partial)`.
    pub fn ticks_using_item(&self, partial: f32) -> f32 {
        if self.using_ticks == 0 {
            0.0
        } else {
            self.using_ticks as f32 + partial
        }
    }

    /// `getEyePosition`: where its light is sampled.
    pub fn eye(&self, partial: f32) -> Vec3 {
        self.pos(partial).as_vec3() + Vec3::Y * self.ty.eye_height * self.scale()
    }

    /// `Entity.getViewVector`.
    pub fn view_vector(&self, partial: f32) -> Vec3 {
        let yaw = rot_lerp(partial, self.prev_head, self.head).to_radians();
        let pitch = (self.prev_pitch + (self.pitch - self.prev_pitch) * partial).to_radians();
        Vec3::new(
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
    }

    /// `extractRenderState`.
    pub fn living_state(&self, partial: f32) -> LivingRenderState {
        let head = rot_lerp(partial, self.prev_head, self.head);
        let body = match self.vehicle_body {
            Some((before, now)) => {
                let mount = rot_lerp(partial, before, now);
                let turn = wrap_degrees(head - mount).clamp(-85.0, 85.0);
                head - turn + if turn.abs() > 50.0 { turn * 0.2 } else { 0.0 }
            }
            None => rot_lerp(partial, self.prev_body, self.body),
        };
        let age = self.age as f32 + partial;
        let riding = self.vehicle.is_some();
        let shake = if self.appearance.shaking() {
            (age.floor() * 3.25).cos() * crate::math::angle::PI * 0.4
        } else {
            0.0
        };
        let scale = self.entity_scale();
        let upside_down = self.upside_down().then(|| self.size().1);
        let invert = if upside_down.is_some() { -1.0 } else { 1.0 };
        LivingRenderState {
            position: self.pos(partial).as_vec3(),
            body_rot: body - 180.0 + shake,
            y_rot: wrap_degrees(head - body) * invert,
            x_rot: (self.prev_pitch + (self.pitch - self.prev_pitch) * partial) * invert,
            walk_position: if riding {
                0.0
            } else {
                self.walk.position(partial)
            },
            walk_speed: if riding {
                0.0
            } else {
                self.walk.speed(partial)
            },
            age_in_ticks: age,
            death_time: if self.death_time > 0 {
                self.death_time as f32 + partial
            } else {
                0.0
            },
            hurt: self.hurt_time > 0 || self.death_time > 0,
            baby: self.appearance.baby,
            riding,
            swim: self.swim(partial),
            upside_down,
            scale,
        }
    }

    /// `Creeper.getSwelling`: 0 to about 1 as the fuse burns.
    pub fn swelling(&self, partial: f32) -> f32 {
        let swell = self.prev_swell as f32 + (self.swell - self.prev_swell) as f32 * partial;
        swell / (MAX_SWELL - 2) as f32
    }

    /// `ChickenRenderState.flap` and `flapSpeed`.
    pub fn flap(&self, partial: f32) -> (f32, f32) {
        (
            self.prev_flap + (self.flap - self.prev_flap) * partial,
            self.prev_flap_speed + (self.flap_speed - self.prev_flap_speed) * partial,
        )
    }
}
