//! The client's own light engine (vanilla `LevelLightEngine` on the client).

use std::collections::VecDeque;
use std::sync::Arc;

use pumpkin_data::BlockStateId;

use super::Level;
use crate::math::{SectionSet, section_coord};

/// The six neighbours, `(dx, dy, dz)`; index 0 is straight down.
const STEPS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// How much light a state takes away (0 clear, 15 opaque).
fn opacity(state: u16) -> u8 {
    BlockStateId::new(state).map_or(0, |id| id.to_state().opacity)
}

/// The block light a state gives off.
fn emission(state: u16) -> u8 {
    BlockStateId::new(state).map_or(0, |id| id.to_state().luminance)
}

/// `LightEngine.hasDifferentLightProperties`.
fn changes_light(old: u16, new: u16) -> bool {
    opacity(old) != opacity(new) || emission(old) != emission(new)
}

impl Level {
    /// Lights the world again after the block at `pos` went from `old` to `state`.
    pub(super) fn update_light(&mut self, pos: [i32; 3], old: u16, state: u16) -> SectionSet {
        let mut touched = SectionSet::default();
        if !changes_light(old, state) {
            return touched;
        }
        self.update_layer(false, pos, emission(state), &mut touched);
        if self.has_skylight {
            self.update_layer(true, pos, 0, &mut touched);
        }
        touched
    }

    /// One layer: takes away the light that came through `pos`, gives `pos`
    fn update_layer(&mut self, sky: bool, pos: [i32; 3], source: u8, touched: &mut SectionSet) {
        let Some(old) = self.light(sky, pos) else {
            return;
        };
        let mut darken = VecDeque::new();
        let mut brighten = VecDeque::new();
        self.set_light(sky, pos, 0, touched);
        darken.push_back((pos, old));

        while let Some((at, level)) = darken.pop_front() {
            for (i, step) in STEPS.iter().enumerate() {
                let next = add(at, *step);
                let Some(there) = self.light(sky, next) else {
                    continue;
                };
                if there == 0 {
                    continue;
                }
                let fed_by_us = there < level || (sky && i == 0 && level == 15 && there == 15);
                if fed_by_us && self.settable(next) {
                    self.set_light(sky, next, 0, touched);
                    darken.push_back((next, there));
                } else {
                    brighten.push_back(next);
                }
            }
        }

        if source > 0 {
            self.set_light(sky, pos, source, touched);
            brighten.push_back(pos);
        }
        for step in STEPS {
            let next = add(pos, step);
            if self.light(sky, next).is_some_and(|level| level > 0) {
                brighten.push_back(next);
            }
        }

        while let Some(at) = brighten.pop_front() {
            let Some(level) = self.light(sky, at) else {
                continue;
            };
            if level <= 1 {
                continue;
            }
            for (i, step) in STEPS.iter().enumerate() {
                let next = add(at, *step);
                if !self.settable(next) {
                    continue;
                }
                let Some(there) = self.light(sky, next) else {
                    continue;
                };
                let block = self.block_state(next[0], next[1], next[2]).unwrap_or(0);
                let opacity = opacity(block);
                let arriving = if sky && i == 0 && level == 15 && opacity == 0 {
                    15
                } else {
                    level.saturating_sub(opacity.max(1))
                };
                if arriving > there {
                    self.set_light(sky, next, arriving, touched);
                    brighten.push_back(next);
                }
            }
        }
    }

    /// The light at `pos`.
    fn light(&self, sky: bool, [x, y, z]: [i32; 3]) -> Option<u8> {
        let loaded = self.chunks.get(&(section_coord(x), section_coord(z)))?;
        if y >= self.height.light_max_y() {
            return Some(if sky && self.has_skylight { 15 } else { 0 });
        }
        loaded.chunk.light.layer_at(sky, x, y, z)
    }

    /// Whether light at `pos` is stored (and so can change).
    fn settable(&self, [x, y, z]: [i32; 3]) -> bool {
        (self.height.light_min_y()..self.height.light_max_y()).contains(&y)
            && self
                .chunks
                .contains_key(&(section_coord(x), section_coord(z)))
    }

    fn set_light(&mut self, sky: bool, [x, y, z]: [i32; 3], value: u8, touched: &mut SectionSet) {
        let key = (section_coord(x), section_coord(z));
        let Some(loaded) = self.chunks.get_mut(&key) else {
            return;
        };
        if loaded.chunk.light.layer_at(sky, x, y, z) == Some(value) {
            return;
        }
        Arc::make_mut(&mut Arc::make_mut(&mut loaded.chunk).light).set_layer(sky, x, y, z, value);
        super::sections_around([x, y, z], self.height.min_y, touched);
    }
}

fn add([x, y, z]: [i32; 3], [dx, dy, dz]: [i32; 3]) -> [i32; 3] {
    [x + dx, y + dy, z + dz]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, ChunkLight, Section};
    use crate::dimension::Height;

    const STONE: u16 = 1;

    /// Air over a stone floor at Y 63, lit as the server would.
    fn flat_world() -> Level {
        let mut level = Level::default();
        let min_y = Height::OVERWORLD.min_y;
        for cx in -1..=1 {
            for cz in -1..=1 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                let mut light = ChunkLight::default();
                for y in min_y..Height::OVERWORLD.light_max_y() {
                    for z in 0..16 {
                        for x in 0..16 {
                            if (min_y..64).contains(&y) {
                                let s = ((y - min_y) / 16) as usize;
                                sections[s].set(Section::index_of_block(x, y, z), STONE);
                            } else {
                                light.set_layer(true, x, y, z, 15);
                            }
                        }
                    }
                }
                level.insert(Chunk::new(cx, cz, sections).with_light(light));
            }
        }
        level
    }

    fn sky(level: &Level, pos: [i32; 3]) -> u8 {
        level.light(true, pos).unwrap()
    }

    #[test]
    fn digging_a_shaft_lets_sky_light_down() {
        let mut level = flat_world();
        for y in (50..64).rev() {
            level.set_block([8, y, 8], 0, false);
        }
        assert_eq!(sky(&level, [8, 50, 8]), 15);
        level.set_block([9, 50, 8], 0, false);
        assert_eq!(sky(&level, [9, 50, 8]), 14);
    }

    #[test]
    fn covering_a_shaft_darkens_it() {
        let mut level = flat_world();
        for y in (50..64).rev() {
            level.set_block([8, y, 8], 0, false);
        }
        level.set_block([8, 64, 8], STONE, false);
        assert_eq!(sky(&level, [8, 63, 8]), 0);
        assert_eq!(sky(&level, [8, 50, 8]), 0);
    }

    #[test]
    fn a_torch_lights_its_surroundings_and_takes_it_back() {
        let mut level = flat_world();
        let torch = pumpkin_data::Block::TORCH.default_state.id.as_u16();
        level.set_block([8, 64, 8], torch, false);
        assert_eq!(level.light(false, [8, 64, 8]), Some(14));
        assert_eq!(level.light(false, [10, 64, 8]), Some(12));
        level.set_block([8, 64, 8], 0, false);
        assert_eq!(level.light(false, [8, 64, 8]), Some(0));
        assert_eq!(level.light(false, [10, 64, 8]), Some(0));
    }

    #[test]
    fn light_crosses_chunk_borders() {
        let mut level = flat_world();
        let torch = pumpkin_data::Block::TORCH.default_state.id.as_u16();
        level.set_block([15, 64, 8], torch, false);
        assert_eq!(level.light(false, [17, 64, 8]), Some(12));
    }
}
