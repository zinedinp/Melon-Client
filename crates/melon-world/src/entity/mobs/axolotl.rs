//! `AdultAxolotlModel` and `BabyAxolotlModel`.

use melon_core::math::angle::{HALF_PI as FRAC_PI_2, PI, QUARTER_PI as FRAC_PI_4};

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::animations::baby_axolotl as baby_anim;
use crate::entity::keyframe::AnimationDefinition;
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use melon_core::math::angle;
use crate::model_part::{Cube, Pose};

/// `Axolotl.Variant` names by id: lucy, wild, gold, cyan, blue.
pub fn texture(variant: i32, baby: bool) -> &'static str {
    const ADULT: [&str; 5] = [
        "minecraft:entity/axolotl/axolotl_lucy",
        "minecraft:entity/axolotl/axolotl_wild",
        "minecraft:entity/axolotl/axolotl_gold",
        "minecraft:entity/axolotl/axolotl_cyan",
        "minecraft:entity/axolotl/axolotl_blue",
    ];
    const BABY: [&str; 5] = [
        "minecraft:entity/axolotl/axolotl_lucy_baby",
        "minecraft:entity/axolotl/axolotl_wild_baby",
        "minecraft:entity/axolotl/axolotl_gold_baby",
        "minecraft:entity/axolotl/axolotl_cyan_baby",
        "minecraft:entity/axolotl/axolotl_blue_baby",
    ];
    let i = usize::try_from(variant)
        .ok()
        .filter(|&i| i < 5)
        .unwrap_or(0);
    if baby { BABY[i] } else { ADULT[i] }
}

/// The baby's animation slots (`PoseInput::anim`).
pub const SWIM: usize = 0;
pub const WALK: usize = 1;
pub const IDLE_FLOOR: usize = 2;
pub const IDLE_UNDERWATER_FLOOR: usize = 3;
pub const IDLE_UNDERWATER: usize = 4;
pub const PLAY_DEAD: usize = 5;
pub const BABY_SLOTS: usize = 6;

/// The baby's animations and the clock slot of each.
pub static BABY_ANIMATIONS: [(&AnimationDefinition, usize); 6] = [
    (&baby_anim::BABY_AXOLOTL_SWIM, SWIM),
    (&baby_anim::WALK_FLOOR_UNDERWATER, WALK),
    (&baby_anim::BABY_AXOLOTL_IDLE_FLOOR, IDLE_FLOOR),
    (&baby_anim::IDLE_FLOOR_UNDERWATER, IDLE_UNDERWATER_FLOOR),
    (&baby_anim::IDLE_UNDERWATER, IDLE_UNDERWATER),
    (&baby_anim::BABY_AXOLOTL_PLAY_DEAD, PLAY_DEAD),
];
pub static WALK_FLOOR: &AnimationDefinition = &baby_anim::AXOLOTL_WALK_FLOOR;

fn cube(uv: (f32, f32), origin: (f32, f32, f32), size: (f32, f32, f32)) -> Vec<Cube> {
    vec![Cube::new(uv, origin, size).grown(0.001)]
}

