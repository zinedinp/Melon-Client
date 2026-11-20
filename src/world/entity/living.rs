//! What every living entity's renderer shares (vanilla

use glam::{Mat4, Vec3};

use crate::math::angle::Rotation;
use crate::math::wrap_degrees;

/// How a swimming body is laid down: `AvatarRenderer.setupRotations` or
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SwimTurn {
    #[default]
    None,
    /// A player: flat on the water along the look when in it, else flat
    Avatar {
        in_water: bool,
        visually_swimming: bool,
    },
    /// A drowned: tipped about the middle of its box, `height` blocks tall
    Drowned { height: f32 },
}

/// `swimAmount`, 0 to 1 over about eleven ticks, and how it lays the body.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Swim {
    pub amount: f32,
    pub turn: SwimTurn,
}

impl Swim {
    /// `LivingEntity.updateSwimAmount`'s step, once a tick: 0.09 toward 1
    pub fn step(amount: f32, swimming: bool) -> f32 {
        if swimming {
            (amount + 0.09).min(1.0)
        } else {
            (amount - 0.09).max(0.0)
        }
    }
}

/// `LivingEntityRenderState`, as far as Melon's models read it.
#[derive(Clone, Copy, Debug)]
pub struct LivingRenderState {
    /// Feet, in the world (or relative to a draw's origin).
    pub position: Vec3,
    /// `bodyRot`, Melon's yaw (0 faces north), degrees.
    pub body_rot: f32,
    /// The head's yaw relative to the body (`yRot`), degrees.
    pub y_rot: f32,
    /// The head's pitch (`xRot`), degrees.
    pub x_rot: f32,
    /// `walkAnimationPos` and `walkAnimationSpeed`.
    pub walk_position: f32,
    pub walk_speed: f32,
    /// `ageInTicks`.
    pub age_in_ticks: f32,
    /// `deathTime` with the partial tick, 0 while alive.
    pub death_time: f32,
    /// Hurt or dying: the red overlay.
    pub hurt: bool,
    pub baby: bool,
    /// `isPassenger`: sitting on a vehicle, legs bent and no walking.
    pub riding: bool,
    pub swim: Swim,
    /// Drawn upside down (`isUpsideDown`: a `Dinnerbone`), with the body's
    pub upside_down: Option<f32>,
    /// `scale`: the `scale` attribute the whole model grows by, about its
    /// feet.
    pub scale: f32,
}

impl Default for LivingRenderState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            body_rot: 0.0,
            y_rot: 0.0,
            x_rot: 0.0,
            walk_position: 0.0,
            walk_speed: 0.0,
            age_in_ticks: 0.0,
            death_time: 0.0,
            hurt: false,
            baby: false,
            riding: false,
            swim: Swim::default(),
            upside_down: None,
            scale: 1.0,
        }
    }
}

/// How a renderer places its model: `LivingEntityRenderer.scale`'s own
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub scale: Vec3,
    pub flip_degrees: f32,
    /// `CatRenderer.setupRotations`: how far a cat lies on its side.
    pub lie_down: f32,
    /// `IronGolemRenderer.setupRotations`: degrees it rocks sideways.
    pub roll_degrees: f32,
    /// `SquidRenderer.setupRotations`, which replaces the body's turn and
    pub squid: Option<super::mobs::squid::Tilt>,
    /// A fish's wag, and its flop out of water.
    pub fish: Option<super::mobs::fish::Wobble>,
    /// Extra pitch after the body's turn (`FoxRenderer.setupRotations`).
    pub extra_pitch: f32,
    /// A move in the scaled model's space before its 1.501 drop (a
    /// sulfur cube's `vOffset`).
    pub offset: Vec3,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            scale: Vec3::ONE,
            flip_degrees: 90.0,
            lie_down: 0.0,
            roll_degrees: 0.0,
            squid: None,
            fish: None,
            extra_pitch: 0.0,
            offset: Vec3::ZERO,
        }
    }
}

