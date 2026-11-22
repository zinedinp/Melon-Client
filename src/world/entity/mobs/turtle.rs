//! `AdultTurtleModel` and `BabyTurtleModel`.

use crate::math::angle::HALF_PI as FRAC_PI_2;

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::math::angle;
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::{self, QuadrupedParts};
use crate::world::model_part::{Cube, Pose};

pub fn adult() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let one = |uv: (f32, f32), origin: (f32, f32, f32), size: (f32, f32, f32)| {
        vec![Cube::new(uv, origin, size)]
    };
    r.add_or_replace_child(
        "head",
        one((3.0, 0.0), (-3.0, -1.0, -3.0), (6.0, 5.0, 6.0)),
        Pose::offset(0.0, 19.0, -10.0),
    );
    r.add_or_replace_child(
        "body",
        vec![
            Cube::new((7.0, 37.0), (-9.5, 3.0, -10.0), (19.0, 20.0, 6.0)),
            Cube::new((31.0, 1.0), (-5.5, 3.0, -13.0), (11.0, 18.0, 3.0)),
        ],
        Pose::offset_rotation((0.0, 11.0, -10.0), (FRAC_PI_2, 0.0, 0.0)),
    );
    r.add_or_replace_child(
        "egg_belly",
        one((70.0, 33.0), (-4.5, 3.0, -14.0), (9.0, 18.0, 1.0)),
        Pose::offset_rotation((0.0, 11.0, -10.0), (FRAC_PI_2, 0.0, 0.0)),
    );
    r.add_or_replace_child(
        "right_hind_leg",
        one((1.0, 23.0), (-2.0, 0.0, 0.0), (4.0, 1.0, 10.0)),
        Pose::offset(-3.5, 22.0, 11.0),
    );
    r.add_or_replace_child(
        "left_hind_leg",
        one((1.0, 12.0), (-2.0, 0.0, 0.0), (4.0, 1.0, 10.0)),
        Pose::offset(3.5, 22.0, 11.0),
    );
    r.add_or_replace_child(
        "right_front_leg",
        one((27.0, 30.0), (-13.0, 0.0, -2.0), (13.0, 1.0, 5.0)),
        Pose::offset(-5.0, 21.0, -4.0),
    );
    r.add_or_replace_child(
        "left_front_leg",
        one((27.0, 24.0), (0.0, 0.0, -2.0), (13.0, 1.0, 5.0)),
        Pose::offset(5.0, 21.0, -4.0),
    );
    LayerDefinition::new(r, 128.0, 64.0)
}

pub fn baby() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let one = |uv: (f32, f32), origin: (f32, f32, f32), size: (f32, f32, f32)| {
        vec![Cube::new(uv, origin, size)]
    };
    r.add_or_replace_child(
        "body",
        one((0.0, 0.0), (-2.0, -1.0, -2.0), (4.0, 2.0, 4.0)),
        Pose::offset(0.0, 22.9, 1.0),
    );
    r.add_or_replace_child(
        "head",
        one((0.0, 6.0), (-1.5, -2.0, -3.0), (3.0, 3.0, 3.0)),
        Pose::offset(0.0, 22.9, -1.0),
    );
    r.add_or_replace_child(
        "right_hind_leg",
        one((-1.0, 0.0), (-2.0, 0.0, -0.5), (2.0, 0.0, 1.0)),
        Pose::offset(-2.0, 23.9, 2.5),
    );
    r.add_or_replace_child(
        "left_hind_leg",
        one((-1.0, 1.0), (0.0, 0.0, -0.5), (2.0, 0.0, 1.0)),
        Pose::offset(2.0, 23.9, 2.5),
    );
    r.add_or_replace_child(
        "right_front_leg",
        one((8.0, 6.0), (-2.0, 0.0, -0.5), (2.0, 0.0, 1.0)),
        Pose::offset(-2.0, 23.9, -0.5),
    );
    r.add_or_replace_child(
        "left_front_leg",
        one((8.0, 7.0), (0.0, 0.0, -0.5), (2.0, 0.0, 1.0)),
        Pose::offset(2.0, 23.9, -0.5),
    );
    LayerDefinition::new(r, 16.0, 16.0)
}

