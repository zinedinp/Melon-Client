//! Vanilla `ServerStatusPinger`: the server list's query.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use image::RgbaImage;
use serde_json::Value;

use melon_protocol::packet::{self, Reader, read_packet, write_packet};
use melon_protocol::varint;
use melon_assets::text::{SPRITE_COUNT, sprite_char};

const TIMEOUT: Duration = Duration::from_secs(5);

/// The hostname did not resolve (`UnknownHostException`).
#[derive(Debug)]
pub struct CannotResolve;

impl std::fmt::Display for CannotResolve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the hostname could not be resolved")
    }
}

impl std::error::Error for CannotResolve {}

/// What the server list shows for a server.
#[derive(Clone, Debug)]
pub struct ServerStatus {
    /// `description`, as the server sent it: a text component.
    pub description: Value,
    pub online: i64,
    pub max: i64,
    /// `players.sample`: some of the online players, for the tooltip.
    pub sample: Vec<String>,
    pub version_name: String,
    pub protocol: i32,
    pub latency_ms: u128,
    /// The textures (`textures/<id>.png`) of the sprites in the description.
    pub sprites: Vec<String>,
    /// `favicon`: the server's 64x64 icon.
    pub favicon: Option<RgbaImage>,
}

pub fn ping(addr: &str) -> Result<ServerStatus> {
    let (host, port) = super::login::resolver::parse_address(addr).ok_or(CannotResolve)?;
    let targets = super::login::resolver::resolve(host, port)?.addrs;
    let connect = || -> Result<TcpStream> {
        let stream = targets
            .iter()
            .find_map(|target| TcpStream::connect_timeout(target, TIMEOUT).ok())
            .with_context(|| format!("connecting to {addr}"))?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_nodelay(true).ok();
        Ok(stream)
    };
    match modern_ping(connect()?, host, port) {
        Ok(status) => Ok(status),
        Err(modern) => legacy_ping(connect()?, host, port).map_err(|_| modern),
    }
}

fn modern_ping(mut stream: TcpStream, host: &str, port: u16) -> Result<ServerStatus> {
    write_packet(&mut stream, None, 0, |buf| {
        varint::write_var_int(buf, super::PROTOCOL_VERSION)?;
        packet::write_string(buf, host)?;
        buf.write_all(&port.to_be_bytes())?;
        Ok(varint::write_var_int(buf, 1)?)
    })?;
    write_packet(&mut stream, None, 0, |_| Ok(()))?;
    let (pid, body) = read_packet(&mut stream, None)?;
    if pid != 0 {
        bail!("unexpected status packet {pid}");
    }
    let json: Value =
        serde_json::from_str(&Reader::new(&body).string()?).context("reading the status JSON")?;

    let sent = Instant::now();
    let token: i64 = 0x004d_656c_6f6e;
    write_packet(&mut stream, None, 1, |buf| {
        buf.write_all(&token.to_be_bytes())?;
        Ok(())
    })?;
    let _ = read_packet(&mut stream, None);
    let latency_ms = sent.elapsed().as_millis();

    let mut sprites = Vec::new();
    let description = sanitize_description(&json["description"], 0, &mut sprites);
    Ok(ServerStatus {
        description,
        sprites,
        online: json["players"]["online"].as_i64().unwrap_or(0),
        max: json["players"]["max"].as_i64().unwrap_or(0),
        sample: json["players"]["sample"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|player| player["name"].as_str().map(str::to_owned))
            .collect(),
        version_name: json["version"]["name"].as_str().unwrap_or("").to_owned(),
        protocol: json["version"]["protocol"].as_i64().unwrap_or(0) as i32,
        latency_ms,
        favicon: json["favicon"].as_str().and_then(decode_favicon),
    })
}

/// `ServerStatusPinger.sanitizeDescription`, for the components Melon can show.
fn sanitize_description(component: &Value, depth: u32, sprites: &mut Vec<String>) -> Value {
    const CONTENT: [&str; 6] = ["text", "translate", "keybind", "selector", "score", "nbt"];
    if depth >= 16 {
        return Value::Null;
    }
    match component {
        Value::Array(parts) => Value::Array(
            parts
                .iter()
                .map(|part| match part {
                    Value::String(text) => serde_json::json!({ "text": text }),
                    part => sanitize_description(part, depth + 1, sprites),
                })
                .collect(),
        ),
        Value::Object(map) => {
            let mut map = map.clone();
            if !CONTENT.iter().any(|key| map.contains_key(*key)) {
                let placeholder = map
                    .get("sprite")
                    .and_then(Value::as_str)
                    .filter(|_| sprites.len() < SPRITE_COUNT)
                    .and_then(|id| {
                        let id = id.strip_prefix("minecraft:").unwrap_or(id);
                        sprites.push(id.to_owned());
                        sprite_char(sprites.len() - 1)
                    });
                map.remove("sprite");
                map.remove("atlas");
                map.insert(
                    "text".to_owned(),
                    Value::String(placeholder.map(String::from).unwrap_or_default()),
                );
            }
            for key in ["extra", "with"] {
                if let Some(children) = map.get(key) {
                    map.insert(
                        key.to_owned(),
                        sanitize_description(children, depth + 1, sprites),
                    );
                }
            }
            Value::Object(map)
        }
        other => other.clone(),
    }
}

