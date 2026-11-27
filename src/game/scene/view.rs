//! The camera and environment a frame is seen with.

use glam::Mat4;
use std::time::Instant;

use crate::game::InWorld;
use crate::game::camera::{self, Camera};
use crate::game::entities::EntityFrame;
use crate::game::environment::FrameEnvironment;
use crate::game::world_resources::WorldResources;
use melon_settings::options::Config;
use melon_world::fog::FogType;
use melon_world::lightmap::LightmapParams;

/// `Camera.calculateHudFov`'s start: the hand's own field of view.
const HAND_FOV: f32 = 70.0;

/// How this frame is seen.
pub(in crate::game) struct FrameView {
    pub camera: Camera,
    pub look: FrameEnvironment,
    /// `getGameTimeDeltaPartialTick(true)`.
    pub partial: f32,
    /// The level's partial tick: 1 while it is frozen.
    pub level_partial: f32,
    /// The hand's field of view, narrowed like the camera's.
    pub hand_fov: f32,
    /// Nausea's and the portal's warp.
    pub distortion: Mat4,
    pub lightmap: LightmapParams,
    pub spectator: bool,
}

impl FrameView {
    /// What the entity layer's draws read of the frame.
    pub fn entity_frame(&self) -> EntityFrame<'_> {
        let look = self.camera.look();
        EntityFrame {
            camera: self.camera.position,
            // `Camera.yRot` / `xRot`, which vanilla's billboards turn to.
            angles: (look.y_rot, look.x_rot),
            partial: self.partial,
            lightmap: &self.lightmap,
        }
    }
}

impl InWorld {
    /// This frame's camera and environment; also hands them to the ears,
    /// nametags, fog and screen effects.
    pub(super) fn frame_view(
        &mut self,
        assets: &WorldResources,
        aspect: f32,
        now: Instant,
        config: &Config,
    ) -> FrameView {
        self.fps.tick(now);
        let partial = self.partial_tick();
        let mut camera = Camera::follow(
            &self.player,
            &self.level,
            self.camera_mode,
            self.fov_with_effects(f32::from(config.fov), partial),
            camera::far_plane(config.render_distance),
        );
        camera.bob = self.hurt_bob(config.damage_tilt_strength as f32, partial);
        if config.bob_view {
            camera.bob *= self.player.view_bob();
        }
        self.update_rain_fog(&camera, aspect, partial, now);
        let look = self.environment(
            &camera,
            aspect,
            partial,
            config.render_distance,
            config.cloud_range,
        );
        let [r, g, b, _] = look.fog.color;
        self.fog.clear_color = [r, g, b, 1.0];
        self.note_sky_light(&look);

        let dying = self.body.death > 0;
        let death_time = self.own_death_time(partial);
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
        self.audio.ears = Some(crate::game::sounds::Ears {
            position: camera.position.to_array(),
            forward: camera.forward().to_array(),
            up: camera.up().to_array(),
        });
        self.name_view = Some(crate::game::nametags::NameView::of(&camera, aspect));
        self.effects_frame = self.screen_effects(
            assets,
            &look,
            partial,
            config.vignette,
            self.camera_mode == camera::CameraMode::FirstPerson,
            config.screen_effect_scale,
        );

        let atmosphere = &look.atmosphere;
        let lightmap = LightmapParams {
            sky: atmosphere.sky_light(),
            gamma: config.gamma as f32,
            ambient: atmosphere.ambient_light_color,
            block_tint: atmosphere.block_light_tint,
        };
        FrameView {
            distortion: self.distortion(partial, config.screen_effect_scale),
            hand_fov: narrowed(HAND_FOV),
            level_partial: self.level_partial_tick(),
            spectator: self.game_mode.current.is_spectator(),
            camera,
            look,
            partial,
            lightmap,
        }
    }
}
