//! Instances and the launcher's own settings.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MAX_NAME_LEN: usize = 48;
/// Melon latest: `instances/melon/`.
pub const MELON: &str = "melon";
/// Vanilla Jar: Java Edition in `instances/vanilla/`.
pub const VANILLA: &str = "vanilla";
/// Alias of [`MELON`] (older settings and `--instance melon`).
pub const DEFAULT: &str = MELON;
const FILE: &str = "instances.json";
const OWN: &str = "instances";
const NOT_COPIED: &[&str] = &[
    "logs",
    "crash-reports",
    ".cache",
    "instances",
    "assets",
    "minecraft",
    "cache",
    "launcher.json",
    "instances.json",
];

/// A display name: letters, digits, `_`, `-` and inner spaces.
pub fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
        && name == name.trim()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b' '))
}

/// Folder name from a display name: lowercase, spaces to `-`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "instance".into()
    } else {
        out
    }
}

/// Whether `id` is one of the two built-in defaults.
pub fn is_builtin(id: &str) -> bool {
    id == MELON || id == VANILLA
}

/// What plays this instance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Melon,
    Java,
}

/// The folder of the instance `id` (a slug, or [`MELON`]).
pub fn dir(main: &Path, id: &str) -> PathBuf {
    main.join(OWN).join(id)
}

/// What the main folder held for the game before Melon latest had a folder
const LEGACY_GAME_FILES: &[&str] = &[
    "saves",
    "mods",
    "plugins",
    "resourcepacks",
    "logs",
    "downloads",
    "screenshots",
    "options.txt",
    "servers.json",
    "last_server.txt",
    "hotbar.nbt",
    "debug_settings.json",
    "command_history.txt",
];

/// Moves the game's files out of the main folder into Melon latest's, never
fn migrate_legacy(main: &Path, melon: &Path) {
    for name in LEGACY_GAME_FILES {
        let (from, to) = (main.join(name), melon.join(name));
        if !from.exists() || to.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(&from, &to) {
            crate::warn!("{} not moved: {e}", from.display());
        }
    }
}

/// Makes the two default instances exist (Melon latest + Vanilla Jar).
pub fn ensure_builtins(main: &Path) {
    let vanilla = dir(main, VANILLA);
    let _ = std::fs::create_dir_all(&vanilla);
    ensure_dirs(&vanilla);
    let melon = dir(main, MELON);
    let fresh = !melon.exists();
    let _ = std::fs::create_dir_all(&melon);
    if fresh {
        migrate_legacy(main, &melon);
    }
    ensure_dirs(&melon);
    let mut store = Store::load(main);
    let mut dirty = false;
    store.instances.entry(MELON.to_string()).or_insert_with(|| {
        dirty = true;
        Meta {
            name: Some("Melon latest".into()),
            engine: Engine::Melon,
            ..Meta::default()
        }
    });
    store
        .instances
        .entry(VANILLA.to_string())
        .or_insert_with(|| {
            dirty = true;
            Meta {
                name: Some("Vanilla Jar".into()),
                engine: Engine::Java,
                java_version: Some(crate::home::VERSION.into()),
                mojang_java: true,
                ..Meta::default()
            }
        });
    if dirty {
        store.save(main);
    }
}

/// The instances there are: Melon latest and Vanilla Jar first, then by group and name.
pub fn list(main: &Path) -> Vec<Instance> {
    ensure_builtins(main);
    let mut store = Store::load(main);
    let mut own: Vec<_> = std::fs::read_dir(main.join(OWN))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.') && !is_builtin(name))
        .collect();
    own.sort();
    if main.join(OWN).is_dir() {
        let before = store.instances.len();
        store
            .instances
            .retain(|id, _| is_builtin(id) || own.contains(id));
        if store.instances.len() != before {
            store.save(main);
        }
    }
    let mut out = Vec::with_capacity(own.len() + 2);
    out.push(from_meta(MELON, &dir(main, MELON), &store));
    out.push(from_meta(VANILLA, &dir(main, VANILLA), &store));
    for id in own {
        let path = main.join(OWN).join(&id);
        out.push(from_meta(&id, &path, &store));
    }
    out[2..].sort_by_key(|i| (i.group.clone(), i.name.to_lowercase()));
    out
}

