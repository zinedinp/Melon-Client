//! Decodes `LevelChunkWithLight`, for the dimension's height.

use crate::network::protocol::packet::Reader;
use crate::world::biome::{ChunkBiomes, SectionBiomes};
use crate::world::chunk::{
    BLOCKS_PER_SECTION, Chunk, ChunkLight, ChunkScan, LIGHT_NIBBLES, LightSection, LightUpdate,
    Section,
};
use crate::world::dimension::Height;
use crate::world::head_profile::{HeadProfile, is_player_head};
use anyhow::{Result, bail};
use pumpkin_protocol::ser::NetworkReadSliceExt;
use std::sync::Arc;

const BIOMES_PER_SECTION: usize = 4 * 4 * 4;
/// `Strategy.createForBlockStates`.
const BLOCK_INDIRECT_MAX_BITS: u32 = 8;
/// `Strategy.createForBiomes`: bits 1..=3 stay indirect.
const BIOME_INDIRECT_MAX_BITS: u32 = 3;
/// `Strategy.createForBlockStates`: indirect palettes of 1..=4 bits take 4.
const BLOCK_INDIRECT_MIN_BITS: u32 = 4;
/// Global ids are `u16` here; a wider index cannot name a state or biome.
const GLOBAL_MAX_BITS: u32 = 16;

/// Reads one `LevelChunkWithLight` packet body of a level `height` tall:
pub fn decode(body: &[u8], height: Height) -> Result<Chunk> {
    let mut r = Reader::new(body);
    let x = r.i32_be()?;
    let z = r.i32_be()?;

    let heightmap_count = r.var_int()?;
    for _ in 0..heightmap_count {
        let _kind = r.var_int()?;
        let Some(bytes) = usize::try_from(r.var_int()?)
            .ok()
            .and_then(|longs| longs.checked_mul(8))
        else {
            bail!("bad heightmap length");
        };
        r.skip(bytes)?;
    }

    let data_len = r.var_int()? as usize;
    let mut data = r.take(data_len)?;

    let mut scan = ChunkScan::default();
    let mut sections = Vec::with_capacity(height.sections());
    let mut biomes = Vec::with_capacity(height.sections());
    for s in 0..height.sections() {
        let _non_empty_block_count = data.i16_be()?;
        let _liquid_count = data.i16_be()?;

        let mut blocks = Box::new([0u16; BLOCKS_PER_SECTION]);
        read_paletted(&mut data, &mut blocks[..], BLOCK_INDIRECT_MAX_BITS, |_| {
            false
        })?;
        if crate::probe::enabled() {
            probe_states(&blocks[..]);
        }
        let section = Arc::new(Section::from_array(&blocks));
        scan.add_section(x, z, height.min_y, s, &section);
        sections.push(section);

        let mut cells = [0u16; BIOMES_PER_SECTION];
        read_paletted(&mut data, &mut cells, BIOME_INDIRECT_MAX_BITS, |_| false)?;
        biomes.push(SectionBiomes::from_cells(cells));
    }

    let mut chunk = Chunk::from_shared(
        x,
        z,
        height,
        sections,
        ChunkBiomes { sections: biomes },
        scan,
    );
    if r.remaining().is_empty() {
        return Ok(chunk);
    }
    chunk.scan.heads = read_block_entities(&mut r, &chunk)?;
    let mut light = ChunkLight::new(height);
    light.apply(&decode_light(&mut r)?);
    chunk = chunk.with_light(light);
    Ok(chunk)
}

