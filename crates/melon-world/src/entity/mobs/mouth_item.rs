//! An item carried in the mouth.

use glam::{Mat4, Vec3};

use melon_core::math::angle::Rotation;

/// Where a dolphin holds it.
pub fn dolphin(root: Mat4, x_rot: f32) -> Mat4 {
    let tilt = x_rot.abs() / 60.0;
    let (y, z) = if x_rot < 0.0 {
        (1.0 - tilt * 0.5, -1.0 + tilt * 0.5)
    } else {
        (1.0 + tilt * 0.8, -1.0 + tilt * 0.2)
    };
    root * Mat4::from_translation(Vec3::new(0.0, y, z))
}

/// How a fox holds it.
#[derive(Clone, Copy, Debug, Default)]
pub struct FoxMouth {
    /// The head part's position this frame, in blocks (`head.x / 16`...).
    pub head: Vec3,
    pub baby: bool,
    pub sleeping: bool,
    /// `FoxRenderState.headRollAngle`, radians.
    pub head_roll: f32,
    /// The head's turn and pitch, degrees.
    pub y_rot: f32,
    pub x_rot: f32,
}

/// `FoxHeldItemLayer`.
pub fn fox(root: Mat4, fox: FoxMouth) -> Mat4 {
    let mut at = root * Mat4::from_translation(fox.head);
    if fox.baby {
        at *= Mat4::from_scale(Vec3::splat(0.75));
    }
    at = at * Mat4::rot_z(fox.head_roll) * Mat4::rot_y_deg(fox.y_rot) * Mat4::rot_x_deg(fox.x_rot);
    let offset = match (fox.baby, fox.sleeping) {
        (true, true) => Vec3::new(0.4, 0.26, 0.15),
        (true, false) => Vec3::new(0.06, 0.26, -0.5),
        (false, true) => Vec3::new(0.46, 0.26, 0.22),
        (false, false) => Vec3::new(0.06, 0.27, -0.5),
    };
    at *= Mat4::from_translation(offset) * Mat4::rot_x_deg(90.0);
    if fox.sleeping {
        at *= Mat4::rot_z_deg(90.0);
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(m: Mat4) -> Vec3 {
        m.transform_point3(Vec3::ZERO)
    }

    #[test]
    fn a_dolphin_carries_it_a_block_ahead_and_lower_when_it_looks_down() {
        let level = at(dolphin(Mat4::IDENTITY, 0.0));
        assert_eq!(level, Vec3::new(0.0, 1.0, -1.0));
        let up = at(dolphin(Mat4::IDENTITY, -60.0));
        assert!((up - Vec3::new(0.0, 0.5, -0.5)).length() < 1e-6);
        let down = at(dolphin(Mat4::IDENTITY, 60.0));
        assert!((down - Vec3::new(0.0, 1.8, -0.8)).length() < 1e-6);
    }

    #[test]
    fn a_fox_holds_it_at_its_snout_and_a_baby_closer_in() {
        let head = Vec3::new(0.0, 0.5, -0.1);
        let adult = at(fox(
            Mat4::IDENTITY,
            FoxMouth {
                head,
                ..Default::default()
            },
        ));
        assert!((adult - (head + Vec3::new(0.06, 0.27, -0.5))).length() < 1e-6);
        let baby = at(fox(
            Mat4::IDENTITY,
            FoxMouth {
                head,
                baby: true,
                ..Default::default()
            },
        ));
        assert!((baby - (head + Vec3::new(0.06, 0.26, -0.5) * 0.75)).length() < 1e-6);
    }

    #[test]
    fn a_sleeping_fox_holds_it_beside_the_head() {
        let sleeping = at(fox(
            Mat4::IDENTITY,
            FoxMouth {
                sleeping: true,
                ..Default::default()
            },
        ));
        assert!((sleeping - Vec3::new(0.46, 0.26, 0.22)).length() < 1e-6);
    }
}
