//! `BlazeModel`: a head and twelve rods circling it in three rings.

use super::{LayerSpec, ModelId as M};
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::entity::quadruped::look;
use melon_core::math::angle;
use crate::model_part::{Cube, Pose};

const RODS: usize = 12;

fn rod_name(i: usize) -> &'static str {
    const NAMES: [&str; RODS] = [
        "part0", "part1", "part2", "part3", "part4", "part5", "part6", "part7", "part8", "part9",
        "part10", "part11",
    ];
    NAMES[i]
}

/// The ring of rod `i` (0 upper, 1 middle, 2 lower) and where in it.
fn ring(i: usize) -> usize {
    i / 4
}

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
        Pose::ZERO,
    );
    let rod = || vec![Cube::new((0.0, 16.0), (0.0, 0.0, 0.0), (2.0, 8.0, 2.0))];
    for i in 0..RODS {
        let (angle, radius, y) = match ring(i) {
            0 => (
                i as f32 * melon_core::math::angle::HALF_PI,
                9.0,
                -2.0 + angle::cos(i as f32 * 2.0 * 0.25),
            ),
            1 => (
                melon_core::math::angle::QUARTER_PI + (i - 4) as f32 * melon_core::math::angle::HALF_PI,
                7.0,
                2.0 + angle::cos(i as f32 * 2.0 * 0.25),
            ),
            _ => (
                0.471_238_9 + (i - 8) as f32 * melon_core::math::angle::HALF_PI,
                5.0,
                11.0 + angle::cos(i as f32 * 1.5 * 0.5),
            ),
        };
        root.add_or_replace_child(
            rod_name(i),
            rod(),
            Pose::offset(angle::cos(angle) * radius, y, angle::sin(angle) * radius),
        );
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct BlazeParts {
    head: PartId,
    rods: [PartId; RODS],
}

impl BlazeParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            rods: std::array::from_fn(|i| model.part(rod_name(i))),
        }
    }
}

/// `BlazeModel.setupAnim`.
pub fn setup_anim(parts: &BlazeParts, pose: &mut ModelPose, state: &LivingRenderState) {
    let age = state.age_in_ticks;
    let pi = melon_core::math::angle::PI;
    for ring in 0..3 {
        let (mut turn, radius, base_y) = match ring {
            0 => (age * pi * -0.1, 9.0, -2.0),
            1 => (pi / 4.0 + age * pi * 0.03, 7.0, 2.0),
            _ => (0.471_238_9 + age * pi * -0.05, 5.0, 11.0),
        };
        for k in 0..4 {
            let i = ring * 4 + k;
            let bob = match ring {
                2 => angle::cos((i as f32 * 1.5 + age) * 0.5),
                _ => angle::cos((i as f32 * 2.0 + age) * 0.25),
            };
            let part = &mut pose[parts.rods[i]].pose;
            part.offset.y = base_y + bob;
            part.offset.x = angle::cos(turn) * radius;
            part.offset.z = angle::sin(turn) * radius;
            turn += pi / 2.0;
        }
    }
    look(&mut pose[parts.head].pose, state);
}

/// `BlazeRenderer`.
pub(super) fn push_layers(out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    out.push(plain(M::Blaze, "minecraft:entity/blaze/blaze"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rods_circle_and_the_rings_differ() {
        let model = Model::bake(&layer());
        let parts = BlazeParts::find(&model);
        let at = |age: f32| {
            let mut pose = model.initial_pose();
            let state = LivingRenderState {
                age_in_ticks: age,
                ..Default::default()
            };
            setup_anim(&parts, &mut pose, &state);
            pose
        };
        let (a, b) = (at(0.0), at(10.0));
        for rod in parts.rods {
            let radius = |p: &ModelPose| p[rod].pose.offset.x.hypot(p[rod].pose.offset.z);
            assert!((radius(&a) - radius(&b)).abs() < 0.05);
        }
        assert_ne!(a[parts.rods[0]].pose.offset, b[parts.rods[0]].pose.offset);
        let r = |i: usize| {
            a[parts.rods[i]]
                .pose
                .offset
                .x
                .hypot(a[parts.rods[i]].pose.offset.z)
        };
        assert!((r(0) - 9.0).abs() < 0.05 && (r(11) - 5.0).abs() < 0.05);
    }
}
