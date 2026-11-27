//! Vanilla `Camera` and `CameraType`.

use glam::{DVec3, Mat4, Vec3};

use melon_core::math::euler::{self, Look, Orientation};

use super::InWorld;
use melon_core::math::angle::{self, Rotation};
use melon_world::culling::frustum::Frustum;
use melon_world::level::Level;
use melon_world::pick;
use melon_world::player::PlayerState;
use melon_world::projection::world_projection;

/// `Camera.getMaxZoom`'s starting distance behind or in front of the head.
const THIRD_PERSON_DISTANCE: f32 = 4.0;

/// Matches vanilla's `CameraType`: `F5` cycles through all three.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    FirstPerson,
    /// Normal over-the-shoulder chase camera, behind the player.
    ThirdPersonBack,
    /// The "selfie" view.
    ThirdPersonFront,
}

impl melon_core::cycle::Cycle for CameraMode {
    const ALL: &'static [Self] = &[
        CameraMode::FirstPerson,
        CameraMode::ThirdPersonBack,
        CameraMode::ThirdPersonFront,
    ];
}

/// One frame's camera.
pub struct Camera {
    /// `Camera.position`, in doubles: everything drawn is made relative
    /// to it before it reaches a float, so the view stays exact however
    /// far out the player is.
    pub position: DVec3,
    /// Its rotation and axes, as `Camera.setRotation` works them out.
    pub orientation: Orientation,
    /// `Camera.getViewRotationMatrix`: from world space relative to
    /// [`Self::position`] to camera space. No translation.
    pub view: Mat4,
    /// Vertical field of view, degrees.
    pub fov: f32,
    /// Far plane, blocks.
    pub far: f32,
    /// `GameRenderer.bobView`, between the projection and the view.
    pub bob: Mat4,
}

impl Camera {
    /// A camera at `position` looking `look`'s way.
    pub fn new(position: DVec3, look: Look, fov: f32, far: f32) -> Self {
        let orientation = Orientation::new(look);
        Self {
            position,
            orientation,
            view: orientation.view,
            fov,
            far,
            bob: Mat4::IDENTITY,
        }
    }

    /// `Camera.alignWithEntity`: at the eye, or pulled back along (or
    /// against) the look by `getMaxZoom`, as far as the blocks allow.
    pub fn follow(
        player: &PlayerState,
        level: &Level,
        mode: CameraMode,
        fov: f32,
        far: f32,
    ) -> Self {
        let eye = player.camera_eye();
        let look = player.look_angles();
        let look = match mode {
            CameraMode::ThirdPersonFront => look.mirrored(),
            _ => look,
        };
        let mut camera = Self::new(eye, look, fov, far);
        if mode != CameraMode::FirstPerson {
            let clip =
                |from: DVec3, to: DVec3| pick::clip_blocks(level, from, to).map(|h| h.location);
            let zoom = max_zoom(eye, camera.forward(), THIRD_PERSON_DISTANCE, clip);
            camera.position = eye + camera.offset(-zoom, 0.0, 0.0);
        }
        camera
    }

    /// `Camera.move`'s offset: `forwards`, `up` and `right` along the
    /// camera's own axes.
    pub fn offset(&self, forwards: f32, up: f32, right: f32) -> DVec3 {
        euler::transform(self.orientation.rotation, Vec3::new(right, up, -forwards)).as_dvec3()
    }

    /// `point` relative to the camera, in the floats the GPU takes.
    #[inline]
    pub fn relative(&self, point: DVec3) -> Vec3 {
        (point - self.position).as_vec3()
    }

    /// Where the camera looks, in world space (`Camera.forwards`).
    pub fn forward(&self) -> Vec3 {
        self.orientation.forwards
    }

    /// The camera's up, in world space.
    pub fn up(&self) -> Vec3 {
        self.orientation.up
    }

    /// The camera's left, in world space.
    pub fn left(&self) -> Vec3 {
        self.orientation.left
    }

    /// `Camera.yRot` and `xRot`, as vanilla's billboards turn to them.
    pub fn look(&self) -> Look {
        self.orientation.look
    }

    /// `Camera.getNearPlane`.
    pub fn near_plane(&self, aspect: f32) -> [Vec3; 5] {
        let near = melon_world::projection::WORLD_Z_NEAR;
        let half_height = (self.fov.to_radians() / 2.0).tan() * near;
        let half_width = half_height * aspect;
        let forward = self.forward() * near;
        let (up, left) = (self.up() * half_height, self.left() * half_width);
        [
            forward,
            forward + up + left,
            forward + up - left,
            forward - up + left,
            forward - up - left,
        ]
    }

    /// The view volume for a framebuffer of this `aspect`, relative to [`Self::position`]
    /// (`Frustum.prepare`).
    pub fn frustum(&self, aspect: f32) -> Frustum {
        self.frustum_with_fov(aspect, self.fov)
    }

