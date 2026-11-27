//! Heads as models (`SkullModelBase` and its kinds), shared by everything that draws one.

use glam::Mat4;

use crate::block_entity::models::{self, SkullType};
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use melon_core::math::angle::{self, PI};
use crate::model_part::{Part, Pose};

/// Names for a head's child parts, in [`models::skull`]'s order.
const CHILDREN: [&str; 4] = ["child0", "child1", "child2", "child3"];

/// One moving piece of a head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub child: usize,
    pub axis: usize,
    pub angle: f32,
}

/// `SkullModelBase.setupAnim` at `animation`.
pub fn joints(skull: SkullType, animation: f32) -> [Option<Joint>; 2] {
    let joint = |child, axis, angle| Some(Joint { child, axis, angle });
    match skull {
        SkullType::Dragon => [
            joint(0, 0, (angle::sin(animation * PI * 0.2) + 1.0) * 0.2),
            None,
        ],
        SkullType::Piglin => [
            joint(0, 2, -(angle::cos(animation * PI * 0.2 * 1.2) + 2.5) * 0.2),
            joint(1, 2, (angle::cos(animation * PI * 0.2) + 2.5) * 0.2),
        ],
        _ => [None; 2],
    }
}

/// Moves a head's [`models::skull`] part to `animation`.
pub fn animate_part(skull: SkullType, part: &mut Part, animation: f32) {
    for joint in joints(skull, animation).into_iter().flatten() {
        if let Some(child) = part.children.get_mut(joint.child) {
            child.pose.rotation[joint.axis] = joint.angle;
        }
    }
}

/// `SkullModelBase.State.animationPos` of a placed head.
pub fn placed_animation(ticks: u32, animating: bool, partial: f32) -> f32 {
    ticks as f32 + if animating { partial } else { 0.0 }
}

/// One kind of head as a model.
pub struct SkullModel {
    kind: SkullType,
    model: Model,
    /// The head's children, by their index in [`models::skull`].
    children: Vec<PartId>,
    /// A player's hat layer, drawn see-through.
    overlay: Option<PartId>,
}

impl SkullModel {
    pub fn new(kind: SkullType) -> Self {
        let part = models::skull(kind);
        let mut root = PartDefinition::root();
        let overlay = kind == SkullType::Player;
        let head_cubes = if overlay {
            part.cubes[..1].to_vec()
        } else {
            part.cubes.clone()
        };
        let head = root.add_or_replace_child("head", head_cubes, part.pose);
        if overlay {
            head.add_or_replace_child("hat", part.cubes[1..].to_vec(), Pose::ZERO);
        }
        for (child, name) in part.children.iter().zip(CHILDREN) {
            add_tree(head, name, child);
        }
        let size = kind.texture_size();
        let model = Model::bake(&LayerDefinition::new(root, size.x, size.y));
        let children = CHILDREN[..part.children.len().min(CHILDREN.len())]
            .iter()
            .map(|name| model.part(name))
            .collect();
        let overlay = overlay.then(|| model.part("hat"));
        Self {
            kind,
            model,
            children,
            overlay,
        }
    }

    pub fn kind(&self) -> SkullType {
        self.kind
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The part drawn see-through over the rest, if there is one.
    pub fn overlay(&self) -> Option<PartId> {
        self.overlay
    }

    /// The head at `animation` ([`joints`]).
    pub fn pose(&self, animation: f32) -> ModelPose {
        let mut pose = self.model.initial_pose();
        for joint in joints(self.kind, animation).into_iter().flatten() {
            if let Some(&part) = self.children.get(joint.child) {
                pose[part].pose.rotation[joint.axis] = joint.angle;
            }
        }
        pose
    }

    /// Every part's matrix at `animation`, the head placed by `at`.
    pub fn matrices(&self, animation: f32, at: Mat4) -> Vec<Option<Mat4>> {
        self.model.part_matrices(&self.pose(animation), at)
    }
}

/// A [`Part`] and its children under `parent`, as `name`.
fn add_tree(parent: &mut PartDefinition, name: &'static str, part: &Part) {
    let node = parent.add_or_replace_child(name, part.cubes.clone(), part.pose);
    for (child, name) in part.children.iter().zip(["grandchild0", "grandchild1"]) {
        add_tree(node, name, child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dragon_opens_its_jaw_and_a_player_head_has_a_hat() {
        let dragon = SkullModel::new(SkullType::Dragon);
        let shut = dragon.pose(0.0);
        let open = dragon.pose(2.5);
        let jaw = dragon.model().part("child0");
        assert!((shut[jaw].pose.rotation.x - 0.2).abs() < 1e-5);
        assert!((open[jaw].pose.rotation.x - 0.4).abs() < 1e-5);
        assert!(dragon.overlay().is_none());

        let player = SkullModel::new(SkullType::Player);
        assert!(player.overlay().is_some());
        let mut part = models::skull(SkullType::Dragon);
        animate_part(SkullType::Dragon, &mut part, 2.5);
        assert!((part.children[0].pose.rotation.x - 0.4).abs() < 1e-5);
    }
}
