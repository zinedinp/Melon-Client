//! Fetches what a prepare needs from Mojang when it isn't on this computer.

use super::{Found, Progress, VERSION, hex, sha1_file};
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const UA: &str = concat!(
    "Melon/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/zinedinp/Melon-Client)"
);
const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const CDN: &str = "https://resources.download.minecraft.net";
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const JAR_LIMIT: u64 = 512 * 1024 * 1024;
/// Parallel object downloads.
const WORKERS: usize = 6;
/// What the client streams or plays rarely and long.
const SKIPPED: [&str; 2] = ["minecraft/sounds/music/", "minecraft/sounds/records/"];

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionFile {
    downloads: Downloads,
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndex,
}

#[derive(Deserialize)]
struct Downloads {
    client: Download,
}

#[derive(Deserialize)]
struct Download {
    url: String,
    sha1: String,
}

#[derive(Deserialize)]
struct AssetIndex {
    id: String,
    url: String,
    sha1: String,
}

/// The objects of an index worth fetching, as (name, hash).
fn wanted(index: &serde_json::Value) -> Vec<(String, String)> {
    let Some(map) = index.get("objects").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    map.iter()
        .filter(|(name, _)| {
            name.starts_with("minecraft/") && !SKIPPED.iter().any(|s| name.starts_with(s))
        })
        .filter_map(|(name, meta)| {
            let hash = meta.get("hash")?.as_str()?;
            (hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| (name.clone(), hash.to_ascii_lowercase()))
        })
        .collect()
}

fn get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = ureq::get(url)
        .header("User-Agent", UA)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

fn sha1_bytes(bytes: &[u8]) -> String {
    hex(Sha1::digest(bytes).as_ref())
}

