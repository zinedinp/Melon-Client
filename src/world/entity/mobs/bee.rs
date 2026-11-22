//! `AdultBeeModel` and `BabyBeeModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle::{self, DEG_TO_RAD, EIGHTH_PI, PI, QUARTER_PI};
use crate::world::entity::animation::rot_lerp_rad;
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// `Bee.FLAG_ROLL`.
pub const FLAG_ROLL: u8 = 2;
/// `Bee.FLAG_HAS_STUNG`.
pub const FLAG_HAS_STUNG: u8 = 4;
/// `Bee.FLAG_HAS_NECTAR`.
pub const FLAG_HAS_NECTAR: u8 = 8;

/// `BeeRenderState` as far as the pose and texture read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeePose {
    pub roll_amount: f32,
    /// `!Bee.hasStung()`.
    pub has_stinger: bool,
    pub on_ground: bool,
    pub angry: bool,
    pub nectar: bool,
}

impl Default for BeePose {
    fn default() -> Self {
        Self {
            roll_amount: 0.0,
            has_stinger: true,
            on_ground: false,
            angry: false,
            nectar: false,
        }
    }
}

/// `BeeRenderer.getTextureLocation`.
pub fn texture(bee: BeePose, baby: bool) -> &'static str {
    match (bee.angry, bee.nectar, baby) {
        (true, true, false) => "minecraft:entity/bee/bee_angry_nectar",
        (true, true, true) => "minecraft:entity/bee/bee_angry_nectar_baby",
        (true, false, false) => "minecraft:entity/bee/bee_angry",
        (true, false, true) => "minecraft:entity/bee/bee_angry_baby",
        (false, true, false) => "minecraft:entity/bee/bee_nectar",
        (false, true, true) => "minecraft:entity/bee/bee_nectar_baby",
        (false, false, false) => "minecraft:entity/bee/bee",
        (false, false, true) => "minecraft:entity/bee/bee_baby",
    }
}

