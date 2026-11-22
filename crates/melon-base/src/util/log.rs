//! [`warn!`](crate::warn) and [`info!`](crate::info): stderr lines tagged
//! with their module, filtered by `MELON_LOG` and limited per call site.
//!
//! `MELON_LOG` holds comma-separated module prefixes (`game::level_meshes`,
//! `renderer`); a `-` before one silences it, and the longest match wins.
//! Everything is on by default: `MELON_LOG=-gui::text` hides the font's
//! lines, `MELON_LOG=-,network` shows only the network's.

use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

/// Lines a call site may write per [`WINDOW_SECS`]; the rest are counted.
const BURST: u32 = 5;
const WINDOW_SECS: u64 = 10;

#[derive(Clone, Copy)]
pub enum Level {
    Warn,
    Info,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warn => "WARN",
            Self::Info => "INFO",
        })
    }
}

/// One call site's rate limit.
pub struct Site {
    window: AtomicU64,
    written: AtomicU32,
    suppressed: AtomicU32,
}

impl Site {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            window: AtomicU64::new(0),
            written: AtomicU32::new(0),
            suppressed: AtomicU32::new(0),
        }
    }

    /// Writes `message` unless `MELON_LOG` silences `module` or this site
    /// wrote [`BURST`] lines in the current window already.
    pub fn write(&self, level: Level, module: &str, message: fmt::Arguments<'_>) {
        let module = module
            .strip_prefix("melon::")
            .or_else(|| module.strip_prefix("melon_base::"))
            .unwrap_or(module);
        if !enabled(filter(), module) {
            return;
        }
        let window = start().elapsed().as_secs() / WINDOW_SECS;
        if self.window.swap(window, Ordering::Relaxed) != window {
            self.written.store(0, Ordering::Relaxed);
            let suppressed = self.suppressed.swap(0, Ordering::Relaxed);
            if suppressed > 0 {
                eprintln!("[{level}] {module}: ({suppressed} similar lines left out)");
            }
        }
        if self.written.fetch_add(1, Ordering::Relaxed) < BURST {
            eprintln!("[{level}] {module}: {message}");
        } else {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// When the first line was written: the windows count from there.
fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// `MELON_LOG`, parsed once: (prefix, on).
fn filter() -> &'static [(String, bool)] {
    static FILTER: OnceLock<Vec<(String, bool)>> = OnceLock::new();
    FILTER.get_or_init(|| parse(&std::env::var("MELON_LOG").unwrap_or_default()))
}

fn parse(setting: &str) -> Vec<(String, bool)> {
    setting
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.strip_prefix('-') {
            Some(prefix) => (prefix.to_owned(), false),
            None => (entry.to_owned(), true),
        })
        .collect()
}

/// The longest prefix of `module` in `filter` decides; on without one.
fn enabled(filter: &[(String, bool)], module: &str) -> bool {
    filter
        .iter()
        .filter(|(prefix, _)| {
            module
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with("::") || prefix.is_empty())
        })
        .max_by_key(|(prefix, _)| prefix.len())
        .is_none_or(|(_, on)| *on)
}

/// A problem worth a line on stderr; see [`crate::util::log`].
#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => {{
        static SITE: $crate::util::log::Site = $crate::util::log::Site::new();
        SITE.write($crate::util::log::Level::Warn, module_path!(), format_args!($($arg)+));
    }};
}

/// What the game is doing, as a line on stderr; see [`crate::util::log`].
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => {{
        static SITE: $crate::util::log::Site = $crate::util::log::Site::new();
        SITE.write($crate::util::log::Level::Info, module_path!(), format_args!($($arg)+));
    }};
}

#[cfg(test)]
mod tests {
    use super::{enabled, parse};

    #[test]
    fn the_longest_prefix_decides() {
        let filter = parse("-gui::text, gui::text::font::load");
        assert!(enabled(&filter, "game::level_meshes"));
        assert!(!enabled(&filter, "gui::text"));
        assert!(!enabled(&filter, "gui::text::font"));
        assert!(enabled(&filter, "gui::text::font::load"));
        assert!(enabled(&filter, "gui::textures"));
    }

    #[test]
    fn a_bare_minus_silences_the_rest() {
        let filter = parse("-,network");
        assert!(!enabled(&filter, "renderer"));
        assert!(enabled(&filter, "network::play"));
        assert!(enabled(&parse(""), "renderer"));
    }
}