pub fn adult() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let body = r.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 11.0), (-4.0, -2.0, -9.0), (8.0, 4.0, 10.0)),
            Cube::new((2.0, 17.0), (0.0, -3.0, -8.0), (0.0, 5.0, 9.0)),
        ],
        Pose::offset(0.0, 19.5, 5.0),
    );
    let head = body.add_or_replace_child(
        "head",
        cube((0.0, 1.0), (-4.0, -3.0, -5.0), (8.0, 5.0, 5.0)),
        Pose::offset(0.0, 0.0, -9.0),
    );
    head.add_or_replace_child(
        "top_gills",
        cube((3.0, 37.0), (-4.0, -3.0, 0.0), (8.0, 3.0, 0.0)),
        Pose::offset(0.0, -3.0, -1.0),
    );
    head.add_or_replace_child(
        "left_gills",
        cube((0.0, 40.0), (-3.0, -5.0, 0.0), (3.0, 7.0, 0.0)),
        Pose::offset(-4.0, 0.0, -1.0),
    );
    head.add_or_replace_child(
        "right_gills",
        cube((11.0, 40.0), (0.0, -5.0, 0.0), (3.0, 7.0, 0.0)),
        Pose::offset(4.0, 0.0, -1.0),
    );
    let left = || cube((2.0, 13.0), (-1.0, 0.0, 0.0), (3.0, 5.0, 0.0));
    let right = || cube((2.0, 13.0), (-2.0, 0.0, 0.0), (3.0, 5.0, 0.0));
    body.add_or_replace_child("right_hind_leg", right(), Pose::offset(-3.5, 1.0, -1.0));
    body.add_or_replace_child("left_hind_leg", left(), Pose::offset(3.5, 1.0, -1.0));
    body.add_or_replace_child("right_front_leg", right(), Pose::offset(-3.5, 1.0, -8.0));
    body.add_or_replace_child("left_front_leg", left(), Pose::offset(3.5, 1.0, -8.0));
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((2.0, 19.0), (0.0, -3.0, 0.0), (0.0, 5.0, 12.0))],
        Pose::offset(0.0, 0.0, 1.0),
    );
    LayerDefinition::new(r, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let root = r.add_or_replace_child("root", vec![], Pose::offset(0.0, 24.0, 0.0));
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 0.0), (-2.0, -0.75, -2.75), (4.0, 2.0, 6.0)),
            Cube::new((0.0, 12.0), (0.0, -1.75, -2.75), (0.0, 3.0, 5.0)),
        ],
        Pose::offset(0.0, -1.25, 1.75),
    );
    body.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((20.0, 16.0), (-3.0, 0.0, -0.5), (3.0, 0.0, 1.0))],
        Pose::offset(-2.0, 0.25, -1.25),
    );
    let hind = body.add_or_replace_child(
        "right_hind_leg",
        vec![],
        Pose::offset_rotation(
            (-2.0, 0.25, 1.75),
            (
                0.0,
                melon_core::math::angle::HALF_PI,
                melon_core::math::angle::HALF_PI,
            ),
        ),
    );
    hind.add_or_replace_child(
        "right_leg_r1",
        vec![Cube::new((20.0, 14.0), (0.0, 0.0, -0.5), (3.0, 0.0, 1.0))],
        Pose::offset_rotation(
            (0.0, 0.0, 0.0),
            (
                -melon_core::math::angle::HALF_PI,
                0.0,
                melon_core::math::angle::HALF_PI,
            ),
        ),
    );
    body.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((20.0, 13.0), (0.0, 0.0, -0.5), (3.0, 0.0, 1.0))],
        Pose::offset(2.0, 0.25, -1.25),
    );
    body.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((20.0, 14.0), (0.0, 0.0, -0.5), (3.0, 0.0, 1.0))],
        Pose::offset(2.0, 0.25, 1.75),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((10.0, 9.0), (0.0, -1.5, -1.0), (0.0, 3.0, 8.0))],
        Pose::offset(0.0, -0.25, 3.25),
    );
    let head = body.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 8.0), (-3.0, -2.0, -4.0), (6.0, 3.0, 4.0))],
        Pose::offset(0.0, 0.25, -2.75),
    );
    head.add_or_replace_child(
        "left_gills",
        vec![Cube::new((20.0, 8.0), (0.0, -3.5, 0.0), (3.0, 5.0, 0.0))],
        Pose::offset(3.0, -0.5, -2.0),
    );
    head.add_or_replace_child(
        "right_gills",
        vec![Cube::new((20.0, 3.0), (-3.0, -3.5, 0.0), (3.0, 5.0, 0.0))],
        Pose::offset(-3.0, -0.5, -2.0),
    );
    head.add_or_replace_child(
        "top_gills",
        vec![Cube::new((20.0, 0.0), (-3.0, -3.0, 0.0), (6.0, 3.0, 0.0))],
        Pose::offset(0.0, -2.0, -2.0),
    );
    LayerDefinition::new(r, 32.0, 32.0)
}