    /// [`Self::frustum`] for another field of view.
    pub fn frustum_with_fov(&self, aspect: f32, fov: f32) -> Frustum {
        Frustum::from_view_projection(
            world_projection(fov, aspect, self.far) * self.bob * self.view,
        )
    }
}

/// `Camera.getMaxZoom`: how far behind `eye` (against `forward`) the
/// camera can go, at most `distance`, before a block is in the way of
/// any corner of a 0.2 block cube around it.
pub fn max_zoom(
    eye: DVec3,
    forward: Vec3,
    distance: f32,
    clip: impl Fn(DVec3, DVec3) -> Option<DVec3>,
) -> f32 {
    let mut distance = distance;
    for corner in 0..8 {
        let offset = Vec3::new(
            ((corner & 1) * 2 - 1) as f32,
            (((corner >> 1) & 1) * 2 - 1) as f32,
            (((corner >> 2) & 1) * 2 - 1) as f32,
        ) * 0.1;
        let from = eye + offset.as_dvec3();
        let to = from + forward.as_dvec3() * f64::from(-distance);
        if let Some(hit) = clip(from, to) {
            let squared = hit.distance_squared(eye) as f32;
            if squared < distance * distance {
                distance = squared.sqrt();
            }
        }
    }
    distance
}

/// `Camera.fovModifier`'s bounds.
const FOV_MODIFIER_RANGE: (f32, f32) = (0.1, 1.5);

/// `Camera.fovModifier` and `oldFovModifier`: the smoothed share of the
/// field of view that sprinting, flying and aiming a bow take.
///
/// Vanilla's `Camera` lives as long as the game: the modifier starts at 0
/// (Java's default) and carries over from one world into the next, so a
/// sprint's wider view is still there on the next world's first tick. The
/// app keeps it between worlds ([`super::Lasting`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FovState {
    modifier: f32,
    old: f32,
}

impl InWorld {
    /// `Camera.tickFov`.
    pub(super) fn tick_fov(&mut self, effect_scale: f32) {
        let target = self.target_fov_modifier(effect_scale);
        self.fov.old = self.fov.modifier;
        self.fov.modifier += (target - self.fov.modifier) * 0.5;
        self.fov.modifier = self
            .fov
            .modifier
            .clamp(FOV_MODIFIER_RANGE.0, FOV_MODIFIER_RANGE.1);
    }

    /// `AbstractClientPlayer.getFieldOfViewModifier`.
    fn target_fov_modifier(&self, effect_scale: f32) -> f32 {
        let mut modifier = self.player.fov_modifier();
        if let Some(using) = self.hands.using {
            if using.item_use.animation == melon_world::player::hands::UseAnimation::Bow {
                let used = (using.item_use.duration - using.remaining) as f32;
                let scale = (used / 20.0).min(1.0);
                modifier *= 1.0 - scale * scale * 0.15;
            } else if self.camera_mode == CameraMode::FirstPerson
                && self.hand_stack(using.hand).item.registry_key == "spyglass"
            {
                return 0.1;
            }
        }
        1.0 + (modifier - 1.0) * effect_scale
    }

    /// `Camera.calculateFov`'s `fov * lerp(partialTicks, old, new)`.
    ///
    /// Before the game's first tick both are 0, as in vanilla; the floor
    /// keeps that frame's projection finite (vanilla's would divide by 0).
    pub(super) fn fov_with_effects(&self, fov: f32, partial: f32) -> f32 {
        let modifier = self.fov.old + (self.fov.modifier - self.fov.old) * partial;
        fov * modifier.max(FOV_MODIFIER_RANGE.0)
    }

    /// `GameRenderer.bobHurt`.
    pub(super) fn hurt_bob(&self, strength: f32, partial: f32) -> Mat4 {
        let mut bob = Mat4::IDENTITY;
        if self.status.health <= 0.0 {
            let duration = (self.body.death as f32 + partial).min(20.0);
            bob *= Mat4::rot_z_deg(40.0 - 8000.0 / (duration + 200.0));
        }
        let hurt = self.body.hurt as f32 - partial;
        if hurt >= 0.0 {
            let hurt = hurt / melon_world::entity::tracker::HURT_DURATION as f32;
            let hurt = angle::sin(hurt * hurt * hurt * hurt * melon_core::math::angle::PI);
            let direction = self.body.hurt_dir;
            bob *= Mat4::rot_y_deg(-direction)
                * Mat4::rot_z_deg(-hurt * 14.0 * strength)
                * Mat4::rot_y_deg(direction);
        }
        bob
    }
}

