//! The packet debugger.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result, bail};

/// How many of each unresolved packet are written out in full.
const SHOWN: u32 = 3;
/// Bytes of a body shown as hex.
const DUMP: usize = 2048;

/// `packets.json` of Pumpkin, which names the protocol's packets.
const PACKETS: &str = include_str!("packets.json");

/// The clientbound play packet `id` is called.
pub fn packet_name(id: i32) -> Option<&'static str> {
    names_for("clientbound").get(&id).map(String::as_str)
}

/// The serverbound play packet `id` is called.
pub fn serverbound_name(id: i32) -> Option<&'static str> {
    names_for("serverbound").get(&id).map(String::as_str)
}

fn names_for(bound: &'static str) -> &'static HashMap<i32, String> {
    static CLIENTBOUND: OnceLock<HashMap<i32, String>> = OnceLock::new();
    static SERVERBOUND: OnceLock<HashMap<i32, String>> = OnceLock::new();
    let slot = if bound == "serverbound" {
        &SERVERBOUND
    } else {
        &CLIENTBOUND
    };
    slot.get_or_init(|| {
        let json: serde_json::Value = serde_json::from_str(PACKETS).unwrap_or_default();
        json[bound]["play"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(name, id)| Some((i32::try_from(id.as_i64()?).ok()?, name.clone())))
            .collect()
    })
}

/// The id behind a packet name or a number.
fn packet_id(token: &str) -> Option<i32> {
    token
        .parse()
        .ok()
        .or_else(|| (0..512).find(|&id| packet_name(id).is_some_and(|name| name == token)))
}

/// A capture file starts with this.
const MAGIC: &[u8; 6] = b"MPKT1\n";

/// Where the packets go while capturing, with when it began.
struct Capture {
    out: BufWriter<File>,
    start: Instant,
}

/// What a connection reports, and what it has seen so far.
#[derive(Default)]
pub struct PacketDebug {
    capture: Option<Capture>,
    on: bool,
    unresolved: bool,
    all: bool,
    traced: HashSet<i32>,
    /// Per packet id.
    seen: BTreeMap<i32, Seen>,
}

#[derive(Default)]
struct Seen {
    count: u32,
    bytes: u64,
    error: Option<String>,
}

impl PacketDebug {
    /// From `MELON_PACKET_DEBUG`.
    pub fn from_env() -> Self {
        let mut debug = match std::env::var("MELON_PACKET_DEBUG") {
            Ok(value) => Self::parse(&value),
            Err(_) => Self::default(),
        };
        if let Some(path) = std::env::var_os("MELON_PACKET_CAPTURE") {
            match start_capture(Path::new(&path)) {
                Ok(capture) => debug.capture = Some(capture),
                Err(e) => eprintln!("MELON_PACKET_CAPTURE: {e:#}"),
            }
        }
        debug
    }

    fn parse(value: &str) -> Self {
        let mut debug = Self::default();
        for token in value.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            match token {
                "1" | "unresolved" => debug.unresolved = true,
                "all" => debug.all = true,
                other => match packet_id(other) {
                    Some(id) => {
                        debug.traced.insert(id);
                    }
                    None => eprintln!("MELON_PACKET_DEBUG: unknown packet \"{other}\""),
                },
            }
        }
        debug.on = debug.unresolved || debug.all || !debug.traced.is_empty();
        debug
    }

    /// A packet came in, before anything decodes it.
    pub fn received(&mut self, pid: i32, body: &[u8]) {
        if crate::probe::enabled() {
            crate::probe::count("packet.seen", label(pid));
            crate::probe::trace_packet(packet_name(pid).unwrap_or("(unknown)"));
        }
        if let Some(capture) = &mut self.capture
            && let Err(e) = record(capture, pid, body)
        {
            eprintln!("[packet] capture ends: {e}");
            self.capture = None;
        }
        if self.all || self.traced.contains(&pid) {
            eprintln!(
                "[packet] <- {}  {} B{}",
                label(pid),
                body.len(),
                dump(body, self.traced.contains(&pid))
            );
        }
    }

    /// No decoder resolved the packet (`error`: or the decoder failed).
    pub fn unresolved(&mut self, pid: i32, body: &[u8], error: Option<String>) {
        if crate::probe::enabled() {
            let category = if error.is_some() {
                "decode.error"
            } else {
                "packet.unresolved"
            };
            crate::probe::note(category, label(pid), || {
                format!(
                    "{} B{}{}",
                    body.len(),
                    error.as_deref().map_or(String::new(), |e| format!(", {e}")),
                    dump(body, true) + &strings(body)
                )
            });
        }
        if !self.on || !self.unresolved && error.is_none() {
            return;
        }
        let seen = self.seen.entry(pid).or_default();
        seen.count += 1;
        seen.bytes += body.len() as u64;
        if seen.count <= SHOWN {
            match &error {
                Some(e) => eprintln!(
                    "[packet] !! {}  {} B, unreadable: {e}{}",
                    label(pid),
                    body.len(),
                    dump(body, true)
                ),
                None => eprintln!(
                    "[packet] ?? {}  {} B, no decoder{}",
                    label(pid),
                    body.len(),
                    dump(body, true)
                ),
            }
        }
        if error.is_some() {
            seen.error = error;
        }
    }

    /// The counts of what went unresolved.
    pub fn summary(&self) {
        if self.seen.is_empty() {
            return;
        }
        eprintln!("[packet] summary, packets not resolved:");
        for (&pid, seen) in &self.seen {
            let error = seen
                .error
                .as_deref()
                .map_or(String::new(), |e| format!("  error: {e}"));
            eprintln!(
                "[packet]   {:<44} {:>6}x {:>9} B{error}",
                label(pid),
                seen.count,
                seen.bytes
            );
        }
    }
}

