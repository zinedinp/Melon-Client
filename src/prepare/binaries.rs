//! Local Melon client binaries.

use std::path::{Path, PathBuf};

/// A Melon (or compatible) client binary already on this machine.
#[derive(Clone, Debug)]
pub struct Binary {
    pub label: String,
    pub path: PathBuf,
}

/// Newest-looking local binary we can run as the Melon client.
pub fn scan(home: &Path) -> Option<Binary> {
    let mut hits = Vec::new();
    hits.extend(beside(home, "Melon folder"));
    if let Ok(exe) = std::env::current_exe() {
        if is_melon_name(&exe) && exe.is_file() {
            hits.push(Binary {
                label: "this binary".into(),
                path: exe.clone(),
            });
        }
        if let Some(parent) = exe.parent() {
            hits.extend(beside(parent, "next to Melon"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        hits.extend(beside(&cwd, "working directory"));
        hits.extend(beside(&cwd.join("target/release"), "target/release"));
        hits.extend(beside(&cwd.join("target/debug"), "target/debug"));
    }
    hits.extend(beside(&home.join("target/release"), "target/release"));
    hits.extend(beside(&home.join("target/debug"), "target/debug"));
    hits.extend(on_path());
    hits.sort_by_key(|b| rank(&b.path));
    let mut seen = Vec::new();
    hits.retain(|b| {
        let ok = b.path.is_file() && !seen.iter().any(|p| p == &b.path);
        if ok {
            seen.push(b.path.clone());
        }
        ok
    });
    hits.into_iter().next()
}

fn names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["melon.exe", "melon-client.exe", "melon-game.exe"]
    } else {
        &["melon", "melon-client", "melon-game"]
    }
}

/// The game client, not `melon-launcher`.
pub fn is_game_name(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        let n = n.to_ascii_lowercase();
        matches!(
            n.as_str(),
            "melon"
                | "melon.exe"
                | "melon-client"
                | "melon-client.exe"
                | "melon-game"
                | "melon-game.exe"
        )
    })
}

fn is_melon_name(path: &Path) -> bool {
    is_game_name(path)
}

fn beside(folder: &Path, label: &str) -> Vec<Binary> {
    names()
        .iter()
        .map(|name| folder.join(name))
        .filter(|p| p.is_file())
        .map(|path| Binary {
            label: label.into(),
            path,
        })
        .collect()
}

fn on_path() -> Vec<Binary> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in std::env::split_paths(&path) {
        for name in names() {
            let p = dir.join(name);
            if p.is_file() {
                out.push(Binary {
                    label: "PATH".into(),
                    path: p,
                });
            }
        }
    }
    out
}

fn rank(path: &Path) -> u8 {
    let text = path.to_string_lossy();
    if text.contains("target/release") {
        0
    } else if text.contains("target/debug") {
        1
    } else if text.contains("/usr") {
        3
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_binary_next_to_home_is_found() {
        let dir = std::env::temp_dir().join(format!("melon-bin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = names()[0];
        let path = dir.join(name);
        fs::write(&path, b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = fs::metadata(&path).unwrap().permissions();
            p.set_mode(0o755);
            let _ = fs::set_permissions(&path, p);
        }
        let hits = beside(&dir, "test");
        assert!(hits.iter().any(|b| b.path == path));
        assert!(is_game_name(&path));
        assert!(!is_game_name(&dir.join("melon-launcher")));
        let _ = fs::remove_dir_all(dir);
    }
}
