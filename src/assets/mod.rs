//! The vanilla `assets/` tree.

pub mod atlas_sources;
pub mod json_model;
pub mod packs;
pub mod palette;
pub mod shaders;

use std::path::{Path, PathBuf};

/// Vanilla's game directory.
pub fn game_dir(assets_root: &Path) -> &Path {
    let mut dir = assets_root;
    if let Some(name) = dir.file_name().and_then(|n| n.to_str())
        && crate::home::version_folder(name).is_some()
    {
        dir = dir.parent().unwrap_or(dir);
    }
    if dir.file_name().is_some_and(|n| n == "assets") {
        dir = dir.parent().unwrap_or(dir);
    }
    dir
}

/// `namespace:path`, with `minecraft` when the namespace is left out.
pub fn split_id(identifier: &str) -> (&str, &str) {
    identifier
        .split_once(':')
        .unwrap_or(("minecraft", identifier))
}

/// A resource file's bytes.
pub fn resource(
    assets_root: &Path,
    kind: &str,
    identifier: &str,
    extension: &str,
) -> Option<Vec<u8>> {
    let (namespace, path) = split_id(identifier);
    let found = packs::active()
        .find(
            assets_root,
            namespace,
            &format!("{kind}/{path}.{extension}"),
        )
        .map(|(bytes, _)| bytes);
    if found.is_none() && crate::probe::enabled() {
        crate::probe::count(
            "resource.missing",
            format!("{kind}/{identifier}.{extension}"),
        );
    }
    found
}

/// Whether [`resource`] would find the file, without reading it.
pub fn resource_exists(assets_root: &Path, kind: &str, identifier: &str, extension: &str) -> bool {
    let (namespace, path) = split_id(identifier);
    packs::active().contains(
        assets_root,
        namespace,
        &format!("{kind}/{path}.{extension}"),
    )
}

/// [`resource`], parsed as JSON.
pub fn json_resource(
    assets_root: &Path,
    kind: &str,
    identifier: &str,
) -> Option<serde_json::Value> {
    let bytes = resource(assets_root, kind, identifier, "json")?;
    serde_json::from_slice(packs::meta::strip_bom(&bytes)).ok()
}

/// A texture's PNG (`textures/<path>.png`) with its `.png.mcmeta`, looked up through the
/// resource packs.
pub fn texture_with_meta(
    assets_root: &Path,
    identifier: &str,
) -> Option<(Vec<u8>, Option<serde_json::Value>)> {
    let stack = packs::active();
    let file = match atlas_sources::resolve(assets_root, &stack, identifier) {
        Some(atlas_sources::SpriteFile::Texture(file)) => file,
        Some(atlas_sources::SpriteFile::Paletted { .. }) => return None,
        None => identifier.to_owned(),
    };
    let (namespace, path) = split_id(&file);
    let path = format!("textures/{path}.png");
    let (bytes, layer) = stack.find(assets_root, namespace, &path)?;
    let meta = stack
        .find_metadata(assets_root, namespace, &path, layer)
        .and_then(|m| serde_json::from_slice(packs::meta::strip_bom(&m)).ok());
    Some((bytes, meta))
}

/// A texture, decoded.
pub fn texture_image(assets_root: &Path, identifier: &str) -> Option<image::RgbaImage> {
    texture_with_meta(assets_root, identifier)
        .and_then(|(bytes, _)| decode_png(&bytes))
        .or_else(
            || match atlas_sources::resolve(assets_root, &packs::active(), identifier)? {
                atlas_sources::SpriteFile::Paletted {
                    texture,
                    key,
                    palette,
                } => palette::permute(assets_root, &texture, &key, &palette),
                atlas_sources::SpriteFile::Texture(_) => None,
            },
        )
        .or_else(|| palette::resolve(assets_root, identifier))
}

/// Vanilla's `MissingTextureAtlasSprite`.
pub fn missing_texture() -> image::RgbaImage {
    image::RgbaImage::from_fn(16, 16, |x, y| {
        if (x < 8) == (y < 8) {
            image::Rgba([248, 0, 248, 255])
        } else {
            image::Rgba([0, 0, 0, 255])
        }
    })
}

