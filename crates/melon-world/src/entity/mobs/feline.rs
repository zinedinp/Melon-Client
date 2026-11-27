//! `AdultFelineModel` and `BabyFelineModel`.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M, dye_tint};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, PI};

/// `Mth.rotLerp` on angles in radians.
fn mix(t: f32, from: f32, to: f32) -> f32 {
    from + t * (to - from)
}

/// `AdultFelineModel.createBodyMesh(g)` at 64x32.
pub fn adult(g: f32) -> LayerDefinition {
    let tail_g = -0.02;
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-2.5, -2.0, -3.0), (5.0, 4.0, 5.0)).grown(g),
            Cube::new((0.0, 24.0), (-1.5, -0.001, -4.0), (3.0, 2.0, 2.0)).grown(g),
            Cube::new((0.0, 10.0), (-2.0, -3.0, 0.0), (1.0, 1.0, 2.0)).grown(g),
            Cube::new((6.0, 10.0), (1.0, -3.0, 0.0), (1.0, 1.0, 2.0)).grown(g),
        ],
        Pose::offset(0.0, 15.0, -9.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((20.0, 0.0), (-2.0, 3.0, -8.0), (4.0, 16.0, 6.0)).grown(g)],
        Pose::offset_rotation((0.0, 12.0, -10.0), (PI / 2.0, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "tail1",
        vec![Cube::new((0.0, 15.0), (-0.5, 0.0, 0.0), (1.0, 8.0, 1.0)).grown(g)],
        Pose::offset_rotation((0.0, 15.0, 8.0), (0.9, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "tail2",
        vec![Cube::new((4.0, 15.0), (-0.5, 0.0, 0.0), (1.0, 8.0, 1.0)).grown(tail_g)],
        Pose::offset(0.0, 20.0, 14.0),
    );
    let hind = || vec![Cube::new((8.0, 13.0), (-1.0, 0.0, 1.0), (2.0, 6.0, 2.0)).grown(g)];
    root.add_or_replace_child("left_hind_leg", hind(), Pose::offset(1.1, 18.0, 5.0));
    root.add_or_replace_child("right_hind_leg", hind(), Pose::offset(-1.1, 18.0, 5.0));
    let front = || vec![Cube::new((40.0, 0.0), (-1.0, 0.0, 0.0), (2.0, 10.0, 2.0)).grown(g)];
    root.add_or_replace_child("left_front_leg", front(), Pose::offset(1.2, 14.1, -5.0));
    root.add_or_replace_child("right_front_leg", front(), Pose::offset(-1.2, 14.1, -5.0));
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `BabyFelineModel.createBabyLayer`; the cat's collar is it scaled by 1.01.
pub fn baby() -> LayerDefinition {
    baby_grown(0.0)
}

/// [`baby`] with every box `g` pixels fuller (the cat's collar).
pub fn baby_grown(g: f32) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-2.5, -3.0, -2.875), (5.0, 4.0, 4.0)).grown(g),
            Cube::new((18.0, 0.0), (-2.0, -4.0, -0.875), (1.0, 1.0, 2.0)).grown(g),
            Cube::new((24.0, 0.0), (1.0, -4.0, -0.875), (1.0, 1.0, 2.0)).grown(g),
            Cube::new((18.0, 3.0), (-1.5, -1.0, -3.875), (3.0, 2.0, 1.0)).grown(g),
        ],
        Pose::offset(0.0, 20.0, -3.125),
    );
    for (name, uv, x, z) in [
        ("left_front_leg", (18.0, 18.0), 1.0, -1.5),
        ("right_front_leg", (12.0, 18.0), -1.0, -1.5),
        ("left_hind_leg", (18.0, 22.0), 1.0, 2.5),
        ("right_hind_leg", (12.0, 22.0), -1.0, 2.5),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-0.5, 0.0, -1.0), (1.0, 2.0, 2.0)).grown(g)],
            Pose::offset(x, 22.0, z),
        );
    }
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 8.0), (-2.0, -1.5, -3.5), (4.0, 3.0, 7.0)).grown(g)],
        Pose::offset(0.0, 20.5, 0.5),
    );
    root.add_or_replace_child(
        "tail1",
        vec![Cube::new((0.0, 18.0), (-0.5, -0.107, 0.0849), (1.0, 1.0, 5.0)).grown(g)],
        Pose::offset_rotation((0.0, 19.107, 3.9151), (-0.567_232, 0.0, 0.0)),
    );
    root.add_or_replace_child("tail2", Vec::new(), Pose::ZERO);
    LayerDefinition::new(root, 32.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct FelineParts {
    head: PartId,
    body: PartId,
    tail1: PartId,
    tail2: PartId,
    left_hind: PartId,
    right_hind: PartId,
    left_front: PartId,
    right_front: PartId,
    baby: bool,
}

impl FelineParts {
    pub fn find(model: &Model, baby: bool) -> Self {
        Self {
            head: model.part("head"),
            body: model.part("body"),
            tail1: model.part("tail1"),
            tail2: model.part("tail2"),
            left_hind: model.part("left_hind_leg"),
            right_hind: model.part("right_hind_leg"),
            left_front: model.part("left_front_leg"),
            right_front: model.part("right_front_leg"),
            baby,
        }
    }
}

/// `FelineRenderState`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FelinePose {
    pub crouching: bool,
    pub sprinting: bool,
    pub sitting: bool,
    pub lie_down: f32,
    pub lie_down_tail: f32,
    pub relax_one: f32,
}

