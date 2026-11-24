//! `NautilusModel`: a shell over a body with a mouth of three parts.

use super::rounded;
use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::network::play::game::EquipmentSlot;
use crate::world::entity::animations::nautilus::SWIMMING;
use crate::world::entity::keyframe::{AnimationDefinition, KeyframeAnimation};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// The swimming animation, driven by the walk.
pub static WALK_ANIMATION: &AnimationDefinition = &SWIMMING;

/// `0.0` for the shell's box and the mouth's, which are grown a hair.
fn mouth(uv: (f32, f32), origin: (f32, f32, f32), size: (f32, f32, f32), grow: f32) -> Vec<Cube> {
    vec![Cube::new(uv, origin, size).grown(grow)]
}

pub fn adult() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let root = r.add_or_replace_child("root", vec![], Pose::offset(0.0, 29.0, -6.0));
    root.add_or_replace_child(
        "shell",
        vec![
            Cube::new((0.0, 0.0), (-7.0, -10.0, -7.0), (14.0, 10.0, 16.0)),
            Cube::new((0.0, 26.0), (-7.0, 0.0, -7.0), (14.0, 8.0, 20.0)),
            Cube::new((48.0, 26.0), (-7.0, 0.0, 6.0), (14.0, 8.0, 0.0)),
        ],
        Pose::offset(0.0, -13.0, 5.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 54.0), (-5.0, -4.51, -3.0), (10.0, 8.0, 14.0)),
            Cube::new((0.0, 76.0), (-5.0, -4.51, 7.0), (10.0, 8.0, 0.0)),
        ],
        Pose::offset(0.0, -8.5, 12.3),
    );
    body.add_or_replace_child(
        "upper_mouth",
        mouth((54.0, 54.0), (-5.0, -2.0, 0.0), (10.0, 4.0, 4.0), -0.001),
        Pose::offset(0.0, -2.51, 7.0),
    );
    body.add_or_replace_child(
        "inner_mouth",
        mouth((54.0, 70.0), (-3.0, -2.0, -0.5), (6.0, 4.0, 4.0), 0.0),
        Pose::offset(0.0, -0.51, 7.5),
    );
    body.add_or_replace_child(
        "lower_mouth",
        mouth((54.0, 62.0), (-5.0, -1.98, 0.0), (10.0, 4.0, 4.0), -0.001),
        Pose::offset(0.0, 1.49, 7.0),
    );
    LayerDefinition::new(r, 128.0, 128.0)
}

/// `ZombieNautilusCoralModel.createBodyLayer`: the adult's body with
/// corals growing on the shell. Vanilla's angles are its own literals.
pub fn zombie_coral() -> LayerDefinition {
    let mut layer = adult();
    let shell = layer.root.child_mut("root").child_mut("shell");
    let corals = shell.add_or_replace_child("corals", vec![], Pose::offset(8.0, 4.5, -8.0));
    let plane = |uv, min, size| vec![Cube::new(uv, min, size)];
    let yellow =
        corals.add_or_replace_child("yellow_coral", vec![], Pose::offset(0.0, -11.0, 11.0));
    yellow.add_or_replace_child(
        "yellow_coral_second",
        plane((0.0, 85.0), (-4.5, -3.5, 0.0), (6.0, 8.0, 0.0)),
        Pose::offset_rotation((0.0, 0.0, 2.0), (0.0, -rounded::PI_4, 0.0)),
    );
    yellow.add_or_replace_child(
        "yellow_coral_first",
        plane((0.0, 85.0), (-4.5, -3.5, 0.0), (6.0, 8.0, 0.0)),
        Pose::offset_rotation((0.0, 0.0, 0.0), (0.0, rounded::PI_4, 0.0)),
    );
    let pink = corals.add_or_replace_child(
        "pink_coral",
        plane((-8.0, 94.0), (-4.5, 4.5, 0.0), (6.0, 0.0, 8.0)),
        Pose::offset(-12.5, -18.0, 11.0),
    );
    pink.add_or_replace_child(
        "pink_coral_second",
        plane((-8.0, 94.0), (-3.0, 0.0, -4.0), (6.0, 0.0, 8.0)),
        Pose::offset_rotation((-1.5, 4.5, 4.0), (0.0, 0.0, rounded::PI_2)),
    );
    let blue = corals.add_or_replace_child("blue_coral", vec![], Pose::offset(-14.0, 0.0, 5.5));
    blue.add_or_replace_child(
        "blue_second",
        plane((0.0, 102.0), (-3.5, -5.5, 0.0), (5.0, 10.0, 0.0)),
        Pose::offset_rotation((0.0, 0.0, -2.0), (0.0, rounded::PI_4, 0.0)),
    );
    blue.add_or_replace_child(
        "blue_first",
        plane((0.0, 102.0), (-3.5, -5.5, 0.0), (5.0, 10.0, 0.0)),
        Pose::offset_rotation((0.0, 0.0, 0.0), (0.0, -rounded::PI_4, 0.0)),
    );
    let red = corals.add_or_replace_child("red_coral", vec![], Pose::ZERO);
    red.add_or_replace_child(
        "red_coral_second",
        plane((0.0, 112.0), (-2.5, -5.5, 0.0), (4.0, 10.0, 0.0)),
        Pose::offset_rotation((-0.5, -1.0, 1.5), (0.0, -0.829, 0.0)),
    );
    red.add_or_replace_child(
        "red_coral_first",
        plane((0.0, 112.0), (-4.5, -5.5, 0.0), (6.0, 10.0, 0.0)),
        Pose::offset_rotation((0.0, 0.0, 0.0), (0.0, rounded::PI_4, 0.0)),
    );
    layer
}

