//! `WolfModel` (`AdultWolfModel`, `BabyWolfModel`).

use super::rounded;
use super::{ClientEntity, LayerSpec, ModelId as M, dye_tint};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, PI};

/// `AdultWolfModel.createBodyLayer(NONE)` at 64x32.
pub fn adult() -> LayerDefinition {
    adult_grown(0.0)
}

/// `WOLF_ARMOR`: the adult grown by 0.2.
pub fn armor() -> LayerDefinition {
    adult_grown(0.2)
}

fn adult_grown(g: f32) -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child("head", Vec::new(), Pose::offset(-1.0, 13.5, -7.0));
    head.add_or_replace_child(
        "real_head",
        vec![
            Cube::new((0.0, 0.0), (-2.0, -3.0, -2.0), (6.0, 6.0, 4.0)).grown(g),
            Cube::new((16.0, 14.0), (-2.0, -5.0, 0.0), (2.0, 2.0, 1.0)).grown(g),
            Cube::new((16.0, 14.0), (2.0, -5.0, 0.0), (2.0, 2.0, 1.0)).grown(g),
            Cube::new((0.0, 10.0), (-0.5, -0.001, -5.0), (3.0, 3.0, 4.0)).grown(g),
        ],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((18.0, 14.0), (-3.0, -2.0, -3.0), (6.0, 9.0, 6.0)).grown(g)],
        Pose::offset_rotation((0.0, 14.0, 2.0), (PI / 2.0, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "upper_body",
        vec![Cube::new((21.0, 0.0), (-3.0, -3.0, -3.0), (8.0, 6.0, 7.0)).grown(g)],
        Pose::offset_rotation((-1.0, 14.0, -3.0), (PI / 2.0, 0.0, 0.0)),
    );
    let leg = |mirror: bool| {
        let cube = Cube::new((0.0, 18.0), (0.0, 0.0, -1.0), (2.0, 8.0, 2.0)).grown(g);
        vec![if mirror { cube.mirrored() } else { cube }]
    };
    root.add_or_replace_child("right_hind_leg", leg(true), Pose::offset(-2.5, 16.0, 7.0));
    root.add_or_replace_child("left_hind_leg", leg(false), Pose::offset(0.5, 16.0, 7.0));
    root.add_or_replace_child("right_front_leg", leg(true), Pose::offset(-2.5, 16.0, -4.0));
    root.add_or_replace_child("left_front_leg", leg(false), Pose::offset(0.5, 16.0, -4.0));
    let tail = root.add_or_replace_child(
        "tail",
        Vec::new(),
        Pose::offset_rotation((-1.0, 12.0, 8.0), (PI / 5.0, 0.0, 0.0)),
    );
    tail.add_or_replace_child(
        "real_tail",
        vec![Cube::new((9.0, 18.0), (0.0, 0.0, -1.0), (2.0, 8.0, 2.0)).grown(g)],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `BabyWolfModel.createBodyLayer`, with vanilla's rounded angles.
pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 12.0), (-2.99, -3.25, -3.0), (6.0, 5.0, 5.0)).grown(0.025),
            Cube::new((17.0, 12.0), (-1.5, -0.24, -5.0), (3.0, 2.0, 2.0)),
        ],
        Pose::offset(0.0, 18.25, -4.0),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((0.0, 5.0), (-1.0, -1.0, -0.5), (2.0, 2.0, 1.0))],
        Pose::offset(-2.0, -4.25, -0.5),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((20.0, 5.0), (-1.0, -1.0, -0.5), (2.0, 2.0, 1.0))],
        Pose::offset(2.0, -4.25, -0.5),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-3.0, -2.0, -4.0), (6.0, 4.0, 8.0))],
        Pose::offset(0.0, 19.0, 0.0),
    );
    for (name, uv, x, z) in [
        ("right_hind_leg", (0.0, 22.0), -1.5, 3.0),
        ("left_hind_leg", (8.0, 22.0), 1.5, 3.0),
        ("right_front_leg", (0.0, 0.0), -1.5, -3.0),
        ("left_front_leg", (20.0, 0.0), 1.5, -3.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
            Pose::offset(x, 21.0, z),
        );
    }
    let tail = root.add_or_replace_child(
        "tail",
        Vec::new(),
        Pose::offset_rotation((0.0, 19.0, 3.0), (-rounded::PI_6, 0.0, 0.0)),
    );
    tail.add_or_replace_child(
        "tail_r1",
        vec![Cube::new((22.0, 16.0), (-1.0, -5.7, -1.0), (2.0, 6.0, 2.0))],
        Pose::offset_rotation((0.0, -0.6, 0.2), (-3.1, 0.0, 0.0)),
    );
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct WolfParts {
    head: PartId,
    /// The head part that rolls: the adult's `real_head`, the baby's head.
    rolling_head: PartId,
    body: PartId,
    /// The adult's chest.
    upper_body: Option<PartId>,
    tail: PartId,
    right_hind: PartId,
    left_hind: PartId,
    right_front: PartId,
    left_front: PartId,
    baby: bool,
}