/// `ClientboundChunksBiomesPacket` (`/fillbiome`).
pub fn decode_biomes(body: &[u8], height: Height) -> Result<Vec<(i32, i32, ChunkBiomes)>> {
    let mut r = Reader::new(body);
    let count = r.var_int()?;
    if !(0..=65_536).contains(&count) {
        bail!("{count} chunk biome entries");
    }
    let mut columns = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key = r.i64_be()?;
        let len = r.var_int()?;
        if !(0..=2_097_152).contains(&len) {
            bail!("biome buffer of {len} bytes");
        }
        let mut data = r.take(len as usize)?;
        let mut sections = Vec::with_capacity(height.sections());
        for _ in 0..height.sections() {
            let mut cells = [0u16; BIOMES_PER_SECTION];
            read_paletted(&mut data, &mut cells, BIOME_INDIRECT_MAX_BITS, |_| false)?;
            sections.push(SectionBiomes::from_cells(cells));
        }
        columns.push((key as i32, (key >> 32) as i32, ChunkBiomes { sections }));
    }
    Ok(columns)
}

/// `ClientboundLightUpdatePacketData`.
pub fn decode_light(r: &mut Reader) -> Result<LightUpdate> {
    Ok(LightUpdate {
        sky_mask: read_bit_set(r)?,
        block_mask: read_bit_set(r)?,
        empty_sky_mask: read_bit_set(r)?,
        empty_block_mask: read_bit_set(r)?,
        sky: read_data_layers(r)?,
        block: read_data_layers(r)?,
    })
}

/// `ByteBufCodecs.BIT_SET`: VarInt byte length, then `BitSet.toByteArray()`
fn read_bit_set(r: &mut Reader) -> Result<Vec<u64>> {
    let len = r.var_int()?;
    if len < 0 {
        bail!("negative BitSet length");
    }
    let bytes = r.slice(len as usize)?;
    let mut words = vec![0u64; bytes.len().div_ceil(8)];
    for (i, &byte) in bytes.iter().enumerate() {
        words[i / 8] |= u64::from(byte) << ((i % 8) * 8);
    }
    Ok(words)
}

/// Each array straight into its [`LightSection`].
fn read_data_layers(r: &mut Reader) -> Result<Vec<LightSection>> {
    let count = r.var_int()?;
    if count < 0 {
        bail!("negative light-array count");
    }
    // Only the up-front reservation is capped (vanilla: `Math.min(count, 65536)`);
    // a lying count still fails on the first missing array.
    let mut arrays = Vec::with_capacity((count as usize).min(4096));
    for _ in 0..count {
        let len = r.var_int()?;
        if !(0..=LIGHT_NIBBLES as i32).contains(&len) {
            bail!("light array of {len} bytes");
        }
        arrays.push(LightSection::from_bytes(r.slice(len as usize)?));
    }
    Ok(arrays)
}

/// `ClientboundLevelChunkPacketData.BlockEntityInfo` list.
fn read_block_entities(r: &mut Reader, chunk: &Chunk) -> Result<Vec<([i32; 3], HeadProfile)>> {
    let count = r.var_int()?;
    if count < 0 {
        bail!("negative block-entity count");
    }
    let mut heads = Vec::new();
    for _ in 0..count {
        let xz = r.u8()?;
        let y = i32::from(r.i16_be()?);
        r.var_int()?;
        let pos = [
            chunk.x * 16 + i32::from(xz >> 4),
            y,
            chunk.z * 16 + i32::from(xz & 15),
        ];
        if !chunk.block_state(pos).is_some_and(is_player_head) {
            skip_optional_nbt(r)?;
            continue;
        }
        let tag = r
            .remaining()
            .get_nbt(&pumpkin_util::version::JavaMinecraftVersion::V_26_3)
            .map_err(|e| anyhow::anyhow!("block entity NBT: {e}"))?;
        if let Some(profile) = tag.as_ref().and_then(head_of_tag) {
            heads.push((pos, profile));
        }
    }
    Ok(heads)
}

/// The profile in a skull block entity's tag.
pub fn head_of_tag(tag: &pumpkin_nbt::tag::NbtTag) -> Option<HeadProfile> {
    let pumpkin_nbt::tag::NbtTag::Compound(compound) = tag else {
        return None;
    };
    HeadProfile::from_nbt(compound.get("profile")?)
}

