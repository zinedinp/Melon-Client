//! Where Melon keeps launcher data: accounts, instances, prepared assets.

use std::path::{Path, PathBuf};

/// Minecraft version this client speaks.
pub const VERSION: &str = "26.3";

/// The folder that holds `launcher.json`, `instances/`
pub fn dir() -> PathBuf {
    if let Some(home) = std::env::var_os("MELON_HOME") {
        return PathBuf::from(home);
    }
    if checkout().is_some() {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }
    platform_data()
}

/// A source checkout of the client or the launcher: `Cargo.toml` plus `src/main.rs` in the cwd.
fn checkout() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    (cwd.join("Cargo.toml").is_file() && cwd.join("src/main.rs").is_file()).then_some(cwd)
}

/// OS personal data folder for Melon (`%APPDATA%/melon`, XDG, Application Support).
pub fn platform_data() -> PathBuf {
    dirs_data().join("melon")
}

/// Where extracted `assets/minecraft` go when settings do not pick a folder.
pub fn default_assets_dir() -> PathBuf {
    if std::env::var_os("MELON_HOME").is_some() {
        dir().join("assets")
    } else {
        platform_data().join("assets")
    }
}

/// Folder name for a Minecraft version: letters, digits, `.`, `-`, `_`.
pub fn version_folder(version: &str) -> Option<&str> {
    let ok = !version.is_empty()
        && version != "."
        && version != ".."
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    ok.then_some(version)
}

/// `base/<version>` — the tree that holds `minecraft/` for that version.
pub fn version_assets(base: &Path, version: &str) -> PathBuf {
    base.join(version_folder(version).unwrap_or(VERSION))
}

/// Folder-picker result.
pub fn assets_base_from_pick(path: &Path) -> PathBuf {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return path.to_owned();
    };
    let versioned = version_folder(name).is_some();
    let under_assets = path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "assets");
    if versioned && (under_assets || path.join("minecraft").is_dir()) {
        path.parent().unwrap_or(path).to_owned()
    } else {
        path.to_owned()
    }
}

fn dirs_data() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
    } else {
        home.join(".local/share")
    }
}

/// Prepared vanilla tree next to `home` (tests and `MELON_HOME` layouts).
pub fn assets_root(home: &Path) -> PathBuf {
    home.join("assets")
}

/// Official-launcher layout for jars and the asset index.
pub fn minecraft_cache(home: &Path) -> PathBuf {
    home.join("minecraft")
}

/// Marker written after a successful prepare of this assets folder.
pub fn prepared_marker(assets: &Path) -> PathBuf {
    assets.join(".melon-prepared")
}

/// Whether `home/assets` is a playable `assets/minecraft` tree.
pub fn is_prepared(home: &Path) -> bool {
    is_prepared_at(&assets_root(home))
}

/// Whether this assets folder already has a playable `minecraft` tree.
pub fn is_prepared_at(assets: &Path) -> bool {
    let minecraft = assets.join("minecraft");
    prepared_marker(assets).is_file()
        && minecraft.join("textures").is_dir()
        && minecraft.join("lang/en_us.json").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melon_home_wins() {
        let prev = std::env::var_os("MELON_HOME");
        unsafe { std::env::set_var("MELON_HOME", "/tmp/melon-home-test") };
        assert_eq!(dir(), PathBuf::from("/tmp/melon-home-test"));
        match prev {
            Some(v) => unsafe { std::env::set_var("MELON_HOME", v) },
            None => unsafe { std::env::remove_var("MELON_HOME") },
        }
    }

    #[test]
    fn version_is_this_client() {
        assert_eq!(VERSION, "26.3");
    }

    #[test]
    fn default_assets_live_in_the_personal_folder() {
        let prev = std::env::var_os("MELON_HOME");
        unsafe { std::env::remove_var("MELON_HOME") };
        let assets = default_assets_dir();
        assert_eq!(assets, platform_data().join("assets"));
        if let Some(v) = prev {
            unsafe { std::env::set_var("MELON_HOME", v) }
        }
    }

    #[test]
    fn versions_are_sibling_folders() {
        let base = PathBuf::from("/tmp/melon-assets");
        assert_eq!(version_assets(&base, "26.3"), base.join("26.3"));
        assert_eq!(version_assets(&base, "26.4"), base.join("26.4"));
        assert_eq!(version_assets(&base, "1.19.1"), base.join("1.19.1"));
        assert_ne!(version_assets(&base, "26.3"), version_assets(&base, "26.4"));
        assert_eq!(version_folder(".."), None);
        assert_eq!(version_folder("a/b"), None);
        assert_eq!(version_folder("26.3-0.19.5"), Some("26.3-0.19.5"));
    }

    #[test]
    fn picking_a_version_folder_uses_the_assets_base() {
        let versioned = PathBuf::from("/tmp/melon/assets/26.3");
        assert_eq!(
            assets_base_from_pick(&versioned),
            PathBuf::from("/tmp/melon/assets")
        );
        let base = PathBuf::from("/data/minecraft-assets");
        assert_eq!(assets_base_from_pick(&base), base);
    }
}
