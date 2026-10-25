use super::ParseError;
use super::vdf::KvValue;

pub fn parse_binary_kv(bytes: &[u8]) -> Result<KvValue, ParseError> {
    let mut reader = Reader { bytes, pos: 0 };
    Ok(KvValue::Obj(reader.object(false)?))
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn invalid(&self, what: &str) -> ParseError {
        ParseError::Invalid(format!("binary keyvalues: {what} at byte {}", self.pos))
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], ParseError> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + N)
            .ok_or_else(|| self.invalid("truncated value"))?;
        self.pos += N;
        slice
            .try_into()
            .map_err(|_| self.invalid("truncated value"))
    }

    fn cstring(&mut self) -> Result<String, ParseError> {
        let rest = &self.bytes[self.pos..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| self.invalid("unterminated string"))?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(text)
    }

    fn object(&mut self, nested: bool) -> Result<Vec<(String, KvValue)>, ParseError> {
        let mut entries = Vec::new();
        loop {
            let Some(kind) = self.byte() else {
                return if nested {
                    Err(self.invalid("unexpected end"))
                } else {
                    Ok(entries)
                };
            };
            if kind == 0x08 || kind == 0x0B {
                return Ok(entries);
            }
            let key = self.cstring()?;
            let value = match kind {
                0x00 => KvValue::Obj(self.object(true)?),
                0x01 => KvValue::Str(self.cstring()?),
                0x02 | 0x04 | 0x06 => {
                    KvValue::Str(i32::from_le_bytes(self.take::<4>()?).to_string())
                }
                0x03 => KvValue::Str(f32::from_le_bytes(self.take::<4>()?).to_string()),
                0x07 => KvValue::Str(u64::from_le_bytes(self.take::<8>()?).to_string()),
                0x0A => KvValue::Str(i64::from_le_bytes(self.take::<8>()?).to_string()),
                other => return Err(self.invalid(&format!("unknown type {other:#04x}"))),
            };
            entries.push((key, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Vec<u8> {
        let mut v = text.as_bytes().to_vec();
        v.push(0);
        v
    }

    #[test]
    fn parses_binary_kv() {
        let mut b = vec![0x00];
        b.extend(s("cache"));
        b.push(0x02);
        b.extend(s("crc"));
        b.extend(7i32.to_le_bytes());
        b.push(0x00);
        b.extend(s("1"));
        b.push(0x02);
        b.extend(s("data"));
        b.extend((-2_147_483_645i32).to_le_bytes());
        b.push(0x01);
        b.extend(s("name"));
        b.extend(s("TheFool"));
        b.extend([0x08, 0x08, 0x08]);
        let kv = parse_binary_kv(&b).unwrap();
        let cache = kv.get("cache").unwrap();
        assert_eq!(cache.get("crc").and_then(KvValue::as_str), Some("7"));
        let stat = cache.get("1").unwrap();
        assert_eq!(
            stat.get("data").and_then(KvValue::as_str),
            Some("-2147483645")
        );
        assert_eq!(stat.get("name").and_then(KvValue::as_str), Some("TheFool"));
    }

    #[test]
    fn truncated_input_is_an_error_not_a_panic() {
        assert!(parse_binary_kv(&[0x00, b'a']).is_err());
        assert!(parse_binary_kv(&[0x02, b'a', 0, 1, 2]).is_err());
        assert!(parse_binary_kv(&[0x00, b'a', 0]).is_err());
        assert!(parse_binary_kv(&[0x7f, b'a', 0]).is_err());
    }
}