/// `NbtIo.readAnyTag`: type 0 is null, else type + unnamed payload.
fn skip_optional_nbt(r: &mut Reader) -> Result<()> {
    let ty = r.u8()?;
    skip_nbt_payload(r, ty, 0)
}

fn skip_nbt_payload(r: &mut Reader, ty: u8, depth: u32) -> Result<()> {
    if depth > 512 {
        bail!("NBT too deep");
    }
    match ty {
        0 => Ok(()),
        1 => r.skip(1),
        2 => r.skip(2),
        3 | 5 => r.skip(4),
        4 | 6 => r.skip(8),
        7 => {
            let len = r.i32_be()?;
            if len < 0 {
                bail!("negative NBT byte array");
            }
            r.skip(len as usize)
        }
        8 => {
            let len = r.i16_be()? as u16 as usize;
            r.skip(len)
        }
        9 => {
            let elem = r.u8()?;
            let len = r.i32_be()?;
            if len < 0 {
                bail!("negative NBT list");
            }
            if elem == 0 && len > 0 {
                bail!("missing type on NBT list");
            }
            for _ in 0..len {
                skip_nbt_payload(r, elem, depth + 1)?;
            }
            Ok(())
        }
        10 => loop {
            let elem = r.u8()?;
            if elem == 0 {
                return Ok(());
            }
            let name_len = r.i16_be()? as u16 as usize;
            r.skip(name_len)?;
            skip_nbt_payload(r, elem, depth + 1)?;
        },
        11 => {
            let len = r.i32_be()?;
            if len < 0 {
                bail!("negative NBT int array");
            }
            r.skip(len as usize * 4)
        }
        12 => {
            let len = r.i32_be()?;
            if len < 0 {
                bail!("negative NBT long array");
            }
            r.skip(len as usize * 8)
        }
        other => bail!("unknown NBT type {other}"),
    }
}

/// The most entries an indirect palette can have: 8 bits per entry.
const MAX_PALETTE: usize = 1 << BLOCK_INDIRECT_MAX_BITS;

/// Decodes one `PalettedContainer`'s network form into `out`:
fn read_paletted(
    r: &mut Reader,
    out: &mut [u16],
    indirect_max_bits: u32,
    wanted: impl Fn(u16) -> bool,
) -> Result<bool> {
    let bits_per_entry = u32::from(r.u8()?);
    let min_bits = if indirect_max_bits == BLOCK_INDIRECT_MAX_BITS {
        BLOCK_INDIRECT_MIN_BITS
    } else {
        1
    };

    if bits_per_entry == 0 {
        let value = r.var_int()? as u16;
        out.fill(value);
        return Ok(wanted(value));
    }

    if bits_per_entry <= indirect_max_bits {
        let palette_len = r.var_int()? as usize;
        if palette_len > MAX_PALETTE {
            bail!("palette of {palette_len} entries");
        }
        let mut palette = [0u16; MAX_PALETTE];
        for entry in &mut palette[..palette_len] {
            *entry = r.var_int()? as u16;
        }
        read_packed_indices(r, out, bits_per_entry.max(min_bits), |index| {
            palette.get(index as usize).copied().unwrap_or(0)
        })?;
        return Ok(palette[..palette_len].iter().any(|&id| wanted(id)));
    }

    if bits_per_entry > GLOBAL_MAX_BITS {
        bail!("{bits_per_entry} bits per global palette entry");
    }
    read_packed_indices(r, out, bits_per_entry, |id| id as u16)?;
    Ok(true)
}

/// `SimpleBitStorage`.
fn read_packed_indices(
    r: &mut Reader,
    out: &mut [u16],
    bits_per_entry: u32,
    map: impl Fn(u64) -> u16,
) -> Result<()> {
    let entries_per_long = 64 / bits_per_entry as usize;
    let mask: u64 = (1u64 << bits_per_entry) - 1;
    for chunk in out.chunks_mut(entries_per_long) {
        let word = r.i64_be()? as u64;
        for (slot, value) in chunk.iter_mut().enumerate() {
            *value = map((word >> (slot as u32 * bits_per_entry)) & mask);
        }
    }
    Ok(())
}

