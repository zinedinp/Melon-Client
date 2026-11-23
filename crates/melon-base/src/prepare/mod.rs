//! Find a legal Minecraft 26.3 client on this machine and turn it into the assets Melon reads.

use crate::home::{self, VERSION};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

mod binaries;
pub mod download;
pub mod hashes;
mod sources;

pub use binaries::{Binary, is_game_name, scan as scan_binaries};
pub use sources::{Found, from_jar, scan};

/// Progress of a prepare: files done of how many, plus a line for the UI.
#[derive(Clone, Debug)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub message: String,
}

/// What a finished prepare used.
#[derive(Clone, Debug)]
pub struct Report {
    pub source: String,
    pub jar: PathBuf,
}

/// Looks for 26.3 next to `home` and in the default launcher folders.
pub fn from_local(
    home: &Path,
    assets_dir: &Path,
    log: &dyn Fn(Progress),
) -> Result<Report, String> {
    let local = scan(home);
    let found = match local {
        Some(found) if found.asset_index.is_some() => found,
        other => match download::fetch(home, other.as_ref(), log) {
            Ok(fetched) => fetched,
            Err(e) => other.ok_or_else(|| {
                format!(
                    "No Minecraft {VERSION} client on this computer and the download failed: {e}. \
                     Put {VERSION}.jar next to Melon, choose a jar, or install the official launcher."
                )
            })?,
        },
    };
    extract_into(found, assets_dir, log)
}

/// Unzip a chosen client jar into `assets_dir`. Zip only: no Python, no Java.
pub fn from_path(jar: &Path, assets_dir: &Path, log: &dyn Fn(Progress)) -> Result<Report, String> {
    if !jar.is_file() {
        return Err(format!("{} is not a file.", jar.display()));
    }
    extract_into(from_jar(jar.to_path_buf()), assets_dir, log)
}

/// [`from_path`] that fills in an asset index the jar has none of beside it:
/// fetched (only what is missing) into Melon's cache under `home`, else the
/// one already cached, else the jar alone. The jar's own panoramas are 1x1
/// placeholders and most sounds exist only in the index.
pub fn from_path_in(
    home: &Path,
    jar: &Path,
    assets_dir: &Path,
    log: &dyn Fn(Progress),
) -> Result<Report, String> {
    if !jar.is_file() {
        return Err(format!("{} is not a file.", jar.display()));
    }
    let found = from_jar(jar.to_path_buf());
    let found = if found.asset_index.is_some() {
        found
    } else {
        match download::fetch(home, Some(&found), log) {
            Ok(fetched) => fetched,
            Err(e) => {
                let found = sources::with_cached_index(found, home);
                if found.asset_index.is_none() {
                    log(Progress {
                        done: 0,
                        total: 3,
                        message: format!(
                            "No asset index ({e}): the panorama and most sounds will be missing."
                        ),
                    });
                }
                found
            }
        }
    };
    extract_into(found, assets_dir, log)
}

pub fn extract_into(
    found: Found,
    assets_dir: &Path,
    log: &dyn Fn(Progress),
) -> Result<Report, String> {
    extract(&found, assets_dir, log)?;
    Ok(Report {
        source: found.label,
        jar: found.jar,
    })
}

/// Welcome stays up when the folder is empty or the extracted jar no longer matches.
pub fn needs_extract(home: &Path, assets_dir: &Path) -> bool {
    !home::is_prepared_at(assets_dir) || is_stale(assets_dir, scan(home).as_ref())
}

fn is_stale(assets_dir: &Path, found: Option<&Found>) -> bool {
    let Some(found) = found else {
        return false;
    };
    let Ok(text) = fs::read_to_string(home::prepared_marker(assets_dir)) else {
        return home::is_prepared_at(assets_dir);
    };
    let Ok(marker) = serde_json::from_str::<serde_json::Value>(&text) else {
        return true;
    };
    let version_ok = marker["version"].as_str() == Some(VERSION);
    let sha_ok = match (marker["sha1"].as_str(), sha1_file(&found.jar)) {
        (Some(saved), Some(now)) => saved == now,
        _ => false,
    };
    !version_ok || !sha_ok
}

fn extract(found: &Found, assets_dir: &Path, log: &dyn Fn(Progress)) -> Result<(), String> {
    log(Progress {
        done: 0,
        total: 3,
        message: format!("Using {} ({})", found.label, found.jar.display()),
    });
    let dest = assets_dir.join("minecraft");
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    unzip_minecraft_assets(&found.jar, &dest, log)?;
    if let Some(index) = &found.asset_index {
        overlay_index_objects(index, &found.objects, &dest, log)?;
    }
    write_marker(assets_dir, found)?;
    log(Progress {
        done: 3,
        total: 3,
        message: "Ready.".into(),
    });
    Ok(())
}

