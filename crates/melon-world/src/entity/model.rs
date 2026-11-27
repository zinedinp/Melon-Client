//! Vanilla's entity model structure, as data.

use glam::{Mat4, Vec2, Vec3};

use crate::geometry;
use crate::mesh::WorldVertex as Vertex;
use crate::model_part::{self, Cube, Part, Pose};

/// A `PartDefinition`: a named part's pose, boxes and children.
#[derive(Clone, Debug)]
pub struct PartDefinition {
    pub name: &'static str,
    pub pose: Pose,
    pub cubes: Vec<Cube>,
    pub children: Vec<PartDefinition>,
}

impl PartDefinition {
    /// `MeshDefinition.getRoot`: the unnamed root at the origin.
    pub fn root() -> Self {
        Self {
            name: "root",
            pose: Pose::ZERO,
            cubes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// `addOrReplaceChild`.
    pub fn add_or_replace_child(
        &mut self,
        name: &'static str,
        cubes: Vec<Cube>,
        pose: Pose,
    ) -> &mut PartDefinition {
        let child = PartDefinition {
            name,
            pose,
            cubes,
            children: Vec::new(),
        };
        let index = match self.children.iter().position(|c| c.name == name) {
            Some(index) => {
                let old = std::mem::replace(&mut self.children[index], child);
                self.children[index].children = old.children;
                index
            }
            None => {
                self.children.push(child);
                self.children.len() - 1
            }
        };
        &mut self.children[index]
    }

    /// Keeps the boxes of the parts named in `keep` (and, if `children`, of their children).
    pub fn retain_cubes(&mut self, keep: &[&str], children: bool) {
        if keep.contains(&self.name) {
            if !children {
                for child in &mut self.children {
                    child.clear_cubes();
                }
            }
            return;
        }
        self.cubes.clear();
        for child in &mut self.children {
            child.retain_cubes(keep, children);
        }
    }

    /// `retainExactParts`: only the parts named in `keep` keep their boxes.
    pub fn retain_exact_cubes(&mut self, keep: &[&str]) {
        if !keep.contains(&self.name) {
            self.cubes.clear();
        }
        for child in &mut self.children {
            child.retain_exact_cubes(keep);
        }
    }

    /// `clearRecursively`: no boxes here or below.
    pub fn clear_cubes(&mut self) {
        self.cubes.clear();
        for child in &mut self.children {
            child.clear_cubes();
        }
    }

    /// `getChild`.
    pub fn child_mut(&mut self, name: &str) -> &mut PartDefinition {
        self.children
            .iter_mut()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("model part {name} is missing"))
    }
}

/// `LayerDefinition`: the part tree and the texture's size in texels.
#[derive(Clone, Debug)]
pub struct LayerDefinition {
    pub root: PartDefinition,
    pub texture_width: f32,
    pub texture_height: f32,
}

impl LayerDefinition {
    /// `LayerDefinition.create(mesh, width, height)`.
    pub fn new(root: PartDefinition, texture_width: f32, texture_height: f32) -> Self {
        Self {
            root,
            texture_width,
            texture_height,
        }
    }
}

/// A pose's angles that aren't zero, densely, and their sines and
/// cosines: one buffer each, kept per thread for its allocation.
#[derive(Default)]
struct Turns {
    angles: Vec<f32>,
    sin: Vec<f32>,
    cos: Vec<f32>,
}

thread_local! {
    static TURNS: std::cell::RefCell<Turns> = std::cell::RefCell::default();
}

/// A part of a [`Model`], by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PartId(pub usize);

#[derive(Clone, Debug)]
struct Node {
    name: &'static str,
    parent: Option<usize>,
    initial: Pose,
    cubes: Vec<Cube>,
}

/// A baked `ModelPart` tree: parents before their children.
#[derive(Clone, Debug)]
pub struct Model {
    nodes: Vec<Node>,
    texture_size: Vec2,
}

/// One part's pose for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartState {
    pub pose: Pose,
    pub visible: bool,
    /// `ModelPart.skipDraw`.
    pub skip_draw: bool,
}

