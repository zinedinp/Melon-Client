//! `PandaModel` and `BabyPandaModel`.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::entity::animation::rot_lerp_rad;
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::{self, QuadrupedParts};
use crate::model_part::{Cube, Pose};
use melon_core::math::angle::{self, HALF_PI, PI, QUARTER_PI};

/// `Panda.FLAG_*` on `DATA_ID_FLAGS`.
pub const FLAG_SNEEZE: u8 = 2;
pub const FLAG_ROLL: u8 = 4;
pub const FLAG_SIT: u8 = 8;
pub const FLAG_ON_BACK: u8 = 16;
pub const FLAG_UNHAPPY: u8 = 32;
pub const FLAG_SCARED: u8 = 64;

/// `Panda.Gene` ids: normal, lazy, worried, playful, brown, weak, aggressive.
pub fn texture(gene: i32, baby: bool) -> &'static str {
    const ADULT: [&str; 7] = [
        "minecraft:entity/panda/panda",
        "minecraft:entity/panda/panda_lazy",
        "minecraft:entity/panda/panda_worried",
        "minecraft:entity/panda/panda_playful",
        "minecraft:entity/panda/panda_brown",
        "minecraft:entity/panda/panda_weak",
        "minecraft:entity/panda/panda_aggressive",
    ];
    const BABY: [&str; 7] = [
        "minecraft:entity/panda/panda_baby",
        "minecraft:entity/panda/lazy_panda_baby",
        "minecraft:entity/panda/worried_panda_baby",
        "minecraft:entity/panda/playful_panda_baby",
        "minecraft:entity/panda/brown_panda_baby",
        "minecraft:entity/panda/weak_panda_baby",
        "minecraft:entity/panda/aggressive_panda_baby",
    ];
    let i = usize::try_from(gene).ok().filter(|&i| i < 7).unwrap_or(0);
    if baby { BABY[i] } else { ADULT[i] }
}

