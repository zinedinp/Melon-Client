//! Blocks drawn by a block entity renderer (vanilla `BlockEntityRenderers`):

mod anim;
pub mod item;
mod kind;
pub mod models;
mod render;
mod textures;

use anim::{Anim, Book, wrap_radians};
use kind::DYES;
use textures::{
    BANNER_BASE, BANNER_SPRITES, BELL_BODY, BOOK, CONDUIT_SHELL, END_PORTAL, POT_BASE, POT_SIDE,
    chest_texture,
};

pub use kind::{KIND_STATES, Kind, Placement, kind_of};
pub use render::{at_rest, skull_transform};
pub use textures::atlas_textures;

use std::collections::HashMap;

/// `BlockEntityRenderer.getViewDistance`'s default, in blocks.
pub const VIEW_DISTANCE: f32 = 64.0;

/// Every block entity's animation state, kept across frames.
#[derive(Default)]
pub struct BlockEntities {
    anims: HashMap<[i32; 3], Anim>,
    /// Client ticks so far: the banners' and pots' clock.
    ticks: u64,
    /// `RandomSource` for the book's page flips.
    seed: u64,
    /// The positions whose animation ran at the last tick, sorted.
    running: Vec<[i32; 3]>,
    /// Bumped whenever a block entity starts or stops moving.
    still_revision: u64,
}

impl BlockEntities {
    /// Changes whenever [`Self::build_still`] could build something else.
    pub fn still_revision(&self) -> u64 {
        self.still_revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::model_part::{Sprite, emit};
    use anim::Lid;
    use glam::Mat4;
    use models::{ChestHalf, SkullType};
    use pumpkin_data::Block;

    #[test]
    fn classifies_block_entity_states() {
        let chest = Block::CHEST.default_state.id.as_u16();
        assert!(matches!(
            kind_of(chest),
            Some(Kind::Chest {
                texture: "normal",
                half: ChestHalf::Single,
                ..
            })
        ));
        let red_banner = Block::RED_BANNER.default_state.id.as_u16();
        assert!(matches!(
            kind_of(red_banner),
            Some(Kind::Banner {
                color: 14,
                placement: Placement::Ground(_)
            })
        ));
        let wall_head = Block::CREEPER_WALL_HEAD.default_state.id.as_u16();
        assert!(matches!(
            kind_of(wall_head),
            Some(Kind::Skull {
                skull: SkullType::Creeper,
                placement: Placement::Wall(_),
                ..
            })
        ));
        let blue_box = Block::BLUE_SHULKER_BOX.default_state.id.as_u16();
        assert!(
            matches!(kind_of(blue_box), Some(Kind::Shulker { texture, .. }) if texture == "minecraft:entity/shulker/shulker_blue")
        );
        assert!(kind_of(Block::STONE.default_state.id.as_u16()).is_none());
        assert!(kind_of(Block::BELL.default_state.id.as_u16()).is_some());
    }

    #[test]
    fn a_chest_lid_eases_open_and_shut() {
        let mut lid = Lid {
            open: true,
            ..Lid::default()
        };
        for _ in 0..10 {
            lid.tick();
        }
        assert_eq!(lid.openness, 1.0);
        lid.open = false;
        lid.tick();
        assert!((lid.at(0.5) - 0.95).abs() < 1e-5);
    }

    #[test]
    fn an_opening_chest_moves_until_its_lid_is_up() {
        use crate::world::chunk::{Chunk, Section};
        use crate::world::level::Level;
        use glam::Vec3;

        let mut level = Level::default();
        level.insert(Chunk::new(
            0,
            0,
            (0..24).map(|_| Section::empty()).collect(),
        ));
        let pos = [1, 64, 2];
        level.set_block(pos, Block::CHEST.default_state.id.as_u16(), false);
        let kind = level.block_state(1, 64, 2).and_then(kind_of).unwrap();
        let far = Vec3::splat(1000.0);
        let mut entities = BlockEntities::default();
        assert!(!entities.is_moving(pos, kind));

        let revision = entities.still_revision();
        entities.event(&level, pos, 1, 1);
        entities.tick(&level, far);
        assert!(entities.is_moving(pos, kind));
        assert!(entities.still_revision() > revision);

        for _ in 0..11 {
            entities.tick(&level, far);
        }
        assert!(!entities.is_moving(pos, kind));
        let revision = entities.still_revision();
        entities.tick(&level, far);
        assert_eq!(entities.still_revision(), revision);
    }

    #[test]
    fn a_closed_chest_stays_inside_its_block() {
        let model = models::chest(ChestHalf::Single);
        let sprite = Sprite {
            rect: [0.0, 0.0, 1.0, 1.0],
            size: models::CHEST_TEXTURE,
        };
        let mut out = Vec::new();
        for part in [&model.bottom, &model.lid, &model.lock] {
            emit(part, Mat4::IDENTITY, &sprite, [1.0; 3], &mut out);
        }
        for v in &out {
            for c in v.pos {
                assert!((-1e-4..=1.0 + 1e-4).contains(&c), "{:?}", v.pos);
            }
        }
        let top = out.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max);
        assert!((top - 14.0 / 16.0).abs() < 1e-4);
    }
}