/// `BinaryAnimator(10, IN_OUT_SINE)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Eased {
    ticks: i32,
    old: i32,
}

impl Eased {
    const LENGTH: i32 = 10;

    pub fn tick(&mut self, active: bool) {
        self.old = self.ticks;
        if active {
            if self.ticks < Self::LENGTH {
                self.ticks += 1;
            }
        } else if self.ticks > 0 {
            self.ticks -= 1;
        }
    }

    pub fn factor(&self, partial: f32) -> f32 {
        let linear =
            (self.old as f32 + (self.ticks - self.old) as f32 * partial) / Self::LENGTH as f32;
        -(angle::cos(PI * linear) - 1.0) / 2.0
    }
}

/// The four factors an adult is posed by.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Factors {
    pub playing_dead: f32,
    pub in_water: f32,
    pub on_ground: f32,
    pub moving: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Animators {
    pub playing_dead: Eased,
    pub in_water: Eased,
    pub on_ground: Eased,
    pub moving: Eased,
}

impl Animators {
    pub fn factors(&self, partial: f32) -> Factors {
        Factors {
            playing_dead: self.playing_dead.factor(partial),
            in_water: self.in_water.factor(partial),
            on_ground: self.on_ground.factor(partial),
            moving: self.moving.factor(partial),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AxolotlParts {
    body: PartId,
    head: PartId,
    tail: PartId,
    left_hind: PartId,
    right_hind: PartId,
    left_front: PartId,
    right_front: PartId,
    top_gills: PartId,
    left_gills: PartId,
    right_gills: PartId,
}

impl AxolotlParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
            head: model.part("head"),
            tail: model.part("tail"),
            left_hind: model.part("left_hind_leg"),
            right_hind: model.part("right_hind_leg"),
            left_front: model.part("left_front_leg"),
            right_front: model.part("right_front_leg"),
            top_gills: model.part("top_gills"),
            left_gills: model.part("left_gills"),
            right_gills: model.part("right_gills"),
        }
    }
}

/// `AdultAxolotlModel.setupAnim`.
pub fn setup_anim(p: &AxolotlParts, pose: &mut ModelPose, state: &LivingRenderState, f: Factors) {
    let age = state.age_in_ticks;
    let not_moving = 1.0 - f.moving;
    let mirrored = 1.0 - f.on_ground.min(f.moving);
    pose[p.body].pose.rotation.y += state.y_rot.to_radians();
    swimming(p, pose, age, state.x_rot, f.moving.min(f.in_water));
    hovering(p, pose, age, not_moving.min(f.in_water));
    crawling(p, pose, age, f.moving.min(f.on_ground));
    lying_still(p, pose, age, not_moving.min(f.on_ground));
    playing_dead(p, pose, f.playing_dead);
    mirror_legs(p, pose, mirrored);
}

fn add(pose: &mut ModelPose, part: PartId, x: f32, y: f32, z: f32) {
    let r = &mut pose[part].pose.rotation;
    r.x += x;
    r.y += y;
    r.z += z;
}

fn lying_still(p: &AxolotlParts, pose: &mut ModelPose, age: f32, factor: f32) {
    if factor <= 1.0e-5 {
        return;
    }
    let speed = age * 0.09;
    let (sin, cos) = (angle::sin(speed), angle::cos(speed));
    let movement = sin * sin - 2.0 * sin;
    let movement2 = cos * cos - 3.0 * sin;
    add(pose, p.head, -0.09 * movement * factor, 0.0, -0.2 * factor);
    add(pose, p.tail, 0.0, (-0.1 + 0.1 * movement) * factor, 0.0);
    let gill = (0.6 + 0.05 * movement2) * factor;
    add(pose, p.top_gills, gill, 0.0, 0.0);
    add(pose, p.left_gills, 0.0, -gill, 0.0);
    add(pose, p.right_gills, 0.0, gill, 0.0);
    add(pose, p.left_hind, 1.1 * factor, 1.0 * factor, 0.0);
    add(
        pose,
        p.left_front,
        0.8 * factor,
        2.3 * factor,
        -0.5 * factor,
    );
}

