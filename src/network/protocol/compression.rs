//! Vanilla `CompressionDecoder` / `CompressionEncoder`.

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

/// `CompressionDecoder.MAXIMUM_UNCOMPRESSED_LENGTH`.
pub const MAXIMUM_UNCOMPRESSED_LENGTH: i32 = 8_388_608;

/// Inflates zlib `src` into exactly `uncompressed_len` bytes.
pub fn inflate(src: &[u8], uncompressed_len: i32) -> Result<Vec<u8>> {
    if uncompressed_len <= 0 || uncompressed_len > MAXIMUM_UNCOMPRESSED_LENGTH {
        bail!(
            "badly compressed packet - size of {uncompressed_len} is larger than protocol maximum of {MAXIMUM_UNCOMPRESSED_LENGTH}"
        );
    }
    let mut decoder = ZlibDecoder::new(src);
    let mut out = vec![0u8; uncompressed_len as usize];
    decoder
        .read_exact(&mut out)
        .context("badly compressed packet")?;
    let mut extra = [0u8; 1];
    match decoder.read(&mut extra) {
        Ok(0) => Ok(out),
        Ok(_) => bail!(
            "badly compressed packet - actual length of uncompressed payload is greater than declared size {uncompressed_len}"
        ),
        Err(e) => Err(e).context("badly compressed packet"),
    }
}

/// Deflates `data` as zlib. Vanilla `CompressionEncoder` uses `new Deflater()`
pub fn deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).context("compressing packet")?;
    encoder.finish().context("compressing packet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zlib_round_trips() {
        let src = b"Hello, compressed world!";
        let compressed = deflate(src).unwrap();
        assert_ne!(&compressed[..], src as &[u8]);
        assert_eq!(inflate(&compressed, src.len() as i32).unwrap(), src);
    }

    #[test]
    fn inflate_rejects_a_wrong_declared_size() {
        let src = b"abcdef";
        let compressed = deflate(src).unwrap();
        assert!(inflate(&compressed, 3).is_err());
        assert!(inflate(&compressed, 100).is_err());
    }

    #[test]
    fn inflate_rejects_invalid_zlib() {
        assert!(inflate(&[0xFF, 0xFF, 0xFF], 10).is_err());
    }
}