pub fn adult() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 6.0), (-6.5, -5.0, -4.0), (13.0, 10.0, 9.0)),
            Cube::new((45.0, 16.0), (-3.5, 0.0, -6.0), (7.0, 5.0, 2.0)),
            Cube::new((52.0, 25.0), (3.5, -8.0, -1.0), (5.0, 4.0, 1.0)),
            Cube::new((52.0, 25.0), (-8.5, -8.0, -1.0), (5.0, 4.0, 1.0)),
        ],
        Pose::offset(0.0, 11.5, -17.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new(
            (0.0, 25.0),
            (-9.5, -13.0, -6.5),
            (19.0, 26.0, 13.0),
        )],
        Pose::offset_rotation((0.0, 10.0, 0.0), (HALF_PI, 0.0, 0.0)),
    );
    let leg = || vec![Cube::new((40.0, 0.0), (-3.0, 0.0, -3.0), (6.0, 9.0, 6.0))];
    root.add_or_replace_child("right_hind_leg", leg(), Pose::offset(-5.5, 15.0, 9.0));
    root.add_or_replace_child("left_hind_leg", leg(), Pose::offset(5.5, 15.0, 9.0));
    root.add_or_replace_child("right_front_leg", leg(), Pose::offset(-5.5, 15.0, -9.0));
    root.add_or_replace_child("left_front_leg", leg(), Pose::offset(5.5, 15.0, -9.0));
    LayerDefinition::new(root, 64.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 11.0), (-4.5, -3.5, -5.5), (9.0, 7.0, 11.0))],
        Pose::offset(0.0, 18.5, 2.5),
    );
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-3.5, -3.0, -5.0), (7.0, 6.0, 5.0)),
            Cube::new((24.0, 6.0), (-2.0, 1.0, -6.0), (4.0, 2.0, 1.0)),
            Cube::new((24.0, 0.0), (-4.5, -4.0, -3.5), (3.0, 3.0, 1.0)),
            Cube::new((33.0, 0.0), (1.5, -4.0, -3.5), (3.0, 3.0, 1.0)),
        ],
        Pose::offset(0.0, 19.0, -3.0),
    );
    root.add_or_replace_child(
        "right_hind_leg",
        vec![Cube::new((0.0, 34.0), (-1.5, 0.0, -1.5), (3.0, 2.0, 3.0))],
        Pose::offset(-3.0, 22.0, 6.5),
    );
    root.add_or_replace_child(
        "left_hind_leg",
        vec![Cube::new((12.0, 34.0), (-1.5, 0.0, -1.5), (3.0, 2.0, 3.0))],
        Pose::offset(3.0, 22.0, 6.5),
    );
    root.add_or_replace_child(
        "right_front_leg",
        vec![Cube::new((0.0, 29.0), (-1.5, 0.0, -1.5), (3.0, 2.0, 3.0))],
        Pose::offset(-3.0, 22.0, -1.5),
    );
    root.add_or_replace_child(
        "left_front_leg",
        vec![Cube::new((12.0, 29.0), (-1.5, 0.0, -1.5), (3.0, 2.0, 3.0))],
        Pose::offset(3.0, 22.0, -1.5),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct PandaParts {
    quadruped: QuadrupedParts,
    body: PartId,
    baby: bool,
}

impl PandaParts {
    pub fn find(model: &Model) -> Self {
        let body = model.part("body");
        let baby = model.initial_pose()[body].pose.rotation.x.abs() < 0.1;
        Self {
            quadruped: QuadrupedParts::find(model),
            body,
            baby,
        }
    }
}

/// `PandaRenderState` as far as the pose reads it.
#[derive(Clone, Copy, Debug, Default)]
pub struct PandaPose {
    pub unhappy: bool,
    pub sneezing: bool,
    /// `Panda.getSneezeCounter`.
    pub sneeze_time: i32,
    pub eating: bool,
    pub scared: bool,
    /// `Panda.getSitAmount`.
    pub sit_amount: f32,
    /// `Panda.getLieOnBackAmount`.
    pub lie_on_back_amount: f32,
    /// `Panda.getRollAmount` (0 for a cub).
    pub roll_amount: f32,
}

/// Sit, lie and roll as 0 or 1 from the flags.
pub fn pose_from_flags(flags: u8) -> PandaPose {
    PandaPose {
        unhappy: flags & FLAG_UNHAPPY != 0,
        sneezing: flags & FLAG_SNEEZE != 0,
        sneeze_time: if flags & FLAG_SNEEZE != 0 { 10 } else { 0 },
        eating: false,
        scared: flags & FLAG_SCARED != 0,
        sit_amount: if flags & FLAG_SIT != 0 { 1.0 } else { 0.0 },
        lie_on_back_amount: if flags & FLAG_ON_BACK != 0 { 1.0 } else { 0.0 },
        roll_amount: if flags & FLAG_ROLL != 0 { 1.0 } else { 0.0 },
    }
}

fn lerp(t: f32, from: f32, to: f32) -> f32 {
    from + t * (to - from)
}

/// `PandaModel.setupAnim`.
pub fn setup_anim(
    parts: &PandaParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    panda: PandaPose,
) {
    quadruped::setup_anim(&parts.quadruped, pose, living);
    let head = parts.quadruped.head;
    let legs = parts.quadruped.legs;
    if panda.unhappy {
        let wiggle = 0.35 * angle::sin(0.6 * living.age_in_ticks);
        pose[head].pose.rotation.y = wiggle;
        pose[head].pose.rotation.z = wiggle;
        pose[legs.right_front].pose.rotation.x = -0.75 * angle::sin(0.3 * living.age_in_ticks);
        pose[legs.left_front].pose.rotation.x = 0.75 * angle::sin(0.3 * living.age_in_ticks);
    } else {
        pose[head].pose.rotation.z = 0.0;
    }

    if panda.sneezing {
        if panda.sneeze_time < 15 {
            pose[head].pose.rotation.x = (-QUARTER_PI) * panda.sneeze_time as f32 / 14.0;
        } else if panda.sneeze_time < 20 {
            let internal = (panda.sneeze_time as f32 - 15.0) / 5.0;
            pose[head].pose.rotation.x = -QUARTER_PI + QUARTER_PI * internal;
        }
    }

    if panda.sit_amount > 0.0 {
        animate_sitting(parts, pose, panda.sit_amount);
        if panda.eating {
            pose[head].pose.rotation.x = HALF_PI + 0.2 * angle::sin(living.age_in_ticks * 0.6);
            pose[legs.right_front].pose.rotation.x =
                -0.4 - 0.2 * angle::sin(living.age_in_ticks * 0.6);
            pose[legs.left_front].pose.rotation.x =
                -0.4 - 0.2 * angle::sin(living.age_in_ticks * 0.6);
        }
        if panda.scared {
            pose[head].pose.rotation.x = 2.1707964;
            pose[legs.right_front].pose.rotation.x = -0.9;
            pose[legs.left_front].pose.rotation.x = -0.9;
        }
    } else {
        pose[legs.right_hind].pose.rotation.z = 0.0;
        pose[legs.left_hind].pose.rotation.z = 0.0;
        pose[legs.right_front].pose.rotation.z = 0.0;
        pose[legs.left_front].pose.rotation.z = 0.0;
    }

    if panda.lie_on_back_amount > 0.0 {
        pose[legs.right_hind].pose.rotation.x = -0.6 * angle::sin(living.age_in_ticks * 0.15);
        pose[legs.left_hind].pose.rotation.x = 0.6 * angle::sin(living.age_in_ticks * 0.15);
        pose[legs.right_front].pose.rotation.x = 0.3 * angle::sin(living.age_in_ticks * 0.25);
        pose[legs.left_front].pose.rotation.x = -0.3 * angle::sin(living.age_in_ticks * 0.25);
        pose[head].pose.rotation.x = rot_lerp_rad(
            panda.lie_on_back_amount,
            pose[head].pose.rotation.x,
            HALF_PI,
        );
    }

    if panda.roll_amount > 0.0 {
        pose[head].pose.rotation.x =
            rot_lerp_rad(panda.roll_amount, pose[head].pose.rotation.x, 2.0561945);
        pose[legs.right_hind].pose.rotation.x = -0.5 * angle::sin(living.age_in_ticks * 0.5);
        pose[legs.left_hind].pose.rotation.x = 0.5 * angle::sin(living.age_in_ticks * 0.5);
        pose[legs.right_front].pose.rotation.x = 0.5 * angle::sin(living.age_in_ticks * 0.5);
        pose[legs.left_front].pose.rotation.x = -0.5 * angle::sin(living.age_in_ticks * 0.5);
    }
}

/// `PandaModel.animateSitting` / `BabyPandaModel.animateSitting`.
fn animate_sitting(parts: &PandaParts, pose: &mut ModelPose, sit: f32) {
    let head = parts.quadruped.head;
    let legs = parts.quadruped.legs;
    if parts.baby {
        pose[parts.body].pose.rotation.x =
            rot_lerp_rad(sit, pose[parts.body].pose.rotation.x, PI / 18.0);
        pose[parts.body].pose.offset.z = lerp(sit, pose[parts.body].pose.offset.z, -1.5);
        pose[head].pose.offset.z = lerp(sit, pose[head].pose.offset.z, -11.5);
        pose[head].pose.offset.y = lerp(sit, pose[head].pose.offset.y, 17.5);
        pose[legs.right_front].pose.offset.z =
            lerp(sit, pose[legs.right_front].pose.offset.z, -5.0);
        pose[legs.left_front].pose.offset.z = lerp(sit, pose[legs.left_front].pose.offset.z, -5.0);
        pose[legs.right_hind].pose.offset.z = lerp(sit, pose[legs.right_hind].pose.offset.z, 3.0);
        pose[legs.left_hind].pose.offset.z = lerp(sit, pose[legs.left_hind].pose.offset.z, 3.0);
    } else {
        pose[parts.body].pose.rotation.x =
            rot_lerp_rad(sit, pose[parts.body].pose.rotation.x, 1.7407963);
        pose[head].pose.rotation.x = rot_lerp_rad(sit, pose[head].pose.rotation.x, HALF_PI);
    }
    pose[legs.right_front].pose.rotation.z = -0.27079642;
    pose[legs.left_front].pose.rotation.z = 0.27079642;
    pose[legs.right_hind].pose.rotation.z = 0.5707964;
    pose[legs.left_hind].pose.rotation.z = -0.5707964;
}

/// `PandaRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    out.push(plain(
        if baby { M::PandaBaby } else { M::Panda },
        texture(i32::from(a.panda_gene), baby),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_models_bake_and_sitting_spreads_the_legs() {
        for (layer, baby) in [(adult(), false), (baby(), true)] {
            let model = Model::bake(&layer);
            let parts = PandaParts::find(&model);
            assert_eq!(parts.baby, baby);
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &LivingRenderState::default(),
                PandaPose {
                    sit_amount: 1.0,
                    ..Default::default()
                },
            );
            assert!(
                (pose[parts.quadruped.legs.right_front].pose.rotation.z + 0.27079642).abs() < 1e-5
            );
        }
        assert_eq!(texture(4, false), "minecraft:entity/panda/panda_brown");
        assert_eq!(texture(1, true), "minecraft:entity/panda/lazy_panda_baby");
    }
}
