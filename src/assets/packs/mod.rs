//! Resource packs over the vanilla `assets/` tree, like vanilla's `PackRepository`.

mod archive;
pub mod meta;
pub mod repository;
pub mod server;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use archive::ZipPack;
pub use meta::{Compatibility, PackMeta};

/// Where a pack's files are.
enum Source {
    Folder(PathBuf),
    Zip(ZipPack),
}

impl Source {
    /// `name` is `/`-separated and already checked by [`valid_path`].
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        match self {
            Self::Folder(root) => std::fs::read(join(root, name)).ok(),
            Self::Zip(zip) => zip.read(name),
        }
    }

    fn contains(&self, name: &str) -> bool {
        match self {
            Self::Folder(root) => join(root, name).is_file(),
            Self::Zip(zip) => zip.contains(name),
        }
    }

    /// Every file below the folder `prefix` (ending in `/`), as names relative to the pack.
    fn list(&self, prefix: &str, out: &mut BTreeSet<String>) {
        match self {
            Self::Folder(root) => {
                let mut stack = vec![join(root, prefix.trim_end_matches('/'))];
                while let Some(dir) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&dir) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if let Ok(rel) = path.strip_prefix(root) {
                            let parts: Vec<_> = rel
                                .components()
                                .filter_map(|c| c.as_os_str().to_str())
                                .collect();
                            out.insert(parts.join("/"));
                        }
                    }
                }
            }
            Self::Zip(zip) => {
                out.extend(
                    zip.names()
                        .filter(|n| n.starts_with(prefix))
                        .map(str::to_owned),
                );
            }
        }
    }
}

/// `base` with the `/`-separated `name` below it.
fn join(base: &Path, name: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    path.extend(name.split('/').filter(|part| !part.is_empty()));
    path
}

/// One pack ready to read from.
pub struct Pack {
    /// `file/<name>` for a pack from the `resourcepacks` folder.
    pub id: String,
    /// The file or folder name, shown as the pack's title.
    pub title: String,
    pub meta: PackMeta,
    source: Source,
    /// The overlay folders that apply, lowest first.
    overlays: Vec<String>,
}

impl Pack {
    /// Opens a folder or zip pack.
    pub fn open(id: String, title: String, path: &Path) -> anyhow::Result<Self> {
        let source = if path.is_dir() {
            Source::Folder(path.to_path_buf())
        } else {
            Source::Zip(ZipPack::open(path)?)
        };
        let meta = source
            .read("pack.mcmeta")
            .and_then(|bytes| PackMeta::parse(&bytes))
            .ok_or_else(|| anyhow::anyhow!("{title}: pack.mcmeta missing or broken"))?;
        Ok(Self {
            id,
            title,
            overlays: meta.active_overlays(),
            meta,
            source,
        })
    }

    /// Extracted zip-mod folder: `assets/` at the root, no `pack.mcmeta`.
    #[must_use]
    pub fn open_mod(id: String, title: String, extracted: &Path) -> Self {
        Self {
            id,
            title,
            overlays: Vec::new(),
            meta: PackMeta::for_mod(),
            source: Source::Folder(extracted.to_path_buf()),
        }
    }

    /// `pack.png`, if there is one.
    pub fn icon(&self) -> Option<Vec<u8>> {
        self.source.read("pack.png")
    }

    /// `assets/<namespace>/<path>`, from the newest overlay that has it, else the pack's root.
    fn read(&self, namespace: &str, path: &str) -> Option<Vec<u8>> {
        let name = format!("assets/{namespace}/{path}");
        for overlay in self.overlays.iter().rev() {
            let overlaid = format!("{overlay}/{name}");
            if self.source.contains(&overlaid) {
                return self.source.read(&overlaid);
            }
        }
        self.source.read(&name)
    }

    /// The paths (relative to the namespace) of every file under `dir/`.
    pub fn list(&self, namespace: &str, dir: &str, out: &mut BTreeSet<String>) {
        let base = format!("assets/{namespace}/");
        let mut names = BTreeSet::new();
        for root in
            std::iter::once(String::new()).chain(self.overlays.iter().map(|o| format!("{o}/")))
        {
            names.clear();
            self.source.list(&format!("{root}{base}{dir}/"), &mut names);
            out.extend(
                names
                    .iter()
                    .filter_map(|n| n.strip_prefix(&root)?.strip_prefix(&base))
                    .map(str::to_owned),
            );
        }
    }

