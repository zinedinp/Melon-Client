//! `SlimeModel` and `MagmaCubeModel`: the cube mobs.

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::model_part::{Cube, Pose};

/// `SlimeModel.createInnerBodyLayer`: the core, eyes and mouth.
pub fn inner() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let part = |root: &mut PartDefinition, name, uv, from, size| {
        root.add_or_replace_child(name, vec![Cube::new(uv, from, size)], Pose::ZERO);
    };
    part(
        &mut root,
        "cube",
        (0.0, 16.0),
        (-3.0, 17.0, -3.0),
        (6.0, 6.0, 6.0),
    );
    part(
        &mut root,
        "right_eye",
        (32.0, 0.0),
        (-3.25, 18.0, -3.5),
        (2.0, 2.0, 2.0),
    );
    part(
        &mut root,
        "left_eye",
        (32.0, 4.0),
        (1.25, 18.0, -3.5),
        (2.0, 2.0, 2.0),
    );
    part(
        &mut root,
        "mouth",
        (32.0, 8.0),
        (0.0, 21.0, -3.5),
        (1.0, 1.0, 1.0),
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

/// `SlimeModel.createOuterBodyLayer`: the jelly around it.
pub fn outer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "cube",
        vec![Cube::new((0.0, 0.0), (-4.0, 16.0, -4.0), (8.0, 8.0, 8.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 32.0)
}

const SEGMENTS: usize = 8;

fn segment_name(i: usize) -> &'static str {
    [
        "cube0", "cube1", "cube2", "cube3", "cube4", "cube5", "cube6", "cube7",
    ][i]
}

/// `MagmaCubeModel.createBodyLayer`.
pub fn magma_cube() -> LayerDefinition {
    let mut root = PartDefinition::root();
    for i in 0..SEGMENTS {
        let (u, v) = match i {
            0 => (0.0, 0.0),
            1..=3 => (0.0, 9.0 * i as f32),
            _ => (32.0, 9.0 * i as f32 - 36.0),
        };
        root.add_or_replace_child(
            segment_name(i),
            vec![Cube::new(
                (u, v),
                (-4.0, 16.0 + i as f32, -4.0),
                (8.0, 1.0, 8.0),
            )],
            Pose::ZERO,
        );
    }
    root.add_or_replace_child(
        "inside_cube",
        vec![Cube::new((24.0, 40.0), (-2.0, 18.0, -2.0), (4.0, 4.0, 4.0))],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// The magma cube's slices, found once.
#[derive(Clone, Copy, Debug)]
pub struct MagmaParts {
    cubes: [PartId; SEGMENTS],
}

impl MagmaParts {
    pub fn find(model: &Model) -> Self {
        Self {
            cubes: std::array::from_fn(|i| model.part(segment_name(i))),
        }
    }
}

/// `MagmaCubeModel.setupAnim`: the slices move apart by the squish.
pub fn setup_anim(parts: &MagmaParts, pose: &mut ModelPose, squish: f32) {
    let squish = squish.max(0.0);
    for (i, &cube) in parts.cubes.iter().enumerate() {
        pose[cube].pose.offset.y = -(4.0 - i as f32) * squish * 1.7;
    }
}

/// `SlimeRenderer` and `MagmaCubeRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Slime => {
            out.push(plain(M::SlimeInner, "minecraft:entity/slime/slime"));
            out.push(plain(M::SlimeOuter, "minecraft:entity/slime/slime").translucent());
        }
        EntityKind::MagmaCube => out.push(plain(M::MagmaCube, "minecraft:entity/slime/magmacube")),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_squashing_magma_cube_spreads_its_slices() {
        let model = Model::bake(&magma_cube());
        let parts = MagmaParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(&parts, &mut pose, 1.0);
        let lowest = pose[parts.cubes[0]].pose.offset.y;
        let highest = pose[parts.cubes[7]].pose.offset.y;
        assert!(lowest < 0.0 && highest > 0.0, "{lowest} {highest}");
        let mut rest = model.initial_pose();
        setup_anim(&parts, &mut rest, -0.5);
        assert_eq!(rest[parts.cubes[0]].pose.offset.y, 0.0);
    }
}