/// PNG bytes to RGBA, `None` when they aren't a readable PNG.
pub fn decode_png(bytes: &[u8]) -> Option<image::RgbaImage> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()
        .map(|i| i.to_rgba8())
        .or_else(|| decode_png_ignoring_checksums(bytes))
}

/// A PNG whose CRCs or zlib checksum are wrong: vanilla's `NativeImage`
fn decode_png_ignoring_checksums(bytes: &[u8]) -> Option<image::RgbaImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.ignore_checksums(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..info.buffer_size()];
    let (width, height) = (info.width, info.height);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    image::RgbaImage::from_raw(width, height, rgba)
}

/// A sound's Ogg file: `sounds/<path>.ogg`.
pub fn sound(assets_root: &Path, identifier: &str) -> Option<Vec<u8>> {
    resource(assets_root, "sounds", identifier, "ogg")
}

/// Every copy of `namespace:path` (vanilla's first, then each pack's), for files the packs
/// merge.
pub fn resource_stack(assets_root: &Path, namespace: &str, path: &str) -> Vec<Vec<u8>> {
    packs::active().find_all(assets_root, namespace, path)
}

/// The copy of `namespace:path` that wins: the top pack's, else vanilla's.
pub fn resource_top(assets_root: &Path, namespace: &str, path: &str) -> Option<Vec<u8>> {
    packs::active()
        .find(assets_root, namespace, path)
        .map(|(bytes, _)| bytes)
}

/// Every namespace any layer has files in.
pub fn namespaces(assets_root: &Path) -> std::collections::BTreeSet<String> {
    packs::active().namespaces(assets_root)
}

/// The prepared vanilla tree the launcher keeps.
pub fn launcher_root() -> Option<PathBuf> {
    let home = crate::home::dir();
    let settings = crate::instances::LauncherSettings::load(&home);
    let root = settings.melon_play_assets(&home);
    root.join("minecraft").is_dir().then_some(root)
}

/// The assets for tests and tools.
pub fn default_root() -> PathBuf {
    Assets::discover().map_or_else(
        |_| Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"),
        |assets| assets.root,
    )
}

/// Tests' assets: `MELON_ASSETS`, else the prepared ones [`Assets::discover`]
/// finds; `None` without either.
#[cfg(test)]
pub fn test_root() -> Option<PathBuf> {
    std::env::var_os("MELON_ASSETS")
        .map(PathBuf::from)
        .or_else(|| Assets::discover().ok().map(|assets| assets.root))
        .filter(|root| root.join("minecraft").is_dir())
}