#[derive(Clone, Copy, Debug)]
pub struct TurtleParts {
    quad: QuadrupedParts,
    egg_belly: Option<PartId>,
    right_front: PartId,
    left_front: PartId,
    right_hind: PartId,
    left_hind: PartId,
}

impl TurtleParts {
    pub fn find(model: &Model) -> Self {
        Self {
            quad: QuadrupedParts::find(model),
            egg_belly: model.find("egg_belly"),
            right_front: model.part("right_front_leg"),
            left_front: model.part("left_front_leg"),
            right_hind: model.part("right_hind_leg"),
            left_hind: model.part("left_hind_leg"),
        }
    }
}

/// What the turtle's pose reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TurtlePose {
    pub on_land: bool,
    pub laying_egg: bool,
    pub has_egg: bool,
}

/// `TurtleModel.setupAnim` (over `QuadrupedModel`'s).
pub fn setup_anim(
    parts: &TurtleParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    turtle: TurtlePose,
) {
    quadruped::setup_anim(&parts.quad, pose, state);
    let (position, speed) = (state.walk_position, state.walk_speed);
    if turtle.on_land {
        let (lay, amplitude) = if turtle.laying_egg {
            (4.0, 2.0)
        } else {
            (1.0, 1.0)
        };
        let swing = position * 5.0;
        let front = angle::cos(lay * swing);
        let hind = angle::cos(swing);
        pose[parts.right_front].pose.rotation.y = -front * 8.0 * speed * amplitude;
        pose[parts.left_front].pose.rotation.y = front * 8.0 * speed * amplitude;
        pose[parts.right_hind].pose.rotation.y = -hind * 3.0 * speed;
        pose[parts.left_hind].pose.rotation.y = hind * 3.0 * speed;
    } else {
        let scale = 0.5 * speed;
        let swing = angle::cos(position * 0.6662 * 0.6) * scale;
        pose[parts.right_hind].pose.rotation.x = swing;
        pose[parts.left_hind].pose.rotation.x = -swing;
        pose[parts.right_front].pose.rotation.z = -swing;
        pose[parts.left_front].pose.rotation.z = swing;
    }
    if let Some(egg) = parts.egg_belly {
        pose[egg].visible = turtle.has_egg;
        if turtle.has_egg {
            let root = pose.get_mut(crate::world::entity::model::PartId(0));
            root.pose.offset.y -= 1.0;
        }
    }
}

/// `TurtleRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    out.push(LayerSpec::aged(
        baby,
        (M::Turtle, "minecraft:entity/turtle/turtle"),
        (M::TurtleBaby, "minecraft:entity/turtle/turtle_baby"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flippers_sweep_on_land_and_row_in_water_and_an_egg_shows() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = TurtleParts::find(&model);
            let state = LivingRenderState {
                walk_position: 2.0,
                walk_speed: 1.0,
                ..Default::default()
            };
            let mut land = model.initial_pose();
            setup_anim(
                &parts,
                &mut land,
                &state,
                TurtlePose {
                    on_land: true,
                    ..Default::default()
                },
            );
            assert_ne!(land[parts.right_front].pose.rotation.y, 0.0);
            let mut water = model.initial_pose();
            setup_anim(&parts, &mut water, &state, TurtlePose::default());
            assert_ne!(water[parts.right_front].pose.rotation.z, 0.0);
            assert_eq!(water[parts.right_front].pose.rotation.y, 0.0);
        }
        let model = Model::bake(&adult());
        let parts = TurtleParts::find(&model);
        let mut pose = model.initial_pose();
        let root_y = pose[PartId(0)].pose.offset.y;
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            TurtlePose {
                has_egg: true,
                ..Default::default()
            },
        );
        assert!(pose[parts.egg_belly.unwrap()].visible);
        assert_eq!(pose[PartId(0)].pose.offset.y, root_y - 1.0);
    }
}
