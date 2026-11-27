//! The pipeline cache each device's pipelines are built with, kept here so
//! the builders take no parameter for it. Without one they build uncached.

use std::sync::{Mutex, MutexGuard, PoisonError};

use ash::vk;

static CACHES: Mutex<Vec<(vk::Device, vk::PipelineCache)>> = Mutex::new(Vec::new());

fn caches() -> MutexGuard<'static, Vec<(vk::Device, vk::PipelineCache)>> {
    CACHES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `device`'s cache, null without one.
pub fn of(device: &ash::Device) -> vk::PipelineCache {
    let handle = device.handle();
    caches()
        .iter()
        .find(|&&(d, _)| d == handle)
        .map_or(vk::PipelineCache::null(), |&(_, c)| c)
}

/// Keeps a cache the one [`of`] gives for its device, until dropped.
pub struct Installed {
    device: vk::Device,
}

impl Installed {
    /// Installs `cache` for `device`, replacing the one it had.
    ///
    /// # Safety
    ///
    /// `cache` was made on `device` and outlives the guard.
    pub unsafe fn new(device: &ash::Device, cache: vk::PipelineCache) -> Self {
        let handle = device.handle();
        let mut caches = caches();
        caches.retain(|&(d, _)| d != handle);
        caches.push((handle, cache));
        Self { device: handle }
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        caches().retain(|&(d, _)| d != self.device);
    }
}
