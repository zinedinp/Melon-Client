//! `FirstPersonHandsAndItemsRenderer`'s transforms, in the camera's own space.

use glam::{Mat4, Vec3};

use crate::entity::player::Arm;
use crate::math::angle::{self, PI, Rotation};

fn invert(arm: Arm) -> f32 {
    match arm {
        Arm::Right => 1.0,
        Arm::Left => -1.0,
    }
}

fn rotate_x(degrees: f32) -> Mat4 {
    Mat4::rot_x_deg(degrees)
}

fn rotate_y(degrees: f32) -> Mat4 {
    Mat4::rot_y_deg(degrees)
}

fn rotate_z(degrees: f32) -> Mat4 {
    Mat4::rot_z_deg(degrees)
}

/// `renderPlayerArm`: the bare arm, punching forward as it swings.
pub fn player_arm(arm: Arm, lowered: f32, attack: f32) -> Mat4 {
    let invert = invert(arm);
    let sqrt_attack = attack.sqrt();
    let x_swing = -0.3 * angle::sin(sqrt_attack * PI);
    let y_swing = 0.4 * angle::sin(sqrt_attack * PI * 2.0);
    let z_swing = -0.4 * angle::sin(attack * PI);
    let z_swing_rotation = angle::sin(attack * attack * PI);
    let y_swing_rotation = angle::sin(sqrt_attack * PI);
    Mat4::from_translation(Vec3::new(
        invert * (x_swing + 0.640_000_05),
        y_swing - 0.6 + lowered * -0.6,
        z_swing - 0.719_999_97,
    )) * rotate_y(invert * 45.0)
        * rotate_y(invert * y_swing_rotation * 70.0)
        * rotate_z(invert * z_swing_rotation * -20.0)
        * Mat4::from_translation(Vec3::new(-invert, 3.6, 3.5))
        * rotate_z(invert * 120.0)
        * rotate_x(200.0)
        * rotate_y(invert * -135.0)
        * Mat4::from_translation(Vec3::new(invert * 5.6, 0.0, 0.0))
}

/// `applyItemArmTransform`: where an item sits in hand.
pub fn item_arm(arm: Arm, lowered: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(invert(arm) * 0.56, -0.52 + lowered * -0.6, -0.72))
}

/// `swingArm`: the held item swept forward and down.
pub fn swing_item(arm: Arm, attack: f32) -> Mat4 {
    let invert = invert(arm);
    let sqrt_attack = attack.sqrt();
    let position = Vec3::new(
        invert * -0.4 * angle::sin(sqrt_attack * PI),
        0.2 * angle::sin(sqrt_attack * PI * 2.0),
        -0.2 * angle::sin(attack * PI),
    );
    let y_swing_rotation = angle::sin(attack * attack * PI);
    let xz_swing_rotation = angle::sin(sqrt_attack * PI);
    Mat4::from_translation(position)
        * rotate_y(invert * (45.0 + y_swing_rotation * -20.0))
        * rotate_z(invert * xz_swing_rotation * -20.0)
        * rotate_x(xz_swing_rotation * -80.0)
        * rotate_y(invert * -45.0)
}

/// `applyEatTransform`.
pub fn eat(arm: Arm, remaining: i32, partial: f32, duration: i32) -> Mat4 {
    let invert = invert(arm);
    let usage = remaining as f32 - partial + 1.0;
    let scaled = usage / duration.max(1) as f32;
    let mut matrix = Mat4::IDENTITY;
    if scaled < 0.8 {
        let bob = (angle::cos(usage / 4.0 * PI) * 0.1).abs();
        matrix *= Mat4::from_translation(Vec3::new(0.0, bob, 0.0));
    }
    let jiggle = 1.0 - scaled.powf(27.0);
    matrix
        * Mat4::from_translation(Vec3::new(jiggle * 0.6 * invert, jiggle * -0.5, 0.0))
        * rotate_y(invert * jiggle * 90.0)
        * rotate_x(jiggle * 10.0)
        * rotate_z(invert * jiggle * 30.0)
}

/// `ItemUseAnimation.BOW` after `applyItemArmTransform`.
pub fn draw_bow(arm: Arm, remaining: i32, partial: f32, duration: i32) -> Mat4 {
    let invert = invert(arm);
    let held = duration as f32 - (remaining as f32 - partial + 1.0);
    let power = (held / 20.0).max(0.0);
    let power = ((power * power + power * 2.0) / 3.0).min(1.0);
    Mat4::from_translation(Vec3::new(invert * -0.278_568_2, 0.183_443_87, 0.157_315_31))
        * rotate_x(-13.935)
        * rotate_y(invert * 35.3)
        * rotate_z(invert * -9.785)
        * hold_shake(held, power)
        * Mat4::from_translation(Vec3::new(0.0, 0.0, power * 0.04))
        * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 + power * 0.2))
        * rotate_y(invert * -45.0)
}

/// `ItemUseAnimation.TRIDENT` after `applyItemArmTransform`.
pub fn draw_trident(arm: Arm, remaining: i32, partial: f32, duration: i32) -> Mat4 {
    let invert = invert(arm);
    let held = duration as f32 - (remaining as f32 - partial + 1.0);
    let power = (held / 10.0).clamp(0.0, 1.0);
    Mat4::from_translation(Vec3::new(invert * -0.5, 0.7, 0.1))
        * rotate_x(-55.0)
        * rotate_y(invert * 35.3)
        * rotate_z(invert * -9.785)
        * hold_shake(held, power)
        * Mat4::from_translation(Vec3::new(0.0, 0.0, power * 0.2))
        * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 + power * 0.2))
        * rotate_y(invert * -45.0)
}

/// The tremble of a bow or trident held at full draw.
fn hold_shake(held: f32, power: f32) -> Mat4 {
    if power <= 0.1 {
        return Mat4::IDENTITY;
    }
    let shake = angle::sin((held - 0.1) * 1.3) * (power - 0.1);
    Mat4::from_translation(Vec3::new(0.0, shake * 0.004, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_rest_the_arm_matches_the_old_fixed_chain() {
        let arm = player_arm(Arm::Right, 0.0, 0.0);
        let origin = arm.transform_point3(Vec3::ZERO);
        let fixed = Mat4::from_translation(Vec3::new(0.640_000_05, -0.6, -0.719_999_97))
            * rotate_y(45.0)
            * Mat4::from_translation(Vec3::new(-1.0, 3.6, 3.5))
            * rotate_z(120.0)
            * rotate_x(200.0)
            * rotate_y(-135.0)
            * Mat4::from_translation(Vec3::new(5.6, 0.0, 0.0));
        assert!(origin.distance(fixed.transform_point3(Vec3::ZERO)) < 1e-5);
    }

    #[test]
    fn a_swing_moves_the_item_and_comes_back() {
        let rest = swing_item(Arm::Right, 0.0);
        assert!(rest.abs_diff_eq(Mat4::IDENTITY, 1e-5));
        let mid = swing_item(Arm::Right, 0.25).transform_point3(Vec3::ZERO);
        assert!(mid.x < -0.2, "{mid}");
        let end = swing_item(Arm::Right, 1.0);
        assert!(end.transform_point3(Vec3::ZERO).length() < 1e-5);
    }

    #[test]
    fn the_left_hand_mirrors_the_right() {
        let right = item_arm(Arm::Right, 0.5).transform_point3(Vec3::ZERO);
        let left = item_arm(Arm::Left, 0.5).transform_point3(Vec3::ZERO);
        assert_eq!(right.x, -left.x);
        assert!((right.y - (-0.52 - 0.3)).abs() < 1e-6);
    }
}
