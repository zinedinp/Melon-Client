//! Which models wear what is in the head slot (`CustomHeadLayer`).

use super::{MobModel, Rig};
use crate::entity::kind::EntityKind;
use crate::entity::model::PartId;

/// `CustomHeadLayer.Transforms`, in blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadTransforms {
    /// Up or down for a worn item (`yOffset`).
    pub y_offset: f32,
    /// Up or down for a worn head (`skullYOffset`).
    pub skull_y_offset: f32,
    /// The scale around the wearer, sideways and up (`horizontalScale`, `verticalScale`).
    pub horizontal_scale: f32,
    pub vertical_scale: f32,
}

impl HeadTransforms {
    /// `Transforms.DEFAULT`.
    pub const DEFAULT: Self = Self {
        y_offset: 0.0,
        skull_y_offset: 0.0,
        horizontal_scale: 1.0,
        vertical_scale: 1.0,
    };
    /// `VillagerRenderer.CUSTOM_HEAD_TRANSFORMS`.
    pub const VILLAGER: Self = Self {
        y_offset: -0.117_187_5,
        skull_y_offset: -0.074_218_75,
        horizontal_scale: 1.0,
        vertical_scale: 1.0,
    };
    /// `PiglinRenderer.PIGLIN_CUSTOM_HEAD_TRANSFORMS`.
    pub const PIGLIN: Self = Self {
        y_offset: 0.0,
        skull_y_offset: 0.0,
        horizontal_scale: 1.001_953_1,
        vertical_scale: 1.0,
    };
}

/// Where a model of `kind` wears its head slot.
pub fn custom_head(kind: EntityKind, mob: &MobModel) -> Option<(PartId, HeadTransforms)> {
    match &mob.rig {
        Rig::Humanoid(parts) => Some((parts.head, HeadTransforms::DEFAULT)),
        Rig::ArmorStand(parts) => Some((parts.humanoid.head, HeadTransforms::DEFAULT)),
        Rig::Bogged(parts) => Some((parts.humanoid.head, HeadTransforms::DEFAULT)),
        Rig::Illager(parts) => Some((parts.humanoid.head, HeadTransforms::DEFAULT)),
        Rig::Piglin(parts) => Some((parts.humanoid.head, HeadTransforms::PIGLIN)),
        Rig::Villager(parts) => match kind {
            EntityKind::Villager | EntityKind::ZombieVillager => {
                Some((parts.head(), HeadTransforms::VILLAGER))
            }
            EntityKind::WanderingTrader => Some((parts.head(), HeadTransforms::DEFAULT)),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::mobs::ModelId;

    #[test]
    fn villagers_and_illagers_wear_heads_and_witches_do_not() {
        let villager = MobModel::new(ModelId::Villager);
        assert_eq!(
            custom_head(EntityKind::Villager, &villager).map(|h| h.1),
            Some(HeadTransforms::VILLAGER)
        );
        assert_eq!(
            custom_head(EntityKind::WanderingTrader, &villager).map(|h| h.1),
            Some(HeadTransforms::DEFAULT)
        );
        let witch = MobModel::new(ModelId::Witch);
        assert!(custom_head(EntityKind::Witch, &witch).is_none());
        let illager = MobModel::new(ModelId::Illager);
        assert!(custom_head(EntityKind::Vindicator, &illager).is_some());
    }
}