/// The instance an id or display name (any case) names.
pub fn find(main: &Path, query: &str) -> Option<Instance> {
    let q = query.trim();
    let all = list(main);
    all.iter()
        .find(|i| i.id == q)
        .or_else(|| all.iter().find(|i| i.name.eq_ignore_ascii_case(q)))
        .cloned()
}

/// Makes a new, empty instance named `name` at `instances/<slug>/`. An
pub fn create(main: &Path, name: &str) -> std::io::Result<Instance> {
    create_from(main, name, None)
}

/// A copy of `from`'s game folder (worlds, options, mods), skipping logs.
pub fn copy(main: &Path, from_id: &str, name: &str) -> std::io::Result<Instance> {
    let from = find(main, from_id).ok_or(std::io::ErrorKind::NotFound)?;
    create_from(main, name, Some(&from))
}

fn create_from(main: &Path, name: &str, from: Option<&Instance>) -> std::io::Result<Instance> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let all = list(main);
    if all.iter().any(|i| i.name.eq_ignore_ascii_case(name)) {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    std::fs::create_dir_all(main.join(OWN))?;
    let base = slug(name);
    let taken = |s: &str| is_builtin(s) || main.join(OWN).join(s).exists();
    let id = (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|s| !taken(s))
        .expect("slug");
    let path = main.join(OWN).join(&id);
    let made = match from {
        Some(from) => copy_home(&from.path, &path),
        None => {
            std::fs::create_dir(&path)?;
            ensure_dirs(&path);
            Ok(())
        }
    };
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(&path);
        return Err(e);
    }
    let mut store = Store::load(main);
    let meta = store.instances.entry(id.clone()).or_default();
    meta.name = (name != id).then(|| name.to_string());
    meta.group = from.and_then(|f| f.group.clone());
    if let Some(from) = from {
        meta.engine = from.engine;
        meta.java_version = from.java_version.clone();
        meta.mojang_java = from.mojang_java;
        meta.use_global_options = from.use_global_options;
    }
    store.save(main);
    Ok(from_meta(&id, &path, &store))
}

fn copy_home(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if NOT_COPIED.iter().any(|n| name == *n) {
            continue;
        }
        let src = entry.path();
        let dest = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir(&src, &dest)?;
        } else {
            std::fs::copy(&src, dest)?;
        }
    }
    ensure_dirs(to);
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Changes the display name. The folder stays.
pub fn rename(main: &Path, id: &str, name: &str) -> std::io::Result<Instance> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    if instance.is_default {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    if list(main)
        .iter()
        .any(|i| i.id != instance.id && i.name.eq_ignore_ascii_case(name))
    {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    let mut store = Store::load(main);
    let meta = store.instances.entry(instance.id.clone()).or_default();
    meta.name = (name != instance.id).then(|| name.to_string());
    store.save(main);
    find(main, &instance.id).ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

/// Folders every instance owns under `instances/<slug>/`.
const GAME_DIRS: [&str; 5] = ["mods", "plugins", "saves", "resourcepacks", "logs"];

/// `mods/` of this game folder (zip mods, cache).
pub fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

/// `saves/` of this game folder (worlds).
pub fn saves_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("saves")
}

/// `logs/` of this game folder.
pub fn logs_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("logs")
}

/// Creates `mods/`, `plugins/`, `saves/`, `resourcepacks/` and `logs/` so a
pub fn ensure_dirs(game_dir: &Path) {
    for name in GAME_DIRS {
        if let Err(e) = std::fs::create_dir_all(game_dir.join(name)) {
            crate::warn!("{} not created: {e}", game_dir.join(name).display());
        }
    }
}