impl WolfParts {
    pub fn find(model: &Model, baby: bool) -> Self {
        let head = model.part("head");
        let tail = model.part("tail");
        Self {
            head,
            rolling_head: model.find("real_head").unwrap_or(head),
            body: model.part("body"),
            upper_body: model.find("upper_body"),
            tail,
            right_hind: model.part("right_hind_leg"),
            left_hind: model.part("left_hind_leg"),
            right_front: model.part("right_front_leg"),
            left_front: model.part("left_front_leg"),
            baby,
        }
    }
}

/// What a wolf's pose reads besides the walk: its mood and sitting.
#[derive(Clone, Copy, Debug)]
pub struct WolfPose {
    pub angry: bool,
    pub sitting: bool,
    /// `Wolf.getTailAngle`.
    pub tail_angle: f32,
    /// `Wolf.getHeadRollAngle`: the tilt of a begging head.
    pub head_roll: f32,
}

impl Default for WolfPose {
    fn default() -> Self {
        Self {
            angry: false,
            sitting: false,
            tail_angle: PI / 5.0,
            head_roll: 0.0,
        }
    }
}

/// `Wolf.getTailAngle`.
pub fn tail_angle(angry: bool, tame: bool, health: f32) -> f32 {
    if angry {
        1.539_380_4
    } else if tame {
        (0.55 - (TAME_MAX_HEALTH - health).max(0.0) * 0.02) * PI
    } else {
        PI / 5.0
    }
}

/// A tamed wolf's `Attributes.MAX_HEALTH`.
const TAME_MAX_HEALTH: f32 = 40.0;

/// `WolfModel.setupAnim` (the water shake aside).
pub fn setup_anim(
    parts: &WolfParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    wolf: WolfPose,
) {
    let age_scale = if parts.baby { 0.5 } else { 1.0 };
    let (position, speed) = (living.walk_position, living.walk_speed);
    pose[parts.tail].pose.rotation.y = if wolf.angry {
        0.0
    } else {
        angle::cos(position * 0.6662) * 1.4 * speed
    };
    if wolf.sitting {
        sit(parts, pose, age_scale);
    } else {
        let swing = |offset: f32| angle::cos(position * 0.6662 + offset) * 1.4 * speed;
        pose[parts.right_hind].pose.rotation.x = swing(0.0);
        pose[parts.left_hind].pose.rotation.x = swing(PI);
        pose[parts.right_front].pose.rotation.x = swing(PI);
        pose[parts.left_front].pose.rotation.x = swing(0.0);
    }
    pose[parts.rolling_head].pose.rotation.z = wolf.head_roll;
    pose[parts.head].pose.rotation.x = living.x_rot.to_radians();
    pose[parts.head].pose.rotation.y = living.y_rot.to_radians();
    pose[parts.tail].pose.rotation.x = wolf.tail_angle;
}

