//! Vanilla `ServerList`: the multiplayer screen's saved servers, in order.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEntry {
    pub name: String,
    /// `host[:port]`, as typed.
    pub ip: String,
    /// `ServerData.acceptTextures`: the server's resource packs are taken
    #[serde(
        rename = "acceptTextures",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub accept_textures: Option<bool>,
}

/// The saved server at `address`, if there is one.
pub fn find(game_dir: &Path, address: &str) -> Option<ServerEntry> {
    load(game_dir).into_iter().find(|s| s.ip == address)
}

/// `ServerList.saveSingleServer` for the pack prompt's answer: every saved
pub fn set_accept_textures(game_dir: &Path, address: &str, accept: Option<bool>) {
    let mut servers = load(game_dir);
    let mut changed = false;
    for server in servers.iter_mut().filter(|s| s.ip == address) {
        changed |= server.accept_textures != accept;
        server.accept_textures = accept;
    }
    if changed {
        save(game_dir, &servers);
    }
}

fn path(game_dir: &Path) -> PathBuf {
    game_dir.join("servers.json")
}

pub fn load(game_dir: &Path) -> Vec<ServerEntry> {
    crate::json_file::load(&path(game_dir)).unwrap_or_default()
}

pub fn save(game_dir: &Path, servers: &[ServerEntry]) {
    if let Err(e) = crate::json_file::save(&path(game_dir), servers) {
        crate::warn!("Server list not saved: {e}");
    }
}

/// Adds a server, or replaces the one at `index` when editing.
pub fn upsert(game_dir: &Path, index: Option<usize>, entry: ServerEntry) {
    let mut servers = load(game_dir);
    match index.and_then(|i| servers.get_mut(i)) {
        Some(existing) => *existing = entry,
        None => servers.push(entry),
    }
    save(game_dir, &servers);
}

pub fn remove(game_dir: &Path, index: usize) {
    let mut servers = load(game_dir);
    if index < servers.len() {
        servers.remove(index);
        save(game_dir, &servers);
    }
}

/// `ServerList.swap`: exchanges two servers, which is how the arrows on a
pub fn swap(game_dir: &Path, a: usize, b: usize) {
    let mut servers = load(game_dir);
    if a < servers.len() && b < servers.len() {
        servers.swap(a, b);
        save(game_dir, &servers);
    }
}