/// Folder whose `options.txt` this instance uses (main folder when global).
pub fn options_dir(main: &Path, instance: &Instance) -> PathBuf {
    if instance.id == MELON || instance.use_global_options {
        dir(main, MELON)
    } else {
        instance.path.clone()
    }
}

/// Copy the Melon latest `options.txt` into this instance so the game
pub fn apply_global_options(main: &Path, instance: &Instance) {
    if instance.id == MELON || !instance.use_global_options {
        return;
    }
    let src = dir(main, MELON).join("options.txt");
    if !src.is_file() {
        return;
    }
    if let Err(e) = std::fs::copy(&src, instance.path.join("options.txt")) {
        crate::warn!("Global options.txt not copied: {e}");
    }
}

/// After Play, copy this instance's `options.txt` back over the global
pub fn collect_global_options(main: &Path, instance_id: Option<&str>) {
    let Some(id) = instance_id.filter(|id| *id != MELON) else {
        return;
    };
    let Some(instance) = find(main, id) else {
        return;
    };
    if !instance.use_global_options {
        return;
    }
    let src = instance.path.join("options.txt");
    if !src.is_file() {
        return;
    }
    if let Err(e) = std::fs::copy(&src, dir(main, MELON).join("options.txt")) {
        crate::warn!("Global options.txt not written back: {e}");
    }
}

/// Per-instance vs Melon latest `options.txt`. Melon latest is always global.
pub fn set_use_global_options(main: &Path, id: &str, on: bool) -> std::io::Result<Instance> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    if instance.id == MELON {
        return Ok(instance);
    }
    let mut store = Store::load(main);
    let meta = store.instances.entry(instance.id.clone()).or_default();
    meta.use_global_options = on;
    store.save(main);
    find(main, &instance.id).ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

/// Melon or Java for a user instance. The two builtins stay as they are.
pub fn set_engine(main: &Path, id: &str, engine: Engine) -> std::io::Result<Instance> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    if instance.is_default {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    let mut store = Store::load(main);
    let meta = store.instances.entry(instance.id.clone()).or_default();
    meta.engine = engine;
    if engine == Engine::Java && meta.java_version.is_none() {
        meta.java_version = Some(crate::home::VERSION.into());
    }
    store.save(main);
    find(main, &instance.id).ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

/// Deletes an instance with everything in it (its worlds too). The default
pub fn delete(main: &Path, id: &str) -> std::io::Result<()> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    if instance.is_default {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    std::fs::remove_dir_all(&instance.path)?;
    let mut store = Store::load(main);
    store.instances.remove(&instance.id);
    store.save(main);
    Ok(())
}

/// Seconds played in the instance, added up by the launcher.
pub fn record_play(main: &Path, id: &str, secs: u64) {
    let Some(instance) = find(main, id) else {
        return;
    };
    let mut store = Store::load(main);
    let meta = store.instances.entry(instance.id).or_default();
    meta.played_secs = meta.played_secs.saturating_add(secs);
    meta.last_played = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    store.save(main);
}

/// What the launcher remembers between runs, in `launcher.json`.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct LauncherSettings {
    /// The instance Play starts; none for the default (main folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Run the game with the Vulkan validation layers.
    #[serde(default)]
    pub validation: bool,
    /// Run the game with the tester, which writes `logs/probe-<time>.md`
    #[serde(default)]
    pub probe: bool,
    /// Leave when the game ends instead of coming back (an error still
    #[serde(default)]
    pub exit_after_game: bool,
    /// Folder extracted assets live in. None: [`crate::home::default_assets_dir`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets_dir: Option<String>,
    /// Local Melon client binary Play should run. None: scan, then this process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub melon_binary: Option<String>,
}

impl LauncherSettings {
    pub fn load(main: &Path) -> Self {
        crate::json_file::load(&main.join("launcher.json")).unwrap_or_default()
    }

    pub fn save(&self, main: &Path) {
        if let Err(e) = crate::json_file::save(&main.join("launcher.json"), self) {
            crate::warn!("launcher.json not saved: {e}");
        }
    }

