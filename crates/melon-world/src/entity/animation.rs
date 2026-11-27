//! Vanilla `AnimationUtils` and the easing the models share: the arms' poses.

use crate::math::angle::{self, PI};
use crate::model_part::Pose;

/// `Ease.outQuart`.
pub fn out_quart(x: f32) -> f32 {
    let inverse = 1.0 - x;
    1.0 - inverse * inverse * inverse * inverse
}

/// `Mth.rotLerpRad`: from `from` toward `to` by `t`, the short way round.
pub fn rot_lerp_rad(t: f32, from: f32, to: f32) -> f32 {
    let mut diff = to - from;
    while diff < -PI {
        diff += 2.0 * PI;
    }
    while diff >= PI {
        diff -= 2.0 * PI;
    }
    from + t * diff
}

/// `AnimationUtils.bobModelPart`: the arms' slow idle sway.
pub fn bob_model_part(part: &mut Pose, age_in_ticks: f32, scale: f32) {
    part.rotation.z += scale * (angle::cos(age_in_ticks * 0.09) * 0.05 + 0.05);
    part.rotation.x += scale * (angle::sin(age_in_ticks * 0.067) * 0.05);
}

/// `AnimationUtils.bobArms`.
pub fn bob_arms(right_arm: &mut Pose, left_arm: &mut Pose, age_in_ticks: f32) {
    bob_model_part(right_arm, age_in_ticks, 1.0);
    bob_model_part(left_arm, age_in_ticks, -1.0);
}

/// `AnimationUtils.animateZombieArms`.
pub fn animate_zombie_arms(
    left_arm: &mut Pose,
    right_arm: &mut Pose,
    aggressive: bool,
    baby_holding: bool,
    swing: f32,
) {
    let raise = !baby_holding;
    let drop = if raise {
        -PI / if aggressive { 1.5 } else { 2.25 }
    } else {
        0.0
    };
    let attack_y = if raise { 1.0 } else { -1.0 } * angle::sin(swing * PI);
    let attack_x = angle::sin((1.0 - (1.0 - swing) * (1.0 - swing)) * PI);
    let x_rot = drop + attack_y * 1.2 - attack_x * 0.4;
    let y_rot = 0.1 - attack_y * 0.6;
    right_arm.rotation = glam::Vec3::new(x_rot, if raise { -y_rot } else { y_rot }, 0.0);
    left_arm.rotation = glam::Vec3::new(x_rot, if raise { y_rot } else { -y_rot }, 0.0);
}

/// `AnimationUtils.animateCrossbowHold`.
pub fn animate_crossbow_hold(
    right_arm: &mut Pose,
    left_arm: &mut Pose,
    head: (f32, f32),
    holding_in_right_arm: bool,
) {
    let (holding, shooting) = if holding_in_right_arm {
        (right_arm, left_arm)
    } else {
        (left_arm, right_arm)
    };
    let side = if holding_in_right_arm { 1.0 } else { -1.0 };
    holding.rotation.y = -0.3 * side + head.1;
    shooting.rotation.y = 0.6 * side + head.1;
    holding.rotation.x = -PI / 2.0 + head.0 + 0.1;
    shooting.rotation.x = -1.5 + head.0;
}

/// `AnimationUtils.animateCrossbowCharge`.
pub fn animate_crossbow_charge(
    right_arm: &mut Pose,
    left_arm: &mut Pose,
    max_duration: f32,
    ticks_using: f32,
    holding_in_right_arm: bool,
) {
    let (holding, pulling) = if holding_in_right_arm {
        (right_arm, left_arm)
    } else {
        (left_arm, right_arm)
    };
    let side = if holding_in_right_arm { 1.0 } else { -1.0 };
    holding.rotation.y = -0.8 * side;
    holding.rotation.x = -0.970_796_35;
    let alpha = ticks_using.clamp(0.0, max_duration) / max_duration;
    pulling.rotation.y = (0.4 + (0.85 - 0.4) * alpha) * side;
    pulling.rotation.x = -0.970_796_35 + (-PI / 2.0 + 0.970_796_35) * alpha;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crossbow_is_aimed_with_the_off_arm_across_and_pulled_back_as_it_charges() {
        let (mut l, mut r) = (Pose::ZERO, Pose::ZERO);
        animate_crossbow_hold(&mut r, &mut l, (0.0, 0.0), true);
        assert!((r.rotation.x + PI / 2.0 - 0.1).abs() < 1e-5 && l.rotation.x == -1.5);
        assert!(r.rotation.y < 0.0 && l.rotation.y > 0.0);
        animate_crossbow_charge(&mut r, &mut l, 25.0, 0.0, true);
        let start = l.rotation.x;
        animate_crossbow_charge(&mut r, &mut l, 25.0, 25.0, true);
        assert!(l.rotation.x < start && (l.rotation.x + PI / 2.0).abs() < 1e-5);
    }

    #[test]
    fn zombie_arms_reach_forward_higher_when_aggressive() {
        let (mut l, mut r) = (Pose::ZERO, Pose::ZERO);
        animate_zombie_arms(&mut l, &mut r, false, false, 0.0);
        let calm = r.rotation.x;
        animate_zombie_arms(&mut l, &mut r, true, false, 0.0);
        assert!(r.rotation.x < calm && calm < -1.0);
        assert_eq!(l.rotation.x, r.rotation.x);
        assert!((l.rotation.y + r.rotation.y).abs() < 1e-6);
    }
}
