use serde::{Deserialize, Serialize};

const MIN_UNLOCK_TIME: i64 = 946_684_800;
const MAX_FUTURE_SKEW: i64 = 300;
const MICROS_THRESHOLD: i64 = 100_000_000_000_000;
const MILLIS_THRESHOLD: i64 = 100_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub current: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unlock {
    pub api_name: String,
    pub unlocked: bool,
    pub unlocked_at: Option<i64>,
    pub progress: Option<Progress>,
}

impl Unlock {
    pub fn new(api_name: impl Into<String>, unlocked: bool, unlocked_at: Option<i64>) -> Unlock {
        Unlock {
            api_name: api_name.into(),
            unlocked,
            unlocked_at,
            progress: None,
        }
    }
}

pub fn normalize_epoch(raw: i64) -> i64 {
    if raw > MICROS_THRESHOLD {
        raw / 1_000_000
    } else if raw > MILLIS_THRESHOLD {
        raw / 1_000
    } else {
        raw
    }
}

pub fn sane_unlock_time(raw: i64, now: i64) -> Option<i64> {
    let secs = normalize_epoch(raw);
    (MIN_UNLOCK_TIME..=now.saturating_add(MAX_FUTURE_SKEW))
        .contains(&secs)
        .then_some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn normalize_epoch_keeps_seconds() {
        assert_eq!(normalize_epoch(1_761_321_600), 1_761_321_600);
    }

    #[test]
    fn normalize_epoch_converts_millis() {
        assert_eq!(normalize_epoch(1_761_321_600_123), 1_761_321_600);
    }

    #[test]
    fn normalize_epoch_converts_micros() {
        assert_eq!(normalize_epoch(1_761_321_600_123_456), 1_761_321_600);
    }

    #[test]
    fn sane_time_rejects_zero_and_pre_2000() {
        assert_eq!(sane_unlock_time(0, NOW), None);
        assert_eq!(sane_unlock_time(946_684_799, NOW), None);
    }

    #[test]
    fn sane_time_accepts_small_future_skew_only() {
        assert_eq!(sane_unlock_time(NOW + 300, NOW), Some(NOW + 300));
        assert_eq!(sane_unlock_time(NOW + 301, NOW), None);
    }

    #[test]
    fn sane_time_normalizes_millis_first() {
        assert_eq!(
            sane_unlock_time(1_761_321_600_000, NOW),
            Some(1_761_321_600)
        );
    }

    #[test]
    fn new_unlock_has_no_progress() {
        let u = Unlock::new("ACH39", true, Some(5));
        assert_eq!(u.progress, None);
        assert_eq!(u.api_name, "ACH39");
    }
}
