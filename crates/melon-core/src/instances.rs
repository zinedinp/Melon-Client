//! The game's side of the launcher's instances: which folder to play in, and where the
//! launcher keeps the assets. Creating and managing instances is the launcher's job.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The instance played when none is named: `instances/melon/`.
pub const MELON: &str = "melon";
const FILE: &str = "instances.json";
const OWN: &str = "instances";
/// Folders every instance has.
const GAME_DIRS: [&str; 5] = ["mods", "plugins", "saves", "resourcepacks", "logs"];

/// The folder of the instance `id`.
pub fn dir(main: &Path, id: &str) -> PathBuf {
    main.join(OWN).join(id)
}

/// A folder name the launcher could have made: letters, digits, `-` and `_`.
fn is_folder_name(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// What the game reads of an `instances.json` entry.
#[derive(Default, Deserialize)]
struct Meta {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Default, Deserialize)]
struct Store {
    #[serde(default)]
    instances: BTreeMap<String, Meta>,
}

/// The folder of the instance an id or display name (any case) names; [`MELON`] always has
/// one, so the game runs without the launcher.
pub fn find(main: &Path, query: &str) -> Option<PathBuf> {
    let query = query.trim();
    if is_folder_name(query) && (query == MELON || dir(main, query).is_dir()) {
        return Some(dir(main, query));
    }
    let store: Store = melon_base::json_file::load(&main.join(FILE)).unwrap_or_default();
    store
        .instances
        .iter()
        .find(|(_, meta)| {
            meta.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(query))
        })
        .map(|(id, _)| id)
        .filter(|id| is_folder_name(id))
        .map(|id| dir(main, id))
}

/// `logs/` of this game folder.
pub fn logs_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("logs")
}

/// Creates the folders every instance has, where they are missing.
pub fn ensure_dirs(game_dir: &Path) {
    for name in GAME_DIRS {
        if let Err(e) = std::fs::create_dir_all(game_dir.join(name)) {
            melon_base::warn!("{} not created: {e}", game_dir.join(name).display());
        }
    }
}

/// Remembers the server joined, for the launcher's quick join.
pub fn save_last_server(game_dir: &Path, address: &str) {
    if let Err(e) = std::fs::write(game_dir.join("last_server.txt"), address) {
        melon_base::warn!("last_server.txt not saved: {e}");
    }
}

/// What the game reads of the launcher's `launcher.json`: where the assets are.
#[derive(Deserialize, Default)]
pub struct LauncherSettings {
    /// Folder extracted assets live in. None: [`melon_base::home::default_assets_dir`].
    #[serde(default)]
    pub assets_dir: Option<String>,
}

impl LauncherSettings {
    pub fn load(main: &Path) -> Self {
        melon_base::json_file::load(&main.join("launcher.json")).unwrap_or_default()
    }

    /// Base folder for extracted assets (settings path, or the OS personal folder).
    pub fn assets_dir(&self, home: &Path) -> PathBuf {
        if let Some(path) = self
            .assets_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return PathBuf::from(path);
        }
        let personal = melon_base::home::default_assets_dir();
        if melon_base::home::is_prepared_at(&melon_base::home::version_assets(
            &personal,
            melon_base::home::VERSION,
        )) || melon_base::home::is_prepared_at(&personal)
            || !melon_base::home::is_prepared(home)
        {
            personal
        } else {
            melon_base::home::assets_root(home)
        }
    }

    /// Folder the game reads (`assets/<VERSION>/`, or the flat `assets/` of old).
    pub fn melon_play_assets(&self, home: &Path) -> PathBuf {
        let versioned = melon_base::home::version_assets(&self.assets_dir(home), melon_base::home::VERSION);
        let base = self.assets_dir(home);
        if !melon_base::home::is_prepared_at(&versioned) && melon_base::home::is_prepared_at(&base) {
            base
        } else {
            versioned
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("melon-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn instances_are_found_by_folder_or_name_and_nothing_is_created() {
        let main = scratch("instances");
        assert_eq!(find(&main, MELON), Some(dir(&main, MELON)));
        assert_eq!(find(&main, "vanilla"), None);
        assert!(!main.join(OWN).exists(), "finding creates no folders");

        std::fs::create_dir_all(dir(&main, "survival-2")).unwrap();
        std::fs::write(
            main.join(FILE),
            r#"{"instances":{"survival-2":{"name":"Survival 2","engine":"melon"}}}"#,
        )
        .unwrap();
        assert_eq!(find(&main, "survival-2"), Some(dir(&main, "survival-2")));
        assert_eq!(find(&main, "survival 2"), Some(dir(&main, "survival-2")));
        assert_eq!(find(&main, "../evil"), None);
        std::fs::remove_dir_all(main).unwrap();
    }

    #[test]
    fn an_instance_gets_its_folders() {
        let main = scratch("dirs");
        let game = dir(&main, MELON);
        ensure_dirs(&game);
        for name in GAME_DIRS {
            assert!(game.join(name).is_dir());
        }
        std::fs::remove_dir_all(main).unwrap();
    }

    #[test]
    fn the_last_server_is_written() {
        let dir = scratch("server");
        save_last_server(&dir, "play.example.org:25565");
        assert_eq!(
            std::fs::read_to_string(dir.join("last_server.txt")).unwrap(),
            "play.example.org:25565"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_settings_assets_path_overrides_the_personal_folder() {
        let settings: LauncherSettings = serde_json::from_str(
            r#"{"assets_dir":"/tmp/melon-assets-test","instance":"x","keep_open":true}"#,
        )
        .unwrap();
        let home = Path::new("/unused");
        assert_eq!(
            settings.assets_dir(home),
            PathBuf::from("/tmp/melon-assets-test")
        );
        assert_eq!(
            settings.melon_play_assets(home),
            PathBuf::from(format!("/tmp/melon-assets-test/{}", melon_base::home::VERSION))
        );
    }
}