impl Drop for PacketDebug {
    fn drop(&mut self) {
        self.summary();
    }
}

fn start_capture(path: &Path) -> Result<Capture> {
    let file = File::create(path).with_context(|| path.display().to_string())?;
    let mut out = BufWriter::new(file);
    out.write_all(MAGIC)?;
    eprintln!("[packet] capturing to {}", path.display());
    Ok(Capture {
        out,
        start: Instant::now(),
    })
}

/// One packet.
fn record(capture: &mut Capture, pid: i32, body: &[u8]) -> std::io::Result<()> {
    let millis = capture.start.elapsed().as_millis() as u32;
    capture.out.write_all(&millis.to_be_bytes())?;
    capture.out.write_all(&pid.to_be_bytes())?;
    capture.out.write_all(&(body.len() as u32).to_be_bytes())?;
    capture.out.write_all(body)?;
    capture.out.flush()
}

/// The packets of a capture: (milliseconds, id, body).
fn read_capture(path: &Path) -> Result<Vec<(u32, i32, Vec<u8>)>> {
    let mut input = BufReader::new(File::open(path).with_context(|| path.display().to_string())?);
    let mut magic = [0; 6];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC {
        bail!("{} is not a packet recording", path.display());
    }
    let mut packets = Vec::new();
    let mut head = [0; 12];
    while input.read_exact(&mut head).is_ok() {
        let millis = u32::from_be_bytes(head[0..4].try_into().unwrap());
        let pid = i32::from_be_bytes(head[4..8].try_into().unwrap());
        let len = u32::from_be_bytes(head[8..12].try_into().unwrap()) as usize;
        let mut body = vec![0; len.min(1 << 26)];
        if input.read_exact(&mut body).is_err() {
            break;
        }
        packets.push((millis, pid, body));
    }
    Ok(packets)
}

/// `melon --packets <file>`.
pub fn replay(path: &Path) -> Result<()> {
    let packets = read_capture(path)?;
    let spec = std::env::var("MELON_PACKET_DEBUG").unwrap_or_else(|_| "unresolved".into());
    let mut listener = super::PlayListener::default();
    listener.set_debug(PacketDebug::parse(&spec));
    let mut kinds: BTreeMap<i32, (u32, u64)> = BTreeMap::new();
    for (_, pid, body) in &packets {
        let kind = kinds.entry(*pid).or_default();
        kind.0 += 1;
        kind.1 += body.len() as u64;
        if let Err(e) = listener.handle(*pid, body) {
            eprintln!("[packet] !! {} unreadable: {e:#}", label(*pid));
        }
    }
    let seconds = packets.last().map_or(0, |p| p.0) / 1000;
    println!("{} Pakete in {seconds} s:", packets.len());
    for (pid, (count, bytes)) in kinds {
        println!("  {:<44} {count:>7}x {bytes:>10} B", label(pid));
    }
    Ok(())
}

/// `0x2a set_cursor_item`.
fn label(pid: i32) -> String {
    format!("{pid:#04x} {}", packet_name(pid).unwrap_or("(unknown)"))
}

/// The start of `body` as hex and text, if `show`.
fn dump(body: &[u8], show: bool) -> String {
    if !show || body.is_empty() {
        return String::new();
    }
    let head = &body[..body.len().min(DUMP)];
    let hex: Vec<String> = head.iter().map(|b| format!("{b:02x}")).collect();
    let text: String = head
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    let more = if body.len() > DUMP { " ..." } else { "" };
    format!("\n        {}{more}\n        |{text}|", hex.join(" "))
}

/// The readable runs in a body (text of a dialog, titles, commands).
fn strings(body: &[u8]) -> String {
    let mut out = Vec::new();
    let mut run = String::new();
    for &b in body {
        if (0x20..0x7f).contains(&b) {
            run.push(b as char);
        } else {
            if run.len() >= 5 {
                out.push(std::mem::take(&mut run));
            }
            run.clear();
        }
        if out.len() >= 80 {
            break;
        }
    }
    if run.len() >= 5 {
        out.push(run);
    }
    if out.is_empty() {
        String::new()
    } else {
        format!("\n  strings: {out:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_are_named_and_picked_by_name_or_number() {
        assert_eq!(packet_name(18), Some("container_set_content"));
        assert_eq!(serverbound_name(13), Some("client_tick_end"));
        assert_eq!(serverbound_name(0), Some("accept_teleportation"));
        assert_eq!(packet_id("container_set_content"), Some(18));
        assert_eq!(packet_id("18"), Some(18));
        let debug = PacketDebug::parse("unresolved, set_cursor_item");
        assert!(debug.on && debug.unresolved && debug.traced.contains(&98));
        assert!(!PacketDebug::parse("").on);
    }

    #[test]
    fn a_capture_reads_back_what_was_recorded() {
        let path = std::env::temp_dir().join(format!("melon-capture-{}", std::process::id()));
        let mut capture = start_capture(&path).unwrap();
        record(&mut capture, 98, b"abc").unwrap();
        record(&mut capture, 7, b"").unwrap();
        let packets = read_capture(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(packets.len(), 2);
        assert_eq!((packets[0].1, packets[0].2.as_slice()), (98, &b"abc"[..]));
        assert_eq!(packets[1].1, 7);
    }

    #[test]
    fn a_dump_shows_hex_and_text() {
        let text = dump(b"hi\x00", true);
        assert!(text.contains("68 69 00") && text.contains("|hi.|"));
        assert_eq!(dump(b"hi", false), "");
    }
}
