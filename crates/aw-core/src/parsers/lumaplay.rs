use crate::model::Unlock;

pub(crate) fn parse_lumaplay(text: &str) -> Vec<Unlock> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| {
            let unlocked = value.trim().parse::<u64>().is_ok_and(|n| n > 0);
            Unlock::new(name.trim(), unlocked, None)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_positive_values() {
        assert_eq!(
            parse_lumaplay("ACH_1=1\nACH_2=0\nACH_3=4294967296\nbroken\nACH_4=x\n"),
            vec![
                Unlock::new("ACH_1", true, None),
                Unlock::new("ACH_2", false, None),
                Unlock::new("ACH_3", true, None),
                Unlock::new("ACH_4", false, None)
            ]
        );
    }
}