/// `NautilusArmorModel.createBodyLayer`.
pub fn armor() -> LayerDefinition {
    let mut layer = adult();
    layer.root.child_mut("root").add_or_replace_child(
        "shell",
        vec![
            Cube::new((0.0, 0.0), (-7.0, -10.0, -7.0), (14.0, 10.0, 16.0)).grown(0.01),
            Cube::new((0.0, 26.0), (-7.0, 0.0, -7.0), (14.0, 8.0, 20.0)).grown(0.01),
            Cube::new((48.0, 26.0), (-7.0, 0.0, 6.0), (14.0, 8.0, 0.0)),
        ],
        Pose::offset(0.0, -13.0, 5.0),
    );
    layer
}

/// `NautilusSaddleModel.createSaddleLayer`: the shell grown by 0.2.
pub fn saddle() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let root = r.add_or_replace_child("root", vec![], Pose::offset(0.0, 29.0, -6.0));
    root.add_or_replace_child(
        "shell",
        vec![Cube::new((0.0, 0.0), (-7.0, -10.0, -7.0), (14.0, 10.0, 16.0)).grown(0.2)],
        Pose::offset(0.0, -13.0, 5.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 54.0), (-5.0, -4.51, -3.0), (10.0, 8.0, 14.0)),
            Cube::new((0.0, 76.0), (-5.0, -4.51, 7.0), (10.0, 8.0, 0.0)),
        ],
        Pose::offset(0.0, -8.5, 12.3),
    );
    body.add_or_replace_child(
        "upper_mouth",
        mouth((54.0, 54.0), (-5.0, -2.0, 0.0), (10.0, 4.0, 4.0), -0.001),
        Pose::offset(0.0, -2.51, 7.0),
    );
    body.add_or_replace_child(
        "inner_mouth",
        mouth((54.0, 70.0), (-3.0, -2.0, -0.5), (6.0, 4.0, 4.0), 0.0),
        Pose::offset(0.0, -0.51, 7.5),
    );
    body.add_or_replace_child(
        "lower_mouth",
        mouth((54.0, 62.0), (-5.0, -1.98, 0.0), (10.0, 4.0, 4.0), -0.001),
        Pose::offset(0.0, 1.49, 7.0),
    );
    LayerDefinition::new(r, 128.0, 128.0)
}