    /// How many files the pack has under each `assets/<namespace>/<folder>/`
    pub fn inventory(&self) -> std::collections::BTreeMap<(String, String), usize> {
        let mut out = std::collections::BTreeMap::new();
        let mut names = BTreeSet::new();
        for root in
            std::iter::once(String::new()).chain(self.overlays.iter().map(|o| format!("{o}/")))
        {
            names.clear();
            self.source.list(&format!("{root}assets/"), &mut names);
            for name in &names {
                let Some(rest) = name
                    .strip_prefix(&root)
                    .and_then(|n| n.strip_prefix("assets/"))
                else {
                    continue;
                };
                let mut parts = rest.splitn(3, '/');
                if let (Some(ns), Some(folder), Some(_)) =
                    (parts.next(), parts.next(), parts.next())
                {
                    *out.entry((ns.to_owned(), folder.to_owned())).or_insert(0) += 1;
                }
            }
        }
        out
    }

    /// The namespaces under `assets/` (and the overlays' `assets/`).
    fn namespaces(&self, out: &mut BTreeSet<String>) {
        let mut names = BTreeSet::new();
        for root in
            std::iter::once(String::new()).chain(self.overlays.iter().map(|o| format!("{o}/")))
        {
            names.clear();
            self.source.list(&format!("{root}assets/"), &mut names);
            for name in &names {
                if let Some(namespace) = name
                    .strip_prefix(&root)
                    .and_then(|n| n.strip_prefix("assets/"))
                    .and_then(|n| n.split_once('/'))
                    .map(|(ns, _)| ns)
                    .filter(|ns| valid_namespace(ns))
                {
                    out.insert(namespace.to_owned());
                }
            }
        }
    }
}

/// The packs in use above vanilla, lowest first.
#[derive(Default)]
pub struct ResourceStack {
    pub packs: Vec<Arc<Pack>>,
}

/// Which layer a file came from: `0` is vanilla, `n` is `packs[n - 1]`.
pub type Layer = usize;

impl ResourceStack {
    /// The top file at `namespace:path` and its layer. A pack's `filter`
    pub fn find(&self, root: &Path, namespace: &str, path: &str) -> Option<(Vec<u8>, Layer)> {
        if !valid_namespace(namespace) || !valid_path(path) {
            return None;
        }
        for (i, pack) in self.packs.iter().enumerate().rev() {
            if let Some(bytes) = pack.read(namespace, path) {
                return Some((bytes, i + 1));
            }
            if pack.meta.filters(namespace, path) {
                return None;
            }
        }
        std::fs::read(join(&root.join(namespace), path))
            .ok()
            .map(|b| (b, 0))
    }

    /// Whether some layer has `namespace:path`, without reading it.
    pub fn contains(&self, root: &Path, namespace: &str, path: &str) -> bool {
        if !valid_namespace(namespace) || !valid_path(path) {
            return false;
        }
        for pack in self.packs.iter().rev() {
            let name = format!("assets/{namespace}/{path}");
            if pack.source.contains(&name)
                || pack
                    .overlays
                    .iter()
                    .any(|o| pack.source.contains(&format!("{o}/{name}")))
            {
                return true;
            }
            if pack.meta.filters(namespace, path) {
                return false;
            }
        }
        join(&root.join(namespace), path).is_file()
    }

    /// `path.mcmeta` for a file from `layer`.
    pub fn find_metadata(
        &self,
        root: &Path,
        namespace: &str,
        path: &str,
        layer: Layer,
    ) -> Option<Vec<u8>> {
        let meta = format!("{path}.mcmeta");
        if !valid_namespace(namespace) || !valid_path(&meta) {
            return None;
        }
        for (i, pack) in self.packs.iter().enumerate().rev() {
            if i + 1 < layer {
                return None;
            }
            if let Some(bytes) = pack.read(namespace, &meta) {
                return Some(bytes);
            }
            if pack.meta.filters(namespace, &meta) {
                return None;
            }
        }
        if layer > 0 {
            return None;
        }
        std::fs::read(join(&root.join(namespace), &meta)).ok()
    }

