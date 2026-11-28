//! What a zip-mod block says about how it is drawn (`render` in its
//! `data/<ns>/blocks/*.json`, see `pumpkin_data::mod_registry::BlockSpec`)
//! beyond its block state: the chunk layer, if not the textures', and
//! whether its faces get corner shading. Solidity, occlusion and light come
//! through the block state like any vanilla block's.

use std::sync::Arc;

use pumpkin_data::mod_registry::{self, RenderHints, RenderLayer};

use super::variants::StateModel;
use super::{BakedQuad, ChunkLayer};

/// `model` with the render hints of state `id` applied; `None` when the
/// state has none (every vanilla state), so the shared quads stay shared.
pub(super) fn apply(id: u16, model: &StateModel<BakedQuad>) -> Option<StateModel<BakedQuad>> {
    let hints = mod_registry::render_hints(id)?;
    Some(model.map(&mut |quads: &Arc<[BakedQuad]>| {
        quads.iter().map(|quad| with_hints(quad, hints)).collect()
    }))
}

fn with_hints(quad: &BakedQuad, hints: RenderHints) -> BakedQuad {
    let mut quad = *quad;
    quad.layer = match hints.layer {
        RenderLayer::Auto => quad.layer,
        RenderLayer::Solid => ChunkLayer::Solid,
        RenderLayer::Cutout => ChunkLayer::Cutout,
        RenderLayer::Translucent => ChunkLayer::Translucent,
    };
    quad.ambient_occlusion &= hints.ambient_occlusion;
    quad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::direction::Direction;
    use crate::block_tint::TintKind;

    fn quad(layer: ChunkLayer) -> BakedQuad {
        BakedQuad {
            cullface: Some(Direction::Up),
            facing: Some(Direction::Up),
            group: Direction::Up.index() as u8,
            layer,
            index: 7,
            emission: 0,
            ambient_occlusion: true,
            tint: TintKind::None,
        }
    }

    #[test]
    fn a_forced_layer_wins_and_auto_keeps_the_textures() {
        let cutout = |layer| RenderHints {
            layer,
            ambient_occlusion: true,
        };
        let solid = quad(ChunkLayer::Solid);
        assert_eq!(
            with_hints(&solid, cutout(RenderLayer::Cutout)).layer,
            ChunkLayer::Cutout
        );
        assert_eq!(
            with_hints(&solid, cutout(RenderLayer::Auto)).layer,
            ChunkLayer::Solid
        );
        let translucent = quad(ChunkLayer::Translucent);
        assert_eq!(
            with_hints(&translucent, cutout(RenderLayer::Solid)).layer,
            ChunkLayer::Solid
        );
    }

    #[test]
    fn ambient_occlusion_can_only_be_turned_off() {
        let off = RenderHints {
            layer: RenderLayer::Auto,
            ambient_occlusion: false,
        };
        let on = RenderHints {
            ambient_occlusion: true,
            ..off
        };
        assert!(!with_hints(&quad(ChunkLayer::Solid), off).ambient_occlusion);
        let mut flat = quad(ChunkLayer::Solid);
        flat.ambient_occlusion = false;
        assert!(!with_hints(&flat, on).ambient_occlusion);
    }

    #[test]
    fn vanilla_states_have_no_hints() {
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        let model = StateModel::Fixed(Arc::from([quad(ChunkLayer::Solid)]));
        assert!(apply(stone, &model).is_none());
    }

    #[test]
    fn a_mod_states_hints_reach_its_quads() {
        let spec: mod_registry::BlockSpec = serde_json::from_str(
            r#"{"render": {"solid": false, "layer": "translucent", "ambient_occlusion": false}}"#,
        )
        .unwrap();
        let block = mod_registry::register_block("melon_test:hinted", &spec).unwrap();
        let id = block.default_state.id.as_u16();
        let model = StateModel::Fixed(Arc::from([quad(ChunkLayer::Solid)]));
        let hinted = apply(id, &model).expect("a mod state has hints");
        let quad = hinted.all_quads().next().unwrap();
        assert_eq!(quad.layer, ChunkLayer::Translucent);
        assert!(!quad.ambient_occlusion);
        assert_eq!(quad.index, 7);
    }
}
