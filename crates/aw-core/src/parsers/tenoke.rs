use super::unlock_time;
use crate::model::Unlock;
use crate::parsers::ini::Ini;

pub(crate) fn parse_tenoke(text: &str) -> Vec<Unlock> {
    let ini = Ini::parse(text);
    let Some(section) = ini.section("ACHIEVEMENTS") else {
        return Vec::new();
    };
    section
        .entries
        .iter()
        .map(|(name, value)| {
            let fields = fields(value);
            let unlocked =
                field(&fields, "unlocked").is_some_and(|v| v.eq_ignore_ascii_case("true"));
            let time = field(&fields, "time").and_then(unlock_time);
            Unlock::new(name.clone(), unlocked, time)
        })
        .collect()
}

fn fields(value: &str) -> Vec<(&str, &str)> {
    let inner = value.trim();
    let inner = inner.strip_prefix('{').unwrap_or(inner);
    let inner = inner.strip_suffix('}').unwrap_or(inner);
    inner
        .split(',')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((key.trim(), value.trim()))
        })
        .collect()
}

fn field<'a>(fields: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_names_and_brace_values() {
        let u = parse_tenoke(
            "[ACHIEVEMENTS]\n\"A1\" = {unlocked = true, time = 1712253396}\n\"A2\" = {unlocked = false, time = 0}\n",
        );
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_712_253_396)),
                Unlock::new("A2", false, None)
            ]
        );
    }

    #[test]
    fn tolerates_compact_spacing_and_case() {
        let u = parse_tenoke("[achievements]\n\"A1\"={Unlocked=TRUE,Time=5}\n");
        assert_eq!(u, vec![Unlock::new("A1", true, Some(5))]);
    }

    #[test]
    fn unlocked_without_time_has_no_time() {
        assert_eq!(
            parse_tenoke("[ACHIEVEMENTS]\n\"A1\" = {unlocked = true}\n"),
            vec![Unlock::new("A1", true, None)]
        );
    }

    #[test]
    fn only_the_exact_time_field_counts() {
        assert_eq!(
            parse_tenoke("[ACHIEVEMENTS]\n\"A1\" = {unlocked = true, lasttime = 99}\n"),
            vec![Unlock::new("A1", true, None)]
        );
    }

    #[test]
    fn missing_section_is_empty() {
        assert!(parse_tenoke("[OTHER]\n\"A1\" = {unlocked = true}\n").is_empty());
    }
}
