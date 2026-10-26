#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KvValue {
    Str(String),
    Obj(Vec<(String, KvValue)>),
}

impl KvValue {
    pub fn get(&self, key: &str) -> Option<&KvValue> {
        self.entries()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            KvValue::Str(s) => Some(s),
            KvValue::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, KvValue)] {
        match self {
            KvValue::Obj(entries) => entries,
            KvValue::Str(_) => &[],
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VdfError {
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("unexpected token at byte {0}")]
    Unexpected(usize),
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Text(String),
    Open,
    Close,
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Lexer<'a> {
        Lexer { text, pos: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn skip_trivia(&mut self) {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            if trimmed.starts_with("//") {
                let line_end = trimmed.find('\n').unwrap_or(trimmed.len());
                self.pos += line_end;
            } else {
                return;
            }
        }
    }

    fn next(&mut self) -> Option<(usize, Token)> {
        self.skip_trivia();
        let start = self.pos;
        let mut chars = self.rest().chars();
        let first = chars.next()?;
        match first {
            '{' => {
                self.pos += 1;
                Some((start, Token::Open))
            }
            '}' => {
                self.pos += 1;
                Some((start, Token::Close))
            }
            '"' => Some((start, Token::Text(self.quoted()))),
            _ => Some((start, Token::Text(self.bare()))),
        }
    }

    fn quoted(&mut self) -> String {
        self.pos += 1;
        let mut out = String::new();
        let mut chars = self.rest().char_indices();
        while let Some((offset, c)) = chars.next() {
            match c {
                '"' => {
                    self.pos += offset + 1;
                    return out;
                }
                '\\' => match chars.next() {
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 't')) => out.push('\t'),
                    Some((_, other)) => out.push(other),
                    None => break,
                },
                other => out.push(other),
            }
        }
        self.pos = self.text.len();
        out
    }

    fn bare(&mut self) -> String {
        let rest = self.rest();
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '{' | '}'))
            .unwrap_or(rest.len());
        self.pos += end;
        rest[..end].to_owned()
    }

    fn skip_conditional(&mut self) {
        self.skip_trivia();
        let rest = self.rest();
        if rest.starts_with('[') {
            let end = rest.find(']').map_or(rest.len(), |i| i + 1);
            self.pos += end;
        }
    }
}

pub fn parse(text: &str) -> Result<KvValue, VdfError> {
    let mut lexer = Lexer::new(text);
    let entries = parse_pairs(&mut lexer, false)?;
    Ok(KvValue::Obj(entries))
}

fn parse_pairs(lexer: &mut Lexer, nested: bool) -> Result<Vec<(String, KvValue)>, VdfError> {
    let mut entries = Vec::new();
    loop {
        let Some((at, token)) = lexer.next() else {
            return if nested {
                Err(VdfError::UnexpectedEnd)
            } else {
                Ok(entries)
            };
        };
        let key = match token {
            Token::Text(key) => key,
            Token::Close if nested => return Ok(entries),
            Token::Close | Token::Open => return Err(VdfError::Unexpected(at)),
        };
        let value = match lexer.next() {
            None => return Err(VdfError::UnexpectedEnd),
            Some((_, Token::Text(value))) => {
                lexer.skip_conditional();
                KvValue::Str(value)
            }
            Some((_, Token::Open)) => KvValue::Obj(parse_pairs(lexer, true)?),
            Some((at, Token::Close)) => return Err(VdfError::Unexpected(at)),
        };
        entries.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_objects_and_unescapes_backslashes() {
        let text = r#""libraryfolders"
{
    "0"
    {
        "path"      "C:\\Program Files (x86)\\Steam"
        "apps"
        {
            "10"        "318799907"
        }
    }
}"#;
        let kv = parse(text).unwrap();
        let lib0 = kv.get("libraryfolders").unwrap().get("0").unwrap();
        assert_eq!(
            lib0.get("path").and_then(KvValue::as_str),
            Some(r"C:\Program Files (x86)\Steam")
        );
        assert_eq!(
            lib0.get("apps")
                .unwrap()
                .get("10")
                .and_then(KvValue::as_str),
            Some("318799907")
        );
    }

    #[test]
    fn get_is_case_insensitive_and_first_duplicate_wins() {
        let kv = parse(r#""AppState" { "appid" "1" "AppID" "2" }"#).unwrap();
        let app = kv.get("appstate").unwrap();
        assert_eq!(app.get("APPID").and_then(KvValue::as_str), Some("1"));
        assert_eq!(app.entries().len(), 2);
    }

    #[test]
    fn accepts_unquoted_tokens_comments_and_conditionals() {
        let kv = parse("// header\nroot { key value [$WIN32] \"q\" \"v\" // tail\n }").unwrap();
        let root = kv.get("root").unwrap();
        assert_eq!(root.get("key").and_then(KvValue::as_str), Some("value"));
        assert_eq!(root.get("q").and_then(KvValue::as_str), Some("v"));
    }

    #[test]
    fn decodes_quote_and_whitespace_escapes() {
        let kv = parse(r#""k" "a\"b\tc\nd""#).unwrap();
        assert_eq!(kv.get("k").and_then(KvValue::as_str), Some("a\"b\tc\nd"));
    }

    #[test]
    fn unterminated_object_is_an_error() {
        assert_eq!(parse(r#""a" { "b" "c""#), Err(VdfError::UnexpectedEnd));
    }

    #[test]
    fn stray_closing_brace_is_an_error() {
        assert!(matches!(
            parse(r#""a" "b" }"#),
            Err(VdfError::Unexpected(_))
        ));
    }
}
