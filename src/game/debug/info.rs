//! The F3 lines that need the running game: the server line, what is looked

use pumpkin_data::tag::{self, RegistryKey, Taggable};
use pumpkin_data::{Block, BlockStateId};

use crate::game::InWorld;
use crate::gui::hud::debug::DebugExtras;
use crate::gui::hud::debug_entries::Entry;
use crate::world::clouds::CloudStatus;

/// A block state as `BlockState.toString` says it: the block's id and its
pub fn describe_state(state: u16) -> (String, Vec<String>) {
    let Some(id) = BlockStateId::new(state) else {
        return ("minecraft:air".to_owned(), Vec::new());
    };
    let block = Block::from_state_id(id);
    let properties = block
        .properties(id)
        .map(|p| {
            p.to_props()
                .into_iter()
                .map(|prop| format!("{}: {}", prop.key_str(), prop.value_str()))
                .collect()
        })
        .unwrap_or_default();
    (format!("minecraft:{}", block.name), properties)
}

/// `ClipboardCommand` of F3+I for a block: the `/setblock` that makes it.
pub fn setblock_command(pos: [i32; 3], state: u16) -> String {
    let (name, properties) = describe_state(state);
    let properties = if properties.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = properties.iter().map(|p| p.replace(": ", "=")).collect();
        format!("[{}]", list.join(","))
    };
    format!(
        "/setblock {} {} {} {name}{properties}",
        pos[0], pos[1], pos[2]
    )
}

/// `addStateProperties`: a `true` green, a `false` red.
fn colored_properties(properties: Vec<String>) -> Vec<String> {
    properties
        .into_iter()
        .map(|p| match p.split_once(": ") {
            Some((key, "true")) => format!("{key}: §atrue"),
            Some((key, "false")) => format!("{key}: §cfalse"),
            _ => p,
        })
        .collect()
}

/// `DebugEntryLookingAt.addTagEntries`: `#namespace:path` of each tag of
/// `key` that holds `id`, sorted.
fn tags_of(key: RegistryKey, id: u16) -> Vec<String> {
    let mut tags: Vec<String> = tag::get_latest_map(key)
        .entries()
        .filter(|(_, tag)| tag.1.contains(&id))
        .map(|(name, _)| {
            if name.contains(':') {
                format!("#{name}")
            } else {
                format!("#minecraft:{name}")
            }
        })
        .collect();
    tags.sort();
    tags
}

/// A block state's fluid as vanilla names it (`minecraft:water`, its
/// flowing form, or `minecraft:empty`), its properties, and its registry
/// id for the tags.
fn describe_fluid(state: u16) -> (String, Vec<String>, u16) {
    let Some(fluid) = crate::world::fluid::fluid_of(state) else {
        return ("minecraft:empty".to_owned(), Vec::new(), 0);
    };
    let (name, id) = match fluid.kind {
        crate::world::fluid::FluidKind::Water => ("water", 2),
        crate::world::fluid::FluidKind::Lava => ("lava", 4),
    };
    let falling = format!("falling: {}", fluid.falling);
    if fluid.amount == 8 && !fluid.falling {
        (format!("minecraft:{name}"), vec![falling], id)
    } else {
        let level = format!("level: {}", fluid.amount);
        (
            format!("minecraft:flowing_{name}"),
            vec![falling, level],
            id - 1,
        )
    }
}

/// `DebugEntryHeightmap`'s client line: one above the top block of each
/// kind in the column, the bottom where there is none.
fn client_heightmaps(level: &crate::world::level::Level, x: i32, z: i32) -> String {
    let (min_y, sections) = level.height();
    let top = min_y + sections * 16;
    let height = |matches: &dyn Fn(u16) -> bool| {
        (min_y..top)
            .rev()
            .find(|&y| matches(level.block_state(x, y, z).unwrap_or(0)))
            .map_or(min_y, |y| y + 1)
    };
    let block = |id: u16| Block::from_state_id(BlockStateId::new_or_air(id));
    let fluid = |id: u16| crate::world::fluid::fluid_of(id).is_some();
    let surface = height(&|id| !block(id).default_state.is_air());
    let motion = height(&|id| {
        block(id).has_tag(&tag::Block::MINECRAFT_BLOCKS_MOTION_IN_HEIGHTMAP) || fluid(id)
    });
    let no_leaves = height(&|id| {
        block(id).has_tag(&tag::Block::MINECRAFT_BLOCKS_MOTION_IN_HEIGHTMAP_NO_LEAVES) || fluid(id)
    });
    format!("CH S: {surface} M: {motion} ML: {no_leaves}")
}