/// `AdultBeeModel.createBodyLayer`.
pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let bone = root.add_or_replace_child("bone", vec![], Pose::offset(0.0, 19.0, 0.0));
    let body = bone.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-3.5, -4.0, -5.0), (7.0, 7.0, 10.0))],
        Pose::ZERO,
    );
    body.add_or_replace_child(
        "stinger",
        vec![Cube::new((26.0, 7.0), (0.0, -1.0, 5.0), (0.0, 1.0, 2.0))],
        Pose::ZERO,
    );
    body.add_or_replace_child(
        "left_antenna",
        vec![Cube::new((2.0, 0.0), (1.5, -2.0, -3.0), (1.0, 2.0, 3.0))],
        Pose::offset(0.0, -2.0, -5.0),
    );
    body.add_or_replace_child(
        "right_antenna",
        vec![Cube::new((2.0, 3.0), (-2.5, -2.0, -3.0), (1.0, 2.0, 3.0))],
        Pose::offset(0.0, -2.0, -5.0),
    );
    let wing = 0.001;
    bone.add_or_replace_child(
        "right_wing",
        vec![Cube::new((0.0, 18.0), (-9.0, 0.0, 0.0), (9.0, 0.0, 6.0)).grown(wing)],
        Pose::offset_rotation((-1.5, -4.0, -3.0), (0.0, -0.2618, 0.0)),
    );
    bone.add_or_replace_child(
        "left_wing",
        vec![
            Cube::new((0.0, 18.0), (0.0, 0.0, 0.0), (9.0, 0.0, 6.0))
                .grown(wing)
                .mirrored(),
        ],
        Pose::offset_rotation((1.5, -4.0, -3.0), (0.0, 0.2618, 0.0)),
    );
    let legs = |uv: (f32, f32)| vec![Cube::new(uv, (-5.0, 0.0, 0.0), (7.0, 2.0, 0.0))];
    bone.add_or_replace_child(
        "front_legs",
        legs((26.0, 1.0)),
        Pose::offset(1.5, 3.0, -2.0),
    );
    bone.add_or_replace_child(
        "middle_legs",
        legs((26.0, 3.0)),
        Pose::offset(1.5, 3.0, 0.0),
    );
    bone.add_or_replace_child("back_legs", legs((26.0, 5.0)), Pose::offset(1.5, 3.0, 2.0));
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyBeeModel.createBodyLayer`.
pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let bone = root.add_or_replace_child(
        "bone",
        vec![
            Cube::new((6.0, 12.0), (1.0, -1.6667, -2.1633), (1.0, 2.0, 2.0)),
            Cube::new((0.0, 12.0), (-2.0, -1.6667, -2.1933), (1.0, 2.0, 2.0)),
        ],
        Pose::offset(0.0, 19.6667, -1.8567),
    );
    let body = bone.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-2.0, -2.0, -2.5), (4.0, 4.0, 5.0))],
        Pose::offset(0.0, 1.3333, 2.3567),
    );
    body.add_or_replace_child(
        "stinger",
        vec![Cube::new((13.0, 2.0), (0.0, -0.5, 0.0), (0.0, 1.0, 1.0))],
        Pose::offset(0.0, 0.5, 2.5),
    );
    bone.add_or_replace_child(
        "right_wing",
        vec![Cube::new((3.0, 9.0), (-3.0, 0.0, 0.0), (3.0, 0.0, 3.0))],
        Pose::offset_rotation((-1.0, -0.6667, 0.8567), (0.2182, 0.3491, 0.0)),
    );
    bone.add_or_replace_child(
        "left_wing",
        vec![Cube::new((-3.0, 9.0), (0.0, 0.0, 0.0), (3.0, 0.0, 3.0)).mirrored()],
        Pose::offset_rotation((1.0, -0.6667, 0.8567), (0.2182, -0.3491, 0.0)),
    );
    let legs = |uv: (f32, f32)| vec![Cube::new(uv, (-1.5, 0.0, 0.0), (3.0, 1.0, 0.0))];
    bone.add_or_replace_child(
        "front_legs",
        legs((13.0, 0.0)),
        Pose::offset(0.0, 3.3333, 1.8567),
    );
    bone.add_or_replace_child(
        "middle_legs",
        legs((13.0, 1.0)),
        Pose::offset(0.0, 3.3333, 2.8567),
    );
    bone.add_or_replace_child(
        "back_legs",
        legs((13.0, 2.0)),
        Pose::offset(0.0, 3.3333, 3.8567),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct BeeParts {
    bone: PartId,
    stinger: PartId,
    right_wing: PartId,
    left_wing: PartId,
    front_legs: PartId,
    middle_legs: PartId,
    back_legs: PartId,
    left_antenna: Option<PartId>,
    right_antenna: Option<PartId>,
}

impl BeeParts {
    pub fn find(model: &Model) -> Self {
        Self {
            bone: model.part("bone"),
            stinger: model.part("stinger"),
            right_wing: model.part("right_wing"),
            left_wing: model.part("left_wing"),
            front_legs: model.part("front_legs"),
            middle_legs: model.part("middle_legs"),
            back_legs: model.part("back_legs"),
            left_antenna: model.find("left_antenna"),
            right_antenna: model.find("right_antenna"),
        }
    }
}

/// `BeeModel.setupAnim` (and the adult's antennae).
pub fn setup_anim(parts: &BeeParts, pose: &mut ModelPose, state: &LivingRenderState, bee: BeePose) {
    pose[parts.stinger].visible = bee.has_stinger;
    if !bee.on_ground {
        let speed = state.age_in_ticks * 120.32113 * DEG_TO_RAD;
        let right = &mut pose[parts.right_wing].pose.rotation;
        right.y = 0.0;
        right.z = angle::cos(speed) * PI * 0.15;
        let right_x = right.x;
        let right_y = right.y;
        let right_z = right.z;
        let left = &mut pose[parts.left_wing].pose.rotation;
        left.x = right_x;
        left.y = right_y;
        left.z = -right_z;
        pose[parts.front_legs].pose.rotation.x = QUARTER_PI;
        pose[parts.middle_legs].pose.rotation.x = QUARTER_PI;
        pose[parts.back_legs].pose.rotation.x = QUARTER_PI;
    }
    if !bee.angry && !bee.on_ground {
        let speed = angle::cos(state.age_in_ticks * 0.18);
        pose[parts.bone].pose.rotation.x = 0.1 + speed * PI * 0.025;
        pose[parts.bone].pose.offset.y -= angle::cos(state.age_in_ticks * 0.18) * 0.9;
        pose[parts.front_legs].pose.rotation.x = -speed * PI * 0.1 + EIGHTH_PI;
        pose[parts.back_legs].pose.rotation.x = -speed * PI * 0.05 + QUARTER_PI;
        let antenna = speed * PI * 0.03;
        if let Some(left) = parts.left_antenna {
            pose[left].pose.rotation.x = antenna;
        }
        if let Some(right) = parts.right_antenna {
            pose[right].pose.rotation.x = antenna;
        }
    }
    if bee.roll_amount > 0.0 {
        let bone = &mut pose[parts.bone].pose.rotation;
        bone.x = rot_lerp_rad(bee.roll_amount, bone.x, 3.0915928);
    }
}

/// `BeeRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    out.push(plain(
        if baby { M::BeeBaby } else { M::Bee },
        texture(
            BeePose {
                roll_amount: if a.bee_flags & FLAG_ROLL != 0 {
                    1.0
                } else {
                    0.0
                },
                has_stinger: a.bee_flags & FLAG_HAS_STUNG == 0,
                on_ground: entity.on_ground,
                angry: a.angry,
                nectar: a.bee_flags & FLAG_HAS_NECTAR != 0,
            },
            baby,
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_meshes_bake_and_a_stung_bee_hides_its_stinger() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = BeeParts::find(&model);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                BeePose::default(),
            );
            assert!(pose[parts.stinger].visible);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                BeePose {
                    has_stinger: false,
                    ..Default::default()
                },
            );
            assert!(!pose[parts.stinger].visible);
        }
    }

    #[test]
    fn a_flying_bee_flaps_and_a_rolling_one_tips() {
        let model = Model::bake(&adult());
        let parts = BeeParts::find(&model);
        let state = LivingRenderState {
            age_in_ticks: 0.0,
            ..Default::default()
        };
        let mut flying = model.initial_pose();
        setup_anim(&parts, &mut flying, &state, BeePose::default());
        assert!((flying[parts.right_wing].pose.rotation.y).abs() < 1e-6);
        assert!((flying[parts.right_wing].pose.rotation.z - PI * 0.15).abs() < 1e-5);
        assert_eq!(
            flying[parts.left_wing].pose.rotation.z,
            -flying[parts.right_wing].pose.rotation.z
        );
        assert!(parts.left_antenna.is_some());

        let mut ground = model.initial_pose();
        setup_anim(
            &parts,
            &mut ground,
            &state,
            BeePose {
                on_ground: true,
                ..Default::default()
            },
        );
        assert!((ground[parts.right_wing].pose.rotation.y + 0.2618).abs() < 1e-5);

        let mut rolling = model.initial_pose();
        setup_anim(
            &parts,
            &mut rolling,
            &state,
            BeePose {
                on_ground: true,
                roll_amount: 1.0,
                ..Default::default()
            },
        );
        assert!((rolling[parts.bone].pose.rotation.x - 3.0915928).abs() < 1e-5);
    }

    #[test]
    fn nectar_and_anger_pick_the_texture() {
        let idle = BeePose::default();
        assert_eq!(texture(idle, false), "minecraft:entity/bee/bee");
        assert_eq!(texture(idle, true), "minecraft:entity/bee/bee_baby");
        let nectar = BeePose {
            nectar: true,
            ..idle
        };
        assert_eq!(texture(nectar, false), "minecraft:entity/bee/bee_nectar");
        let angry = BeePose {
            angry: true,
            nectar: true,
            ..idle
        };
        assert_eq!(
            texture(angry, true),
            "minecraft:entity/bee/bee_angry_nectar_baby"
        );
        assert_eq!(FLAG_ROLL | FLAG_HAS_STUNG | FLAG_HAS_NECTAR, 14);
    }
}
