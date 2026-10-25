use super::unlock_time;
use crate::model::Unlock;

pub(crate) fn parse_razor(text: &str) -> Vec<Unlock> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let state = parts.next()?;
            let unlocked = state == "1";
            let time = parts.next().and_then(unlock_time);
            Some(Unlock::new(name, unlocked, time.filter(|_| unlocked)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_state_time_lines() {
        assert_eq!(
            parse_razor("ACH_1 1 1700000000\r\nACH_2 0 0\n\nbroken\nACH_3 1\n"),
            vec![
                Unlock::new("ACH_1", true, Some(1_700_000_000)),
                Unlock::new("ACH_2", false, None),
                Unlock::new("ACH_3", true, None),
            ]
        );
    }
}