    /// Every copy of `namespace:path`, vanilla's first, for files the packs merge.
    pub fn find_all(&self, root: &Path, namespace: &str, path: &str) -> Vec<Vec<u8>> {
        if !valid_namespace(namespace) || !valid_path(path) {
            return Vec::new();
        }
        let mut found = Vec::new();
        let mut reached_vanilla = true;
        for pack in self.packs.iter().rev() {
            if let Some(bytes) = pack.read(namespace, path) {
                found.push(bytes);
            }
            if pack.meta.filters(namespace, path) {
                reached_vanilla = false;
                break;
            }
        }
        if reached_vanilla && let Ok(bytes) = std::fs::read(join(&root.join(namespace), path)) {
            found.push(bytes);
        }
        found.reverse();
        found
    }

    /// The paths (relative to `namespace`) of every file under `dir/`
    pub fn list(&self, root: &Path, namespace: &str, dir: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        if !valid_namespace(namespace) || !valid_path(dir) {
            return out;
        }
        Source::Folder(root.join(namespace)).list(&format!("{dir}/"), &mut out);
        for pack in &self.packs {
            if pack.meta.filters_namespace(namespace) {
                out.retain(|path| !pack.meta.filters(namespace, path));
            }
            pack.list(namespace, dir, &mut out);
        }
        out
    }

    /// Every namespace with files in some layer.
    pub fn namespaces(&self, root: &Path) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|ns| valid_namespace(ns))
            .collect();
        for pack in &self.packs {
            pack.namespaces(&mut out);
        }
        out
    }

    /// The pack ids, lowest first: what [`activate`] was last given.
    pub fn ids(&self) -> Vec<String> {
        self.packs.iter().map(|p| p.id.clone()).collect()
    }
}

static ACTIVE: RwLock<Option<Arc<ResourceStack>>> = RwLock::new(None);

/// The packs every resource is read through.
pub fn active() -> Arc<ResourceStack> {
    ACTIVE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Reads through `stack` from now on.
pub fn activate(stack: ResourceStack) {
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(stack));
}

/// `Identifier.isValidNamespace`.
pub fn valid_namespace(namespace: &str) -> bool {
    !namespace.is_empty()
        && namespace
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'))
}

