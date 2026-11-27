//! Vanilla `LanServerDetection`: worlds opened to the LAN, heard from the
//! `LanServerPinger` multicast every computer on the network may send.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// `LanServerPinger.MULTICAST_GROUP` and `PING_PORT`.
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
const PORT: u16 = 4445;
/// How often the thread looks whether it should stop (vanilla waits 5 s).
const POLL: Duration = Duration::from_millis(500);
/// Most worlds listed; anyone on the network can send pings, so the list is bounded
/// (vanilla's is not).
const MAX_SERVERS: usize = 64;
/// A world not heard for this long leaves the list. Vanilla keeps every world for as long as the
/// screen is open; it pings every 1.5 s, so this allows about three lost pings.
const EXPIRY: Duration = Duration::from_secs(5);

/// A world on the LAN: `LanServer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanServer {
    pub motd: String,
    /// `<sender's IP>:<port it named>`, what joining connects to.
    pub address: String,
}

/// `LanServerPinger.parseMotd`.
pub fn parse_motd(ping: &str) -> String {
    const START: &str = "[MOTD]";
    let Some(start) = ping.find(START) else {
        return "missing no".to_owned();
    };
    let from = start + START.len();
    match ping[from..].find("[/MOTD]") {
        Some(end) => ping[from..from + end].to_owned(),
        None => "missing no".to_owned(),
    }
}

/// `LanServerPinger.parseAddress`: the port, if the ping is well formed.
pub fn parse_address(ping: &str) -> Option<&str> {
    const END_MOTD: &str = "[/MOTD]";
    let end_motd = ping.find(END_MOTD)? + END_MOTD.len();
    if ping[end_motd..].contains(END_MOTD) {
        return None;
    }
    let start = end_motd + ping[end_motd..].find("[AD]")? + "[AD]".len();
    let end = ping[start..].find("[/AD]")?;
    Some(&ping[start..start + end])
}

/// `LanServerList`: what was heard, and whether it changed.
#[derive(Default)]
struct List {
    /// Each world with when it was last heard (`LanServer.pingTime`).
    servers: Vec<(LanServer, Instant)>,
    dirty: bool,
}

impl List {
    /// `addServer`. Unlike vanilla, a port that is no number is not listed: joining it would
    /// fail anyway.
    fn add(&mut self, ping: &str, from: std::net::IpAddr, now: Instant) {
        let Some(port) = parse_address(ping).and_then(|p| p.parse::<u16>().ok()) else {
            return;
        };
        let address = format!("{from}:{port}");
        if let Some((_, heard)) = self.servers.iter_mut().find(|(s, _)| s.address == address) {
            *heard = now;
            return;
        }
        if self.servers.len() >= MAX_SERVERS {
            return;
        }
        let server = LanServer {
            motd: parse_motd(ping),
            address,
        };
        self.servers.push((server, now));
        self.dirty = true;
    }

    /// Drops the worlds not heard within [`EXPIRY`].
    fn expire(&mut self, now: Instant) {
        let before = self.servers.len();
        self.servers
            .retain(|(_, heard)| now.duration_since(*heard) < EXPIRY);
        self.dirty |= self.servers.len() != before;
    }
}

/// `LanServerDetector`: listens while it lives.
pub struct LanDetector {
    list: Arc<Mutex<List>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LanDetector {
    /// Starts listening; `None` when the group can't be joined (no network).
    pub fn start() -> Option<Self> {
        let socket = match bind() {
            Ok(socket) => socket,
            Err(error) => {
                melon_base::warn!("No LAN server detection: {error}");
                return None;
            }
        };
        let list = Arc::new(Mutex::new(List::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new()
            .name("lan-detector".to_owned())
            .spawn({
                let (list, stop) = (Arc::clone(&list), Arc::clone(&stop));
                move || listen(&socket, &list, &stop)
            })
            .ok()?;
        Some(Self {
            list,
            stop,
            thread: Some(thread),
        })
    }

    /// `takeDirtyServers`: every server heard, if one came or went since.
    pub fn take_dirty(&self) -> Option<Vec<LanServer>> {
        let mut list = self.list.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut list.dirty).then(|| {
            list.servers
                .iter()
                .map(|(server, _)| server.clone())
                .collect()
        })
    }
}

impl Drop for LanDetector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // It notices within `POLL`; nothing waits for it.
        drop(self.thread.take());
    }
}