    /// The selected instance's slug, including the two defaults.
    pub fn instance_in(&self, main: &Path) -> Option<String> {
        let query = self.instance.as_deref().unwrap_or(MELON);
        find(main, query).map(|i| i.id)
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
        let personal = crate::home::default_assets_dir();
        if crate::home::is_prepared_at(&crate::home::version_assets(
            &personal,
            crate::home::VERSION,
        )) || crate::home::is_prepared_at(&personal)
            || !crate::home::is_prepared(home)
        {
            personal
        } else {
            crate::home::assets_root(home)
        }
    }

    /// Where this version's `minecraft/` tree is extracted. Always a sibling
    pub fn extract_dir(&self, home: &Path, version: &str) -> PathBuf {
        crate::home::version_assets(&self.assets_dir(home), version)
    }

    /// Folder the game reads for `version`: the versioned tree, or the legacy
    pub fn assets_for(&self, home: &Path, version: &str) -> PathBuf {
        let versioned = self.extract_dir(home, version);
        if crate::home::is_prepared_at(&versioned) {
            return versioned;
        }
        let base = self.assets_dir(home);
        if crate::home::is_prepared_at(&base) {
            base
        } else {
            versioned
        }
    }

    /// Extract dest for this Melon client (`assets/<VERSION>/`).
    pub fn melon_extract(&self, home: &Path) -> PathBuf {
        self.extract_dir(home, crate::home::VERSION)
    }

    /// Folder Melon Play reads (`assets/<VERSION>/`, or legacy flat `assets/`).
    pub fn melon_play_assets(&self, home: &Path) -> PathBuf {
        self.assets_for(home, crate::home::VERSION)
    }

    /// Store a chosen assets base in `launcher.json`.
    pub fn set_assets_path(&mut self, home: &Path, path: PathBuf) {
        let path = crate::home::assets_base_from_pick(&path);
        let default = crate::home::default_assets_dir();
        self.assets_dir = if path == default {
            None
        } else {
            Some(path.display().to_string())
        };
        self.save(home);
    }

    /// The Melon binary Play starts: the saved path, a local scan, or this
    pub fn melon_exe(&self, home: &Path) -> PathBuf {
        if let Some(path) = self.melon_binary.as_deref() {
            let path = PathBuf::from(path);
            if path.is_file() {
                return path;
            }
        }
        if let Some(found) = crate::prepare::scan_binaries(home) {
            return found.path;
        }
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("melon"));
        if crate::prepare::is_game_name(&exe) {
            exe
        } else {
            PathBuf::from("melon")
        }
    }
}

/// The selected instance's game folder.
pub fn selected_dir(main: &Path, settings: &LauncherSettings) -> PathBuf {
    match settings.instance_in(main) {
        Some(id) => dir(main, &id),
        None => dir(main, MELON),
    }
}

/// The server the game joined last, for the launcher's quick join.
pub fn last_server(game_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(game_dir.join("last_server.txt")).ok()?;
    let address = text.trim();
    (!address.is_empty()).then(|| address.to_owned())
}

/// Quick-join address for this instance. Melon latest points at the game
pub fn quick_join(instance: &Instance) -> Option<String> {
    if instance.id == MELON {
        None
    } else {
        last_server(&instance.path)
    }
}

pub fn save_last_server(game_dir: &Path, address: &str) {
    if let Err(e) = std::fs::write(game_dir.join("last_server.txt"), address) {
        crate::warn!("last_server.txt not saved: {e}");
    }
}

