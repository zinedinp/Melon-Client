//! Vanilla `WalkAnimationState`.

/// `WalkAnimationState`.
#[derive(Clone, Copy, Debug)]
pub struct WalkAnimation {
    speed_old: f32,
    speed: f32,
    position: f32,
    position_scale: f32,
}

impl Default for WalkAnimation {
    fn default() -> Self {
        Self {
            speed_old: 0.0,
            speed: 0.0,
            position: 0.0,
            position_scale: 1.0,
        }
    }
}

impl WalkAnimation {
    /// `LivingEntity.updateWalkAnimation`.
    pub fn tick(&mut self, distance: f32, baby: bool) {
        let target = (distance * 4.0).min(1.0);
        self.update(target, 0.4, if baby { 3.0 } else { 1.0 });
    }

    /// `update`.
    pub fn update(&mut self, target_speed: f32, factor: f32, position_scale: f32) {
        self.speed_old = self.speed;
        self.speed += (target_speed - self.speed) * factor;
        self.position += self.speed;
        self.position_scale = position_scale;
    }

    /// `stop`: riding or dead.
    pub fn stop(&mut self) {
        self.speed_old = 0.0;
        self.speed = 0.0;
        self.position = 0.0;
    }

    /// `speed(partialTicks)`.
    pub fn speed(&self, partial: f32) -> f32 {
        (self.speed_old + (self.speed - self.speed_old) * partial).min(1.0)
    }

    /// `position(partialTicks)`.
    pub fn position(&self, partial: f32) -> f32 {
        (self.position - self.speed * (1.0 - partial)) * self.position_scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_speeds_up_the_swing_and_standing_slows_it() {
        let mut walk = WalkAnimation::default();
        for _ in 0..20 {
            walk.tick(0.2, false);
        }
        assert!((walk.speed(1.0) - 0.8).abs() < 1e-3);
        let moving = walk.position(1.0);
        assert!(moving > 10.0);
        assert!(walk.position(0.5) < moving && walk.position(0.5) > walk.position(0.0));
        for _ in 0..40 {
            walk.tick(0.0, false);
        }
        assert!(walk.speed(1.0) < 1e-3);
    }
}
