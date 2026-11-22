//! A world's data packs and experiments, as Pumpkin keeps them.

use std::path::{Path, PathBuf};

use pumpkin_data::dimension::Dimension;
use pumpkin_util::world_seed::Seed;
use pumpkin_world::generation::generator::{GeneratorInit, VanillaGenerator};
use pumpkin_world::world_info::anvil::{AnvilLevelInfo, LEVEL_DAT_FILE_NAME};
use pumpkin_world::world_info::{LevelData, WorldInfoWriter};

/// The id Pumpkin always has enabled.
pub const VANILLA: &str = "vanilla";

/// The feature packs Pumpkin loads (`load_bundled_datapacks`).
pub const EXPERIMENTS: [&str; 3] = [
    "trade_rebalance",
    "minecart_improvements",
    "redstone_experiments",
];

/// A data pack found in a world's `datapacks` folder.
pub struct FoundPack {
    /// `file/<folder name>`, what `DataPacks.Enabled` holds.
    pub id: String,
    pub name: String,
    /// `pack.mcmeta`'s `pack.description`.
    pub description: serde_json::Value,
    /// `pack.png`, undecoded.
    pub icon: Option<Vec<u8>>,
}

pub fn folder(world_dir: &Path) -> PathBuf {
    world_dir.join("datapacks")
}

/// The packs in a `datapacks` folder, by id.
pub fn discover(packs_dir: &Path) -> Vec<FoundPack> {
    let Ok(entries) = std::fs::read_dir(packs_dir) else {
        return Vec::new();
    };
    let mut packs: Vec<FoundPack> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') || !path.is_dir() {
                return None;
            }
            let meta: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path.join("pack.mcmeta")).ok()?).ok()?;
            let description = meta
                .pointer("/pack/description")
                .cloned()
                .unwrap_or(serde_json::Value::String(String::new()));
            Some(FoundPack {
                id: format!("file/{name}"),
                name,
                description,
                icon: std::fs::read(path.join("pack.png")).ok(),
            })
        })
        .collect();
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

/// `DataPacks.Enabled` for the chosen ids: `vanilla`, then the others once.
pub fn enabled_ids(chosen: &[String]) -> Vec<String> {
    let mut ids = vec![VANILLA.to_owned()];
    for id in chosen {
        if id != VANILLA && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// Writes the new world's `level.dat` with these packs, as Pumpkin would make it.
pub fn prepare_level(world_dir: &Path, seed: i64, chosen: &[String]) -> std::io::Result<()> {
    if chosen.is_empty() || world_dir.join(LEVEL_DAT_FILE_NAME).exists() {
        return Ok(());
    }
    let seed = Seed(seed as u64);
    let generator = VanillaGenerator::new(seed, Dimension::OVERWORLD);
    let mut level = LevelData::from_world_generator(seed, &generator);
    level.data_packs.enabled = enabled_ids(chosen);
    AnvilLevelInfo
        .write_world_info(&level, world_dir)
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Copies a folder's contents into `to` (`Util.copyBetweenDirs`), creating it first.
pub fn copy_into(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_into(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_comes_first_and_ids_are_unique() {
        let chosen = ["bundle".to_owned(), VANILLA.to_owned(), "bundle".to_owned()];
        assert_eq!(enabled_ids(&chosen), ["vanilla", "bundle"]);
        assert_eq!(enabled_ids(&[]), ["vanilla"]);
    }

    #[test]
    fn only_folders_with_a_pack_mcmeta_are_packs() {
        let dir = std::env::temp_dir().join(format!("melon-dp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let packs = folder(&dir);
        std::fs::create_dir_all(packs.join("good")).unwrap();
        std::fs::create_dir_all(packs.join("empty")).unwrap();
        std::fs::write(
            packs.join("good/pack.mcmeta"),
            r#"{"pack":{"description":"Hi","pack_format":1}}"#,
        )
        .unwrap();
        let found = discover(&packs);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "file/good");
        assert_eq!(found[0].description, serde_json::json!("Hi"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