#[derive(Clone, Debug)]
pub struct Instance {
    /// Folder name, or [`DEFAULT`].
    pub id: String,
    /// What the list shows.
    pub name: String,
    pub path: PathBuf,
    pub is_default: bool,
    pub engine: Engine,
    /// Java version id (`26.3`); unused for Melon.
    pub java_version: Option<String>,
    /// Official Mojang files (Vanilla Jar). False: extract assets from the jar.
    pub mojang_java: bool,
    pub group: Option<String>,
    pub played_secs: u64,
    pub last_played: Option<u64>,
    /// When true, language / fullscreen / in-game options come from the
    pub use_global_options: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    played_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_played: Option<u64>,
    #[serde(default)]
    engine: Engine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    java_version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    mojang_java: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    use_global_options: bool,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    instances: BTreeMap<String, Meta>,
}

impl Store {
    fn load(main: &Path) -> Store {
        crate::json_file::load(&main.join(FILE)).unwrap_or_default()
    }

    fn save(&self, main: &Path) {
        if let Err(e) = crate::json_file::save(&main.join(FILE), self) {
            crate::warn!("{FILE} not saved: {e}");
        }
    }
}

fn from_meta(id: &str, path: &Path, store: &Store) -> Instance {
    let meta = store.instances.get(id).cloned().unwrap_or_default();
    let engine = if id == VANILLA {
        Engine::Java
    } else {
        meta.engine
    };
    let java_version = if engine == Engine::Java {
        meta.java_version
            .or_else(|| Some(crate::home::VERSION.into()))
    } else {
        meta.java_version
    };
    Instance {
        name: meta.name.unwrap_or_else(|| match id {
            MELON => "Melon latest".into(),
            VANILLA => "Vanilla Jar".into(),
            _ => id.to_string(),
        }),
        group: meta.group,
        played_secs: meta.played_secs,
        last_played: meta.last_played,
        is_default: is_builtin(id),
        engine,
        java_version,
        mojang_java: id == VANILLA || meta.mojang_java,
        use_global_options: id == MELON || meta.use_global_options,
        id: id.to_string(),
        path: path.to_path_buf(),
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
    fn names_cannot_leave_the_instances_folder() {
        assert!(valid_name("Survival 2"));
        assert!(!valid_name(".."));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(" lead"));
        assert!(!valid_name(""));
        assert!(!valid_name(&"x".repeat(49)));
    }

    #[test]
    fn slug_is_a_folder_name() {
        assert_eq!(slug("Survival 2"), "survival-2");
        assert_eq!(slug("  Hello--World  "), "hello-world");
    }

    #[test]
    fn instances_are_created_listed_copied_renamed_and_deleted() {
        let main = scratch("instances");
        let zeta = create(&main, "Zeta").unwrap();
        assert_eq!(zeta.id, "zeta");
        assert_eq!(zeta.name, "Zeta");
        create(&main, "alpha").unwrap();
        assert!(mods_dir(&dir(&main, "alpha")).is_dir());
        assert!(saves_dir(&dir(&main, "alpha")).is_dir());
        assert!(dir(&main, "alpha").join("plugins").is_dir());
        assert!(dir(&main, "alpha").join("resourcepacks").is_dir());
        assert!(logs_dir(&dir(&main, "alpha")).is_dir());
        assert!(create(&main, "alpha").is_err());
        assert!(create(&main, "../evil").is_err());
        let names: Vec<_> = list(&main).into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["Melon latest", "Vanilla Jar", "alpha", "Zeta"]);
        assert_eq!(list(&main)[0].engine, Engine::Melon);
        assert_eq!(list(&main)[1].engine, Engine::Java);

        std::fs::write(dir(&main, "alpha").join("options.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir(&main, "alpha").join("logs")).unwrap();
        std::fs::write(dir(&main, "alpha").join("logs/latest.log"), b"log").unwrap();
        let copy = copy(&main, "alpha", "Alpha Two").unwrap();
        assert_eq!(copy.id, "alpha-two");
        assert!(copy.path.join("options.txt").is_file());
        assert!(!copy.path.join("logs/latest.log").is_file());
        assert!(logs_dir(&copy.path).is_dir());

        let renamed = rename(&main, "alpha-two", "Second World").unwrap();
        assert_eq!(renamed.id, "alpha-two");
        assert_eq!(renamed.name, "Second World");
        assert!(dir(&main, "alpha-two").is_dir());

        delete(&main, "alpha").unwrap();
        let names: Vec<_> = list(&main).into_iter().map(|i| i.name).collect();
        assert_eq!(
            names,
            ["Melon latest", "Vanilla Jar", "Second World", "Zeta"]
        );
        assert!(delete(&main, DEFAULT).is_err());
        assert!(delete(&main, VANILLA).is_err());

        let engine = set_engine(&main, "zeta", Engine::Java).unwrap();
        assert_eq!(engine.engine, Engine::Java);
        assert_eq!(engine.java_version.as_deref(), Some(crate::home::VERSION));
        assert!(set_engine(&main, MELON, Engine::Java).is_err());

        let zeta = find(&main, "zeta").unwrap();
        assert!(!zeta.use_global_options);
        assert_eq!(options_dir(&main, &zeta), zeta.path);
        let zeta = set_use_global_options(&main, "zeta", true).unwrap();
        assert!(zeta.use_global_options);
        assert_eq!(options_dir(&main, &zeta), dir(&main, MELON));
        let melon = find(&main, MELON).unwrap();
        assert!(melon.use_global_options);
        assert_eq!(options_dir(&main, &melon), melon.path);
        std::fs::write(melon.path.join("options.txt"), b"lang:de_de").unwrap();
        apply_global_options(&main, &zeta);
        assert_eq!(
            std::fs::read_to_string(zeta.path.join("options.txt")).unwrap(),
            "lang:de_de"
        );
        std::fs::write(zeta.path.join("options.txt"), b"lang:en_us").unwrap();
        collect_global_options(&main, Some("zeta"));
        assert_eq!(
            std::fs::read_to_string(melon.path.join("options.txt")).unwrap(),
            "lang:en_us"
        );
        std::fs::remove_dir_all(main).unwrap();
    }

    #[test]
    fn a_missing_instance_falls_back_to_the_main_folder() {
        let main = scratch("settings");
        let settings = LauncherSettings {
            instance: Some("gone".into()),
            ..LauncherSettings::default()
        };
        assert_eq!(settings.instance_in(&main), None);
        let melon = LauncherSettings::default();
        assert_eq!(melon.instance_in(&main).as_deref(), Some(MELON));
        std::fs::remove_dir_all(main).unwrap();
    }

    #[test]
    fn the_last_server_round_trips() {
        let dir = scratch("server");
        assert_eq!(last_server(&dir), None);
        save_last_server(&dir, "play.example.org:25565");
        assert_eq!(last_server(&dir).as_deref(), Some("play.example.org:25565"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn melon_latest_has_no_quick_join() {
        let main = scratch("quick-join");
        let melon = find(&main, MELON).unwrap();
        save_last_server(&melon.path, "GommeHD.net");
        assert_eq!(quick_join(&melon), None);
        let vanilla = find(&main, VANILLA).unwrap();
        save_last_server(&vanilla.path, "play.example.org");
        assert_eq!(quick_join(&vanilla).as_deref(), Some("play.example.org"));
        std::fs::remove_dir_all(main).unwrap();
    }

    #[test]
    fn a_settings_assets_path_overrides_the_personal_folder() {
        let settings = LauncherSettings {
            assets_dir: Some("/tmp/melon-assets-test".into()),
            ..LauncherSettings::default()
        };
        let home = Path::new("/unused");
        assert_eq!(
            settings.assets_dir(home),
            PathBuf::from("/tmp/melon-assets-test")
        );
        assert_eq!(
            settings.extract_dir(home, "26.3"),
            PathBuf::from("/tmp/melon-assets-test/26.3")
        );
        assert_eq!(
            settings.extract_dir(home, "1.19.1"),
            PathBuf::from("/tmp/melon-assets-test/1.19.1")
        );
        assert_eq!(
            settings.melon_extract(home),
            PathBuf::from(format!("/tmp/melon-assets-test/{}", crate::home::VERSION))
        );
    }
}