impl InWorld {
    /// The game's F3 lines, each worked out only when its entry is on.
    pub(in crate::game) fn debug_extras(
        &self,
        clouds: CloudStatus,
        biome_blend: u8,
    ) -> DebugExtras {
        let entries = self.debug.entries();
        let on = |entry: Entry| entries.is_enabled(entry);
        let mut extras = DebugExtras::default();
        if on(Entry::Tps) {
            let (sent, received) = self.subsystems.packets();
            extras.tps = Some(if self.is_integrated() {
                match self.server_mspt() {
                    Some(mspt) => format!(
                        "Integrated server @ {mspt:.1}/50.0 ms, {sent:.0} tx, {received:.0} rx"
                    ),
                    None => format!("Integrated server, {sent:.0} tx, {received:.0} rx"),
                }
            } else {
                let brand = self.registries.brand.as_deref().unwrap_or("unknown");
                format!("\"{brand}\" server, {sent:.0} tx, {received:.0} rx")
            });
        }
        if on(Entry::SimplePerformanceImpactors) {
            extras.impactors.push(format!(
                "{}B: {biome_blend}",
                match clouds {
                    CloudStatus::Off => "",
                    CloudStatus::Fast => "fast-clouds ",
                    CloudStatus::Fancy => "fancy-clouds ",
                }
            ));
            extras
                .impactors
                .push("Terrain Rendering: multidrawindirect".to_owned());
        }

        let feet = self.player.pos();
        let (bx, by, bz) = (
            feet.x.floor() as i32,
            feet.y.floor() as i32,
            feet.z.floor() as i32,
        );
        if on(Entry::PlayerSpeed) {
            extras.speed = Some(format!(
                "Speed: {:.3} blocks/tick",
                self.player.velocity().length()
            ));
        }
        // `cameraEntity.pick(20.0, 0.0F, fluids)`: further than the reach.
        let eye = self.player.tick_eye();
        let reach = eye + self.player.forward_vector().as_dvec3() * 20.0;
        if (on(Entry::LookingAtBlockState) || on(Entry::LookingAtBlockTags))
            && let Some(hit) = crate::world::pick::clip_blocks(&self.level, eye, reach)
        {
            let [x, y, z] = hit.pos;
            let state = self.level.block_state(x, y, z).unwrap_or(0);
            if on(Entry::LookingAtBlockState) {
                let (name, properties) = describe_state(state);
                extras
                    .block
                    .push(format!("§nTargeted Block: {x}, {y}, {z}"));
                extras.block.push(name);
                extras.block.extend(colored_properties(properties));
            }
            if on(Entry::LookingAtBlockTags) {
                let block = Block::from_state_id(BlockStateId::new_or_air(state));
                extras.block_tags = tags_of(RegistryKey::Block, block.registry_id());
            }
        }
        if (on(Entry::LookingAtFluidState) || on(Entry::LookingAtFluidTags))
            && let Some(hit) = crate::world::pick::clip_blocks_and_fluids(&self.level, eye, reach)
        {
            let [x, y, z] = hit.pos;
            let state = self.level.block_state(x, y, z).unwrap_or(0);
            let (name, properties, id) = describe_fluid(state);
            if on(Entry::LookingAtFluidState) {
                extras
                    .fluid
                    .push(format!("§nTargeted Fluid: {x}, {y}, {z}"));
                extras.fluid.push(name);
                extras.fluid.extend(colored_properties(properties));
            }
            if on(Entry::LookingAtFluidTags) {
                extras.fluid_tags = tags_of(RegistryKey::Fluid, id);
            }
        }
        if let Some(entity) = self.hit_entity().and_then(|hit| self.entities.get(hit.id)) {
            if on(Entry::LookingAtEntity) {
                extras.entity = vec![
                    "§nTargeted Entity".to_owned(),
                    format!("minecraft:{}", entity.ty.resource_name),
                ];
            }
            if on(Entry::LookingAtEntityTags) {
                extras.entity_tags = tags_of(RegistryKey::EntityType, entity.ty.id);
            }
        }
        if on(Entry::Biome)
            && let Some(biome) = self.level.biome(bx, by, bz)
        {
            let name = self
                .level_info
                .biomes
                .name(biome)
                .unwrap_or("[unregistered]");
            extras.biome = Some(format!("Biome: {name}"));
        }
        if on(Entry::LightLevels) {
            let (block, sky) = self.level.light_at(bx, by, bz);
            extras.light = Some(format!(
                "Client Light: {} ({sky} sky, {block} block)",
                block.max(sky)
            ));
        }
        let server = if on(Entry::Heightmap)
            || on(Entry::LocalDifficulty)
            || on(Entry::EntitySpawnCounts)
            || on(Entry::ChunkGenerationStats)
        {
            self.server_world()
        } else {
            None
        };
        if on(Entry::Heightmap) && self.level.block_state(bx, by, bz).is_some() {
            // The server's heightmaps: Pumpkin's, in singleplayer.
            extras.heightmaps = vec![
                client_heightmaps(&self.level, bx, bz),
                match &server {
                    Some(world) => super::server::server_heightmaps(world, bx, bz),
                    None => "SH S: ?? O: ?? M: ?? ML: ??".to_owned(),
                },
            ];
        }
        if let Some(world) = &server {
            let at = feet.to_array();
            if on(Entry::LocalDifficulty) {
                extras.local_difficulty = super::server::local_difficulty(world, at);
            }
            if on(Entry::EntitySpawnCounts) {
                extras.spawn_counts = super::server::spawn_counts(world);
            }
            if on(Entry::ChunkGenerationStats) {
                // `DebugEntryChunkGeneration`: sampled again only when the
                // feet move to another block.
                let pos = [bx, by, bz];
                let mut cache = self.chunk_generation.borrow_mut();
                if cache.as_ref().is_none_or(|(at, _)| *at != pos) {
                    *cache = Some((pos, super::server::chunk_generation(world, pos)));
                }
                if let Some((_, lines)) = cache.as_ref() {
                    extras.chunk_generation = lines.clone();
                }
            }
        }
        if on(Entry::SoundMood) {
            let [used, streaming] = self.audio.channels;
            extras.sound_mood = Some(format!(
                "Sounds: {used}/{} + {streaming}/{} (Mood {}%)",
                crate::audio::stats::STATIC_CHANNELS,
                crate::audio::stats::STREAMING_CHANNELS,
                (self.mood() * 100.0 + 0.5).floor() as i32,
            ));
        }
        if on(Entry::SoundCache) {
            // Whole MiB, rounded up.
            let (buffers, bytes) = self.audio.cache;
            extras.sound_cache = Some(format!(
                "Sound cache: {buffers} buffers, {} MiB",
                bytes.div_ceil(1024 * 1024)
            ));
        }
        if on(Entry::ChunkSourceStats) {
            extras.chunk_source = Some(format!(
                "Chunks[C] W: {} E: {}",
                self.level.chunk_count(),
                self.entities.len()
            ));
        }
        extras
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluids_read_as_vanilla_names_them() {
        let water = Block::WATER.default_state.id.as_u16();
        assert_eq!(
            describe_fluid(water),
            (
                "minecraft:water".to_owned(),
                vec!["falling: false".to_owned()],
                2
            )
        );
        let (name, properties, id) = describe_fluid(water + 3);
        assert_eq!((name.as_str(), id), ("minecraft:flowing_water", 1));
        assert_eq!(properties, ["falling: false", "level: 5"]);
        let stone = Block::STONE.default_state.id.as_u16();
        assert_eq!(describe_fluid(stone).0, "minecraft:empty");
        let tags = tags_of(RegistryKey::Fluid, 2);
        assert!(tags.contains(&"#minecraft:water".to_owned()), "{tags:?}");
        assert!(tags_of(RegistryKey::Fluid, 0).is_empty());
    }

    #[test]
    fn booleans_are_green_and_red_and_tags_sorted() {
        let colored = colored_properties(vec![
            "lit: true".into(),
            "open: false".into(),
            "facing: north".into(),
        ]);
        assert_eq!(colored, ["lit: §atrue", "open: §cfalse", "facing: north"]);
        let tags = tags_of(RegistryKey::Block, Block::OAK_LOG.registry_id());
        assert!(tags.contains(&"#minecraft:logs".to_owned()), "{tags:?}");
        assert!(tags.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn a_state_reads_as_its_id_and_properties() {
        let (name, properties) = describe_state(Block::STONE.default_state.id.as_u16());
        assert_eq!(name, "minecraft:stone");
        assert!(properties.is_empty());
        let furnace = Block::FURNACE.default_state.id.as_u16();
        let (name, properties) = describe_state(furnace);
        assert_eq!(name, "minecraft:furnace");
        assert!(
            properties.iter().any(|p| p.starts_with("facing: ")),
            "{properties:?}"
        );
    }

    #[test]
    fn a_setblock_command_names_the_properties() {
        let command = setblock_command([1, 64, -2], Block::FURNACE.default_state.id.as_u16());
        assert!(
            command.starts_with("/setblock 1 64 -2 minecraft:furnace["),
            "{command}"
        );
        assert!(command.contains("facing=north"), "{command}");
        assert_eq!(
            setblock_command([0, 0, 0], Block::STONE.default_state.id.as_u16()),
            "/setblock 0 0 0 minecraft:stone"
        );
    }
}