/// `LivingEntityRenderer.submit`'s transform: `EntityRenderDispatcher`
pub fn root_transform(state: &LivingRenderState, placement: Placement) -> Mat4 {
    let bob = placement.fish.map_or(0.0, |fish| fish.bob);
    // `poseStack.scale(scale)` before `setupRotations`, which takes its
    // moves in the scaled space (`boundingBoxHeight / entityScale`).
    let scale = if state.scale > 0.0 { state.scale } else { 1.0 };
    let grow = Mat4::from_scale(Vec3::splat(scale));
    let mut m = Mat4::from_translation(state.position + Vec3::new(0.0, bob, 0.0))
        * grow
        * Mat4::rot_y(-state.body_rot.to_radians());
    if let Some(tilt) = placement.squid {
        m = Mat4::from_translation(state.position + Vec3::new(0.0, tilt.lift(), 0.0))
            * grow
            * Mat4::rot_y(-state.body_rot.to_radians())
            * Mat4::rot_x_deg(tilt.x_degrees)
            * Mat4::rot_y_deg(tilt.z_degrees)
            * Mat4::from_translation(Vec3::new(0.0, tilt.drop(), 0.0));
    } else if state.death_time > 0.0 {
        let fall = ((state.death_time - 1.0) / 20.0 * 1.6)
            .max(0.0)
            .sqrt()
            .min(1.0);
        m *= Mat4::rot_z_deg(fall * placement.flip_degrees);
    } else if let Some(height) = state.upside_down {
        m *= Mat4::from_translation(Vec3::new(0.0, (height + 0.1) / scale, 0.0))
            * Mat4::rot_z_deg(180.0);
    }
    if let Some(fish) = placement.fish {
        m *= Mat4::rot_y_deg(fish.yaw);
        if let Some(flop) = fish.flop {
            m *= Mat4::from_translation(flop) * Mat4::rot_z_deg(90.0);
        }
    }
    if placement.roll_degrees != 0.0 {
        m *= Mat4::rot_z_deg(placement.roll_degrees);
    }
    if placement.lie_down > 0.0 {
        let a = placement.lie_down;
        m *= Mat4::from_translation(Vec3::new(0.4 * a, 0.15 * a, 0.1 * a))
            * Mat4::rot_z_deg(90.0 * a);
    }
    if state.swim.amount > 0.0 {
        m *= swim_rotation(state);
    }
    if placement.extra_pitch != 0.0 {
        m *= Mat4::rot_x_deg(placement.extra_pitch);
    }
    m * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0) * placement.scale)
        * Mat4::from_translation(placement.offset + Vec3::new(0.0, -1.501, 0.0))
}

/// The rest of `setupRotations` for a swimmer, after the yaw and the
fn swim_rotation(state: &LivingRenderState) -> Mat4 {
    let amount = state.swim.amount;
    match state.swim.turn {
        SwimTurn::None => Mat4::IDENTITY,
        SwimTurn::Avatar {
            in_water,
            visually_swimming,
        } => {
            let target = if in_water { -90.0 - state.x_rot } else { -90.0 };
            let tipped = Mat4::rot_x_deg(amount * target);
            if visually_swimming {
                tipped * Mat4::from_translation(Vec3::new(0.0, -1.0, 0.3))
            } else {
                tipped
            }
        }
        SwimTurn::Drowned { height } => {
            let middle = Vec3::new(0.0, height / 2.0, 0.0);
            let target = -10.0 - state.x_rot;
            Mat4::from_translation(middle)
                * Mat4::rot_x_deg(amount * target)
                * Mat4::from_translation(-middle)
        }
    }
}

/// `LivingEntity.tickHeadTurn`: the body follows where it is heading
pub fn tick_head_turn(body: f32, target: f32, head: f32, max_head: f32) -> f32 {
    let mut body = body + wrap_degrees(target - body) * 0.3;
    let diff = wrap_degrees(head - body);
    if diff.abs() > max_head {
        body += diff - diff.signum() * max_head;
    }
    body
}

/// `Mth.rotateIfNecessary`: `current` turned just enough to be within
pub fn rotate_if_necessary(current: f32, target: f32, max: f32) -> f32 {
    let diff = wrap_degrees(target - current);
    target - diff.clamp(-max, max)
}

/// `BodyRotationControl.clientTick`: how a mob's body follows its head on
#[derive(Clone, Copy, Debug, Default)]
pub struct BodyRotation {
    head_stable_time: i32,
    last_stable_head: f32,
}

impl BodyRotation {
    /// `Mob.getMaxHeadYRot`.
    pub const MAX_HEAD: f32 = 75.0;

