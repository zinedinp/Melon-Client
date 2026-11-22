//! Base64 as `java.util.Base64.getDecoder()` reads it: the server icon and
//! the skin textures property.

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
    use super::decode;

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
