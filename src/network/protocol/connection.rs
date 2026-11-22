//! Wraps the byte stream (a `TcpStream`, or the in-memory pipe to the integrated server) with
//! framing, compression and encryption.

use super::packet;
use aes::Aes128;
use aes::cipher::KeyIvInit;
use anyhow::Result;
use cfb8::{Decryptor, Encryptor};
use std::io::{self, Read, Write};

/// Anything the protocol can be spoken over.
pub trait Stream: Read + Write + Send {
    /// Splits into independent read and write ends, so one thread can block on reading while
    /// another writes.
    fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)>;
}

impl Stream for std::net::TcpStream {
    fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        let write = self.try_clone()?;
        Ok((self, Box::new(write)))
    }
}

pub struct Connection {
    stream: Box<dyn Stream>,
    read_ahead: ReadAhead,
    encryptor: Option<Encryptor<Aes128>>,
    decryptor: Option<Decryptor<Aes128>>,
    /// Threshold from `ClientboundLoginCompressionPacket`, or `None` while uncompressed.
    compression: Option<i32>,
}

impl Connection {
    pub fn new(stream: impl Stream + 'static) -> Self {
        Self {
            stream: Box::new(stream),
            read_ahead: ReadAhead::default(),
            encryptor: None,
            decryptor: None,
            compression: None,
        }
    }

    /// Switches the connection to encrypted mode from this point on.
    pub fn enable_encryption(&mut self, key: &[u8; 16]) {
        self.encryptor = Some(Encryptor::new(key.into(), key.into()));
        self.decryptor = Some(Decryptor::new(key.into(), key.into()));
    }

    /// `ClientHandshakePacketListenerImpl.handleCompression`:
    pub fn enable_compression(&mut self, threshold: i32) {
        self.compression = (threshold >= 0).then_some(threshold);
    }

    pub fn read_packet(&mut self) -> Result<(i32, Vec<u8>)> {
        packet::read_packet(self, self.compression)
    }

    pub fn write_packet(
        &mut self,
        id: i32,
        write_body: impl FnOnce(&mut Vec<u8>) -> Result<()>,
    ) -> Result<()> {
        packet::write_packet(self, self.compression, id, write_body)
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_ahead
            .read(&mut *self.stream, self.decryptor.as_mut(), buf)
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }

    /// See [`write_encrypted`].
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        write_encrypted(&mut *self.stream, self.encryptor.as_mut(), buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl Connection {
    /// Splits once logged in.
    pub fn into_split(self) -> io::Result<(ReadHalf, WriteHalf)> {
        let (read, write) = self.stream.split()?;
        Ok((
            ReadHalf {
                stream: read,
                read_ahead: self.read_ahead,
                decryptor: self.decryptor,
                compression: self.compression,
            },
            WriteHalf {
                stream: write,
                encryptor: self.encryptor,
                compression: self.compression,
                play: false,
            },
        ))
    }
}

pub struct ReadHalf {
    stream: Box<dyn Read + Send>,
    read_ahead: ReadAhead,
    decryptor: Option<Decryptor<Aes128>>,
    compression: Option<i32>,
}

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_ahead
            .read(&mut *self.stream, self.decryptor.as_mut(), buf)
    }
}

impl ReadHalf {
    pub fn read_packet(&mut self) -> Result<(i32, Vec<u8>)> {
        packet::read_packet(self, self.compression)
    }
}

pub struct WriteHalf {
    stream: Box<dyn Write + Send>,
    encryptor: Option<Encryptor<Aes128>>,
    compression: Option<i32>,
    /// Past the configuration phase.
    pub play: bool,
}

impl WriteHalf {
    pub(crate) fn compression(&self) -> Option<i32> {
        self.compression
    }

    pub fn write_packet(
        &mut self,
        id: i32,
        write_body: impl FnOnce(&mut Vec<u8>) -> Result<()>,
    ) -> Result<()> {
        packet::write_packet(self, self.compression, id, write_body)
    }
}

