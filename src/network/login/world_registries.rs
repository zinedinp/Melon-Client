//! The registries that decide what a level looks like.

use anyhow::Result;
use pumpkin_protocol::ser::{NetworkReadSliceExt, nbt_tag_to_json_ext};
use pumpkin_util::version::JavaMinecraftVersion;
use serde_json::Value;

use crate::network::protocol::packet::Reader;
use crate::world::biome::{Biome, Biomes};
use crate::world::dimension::{DimensionType, DimensionTypes};

const VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

#[derive(Clone, Debug, Default)]
pub struct WorldRegistries {
    pub dimension_types: DimensionTypes,
    pub biomes: Biomes,
    /// `minecraft:damage_type`: each type's `effects`, by registry id.
    pub damage_effects: std::sync::Arc<[String]>,
}

impl WorldRegistries {
    /// Takes in a registry data packet if it is one of those kept.
    pub fn read_packet(&mut self, body: &[u8]) -> Result<()> {
        let mut r = Reader::new(body);
        match r.string()?.as_str() {
            "minecraft:dimension_type" => self.dimension_types = read_dimension_types(&mut r)?,
            "minecraft:worldgen/biome" => self.biomes = read_biomes(&mut r)?,
            "minecraft:damage_type" => self.damage_effects = read_damage_effects(&mut r)?,
            _ => {}
        }
        Ok(())
    }
}

fn read_dimension_types(r: &mut Reader) -> Result<DimensionTypes> {
    let count = r.var_int()?.clamp(0, 1 << 12) as usize;
    let mut types = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.string()?;
        let base = DimensionType::built_in(&name).unwrap_or_else(DimensionType::overworld);
        let parsed = match read_entry(r)? {
            Some(json) => DimensionType::from_json(&json, &base).ok(),
            None => None,
        };
        types.push((name, parsed.unwrap_or(base)));
    }
    Ok(DimensionTypes::new(types))
}

fn read_damage_effects(r: &mut Reader) -> Result<std::sync::Arc<[String]>> {
    let count = r.var_int()?.clamp(0, 1 << 12) as usize;
    let mut effects = Vec::with_capacity(count);
    for _ in 0..count {
        let _name = r.string()?;
        let json = read_entry(r)?;
        let effect = json
            .as_ref()
            .and_then(|j| j.get("effects"))
            .and_then(Value::as_str)
            .unwrap_or("hurt");
        effects.push(effect.to_string());
    }
    Ok(effects.into())
}

fn read_biomes(r: &mut Reader) -> Result<Biomes> {
    let count = r.var_int()?.clamp(0, 1 << 12) as usize;
    let mut biomes = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.string()?;
        let biome = read_entry(r)?.map_or_else(Biome::default, |json| Biome::from_json(&json));
        biomes.push((name, biome));
    }
    Ok(Biomes::new(biomes))
}

/// A registry entry's optional data, as JSON.
fn read_entry(r: &mut Reader) -> Result<Option<Value>> {
    if !r.bool()? {
        return Ok(None);
    }
    let tag = r
        .remaining()
        .get_nbt(&VERSION)
        .map_err(|e| anyhow::anyhow!("registry entry NBT: {e}"))?;
    Ok(tag.map(|tag| nbt_tag_to_json_ext(&tag, true)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::network::protocol::{packet, varint};
    use crate::world::dimension::{Height, rgb};

    /// The synced registry as the server's data holds it, as a packet.
    pub(crate) fn registry_packet(registry: &str) -> Vec<u8> {
        let all = pumpkin_data::registry::Registry::get_synced(VERSION);
        let reg = all
            .iter()
            .find(|r| r.registry_id == registry)
            .expect("the registry exists");
        let mut body = Vec::new();
        packet::write_string(&mut body, registry).unwrap();
        varint::write_var_int(&mut body, reg.registry_entries.len() as i32).unwrap();
        for entry in &reg.registry_entries {
            packet::write_string(&mut body, &entry.entry_id).unwrap();
            body.push(1);
            body.extend_from_slice(entry.data.as_deref().unwrap());
        }
        body
    }

    #[test]
    fn the_servers_biomes_bring_fog_and_water_colours() {
        let mut registries = WorldRegistries::default();
        registries
            .read_packet(&registry_packet("minecraft:worldgen/biome"))
            .unwrap();
        let biomes = &registries.biomes;
        let named = |name: &str| {
            (0..128)
                .find(|&i| biomes.name(i) == Some(name))
                .map(|i| biomes.get(i))
                .unwrap_or_else(|| panic!("{name} missing"))
        };
        assert_eq!(
            named("minecraft:nether_wastes").visuals.fog_color,
            Some(rgb(0x330808))
        );
        assert_eq!(
            named("minecraft:crimson_forest").visuals.fog_color,
            Some(rgb(0x330303))
        );
        assert_eq!(named("minecraft:warm_ocean").water_color, rgb(0x43d5ee));
        assert_eq!(
            named("minecraft:plains").visuals.sky_color,
            Some(rgb(0x78a7ff))
        );
        assert_eq!(
            named("minecraft:swamp").visuals.water_fog_end_scale,
            Some(0.85)
        );
    }

    #[test]
    fn the_servers_dimension_types_give_each_level_its_height() {
        let mut registries = WorldRegistries::default();
        registries
            .read_packet(&registry_packet("minecraft:dimension_type"))
            .unwrap();
        let types = &registries.dimension_types;
        let named = |name: &str| {
            (0..8)
                .find(|&i| types.name(i) == Some(name))
                .map(|i| types.get(i))
                .unwrap_or_else(|| panic!("{name} missing"))
        };
        assert_eq!(named("minecraft:overworld").height, Height::OVERWORLD);
        let nether = named("minecraft:the_nether");
        assert_eq!(nether.height, Height::NETHER);
        assert!(!nether.has_skylight);
        assert_eq!(nether.attributes.fog_end_distance, Some(96.0));
        assert_eq!(nether.attributes.fog_start_distance, Some(10.0));
        let end = named("minecraft:the_end");
        assert!(end.has_skylight && end.fixed_time);
        assert_eq!(end.attributes.sky_color, Some([0.0; 3]));
    }
}
