use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, WINDOWS_1252};

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16BE_BOM: [u8; 2] = [0xFE, 0xFF];

const SNIFF_LEN: usize = 512;
const SNIFF_MIN_LEN: usize = 4;
const SNIFF_ZERO_PERCENT: usize = 30;

pub fn decode(bytes: &[u8]) -> String {
    let mut text = decode_untrimmed(bytes);
    text.truncate(text.trim_end_matches('\0').len());
    text
}

fn decode_untrimmed(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
        return decode_with(UTF_16LE, rest);
    }
    if let Some(rest) = bytes.strip_prefix(&UTF16BE_BOM) {
        return decode_with(UTF_16BE, rest);
    }
    if let Some(encoding) = sniff_utf16(trim_trailing_zeros(bytes)) {
        return decode_with(encoding, bytes);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(error) if error.error_len().is_none() => String::from_utf8_lossy(bytes).into_owned(),
        Err(_) => decode_with(WINDOWS_1252, bytes),
    }
}

fn trim_trailing_zeros(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &bytes[..end]
}

fn decode_with(encoding: &'static Encoding, bytes: &[u8]) -> String {
    encoding.decode_without_bom_handling(bytes).0.into_owned()
}

fn sniff_utf16(bytes: &[u8]) -> Option<&'static Encoding> {
    if bytes.len() < SNIFF_MIN_LEN {
        return None;
    }
    let window = &bytes[..bytes.len().min(SNIFF_LEN)];
    let odd_total = window.len() / 2;
    let even_total = window.len() - odd_total;
    let odd_zeros = window
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let even_zeros = window.iter().step_by(2).filter(|&&b| b == 0).count();
    if odd_zeros * 100 >= odd_total * SNIFF_ZERO_PERCENT {
        Some(UTF_16LE)
    } else if even_zeros * 100 >= even_total * SNIFF_ZERO_PERCENT {
        Some(UTF_16BE)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str, little_endian: bool, bom: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if bom {
            out.extend_from_slice(if little_endian {
                &[0xFF, 0xFE]
            } else {
                &[0xFE, 0xFF]
            });
        }
        for unit in s.encode_utf16() {
            let b = if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            };
            out.extend_from_slice(&b);
        }
        out
    }

    #[test]
    fn decodes_plain_utf8() {
        assert_eq!(decode(b"[A]\nx=1"), "[A]\nx=1");
    }

    #[test]
    fn strips_utf8_bom() {
        assert_eq!(decode(b"\xEF\xBB\xBF[A]"), "[A]");
    }

    #[test]
    fn decodes_utf16le_with_bom() {
        assert_eq!(decode(&utf16("[A]\r\nk=é", true, true)), "[A]\r\nk=é");
    }

    #[test]
    fn decodes_utf16be_with_bom() {
        assert_eq!(decode(&utf16("[A]", false, true)), "[A]");
    }

    #[test]
    fn decodes_utf16le_without_bom() {
        assert_eq!(
            decode(&utf16("[ACH39]\r\nachieved=true", true, false)),
            "[ACH39]\r\nachieved=true"
        );
    }

    #[test]
    fn falls_back_to_windows_1252() {
        assert_eq!(decode(b"Conquista \xE9pica"), "Conquista épica");
    }

    #[test]
    fn empty_input_is_empty_string() {
        assert_eq!(decode(b""), "");
    }

    #[test]
    fn all_nul_input_decodes_to_empty() {
        assert_eq!(decode(&[0u8; 64]), "");
    }

    #[test]
    fn nul_padding_after_utf8_is_stripped() {
        let mut bytes = b"[A]\nk=v\n".to_vec();
        bytes.extend_from_slice(&[0u8; 200]);
        assert_eq!(decode(&bytes), "[A]\nk=v\n");
    }

    #[test]
    fn decodes_utf16be_without_bom() {
        assert_eq!(
            decode(&utf16("[ACH39]\r\nachieved=true", false, false)),
            "[ACH39]\r\nachieved=true"
        );
    }

    #[test]
    fn keeps_multibyte_utf8_without_bom() {
        assert_eq!(decode("café ÉPICO".as_bytes()), "café ÉPICO");
    }

    #[test]
    fn truncated_utf8_tail_stays_utf8() {
        assert!(decode(b"[\xC3\x89PICO]\n\xE2\x82").starts_with("[ÉPICO]\n"));
    }
}