/// `MulticastSocket(4445)` in the group. Shared with other listeners on this
/// computer (another game's list): `SO_REUSEADDR`, as Java's sockets set it.
fn bind() -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    // The BSDs (macOS among them) share a multicast port only with this too.
    #[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
    socket.set_reuse_port(true)?;
    let any = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PORT));
    socket.bind(&any.into())?;
    let socket: UdpSocket = socket.into();
    socket.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
    socket.set_read_timeout(Some(POLL))?;
    Ok(socket)
}

fn listen(socket: &UdpSocket, list: &Mutex<List>, stop: &AtomicBool) {
    // Any datagram fits: on Windows a bigger one than the buffer is an error
    // (`WSAEMSGSIZE`) where Java's 1024-byte packet just cuts it.
    let mut buffer = vec![0u8; 1 << 16];
    while !stop.load(Ordering::Relaxed) {
        let received = socket.recv_from(&mut buffer);
        let now = Instant::now();
        let mut list = list.lock().unwrap_or_else(PoisonError::into_inner);
        match received {
            Ok((len, from)) => {
                let text = String::from_utf8_lossy(&buffer[..len.min(1024)]);
                list.add(&text, from.ip(), now);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                melon_base::warn!("LAN server detection stopped: {error}");
                break;
            }
        }
        list.expire(now);
    }
    let _ = socket.leave_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ping_names_its_motd_and_port() {
        let ping = "[MOTD]Steve - World[/MOTD][AD]51234[/AD]";
        assert_eq!(parse_motd(ping), "Steve - World");
        assert_eq!(parse_address(ping), Some("51234"));
        assert_eq!(parse_motd("nothing"), "missing no");
        assert_eq!(parse_address("[MOTD]a[/MOTD]"), None);
        // Two MOTD ends: not trusted.
        assert_eq!(parse_address("[MOTD]a[/MOTD][/MOTD][AD]1[/AD]"), None);
    }

    fn addresses(list: &List) -> Vec<&str> {
        list.servers
            .iter()
            .map(|(s, _)| s.address.as_str())
            .collect()
    }

    #[test]
    fn a_server_is_listed_once_per_address() {
        let mut list = List::default();
        let from = std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7));
        let now = Instant::now();
        list.add("[MOTD]A[/MOTD][AD]1000[/AD]", from, now);
        list.add("[MOTD]A[/MOTD][AD]1000[/AD]", from, now);
        list.add("[MOTD]B[/MOTD][AD]2000[/AD]", from, now);
        list.add("garbage", from, now);
        list.add("[MOTD]C[/MOTD][AD]1000; rm[/AD]", from, now);
        assert_eq!(addresses(&list), ["192.168.1.7:1000", "192.168.1.7:2000"]);
        assert!(list.dirty);
    }

    #[test]
    fn the_list_is_bounded() {
        let mut list = List::default();
        let now = Instant::now();
        for port in 0..200u16 {
            let from = std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
            list.add(&format!("[MOTD]x[/MOTD][AD]{port}[/AD]"), from, now);
        }
        assert_eq!(list.servers.len(), MAX_SERVERS);
    }

    #[test]
    fn a_silent_server_expires_and_a_pinging_one_stays() {
        let mut list = List::default();
        let from = std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7));
        let start = Instant::now();
        list.add("[MOTD]A[/MOTD][AD]1000[/AD]", from, start);
        list.add("[MOTD]B[/MOTD][AD]2000[/AD]", from, start);
        list.dirty = false;
        // B keeps pinging every 1.5 s, A stops.
        for step in 1..=4u32 {
            let now = start + Duration::from_millis(1500) * step;
            list.add("[MOTD]B[/MOTD][AD]2000[/AD]", from, now);
            list.expire(now);
        }
        assert_eq!(addresses(&list), ["192.168.1.7:2000"]);
        assert!(list.dirty, "a world leaving changes the list");
    }

    /// A ping sent to the group comes back to a detector on this computer.
    #[test]
    #[ignore = "needs multicast on the loopback or LAN interface"]
    fn a_detector_hears_a_ping() {
        let detector = LanDetector::start().expect("joins the group");
        let sender = UdpSocket::bind("0.0.0.0:0").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            sender
                .send_to(b"[MOTD]Probe[/MOTD][AD]4321[/AD]", (GROUP, PORT))
                .unwrap();
            if let Some(servers) = detector.take_dirty() {
                assert!(servers.iter().any(|s| s.motd == "Probe"));
                return;
            }
            assert!(std::time::Instant::now() < deadline, "nothing heard");
            thread::sleep(Duration::from_millis(200));
        }
    }
}