/// The post chains tests check: the one in `MELON_PACK_JSON`, else every
/// vanilla `post_effect/*.json` under `root`.
#[cfg(test)]
pub fn test_post_chains(root: Option<&Path>) -> Vec<(String, serde_json::Value)> {
    let read = |path: &Path| {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    if let Some(path) = std::env::var_os("MELON_PACK_JSON") {
        let path = PathBuf::from(path);
        return vec![(path.display().to_string(), read(&path))];
    }
    let Some(dir) = root.map(|root| root.join("minecraft/post_effect")) else {
        return Vec::new();
    };
    let mut chains: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .map(|path| (path.display().to_string(), read(&path)))
        .collect();
    chains.sort_by(|a, b| a.0.cmp(&b.0));
    chains
}

/// Local `assets/` tree from the vanilla client.jar (`assets/<namespace>/...`).
pub struct Assets {
    root: PathBuf,
    /// The instance's folder, else the game folder beside `assets/`.
    instance_dir: Option<PathBuf>,
}

impl Assets {
    /// Known root, skip `discover()`.
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            root,
            instance_dir: None,
        }
    }

    pub fn discover() -> anyhow::Result<Self> {
        if let Some(root) = launcher_root() {
            return Ok(Self::from_root(root));
        }
        let exe_assets = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("assets")))
            .unwrap_or_else(|| PathBuf::from("assets"));
        let candidates = [
            PathBuf::from("assets"),
            exe_assets,
            crate::home::version_assets(&crate::home::default_assets_dir(), crate::home::VERSION),
            crate::home::default_assets_dir(),
            crate::home::version_assets(
                &crate::home::assets_root(&crate::home::dir()),
                crate::home::VERSION,
            ),
            crate::home::assets_root(&crate::home::dir()),
        ];
        for candidate in candidates {
            if candidate.join("minecraft").is_dir() {
                return Ok(Self::from_root(candidate));
            }
        }
        anyhow::bail!("assets/minecraft/ not found.")
    }

    /// Decodes the PNG behind a texture identifier (`namespace:path` to `textures/<path>.png`).
    pub fn image(&self, identifier: &str) -> anyhow::Result<image::RgbaImage> {
        let (namespace, path) = split_id(identifier);
        let bytes = packs::active()
            .find(&self.root, namespace, &format!("textures/{path}"))
            .map(|(bytes, _)| bytes)
            .ok_or_else(|| anyhow::anyhow!("Textur fehlt: {identifier}"))?;
        decode_png(&bytes).ok_or_else(|| anyhow::anyhow!("Textur kaputt: {identifier}"))
    }

    /// [`Self::image`], uploaded.
    pub fn texture(
        &self,
        renderer: &mut crate::renderer::Renderer,
        identifier: &str,
    ) -> anyhow::Result<crate::renderer::TextureId> {
        renderer.load_texture(&self.image(identifier)?)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The game folder: the instance's, if one was picked, else the main one.
    pub fn game_dir(&self) -> &Path {
        self.instance_dir
            .as_deref()
            .unwrap_or_else(|| game_dir(&self.root))
    }

    /// The main game folder, where accounts and the launcher's settings live.
    pub fn launcher_dir(&self) -> &Path {
        game_dir(&self.root)
    }

    /// Plays in the instance `name` (see [`crate::instances`]).
    pub fn with_instance(mut self, name: &str) -> anyhow::Result<Self> {
        let instance = crate::instances::find(self.launcher_dir(), name)
            .ok_or_else(|| anyhow::anyhow!("ungueltiger Instanzname: {name}"))?;
        let dir = instance.path;
        std::fs::create_dir_all(&dir)?;
        self.instance_dir = Some(dir);
        Ok(self)
    }
}

/// Vanilla `TextureAtlasSprite` tint.
pub fn tint(img: &mut image::RgbaImage, color: [u8; 3]) {
    for pixel in img.pixels_mut() {
        for (channel, &tint) in pixel.0.iter_mut().zip(&color) {
            *channel = ((u16::from(*channel) * u16::from(tint)) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod path_tests {
    use super::game_dir;
    use std::path::Path;

    #[test]
    fn versioned_assets_still_map_to_the_melon_home() {
        let versioned = Path::new("/home/user/.local/share/melon/assets/26.3");
        assert_eq!(
            game_dir(versioned),
            Path::new("/home/user/.local/share/melon")
        );
        let legacy = Path::new("/home/user/.local/share/melon/assets");
        assert_eq!(game_dir(legacy), Path::new("/home/user/.local/share/melon"));
        let other = Path::new("/tmp/custom-assets/1.19.1");
        assert_eq!(game_dir(other), Path::new("/tmp/custom-assets"));
    }
}

#[cfg(test)]
mod png_tests {
    use super::decode_png;

    /// A PNG of one palette-less pixel row with every chunk's CRC ruined.
    fn spoiled_png() -> (image::RgbaImage, Vec<u8>) {
        let mut image = image::RgbaImage::new(4, 2);
        for (i, pixel) in image.pixels_mut().enumerate() {
            *pixel = image::Rgba([i as u8 * 30, 10, 200, 255 - i as u8 * 20]);
        }
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let mut at = 8;
        while at < bytes.len() {
            let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let crc = at + 8 + length;
            bytes[crc] ^= 0xff;
            at = crc + 4;
        }
        (image, bytes)
    }

    #[test]
    fn a_png_with_wrong_checksums_still_reads() {
        let (image, bytes) = spoiled_png();
        assert!(
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).is_err(),
            "the image crate should refuse it"
        );
        assert_eq!(decode_png(&bytes), Some(image));
    }

    /// A real pack's PNG (`MELON_PNG`) decodes.
    #[test]
    #[ignore = "needs MELON_PNG"]
    fn a_real_png_decodes() {
        let path = std::env::var("MELON_PNG").expect("MELON_PNG");
        let bytes = std::fs::read(path).unwrap();
        let image = decode_png(&bytes).expect("decodes");
        eprintln!("{}x{}", image.width(), image.height());
    }
}
