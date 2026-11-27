//! The pipeline cache, kept on disk between runs so a start doesn't compile
//! every pipeline again. A file from another GPU or driver seeds nothing.

use std::path::{Path, PathBuf};

use majestix_vk::Simple as _;
use melon_base::gfx::pipeline::cache::Installed;

/// Under the home's `cache` directory.
const FILE: &str = "pipelines.bin";

pub(super) struct PipelineCache {
    cache: majestix_vk::PipelineCache,
    path: PathBuf,
    /// Hands `cache` to the pipeline builders while it lives.
    installed: Installed,
}

impl PipelineCache {
    /// Last run's cache, or an empty one; `None` when the driver makes none.
    pub(super) fn open(layer: &majestix_vk::Device) -> Option<Self> {
        let path = melon_base::home::dir().join("cache").join(FILE);
        let data = std::fs::read(&path).unwrap_or_default();
        let cache = match layer.create_pipeline_cache(&data) {
            Ok(cache) => cache,
            Err(e) => {
                melon_base::warn!("renderer: no pipeline cache: {e}");
                return None;
            }
        };
        // SAFETY: made on `layer`'s device, retired only after the guard is
        // dropped (in `close`, or with the device).
        let installed = unsafe { Installed::new(layer.raw(), cache.raw()) };
        Some(Self {
            cache,
            path,
            installed,
        })
    }

    /// For pipelines majestix-vk builds.
    pub(super) fn handle(&self) -> majestix_vk::PipelineCache {
        self.cache
    }

    /// Writes the cache to disk and retires it.
    pub(super) fn close(self, layer: &majestix_vk::Device) {
        let Self {
            cache,
            path,
            installed,
        } = self;
        drop(installed);
        match layer.pipeline_cache_data(cache) {
            Ok(data) => {
                if let Err(e) = write(&path, &data) {
                    melon_base::warn!("renderer: pipeline cache not saved: {e}");
                }
            }
            Err(e) => melon_base::warn!("renderer: pipeline cache not read: {e}"),
        }
        layer.retire(cache);
    }
}

/// Writes `data` beside `path` first, so a crash midway leaves the old file.
fn write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("bin.part");
    std::fs::write(&partial, data)?;
    std::fs::rename(&partial, path)
}