/// `WolfModel.setSittingPose` with the adult's and the baby's additions.
fn sit(parts: &WolfParts, pose: &mut ModelPose, s: f32) {
    let body = &mut pose[parts.body].pose;
    body.offset.y += 4.0 * s;
    body.offset.z -= 2.0 * s;
    body.rotation.x = PI / 4.0;
    let tail = &mut pose[parts.tail].pose;
    tail.offset.y += 9.0 * s;
    tail.offset.z -= 2.0 * s;
    for leg in [parts.right_hind, parts.left_hind] {
        let leg = &mut pose[leg].pose;
        leg.offset.y += 6.7 * s;
        leg.offset.z -= 5.0 * s;
        leg.rotation.x = PI * 3.0 / 2.0;
    }
    for (leg, side) in [(parts.right_front, 1.0), (parts.left_front, -1.0)] {
        let leg = &mut pose[leg].pose;
        leg.rotation.x = 5.811_947;
        leg.offset.x += 0.01 * s * side;
        leg.offset.y += s;
    }
    match parts.upper_body {
        Some(upper) => {
            let upper = &mut pose[upper].pose;
            upper.offset.y += 2.0;
            upper.rotation.x = PI * 2.0 / 5.0;
            upper.rotation.y = 0.0;
        }
        None => pose[parts.body].pose.rotation.x -= PI / 2.0,
    }
}

/// A wolf's textures by age and state: wild, tame, angry.
macro_rules! wolf_set {
    ($base:literal) => {
        [
            [
                concat!("minecraft:entity/wolf/", $base),
                concat!("minecraft:entity/wolf/", $base, "_tame"),
                concat!("minecraft:entity/wolf/", $base, "_angry"),
            ],
            [
                concat!("minecraft:entity/wolf/", $base, "_baby"),
                concat!("minecraft:entity/wolf/", $base, "_tame_baby"),
                concat!("minecraft:entity/wolf/", $base, "_angry_baby"),
            ],
        ]
    };
}

/// `WolfVariant` in registry order.
const TEXTURES: [[[&str; 3]; 2]; 9] = [
    wolf_set!("wolf_ashen"),
    wolf_set!("wolf_black"),
    wolf_set!("wolf_chestnut"),
    wolf_set!("wolf"),
    wolf_set!("wolf_rusty"),
    wolf_set!("wolf_snowy"),
    wolf_set!("wolf_spotted"),
    wolf_set!("wolf_striped"),
    wolf_set!("wolf_woods"),
];

/// `WolfRenderer` and `WolfCollarLayer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    let variant = usize::try_from(a.wolf_variant)
        .ok()
        .filter(|&v| v < TEXTURES.len())
        .unwrap_or(3);
    let state = if a.tame() {
        1
    } else if a.angry {
        2
    } else {
        0
    };
    let tints = TEXTURES[variant][usize::from(baby)][state];
    out.push(plain(if baby { M::WolfBaby } else { M::Wolf }, tints));
    if a.tame() {
        out.push(
            LayerSpec::aged(
                baby,
                (M::Wolf, "minecraft:entity/wolf/wolf_collar"),
                (M::WolfBaby, "minecraft:entity/wolf/wolf_collar_baby"),
            )
            .with_tint(dye_tint(a.collar_color())),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sitting_wolf_lowers_its_body_and_folds_its_hind_legs() {
        for (layer, baby) in [(adult(), false), (baby(), true)] {
            let model = Model::bake(&layer);
            let parts = WolfParts::find(&model, baby);
            let mut walking = model.initial_pose();
            let mut sitting = model.initial_pose();
            let living = LivingRenderState::default();
            setup_anim(&parts, &mut walking, &living, WolfPose::default());
            setup_anim(
                &parts,
                &mut sitting,
                &living,
                WolfPose {
                    sitting: true,
                    ..Default::default()
                },
            );
            assert!(sitting[parts.body].pose.offset.y > walking[parts.body].pose.offset.y);
            assert!((sitting[parts.right_hind].pose.rotation.x - PI * 1.5).abs() < 1e-5);
        }
    }

    #[test]
    fn the_tail_shows_the_mood() {
        assert!(tail_angle(true, false, 8.0) > tail_angle(false, false, 8.0));
        assert!(tail_angle(false, true, 40.0) > tail_angle(false, true, 10.0));
    }
}
