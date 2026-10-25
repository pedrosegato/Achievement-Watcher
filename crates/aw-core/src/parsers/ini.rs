use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ini {
    sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut sections: Vec<Section> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                sections.push(Section {
                    name: name.to_owned(),
                    entries: Vec::new(),
                });
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if sections.is_empty() {
                sections.push(Section::default());
            }
            if let Some(section) = sections.last_mut() {
                section.entries.push((
                    unquote(key.trim()).to_owned(),
                    unquote(value.trim()).to_owned(),
                ));
            }
        }
        Ini { sections }
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|section| section.name.eq_ignore_ascii_case(name))
    }
}

impl Section {
    pub fn lookup(&self) -> HashMap<String, &str> {
        let mut map = HashMap::with_capacity(self.entries.len());
        for (key, value) in &self.entries {
            map.entry(key.to_ascii_lowercase())
                .or_insert(value.as_str());
        }
        map
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_keeps_first_duplicate_and_ignores_case() {
        let ini = Ini::parse(
            "[S]
Abc=1
ABC=2
x=3
",
        );
        let map = ini.section("s").unwrap().lookup();
        assert_eq!(map.get("abc"), Some(&"1"));
        assert_eq!(map.get("x"), Some(&"3"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn finds_sections_and_keys_case_insensitively() {
        let ini = Ini::parse("[ACH39]\nAchieved=true\n");
        assert_eq!(ini.section("ach39").unwrap().get("ACHIEVED"), Some("true"));
    }

    #[test]
    fn keeps_section_order_and_original_names() {
        let ini = Ini::parse("[B]\n[a]\n");
        let names: Vec<&str> = ini.sections().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["B", "a"]);
    }

    #[test]
    fn strips_quotes_and_whitespace() {
        let ini = Ini::parse("[ACHIEVEMENTS]\n\"ACH_1\" = {unlocked = true, time = 5}\n");
        assert_eq!(
            ini.section("achievements").unwrap().get("ach_1"),
            Some("{unlocked = true, time = 5}")
        );
    }

    #[test]
    fn ignores_comments_blank_lines_and_lines_without_equals() {
        let ini = Ini::parse("; c\n# c\n\n[A]\njunk\nk=v\n");
        assert_eq!(ini.section("A").unwrap().entries.len(), 1);
    }

    #[test]
    fn handles_crlf() {
        let ini = Ini::parse("[A]\r\nk=v\r\n");
        assert_eq!(ini.section("A").unwrap().get("k"), Some("v"));
    }

    #[test]
    fn keys_before_first_section_go_to_unnamed_section() {
        let ini = Ini::parse("k=v\n[A]\n");
        assert_eq!(ini.section("").unwrap().get("k"), Some("v"));
    }

    #[test]
    fn first_duplicate_key_wins() {
        let ini = Ini::parse("[A]\nk=1\nk=2\n");
        assert_eq!(ini.section("A").unwrap().get("k"), Some("1"));
    }

    #[test]
    fn value_keeps_inner_equals() {
        let ini = Ini::parse("[A]\nk=a=b\n");
        assert_eq!(ini.section("A").unwrap().get("k"), Some("a=b"));
    }
}