impl Write for WriteHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }

    /// See [`write_encrypted`].
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        write_encrypted(&mut *self.stream, self.encryptor.as_mut(), buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// Bytes read from the stream but not yet taken.
#[derive(Default)]
struct ReadAhead {
    buf: Vec<u8>,
    pos: usize,
}

impl ReadAhead {
    /// How much one refill asks the stream for.
    const CAPACITY: usize = 64 * 1024;

    /// Hands out buffered bytes first, and refills only when they run out.
    fn read(
        &mut self,
        stream: &mut (impl Read + ?Sized),
        decryptor: Option<&mut Decryptor<Aes128>>,
        out: &mut [u8],
    ) -> io::Result<usize> {
        if self.pos == self.buf.len() {
            if out.len() >= Self::CAPACITY {
                return read_decrypted(stream, decryptor, out);
            }
            self.buf.resize(Self::CAPACITY, 0);
            self.pos = 0;
            match read_decrypted(stream, decryptor, &mut self.buf) {
                Ok(n) => self.buf.truncate(n),
                Err(e) => {
                    self.buf.clear();
                    return Err(e);
                }
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Reads, then decrypts what arrived when encryption is on.
fn read_decrypted(
    stream: &mut (impl Read + ?Sized),
    decryptor: Option<&mut Decryptor<Aes128>>,
    buf: &mut [u8],
) -> io::Result<usize> {
    let n = stream.read(buf)?;
    if let Some(decryptor) = decryptor {
        decryptor.decrypt(&mut buf[..n]);
    }
    Ok(n)
}

/// Encrypts the whole buffer once, when encryption is on, then writes it.
fn write_encrypted(
    stream: &mut (impl Write + ?Sized),
    encryptor: Option<&mut Encryptor<Aes128>>,
    buf: &[u8],
) -> io::Result<()> {
    match encryptor {
        Some(encryptor) => {
            let mut ciphertext = buf.to_vec();
            encryptor.encrypt(&mut ciphertext);
            stream.write_all(&ciphertext)
        }
        None => stream.write_all(buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::packet::write_packet;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// A stream whose incoming bytes the test adds as it goes.
    #[derive(Clone)]
    struct Trickle {
        incoming: Arc<Mutex<VecDeque<u8>>>,
        step: usize,
        reads: Arc<Mutex<usize>>,
    }

    impl Trickle {
        fn new(step: usize) -> Self {
            Self {
                incoming: Arc::default(),
                step,
                reads: Arc::default(),
            }
        }

        fn push(&self, bytes: &[u8]) {
            self.incoming.lock().unwrap().extend(bytes);
        }
    }

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            *self.reads.lock().unwrap() += 1;
            let mut incoming = self.incoming.lock().unwrap();
            let n = buf.len().min(self.step).min(incoming.len());
            for (slot, byte) in buf.iter_mut().zip(incoming.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }
    }

    impl Write for Trickle {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Stream for Trickle {
        fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
            Ok((self.clone(), self))
        }
    }

    fn packet(id: i32, body: &[u8]) -> Vec<u8> {
        packet_with(None, id, body)
    }

    fn packet_with(compression: Option<i32>, id: i32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_packet(&mut out, compression, id, |buf| {
            buf.extend_from_slice(body);
            Ok(())
        })
        .unwrap();
        out
    }

    #[test]
    fn packets_survive_any_delivery_size() {
        for step in [1, 3, 7, 1 << 20] {
            let stream = Trickle::new(step);
            let big = vec![0xAB; 200_000];
            stream.push(&packet(1, b"hello"));
            stream.push(&packet(2, &big));
            stream.push(&packet(3, b""));
            let mut conn = Connection::new(stream);
            assert_eq!(conn.read_packet().unwrap(), (1, b"hello".to_vec()));
            assert_eq!(conn.read_packet().unwrap(), (2, big));
            assert_eq!(conn.read_packet().unwrap(), (3, Vec::new()));
        }
    }

    #[test]
    fn small_packets_take_one_read_not_one_per_byte() {
        let stream = Trickle::new(usize::MAX);
        for i in 0..100 {
            stream.push(&packet(i, &[i as u8; 300]));
        }
        let reads = Arc::clone(&stream.reads);
        let mut conn = Connection::new(stream);
        for i in 0..100 {
            assert_eq!(conn.read_packet().unwrap().0, i);
        }
        assert_eq!(*reads.lock().unwrap(), 1);
    }

    #[test]
    fn encryption_starts_exactly_at_the_switch() {
        let key = [7u8; 16];
        let stream = Trickle::new(5);
        let incoming = stream.clone();
        let mut conn = Connection::new(stream);

        incoming.push(&packet(1, b"plain"));
        assert_eq!(conn.read_packet().unwrap(), (1, b"plain".to_vec()));
        conn.enable_encryption(&key);

        let mut cipher = Encryptor::<Aes128>::new(&key.into(), &key.into());
        let mut secret = packet(2, b"secret");
        secret.extend(packet(3, &[9; 70_000]));
        cipher.encrypt(&mut secret);
        incoming.push(&secret);

        assert_eq!(conn.read_packet().unwrap(), (2, b"secret".to_vec()));
        let (mut reader, _) = conn.into_split().unwrap();
        assert_eq!(reader.read_packet().unwrap(), (3, vec![9; 70_000]));
    }

    #[test]
    fn compression_survives_encryption_and_the_split() {
        let key = [9u8; 16];
        let threshold = 64;
        let stream = Trickle::new(7);
        let incoming = stream.clone();
        let mut conn = Connection::new(stream);

        incoming.push(&packet(1, b"plain"));
        assert_eq!(conn.read_packet().unwrap(), (1, b"plain".to_vec()));
        conn.enable_compression(threshold);
        conn.enable_encryption(&key);

        let mut cipher = Encryptor::<Aes128>::new(&key.into(), &key.into());
        let mut secret = packet_with(Some(threshold), 2, b"tiny");
        secret.extend(packet_with(Some(threshold), 3, &[9; 70_000]));
        cipher.encrypt(&mut secret);
        incoming.push(&secret);

        assert_eq!(conn.read_packet().unwrap(), (2, b"tiny".to_vec()));
        let (mut reader, _) = conn.into_split().unwrap();
        assert_eq!(reader.read_packet().unwrap(), (3, vec![9; 70_000]));
    }

    #[test]
    fn vanilla_enables_encryption_then_compression() {
        let key = [3u8; 16];
        let threshold = 64;
        let stream = Trickle::new(4);
        let incoming = stream.clone();
        let mut conn = Connection::new(stream);

        incoming.push(&packet(1, b"plain"));
        assert_eq!(conn.read_packet().unwrap(), (1, b"plain".to_vec()));
        conn.enable_encryption(&key);

        let mut cipher = Encryptor::<Aes128>::new(&key.into(), &key.into());
        let mut hello = packet(2, b"secret");
        cipher.encrypt(&mut hello);
        incoming.push(&hello);
        assert_eq!(conn.read_packet().unwrap(), (2, b"secret".to_vec()));

        conn.enable_compression(threshold);
        let mut compressed = packet_with(Some(threshold), 3, &[7; 8_000]);
        cipher.encrypt(&mut compressed);
        incoming.push(&compressed);
        assert_eq!(conn.read_packet().unwrap(), (3, vec![7; 8_000]));
    }

    #[test]
    fn negative_threshold_turns_compression_off() {
        let stream = Trickle::new(usize::MAX);
        stream.push(&packet(1, b"raw"));
        let mut conn = Connection::new(stream);
        conn.enable_compression(256);
        conn.enable_compression(-1);
        assert_eq!(conn.read_packet().unwrap(), (1, b"raw".to_vec()));
    }

    #[test]
    fn compression_applies_to_bytes_already_buffered() {
        let threshold = 64;
        let stream = Trickle::new(usize::MAX);
        let mut threshold_body = Vec::new();
        crate::network::protocol::varint::write_var_int(&mut threshold_body, threshold).unwrap();
        let mut wire = packet(3, &threshold_body);
        wire.extend(packet_with(Some(threshold), 2, &[9; 300]));
        stream.push(&wire);

        let mut conn = Connection::new(stream);
        let (id, body) = conn.read_packet().unwrap();
        assert_eq!(id, 3);
        assert_eq!(body, threshold_body);
        conn.enable_compression(threshold);
        assert_eq!(conn.read_packet().unwrap(), (2, vec![9; 300]));
    }
}