/// `Identifier.isValidPath`, plus no `.`/`..` parts so nothing leaves the pack.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'))
        && path.split('/').all(|part| part != ".." && part != ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder_pack(dir: &Path, id: &str, mcmeta: &str, files: &[(&str, &str)]) -> Arc<Pack> {
        let root = dir.join(id);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("pack.mcmeta"), mcmeta).unwrap();
        for (name, body) in files {
            let path = join(&root, name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        Arc::new(Pack::open(format!("file/{id}"), id.to_owned(), &root).unwrap())
    }

    /// A zip in `resourcepacks/` overrides a texture with an HD one, adds a file, and every
    /// reader sees both.
    #[test]
    fn zip_pack_reaches_every_reader() {
        use std::io::Write;
        let game = std::env::temp_dir().join(format!("melon-zip-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        let vanilla = game.join("assets");
        std::fs::create_dir_all(vanilla.join("minecraft/textures/block")).unwrap();
        std::fs::create_dir_all(vanilla.join("minecraft/lang")).unwrap();
        image::RgbaImage::from_pixel(16, 16, image::Rgba([1, 2, 3, 255]))
            .save(vanilla.join("minecraft/textures/block/stone.png"))
            .unwrap();
        std::fs::write(
            vanilla.join("minecraft/lang/en_us.json"),
            r#"{"a":"vanilla a","b":"vanilla b"}"#,
        )
        .unwrap();
        std::fs::write(
            vanilla.join("minecraft/sounds.json"),
            r#"{"x":{"sounds":["one"]}}"#,
        )
        .unwrap();

        let packs = repository::folder(&game);
        std::fs::create_dir_all(&packs).unwrap();
        let mut hd = Vec::new();
        image::RgbaImage::from_pixel(64, 64, image::Rgba([9, 9, 9, 255]))
            .write_to(&mut std::io::Cursor::new(&mut hd), image::ImageFormat::Png)
            .unwrap();
        let mut zip =
            zip::ZipWriter::new(std::fs::File::create(packs.join("HD Pack.zip")).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            (
                "pack.mcmeta",
                &br#"{"pack":{"description":"\u00a7cRed","min_format":97,"max_format":99}}"#[..],
            ),
            ("assets/minecraft/textures/block/stone.png", &hd[..]),
            (
                "assets/minecraft/lang/en_us.json",
                &br#"{"b":"pack b"}"#[..],
            ),
            (
                "assets/minecraft/sounds.json",
                &br#"{"x":{"sounds":["two"]},"y":{"replace":true,"sounds":["three"]}}"#[..],
            ),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();

        let available = repository::discover(&game);
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].id, "file/HD Pack.zip");
        assert_eq!(available[0].meta.compatibility(), Compatibility::Compatible);
        let stack = repository::stack(
            &available,
            &["vanilla".into(), "file/HD Pack.zip".into()],
            &[],
        );
        assert_eq!(stack.packs.len(), 1);
        let (png, layer) = stack
            .find(&vanilla, "minecraft", "textures/block/stone.png")
            .unwrap();
        assert_eq!(layer, 1);
        assert_eq!(crate::assets::decode_png(&png).unwrap().width(), 64);
        let langs = stack.find_all(&vanilla, "minecraft", "lang/en_us.json");
        assert_eq!(langs.len(), 2);
        let sounds = stack.find_all(&vanilla, "minecraft", "sounds.json");
        let mut events = crate::audio::library::SoundEvents::default();
        for json in &sounds {
            events.merge("minecraft", json).unwrap();
        }
        assert_eq!(events.len(), 2, "x merged, y added");
        assert!(stack.namespaces(&vanilla).contains("minecraft"));
        std::fs::remove_dir_all(&game).ok();
    }

    #[test]
    fn stacks_filters_and_overlays() {
        let dir = std::env::temp_dir().join(format!("melon-packs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let vanilla = dir.join("vanilla");
        for (name, body) in [
            ("minecraft/texts/a.txt", "vanilla a"),
            ("minecraft/texts/b.txt", "vanilla b"),
            ("minecraft/texts/c.txt", "vanilla c"),
        ] {
            let path = join(&vanilla, name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let low = folder_pack(
            &dir,
            "low",
            r#"{"pack":{"description":"","min_format":97,"max_format":97},
                "overlays":{"entries":[{"directory":"ov","min_format":97,"max_format":97}]}}"#,
            &[
                ("assets/minecraft/texts/a.txt", "low a"),
                ("ov/assets/minecraft/texts/a.txt", "overlay a"),
                ("assets/minecraft/texts/a.txt.mcmeta", "low meta"),
            ],
        );
        let high = folder_pack(
            &dir,
            "high",
            r#"{"pack":{"description":"","pack_format":97},
                "filter":{"block":[{"path":"^texts/b"}]}}"#,
            &[("assets/minecraft/texts/c.txt", "high c")],
        );
        let stack = ResourceStack {
            packs: vec![low, high],
        };
        let read = |p: &str| {
            stack
                .find(&vanilla, "minecraft", p)
                .map(|(b, layer)| (String::from_utf8(b).unwrap(), layer))
        };
        assert_eq!(read("texts/a.txt"), Some(("overlay a".into(), 1)));
        assert_eq!(read("texts/b.txt"), None, "filtered");
        assert_eq!(read("texts/c.txt"), Some(("high c".into(), 2)));
        assert_eq!(read("../vanilla/minecraft/texts/a.txt"), None);
        assert_eq!(
            stack
                .find_metadata(&vanilla, "minecraft", "texts/a.txt", 1)
                .as_deref(),
            Some(&b"low meta"[..])
        );
        assert_eq!(
            stack.find_metadata(&vanilla, "minecraft", "texts/c.txt", 2),
            None
        );
        assert_eq!(
            stack.find_all(&vanilla, "minecraft", "texts/c.txt").len(),
            2
        );
        assert_eq!(
            stack.find_all(&vanilla, "minecraft", "texts/b.txt").len(),
            0
        );
        let listed: Vec<_> = stack
            .list(&vanilla, "minecraft", "texts")
            .into_iter()
            .collect();
        assert_eq!(listed, ["texts/a.txt", "texts/a.txt.mcmeta", "texts/c.txt"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
