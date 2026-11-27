//! The worlds in `saves/`.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WorldCreationMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Hardcore,
}

impl WorldCreationMode {
    pub const ALL: [WorldCreationMode; 4] = [
        WorldCreationMode::Survival,
        WorldCreationMode::Creative,
        WorldCreationMode::Adventure,
        WorldCreationMode::Hardcore,
    ];

    pub fn id(self) -> &'static str {
        match self {
            WorldCreationMode::Survival => "survival",
            WorldCreationMode::Creative => "creative",
            WorldCreationMode::Adventure => "adventure",
            WorldCreationMode::Hardcore => "hardcore",
        }
    }

    /// `Component.translatable("gameMode." + gameMode.getName())`.
    pub fn translation_key(self) -> &'static str {
        match self {
            WorldCreationMode::Survival => "gameMode.survival",
            WorldCreationMode::Creative => "gameMode.creative",
            WorldCreationMode::Adventure => "gameMode.adventure",
            WorldCreationMode::Hardcore => "gameMode.hardcore",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "creative" => WorldCreationMode::Creative,
            "adventure" => WorldCreationMode::Adventure,
            "hardcore" => WorldCreationMode::Hardcore,
            _ => WorldCreationMode::Survival,
        }
    }
}

impl melon_core::cycle::Cycle for WorldCreationMode {
    const ALL: &'static [Self] = &WorldCreationMode::ALL;
}

/// `Difficulty`, the create screen's Difficulty button.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Difficulty {
    Peaceful,
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Peaceful,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Difficulty::Peaceful => "peaceful",
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "peaceful" => Difficulty::Peaceful,
            "easy" => Difficulty::Easy,
            "hard" => Difficulty::Hard,
            _ => Difficulty::Normal,
        }
    }

    /// `level.dat`'s `difficulty` byte.
    fn from_level(id: i32) -> Self {
        match id {
            0 => Difficulty::Peaceful,
            1 => Difficulty::Easy,
            3 => Difficulty::Hard,
            _ => Difficulty::Normal,
        }
    }

    pub fn translation_key(self) -> &'static str {
        match self {
            Difficulty::Peaceful => "options.difficulty.peaceful",
            Difficulty::Easy => "options.difficulty.easy",
            Difficulty::Normal => "options.difficulty.normal",
            Difficulty::Hard => "options.difficulty.hard",
        }
    }
}

impl melon_core::cycle::Cycle for Difficulty {
    const ALL: &'static [Self] = &Difficulty::ALL;
}