/// `AdultFelineModel.setupAnim` and `BabyFelineModel.setupAnim`.
pub fn setup_anim(
    parts: &FelineParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    cat: FelinePose,
) {
    let s = if parts.baby { 0.5 } else { 1.0 };
    let (position, speed) = (living.walk_position, living.walk_speed);
    let tail_rest = 1.727_876_1;
    if cat.crouching {
        pose[parts.body].pose.offset.y += s;
        pose[parts.head].pose.offset.y += 2.0 * s;
        pose[parts.tail1].pose.offset.y += s;
        let tail2 = &mut pose[parts.tail2].pose;
        tail2.offset.y -= 4.0 * s;
        tail2.offset.z += 2.0 * s;
        pose[parts.tail1].pose.rotation.x = PI / 2.0;
        pose[parts.tail2].pose.rotation.x = PI / 2.0;
    } else if cat.sprinting {
        let y = pose[parts.tail1].pose.offset.y;
        let tail2 = &mut pose[parts.tail2].pose;
        tail2.offset.y = y;
        tail2.offset.z += 2.0 * s;
        pose[parts.tail1].pose.rotation.x = PI / 2.0;
        pose[parts.tail2].pose.rotation.x = PI / 2.0;
    }
    pose[parts.head].pose.rotation.x = living.x_rot.to_radians();
    pose[parts.head].pose.rotation.y = living.y_rot.to_radians();
    if !cat.sitting {
        if !parts.baby {
            pose[parts.body].pose.rotation.x = PI / 2.0;
        }
        let cos = |offset: f32| angle::cos(position * 0.6662 + offset) * speed;
        if cat.sprinting {
            pose[parts.left_hind].pose.rotation.x = cos(0.0);
            pose[parts.right_hind].pose.rotation.x = cos(0.3);
            pose[parts.left_front].pose.rotation.x = cos(PI + 0.3);
            pose[parts.right_front].pose.rotation.x = cos(PI);
            pose[parts.tail2].pose.rotation.x =
                tail_rest + PI / 10.0 * angle::cos(position) * speed;
        } else {
            pose[parts.left_hind].pose.rotation.x = cos(0.0);
            pose[parts.right_hind].pose.rotation.x = cos(PI);
            pose[parts.left_front].pose.rotation.x = cos(PI);
            pose[parts.right_front].pose.rotation.x = cos(0.0);
            let swing = if cat.crouching {
                0.471_238_94
            } else {
                PI / 4.0
            };
            pose[parts.tail2].pose.rotation.x = tail_rest + swing * angle::cos(position) * speed;
        }
    } else if parts.baby {
        sit_baby(parts, pose);
    } else {
        sit(parts, pose, s);
    }
    if cat.lie_down > 0.0 {
        if parts.baby {
            lie_baby(parts, pose, &cat);
        } else {
            lie(parts, pose, &cat, s);
        }
    }
    if cat.relax_one > 0.0 {
        let head = &mut pose[parts.head].pose.rotation.x;
        *head = mix(cat.relax_one, *head, -0.581_776_44);
    }
}