/// Unzip `assets/minecraft/` from `jar` into `dest` (the minecraft assets tree).
pub fn extract_jar_assets(jar: &Path, dest: &Path, log: &dyn Fn(Progress)) -> Result<(), String> {
    unzip_minecraft_assets(jar, dest, log)
}

/// `rel` as a path that stays inside the folder it is joined to: plain names only, no root,
/// drive, `.` or `..`.
fn contained(rel: &str) -> Option<&Path> {
    let path = Path::new(rel);
    let plain = path.components().all(|c| matches!(c, Component::Normal(_)));
    (plain && path.components().next().is_some()).then_some(path)
}

/// Each folder is made once, and entries outside `assets/minecraft/` are skipped by name,
/// unopened. Creating the files is what takes the time (on Windows, mostly the virus
/// scanner's; more threads don't help, it checks one file after another).
fn unzip_minecraft_assets(jar: &Path, dest: &Path, log: &dyn Fn(Progress)) -> Result<(), String> {
    let bytes = fs::read(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
    let mut zip = zip::ZipArchive::new(io::Cursor::new(bytes.as_slice()))
        .map_err(|e| format!("{}: {e}", jar.display()))?;
    let mut jobs = Vec::new();
    for (i, name) in zip.file_names().enumerate() {
        let name = name
            .map_err(|e| format!("jar entry {i}: {e}"))?
            .replace('\\', "/");
        let Some(rel) = name.strip_prefix("assets/minecraft/") else {
            continue;
        };
        if rel.ends_with('/') {
            continue;
        }
        if let Some(rel) = contained(rel) {
            jobs.push((i, dest.join(rel)));
        }
    }
    let mut dirs: Vec<&Path> = jobs.iter().filter_map(|(_, out)| out.parent()).collect();
    dirs.sort_unstable();
    dirs.dedup();
    for dir in dirs {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let total = jobs.len().max(1);
    for (done, (i, out)) in jobs.iter().enumerate() {
        let mut entry = zip
            .by_index(*i)
            .map_err(|e| format!("jar entry {i}: {e}"))?;
        let mut file = fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
        io::copy(&mut entry, &mut file).map_err(|e| format!("{}: {e}", out.display()))?;
        if done % 200 == 0 {
            let rel = out.strip_prefix(dest).unwrap_or(out);
            log(Progress {
                done,
                total,
                message: format!("Extracting {}", rel.display()),
            });
        }
    }
    log(Progress {
        done: 2,
        total: 3,
        message: "Client jar extracted.".into(),
    });
    Ok(())
}

/// Copies objects from an official `assets/objects` store into the tree.
fn overlay_index_objects(
    index: &Path,
    objects: &Path,
    dest: &Path,
    log: &dyn Fn(Progress),
) -> Result<(), String> {
    let text = fs::read_to_string(index).map_err(|e| format!("{}: {e}", index.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", index.display()))?;
    let Some(map) = value.get("objects").and_then(|v| v.as_object()) else {
        return Ok(());
    };
    let mut copied = 0;
    for (name, meta) in map {
        let Some(rel) = name.strip_prefix("minecraft/").and_then(contained) else {
            continue;
        };
        let Some(hash) = meta.get("hash").and_then(|v| v.as_str()) else {
            continue;
        };
        // A SHA-1, which names the object's file: nothing else may pick a path.
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let src = objects.join(&hash[..2]).join(hash);
        if !src.is_file() {
            continue;
        }
        let out = dest.join(rel);
        if let Some(parent) = out.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::copy(&src, &out).is_ok() {
            copied += 1;
        }
    }
    log(Progress {
        done: 3,
        total: 3,
        message: format!("Copied {copied} extra assets from the index."),
    });
    Ok(())
}

fn write_marker(assets_dir: &Path, found: &Found) -> Result<(), String> {
    let marker = home::prepared_marker(assets_dir);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let sha1 = sha1_file(&found.jar).unwrap_or_default();
    let body = serde_json::json!({
        "version": VERSION,
        "jar": found.jar.to_string_lossy(),
        "sha1": sha1,
    });
    fs::write(&marker, body.to_string()).map_err(|e| format!("{}: {e}", marker.display()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0xf) as usize] as char);
    }
    out
}

pub fn sha1_file(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex(hasher.finalize().as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("melon-prepare-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_plain_relative_paths_are_contained() {
        for ok in [
            "lang/en_us.json",
            "a//b.png",
            "foo..png",
            "textures/block/stone.png",
        ] {
            assert!(contained(ok).is_some(), "{ok}");
        }
        for bad in ["", "/home/user/.bashrc", "../x", "a/../../x", ".."] {
            assert!(contained(bad).is_none(), "{bad}");
        }
        #[cfg(windows)]
        for bad in ["C:/Windows/x", "C:x", "//server/share/x"] {
            assert!(contained(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_jar_entry_with_an_absolute_path_stays_out() {
        let dir = scratch("jar-abs");
        let outside = dir.join("outside.txt");
        let jar = dir.join("evil.jar");
        {
            let file = fs::File::create(&jar).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opt = zip::write::SimpleFileOptions::default();
            let evil = format!(
                "assets/minecraft/{}",
                outside.to_string_lossy().replace('\\', "/")
            );
            zip.start_file(evil, opt).unwrap();
            zip.write_all(b"escaped").unwrap();
            zip.start_file("assets/minecraft/lang/en_us.json", opt)
                .unwrap();
            zip.write_all(b"{}").unwrap();
            zip.finish().unwrap();
        }
        let dest = dir.join("dest");
        fs::create_dir_all(&dest).unwrap();
        extract_jar_assets(&jar, &dest, &|_| {}).unwrap();
        assert!(
            !outside.exists(),
            "an entry was written outside the destination"
        );
        assert!(dest.join("lang/en_us.json").is_file());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_jar_with_one_texture_extracts_into_assets() {
        let dir = scratch("jar");
        let jar = dir.join("26.3.jar");
        {
            let file = fs::File::create(&jar).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opt = zip::write::SimpleFileOptions::default();
            zip.start_file("assets/minecraft/lang/en_us.json", opt)
                .unwrap();
            zip.write_all(br#"{"menu.game":"Game"}"#).unwrap();
            zip.start_file("assets/minecraft/textures/block/stone.png", opt)
                .unwrap();
            zip.write_all(&[0u8; 8]).unwrap();
            zip.finish().unwrap();
        }
        let home = scratch("home");
        let found = Found {
            label: "test".into(),
            jar,
            asset_index: None,
            objects: PathBuf::new(),
        };
        let assets = home.join("assets");
        extract(&found, &assets, &|_| {}).unwrap();
        assert!(
            assets.join("minecraft/lang/en_us.json").is_file(),
            "lang missing"
        );
        assert!(home::is_prepared_at(&assets));
        extract(&found, &assets, &|_| {}).unwrap();
        assert!(home::is_prepared_at(&assets));
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn extracting_one_version_leaves_a_sibling_version() {
        let dir = scratch("jar-sib");
        let jar = dir.join("client.jar");
        {
            let file = fs::File::create(&jar).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opt = zip::write::SimpleFileOptions::default();
            zip.start_file("assets/minecraft/lang/en_us.json", opt)
                .unwrap();
            zip.write_all(br#"{"menu.game":"Game"}"#).unwrap();
            zip.start_file("assets/minecraft/textures/block/stone.png", opt)
                .unwrap();
            zip.write_all(&[0u8; 8]).unwrap();
            zip.finish().unwrap();
        }
        let found = Found {
            label: "test".into(),
            jar,
            asset_index: None,
            objects: PathBuf::new(),
        };
        let base = scratch("assets-sib");
        let v263 = home::version_assets(&base, "26.3");
        let v264 = home::version_assets(&base, "26.4");
        let v119 = home::version_assets(&base, "1.19.1");
        extract(&found, &v263, &|_| {}).unwrap();
        fs::create_dir_all(v119.join("minecraft/textures")).unwrap();
        fs::create_dir_all(v119.join("minecraft/lang")).unwrap();
        fs::write(v119.join("minecraft/lang/en_us.json"), "{}").unwrap();
        fs::write(home::prepared_marker(&v119), "1.19.1").unwrap();
        extract(&found, &v264, &|_| {}).unwrap();
        assert!(
            home::is_prepared_at(&v263),
            "26.3 must stay after 26.4 extract"
        );
        assert!(home::is_prepared_at(&v264));
        assert!(
            home::is_prepared_at(&v119),
            "1.19.1 must stay after 26.4 extract"
        );
        assert_ne!(v263, v264);
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(base);
    }
}