fn crawling(p: &AxolotlParts, pose: &mut ModelPose, age: f32, factor: f32) {
    if factor <= 1.0e-5 {
        return;
    }
    let speed = age * 0.11;
    let cos = angle::cos(speed);
    let hind_sway = (cos * cos - 2.0 * cos) / 5.0;
    let front_sway = 0.7 * cos;
    let head_tail = 0.09 * cos * factor;
    add(pose, p.head, 0.0, head_tail, 0.0);
    add(pose, p.tail, 0.0, head_tail, 0.0);
    let gill = (0.6 - 0.08 * (cos * cos + 2.0 * angle::sin(speed))) * factor;
    add(pose, p.top_gills, gill, 0.0, 0.0);
    add(pose, p.left_gills, 0.0, -gill, 0.0);
    add(pose, p.right_gills, 0.0, gill, 0.0);
    let (hind_x, front_x) = (0.942_477_9 * factor, 1.099_557_4 * factor);
    add(
        pose,
        p.left_hind,
        hind_x,
        (1.5 - hind_sway) * factor,
        -0.1 * factor,
    );
    add(
        pose,
        p.left_front,
        front_x,
        (FRAC_PI_2 - front_sway) * factor,
        0.0,
    );
    add(pose, p.right_hind, hind_x, (-1.0 - hind_sway) * factor, 0.0);
    add(
        pose,
        p.right_front,
        front_x,
        (-FRAC_PI_2 - front_sway) * factor,
        0.0,
    );
}

fn hovering(p: &AxolotlParts, pose: &mut ModelPose, age: f32, factor: f32) {
    if factor <= 1.0e-5 {
        return;
    }
    let speed = age * 0.075;
    let cos = angle::cos(speed);
    let sin = angle::sin(speed) * 0.15;
    let body_x = (-0.15 + 0.075 * cos) * factor;
    add(pose, p.body, body_x, 0.0, 0.0);
    pose[p.body].pose.offset.y -= sin * factor;
    add(pose, p.head, -body_x, 0.0, 0.0);
    add(pose, p.top_gills, 0.2 * cos * factor, 0.0, 0.0);
    let gill_y = (-0.3 * cos - 0.19) * factor;
    add(pose, p.left_gills, 0.0, gill_y, 0.0);
    add(pose, p.right_gills, 0.0, -gill_y, 0.0);
    add(
        pose,
        p.left_hind,
        (PI * 3.0 / 4.0 - cos * 0.11) * factor,
        0.471_238_94 * factor,
        1.727_876_1 * factor,
    );
    add(
        pose,
        p.left_front,
        (FRAC_PI_4 - cos * 0.2) * factor,
        2.042_035 * factor,
        0.0,
    );
    add(pose, p.tail, 0.0, 0.5 * cos * factor, 0.0);
}

fn swimming(p: &AxolotlParts, pose: &mut ModelPose, age: f32, x_rot: f32, factor: f32) {
    if factor <= 1.0e-5 {
        return;
    }
    let speed = age * 0.33;
    let (sin, cos) = (angle::sin(speed), angle::cos(speed));
    let sway = 0.13 * sin;
    add(pose, p.body, (x_rot.to_radians() + sway) * factor, 0.0, 0.0);
    add(pose, p.head, -sway * 1.8 * factor, 0.0, 0.0);
    pose[p.body].pose.offset.y -= 0.45 * cos * factor;
    add(pose, p.top_gills, (-0.5 * sin - 0.8) * factor, 0.0, 0.0);
    let gill_y = (0.3 * sin + 0.9) * factor;
    add(pose, p.left_gills, 0.0, gill_y, 0.0);
    add(pose, p.right_gills, 0.0, -gill_y, 0.0);
    add(
        pose,
        p.tail,
        0.0,
        0.3 * angle::cos(speed * 0.9) * factor,
        0.0,
    );
    add(
        pose,
        p.left_hind,
        1.884_955_6 * factor,
        -0.4 * sin * factor,
        FRAC_PI_2 * factor,
    );
    add(
        pose,
        p.left_front,
        1.884_955_6 * factor,
        (-0.2 * cos - 0.1) * factor,
        FRAC_PI_2 * factor,
    );
}

