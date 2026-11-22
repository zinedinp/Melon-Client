//! Base64 as `java.util.Base64` reads and writes it: the server icon, the skin textures
//! property, and the sign-in service's metadata handed to authlib-injector.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The basic alphabet with padding, as `getEncoder()` writes it.
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let bits = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(bits >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The basic alphabet, padding optional. `None` where Java throws: another
/// character, a wrong ending unit or anything after the padding.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    let end = text.iter().position(|&b| b == b'=').unwrap_or(text.len());
    let (data, padding) = text.split_at(end);
    let valid_end = match (data.len() % 4, padding) {
        (1, _) => false,
        (_, []) | (2, b"==") | (3, b"=") => true,
        _ => false,
    };
    if !valid_end {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0u32);
    for &byte in data {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn encoding_pads_and_round_trips() {
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(b"Ma"), "TWE=");
        assert_eq!(encode(b"M"), "TQ==");
        assert_eq!(encode(b""), "");
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&all)).unwrap(), all);
    }

    #[test]
    fn padding_is_optional_but_must_fit() {
        assert_eq!(decode("TWFu").unwrap(), b"Man");
        assert_eq!(decode("TWE=").unwrap(), b"Ma");
        assert_eq!(decode("TWE").unwrap(), b"Ma");
        assert_eq!(decode("TQ==").unwrap(), b"M");
        assert_eq!(decode("TQ").unwrap(), b"M");
        assert_eq!(decode("TWVsb24=").unwrap(), b"Melon");
        assert_eq!(decode("").unwrap(), b"");
        assert!(decode("TQ=").is_none());
        assert!(decode("TWE==").is_none());
        assert!(decode("TWFuT").is_none());
        assert!(decode("TWE=TWE=").is_none());
    }

    #[test]
    fn only_the_basic_alphabet_is_read() {
        assert!(decode("T!E").is_none());
        assert!(decode("TW-u").is_none());
        assert!(decode("TW_u").is_none());
        assert!(decode("TWFu\n").is_none());
        assert!(decode("TW Fu").is_none());
    }
}