/// `Camera.modifyFovBasedOnDeathOrFluid`.
pub fn dying_fov(fov: f32, death_time: f32) -> f32 {
    let duration = death_time.min(20.0);
    fov / ((1.0 - 500.0 / (duration + 500.0)) * 2.0 + 1.0)
}

/// `GameRenderer.getDepthFar`.
pub fn far_plane(render_distance: u8) -> f32 {
    (f32::from(render_distance) * 16.0 * 4.0).max(256.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use melon_core::math::angle::{HALF_PI, Rotation};

    /// A wall at `x = wall`, hit by any segment crossing it.
    fn wall(wall: f64) -> impl Fn(DVec3, DVec3) -> Option<DVec3> {
        move |from, to| {
            let (low, high) = (from.x.min(to.x), from.x.max(to.x));
            (low <= wall && wall <= high).then(|| DVec3::new(wall, from.y, from.z))
        }
    }

    /// A million blocks out a float steps by 1/16: the camera, made from
    /// doubles, still moves by the 1/200 block a slow frame moves it,
    /// and what it culls and draws is relative to it before it is a float.
    #[test]
    fn far_out_the_camera_is_as_exact_as_at_the_origin() {
        use melon_world::culling::{CullSettings, Culler};
        let far = DVec3::new(1_000_000.0, 64.0, -2_000_000.0);
        let mut player = PlayerState::new(far);
        player.teleport(far + DVec3::X * 0.01, DVec3::ZERO, far);
        player.update_render(0.5, 0.0);
        let eye = player.camera_eye();
        assert_eq!(eye.x, 1_000_000.005);
        // f32 can't hold this: the old eye snapped to a 16th of a block.
        assert_eq!(eye.x as f32 as f64, 1_000_000.0);

        let camera = Camera::new(eye, Look::new(-90.0, 0.0), 70.0, 256.0);
        let ahead = eye + DVec3::new(2.0, 0.0, 0.0);
        assert!((camera.relative(ahead) - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-9);
        let culler = Culler::new(
            camera.frustum(1.0),
            None,
            camera.position,
            (-64, 24),
            CullSettings::default(),
        );
        let block = |at: DVec3| (at - DVec3::splat(0.05), at + DVec3::splat(0.05));
        let (min, max) = block(ahead);
        assert!(culler.sees_box(min, max));
        let (min, max) = block(eye - DVec3::new(2.0, 0.0, 0.0));
        assert!(!culler.sees_box(min, max));
        // A tenth-block box just past the edge of the view stays culled.
        let edge = eye + DVec3::new(2.0, 0.0, 2.0 * (35f64.to_radians().tan() + 0.1));
        let (min, max) = block(edge);
        assert!(!culler.sees_box(min, max));
    }

    #[test]
    fn the_camera_goes_the_whole_way_back_in_open_air() {
        let eye = DVec3::new(0.0, 65.0, 0.0);
        assert_eq!(max_zoom(eye, Vec3::X, 4.0, |_, _| None), 4.0);
    }

    #[test]
    fn a_wall_behind_the_player_stops_the_camera_short_of_it() {
        let eye = DVec3::new(0.0, 65.0, 0.0);
        let zoom = max_zoom(eye, Vec3::X, 4.0, wall(-2.0));
        assert!((2.0..2.01).contains(&zoom), "{zoom}");
        assert_eq!(max_zoom(eye, Vec3::X, 4.0, wall(2.0)), 4.0);
    }

    #[test]
    fn a_dying_camera_narrows_its_view_for_twenty_ticks() {
        assert_eq!(dying_fov(70.0, 0.0), 70.0 / 1.0);
        let narrowed = dying_fov(70.0, 20.0);
        assert!(narrowed < 70.0 && narrowed > 60.0, "{narrowed}");
        assert_eq!(dying_fov(70.0, 40.0), dying_fov(70.0, 20.0));
    }

    #[test]
    fn the_axes_follow_the_view() {
        // Vanilla's yRot -90 looks east.
        let camera = Camera::new(DVec3::ZERO, Look::new(-90.0, 0.0), 70.0, 256.0);
        assert!(camera.view.abs_diff_eq(Mat4::rot_y(HALF_PI), 1e-6));
        assert!((camera.forward() - Vec3::X).length() < 1e-6);
        assert!((camera.up() - Vec3::Y).length() < 1e-6);
        assert!((camera.left() - Vec3::NEG_Z).length() < 1e-6);
        let back = camera.offset(-4.0, 0.0, 0.0);
        assert!(back.abs_diff_eq(DVec3::NEG_X * 4.0, 1e-6), "{back}");
        let plane = camera.near_plane(2.0);
        assert!((plane[0] - Vec3::X * 0.05).length() < 1e-6);
        let mean = plane[1..].iter().copied().sum::<Vec3>() / 4.0;
        assert!((mean - plane[0]).length() < 1e-6);
    }
}