/// Every part's [`PartState`], by [`PartId`].
#[derive(Debug, Default, PartialEq)]
pub struct ModelPose {
    parts: Vec<PartState>,
}

impl Clone for ModelPose {
    fn clone(&self) -> Self {
        Self {
            parts: self.parts.clone(),
        }
    }

    /// Into the parts already there.
    fn clone_from(&mut self, source: &Self) {
        self.parts.clone_from(&source.parts);
    }
}

impl ModelPose {
    pub fn get(&self, part: PartId) -> &PartState {
        &self.parts[part.0]
    }

    pub fn get_mut(&mut self, part: PartId) -> &mut PartState {
        &mut self.parts[part.0]
    }

    /// Two different parts at once, for animations that move a pair.
    pub fn get_two_mut(&mut self, a: PartId, b: PartId) -> [&mut PartState; 2] {
        self.parts
            .get_disjoint_mut([a.0, b.0])
            .expect("two different parts")
    }
}

impl std::ops::Index<PartId> for ModelPose {
    type Output = PartState;

    fn index(&self, part: PartId) -> &PartState {
        self.get(part)
    }
}

impl std::ops::IndexMut<PartId> for ModelPose {
    fn index_mut(&mut self, part: PartId) -> &mut PartState {
        self.get_mut(part)
    }
}

