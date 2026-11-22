//! The tester.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static ENABLED: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// When recording began.
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);

/// How many different keys one category keeps, so a flood of them can't grow the report.
const MAX_KEYS: usize = 400;
/// How often the report is written while the game runs.
const WRITE_EVERY: Duration = Duration::from_secs(20);

struct Entry {
    count: u64,
    detail: String,
}

struct State {
    path: PathBuf,
    started: Instant,
    written: Instant,
    /// Category, then key.
    notes: BTreeMap<&'static str, BTreeMap<String, Entry>>,
    dropped: BTreeMap<&'static str, u64>,
    /// Extra text a report carries (what the packs hold).
    extra: Vec<(String, String)>,
}

/// What each category in the report means, in the order it is printed.
const CATEGORIES: &[(&str, &str)] = &[
    ("server", "What the server announced about itself"),
    (
        "teleport",
        "Position corrections from the server, grouped by destination and relative flags",
    ),
    (
        "tab.text",
        "Tab list header, footer and display names: text and fonts",
    ),
    (
        "prediction.reverted",
        "Blocks the server answered differently than predicted",
    ),
    (
        "block.near",
        "Server block changes within 4 blocks of the player (state actually changed)",
    ),
    (
        "hold.trace",
        "Our position, motion and ground flag each tick of a hold",
    ),
    (
        "hold",
        "The packets just before the server began pinning the player",
    ),
    (
        "packet.unresolved",
        "Packets no decoder resolved, or that failed to decode",
    ),
    (
        "decode.error",
        "Packets that failed in a decoder that exists",
    ),
    ("entity.spawned", "Entity types the server spawned"),
    (
        "nametag",
        "Players and named entities: whether the name tag was drawn, and why not",
    ),
    (
        "nametag.draw",
        "Name tags handed to the GUI: screen position, scale, glyph vertices",
    ),
    (
        "entity.not_drawn",
        "Entity types Melon spawns but doesn't draw",
    ),
    (
        "entity.data",
        "Synced data of entities Melon doesn't draw (displays, markers...)",
    ),
    (
        "text_display",
        "Text displays as drawn: position, transformation",
    ),
    (
        "item.definition_missing",
        "Item definitions that no pack or vanilla has",
    ),
    (
        "item.model_missing",
        "Models an item definition names that can't be read",
    ),
    (
        "item.unsupported",
        "Item model types and properties Melon doesn't know",
    ),
    ("block.no_model", "Blocks with no model for some state"),
    ("block.model_missing", "Block models that can't be read"),
    ("block.texture_missing", "Block textures that don't exist"),
    (
        "resource.missing",
        "Files looked up that no pack or vanilla has",
    ),
    ("font.missing", "Fonts a text names that aren't loaded"),
    ("font.glyph_missing", "Characters no font has a glyph for"),
    ("lang.missing", "Translation keys with no text"),
    (
        "text.unsupported",
        "Text component parts Melon shows as nothing",
    ),
    ("sound.unknown", "Sound events with no entry in sounds.json"),
];

/// Starts recording into `path`.
pub fn enable(path: PathBuf) {
    let now = Instant::now();
    *STARTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(State {
        path,
        started: now,
        written: now,
        notes: BTreeMap::new(),
        dropped: BTreeMap::new(),
        extra: Vec::new(),
    });
    ENABLED.store(true, Ordering::Relaxed);
}

/// Whether the tester is on.
#[inline]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Writes down that `key` happened in `category`; `detail` is only made the first time.
pub fn note(category: &'static str, key: impl AsRef<str>, detail: impl FnOnce() -> String) {
    if !enabled() {
        return;
    }
    let key = key.as_ref();
    {
        let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = guard.as_mut() else {
            return;
        };
        let map = state.notes.entry(category).or_default();
        if let Some(entry) = map.get_mut(key) {
            entry.count += 1;
            return;
        }
        if map.len() >= MAX_KEYS {
            *state.dropped.entry(category).or_default() += 1;
            return;
        }
    }
    let text = detail();
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = guard.as_mut() else {
        return;
    };
    let map = state.notes.entry(category).or_default();
    if let Some(entry) = map.get_mut(key) {
        entry.count += 1;
        return;
    }
    if map.len() >= MAX_KEYS {
        *state.dropped.entry(category).or_default() += 1;
        return;
    }
    map.insert(
        key.to_owned(),
        Entry {
            count: 1,
            detail: text,
        },
    );
}

