//! Vanilla `ServerNameResolver`: a server address to socket addresses.

#[cfg(not(windows))]
use std::net::UdpSocket;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
#[cfg(not(windows))]
use std::time::Duration;

use crate::network::status::CannotResolve;

const DEFAULT_PORT: u16 = 25565;
#[cfg(not(windows))]
const DNS_TIMEOUT: Duration = Duration::from_secs(2);
const TYPE_SRV: u16 = 33;

/// Where to connect, and the host and port the handshake names.
pub struct Resolved {
    pub addrs: Vec<SocketAddr>,
    pub host: String,
    pub port: u16,
}

/// `resolveAddress`: the SRV redirect is looked up even when the host itself
/// does not resolve (a name with only an SRV record).
pub fn resolve(host: &str, port: u16) -> Result<Resolved, CannotResolve> {
    if port == DEFAULT_PORT
        && host.parse::<IpAddr>().is_err()
        && let Some((target, target_port)) = lookup_srv(host)
    {
        let addrs: Vec<SocketAddr> = (target.as_str(), target_port)
            .to_socket_addrs()
            .map_err(|_| CannotResolve)?
            .collect();
        if addrs.is_empty() {
            return Err(CannotResolve);
        }
        return Ok(Resolved {
            addrs,
            host: target,
            port: target_port,
        });
    }
    let own: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|_| CannotResolve)?
        .collect();
    if own.is_empty() {
        return Err(CannotResolve);
    }
    Ok(Resolved {
        addrs: own,
        host: host.to_owned(),
        port,
    })
}

/// The target and port of `_minecraft._tcp.<host>`, if there is a record: the lowest priority,
/// through the system resolver (its cache, hosts file and DNS-over-HTTPS settings).
#[cfg(windows)]
fn lookup_srv(host: &str) -> Option<(String, u16)> {
    use windows_sys::Win32::NetworkManagement::Dns::{
        DNS_QUERY_STANDARD, DNS_RECORDW, DnsFree, DnsFreeRecordList, DnsQuery_W,
    };

    let name: Vec<u16> = format!("_minecraft._tcp.{host}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut records: *mut DNS_RECORDW = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated; `DnsQuery_W` fills a `DNS_RECORDW` list (the binding
    // names the A type), freed below with `DnsFree`.
    let status = unsafe {
        DnsQuery_W(
            name.as_ptr(),
            TYPE_SRV,
            DNS_QUERY_STANDARD,
            std::ptr::null_mut(),
            (&raw mut records).cast(),
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return None;
    }
    let mut best: Option<(u16, String, u16)> = None;
    let mut at = records;
    while !at.is_null() {
        // SAFETY: `at` is a record of the list, which lives until `DnsFree`; an SRV record's data
        // is the `SRV` member, its target a NUL-terminated wide string.
        let (next, found) = unsafe {
            let record = &*at;
            let found = (record.wType == TYPE_SRV).then(|| {
                let srv = record.Data.SRV;
                let target = if srv.pNameTarget.is_null() {
                    String::new()
                } else {
                    let length = (0..).take_while(|&i| *srv.pNameTarget.add(i) != 0).count();
                    String::from_utf16_lossy(std::slice::from_raw_parts(srv.pNameTarget, length))
                };
                (srv.wPriority, target, srv.wPort)
            });
            (record.pNext, found)
        };
        if let Some(record) = found
            && best.as_ref().is_none_or(|(p, ..)| record.0 < *p)
        {
            best = Some(record);
        }
        at = next;
    }
    if !records.is_null() {
        // SAFETY: the list came from `DnsQuery_W` and is not touched again.
        unsafe { DnsFree(records.cast_const().cast(), DnsFreeRecordList) };
    }
    best.map(|(_, target, port)| (target.trim_end_matches('.').to_owned(), port))
        .filter(|(target, _)| !target.is_empty())
}

/// The target and port of `_minecraft._tcp.<host>`, if there is a record.
#[cfg(not(windows))]
fn lookup_srv(host: &str) -> Option<(String, u16)> {
    let id = rand::random::<u16>();
    let query = srv_query(&format!("_minecraft._tcp.{host}"), id)?;
    for server in nameservers() {
        let Ok(socket) = UdpSocket::bind(if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        }) else {
            continue;
        };
        socket.set_read_timeout(Some(DNS_TIMEOUT)).ok();
        let mut answer = [0u8; 1500];
        if socket.send_to(&query, server).is_err() {
            continue;
        }
        let Ok((len, from)) = socket.recv_from(&mut answer) else {
            continue;
        };
        if from.ip() == server.ip() && answer[..len].starts_with(&id.to_be_bytes()) {
            return parse_srv(&answer[..len]);
        }
    }
    None
}

/// `/etc/resolv.conf`'s `nameserver` lines.
#[cfg(not(windows))]
fn nameservers() -> Vec<SocketAddr> {
    let Ok(conf) = std::fs::read_to_string("/etc/resolv.conf") else {
        return Vec::new();
    };
    conf.lines()
        .filter_map(|line| line.trim().strip_prefix("nameserver"))
        .filter_map(|address| address.split('%').next()?.trim().parse::<IpAddr>().ok())
        .map(|ip| SocketAddr::new(ip, 53))
        .collect()
}

/// A recursive query for `name`'s SRV record, with transaction `id`.
#[cfg(any(not(windows), test))]
fn srv_query(name: &str, id: u16) -> Option<Vec<u8>> {
    let [hi, lo] = id.to_be_bytes();
    let mut query = vec![hi, lo, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        query.push(label.len() as u8);
        query.extend(label.as_bytes());
    }
    query.push(0);
    query.extend(TYPE_SRV.to_be_bytes());
    query.extend(1u16.to_be_bytes());
    Some(query)
}

/// The best SRV answer (lowest priority) of a DNS response.
#[cfg(any(not(windows), test))]
fn parse_srv(message: &[u8]) -> Option<(String, u16)> {
    let u16_at = |at: usize| {
        Some(u16::from_be_bytes([
            *message.get(at)?,
            *message.get(at + 1)?,
        ]))
    };
    if message.get(2)? & 0x80 == 0 || message.get(3)? & 0x0F != 0 {
        return None;
    }
    let (questions, answers) = (u16_at(4)?, u16_at(6)?);
    let mut at = 12;
    for _ in 0..questions {
        at = skip_name(message, at)? + 4;
    }
    let mut best: Option<(u16, String, u16)> = None;
    for _ in 0..answers {
        at = skip_name(message, at)?;
        let (kind, length) = (u16_at(at)?, usize::from(u16_at(at + 8)?));
        let data = at + 10;
        if kind == TYPE_SRV {
            let (priority, port) = (u16_at(data)?, u16_at(data + 4)?);
            let target = read_name(message, data + 6)?;
            if best.as_ref().is_none_or(|(p, ..)| priority < *p) {
                best = Some((priority, target, port));
            }
        }
        at = data + length;
    }
    best.map(|(_, target, port)| (target, port))
        .filter(|(target, _)| !target.is_empty())
}

/// The offset after the name starting at `at` (a pointer ends it).
#[cfg(any(not(windows), test))]
fn skip_name(message: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let length = *message.get(at)?;
        match length {
            0 => return Some(at + 1),
            l if l & 0xC0 == 0xC0 => return Some(at + 2),
            l => at += 1 + usize::from(l),
        }
    }
}

