//! The world's `icon.png` (vanilla's `GameRenderer.tryTakeScreenshotIfNeeded`):

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::game::InWorld;

/// `GameRenderer.lastScreenshotAttempt`: at most one attempt a second.
const RETRY: Duration = Duration::from_secs(1);
/// Chunks that must be loaded, and have stopped arriving, before the view is taken.
const MIN_CHUNKS: usize = 25;
/// `FaviconTexture`'s size.
const SIZE: u32 = 64;

pub(super) struct WorldIcon {
    next_attempt: Instant,
    /// The chunk count at the last attempt.
    chunks: usize,
}

impl WorldIcon {
    pub(super) fn new() -> Self {
        Self {
            next_attempt: Instant::now(),
            chunks: 0,
        }
    }

    /// Where the icon goes, if this frame is the one to copy out.
    pub(super) fn due(&mut self, world: &InWorld, now: Instant) -> Option<PathBuf> {
        if now < self.next_attempt {
            return None;
        }
        self.next_attempt = now + RETRY;
        let path = world.missing_icon()?;
        let chunks = world.chunk_count();
        let settled = chunks >= MIN_CHUNKS && chunks == self.chunks;
        self.chunks = chunks;
        settled.then_some(path)
    }
}

/// `takeAutoScreenshot`: the middle square of `frame`, scaled to 64x64.
pub(super) fn save(path: &Path, frame: &image::RgbaImage) {
    let (width, height) = frame.dimensions();
    let side = width.min(height);
    let view =
        image::imageops::crop_imm(frame, (width - side) / 2, (height - side) / 2, side, side);
    let icon = image::imageops::resize(&*view, SIZE, SIZE, image::imageops::FilterType::Triangle);
    if let Err(e) = icon.save_with_format(path, image::ImageFormat::Png) {
        crate::warn!("World icon not saved ({}): {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_the_middle_square_at_64_pixels() {
        let mut frame = image::RgbaImage::new(300, 100);
        for (x, _, pixel) in frame.enumerate_pixels_mut() {
            *pixel = match x {
                0..100 => image::Rgba([255, 0, 0, 255]),
                100..200 => image::Rgba([0, 255, 0, 255]),
                _ => image::Rgba([0, 0, 255, 255]),
            };
        }
        let dir = std::env::temp_dir().join(format!("melon-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("icon.png");
        save(&path, &frame);
        let icon = image::open(&path).unwrap().into_rgba8();
        assert_eq!(icon.dimensions(), (64, 64));
        assert_eq!(*icon.get_pixel(32, 32), image::Rgba([0, 255, 0, 255]));
        assert_eq!(*icon.get_pixel(1, 1), image::Rgba([0, 255, 0, 255]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
