use super::{ParseError, epoch_time};
use crate::model::Unlock;

pub const CRC_PREFIX: &str = "#crc32:";
const ENTRY_LEN: usize = 24;

pub fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(u32::MAX, |crc, &byte| {
        (0..8).fold(crc ^ u32::from(byte), |c, _| {
            if c & 1 == 1 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            }
        })
    })
}

pub fn crc_name(crc: u32) -> String {
    format!("{CRC_PREFIX}{crc:08x}")
}

pub(crate) fn parse_smartsteam(bytes: &[u8]) -> Result<Vec<Unlock>, ParseError> {
    let header: [u8; 4] = bytes
        .get(..4)
        .and_then(|h| h.try_into().ok())
        .ok_or(ParseError::Empty)?;
    let count = u32::from_le_bytes(header) as usize;
    let body = &bytes[4..];
    if count.checked_mul(ENTRY_LEN) != Some(body.len()) {
        return Err(ParseError::Invalid(format!(
            "stats.bin declares {count} entries but has {} bytes",
            body.len()
        )));
    }
    Ok(body
        .chunks_exact(ENTRY_LEN)
        .map(|entry| {
            let crc = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]);
            let time = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
            let unlocked = entry[20] == 1;
            Unlock::new(
                crc_name(crc),
                unlocked,
                epoch_time(i64::from(time)).filter(|_| unlocked),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, time: u32, value: u8) -> Vec<u8> {
        let mut e = vec![0u8; ENTRY_LEN];
        e[0..4].copy_from_slice(&crc32(name.as_bytes()).to_le_bytes());
        e[8..12].copy_from_slice(&time.to_le_bytes());
        e[20] = value;
        e
    }

    #[test]
    fn crc32_matches_reference() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn reads_entries_by_crc() {
        let mut bytes = 2u32.to_le_bytes().to_vec();
        bytes.extend(entry("ACH_A", 1_700_000_000, 1));
        bytes.extend(entry("ACH_B", 0, 0));
        let u = parse_smartsteam(&bytes).unwrap();
        assert_eq!(
            u[0],
            Unlock::new(crc_name(crc32(b"ACH_A")), true, Some(1_700_000_000))
        );
        assert_eq!(u[1], Unlock::new(crc_name(crc32(b"ACH_B")), false, None));
    }

    #[test]
    fn rejects_count_mismatch_and_short_files() {
        let mut bytes = 3u32.to_le_bytes().to_vec();
        bytes.extend(entry("A", 1, 1));
        assert!(matches!(
            parse_smartsteam(&bytes),
            Err(ParseError::Invalid(_))
        ));
        assert!(matches!(parse_smartsteam(&[1, 2]), Err(ParseError::Empty)));
    }
}
