//! Part matrices kept from the frame before.

use std::collections::HashMap;

use glam::Mat4;

use super::mobs::ModelId;
use super::model::{Model, ModelPose};
use melon_core::math::ChunkHash;

struct Kept {
    model: Option<ModelId>,
    pose: ModelPose,
    root: Mat4,
    matrices: Vec<Option<Mat4>>,
    /// Asked for this frame; the others go in [`PoseCache::end_frame`].
    seen: bool,
}

#[derive(Default)]
pub struct PoseCache {
    /// Entity ids are small server counters: no need for SipHash.
    by_entity: HashMap<i32, Kept, ChunkHash>,
    /// Lookups this frame that reused the matrices.
    reused: usize,
}

impl PoseCache {
    /// The part matrices of `model` (baked as `id`) in `pose`, placed by `root`.
    pub fn matrices(
        &mut self,
        entity: i32,
        id: ModelId,
        model: &Model,
        pose: &ModelPose,
        root: Mat4,
    ) -> &[Option<Mat4>] {
        let kept = self.by_entity.entry(entity).or_insert_with(|| Kept {
            model: None,
            pose: ModelPose::default(),
            root,
            matrices: Vec::new(),
            seen: false,
        });
        kept.seen = true;
        if kept.model == Some(id) && kept.root == root && kept.pose == *pose {
            self.reused += 1;
        } else {
            model.part_matrices_into(pose, root, &mut kept.matrices);
            kept.model = Some(id);
            kept.root = root;
            kept.pose.clone_from(pose);
        }
        &kept.matrices
    }

    /// How many of this frame's lookups reused last frame's matrices.
    pub fn reused(&self) -> usize {
        self.reused
    }

    /// Forgets the entities not drawn since the last call.
    pub fn end_frame(&mut self) {
        self.by_entity
            .retain(|_, kept| std::mem::take(&mut kept.seen));
        self.reused = 0;
    }

    pub fn clear(&mut self) {
        self.by_entity = HashMap::default();
        self.reused = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::living::LivingRenderState;
    use crate::entity::mobs::{MobModel, PoseInput};

    fn input(head: f32) -> PoseInput {
        PoseInput {
            living: LivingRenderState {
                y_rot: head,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_mob_standing_still_reuses_its_matrices() {
        let pig = MobModel::new(ModelId::Pig);
        let root = Mat4::from_translation(glam::Vec3::new(0.5, 0.0, 0.5));
        let mut cache = PoseCache::default();
        let pose = pig.pose(&input(10.0));
        let first = cache
            .matrices(7, ModelId::Pig, &pig.model, &pose, root)
            .to_vec();
        cache.end_frame();
        let again = cache.matrices(7, ModelId::Pig, &pig.model, &pose, root);
        assert_eq!(again, first);
        assert_eq!(cache.reused(), 1);
    }

    #[test]
    fn a_turned_head_a_step_or_another_model_computes_anew() {
        let pig = MobModel::new(ModelId::Pig);
        let cold = MobModel::new(ModelId::PigCold);
        let root = Mat4::IDENTITY;
        let moved = Mat4::from_translation(glam::Vec3::X);
        let mut cache = PoseCache::default();
        let still = pig.pose(&input(10.0));
        let turned = pig.pose(&input(20.0));
        cache.matrices(1, ModelId::Pig, &pig.model, &still, root);
        let cases = [
            (ModelId::Pig, &pig, &turned, root),
            (ModelId::Pig, &pig, &turned, moved),
            (ModelId::PigCold, &cold, &cold.pose(&input(20.0)), moved),
        ];
        for (id, mob, pose, root) in cases {
            let got = cache.matrices(1, id, &mob.model, pose, root).to_vec();
            assert_eq!(got, mob.model.part_matrices(pose, root), "{id:?}");
        }
        assert_eq!(cache.reused(), 0);
    }

    #[test]
    fn entities_not_drawn_are_forgotten() {
        let pig = MobModel::new(ModelId::Pig);
        let pose = pig.pose(&input(0.0));
        let mut cache = PoseCache::default();
        for id in 0..3 {
            cache.matrices(id, ModelId::Pig, &pig.model, &pose, Mat4::IDENTITY);
        }
        cache.end_frame();
        cache.matrices(1, ModelId::Pig, &pig.model, &pose, Mat4::IDENTITY);
        cache.end_frame();
        assert_eq!(cache.by_entity.len(), 1);
        assert!(cache.by_entity.contains_key(&1));
    }
}
