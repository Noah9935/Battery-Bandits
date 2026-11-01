use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};

/// Aktuelle Unix-Zeit in Millisekunden.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Unix-Millisekunden als Zeitstempel für die Datenbank.
pub fn ts_from_ms(ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or(DateTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let ms = 1_793_000_123_456;
        assert_eq!(ts_from_ms(ms).timestamp_millis(), ms);
    }

    #[test]
    fn now_is_plausible() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}