pub fn baby() -> LayerDefinition {
    let mut r = PartDefinition::root();
    let root = r.add_or_replace_child("root", vec![], Pose::offset(-0.5, 28.0, -0.5));
    root.add_or_replace_child(
        "shell",
        vec![
            Cube::new((0.0, 0.0), (-6.0, -4.0, -1.0), (7.0, 4.0, 7.0)),
            Cube::new((0.0, 11.0), (-6.0, 0.0, -1.0), (7.0, 4.0, 9.0)),
            Cube::new((23.0, 11.0), (-6.0, 0.0, 5.0), (7.0, 4.0, 0.0)),
        ],
        Pose::offset(3.0, -8.0, -2.0),
    );
    let body = root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 24.0), (-2.5, -3.01, -1.0), (5.0, 4.0, 7.0)),
            Cube::new((0.0, 35.0), (-2.5, -3.01, 4.1), (5.0, 4.0, 0.0)),
        ],
        Pose::offset(0.5, -5.0, 3.0),
    );
    body.add_or_replace_child(
        "upper_mouth",
        mouth((24.0, 24.0), (-2.5, -1.0, 0.0), (5.0, 2.0, 2.0), -0.001),
        Pose::offset(0.0, -2.01, 3.9),
    );
    body.add_or_replace_child(
        "inner_mouth",
        mouth((24.0, 32.0), (-1.5, -1.0, -1.0), (3.0, 2.0, 2.0), 0.0),
        Pose::offset(0.0, -1.01, 4.9),
    );
    body.add_or_replace_child(
        "lower_mouth",
        mouth((24.0, 28.0), (-2.5, -1.0, 0.0), (5.0, 2.0, 2.0), -0.001),
        Pose::offset(0.0, -0.01, 3.9),
    );
    LayerDefinition::new(r, 64.0, 64.0)
}

#[derive(Clone, Copy, Debug)]
pub struct NautilusParts {
    body: PartId,
}

impl NautilusParts {
    pub fn find(model: &Model) -> Self {
        Self {
            body: model.part("body"),
        }
    }
}

/// `NautilusModel.setupAnim`'s turn of the body toward the look.
pub fn setup_anim(
    parts: &NautilusParts,
    swim: &KeyframeAnimation,
    pose: &mut ModelPose,
    state: &LivingRenderState,
) {
    let body = &mut pose[parts.body].pose;
    body.rotation.y = state.y_rot.clamp(-10.0, 10.0).to_radians();
    body.rotation.x = state.x_rot.clamp(-10.0, 10.0).to_radians();
    swim.apply_walk(
        pose,
        state.walk_position + state.age_in_ticks / 5.0,
        state.walk_speed + 0.2,
        2.0,
        3.0,
    );
}

/// `NautilusRenderer` and `ZombieNautilusRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Nautilus => out.push(LayerSpec::aged(
            baby,
            (M::Nautilus, "minecraft:entity/nautilus/nautilus"),
            (M::NautilusBaby, "minecraft:entity/nautilus/nautilus_baby"),
        )),
        EntityKind::ZombieNautilus => {
            use pumpkin_data::zombie_nautilus_variant::ZombieNautilusVariant;
            let variant = u8::try_from(a.thing.variant)
                .ok()
                .and_then(ZombieNautilusVariant::from_id)
                .unwrap_or_default();
            // `ZombieNautilusCoralModel`: the corals hide under armor.
            let armored = !entity.equipment.get(EquipmentSlot::Body).is_empty();
            let model = if variant.model() == Some("warm") && !armored {
                M::ZombieNautilusCoral
            } else {
                M::Nautilus
            };
            out.push(plain(model, variant.asset_id()));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nautilus_pulses_even_standing_still_and_looks_a_little() {
        for layer in [adult(), baby()] {
            let model = Model::bake(&layer);
            let parts = NautilusParts::find(&model);
            let swim = KeyframeAnimation::bake(&model, WALK_ANIMATION).unwrap();
            let rest = model.initial_pose();
            let mut pose = model.initial_pose();
            let state = LivingRenderState {
                age_in_ticks: 7.0,
                y_rot: 45.0,
                ..Default::default()
            };
            setup_anim(&parts, &swim, &mut pose, &state);
            assert_ne!(pose, rest);
            assert!((pose[parts.body].pose.rotation.y - 10.0_f32.to_radians()).abs() < 1e-5);
        }
    }
}