/// Writes through a temporary name, so an interrupted download never leaves a broken file.
fn store(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let part = path.with_extension("part");
    fs::write(&part, bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    fs::rename(&part, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `url` into `path` unless a file with `sha1` is there.
fn fetch_file(url: &str, sha1: &str, path: &Path, limit: u64) -> Result<bool, String> {
    if sha1_file(path).is_some_and(|have| have.eq_ignore_ascii_case(sha1)) {
        return Ok(false);
    }
    let bytes = get(url, limit)?;
    if !sha1_bytes(&bytes).eq_ignore_ascii_case(sha1) {
        return Err(format!("{url}: SHA-1 mismatch"));
    }
    store(path, &bytes)?;
    Ok(true)
}

/// Downloads the client, its index and the objects into `<home>/minecraft`
pub fn fetch(
    home: &Path,
    local_jar: Option<&Found>,
    log: &dyn Fn(Progress),
) -> Result<Found, String> {
    let say = |done: usize, total: usize, message: String| {
        log(Progress {
            done,
            total,
            message,
        })
    };
    say(0, 3, format!("Asking Mojang for Minecraft {VERSION}…"));
    let manifest: Manifest = serde_json::from_slice(&get(MANIFEST, 64 << 20)?)
        .map_err(|e| format!("version manifest: {e}"))?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == VERSION)
        .ok_or_else(|| format!("Mojang lists no version {VERSION}."))?;
    let version: VersionFile = serde_json::from_slice(&get(&entry.url, 64 << 20)?)
        .map_err(|e| format!("{VERSION}.json: {e}"))?;

    let root = crate::home::minecraft_cache(home);
    let jar = root
        .join("versions")
        .join(VERSION)
        .join(format!("{VERSION}.jar"));
    let (jar, label) = match local_jar {
        Some(local) => (local.jar.clone(), local.label.clone()),
        None => {
            say(1, 3, format!("Client jar {VERSION}…"));
            fetch_file(
                &version.downloads.client.url,
                &version.downloads.client.sha1,
                &jar,
                JAR_LIMIT,
            )?;
            (jar, "Mojang download".to_owned())
        }
    };

    let index_path = root
        .join("assets/indexes")
        .join(format!("{}.json", version.asset_index.id));
    fetch_file(
        &version.asset_index.url,
        &version.asset_index.sha1,
        &index_path,
        64 << 20,
    )?;
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(&index_path).map_err(|e| format!("{}: {e}", index_path.display()))?,
    )
    .map_err(|e| format!("asset index: {e}"))?;

    let objects = root.join("assets/objects");
    fetch_objects(&wanted(&index), &objects, &say)?;
    Ok(Found {
        label,
        jar,
        asset_index: Some(index_path),
        objects,
    })
}

/// The objects not yet in the store, fetched by a few threads at once.
fn fetch_objects(
    list: &[(String, String)],
    store_dir: &Path,
    say: &dyn Fn(usize, usize, String),
) -> Result<(), String> {
    let path_of = |hash: &str| -> PathBuf { store_dir.join(&hash[..2]).join(hash) };
    // Many names share one object: fetched once, or two threads would write
    // the same `.part` file.
    let mut seen = std::collections::HashSet::new();
    let missing: Vec<&(String, String)> = list
        .iter()
        .filter(|(_, hash)| seen.insert(hash.as_str()) && !path_of(hash).is_file())
        .collect();
    let total = missing.len();
    if total == 0 {
        return Ok(());
    }
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failure: Mutex<Option<String>> = Mutex::new(None);
    let (tx, rx) = std::sync::mpsc::channel::<(usize, String)>();
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(total) {
            let tx = tx.clone();
            let (next, done, failure, missing) = (&next, &done, &failure, &missing);
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((name, hash)) = missing.get(i) else {
                        return;
                    };
                    if failure.lock().is_ok_and(|f| f.is_some()) {
                        return;
                    }
                    let url = format!("{CDN}/{}/{hash}", &hash[..2]);
                    if let Err(e) = fetch_file(&url, hash, &path_of(hash), 256 << 20) {
                        if let Ok(mut f) = failure.lock() {
                            f.get_or_insert(e);
                        }
                        return;
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 50 == 0 || n == total {
                        let _ = tx.send((n, name.clone()));
                    }
                }
            });
        }
        drop(tx);
        for (n, name) in rx {
            say(2, 3, format!("Assets {n}/{total} ({name})"));
        }
    });
    match failure.into_inner() {
        Ok(Some(e)) => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_and_foreign_namespaces_are_not_fetched() {
        let index = serde_json::json!({"objects": {
            "minecraft/sounds.json": {"hash": "a".repeat(40)},
            "minecraft/sounds/music/menu1.ogg": {"hash": "b".repeat(40)},
            "minecraft/sounds/records/cat.ogg": {"hash": "c".repeat(40)},
            "minecraft/lang/de_de.json": {"hash": "d".repeat(40)},
            "realms/x.png": {"hash": "e".repeat(40)},
            "minecraft/bad": {"hash": "zz"}
        }});
        let mut names: Vec<_> = wanted(&index).into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(
            names,
            ["minecraft/lang/de_de.json", "minecraft/sounds.json"]
        );
    }

    #[test]
    fn a_file_that_is_there_and_right_is_not_fetched_again() {
        let dir = std::env::temp_dir().join(format!("melon-dl-{}", std::process::id()));
        let path = dir.join("x");
        store(&path, b"hello").unwrap();
        let sha = sha1_bytes(b"hello");
        assert_eq!(
            fetch_file("http://127.0.0.1:1/x", &sha, &path, 10),
            Ok(false)
        );
        assert!(fetch_file("http://127.0.0.1:1/x", &"0".repeat(40), &path, 10).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    /// Downloads ~220 MB: `MELON_TEST_FETCH=1 cargo test -- --ignored fetches_a_real_client`.
    #[test]
    #[ignore]
    fn fetches_a_real_client() {
        let home = std::env::temp_dir().join("melon-fetch-test");
        let found = fetch(&home, None, &|p| {
            eprintln!("{}/{} {}", p.done, p.total, p.message)
        })
        .expect("download");
        assert!(found.jar.is_file() && found.asset_index.is_some());
        let out = home.join("assets/26.3");
        super::super::extract_into(found, &out, &|_| {}).expect("extract");
        for rel in [
            "sounds.json",
            "lang/de_de.json",
            "font/unifont.zip",
            "textures/gui/title/background/panorama_0.png",
        ] {
            assert!(out.join("minecraft").join(rel).is_file(), "{rel}");
        }
    }
}
