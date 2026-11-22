//! Typed packet-body reader and the small writers used to build bodies.

use super::varint::{read_var_int, write_var_int};
use anyhow::{Result, bail};
use std::io::{Read, Write};

pub fn write_string(w: &mut impl Write, s: &str) -> Result<()> {
    write_var_int(w, s.len() as i32)?;
    w.write_all(s.as_bytes())?;
    Ok(())
}

pub fn write_bool(w: &mut impl Write, b: bool) -> Result<()> {
    w.write_all(&[u8::from(b)])?;
    Ok(())
}

/// `LpVec3.write`.
pub fn write_lp_vec3(w: &mut impl Write, value: [f64; 3]) -> Result<()> {
    const LIMIT: f64 = 1.717_986_918_3e10;
    let [x, y, z] = value.map(|v| {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(-LIMIT, LIMIT)
        }
    });
    let longest = x.abs().max(y.abs()).max(z.abs());
    if longest < 3.051_944_088_384_301e-5 {
        w.write_all(&[0])?;
        return Ok(());
    }
    let scale = longest.ceil() as u64;
    let partial = scale & 3 != scale;
    let markers = if partial { scale & 3 | 4 } else { scale };
    let scale_f = scale as f64;
    let pack = |v: f64| ((v / scale_f * 0.5 + 0.5) * 32766.0).round() as u64;
    let buffer = markers | pack(x) << 3 | pack(y) << 18 | pack(z) << 33;
    w.write_all(&[buffer as u8, (buffer >> 8) as u8])?;
    w.write_all(&((buffer >> 16) as u32).to_be_bytes())?;
    if partial {
        write_var_int(w, (scale >> 2) as i32)?;
    }
    Ok(())
}

pub fn write_uuid(w: &mut impl Write, uuid: &uuid::Uuid) -> Result<()> {
    w.write_all(uuid.as_bytes())?;
    Ok(())
}

pub fn write_f32_be(w: &mut impl Write, v: f32) -> Result<()> {
    w.write_all(&v.to_be_bytes())?;
    Ok(())
}

/// Typed cursor over one packet body (the bytes after the packet-ID `VarInt`).
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    pub fn var_int(&mut self) -> Result<i32> {
        Ok(read_var_int(&mut self.buf)?)
    }

    pub fn u8(&mut self) -> Result<u8> {
        let mut b = [0u8; 1];
        self.buf.read_exact(&mut b)?;
        Ok(b[0])
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn i16_be(&mut self) -> Result<i16> {
        let mut b = [0u8; 2];
        self.buf.read_exact(&mut b)?;
        Ok(i16::from_be_bytes(b))
    }

    pub fn i32_be(&mut self) -> Result<i32> {
        let mut b = [0u8; 4];
        self.buf.read_exact(&mut b)?;
        Ok(i32::from_be_bytes(b))
    }

    pub fn i64_be(&mut self) -> Result<i64> {
        let mut b = [0u8; 8];
        self.buf.read_exact(&mut b)?;
        Ok(i64::from_be_bytes(b))
    }

    /// `VarLong`: 7 bits per byte, low first, up to 10 bytes.
    pub fn var_long(&mut self) -> Result<i64> {
        let mut value = 0i64;
        for shift in (0..70).step_by(7) {
            let byte = self.u8()?;
            value |= i64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        anyhow::bail!("VarLong is too long")
    }

    /// `BlockPos.STREAM_CODEC`.
    pub fn block_pos(&mut self) -> Result<[i32; 3]> {
        Ok(unpack_block_pos(self.i64_be()?))
    }

    pub fn f32_be(&mut self) -> Result<f32> {
        let mut b = [0u8; 4];
        self.buf.read_exact(&mut b)?;
        Ok(f32::from_be_bytes(b))
    }

    pub fn f64_be(&mut self) -> Result<f64> {
        let mut b = [0u8; 8];
        self.buf.read_exact(&mut b)?;
        Ok(f64::from_be_bytes(b))
    }

    /// `LpVec3.read`.
    pub fn lp_vec3(&mut self) -> Result<[f64; 3]> {
        let lowest = u64::from(self.u8()?);
        if lowest == 0 {
            return Ok([0.0; 3]);
        }
        let middle = u64::from(self.u8()?);
        let highest = u64::from(self.i32_be()? as u32);
        let packed = highest << 16 | middle << 8 | lowest;
        let mut scale = lowest & 3;
        if lowest & 4 != 0 {
            scale |= u64::from(self.var_int()? as u32) << 2;
        }
        let unpack = |bits: u64| ((bits & 0x7FFF) as f64).min(32766.0) * 2.0 / 32766.0 - 1.0;
        let scale = scale as f64;
        Ok([
            unpack(packed >> 3) * scale,
            unpack(packed >> 18) * scale,
            unpack(packed >> 33) * scale,
        ])
    }

    /// Three big-endian doubles (`Vec3.STREAM_CODEC`).
    pub fn vec3_f64(&mut self) -> Result<[f64; 3]> {
        Ok([self.f64_be()?, self.f64_be()?, self.f64_be()?])
    }

    /// A string of at most vanilla's default 32767 characters.
    pub fn string(&mut self) -> Result<String> {
        self.string_max(32767)
    }

    /// A string of at most `max_chars` characters, checked the way vanilla's `readUtf` does.
    pub fn string_max(&mut self, max_chars: usize) -> Result<String> {
        let len = self.var_int()?;
        let max_bytes = max_chars.saturating_mul(3);
        let Ok(len) = usize::try_from(len) else {
            bail!("string length {len} is less than zero");
        };
        if len > max_bytes {
            bail!("string of {len} bytes is longer than the {max_bytes} allowed");
        }
        let bytes = self.bytes(len)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if text.chars().count() > max_chars {
            bail!("string is longer than the {max_chars} characters allowed");
        }
        Ok(text)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        if n > self.buf.len() {
            bail!("tried to skip {n} bytes but only {} remain", self.buf.len());
        }
        self.buf = &self.buf[n..];
        Ok(())
    }

    /// `UUIDUtil.STREAM_CODEC`: two big-endian longs.
    pub fn uuid(&mut self) -> Result<uuid::Uuid> {
        let high = self.i64_be()? as u64;
        let low = self.i64_be()? as u64;
        Ok(uuid::Uuid::from_u64_pair(high, low))
    }

    /// Reads `n` raw bytes (the DER public key, verify token or shared secret).
    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        if n > self.buf.len() {
            bail!("tried to read {n} bytes but only {} remain", self.buf.len());
        }
        let mut out = vec![0u8; n];
        self.buf.read_exact(&mut out)?;
        Ok(out)
    }

    /// The unread bytes, for a decoder that reads straight from a slice.
    pub fn remaining(&mut self) -> &mut &'a [u8] {
        &mut self.buf
    }

    /// Splits off and returns the next `n` bytes as their own sub-slice.
    pub fn slice(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.buf.len() {
            bail!("tried to read {n} bytes but only {} remain", self.buf.len());
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(head)
    }

    pub fn take(&mut self, n: usize) -> Result<Reader<'a>> {
        if n > self.buf.len() {
            bail!("tried to take {n} bytes but only {} remain", self.buf.len());
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(Reader::new(head))
    }
}