/// The adult's sitting pose.
fn sit(parts: &FelineParts, pose: &mut ModelPose, s: f32) {
    let body = &mut pose[parts.body].pose;
    body.rotation.x = PI / 4.0;
    body.offset.y -= 4.0 * s;
    body.offset.z += 5.0 * s;
    let head = &mut pose[parts.head].pose;
    head.offset.y -= 3.3 * s;
    head.offset.z += s;
    let tail1 = &mut pose[parts.tail1].pose;
    tail1.offset.y += 8.0 * s;
    tail1.offset.z -= 2.0 * s;
    tail1.rotation.x = 1.727_876_1;
    let tail2 = &mut pose[parts.tail2].pose;
    tail2.offset.y += 2.0 * s;
    tail2.offset.z -= 0.8 * s;
    tail2.rotation.x = 2.670_354;
    for leg in [parts.left_front, parts.right_front] {
        let leg = &mut pose[leg].pose;
        leg.rotation.x = -PI / 20.0;
        leg.offset.y += 2.0 * s;
        leg.offset.z -= 2.0 * s;
    }
    for leg in [parts.left_hind, parts.right_hind] {
        let leg = &mut pose[leg].pose;
        leg.rotation.x = -PI / 2.0;
        leg.offset.y += 3.0 * s;
        leg.offset.z -= 4.0 * s;
    }
}

/// The kitten's sitting pose.
fn sit_baby(parts: &FelineParts, pose: &mut ModelPose) {
    let body = &mut pose[parts.body].pose;
    body.rotation.x += -0.436_332_32;
    body.offset.y += 1.25;
    pose[parts.head].pose.offset.z += 0.75;
    let tail1 = &mut pose[parts.tail1].pose;
    tail1.rotation.x += 0.545_415_4;
    tail1.offset.y += 4.0;
    tail1.offset.z -= 0.9;
    pose[parts.left_hind].pose.offset.z -= 0.9;
    pose[parts.right_hind].pose.offset.z -= 0.9;
}

/// The adult lying on its side.
fn lie(parts: &FelineParts, pose: &mut ModelPose, cat: &FelinePose, s: f32) {
    let head = &mut pose[parts.head].pose.rotation;
    head.z = mix(cat.lie_down, head.z, -1.270_796_3);
    head.y = mix(cat.lie_down, head.y, 1.270_796_3);
    pose[parts.left_front].pose.rotation.x = -1.270_796_3;
    let right_front = &mut pose[parts.right_front].pose;
    right_front.rotation.x = -0.470_796_35;
    right_front.rotation.z = -0.2;
    right_front.offset.x += s;
    pose[parts.left_hind].pose.rotation.x = -0.4;
    let right_hind = &mut pose[parts.right_hind].pose;
    right_hind.rotation.x = 0.5;
    right_hind.rotation.z = -0.5;
    right_hind.offset.x += 0.8 * s;
    right_hind.offset.y += 2.0 * s;
    let tail1 = &mut pose[parts.tail1].pose.rotation.x;
    *tail1 = mix(cat.lie_down_tail, *tail1, 0.8);
    let tail2 = &mut pose[parts.tail2].pose.rotation.x;
    *tail2 = mix(cat.lie_down_tail, *tail2, -0.4);
}

/// The kitten lying on its side.
fn lie_baby(parts: &FelineParts, pose: &mut ModelPose, cat: &FelinePose) {
    pose[parts.body].pose.offset.x += 1.0;
    let head = &mut pose[parts.head].pose;
    head.rotation.x = mix(cat.lie_down, head.rotation.x, PI / 18.0);
    head.rotation.z = mix(cat.lie_down, head.rotation.z, -PI * 5.0 / 12.0);
    head.offset.x += 1.5;
    head.offset.y += 0.75;
    head.offset.z -= 0.5;
    let right_front = &mut pose[parts.right_front].pose;
    right_front.rotation.x = -PI / 4.0;
    right_front.offset.x += 3.5;
    right_front.offset.y -= 0.5;
    let left_front = &mut pose[parts.left_front].pose;
    left_front.rotation.x = -PI / 2.0;
    left_front.offset.x += 1.5;
    left_front.offset.y -= 1.0;
    left_front.offset.z -= 2.0;
    let right_hind = &mut pose[parts.right_hind].pose;
    right_hind.rotation.x = PI * 2.0 / 9.0;
    right_hind.rotation.y = PI / 9.0;
    right_hind.rotation.z = -PI / 9.0;
    right_hind.offset.x += 2.5;
    right_hind.offset.y -= 0.25;
    right_hind.offset.z += 0.5;
    let left_hind = &mut pose[parts.left_hind].pose;
    left_hind.offset.x += 1.5;
    left_hind.offset.z -= 1.0;
    let tail = &mut pose[parts.tail1].pose;
    tail.rotation.x += mix(cat.lie_down_tail, tail.rotation.x, -PI / 6.0);
    tail.rotation.y += mix(cat.lie_down_tail, tail.rotation.y, 0.0);
    tail.rotation.z += mix(cat.lie_down_tail, tail.rotation.z, -PI / 18.0);
    tail.offset.x += 1.0;
    tail.offset.y += 0.5;
    tail.offset.z -= 0.25;
}