impl Model {
    /// `LayerDefinition.bakeRoot`.
    pub fn bake(layer: &LayerDefinition) -> Self {
        let mut nodes = Vec::new();
        fn walk(def: &PartDefinition, parent: Option<usize>, nodes: &mut Vec<Node>) {
            let index = nodes.len();
            nodes.push(Node {
                name: def.name,
                parent,
                initial: def.pose,
                cubes: def.cubes.clone(),
            });
            for child in &def.children {
                walk(child, Some(index), nodes);
            }
        }
        walk(&layer.root, None, &mut nodes);
        Self {
            nodes,
            texture_size: Vec2::new(layer.texture_width, layer.texture_height),
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn root(&self) -> PartId {
        PartId(0)
    }

    /// The part named `name` anywhere in the tree (`getChild` down the parts).
    pub fn part(&self, name: &str) -> PartId {
        self.find(name)
            .unwrap_or_else(|| panic!("model part {name} is missing"))
    }

    /// The part named `name`, if the model has one.
    pub fn find(&self, name: &str) -> Option<PartId> {
        self.nodes.iter().position(|n| n.name == name).map(PartId)
    }

    pub fn name(&self, part: PartId) -> &'static str {
        self.nodes[part.0].name
    }

    /// The definition's pose, every part visible (`resetPose`).
    pub fn initial_pose(&self) -> ModelPose {
        ModelPose {
            parts: self
                .nodes
                .iter()
                .map(|n| PartState {
                    pose: n.initial,
                    visible: true,
                    skip_draw: false,
                })
                .collect(),
        }
    }

    /// The definition's pose of `part`.
    pub fn initial(&self, part: PartId) -> Pose {
        self.nodes[part.0].initial
    }

    /// `ModelPart.resetPose` for one part.
    pub fn reset_part(&self, pose: &mut ModelPose, part: PartId) {
        pose.parts[part.0].pose = self.nodes[part.0].initial;
    }

    /// Whether `part` is `ancestor` or below it.
    pub fn is_within(&self, part: PartId, ancestor: PartId) -> bool {
        let mut at = Some(part.0);
        while let Some(index) = at {
            if index == ancestor.0 {
                return true;
            }
            at = self.nodes[index].parent;
        }
        false
    }

    /// Each part's matrix from model pixels to the world, `None` for the hidden ones.
    pub fn part_matrices(&self, pose: &ModelPose, root: Mat4) -> Vec<Option<Mat4>> {
        let mut out = Vec::with_capacity(self.nodes.len());
        self.part_matrices_into(pose, root, &mut out);
        out
    }

    /// [`Self::part_matrices`] into `out`, replacing what it held.
    pub fn part_matrices_into(&self, pose: &ModelPose, root: Mat4, out: &mut Vec<Option<Mat4>>) {
        out.clear();
        TURNS.with_borrow_mut(|Turns { angles, sin, cos }| {
            // Every angle a part turns by, all at once (most parts turn
            // about one axis: the zeros are left out). Padded with zeros
            // to whole steps, so the kernel never copies a short last one.
            angles.clear();
            for state in &pose.parts {
                let r = state.pose.rotation;
                if r != Vec3::ZERO {
                    angles.extend(r.to_array().into_iter().filter(|&a| a != 0.0));
                }
            }
            let dense = angles.len();
            if dense > 0 {
                let padded = dense.next_multiple_of(crate::math::simd::PAD);
                angles.resize(padded, 0.0);
                sin.resize(padded, 0.0);
                cos.resize(padded, 0.0);
                crate::math::sin_cos::sin_cos(angles, sin, cos);
            }
            let mut next = 0;
            let mut take = |a: f32| {
                // `euler_zyx`'s own shortcut for an axis it doesn't turn
                // about: the same bits as the sine of `±0`.
                if a == 0.0 {
                    return (a, 1.0);
                }
                let at = next;
                next += 1;
                (sin[at], cos[at])
            };
            for (node, state) in self.nodes.iter().zip(&pose.parts) {
                let parent = match node.parent {
                    Some(parent) => out[parent],
                    None => Some(root),
                };
                let r = state.pose.rotation;
                let turn = (r != Vec3::ZERO).then(|| {
                    let [(sx, cx), (sy, cy), (sz, cz)] = r.to_array().map(&mut take);
                    crate::math::angle::euler_zyx_of(Vec3::new(sx, sy, sz), Vec3::new(cx, cy, cz))
                });
                out.push(
                    parent
                        .filter(|_| state.visible)
                        .map(|parent| state.pose.placed_in_turned(parent, turn)),
                );
            }
        });
        for (matrix, state) in out.iter_mut().zip(&pose.parts) {
            if state.skip_draw {
                *matrix = None;
            }
        }
    }

    /// Whether `other`'s parts hang off each other the way this model's do.
    pub fn shares_tree(&self, other: &Model) -> bool {
        self.nodes.len() == other.nodes.len()
            && self
                .nodes
                .iter()
                .zip(&other.nodes)
                .all(|(a, b)| a.parent == b.parent)
    }

    /// The matrices of `part` and the parts below it only.
    pub fn subtree_matrices(
        &self,
        pose: &ModelPose,
        part: PartId,
        base: Mat4,
    ) -> Vec<Option<Mat4>> {
        let mut out: Vec<Option<Mat4>> = vec![None; self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate() {
            if !self.is_within(PartId(index), part) {
                continue;
            }
            let state = &pose.parts[index];
            let parent = if index == part.0 {
                Some(base)
            } else {
                node.parent.and_then(|p| out[p])
            };
            out[index] = parent
                .filter(|_| state.visible)
                .map(|parent| state.pose.placed_in(parent));
        }
        for (index, state) in pose.parts.iter().enumerate() {
            if state.skip_draw {
                out[index] = None;
            }
        }
        out
    }

    /// The matrix `translateAndRotate` leaves for `part` and its ancestors.
    pub fn part_transform(&self, pose: &ModelPose, part: PartId, root: Mat4) -> Mat4 {
        pose[part]
            .pose
            .placed_in(self.parent_transform(pose, part, root))
    }

    /// The matrix `part`'s own pose is applied in.
    pub fn parent_transform(&self, pose: &ModelPose, part: PartId, root: Mat4) -> Mat4 {
        match self.nodes[part.0].parent {
            Some(parent) => self.part_transform(pose, PartId(parent), root),
            None => root,
        }
    }

    /// Every face of `part`'s own boxes in its space (blocks, Y down).
    pub fn part_faces(&self, part: PartId, visit: &mut impl FnMut(ModelFace)) {
        let node = &self.nodes[part.0];
        if node.cubes.is_empty() {
            return;
        }
        model_part::faces(
            &Part::new(Pose::ZERO, node.cubes.clone()),
            Mat4::IDENTITY,
            &mut |face| {
                let [a, b, c, _] = face.corners;
                visit(ModelFace {
                    corners: face.corners,
                    uvs: face.texels.map(|t| t / self.texture_size),
                    normal: (b - a).cross(c - a).normalize_or_zero(),
                });
            },
        );
    }

    /// `part`'s own boxes as vertices in its space (blocks, Y down).
    pub fn part_vertices(&self, part: PartId) -> Vec<Vertex> {
        self.part_vertices_sided(part, false)
    }

    /// [`Self::part_vertices`], each face also wound the other way when
    /// `two_sided`: what a render type without culling shows. The back is
    /// shaded for the opposite normal (`PER_FACE_LIGHTING`'s `-light`).
    pub fn part_vertices_sided(&self, part: PartId, two_sided: bool) -> Vec<Vertex> {
        let mut out = Vec::new();
        self.part_faces(part, &mut |face| {
            let n = face.normal;
            let shade = geometry::shade(Vec3::new(-n.x, -n.y, n.z));
            let back_shade = geometry::shade(Vec3::new(n.x, n.y, -n.z));
            let corners = std::array::from_fn(|i| {
                let [u, v] = face.uvs[i].to_array();
                (
                    face.corners[i].to_array(),
                    geometry::atlas_uv([0.0, 0.0, 1.0, 1.0], u, v),
                )
            });
            geometry::push_quad(&mut out, corners, [shade; 3], false);
            if two_sided {
                geometry::push_quad(&mut out, corners, [back_shade; 3], true);
            }
        });
        out
    }
}

/// One face of a part's boxes, in the part's space.
#[derive(Clone, Copy, Debug)]
pub struct ModelFace {
    pub corners: [Vec3; 4],
    /// Texture coordinates, `0..1` over the layer's texture.
    pub uvs: [Vec2; 4],
    /// Outward, in model space (Y down).
    pub normal: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sines worked out all at once give every mob the matrices one
    /// part at a time gives, to the bit.
    #[test]
    fn batched_turns_place_every_mob_as_one_by_one() {
        use crate::entity::humanoid::HumanoidRenderState;
        use crate::entity::living::LivingRenderState;
        use crate::entity::mobs::{MobModel, ModelId, PoseInput};
        let root = Mat4::from_translation(Vec3::new(0.3, 1.5, -0.2));
        for (k, id) in ModelId::ALL.into_iter().enumerate() {
            let living = LivingRenderState {
                body_rot: k as f32 * 7.3,
                y_rot: -24.0 + k as f32,
                x_rot: 12.5,
                walk_position: 3.7 + k as f32,
                walk_speed: 0.8,
                age_in_ticks: 41.25 + k as f32,
                ..Default::default()
            };
            let input = PoseInput {
                living,
                humanoid: HumanoidRenderState::from_living(&living),
                flap: (0.4, 0.5),
                ..Default::default()
            };
            let mob = MobModel::new(id);
            let pose = mob.pose(&input);
            let batched = mob.model.part_matrices(&pose, root);
            let mut one_by_one: Vec<Option<Mat4>> = Vec::new();
            for (node, state) in mob.model.nodes.iter().zip(&pose.parts) {
                let parent = node.parent.map_or(Some(root), |p| one_by_one[p]);
                one_by_one.push(
                    parent
                        .filter(|_| state.visible)
                        .map(|parent| state.pose.placed_in(parent)),
                );
            }
            for (m, state) in one_by_one.iter_mut().zip(&pose.parts) {
                if state.skip_draw {
                    *m = None;
                }
            }
            let bits = |m: &Vec<Option<Mat4>>| {
                m.iter()
                    .map(|m| m.map(|m| m.to_cols_array().map(f32::to_bits)))
                    .collect::<Vec<_>>()
            };
            assert_eq!(bits(&batched), bits(&one_by_one), "{id:?}");
        }
    }

    fn layer() -> LayerDefinition {
        let mut root = PartDefinition::root();
        let arm = root.add_or_replace_child(
            "arm",
            vec![Cube::new((0.0, 0.0), (0.0, 0.0, 0.0), (1.0, 1.0, 1.0))],
            Pose::offset(16.0, 0.0, 0.0),
        );
        arm.add_or_replace_child("sleeve", Vec::new(), Pose::offset(0.0, 16.0, 0.0));
        LayerDefinition {
            root,
            texture_width: 64.0,
            texture_height: 64.0,
        }
    }

    #[test]
    fn children_follow_their_parent() {
        let model = Model::bake(&layer());
        let pose = model.initial_pose();
        let matrices = model.part_matrices(&pose, Mat4::IDENTITY);
        let sleeve = model.part("sleeve");
        let at = matrices[sleeve.0].unwrap().transform_point3(Vec3::ZERO);
        assert!((at - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-6, "{at}");
    }

    #[test]
    fn a_hidden_part_hides_its_children() {
        let model = Model::bake(&layer());
        let mut pose = model.initial_pose();
        pose[model.part("arm")].visible = false;
        let matrices = model.part_matrices(&pose, Mat4::IDENTITY);
        assert!(matrices.iter().skip(1).all(Option::is_none));
    }

    #[test]
    fn skip_draw_hides_a_part_and_leaves_its_children() {
        let model = Model::bake(&layer());
        let mut pose = model.initial_pose();
        pose[model.part("arm")].skip_draw = true;
        let matrices = model.part_matrices(&pose, Mat4::IDENTITY);
        assert!(matrices[model.part("arm").0].is_none());
        assert!(matrices[model.part("sleeve").0].is_some());
    }

    #[test]
    fn replacing_a_child_keeps_one_of_that_name_and_its_children() {
        let mut def = layer();
        def.root.add_or_replace_child("arm", Vec::new(), Pose::ZERO);
        assert_eq!(def.root.children.len(), 1);
        assert!(def.root.children[0].cubes.is_empty());
        assert_eq!(def.root.children[0].children[0].name, "sleeve");
    }

    #[test]
    fn retaining_cubes_keeps_the_tree() {
        let mut def = layer();
        def.root.retain_cubes(&["sleeve"], false);
        let model = Model::bake(&def);
        assert_eq!(model.len(), 3);
        assert!(model.part_vertices(model.part("arm")).is_empty());
    }

    #[test]
    fn a_subtree_starts_at_its_own_part() {
        let model = Model::bake(&layer());
        let pose = model.initial_pose();
        let matrices = model.subtree_matrices(&pose, model.part("arm"), Mat4::IDENTITY);
        assert!(matrices[0].is_none());
        let at = matrices[model.part("arm").0]
            .unwrap()
            .transform_point3(Vec3::ZERO);
        assert!((at - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn tops_are_shaded_bright_once_upright() {
        let model = Model::bake(&layer());
        let vertices = model.part_vertices(model.part("arm"));
        let top = vertices
            .chunks(4)
            .find(|quad| quad.iter().all(|v| v.pos[1] == 0.0))
            .unwrap();
        let bottom = vertices
            .chunks(4)
            .find(|quad| quad.iter().all(|v| v.pos[1] > 0.0))
            .unwrap();
        assert!(top[0].color[0] > bottom[0].color[0]);
    }

    #[test]
    fn a_mirrored_box_still_faces_out() {
        let mut root = PartDefinition::root();
        root.add_or_replace_child(
            "box",
            vec![Cube::new((0.0, 0.0), (-1.0, -1.0, -1.0), (2.0, 2.0, 2.0)).mirrored()],
            Pose::ZERO,
        );
        let model = Model::bake(&LayerDefinition {
            root,
            texture_width: 64.0,
            texture_height: 64.0,
        });
        model.part_faces(model.part("box"), &mut |face| {
            let center = face.corners.iter().sum::<Vec3>() / 4.0;
            assert!(face.normal.dot(center) > 0.0, "{face:?}");
        });
    }
}