/// `BlockPos.of(long)`.
pub fn unpack_block_pos(packed: i64) -> [i32; 3] {
    [
        (packed >> 38) as i32,
        (packed << 52 >> 52) as i32,
        (packed << 26 >> 38) as i32,
    ]
}

/// `BlockPos.asLong`: x in the top 26 bits, z in the next 26, y in the low 12.
pub fn pack_block_pos([x, y, z]: [i32; 3]) -> i64 {
    ((i64::from(x) & 0x3FF_FFFF) << 38)
        | ((i64::from(z) & 0x3FF_FFFF) << 12)
        | (i64::from(y) & 0xFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lp_vec3_round_trips_within_its_precision() {
        for v in [
            [0.0; 3],
            [0.3, -0.45, 0.05],
            [1.2, 0.0, -0.7],
            [-7.5, 3.25, 40.0],
        ] {
            let mut buf = Vec::new();
            write_lp_vec3(&mut buf, v).unwrap();
            let back = Reader::new(&buf).lp_vec3().unwrap();
            let longest = v.iter().fold(0.0f64, |m, a| m.max(a.abs())).ceil().max(1.0);
            for (a, b) in v.iter().zip(back) {
                assert!(
                    (a - b).abs() <= longest / 16000.0,
                    "{v:?} came back as {back:?}"
                );
            }
        }
    }

    #[test]
    fn packed_block_pos_round_trips() {
        for pos in [[0, 0, 0], [1, 2, 3], [-12, 64, 80], [100, -60, -200]] {
            assert_eq!(unpack_block_pos(pack_block_pos(pos)), pos);
        }
    }

    #[test]
    fn strings_are_checked_before_allocating() {
        for prefix in [
            &[0xff, 0xff, 0xff, 0xff, 0x0f][..],
            &[0xff, 0xff, 0xff, 0x7f][..],
        ] {
            assert!(Reader::new(prefix).string().is_err());
        }
        assert!(Reader::new(&[5, b'a']).string().is_err());
        assert!(
            Reader::new(&[4, b'a', b'b', b'c', b'd'])
                .string_max(1)
                .is_err()
        );
        assert_eq!(Reader::new(&[2, b'h', b'i']).string().unwrap(), "hi");
        assert_eq!(Reader::new(&[2, b'a', 0xE4]).string().unwrap(), "a\u{FFFD}");
    }
}
