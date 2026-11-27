//! What the player sees laid over the world, and what warps the view.

use glam::{Mat4, Vec3};
use pumpkin_data::{Block, BlockStateId};

use super::InWorld;
use super::environment::FrameEnvironment;
use super::world_resources::WorldResources;
use melon_core::math::angle;
use melon_gui::effects::{AtlasSprite, ScreenEffects, Underwater, Vignette};
use melon_world::fog::FogType;
use melon_world::level::Level;
use melon_world::physics::Aabb;

/// What changes from tick to tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectState {
    /// `Hud.vignetteBrightness`.
    vignette: f32,
    /// `LocalPlayer.portalEffectIntensity` now and a tick ago.
    portal: f32,
    old_portal: f32,
    /// `spinningEffectTime` and `spinningEffectSpeed`, in degrees.
    spin_time: f32,
    spin_speed: f32,
    /// Shared flag 0 (`Entity.FLAG_ON_FIRE`).
    on_fire: bool,
    /// Remaining nausea ticks (`-1` infinite, `0` gone).
    nausea_ticks: i32,
    /// `MobEffectInstance` blend factor, previous tick and now.
    old_nausea: f32,
    nausea: f32,
    /// How bright the sky counts, as of the last frame.
    sky_light_level: f32,
}

impl EffectState {
    /// A new world's, with the vignette the last one left.
    pub(super) fn with_vignette(vignette: f32) -> Self {
        Self {
            vignette,
            ..Self::default()
        }
    }

    pub(super) fn vignette(&self) -> f32 {
        self.vignette
    }

    /// How bright the sky counts, as of the last frame.
    pub fn sky_light_level(&self) -> f32 {
        self.sky_light_level
    }

    pub fn set_on_fire(&mut self, on: bool) {
        self.on_fire = on;
    }

    pub fn set_nausea_ticks(&mut self, ticks: i32) {
        self.nausea_ticks = ticks;
    }

    fn nausea(&self, partial: f32) -> f32 {
        self.old_nausea + (self.nausea - self.old_nausea) * partial
    }
}

/// Nausea `setBlendDuration(150, 20, 60)`.
const NAUSEA_BLEND_IN: f32 = 1.0 / 150.0;
const NAUSEA_BLEND_OUT: f32 = 1.0 / 20.0;
const NAUSEA_BLEND_OUT_ADVANCE: i32 = 60;

/// `Lightmap.getBrightness`.
pub fn level_brightness(level: u8, ambient_light: f32) -> f32 {
    let v = f32::from(level.min(15)) / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    curved + (1.0 - curved) * ambient_light
}

/// `getMaxLocalRawBrightness`.
pub fn raw_brightness(block: u8, sky: u8, sky_light_level: f32) -> u8 {
    let darken = (15.0 - sky_light_level).round().clamp(0.0, 15.0) as i32;
    block.max((i32::from(sky) - darken).clamp(0, 15) as u8)
}

impl InWorld {
    /// One client tick of the effects.
    pub(super) fn tick_effects(&mut self) {
        let level = &self.level;
        let feet = self.player.feet();
        let inside = Aabb::at_feet(feet, 0.6, 1.8).deflate(1.0e-5);
        let in_portal = touches_portal(level, &inside);
        let state = &mut self.effects;

        state.old_portal = state.portal;
        let step = if in_portal {
            0.0125
        } else if state.portal > 0.0 {
            -0.05
        } else {
            0.0
        };
        state.portal = (state.portal + step).clamp(0.0, 1.0);

        if state.nausea_ticks > 0 {
            state.nausea_ticks -= 1;
        }
        state.old_nausea = state.nausea;
        let has = state.nausea_ticks < 0 || state.nausea_ticks > NAUSEA_BLEND_OUT_ADVANCE;
        let target = if has { 1.0 } else { 0.0 };
        let delta = if has {
            NAUSEA_BLEND_IN
        } else {
            NAUSEA_BLEND_OUT
        };
        state.nausea =
            (state.nausea + (target - state.nausea).clamp(-delta, delta)).clamp(0.0, 1.0);

        let nausea = state.nausea;
        if state.portal > 0.0 || nausea > 0.0 {
            state.spin_speed = (state.portal * 20.0 + nausea * 7.0) / (state.portal + nausea);
            state.spin_time += state.spin_speed;
        } else {
            state.spin_speed = 0.0;
        }

        let eye = self.player.tick_eye().floor().as_ivec3();
        let (block, sky) = level.light_at(eye.x, eye.y, eye.z);
        let brightness = level_brightness(
            raw_brightness(block, sky, state.sky_light_level),
            self.level_info.dimension_type.ambient_light,
        );
        let target = (1.0 - brightness).clamp(0.0, 1.0);
        state.vignette += (target - state.vignette) * 0.01;
    }

    /// The warp of the view this frame.
    pub(super) fn distortion(&self, partial: f32, effect_scale: f64) -> Mat4 {
        let state = &self.effects;
        let portal = state.old_portal + (state.portal - state.old_portal) * partial;
        let nausea = state.nausea(partial);
        let scale = effect_scale as f32;
        let intensity = portal.max(nausea) * scale * scale;
        if intensity <= 0.0 {
            return Mat4::IDENTITY;
        }
        let skew = 5.0 / (intensity * intensity + 5.0) - intensity * 0.04;
        let skew = skew * skew;
        let axis = Vec3::new(
            0.0,
            melon_core::math::SQRT_2 / 2.0,
            melon_core::math::SQRT_2 / 2.0,
        );
        let angle = (state.spin_time + partial * state.spin_speed).to_radians();
        Mat4::from_quat(angle::axis_angle(axis, angle))
            * Mat4::from_scale(Vec3::new(1.0 / skew, 1.0, 1.0))
            * Mat4::from_quat(angle::axis_angle(axis, -angle))
    }