/// A name, following compression pointers (at most a few, so a loop in a reply ends).
#[cfg(any(not(windows), test))]
fn read_name(message: &[u8], mut at: usize) -> Option<String> {
    let mut name = String::new();
    for _ in 0..64 {
        let length = *message.get(at)?;
        if length == 0 {
            return Some(name);
        }
        if length & 0xC0 == 0xC0 {
            at = usize::from(length & 0x3F) << 8 | usize::from(*message.get(at + 1)?);
            continue;
        }
        let label = message.get(at + 1..at + 1 + usize::from(length))?;
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(std::str::from_utf8(label).ok()?);
        at += 1 + usize::from(length);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A response to `_minecraft._tcp.a.b`.
    fn response() -> Vec<u8> {
        let mut m = vec![0x4D, 0x4C, 0x81, 0x80, 0, 1, 0, 2, 0, 0, 0, 0];
        for label in ["_minecraft", "_tcp", "a", "b"] {
            m.push(label.len() as u8);
            m.extend(label.as_bytes());
        }
        m.extend([0, 0, 33, 0, 1]);
        let mut far = vec![0, 10, 0, 5, 0x04, 0x57];
        for label in ["far", "example"] {
            far.push(label.len() as u8);
            far.extend(label.as_bytes());
        }
        far.push(0);
        m.extend([0xC0, 12, 0, 33, 0, 1, 0, 0, 0, 60]);
        m.extend((far.len() as u16).to_be_bytes());
        m.extend(far);
        let mut near = vec![0, 1, 0, 5, 0x63, 0xDD, 4];
        near.extend(b"play");
        near.extend([0xC0, 12 + 11 + 5]);
        m.extend([0xC0, 12, 0, 33, 0, 1, 0, 0, 0, 60]);
        m.extend((near.len() as u16).to_be_bytes());
        m.extend(near);
        m
    }

    #[test]
    fn the_lowest_priority_record_wins() {
        assert_eq!(parse_srv(&response()), Some(("play.a.b".to_owned(), 25565)));
    }

    #[test]
    fn answers_without_a_record_or_with_an_error_give_none() {
        let mut empty = response();
        empty[7] = 0;
        assert_eq!(parse_srv(&empty), None);
        let mut error = response();
        error[3] = 0x83;
        assert_eq!(parse_srv(&error), None);
        assert_eq!(parse_srv(&response()[..20]), None);
    }

    /// `hypixel.net` names `mc.hypixel.net` in its SRV record.
    #[test]
    #[ignore = "needs the network"]
    fn a_public_srv_record_redirects() {
        let resolved = resolve("hypixel.net", DEFAULT_PORT).unwrap();
        assert_eq!(resolved.host, "mc.hypixel.net");
        assert!(!resolved.addrs.is_empty());
    }

    #[test]
    fn the_query_names_the_srv_record() {
        let query = srv_query("_minecraft._tcp.example.org", 0xBEEF).unwrap();
        assert_eq!(query[..2], [0xBE, 0xEF]);
        assert_eq!(&query[12..23], b"\x0a_minecraft");
        assert_eq!(query[query.len() - 4..], [0, 33, 0, 1]);
        assert!(srv_query("a..b", 0).is_none());
    }
}
