//! The `resourcepacks` folder (vanilla `FolderRepositorySource`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{Pack, ResourceStack};

/// The id `options.txt` lists for the default resources; always at the bottom.
pub const VANILLA_ID: &str = "vanilla";

/// `<game dir>/resourcepacks`.
pub fn folder(game_dir: &Path) -> PathBuf {
    game_dir.join("resourcepacks")
}

/// Every pack in the folder, by id.
pub fn discover(game_dir: &Path) -> Vec<Arc<Pack>> {
    let dir = folder(game_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut packs: Vec<Arc<Pack>> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().into_string().ok()?;
            let is_zip = path.is_file() && name.to_ascii_lowercase().ends_with(".zip");
            if !(is_zip || path.is_dir()) {
                return None;
            }
            match Pack::open(format!("file/{name}"), name.clone(), &path) {
                Ok(pack) => Some(Arc::new(pack)),
                Err(e) => {
                    melon_base::warn!("Resource pack {name} skipped: {e:#}");
                    None
                }
            }
        })
        .collect();
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

/// Extracted zip-mods as resource packs (`assets/` under `mods/.cache`).
pub fn discover_mods(game_dir: &Path) -> Vec<Arc<Pack>> {
    pumpkin_mod::extracted_mods(&game_dir.join("mods").join(".cache"))
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("mod")
                .to_owned();
            Arc::new(Pack::open_mod(format!("mod/{name}"), name, &path))
        })
        .collect()
}

/// The packs of `selected` (ids, lowest first, as `options.txt` keeps them), with the server's
/// on top.
pub fn stack(available: &[Arc<Pack>], selected: &[String], server: &[Arc<Pack>]) -> ResourceStack {
    let mut packs: Vec<Arc<Pack>> = selected
        .iter()
        .filter_map(|id| available.iter().find(|p| &p.id == id).cloned())
        .collect();
    packs.extend(server.iter().cloned());
    ResourceStack { packs }
}
