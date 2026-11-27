//! Packet framing.

use super::compression::{self, MAXIMUM_UNCOMPRESSED_LENGTH};
use super::varint::{read_var_int, write_var_int};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};

pub use super::reader::{
    Reader, pack_block_pos, write_bool, write_f32_be, write_lp_vec3, write_string, write_uuid,
};

/// `Varint21FrameDecoder` max: a 3-byte VarInt, 2^21 − 1.
const MAX_PACKET_LEN: i32 = 2_097_152;

/// Room for the longest `VarInt` length prefix, written right-aligned once the length is known.
const PREFIX_ROOM: usize = 5;

/// Reads one length-prefixed packet and splits off its `VarInt` packet ID.
pub fn read_packet(stream: &mut impl Read, compression: Option<i32>) -> Result<(i32, Vec<u8>)> {
    let len = read_var_int(stream).context("packet length")?;
    if !(0..=MAX_PACKET_LEN).contains(&len) {
        bail!("implausible packet length {len}");
    }
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).context("packet body")?;
    if compression.is_some() {
        buf = unwrap_compressed(buf)?;
    }
    split_packet_id(buf)
}

/// Vanilla `CompressionDecoder.decode` on one already-split frame.
fn unwrap_compressed(buf: Vec<u8>) -> Result<Vec<u8>> {
    let mut rest = buf.as_slice();
    let data_len = read_var_int(&mut rest).context("data length")?;
    if data_len == 0 {
        let skip = buf.len() - rest.len();
        let mut payload = buf;
        payload.drain(..skip);
        return Ok(payload);
    }
    compression::inflate(rest, data_len)
}

fn split_packet_id(mut buf: Vec<u8>) -> Result<(i32, Vec<u8>)> {
    let mut body = buf.as_slice();
    let id = read_var_int(&mut body).context("packet id")?;
    let id_len = buf.len() - body.len();
    buf.drain(..id_len);
    Ok((id, buf))
}

/// Writes one length-prefixed packet: `id` followed by whatever `write_body`
pub fn write_packet(
    stream: &mut impl Write,
    compression: Option<i32>,
    id: i32,
    write_body: impl FnOnce(&mut Vec<u8>) -> Result<()>,
) -> Result<()> {
    match compression {
        None => {
            let mut framed = vec![0u8; PREFIX_ROOM];
            write_var_int(&mut framed, id)?;
            write_body(&mut framed)?;
            trace_sent(id, &framed[PREFIX_ROOM..]);
            send_framed(stream, framed)
        }
        Some(threshold) => {
            let mut payload = Vec::new();
            write_var_int(&mut payload, id)?;
            write_body(&mut payload)?;
            trace_sent(id, &payload);
            if payload.len() > MAXIMUM_UNCOMPRESSED_LENGTH as usize {
                bail!(
                    "packet too big (is {}, should be less than {MAXIMUM_UNCOMPRESSED_LENGTH})",
                    payload.len()
                );
            }
            let mut inner = vec![0u8; PREFIX_ROOM];
            if (payload.len() as i32) < threshold {
                write_var_int(&mut inner, 0)?;
                inner.extend_from_slice(&payload);
            } else {
                let compressed = compression::deflate(&payload)?;
                write_var_int(&mut inner, payload.len() as i32)?;
                inner.extend_from_slice(&compressed);
            }
            send_framed(stream, inner)
        }
    }
}

/// `MELON_PACKET_DEBUG=sent`.
fn trace_sent(id: i32, body: &[u8]) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let on = *ON.get_or_init(|| {
        std::env::var("MELON_PACKET_DEBUG").is_ok_and(|v| v.split(',').any(|s| s.trim() == "sent"))
    });
    if on {
        let head: String = body.iter().take(24).map(|b| format!("{b:02x}")).collect();
        eprintln!("[packet] -> 0x{id:02x} {} B {head}", body.len());
    }
}

fn send_framed(stream: &mut impl Write, mut framed: Vec<u8>) -> Result<()> {
    let mut prefix = Vec::with_capacity(PREFIX_ROOM);
    write_var_int(&mut prefix, (framed.len() - PREFIX_ROOM) as i32)?;
    let start = PREFIX_ROOM - prefix.len();
    framed[start..PREFIX_ROOM].copy_from_slice(&prefix);
    stream.write_all(&framed[start..])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(compression: Option<i32>, body: &[u8]) {
        let mut wire = Vec::new();
        write_packet(&mut wire, compression, 0x2A, |buf| {
            buf.extend_from_slice(body);
            Ok(())
        })
        .unwrap();
        let (id, read) = read_packet(&mut wire.as_slice(), compression).unwrap();
        assert_eq!(id, 0x2A);
        assert_eq!(read, body);
    }

    #[test]
    fn packets_round_trip_with_short_and_long_length_prefixes() {
        for body_len in [0usize, 1, 126, 127, 128, 300, 70_000] {
            let body: Vec<u8> = (0..body_len).map(|i| i as u8).collect();
            round_trip(None, &body);
        }
    }

    #[test]
    fn compressed_packets_round_trip_below_and_above_threshold() {
        let threshold = Some(256);
        round_trip(threshold, b"tiny");
        round_trip(threshold, &vec![0xAB; 256]);
        round_trip(threshold, &vec![0xCD; 70_000]);
    }

    #[test]
    fn uncompressed_write_is_not_read_as_compressed() {
        let mut wire = Vec::new();
        write_packet(&mut wire, None, 1, |buf| {
            buf.extend_from_slice(b"hello");
            Ok(())
        })
        .unwrap();
        assert!(read_packet(&mut wire.as_slice(), Some(256)).is_err());
    }

    #[test]
    fn packets_below_threshold_carry_a_zero_data_length() {
        let mut wire = Vec::new();
        write_packet(&mut wire, Some(256), 1, |buf| {
            buf.extend_from_slice(b"hi");
            Ok(())
        })
        .unwrap();
        let mut rest = wire.as_slice();
        let frame_len = crate::varint::read_var_int(&mut rest).unwrap();
        let mut frame = &rest[..frame_len as usize];
        let data_len = crate::varint::read_var_int(&mut frame).unwrap();
        assert_eq!(data_len, 0);
    }

    #[test]
    fn packets_at_the_threshold_are_zlib() {
        let mut wire = Vec::new();
        let body = vec![0xAB; 256];
        write_packet(&mut wire, Some(256), 0, |buf| {
            buf.extend_from_slice(&body);
            Ok(())
        })
        .unwrap();
        let mut rest = wire.as_slice();
        let frame_len = crate::varint::read_var_int(&mut rest).unwrap();
        let mut frame = &rest[..frame_len as usize];
        let data_len = crate::varint::read_var_int(&mut frame).unwrap();
        assert_eq!(data_len, 1 + 256);
        assert_ne!(frame, body.as_slice());
    }
}