    /// Remembers how bright the sky counts, for the next tick's vignette.
    pub(super) fn note_sky_light(&mut self, look: &FrameEnvironment) {
        self.effects.sky_light_level = look.atmosphere.sky_light_level;
    }

    /// Everything laid over this frame's view.
    pub(super) fn screen_effects(
        &self,
        assets: &WorldResources,
        look: &FrameEnvironment,
        partial: f32,
        vignette: bool,
        first_person: bool,
        effect_scale: f64,
    ) -> ScreenEffects {
        let state = &self.effects;
        let spectator = self.game_mode.current.is_spectator();
        let sprite = |id: &str| {
            assets.models.sprite(id).map(|uv| AtlasSprite {
                atlas: assets.atlas,
                uv,
            })
        };
        let mut effects = ScreenEffects {
            vignette: vignette.then_some(Vignette {
                brightness: state.vignette,
                border_warning: {
                    let feet = self.player.feet();
                    self.border.warning(feet.x, feet.z)
                },
            }),
            portal: state.old_portal + (state.portal - state.old_portal) * partial,
            portal_sprite: sprite("minecraft:block/nether_portal"),
            ..ScreenEffects::default()
        };
        let nausea = state.nausea(partial);
        if effects.portal <= 0.0 && nausea > 0.0 && effect_scale < 1.0 {
            effects.nausea = nausea * (1.0 - effect_scale as f32);
        }
        if first_person && !spectator {
            effects.wall = self.view_blocking_sprite(assets);
            if look.fog_type == FogType::Water {
                let eye = self.player.tick_eye().floor().as_ivec3();
                let (block, sky) = self.level.light_at(eye.x, eye.y, eye.z);
                let raw = raw_brightness(block, sky, look.atmosphere.sky_light_level);
                let b = level_brightness(raw, self.level_info.dimension_type.ambient_light);
                effects.underwater = Some(Underwater {
                    color: [b, b, b, 0.1],
                    u_offset: -super::minecraft_yaw(self.player.yaw()) / 64.0,
                    v_offset: self.player.pitch() / 64.0,
                });
            }
            if look.fog_type == FogType::Lava || state.on_fire {
                effects.fire = sprite("minecraft:block/fire_1");
            }
        }
        effects
    }

    /// `LevelExtractor.getViewBlockingState`.
    fn view_blocking_sprite(&self, assets: &WorldResources) -> Option<AtlasSprite> {
        let feet = self.player.feet();
        let eye_y = self.player.tick_eye().y;
        let mut found = None;
        for i in 0..8 {
            let x = feet.x + (f64::from(i % 2) - 0.5) * 0.6 * 0.8;
            let y = eye_y + (f64::from((i >> 1) % 2) - 0.5) * 0.1;
            let z = feet.z + (f64::from((i >> 2) % 2) - 0.5) * 0.6 * 0.8;
            let (bx, by, bz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
            if let Some(state) = self.level.block_state(bx, by, bz)
                && melon_world::block::state_flags::is_solid(state)
                && let Some(uv) = assets.models.particle_sprite(state)
            {
                found = Some(AtlasSprite {
                    atlas: assets.atlas,
                    uv,
                });
            }
        }
        found
    }
}

/// Whether `region` touches a nether portal block.
fn touches_portal(level: &Level, region: &Aabb) -> bool {
    let (min, max) = (region.min.floor().as_ivec3(), region.max.floor().as_ivec3());
    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                if level.block_state(x, y, z).is_some_and(is_portal) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_portal(state: u16) -> bool {
    BlockStateId::new(state).is_some_and(|id| Block::from_state_id(id) == &Block::NETHER_PORTAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_curves_and_the_nethers_ambient_lifts_the_dark() {
        assert_eq!(level_brightness(0, 0.0), 0.0);
        assert!((level_brightness(15, 0.0) - 1.0).abs() < 1e-6);
        assert!((level_brightness(0, 0.1) - 0.1).abs() < 1e-6);
        assert!(level_brightness(7, 0.0) < 0.5);
    }

    #[test]
    fn the_night_sky_counts_for_less() {
        assert_eq!(raw_brightness(0, 15, 15.0), 15);
        assert_eq!(raw_brightness(0, 15, 4.0), 4);
        assert_eq!(raw_brightness(9, 15, 4.0), 9);
        assert_eq!(raw_brightness(0, 3, 4.0), 0);
    }

    #[test]
    fn a_portal_block_is_found_around_the_player() {
        use melon_world::chunk::{Chunk, Section};
        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let portal = Block::NETHER_PORTAL.default_state.id.as_u16();
        level.set_block([4, 70, 4], portal, false);
        let at = |x: f64| Aabb::at_feet(glam::DVec3::new(x, 70.0, 4.5), 0.6, 1.8).deflate(1e-5);
        assert!(touches_portal(&level, &at(4.5)));
        assert!(!touches_portal(&level, &at(6.5)));
        assert!(is_portal(portal));
    }
}