static RECENT: Mutex<std::collections::VecDeque<(Instant, &'static str)>> =
    Mutex::new(std::collections::VecDeque::new());

/// A packet came in: kept for [`recent_packets`] (the last 80).
pub fn trace_packet(label: &'static str) {
    if !enabled() {
        return;
    }
    let mut ring = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    if ring.len() >= 80 {
        ring.pop_front();
    }
    ring.push_back((Instant::now(), label));
}

static MOVES: Mutex<std::collections::VecDeque<String>> =
    Mutex::new(std::collections::VecDeque::new());

/// One tick of our own movement, kept for [`recent_moves`] (the last 40).
pub fn trace_move(line: String) {
    if !enabled() {
        return;
    }
    let mut ring = MOVES.lock().unwrap_or_else(|e| e.into_inner());
    if ring.len() >= 40 {
        ring.pop_front();
    }
    ring.push_back(line);
}

pub fn recent_moves() -> String {
    let ring = MOVES.lock().unwrap_or_else(|e| e.into_inner());
    ring.iter().map(|l| format!("\n  {l}")).collect()
}

static SENT: Mutex<std::collections::VecDeque<(Instant, i32)>> =
    Mutex::new(std::collections::VecDeque::new());

/// A packet went out: kept for [`recent_sent`] (the last 120).
pub fn trace_sent(id: i32) {
    if !enabled() {
        return;
    }
    let mut ring = SENT.lock().unwrap_or_else(|e| e.into_inner());
    if ring.len() >= 120 {
        ring.pop_front();
    }
    ring.push_back((Instant::now(), id));
}

/// The play packets sent in the last `secs` seconds, named, runs folded.
pub fn recent_sent(secs: u64) -> String {
    let ring = SENT.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<(i32, u32)> = Vec::new();
    for (at, id) in ring.iter() {
        if at.elapsed().as_secs() > secs {
            continue;
        }
        match out.last_mut() {
            Some((l, n)) if l == id => *n += 1,
            _ => out.push((*id, 1)),
        }
    }
    out.iter()
        .map(|(id, n)| {
            let name = crate::network::play::debug::serverbound_name(*id).unwrap_or("unknown");
            if *n > 1 {
                format!("{name} x{n}")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One movement-related packet, received or sent, with how it was handled.
struct IoLine {
    at: Instant,
    tx: bool,
    name: String,
    detail: String,
}

static IO: Mutex<std::collections::VecDeque<IoLine>> =
    Mutex::new(std::collections::VecDeque::new());
const IO_KEEP: usize = 240;

/// A movement packet.
pub fn trace_io(tx: bool, name: impl Into<String>, detail: impl Into<String>) {
    if !enabled() {
        return;
    }
    let mut ring = IO.lock().unwrap_or_else(|e| e.into_inner());
    if ring.len() >= IO_KEEP {
        ring.pop_front();
    }
    ring.push_back(IoLine {
        at: Instant::now(),
        tx,
        name: name.into(),
        detail: detail.into(),
    });
}

/// `Relative` bits as names, for a teleport's flags.
pub fn relative_flags(rel: i32) -> String {
    const NAMES: &[(i32, &str)] = &[
        (1 << 0, "X"),
        (1 << 1, "Y"),
        (1 << 2, "Z"),
        (1 << 3, "YAW"),
        (1 << 4, "PITCH"),
        (1 << 5, "DELTA_X"),
        (1 << 6, "DELTA_Y"),
        (1 << 7, "DELTA_Z"),
        (1 << 8, "ROTATE_DELTA"),
    ];
    let set: Vec<&str> = NAMES
        .iter()
        .filter(|(bit, _)| rel & bit != 0)
        .map(|(_, n)| *n)
        .collect();
    if set.is_empty() {
        "absolute".into()
    } else {
        format!("{rel:#x} relative {}", set.join("|"))
    }
}

fn format_io(started: Instant, line: &IoLine) -> String {
    let ms = line.at.saturating_duration_since(started).as_millis();
    let dir = if line.tx { "tx" } else { "rx" };
    format!("+{ms}ms {dir} {} {}", line.name, line.detail)
}

fn format_io_lines(started: Instant) -> String {
    let ring = IO.lock().unwrap_or_else(|e| e.into_inner());
    ring.iter()
        .map(|line| format!("\n  {}", format_io(started, line)))
        .collect()
}

fn recording_started() -> Instant {
    STARTED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_else(Instant::now)
}

/// The last movement packets, oldest first, for a hold or the report.
pub fn recent_io() -> String {
    format_io_lines(recording_started())
}

/// The packets of the last `secs` seconds, oldest first, runs of one kind folded.
pub fn recent_packets(secs: u64) -> String {
    let ring = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<(&str, u32)> = Vec::new();
    for (at, label) in ring.iter() {
        if at.elapsed().as_secs() > secs {
            continue;
        }
        match out.last_mut() {
            Some((l, n)) if l == label => *n += 1,
            _ => out.push((label, 1)),
        }
    }
    out.iter()
        .map(|(l, n)| {
            if *n > 1 {
                format!("{l} x{n}")
            } else {
                (*l).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// [`note`] without a detail.
pub fn count(category: &'static str, key: impl AsRef<str>) {
    note(category, key, String::new);
}

/// Adds a block of text to the report under `title`, replacing one of the same title.
pub fn section(title: &str, text: String) {
    if !enabled() {
        return;
    }
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = guard.as_mut() {
        match state.extra.iter_mut().find(|(t, _)| t == title) {
            Some(slot) => slot.1 = text,
            None => state.extra.push((title.to_owned(), text)),
        }
    }
}

/// Writes the report if a while has passed since the last.
pub fn flush_if_due() {
    if !enabled() {
        return;
    }
    let due = {
        let guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .as_ref()
            .is_some_and(|s| s.written.elapsed() >= WRITE_EVERY)
    };
    if due {
        write_report();
    }
}

/// Writes the report now.
pub fn write_report() {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = guard.as_mut() else {
        return;
    };
    state.written = Instant::now();
    let text = render(state);
    if let Some(parent) = state.path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&state.path, text) {
        crate::warn!("Tester: {} not written: {e}", state.path.display());
    }
}

/// Where the next screenshot goes, if one is due: every few seconds, three kept in turn.
pub fn screenshot_due() -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    static SHOTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = guard.as_ref()?;
    let due = state.started.elapsed().as_secs() / 6;
    let taken = SHOTS.load(Ordering::Relaxed);
    if due < taken.max(1) {
        return None;
    }
    SHOTS.store(due + 1, Ordering::Relaxed);
    Some(state.path.with_extension(format!("{}.png", due % 3)))
}

/// The path the report is written to.
pub fn report_path() -> Option<PathBuf> {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|s| s.path.clone())
}

fn render(state: &State) -> String {
    let mut out = String::new();
    out.push_str("# Melon tester report\n\n");
    out.push_str(&format!(
        "Melon {}, recorded for {} s.\n\n",
        env!("CARGO_PKG_VERSION"),
        state.started.elapsed().as_secs()
    ));
    for (title, text) in &state.extra {
        out.push_str(&format!("## {title}\n\n{text}\n\n"));
    }
    let io = format_io_lines(state.started);
    if !io.is_empty() {
        out.push_str("## move.log: Movement packets in order, with how Melon handled them\n\n");
        out.push_str("Vanilla handles `ping` and `player_position` on the game thread, in arrival order, and replies (`pong`, `accept_teleportation`) from that same handler. A later `pong` must not overtake an `accept_teleportation` that is still queued.\n");
        out.push_str(&io);
        out.push_str("\n\n");
    }
    for &(category, meaning) in CATEGORIES {
        let Some(map) = state.notes.get(category) else {
            continue;
        };
        out.push_str(&format!("## {category}: {meaning}\n\n"));
        let mut rows: Vec<_> = map.iter().collect();
        rows.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        for (key, entry) in rows {
            if entry.detail.is_empty() {
                out.push_str(&format!("- `{key}` x{}\n", entry.count));
            } else {
                out.push_str(&format!(
                    "- `{key}` x{}: {}\n",
                    entry.count,
                    entry.detail.replace('\n', " ")
                ));
            }
        }
        if let Some(dropped) = state.dropped.get(category) {
            out.push_str(&format!("- ... and {dropped} more kept out\n"));
        }
        out.push('\n');
    }
    out
}

/// What the active resource packs hold, and which of it Melon has no use for.
pub fn describe_packs(assets_root: &std::path::Path) {
    if !enabled() {
        return;
    }
    use crate::assets::packs;
    const USED: &[&str] = &[
        "models",
        "textures",
        "items",
        "blockstates",
        "font",
        "lang",
        "sounds",
        "post_effect",
        "shaders",
        "atlases",
        "particles",
        "texts",
    ];
    let stack = packs::active();
    let mut text = String::new();
    if stack.packs.is_empty() {
        return;
    }
    for pack in &stack.packs {
        text.push_str(&format!("### {} ({})\n\n", pack.title, pack.id));
        let inventory = pack.inventory();
        let mut by_namespace: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
        for ((namespace, folder), n) in &inventory {
            by_namespace
                .entry(namespace)
                .or_default()
                .push((folder, *n));
        }
        for (namespace, folders) in by_namespace {
            let list: Vec<String> = folders.iter().map(|(f, n)| format!("{f} {n}")).collect();
            text.push_str(&format!("- `{namespace}`: {}\n", list.join(", ")));
            for (folder, n) in &folders {
                if !USED.contains(folder) {
                    note("pack.feature", format!("{namespace}/{folder}"), || {
                        format!("{n} files in {}, not read by Melon", pack.id)
                    });
                }
            }
        }
        let mut files = BTreeSet::new();
        pack.list("minecraft", "shaders/core", &mut files);
        for file in &files {
            let name = file.trim_start_matches("shaders/core/");
            let stem = name.split('.').next().unwrap_or(name);
            if !["text", "gui", "position_tex_color"].contains(&stem) {
                note("pack.feature", format!("core shader {name}"), || {
                    format!(
                        "replaces a vanilla world shader in {}; Melon draws the world with its own",
                        pack.id
                    )
                });
            }
        }
        files.clear();
        pack.list("minecraft", "post_effect", &mut files);
        for file in &files {
            let id = file
                .trim_start_matches("post_effect/")
                .trim_end_matches(".json");
            if !crate::renderer::post::load::IDS
                .iter()
                .any(|known| known.trim_start_matches("minecraft:") == id)
            {
                note("pack.feature", format!("post effect {id}"), || {
                    format!("a chain Melon doesn't run, in {}", pack.id)
                });
            }
        }
        text.push('\n');
    }
    let _ = assets_root;
    section("Packs", text);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off, nothing is kept; on, the first detail stays and the rest count.
    #[test]
    fn notes_count_and_keep_the_first_detail() {
        enable(std::env::temp_dir().join("melon-probe-test.md"));
        note("packet.unresolved", "set_cursor_item", || "first".into());
        note("packet.unresolved", "set_cursor_item", || "second".into());
        count("lang.missing", "a.b");
        section("Packs", "two".into());
        let guard = STATE.lock().unwrap();
        let state = guard.as_ref().unwrap();
        let entry = &state.notes["packet.unresolved"]["set_cursor_item"];
        assert_eq!((entry.count, entry.detail.as_str()), (2, "first"));
        let text = render(state);
        assert!(text.contains("`set_cursor_item` x2: first"));
        assert!(text.contains("`a.b` x1"));
        assert!(text.contains("## Packs"));
        drop(guard);
        ENABLED.store(false, Ordering::Relaxed);
        note("lang.missing", "c.d", String::new);
        let guard = STATE.lock().unwrap();
        assert!(!guard.as_ref().unwrap().notes["lang.missing"].contains_key("c.d"));
    }

    #[test]
    fn a_note_detail_may_read_the_move_log() {
        enable(std::env::temp_dir().join("melon-probe-io-test.md"));
        trace_io(false, "ping", "id=1");
        note("hold", "nested", || format!("log:{}", recent_io()));
        let guard = STATE.lock().unwrap();
        let detail = &guard.as_ref().unwrap().notes["hold"]["nested"].detail;
        assert!(detail.contains("rx ping"));
        drop(guard);
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn relative_flags_name_the_set_bits() {
        assert_eq!(relative_flags(0), "absolute");
        assert_eq!(
            relative_flags(0xf8),
            "0xf8 relative YAW|PITCH|DELTA_X|DELTA_Y|DELTA_Z"
        );
    }

    #[test]
    fn a_flood_of_keys_is_capped() {
        let mut state = State {
            path: PathBuf::new(),
            started: Instant::now(),
            written: Instant::now(),
            notes: BTreeMap::new(),
            dropped: BTreeMap::new(),
            extra: Vec::new(),
        };
        let map = state.notes.entry("lang.missing").or_default();
        for i in 0..MAX_KEYS {
            map.insert(
                i.to_string(),
                Entry {
                    count: 1,
                    detail: String::new(),
                },
            );
        }
        state.dropped.insert("lang.missing", 5);
        assert!(render(&state).contains("and 5 more"));
    }
}
