//! Leads between an entity and what holds it.

use glam::Vec3;

use melon_core::math::angle;
use crate::mesh::WorldVertex;

/// `LeashFeatureRenderer.LEASH_RENDER_STEPS`.
const STEPS: usize = 24;
/// `LeashFeatureRenderer.LEASH_WIDTH`.
const WIDTH: f32 = 0.05;

/// One lead to draw, in the world (`EntityRenderState.LeashState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lead {
    /// Where it is tied on the leashed entity.
    pub start: Vec3,
    /// Where the holder holds it.
    pub end: Vec3,
    /// Block and sky light (`0..=15`) at the leashed entity's eyes and at the holder.
    pub start_light: (u8, u8),
    pub end_light: (u8, u8),
}

/// `Vec3.yRot(-yaw)`.
fn turned(offset: Vec3, yaw: f32) -> Vec3 {
    let (sin, cos) = angle::unit(-yaw * angle::DEG_TO_RAD);
    Vec3::new(
        offset.x * cos + offset.z * sin,
        offset.y,
        offset.z * cos - offset.x * sin,
    )
}

/// `Leashable.getLeashOffset` turned with the body.
pub fn tied_at(feet: Vec3, body_yaw: f32, eye_height: f32, width: f32) -> Vec3 {
    feet + turned(Vec3::new(0.0, eye_height, width * 0.4), body_yaw)
}

/// `Player.getRopeHoldPosition` standing or crouching: by the main hand.
pub fn player_hold(
    feet: Vec3,
    body_yaw: f32,
    right_handed: bool,
    crouching: bool,
    height: f32,
) -> Vec3 {
    let x = if right_handed { -0.22 } else { 0.22 };
    let z = if crouching { -0.2 } else { 0.07 };
    feet + turned(Vec3::new(x, height - 1.0, z), body_yaw)
}

/// `LeashFenceKnotEntity.getRopeHoldPosition`.
pub fn knot_hold(pos: Vec3) -> Vec3 {
    pos + Vec3::Y * 0.2
}

/// `Entity.getRopeHoldPosition` for any other holder.
pub fn entity_hold(feet: Vec3, eye_height: f32) -> Vec3 {
    feet + Vec3::Y * (eye_height * 0.7)
}

impl Lead {
    /// `LeashFeatureRenderer.prepare` as quads, relative to `origin`.
    pub fn quads(&self, origin: Vec3) -> Vec<WorldVertex> {
        let d = self.end - self.start;
        let horizontal = (d.x * d.x + d.z * d.z).sqrt();
        let (dx_off, dz_off) = if horizontal > 1.0e-6 {
            let f = WIDTH / 2.0 / horizontal;
            (d.z * f, d.x * f)
        } else {
            (WIDTH / 2.0, 0.0)
        };
        let start = self.start - origin;
        let pair = |k: usize, fudge: f32, backwards: bool| {
            let progress = k as f32 / STEPS as f32;
            let lerp =
                |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * progress) as u8;
            let light = [
                lerp(self.start_light.0, self.end_light.0) * 16,
                lerp(self.start_light.1, self.end_light.1) * 16,
            ];
            let shade = if k % 2 == usize::from(backwards) {
                0.7
            } else {
                1.0
            };
            let color = [0.5 * shade, 0.4 * shade, 0.3 * shade];
            let y = if d.y > 0.0 {
                d.y * progress * progress
            } else {
                d.y - d.y * (1.0 - progress) * (1.0 - progress)
            };
            let along = start + Vec3::new(d.x * progress, y, d.z * progress);
            let vertex = |p: Vec3| {
                let mut v = WorldVertex::new(p.to_array(), [0.5, 0.5], color);
                v.light = light;
                v
            };
            [
                vertex(along + Vec3::new(-dx_off, fudge, dz_off)),
                vertex(along + Vec3::new(dx_off, WIDTH - fudge, -dz_off)),
            ]
        };
        let mut out = Vec::with_capacity(STEPS * 2 * 2 * 4);
        for (fudge, backwards) in [(WIDTH, false), (0.0, true)] {
            for k in 0..STEPS {
                let [a0, b0] = pair(k, fudge, backwards);
                let [a1, b1] = pair(k + 1, fudge, backwards);
                out.extend([a0, b0, b1, a1]);
                out.extend([a1, b1, b0, a0]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lead(start: Vec3, end: Vec3) -> Lead {
        Lead {
            start,
            end,
            start_light: (0, 15),
            end_light: (15, 15),
        }
    }

    #[test]
    fn the_ribbon_runs_from_the_mob_to_the_hand_and_sags() {
        let from = Vec3::new(0.0, 1.0, 0.0);
        let to = Vec3::new(4.0, 2.0, 0.0);
        let quads = lead(from, to).quads(Vec3::ZERO);
        assert_eq!(quads.len(), 24 * 2 * 2 * 4);
        let near = |p: Vec3, v: &WorldVertex| Vec3::from_array(v.pos).distance(p) < 0.06;
        assert!(near(from, &quads[0]), "{:?}", quads[0].pos);
        assert!(quads.iter().any(|v| near(to, v)));
        let middle: Vec<_> = quads
            .iter()
            .filter(|v| (v.pos[0] - 2.0).abs() < 0.01)
            .collect();
        assert!(!middle.is_empty());
        assert!(middle.iter().all(|v| v.pos[1] < 1.4), "{:?}", middle[0].pos);
    }

    #[test]
    fn light_goes_from_one_end_to_the_other() {
        let quads = lead(Vec3::ZERO, Vec3::new(0.0, 0.0, 3.0)).quads(Vec3::ZERO);
        assert_eq!(quads[0].light, [0, 240]);
        assert!(quads.iter().any(|v| v.light == [240, 240]));
    }

    #[test]
    fn a_lead_straight_up_still_has_width() {
        let quads = lead(Vec3::ZERO, Vec3::Y * 2.0).quads(Vec3::ZERO);
        assert!(quads.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
        let xs: Vec<f32> = quads.iter().map(|v| v.pos[0]).collect();
        assert!(xs.iter().any(|&x| x > 0.0) && xs.iter().any(|&x| x < 0.0));
    }

    #[test]
    fn the_hand_and_the_tie_turn_with_the_body() {
        let tie = tied_at(Vec3::ZERO, 0.0, 1.0, 1.0);
        assert!((tie - Vec3::new(0.0, 1.0, 0.4)).length() < 1e-5, "{tie}");
        let east = tied_at(Vec3::ZERO, -90.0, 1.0, 1.0);
        assert!((east - Vec3::new(0.4, 1.0, 0.0)).length() < 1e-5, "{east}");
        let hand = player_hold(Vec3::ZERO, 0.0, true, false, 1.8);
        assert!(
            (hand - Vec3::new(-0.22, 0.8, 0.07)).length() < 1e-5,
            "{hand}"
        );
    }
}