/// The tester: the states a section holds, by block.
fn probe_states(blocks: &[u16]) {
    use pumpkin_data::{Block, BlockStateId};
    let mut seen = std::collections::HashSet::new();
    for &state in blocks {
        if !seen.insert(state) {
            continue;
        }
        let Some(id) = BlockStateId::new(state) else {
            crate::probe::count(
                "block.seen",
                format!("(state {state} outside the registry)"),
            );
            continue;
        };
        let block = Block::from_state_id(id);
        crate::probe::note("block.seen", block.name, || format!("first state {state}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::varint::write_var_int;
    use crate::world::chunk::LIGHT_NIBBLES;

    const OVERWORLD_SECTIONS: usize = Height::OVERWORLD.sections();

    #[test]
    fn chunks_biomes_reads_each_column_and_section() {
        let mut body = Vec::new();
        write_var_int(&mut body, 2).unwrap();
        for (x, z, biome) in [(3i32, -2i32, 5), (-1, 4, 2)] {
            body.extend_from_slice(&((i64::from(z) << 32) | i64::from(x as u32)).to_be_bytes());
            let mut buffer = Vec::new();
            for s in 0..OVERWORLD_SECTIONS {
                buffer.push(0);
                let id = if biome == 5 && s == 1 { 7 } else { biome };
                write_var_int(&mut buffer, id).unwrap();
            }
            write_var_int(&mut body, buffer.len() as i32).unwrap();
            body.extend_from_slice(&buffer);
        }
        let columns = decode_biomes(&body, Height::OVERWORLD).unwrap();
        assert_eq!(columns.len(), 2);
        let (x, z, biomes) = &columns[0];
        assert_eq!((*x, *z), (3, -2));
        assert_eq!(
            (biomes.at(0, 0, 0), biomes.at(5, 20, 9), biomes.at(0, 40, 0)),
            (5, 7, 5)
        );
        let (x, z, biomes) = &columns[1];
        assert_eq!((*x, *z, biomes.at(15, 300, 15)), (-1, 4, 2));
    }

    /// An overworld column's body, see [`body_of`].
    fn body() -> Vec<u8> {
        body_of(OVERWORLD_SECTIONS)
    }

    /// A body of `sections` sections with no heightmaps.
    fn body_of(sections: usize) -> Vec<u8> {
        let mut data = Vec::new();
        for section in 0..sections {
            data.extend_from_slice(&0i16.to_be_bytes());
            data.extend_from_slice(&0i16.to_be_bytes());
            if section == 0 {
                data.push(4);
                write_var_int(&mut data, 2).unwrap();
                write_var_int(&mut data, 1).unwrap();
                write_var_int(&mut data, 10).unwrap();
                let word: u64 = (0..16).map(|slot| (slot % 2) << (slot * 4)).sum();
                for _ in 0..BLOCKS_PER_SECTION / 16 {
                    data.extend_from_slice(&word.to_be_bytes());
                }
            } else {
                data.push(0);
                write_var_int(&mut data, 9).unwrap();
            }
            if section % 2 == 0 {
                data.push(0);
                write_var_int(&mut data, 3).unwrap();
            } else {
                data.push(1);
                write_var_int(&mut data, 2).unwrap();
                write_var_int(&mut data, 0).unwrap();
                write_var_int(&mut data, 5).unwrap();
                data.extend_from_slice(&[0; 8]);
            }
        }
        let mut body = Vec::new();
        body.extend_from_slice(&3i32.to_be_bytes());
        body.extend_from_slice(&(-2i32).to_be_bytes());
        write_var_int(&mut body, 0).unwrap();
        write_var_int(&mut body, data.len() as i32).unwrap();
        body.extend_from_slice(&data);
        body
    }

    #[test]
    fn decodes_single_and_indirect_palettes() {
        let chunk = decode(&body(), Height::OVERWORLD).unwrap();
        assert_eq!((chunk.x, chunk.z), (3, -2));
        assert_eq!(chunk.sections.len(), OVERWORLD_SECTIONS);
        assert_eq!(chunk.sections[0].get(0), 1);
        assert_eq!(chunk.sections[0].get(1), 10);
        assert_eq!(chunk.sections[0].get(BLOCKS_PER_SECTION - 1), 10);
        assert!(chunk.sections[1].iter().all(|id| id == 9));
        assert_eq!(chunk.light.at(0, 64, 0), (0, 0));
    }

    #[test]
    fn biomes_come_per_cell_by_registry_index() {
        let chunk = decode(&body(), Height::OVERWORLD).unwrap();
        assert_eq!(chunk.biomes.sections.len(), OVERWORLD_SECTIONS);
        assert_eq!(chunk.biomes.at(0, 0, 0), 3);
        assert_eq!(chunk.biomes.at(15, 16, 15), 0);
        assert_eq!(chunk.biomes.at(8, 32 + 3, 8), 3);
    }

    #[test]
    fn a_nether_column_has_sixteen_sections_from_zero() {
        let nether = body_of(Height::NETHER.sections());
        let chunk = decode(&nether, Height::NETHER).unwrap();
        assert_eq!(chunk.sections.len(), 16);
        assert_eq!(chunk.min_y(), 0);
        assert_eq!(chunk.sections[0].get(1), 10);
        assert!(decode(&nether, Height::OVERWORLD).is_err());
    }

    /// One section of `blocks` as a `bits`-per-entry container.
    fn write_section(data: &mut Vec<u8>, bits: u32, blocks: &[u16; BLOCKS_PER_SECTION]) {
        data.extend_from_slice(&0i16.to_be_bytes());
        data.extend_from_slice(&0i16.to_be_bytes());
        data.push(bits as u8);
        let indices: Vec<u64> = if bits == 0 {
            write_var_int(data, i32::from(blocks[0])).unwrap();
            Vec::new()
        } else if bits <= BLOCK_INDIRECT_MAX_BITS {
            let mut palette: Vec<u16> = blocks.to_vec();
            palette.sort_unstable();
            palette.dedup();
            write_var_int(data, palette.len() as i32).unwrap();
            for &id in &palette {
                write_var_int(data, i32::from(id)).unwrap();
            }
            let index = |id| palette.iter().position(|&p| p == id).unwrap() as u64;
            blocks.iter().map(|&id| index(id)).collect()
        } else {
            blocks.iter().map(|&id| u64::from(id)).collect()
        };
        if bits > 0 {
            let per_long = 64 / bits as usize;
            for chunk in indices.chunks(per_long) {
                let word: u64 = chunk
                    .iter()
                    .enumerate()
                    .map(|(slot, &v)| v << (slot as u32 * bits))
                    .sum();
                data.extend_from_slice(&word.to_be_bytes());
            }
        }
        data.push(0);
        write_var_int(data, 0).unwrap();
    }

    #[test]
    fn palettes_decide_which_sections_are_searched_without_missing_any() {
        let chest = pumpkin_data::Block::CHEST.default_state.id.as_u16();
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        let mut sections = vec![[0u16; BLOCKS_PER_SECTION]; OVERWORLD_SECTIONS];
        sections[0].fill(stone);
        sections[0][300] = chest;
        sections[1][8] = stone;
        sections[2].fill(chest);
        sections[3][4095] = chest;
        sections[3][17] = stone;
        sections[4].fill(stone);
        let bits = |s: usize| match s {
            0 | 1 => 4,
            2 | 4 => 0,
            3 => 15,
            _ => 0,
        };
        let mut data = Vec::new();
        for (s, blocks) in sections.iter().enumerate() {
            write_section(&mut data, bits(s), blocks);
        }
        let mut body = Vec::new();
        body.extend_from_slice(&5i32.to_be_bytes());
        body.extend_from_slice(&7i32.to_be_bytes());
        write_var_int(&mut body, 0).unwrap();
        write_var_int(&mut body, data.len() as i32).unwrap();
        body.extend_from_slice(&data);

        let chunk = decode(&body, Height::OVERWORLD).unwrap();
        let plain: Vec<Section> = chunk.sections.iter().map(|s| (**s).clone()).collect();
        for (decoded, blocks) in plain.iter().zip(&sections) {
            assert_eq!(&*decoded.to_array(), blocks);
        }
        let full = ChunkScan::of(5, 7, Height::OVERWORLD.min_y, &plain);
        assert_eq!(chunk.scan.block_entities, full.block_entities);
        assert_eq!(full.block_entities.len(), 1 + BLOCKS_PER_SECTION + 1);
    }

    #[test]
    fn small_block_palettes_are_read_as_four_bits_like_vanilla() {
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        let mut blocks = [0u16; BLOCKS_PER_SECTION];
        blocks[5] = stone;
        let mut data = Vec::new();
        write_section(&mut data, 4, &blocks);
        data[4] = 2;
        for _ in 1..OVERWORLD_SECTIONS {
            write_section(&mut data, 0, &[0; BLOCKS_PER_SECTION]);
        }
        let mut body = Vec::new();
        body.extend_from_slice(&[0; 8]);
        write_var_int(&mut body, 0).unwrap();
        write_var_int(&mut body, data.len() as i32).unwrap();
        body.extend_from_slice(&data);
        let chunk = decode(&body, Height::OVERWORLD).unwrap();
        assert_eq!(&*chunk.sections[0].to_array(), &blocks);

        data[4] = 70;
        let mut wide = body[..body.len() - data.len()].to_vec();
        wide.extend_from_slice(&data);
        assert!(
            decode(&wide, Height::OVERWORLD).is_err(),
            "no panic on 70 bits"
        );
    }

    fn write_bitset(out: &mut Vec<u8>, words: &[u64]) {
        let mut bytes = Vec::new();
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let trimmed = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        write_var_int(out, trimmed as i32).unwrap();
        out.extend_from_slice(&bytes[..trimmed]);
    }

    fn write_layers(out: &mut Vec<u8>, layers: &[&[u8]]) {
        write_var_int(out, layers.len() as i32).unwrap();
        for layer in layers {
            write_var_int(out, layer.len() as i32).unwrap();
            out.extend_from_slice(layer);
        }
    }

    #[test]
    fn decodes_sky_light_after_empty_block_entities() {
        let mut body = body();
        write_var_int(&mut body, 0).unwrap();
        write_bitset(&mut body, &[1 << 1]);
        write_bitset(&mut body, &[]);
        write_bitset(&mut body, &[]);
        write_bitset(&mut body, &[]);
        let sky = [0xFFu8; LIGHT_NIBBLES];
        write_layers(&mut body, &[&sky]);
        write_layers(&mut body, &[]);

        let chunk = decode(&body, Height::OVERWORLD).unwrap();
        assert_eq!(chunk.light.at(0, -64, 0), (0, 15));
        assert_eq!(chunk.light.at(15, -49, 15), (0, 15));
        assert_eq!(chunk.light.at(0, 64, 0), (0, 0));
    }

    #[test]
    fn skips_a_block_entity_with_empty_nbt() {
        let mut body = body();
        write_var_int(&mut body, 1).unwrap();
        body.push(0);
        body.extend_from_slice(&64i16.to_be_bytes());
        write_var_int(&mut body, 0).unwrap();
        body.push(0);
        write_bitset(&mut body, &[]);
        write_bitset(&mut body, &[1 << 5]);
        write_bitset(&mut body, &[]);
        write_bitset(&mut body, &[]);
        write_layers(&mut body, &[]);
        let block = [0x77u8; LIGHT_NIBBLES];
        write_layers(&mut body, &[&block]);

        let chunk = decode(&body, Height::OVERWORLD).unwrap();
        assert_eq!(chunk.light.at(0, 0, 0), (7, 0));
    }
}
