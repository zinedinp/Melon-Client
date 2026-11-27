//! VarInt wire format: 7 data bits per byte, high bit = "more bytes follow".

use std::io::{self, Read, Write};

pub fn write_var_int(w: &mut impl Write, value: i32) -> io::Result<()> {
    let mut v = value as u32;
    loop {
        if v & !0x7Fu32 == 0 {
            w.write_all(&[v as u8])?;
            return Ok(());
        }
        w.write_all(&[((v & 0x7F) | 0x80) as u8])?;
        v >>= 7;
    }
}

pub fn read_var_int(r: &mut impl Read) -> io::Result<i32> {
    let mut value: i32 = 0;
    let mut shift = 0u32;
    loop {
        let mut byte = [0u8; 1];
        r.read_exact(&mut byte)?;
        let byte = byte[0];
        value |= i32::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 32 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "VarInt too big"));
        }
    }
}
