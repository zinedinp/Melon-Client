//! Vanilla `LanServerDetection`: worlds opened to the LAN, heard from the
//! `LanServerPinger` multicast every computer on the network may send.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// `LanServerPinger.MULTICAST_GROUP` and `PING_PORT`.
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
const PORT: u16 = 4445;
/// How often the thread looks whether it should stop (vanilla waits 5 s).
const POLL: Duration = Duration::from_millis(500);

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

/// `LanServerList`: what was heard, and whether anything new came.
#[derive(Default)]
struct List {
    servers: Vec<LanServer>,
    dirty: bool,
}

impl List {
    /// `addServer`.
    fn add(&mut self, ping: &str, from: std::net::IpAddr) {
        let Some(port) = parse_address(ping) else {
            return;
        };
        let address = format!("{from}:{port}");
        if self.servers.iter().any(|s| s.address == address) {
            return;
        }
        self.servers.push(LanServer {
            motd: parse_motd(ping),
            address,
        });
        self.dirty = true;
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
                crate::warn!("No LAN server detection: {error}");
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

    /// `takeDirtyServers`: every server heard, if a new one came since.
    pub fn take_dirty(&self) -> Option<Vec<LanServer>> {
        let mut list = self.list.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut list.dirty).then(|| list.servers.clone())
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
    let mut buffer = [0u8; 1024];
    while !stop.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buffer) {
            Ok((len, from)) => {
                let ping = String::from_utf8_lossy(&buffer[..len]);
                list.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .add(&ping, from.ip());
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                crate::warn!("LAN server detection stopped: {error}");
                break;
            }
        }
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

    #[test]
    fn a_server_is_listed_once_per_address() {
        let mut list = List::default();
        let from = std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7));
        list.add("[MOTD]A[/MOTD][AD]1000[/AD]", from);
        list.add("[MOTD]A[/MOTD][AD]1000[/AD]", from);
        list.add("[MOTD]B[/MOTD][AD]2000[/AD]", from);
        list.add("garbage", from);
        let addresses: Vec<_> = list.servers.iter().map(|s| s.address.as_str()).collect();
        assert_eq!(addresses, ["192.168.1.7:1000", "192.168.1.7:2000"]);
        assert!(list.dirty);
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
