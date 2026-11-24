//! Where a 26.3 client jar already lives.

use super::VERSION;
use std::fs;
use std::path::{Path, PathBuf};

/// A client jar plus optional asset index/objects from an official layout.
#[derive(Clone, Debug)]
pub struct Found {
    pub label: String,
    pub jar: PathBuf,
    pub asset_index: Option<PathBuf>,
    pub objects: PathBuf,
}

/// Newest 26.3 jar we can see, official layout preferred (it has the index).
pub fn scan(home: &Path) -> Option<Found> {
    let mut hits = Vec::new();
    hits.extend(beside_melon(home));
    hits.extend(known_installs());
    hits.sort_by_key(|f| {
        let stem = f.jar.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        (f.asset_index.is_none(), stem != VERSION)
    });
    hits.into_iter().next()
}

fn beside_melon(home: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    let names = [
        format!("{VERSION}.jar"),
        "client.jar".into(),
        format!("minecraft-{VERSION}-client.jar"),
    ];
    let folders = [
        home.to_path_buf(),
        home.join("minecraft").join("versions").join(VERSION),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(".")),
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    ];
    for folder in folders {
        for name in &names {
            let jar = folder.join(name);
            if jar.is_file() {
                out.push(loose(&folder, jar, "Melon folder"));
            }
        }
        if let Some(found) = official_version(&folder, "Melon folder") {
            out.push(found);
        }
    }
    out
}

/// Where launchers keep their instances on this OS besides the XDG places.
#[cfg(target_os = "macos")]
fn platform_roots(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let support = home.join("Library/Application Support");
    vec![
        ("the official launcher", support.join("minecraft")),
        ("Prism Launcher", support.join("PrismLauncher")),
        ("Modrinth App", support.join("ModrinthApp")),
        ("XMCL", support.join("xmcl")),
    ]
}

/// Where launchers keep their instances on this OS besides the XDG places.
#[cfg(windows)]
fn platform_roots(_home: &Path) -> Vec<(&'static str, PathBuf)> {
    let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        return Vec::new();
    };
    vec![
        ("the official launcher", appdata.join(".minecraft")),
        ("Prism Launcher", appdata.join("PrismLauncher")),
        ("Modrinth App", appdata.join("ModrinthApp")),
        ("XMCL", appdata.join(".minecraftx")),
        ("XMCL", appdata.join("xmcl")),
    ]
}

/// Where launchers keep their instances on this OS besides the XDG places.
#[cfg(not(any(target_os = "macos", windows)))]
fn platform_roots(_home: &Path) -> Vec<(&'static str, PathBuf)> {
    Vec::new()
}

fn known_installs() -> Vec<Found> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let Some(home) = home else {
        return Vec::new();
    };
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let mut roots: Vec<(&str, PathBuf)> = vec![
        ("the official launcher", home.join(".minecraft")),
        (
            "the official launcher",
            home.join(".var/app/com.mojang.Minecraft/.minecraft"),
        ),
        ("Prism Launcher", data.join("PrismLauncher")),
        (
            "Prism Launcher",
            home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher"),
        ),
        ("MultiMC", data.join("multimc")),
        ("Modrinth App", data.join("ModrinthApp")),
        ("Modrinth App", data.join("com.modrinth.theseus")),
        (
            "Modrinth App",
            home.join(".var/app/com.modrinth.ModrinthApp/data/ModrinthApp"),
        ),
        ("ATLauncher", data.join("atlauncher")),
        ("XMCL", home.join(".minecraftx")),
        ("XMCL", data.join("xmcl")),
        ("XMCL", config.join("xmcl")),
    ];
    roots.extend(platform_roots(&home));
    let mut out = Vec::new();
    for (label, root) in roots {
        out.extend(scan_root(&root, label));
    }
    out
}

/// Official `versions/<id>/<id>.jar`, Modrinth `meta/versions/` and Prism maven jars under
/// `root`.
fn scan_root(root: &Path, label: &str) -> Vec<Found> {
    let mut out = version_jars(root, label);
    out.extend(version_jars(&root.join("meta"), label));
    let maven = root
        .join("libraries/com/mojang/minecraft")
        .join(VERSION)
        .join(format!("minecraft-{VERSION}-client.jar"));
    if maven.is_file() {
        out.push(loose(root, maven, label));
    }
    let atlauncher = root
        .join("libraries/net/minecraft/client")
        .join(VERSION)
        .join(format!("client-{VERSION}.jar"));
    if atlauncher.is_file() {
        out.push(loose(root, atlauncher, label));
    }
    out.extend(instance_jars(root, label));
    out
}

fn official_version(root: &Path, label: &str) -> Option<Found> {
    let dir = root.join("versions").join(VERSION);
    let jar = dir.join(format!("{VERSION}.jar"));
    jar.is_file().then(|| found_at(root, jar, label))
}