/// Vanilla `WorldOptions.parseSeed`.
pub fn parse_seed(seed: &str) -> Option<i64> {
    let seed = seed.trim();
    if seed.is_empty() {
        return None;
    }
    Some(
        seed.parse::<i64>()
            .unwrap_or_else(|_| i64::from(melon_world::java_random::java_string_hash(seed))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_follow_file_util() {
        let dir = std::env::temp_dir().join(format!("melon-saves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(saves_dir(&dir)).unwrap();
        let make = |name: &str| std::fs::create_dir_all(saves_dir(&dir).join(name)).unwrap();
        assert_eq!(folder_name_for(&dir, "a/b:c.d"), "a_b_c_d");
        assert_eq!(folder_name_for(&dir, "  "), "World");
        assert_eq!(folder_name_for(&dir, "con"), "_con_");
        make("New World");
        assert_eq!(folder_name_for(&dir, "New World"), "New World (1)");
        make("New World (1)");
        assert_eq!(folder_name_for(&dir, "New World"), "New World (2)");
        make("Foo (3)");
        assert_eq!(folder_name_for(&dir, "Foo (3)"), "Foo (4)");
        assert_eq!(split_copy_counter("x (12)"), Some(("x", 12)));
        assert_eq!(split_copy_counter("x (a)"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_backup_zips_the_world_without_its_lock() {
        let dir = std::env::temp_dir().join(format!("melon-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let world = world_dir(&dir, "w");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"data").unwrap();
        std::fs::write(world.join("session.lock"), b"x").unwrap();
        std::fs::write(world.join("region/r.0.0.mca"), b"chunk").unwrap();
        let (path, size) = backup_world(&dir, "w").unwrap();
        assert!(size > 0);
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut names: Vec<_> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["w/level.dat", "w/region/", "w/region/r.0.0.mca"]);
        assert_eq!(zip.by_name("w/level.dat").unwrap().size(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worlds_without_a_sidecar_are_listed_and_the_newest_comes_first() {
        use pumpkin_world::world_info::WorldInfoWriter;
        let dir = std::env::temp_dir().join(format!("melon-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let old = world_dir(&dir, "New World");
        let mut level =
            pumpkin_world::world_info::LevelData::default(pumpkin_util::world_seed::Seed(1));
        level.level_name = "Old".to_owned();
        pumpkin_world::world_info::anvil::AnvilLevelInfo
            .write_world_info(&level, &old)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let settings = WorldSettings {
            name: "Fresh".to_owned(),
            ..WorldSettings::default()
        };
        save_world(&dir, "Fresh", &settings).unwrap();
        let worlds = list_worlds(&dir);
        let names: Vec<_> = worlds.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["Fresh", "Old"]);
        assert_eq!(folder_name_for(&dir, "New World"), "New World (1)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_text_parses_like_vanilla() {
        assert_eq!(parse_seed(""), None);
        assert_eq!(parse_seed("   "), None);
        assert_eq!(parse_seed(" 12345 "), Some(12345));
        assert_eq!(parse_seed("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(parse_seed("hello"), Some(99_162_322));
        assert_eq!(parse_seed("9223372036854775808"), Some(-1_773_151_197));
        assert_eq!(parse_seed("Glacier"), Some(1_772_835_215));
    }
}

#[derive(Serialize, Deserialize)]
struct WorldFile {
    name: String,
    game_mode: String,
    /// As typed; only the first launch reads it, then `level.dat` has it.
    #[serde(default)]
    seed: String,
    #[serde(default)]
    difficulty: String,
    #[serde(default)]
    allow_cheats: bool,
    /// The world preset's id; only the default can be generated yet.
    #[serde(default)]
    world_type: String,
    /// `DataPacks.Enabled` without `vanilla`: data packs and experiments.
    #[serde(default)]
    data_packs: Vec<String>,
}

/// What the create/edit screen sets for a world.
#[derive(Clone, Default)]
pub struct WorldSettings {
    pub name: String,
    pub game_mode: WorldCreationMode,
    pub seed: String,
    pub difficulty: Difficulty,
    pub allow_cheats: bool,
    pub world_type: super::WorldType,
    /// The enabled data packs and experiments, without `vanilla`.
    pub data_packs: Vec<String>,
}

#[derive(Clone)]
pub struct WorldEntry {
    /// `LevelSummary.getLastPlayed`: milliseconds since the epoch, `-1`
    pub last_played: i64,
    /// `LevelSummary.getWorldVersionName`, from `level.dat`.
    pub version: Option<String>,
    pub folder: String,
    pub name: String,
    pub game_mode: WorldCreationMode,
    pub seed: String,
    pub difficulty: Difficulty,
    pub allow_cheats: bool,
    pub world_type: super::WorldType,
    pub data_packs: Vec<String>,
}

impl WorldEntry {
    pub fn settings(&self) -> WorldSettings {
        WorldSettings {
            name: self.name.clone(),
            game_mode: self.game_mode,
            seed: self.seed.clone(),
            difficulty: self.difficulty,
            allow_cheats: self.allow_cheats,
            world_type: self.world_type,
            data_packs: self.data_packs.clone(),
        }
    }
}

/// Vanilla `<gameDir>/saves/`.
fn saves_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("saves")
}

/// `saves/<folder>/`.
pub fn world_dir(game_dir: &Path, folder: &str) -> PathBuf {
    saves_dir(game_dir).join(folder)
}

/// `SelectWorldScreen`'s list: every `saves/<folder>/melon_world.json`, the last played first.
pub fn list_worlds(game_dir: &Path) -> Vec<WorldEntry> {
    let Ok(read_dir) = std::fs::read_dir(saves_dir(game_dir)) else {
        return Vec::new();
    };
    let mut worlds: Vec<WorldEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            let level = read_level(&path);
            let file: Option<WorldFile> = melon_base::json_file::load(&path.join("melon_world.json"));
            if file.is_none() && level.is_none() {
                return None;
            }
            let last_played = level
                .as_ref()
                .map(|l| l.last_played)
                .filter(|&t| t > 0)
                .or_else(|| modified_millis(&path.join("melon_world.json")))
                .or_else(|| modified_millis(&path.join("level.dat")))
                .unwrap_or(-1);
            let (name, game_mode, seed, difficulty, allow_cheats, world_type, data_packs) =
                match (&file, &level) {
                    (Some(f), _) => (
                        f.name.clone(),
                        WorldCreationMode::from_id(&f.game_mode),
                        f.seed.clone(),
                        Difficulty::from_id(&f.difficulty),
                        f.allow_cheats,
                        super::WorldType::from_id(&f.world_type),
                        f.data_packs.clone(),
                    ),
                    (None, Some(l)) => (
                        l.level_name.clone(),
                        WorldCreationMode::Survival,
                        String::new(),
                        Difficulty::from_level(l.difficulty as i32),
                        l.allow_commands,
                        super::WorldType::Default,
                        l.data_packs
                            .enabled
                            .iter()
                            .filter(|id| id.as_str() != super::data_packs::VANILLA)
                            .cloned()
                            .collect(),
                    ),
                    (None, None) => return None,
                };
            Some(WorldEntry {
                last_played,
                version: level.map(|l| l.world_version.name),
                folder,
                name,
                game_mode,
                seed,
                difficulty,
                allow_cheats,
                world_type,
                data_packs,
            })
        })
        .collect();
    worlds.sort_by(|a, b| {
        b.last_played
            .cmp(&a.last_played)
            .then_with(|| a.folder.cmp(&b.folder))
    });
    worlds
}

/// `SharedConstants.ILLEGAL_FILE_CHARACTERS`, plus the `[./"]` that `FileUtil` replaces too.
const ILLEGAL_FILE_CHARACTERS: &[char] = &[
    '/', '\n', '\r', '\t', '\0', '\u{c}', '`', '?', '*', '\\', '<', '>', '|', '"', ':', '.',
];

/// `FileUtil.isPathPartPortable`: not a name Windows reserves.
fn is_portable(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
    };
    let reserved = matches!(stem, "COM" | "CLOCK$" | "CON" | "PRN" | "AUX" | "NUL")
        || numbered("COM")
        || numbered("LPT");
    !(reserved || name.ends_with('.'))
}

/// `FileUtil.findAvailableName(savesDir, name, "")`.
pub fn folder_name_for(game_dir: &Path, world_name: &str) -> String {
    let name = world_name.trim();
    let name = if name.is_empty() { "World" } else { name };
    let mut base: String = name
        .chars()
        .map(|c| {
            if ILLEGAL_FILE_CHARACTERS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if !is_portable(&base) {
        base = format!("_{base}_");
    }
    let mut count = 0u32;
    if let Some((stem, n)) = split_copy_counter(&base) {
        count = n;
        base = stem.to_owned();
    }
    let dir = saves_dir(game_dir);
    loop {
        let candidate = if count == 0 {
            base.clone()
        } else {
            format!("{base} ({count})")
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        count += 1;
    }
}

/// `COPY_COUNTER_PATTERN`: `"name (3)"` is `("name", 3)`; an empty count is 0.
fn split_copy_counter(name: &str) -> Option<(&str, u32)> {
    let body = name.strip_suffix(')')?;
    let (stem, digits) = body.rsplit_once(" (")?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((stem, digits.parse().unwrap_or(0)))
}

/// Writes `saves/<folder>/melon_world.json`.
pub fn save_world(game_dir: &Path, folder: &str, settings: &WorldSettings) -> io::Result<()> {
    let dir = saves_dir(game_dir).join(folder);
    std::fs::create_dir_all(&dir)?;
    let file = WorldFile {
        name: settings.name.clone(),
        game_mode: settings.game_mode.id().to_string(),
        seed: settings.seed.clone(),
        difficulty: settings.difficulty.id().to_string(),
        allow_cheats: settings.allow_cheats,
        world_type: settings.world_type.id().to_string(),
        data_packs: settings.data_packs.clone(),
    };
    melon_base::json_file::save(&dir.join("melon_world.json"), &file)
}

/// The seed the integrated server generated the world with, from its `level.dat`.
pub fn read_seed(game_dir: &Path, folder: &str) -> Option<i64> {
    use pumpkin_world::world_info::WorldInfoReader;
    pumpkin_world::world_info::anvil::AnvilLevelInfo
        .read_world_info(&world_dir(game_dir, folder))
        .ok()
        .map(|level| level.world_gen_settings.seed)
}

/// `LevelStorageAccess.renameLevel`.
pub fn rename_world(game_dir: &Path, folder: &str, name: &str) -> io::Result<()> {
    let path = world_dir(game_dir, folder).join("melon_world.json");
    let mut file: WorldFile = melon_base::json_file::load(&path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "melon_world.json"))?;
    file.name = name.to_owned();
    melon_base::json_file::save(&path, &file)
}

/// `saves/<folder>/icon.png`: the world's list icon, which Reset Icon removes.
pub fn icon_path(game_dir: &Path, folder: &str) -> PathBuf {
    world_dir(game_dir, folder).join("icon.png")
}

/// `LevelStorageAccess.makeWorldBackup`.
pub fn backup_world(game_dir: &Path, folder: &str) -> io::Result<(PathBuf, u64)> {
    use std::io::Write;
    let backups = backups_dir(game_dir);
    std::fs::create_dir_all(&backups)?;
    let stamp = crate::singleplayer::datetime::backup_stamp(now_millis());
    let mut n = 0;
    let zip_path = loop {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let candidate = backups.join(format!("{stamp}_{folder}{suffix}.zip"));
        if !candidate.exists() {
            break candidate;
        }
        n += 1;
    };
    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let root = world_dir(game_dir, folder);
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.file_name().is_some_and(|n| n == "session.lock") {
                continue;
            }
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            let name = format!(
                "{folder}/{}",
                relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/")
            );
            if path.is_dir() {
                zip.add_directory(format!("{name}/"), options)
                    .map_err(io::Error::other)?;
                stack.push(path);
            } else {
                zip.start_file(name, options).map_err(io::Error::other)?;
                zip.write_all(&std::fs::read(&path)?)?;
            }
        }
    }
    zip.finish().map_err(io::Error::other)?;
    let size = std::fs::metadata(&zip_path)?.len();
    Ok((zip_path, size))
}

/// Vanilla `LevelStorageSource.getBackupPath`: `<gameDir>/backups`.
pub fn backups_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("backups")
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn modified_millis(path: &Path) -> Option<i64> {
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(time.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
}

/// The world's `level.dat`, if it has been played.
fn read_level(dir: &Path) -> Option<pumpkin_world::world_info::LevelData> {
    use pumpkin_world::world_info::WorldInfoReader;
    pumpkin_world::world_info::anvil::AnvilLevelInfo
        .read_world_info(dir)
        .ok()
}

pub fn delete_world(game_dir: &Path, folder: &str) -> io::Result<()> {
    std::fs::remove_dir_all(saves_dir(game_dir).join(folder))
}