/// `Cat.getLieDownAmount` and friends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Amounts {
    pub lie_down: f32,
    pub lie_down_tail: f32,
    pub relax_one: f32,
}

impl Amounts {
    pub fn step(&mut self, lying: bool, relaxing: bool) {
        if lying {
            self.lie_down = (self.lie_down + 0.15).min(1.0);
            self.lie_down_tail = (self.lie_down_tail + 0.08).min(1.0);
        } else {
            self.lie_down = (self.lie_down - 0.22).max(0.0);
            self.lie_down_tail = (self.lie_down_tail - 0.13).max(0.0);
        }
        self.relax_one = if relaxing {
            (self.relax_one + 0.1).min(1.0)
        } else {
            (self.relax_one - 0.13).max(0.0)
        };
    }

    /// Between `before` and `self` at `partial`.
    pub fn lerp(before: Self, now: Self, partial: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * partial;
        Self {
            lie_down: mix(before.lie_down, now.lie_down),
            lie_down_tail: mix(before.lie_down_tail, now.lie_down_tail),
            relax_one: mix(before.relax_one, now.relax_one),
        }
    }
}

/// `CatVariant` (registry order) by age.
const TEXTURES: [[&str; 2]; 11] = {
    macro_rules! cats {
        ($($v:literal),*) => { [$([
            concat!("minecraft:entity/cat/cat_", $v),
            concat!("minecraft:entity/cat/cat_", $v, "_baby"),
        ]),*] };
    }
    cats!(
        "all_black",
        "black",
        "british_shorthair",
        "calico",
        "jellie",
        "persian",
        "ragdoll",
        "red",
        "siamese",
        "tabby",
        "white"
    )
};

/// `CatRenderer` and `OcelotRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Cat => {
            let variant = usize::try_from(a.cat_variant)
                .ok()
                .filter(|&v| v < TEXTURES.len())
                .unwrap_or(9);
            out.push(plain(
                if baby { M::FelineBaby } else { M::Feline },
                TEXTURES[variant][usize::from(baby)],
            ));
            if a.tame() {
                out.push(
                    LayerSpec::aged(
                        baby,
                        (M::FelineCollar, "minecraft:entity/cat/cat_collar"),
                        (M::FelineBabyCollar, "minecraft:entity/cat/cat_collar_baby"),
                    )
                    .with_tint(dye_tint(a.collar_color())),
                );
            }
        }
        EntityKind::Ocelot => out.push(LayerSpec::aged(
            baby,
            (M::Feline, "minecraft:entity/cat/ocelot"),
            (M::FelineBaby, "minecraft:entity/cat/ocelot_baby"),
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sitting_cat_folds_its_hind_legs_and_a_walking_one_swings_them() {
        for (layer, baby) in [(adult(0.0), false), (baby(), true)] {
            let model = Model::bake(&layer);
            let parts = FelineParts::find(&model, baby);
            let mut sitting = model.initial_pose();
            setup_anim(
                &parts,
                &mut sitting,
                &LivingRenderState::default(),
                FelinePose {
                    sitting: true,
                    ..Default::default()
                },
            );
            let mut walking = model.initial_pose();
            let living = LivingRenderState {
                walk_position: 1.0,
                walk_speed: 1.0,
                ..Default::default()
            };
            setup_anim(&parts, &mut walking, &living, FelinePose::default());
            if !baby {
                assert_eq!(sitting[parts.left_hind].pose.rotation.x, -PI / 2.0);
            }
            assert!(walking[parts.left_hind].pose.rotation.x != 0.0);
        }
    }

    #[test]
    fn lying_down_builds_up_and_fades_away() {
        let mut amounts = Amounts::default();
        for _ in 0..10 {
            amounts.step(true, false);
        }
        assert_eq!(amounts.lie_down, 1.0);
        for _ in 0..10 {
            amounts.step(false, false);
        }
        assert_eq!(amounts, Amounts::default());
    }
}