/// `LegacyServerPinger`.
fn legacy_ping(mut stream: TcpStream, host: &str, port: u16) -> Result<ServerStatus> {
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let units = |text: &str| (text.encode_utf16().count() as u16).to_be_bytes();
    let mut payload = vec![127];
    payload.extend(units(host));
    payload.extend(utf16(host));
    payload.extend(i32::from(port).to_be_bytes());
    let mut request = vec![0xFE, 0x01, 0xFA];
    request.extend(units("MC|PingHost"));
    request.extend(utf16("MC|PingHost"));
    request.extend((payload.len() as u16).to_be_bytes());
    request.extend(payload);

    let sent = Instant::now();
    stream.write_all(&request)?;
    let mut head = [0u8; 3];
    stream.read_exact(&mut head)?;
    if head[0] != 0xFF {
        bail!("not a legacy status answer");
    }
    let mut body = vec![0u8; usize::from(u16::from_be_bytes([head[1], head[2]])) * 2];
    stream.read_exact(&mut body)?;
    let latency_ms = sent.elapsed().as_millis();
    parse_legacy(&body, latency_ms)
}

fn parse_legacy(body: &[u8], latency_ms: u128) -> Result<ServerStatus> {
    let units: Vec<u16> = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    let text = String::from_utf16_lossy(&units);
    let fields: Vec<&str> = text.split('\0').collect();
    let [marker, protocol, version, motd, online, max] = fields[..] else {
        bail!("unexpected legacy status {text:?}");
    };
    if marker != "§1" {
        bail!("unexpected legacy status {text:?}");
    }
    Ok(ServerStatus {
        description: Value::String(motd.to_owned()),
        online: online.parse().unwrap_or(-1),
        max: max.parse().unwrap_or(-1),
        sample: Vec::new(),
        version_name: version.to_owned(),
        protocol: protocol.parse().unwrap_or(0),
        latency_ms,
        favicon: None,
        sprites: Vec::new(),
    })
}

/// `ServerData.validateIcon`.
fn decode_favicon(url: &str) -> Option<RgbaImage> {
    let data = url.strip_prefix("data:image/png;base64,")?;
    let png = melon_core::util::base64::decode(&data.replace('\n', ""))?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    (image.dimensions() == (64, 64)).then_some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MELON_TEST_SERVER=host:port cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a running server in MELON_TEST_SERVER"]
    fn pings_a_real_server() {
        let addr = std::env::var("MELON_TEST_SERVER").expect("MELON_TEST_SERVER");
        let status = ping(&addr).expect("the server should answer");
        eprintln!("{status:?}");
        eprintln!(
            "FAVICON: {:?}",
            status.favicon.as_ref().map(RgbaImage::dimensions)
        );
        assert_eq!(status.protocol, crate::PROTOCOL_VERSION);
        assert!(status.max > 0);
        assert!(!status.description.is_null());
        let motd = melon_protocol::component::decode::component(&status.description);
        let text = melon_assets::text::StyledText::from_component(&motd, &melon_assets::i18n::Lang::empty());
        eprintln!("MOTD: {:?}", text.to_plain());
    }

    #[test]
    fn sprites_in_a_motd_do_not_hide_it() {
        let motd: Value = serde_json::from_str(
            r##"{"extra":[{"color":"white","atlas":"minecraft:items","sprite":"minecraft:item/totem_of_undying"},
                " ",{"bold":true,"extra":[{"color":"#FFD900","text":"G"}],"text":""},"\n  ",
                {"color":"gray","text":"+"}],"text":"   "}"##,
        )
        .unwrap();
        let mut sprites = Vec::new();
        let sanitized = sanitize_description(&motd, 0, &mut sprites);
        let component = melon_protocol::component::decode::component(&sanitized);
        let text = melon_assets::text::StyledText::from_component(&component, &melon_assets::i18n::Lang::empty());
        assert_eq!(text.to_plain(), "   \u{E000} G\n  +");
        assert_eq!(sprites, ["item/totem_of_undying"]);
    }

    #[test]
    fn nesting_stops_at_sixteen_levels() {
        let mut deep = Value::String("x".into());
        for _ in 0..20 {
            deep = serde_json::json!({"text": "", "extra": [deep]});
        }
        assert!(
            sanitize_description(&deep, 0, &mut Vec::new())
                .to_string()
                .len()
                < deep.to_string().len()
        );
    }

    #[test]
    fn reads_a_legacy_answer() {
        let text = ["§1", "127", "1.6.4", "An old server", "3", "20"].join("\0");
        let body: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let status = parse_legacy(&body, 12).unwrap();
        assert_eq!(
            (
                status.protocol,
                status.version_name.as_str(),
                status.online,
                status.max
            ),
            (127, "1.6.4", 3, 20)
        );
        assert_eq!(status.description, Value::String("An old server".into()));
        assert!(parse_legacy(&body[..4], 0).is_err());
    }

    #[test]
    fn favicons_must_be_64_square_pngs() {
        let encode = |size: u32| {
            let mut png = Vec::new();
            RgbaImage::new(size, size)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut text = String::from("data:image/png;base64,");
            for chunk in png.chunks(3) {
                let n = chunk
                    .iter()
                    .enumerate()
                    .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
                for i in 0..=chunk.len() {
                    text.push(table[(n >> (18 - 6 * i) & 63) as usize] as char);
                }
            }
            text
        };
        assert!(decode_favicon(&encode(64)).is_some());
        let mut wrapped = encode(64);
        wrapped.insert(40, '\n');
        assert!(decode_favicon(&wrapped).is_some());
        assert!(decode_favicon(&encode(32)).is_none());
        assert!(decode_favicon("data:image/jpeg;base64,AAAA").is_none());
    }
}
