//! Small JSON files next to the game.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// The file's contents, or `None` when it is missing or unreadable.
pub fn load<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes `value` pretty-printed.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    replace(path, &serde_json::to_string_pretty(value)?, false)
}

/// Like [`save`], for a file only its owner may read.
pub fn save_private<T: Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    replace(path, &serde_json::to_string_pretty(value)?, true)
}

/// Writes `text` beside `path` and renames it over: a crash mid-write
/// leaves the old file, not half of the new one. `private`: owner-only on Unix.
fn replace(path: &Path, text: &str, private: bool) -> std::io::Result<()> {
    use std::io::Write;
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let written = options.open(&temp).and_then(|mut file| {
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    let result = written.and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_save_replaces_the_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("melon-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.json");
        super::save(&path, &[1, 2]).unwrap();
        super::save_private(&path, &[3]).unwrap();
        assert_eq!(super::load::<Vec<i32>>(&path), Some(vec![3]));
        assert!(!dir.join("a.json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