/// Vanilla `versions/26.3/26.3.jar` and Modrinth loader copies `26.3-<loader>/`.
fn version_jars(root: &Path, label: &str) -> Vec<Found> {
    let mut out = Vec::new();
    if let Some(found) = official_version(root, label) {
        out.push(found);
    }
    let prefix = format!("{VERSION}-");
    let Ok(entries) = fs::read_dir(root.join("versions")) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let jar = entry.path().join(format!("{name}.jar"));
        if jar.is_file() {
            out.push(found_at(root, jar, label));
        }
    }
    out
}

/// Prism/MultiMC: `instances/<id>/.minecraft`.
fn instance_jars(root: &Path, label: &str) -> Vec<Found> {
    let Ok(entries) = fs::read_dir(root.join("instances")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        out.extend(version_jars(&path.join(".minecraft"), label));
        out.extend(version_jars(&path.join("minecraft"), label));
        out.extend(version_jars(&path, label));
    }
    out
}

fn found_at(root: &Path, jar: PathBuf, label: &str) -> Found {
    Found {
        label: label.into(),
        jar,
        asset_index: newest_index(root),
        objects: root.join("assets/objects"),
    }
}

fn newest_index(root: &Path) -> Option<PathBuf> {
    let dir = root.join("assets/indexes");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files.pop()
}

/// A jar the player picked: look for an official `assets/` tree next to it.
pub fn from_jar(jar: PathBuf) -> Found {
    let folder = jar.parent().unwrap_or(Path::new(".")).to_path_buf();
    let root = folder
        .parent()
        .and_then(|versions| {
            versions
                .file_name()
                .is_some_and(|n| n == "versions")
                .then(|| versions.parent().unwrap_or(versions).to_path_buf())
        })
        .unwrap_or(folder.clone());
    Found {
        label: "chosen file".into(),
        jar,
        asset_index: newest_index(&root).or_else(|| newest_index(&folder)),
        objects: root.join("assets/objects"),
    }
}

/// `found` with the index and objects [`super::download::fetch`] keeps under
/// `home`, when it has none of its own and that cache has one.
pub fn with_cached_index(found: Found, home: &Path) -> Found {
    if found.asset_index.is_some() {
        return found;
    }
    let cache = crate::home::minecraft_cache(home);
    match newest_index(&cache) {
        Some(index) => Found {
            asset_index: Some(index),
            objects: cache.join("assets/objects"),
            ..found
        },
        None => found,
    }
}

fn loose(folder: &Path, jar: PathBuf, label: &str) -> Found {
    Found {
        label: label.into(),
        jar,
        asset_index: newest_index(folder),
        objects: folder.join("assets/objects"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_jar_next_to_home_is_found() {
        let dir = scratch("beside");
        let jar = dir.join(format!("{VERSION}.jar"));
        fs::write(&jar, b"pk").unwrap();
        let hits = beside_melon(&dir);
        assert!(hits.iter().any(|f| f.jar == jar));
        let _ = fs::remove_dir_all(dir);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "melon-src-{name}-{}-{}",
            std::process::id(),
            name.len()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_version(root: &Path, id: &str) -> PathBuf {
        let dir = root.join("versions").join(id);
        fs::create_dir_all(&dir).unwrap();
        let jar = dir.join(format!("{id}.jar"));
        fs::write(&jar, b"pk").unwrap();
        fs::write(dir.join(format!("{id}.json")), b"{\"id\":\"x\"}").unwrap();
        jar
    }

    #[test]
    fn the_modrinth_app_keeps_jars_under_meta_versions() {
        let root = scratch("modrinth");
        let jar = write_version(&root.join("meta"), VERSION);
        fs::create_dir_all(root.join("meta/assets/indexes")).unwrap();
        fs::write(root.join("meta/assets/indexes/26.json"), b"{}").unwrap();
        let hits = scan_root(&root, "Modrinth App");
        assert!(
            hits.iter().any(|f| f.jar == jar && f.asset_index.is_some()),
            "{hits:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_modrinth_loader_profile_is_found_when_vanilla_is_missing() {
        let root = scratch("modrinth-loader");
        let jar = write_version(&root.join("meta"), &format!("{VERSION}-0.19.5"));
        let hits = scan_root(&root, "Modrinth App");
        assert!(hits.iter().any(|f| f.jar == jar), "{hits:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn xmcl_uses_the_official_versions_layout() {
        let root = scratch("xmcl");
        let jar = write_version(&root, VERSION);
        let hits = scan_root(&root, "XMCL");
        assert!(hits.iter().any(|f| f.jar == jar), "{hits:?}");
        let instance = write_version(&root.join("instances/Minecraft"), VERSION);
        let hits = scan_root(&root, "XMCL");
        assert!(hits.iter().any(|f| f.jar == instance), "{hits:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_chosen_jar_uses_the_versions_tree_beside_it() {
        let root = scratch("picked");
        fs::create_dir_all(root.join("assets/indexes")).unwrap();
        fs::write(root.join("assets/indexes/26.json"), b"{}").unwrap();
        let jar = write_version(&root, VERSION);
        let found = from_jar(jar.clone());
        assert_eq!(found.jar, jar);
        assert_eq!(found.label, "chosen file");
        assert!(found.asset_index.is_some(), "{found:?}");
        let _ = fs::remove_dir_all(root);
    }
}