    /// One tick; `body` and `head` are updated in place.
    pub fn tick(&mut self, moved: bool, yaw: f32, body: &mut f32, head: &mut f32) {
        if moved {
            *body = yaw;
            *head = rotate_if_necessary(*head, *body, Self::MAX_HEAD);
            self.last_stable_head = *head;
            self.head_stable_time = 0;
        } else if (*head - self.last_stable_head).abs() > 15.0 {
            self.head_stable_time = 0;
            self.last_stable_head = *head;
            *body = rotate_if_necessary(*body, *head, Self::MAX_HEAD);
        } else {
            self.head_stable_time += 1;
            if self.head_stable_time > 10 {
                let fraction = ((self.head_stable_time - 10) as f32 / 10.0).clamp(0.0, 1.0);
                *body = rotate_if_necessary(*body, *head, Self::MAX_HEAD * (1.0 - fraction));
            }
        }
    }
}

/// `Mth.rotLerp`: from `a` toward `b` the short way round.
pub fn rot_lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * wrap_degrees(b - a)
}

#[cfg(test)]
mod upside_down_tests {
    use super::*;

    /// Where the model's down (+Y, pixels) points in the world.
    fn model_down(state: &LivingRenderState) -> f32 {
        root_transform(state, Placement::default())
            .transform_vector3(Vec3::Y)
            .y
    }

    #[test]
    fn an_upside_down_body_stands_on_its_head_but_a_dying_one_falls_as_usual() {
        let upright = LivingRenderState::default();
        assert!(model_down(&upright) < 0.0);
        let flipped = LivingRenderState {
            upside_down: Some(1.8),
            ..Default::default()
        };
        assert!(model_down(&flipped) > 0.0);
        let lift = root_transform(&flipped, Placement::default())
            .transform_point3(Vec3::ZERO)
            .y
            - root_transform(&upright, Placement::default())
                .transform_point3(Vec3::ZERO)
                .y;
        assert!(lift.abs() > 0.5, "{lift}");
        let dying = LivingRenderState {
            upside_down: Some(1.8),
            death_time: 20.0,
            ..Default::default()
        };
        assert!(model_down(&dying).abs() < 0.2, "{}", model_down(&dying));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_stays_within_fifty_degrees_of_the_head() {
        let body = tick_head_turn(0.0, 0.0, 120.0, 50.0);
        assert!((body - 70.0).abs() < 1e-4, "{body}");
        let body = tick_head_turn(170.0, -170.0, -170.0, 50.0);
        assert!(body > 170.0, "{body}");
    }

    #[test]
    fn a_dead_entity_falls_on_its_side() {
        let state = LivingRenderState {
            death_time: 20.0,
            ..Default::default()
        };
        let m = root_transform(&state, Placement::default());
        let up = m.transform_vector3(Vec3::NEG_Y);
        assert!(up.y.abs() < 1e-4, "{up}");
    }

    #[test]
    fn a_swimmer_lies_flat_with_the_head_forward() {
        let state = LivingRenderState {
            swim: Swim {
                amount: 1.0,
                turn: SwimTurn::Avatar {
                    in_water: false,
                    visually_swimming: false,
                },
            },
            ..Default::default()
        };
        let m = root_transform(&state, Placement::default());
        let up = m.transform_vector3(Vec3::NEG_Y);
        assert!(up.y.abs() < 1e-4 && up.z < -0.99, "{up}");
        let tilted = LivingRenderState {
            x_rot: 30.0,
            swim: Swim {
                amount: 1.0,
                turn: SwimTurn::Avatar {
                    in_water: true,
                    visually_swimming: true,
                },
            },
            ..Default::default()
        };
        let up = root_transform(&tilted, Placement::default()).transform_vector3(Vec3::NEG_Y);
        assert!(up.y < -0.4 && up.z < -0.8, "{up}");
    }

    #[test]
    fn the_swim_amount_takes_eleven_ticks() {
        let mut amount = 0.0;
        for _ in 0..11 {
            amount = Swim::step(amount, true);
        }
        assert!(amount > 0.98);
        assert_eq!(Swim::step(1.0, true), 1.0);
        assert_eq!(Swim::step(0.05, false), 0.0);
    }
}