fn playing_dead(p: &AxolotlParts, pose: &mut ModelPose, factor: f32) {
    if factor <= 1.0e-5 {
        return;
    }
    add(
        pose,
        p.left_hind,
        1.413_716_7 * factor,
        1.099_557_4 * factor,
        FRAC_PI_4 * factor,
    );
    add(
        pose,
        p.left_front,
        FRAC_PI_4 * factor,
        2.042_035 * factor,
        0.0,
    );
    add(pose, p.body, -0.15 * factor, 0.0, 0.35 * factor);
}

fn mirror_legs(p: &AxolotlParts, pose: &mut ModelPose, factor: f32) {
    if factor <= 1.0e-5 {
        return;
    }
    let (lh, lf) = (
        pose[p.left_hind].pose.rotation,
        pose[p.left_front].pose.rotation,
    );
    add(
        pose,
        p.right_hind,
        lh.x * factor,
        -lh.y * factor,
        -lh.z * factor,
    );
    add(
        pose,
        p.right_front,
        lf.x * factor,
        -lf.y * factor,
        -lf.z * factor,
    );
}

/// `AxolotlRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    let texture = texture(a.axolotl_variant, baby);
    out.push(plain(
        if baby { M::AxolotlBaby } else { M::Axolotl },
        texture,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_factors_ease_over_ten_ticks() {
        let mut e = Eased::default();
        assert_eq!(e.factor(1.0), 0.0);
        for _ in 0..10 {
            e.tick(true);
        }
        assert!((e.factor(1.0) - 1.0).abs() < 1e-3);
        let mid = {
            let mut e = Eased::default();
            for _ in 0..5 {
                e.tick(true);
            }
            e.factor(1.0)
        };
        assert!((mid - 0.5).abs() < 0.01, "{mid}");
        e.tick(false);
        assert!(e.factor(1.0) < 1.0);
    }

    #[test]
    fn each_pose_changes_the_axolotl_and_the_legs_mirror() {
        let model = Model::bake(&adult());
        let parts = AxolotlParts::find(&model);
        let state = LivingRenderState {
            age_in_ticks: 11.0,
            x_rot: 20.0,
            ..Default::default()
        };
        let rest = model.initial_pose();
        for factors in [
            Factors {
                in_water: 1.0,
                moving: 1.0,
                ..Default::default()
            },
            Factors {
                in_water: 1.0,
                ..Default::default()
            },
            Factors {
                on_ground: 1.0,
                moving: 1.0,
                ..Default::default()
            },
            Factors {
                on_ground: 1.0,
                ..Default::default()
            },
            Factors {
                playing_dead: 1.0,
                in_water: 1.0,
                ..Default::default()
            },
        ] {
            let mut pose = model.initial_pose();
            setup_anim(&parts, &mut pose, &state, factors);
            assert_ne!(pose, rest, "{factors:?}");
        }
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &state,
            Factors {
                in_water: 1.0,
                moving: 1.0,
                ..Default::default()
            },
        );
        let (l, r) = (
            pose[parts.left_hind].pose.rotation,
            pose[parts.right_hind].pose.rotation,
        );
        assert!((l.x - r.x).abs() < 1e-5 && (l.y + r.y).abs() < 1e-5);
    }

    #[test]
    fn a_baby_bakes_with_its_animations() {
        use crate::entity::keyframe::KeyframeSet;
        let model = Model::bake(&baby());
        KeyframeSet::bake(&model, &BABY_ANIMATIONS).unwrap();
        crate::entity::keyframe::KeyframeAnimation::bake(&model, WALK_FLOOR).unwrap();
        assert!(texture(4, true).ends_with("blue_baby"));
        assert!(texture(99, false).ends_with("lucy"));
    }
}
