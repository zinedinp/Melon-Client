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
    std::fs::write(path, serde_json::to_string_pretty(value)?)
}

/// Like [`save`], for a file only its owner may read.
pub fn save_private<T: Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(serde_json::to_string_pretty(value)?.as_bytes())
    }
    #[cfg(not(unix))]
    {
        save(path, value)
    }
}
