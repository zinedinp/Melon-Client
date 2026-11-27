//! `Screenshot.grab` (F2, `key.screenshot`): the next frame as drawn, GUI
//! and all, saved as `screenshots/<yyyy-MM-dd_HH.mm.ss>.png` in the game
//! folder (the instance's), written off the render thread as vanilla does
//! on its IO pool. Then `Saved screenshot as <name>` in the chat, the name
//! a link that opens the file.

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};

/// `Screenshot.SCREENSHOT_DIR`.
pub const DIR: &str = "screenshots";

/// What a save came to: the file, or why it could not be written.
pub type Saved = Result<PathBuf, String>;

pub struct Screenshots {
    requested: bool,
    capturing: bool,
    done: Sender<Saved>,
    finished: Receiver<Saved>,
}

impl Default for Screenshots {
    fn default() -> Self {
        let (done, finished) = crossbeam_channel::unbounded();
        Self {
            requested: false,
            capturing: false,
            done,
            finished,
        }
    }
}

impl Screenshots {
    pub fn request(&mut self) {
        self.requested = true;
    }

    /// Whether to capture the frame about to be drawn; `free` is false when
    /// the capture is taken by something else this frame (the world icon),
    /// and the screenshot waits for the next.
    pub fn begin_frame(&mut self, free: bool) -> bool {
        self.capturing = self.requested && free;
        if self.capturing {
            self.requested = false;
        }
        self.capturing
    }

    /// The frame was drawn: saves its picture, if it was the captured one.
    pub fn end_frame(&mut self, game_dir: &Path, image: impl FnOnce() -> Option<image::RgbaImage>) {
        if !std::mem::take(&mut self.capturing) {
            return;
        }
        let Some(image) = image() else {
            let _ = self.done.send(Err("no frame".to_owned()));
            return;
        };
        let dir = game_dir.join(DIR);
        let done = self.done.clone();
        let spawned = std::thread::Builder::new()
            .name("screenshot".to_owned())
            .spawn(move || {
                let _ = done.send(save(&dir, &image));
            });
        if let Err(e) = spawned {
            let _ = self.done.send(Err(e.to_string()));
        }
    }

    /// The saves finished since the last call.
    pub fn finished(&self) -> Vec<Saved> {
        self.finished.try_iter().collect()
    }
}

/// `screenshot.success` with the file's name underlined, a click on it
/// opening the file (`ClickEvent.OpenFile`).
pub fn saved_message(lang: &melon_assets::i18n::Lang, file: &Path) -> melon_gui::text::StyledText {
    use melon_protocol::component::ClickEvent;
    use melon_gui::text::{StyledText, TextStyle};
    let name = file
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let pattern = lang
        .raw_dynamic("screenshot.success")
        .unwrap_or("Saved screenshot as %s");
    let (before, after) = pattern.split_once("%s").unwrap_or((pattern, ""));
    let link = TextStyle {
        underlined: true,
        click: Some(std::sync::Arc::new(ClickEvent::OpenFile {
            path: file.to_string_lossy().into_owned(),
        })),
        ..TextStyle::default()
    };
    let mut text = StyledText::default();
    text.push(&before.replace("%%", "%"), &TextStyle::default());
    text.push(&name, &link);
    text.push(&after.replace("%%", "%"), &TextStyle::default());
    text
}

/// `Screenshot.grab`'s write: an opaque PNG in `dir`, made if missing.
fn save(dir: &Path, image: &image::RgbaImage) -> Saved {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = free_name(dir, &file_stem(chrono::Local::now()));
    let mut opaque = image.clone();
    for pixel in opaque.pixels_mut() {
        pixel.0[3] = 255;
    }
    opaque.save(&file).map_err(|e| e.to_string())?;
    Ok(file)
}

/// `Util.getFilenameFormattedDateTime`: `yyyy-MM-dd_HH.mm.ss`.
fn file_stem(now: chrono::DateTime<chrono::Local>) -> String {
    now.format("%Y-%m-%d_%H.%M.%S").to_string()
}

/// `Screenshot.getFile`: `<stem>.png`, else `<stem>_2.png`, `_3` and on.
fn free_name(dir: &Path, stem: &str) -> PathBuf {
    (1..)
        .map(|count| {
            let suffix = if count == 1 {
                String::new()
            } else {
                format!("_{count}")
            };
            dir.join(format!("{stem}{suffix}.png"))
        })
        .find(|file| !file.exists())
        .expect("some name is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn names_follow_the_date_and_count_up_when_taken() {
        let at = chrono::Local
            .with_ymd_and_hms(2026, 11, 14, 9, 5, 7)
            .single()
            .unwrap();
        assert_eq!(file_stem(at), "2026-11-14_09.05.07");
        let dir = std::env::temp_dir().join(format!("melon-shots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = free_name(&dir, "x");
        assert_eq!(first.file_name().unwrap(), "x.png");
        std::fs::write(&first, b"").unwrap();
        assert_eq!(free_name(&dir, "x").file_name().unwrap(), "x_2.png");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_saved_shot_is_opaque_and_in_the_folder() {
        let dir = std::env::temp_dir().join(format!("melon-shot-{}", std::process::id()));
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 0]));
        let file = save(&dir.join(DIR), &image).unwrap();
        assert!(file.starts_with(dir.join(DIR)));
        let back = image::open(&file).unwrap().to_rgba8();
        assert_eq!(back.get_pixel(1, 1).0, [10, 20, 30, 255]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_shot_waits_for_a_free_capture() {
        let mut shots = Screenshots::default();
        assert!(!shots.begin_frame(true));
        shots.request();
        assert!(!shots.begin_frame(false));
        assert!(shots.begin_frame(true));
        shots.end_frame(Path::new("/nonexistent"), || None);
        assert!(matches!(shots.finished().as_slice(), [Err(_)]));
    }
}
