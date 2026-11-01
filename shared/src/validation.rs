//! Prüfung eingehender MQTT-Nachrichten (F06).
//!
//! Format und Topic-Struktur der TrafficNodes sind noch nicht mit dem
//! Auftraggeber geklärt (README Frage 1). Dieses Modul kapselt die Annahme:
//!
//! * Topic: `trafficnode/<geraete-id>/battery`
//! * Nutzlast: JSON `{"device_id": "TN-0001", "timestamp": "2026-11-01T12:00:00Z", "charge_pct": 87.5}`
//!
//! `device_id` und `timestamp` sind optional. Fehlt die Geräte-ID, gilt die aus
//! dem Topic; fehlt der Zeitstempel, gilt der Empfangszeitpunkt. Stehen
//! Topic und Nutzlast im Widerspruch, wird die Nachricht verworfen.

use chrono::DateTime;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::Reading;

const MAX_DEVICE_ID_LEN: usize = 64;

/// Grenzen für plausible Messwerte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    /// Wie weit ein Zeitstempel in der Zukunft liegen darf
    pub max_future_ms: i64,
    /// Wie alt ein Zeitstempel höchstens sein darf
    pub max_age_ms: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            max_future_ms: 5 * 60 * 1000,
            max_age_ms: 365 * 24 * 3600 * 1000,
        }
    }
}

/// Grund, warum eine Nachricht verworfen wird. Der Text landet im Protokoll.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Rejection {
    #[error("Nutzlast ist kein gültiges UTF-8")]
    NotUtf8,
    #[error("Nutzlast ist kein gültiges JSON: {0}")]
    InvalidJson(String),
    #[error("Nutzlast ist kein JSON-Objekt")]
    NotAnObject,
    #[error("Topic enthält keine Geräte-ID")]
    NoDeviceInTopic,
    #[error("Geräte-ID ist ungültig")]
    InvalidDeviceId,
    #[error("Geräte-ID in der Nutzlast ({payload}) passt nicht zum Topic ({topic})")]
    DeviceMismatch { payload: String, topic: String },
    #[error("Ladestand fehlt")]
    MissingCharge,
    #[error("Ladestand ist keine Zahl")]
    ChargeNotANumber,
    #[error("Ladestand außerhalb von 0 bis 100 Prozent: {0}")]
    ChargeOutOfRange(f64),
    #[error("Zeitstempel ist ungültig")]
    InvalidTimestamp,
    #[error("Zeitstempel liegt zu weit in der Zukunft")]
    TimestampInFuture,
    #[error("Zeitstempel ist zu alt")]
    TimestampTooOld,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    device_id: Option<String>,
    timestamp: Option<Value>,
    #[serde(alias = "charge", alias = "soc")]
    charge_pct: Option<Value>,
}

/// Geräte-ID aus dem Topic `trafficnode/<id>/battery`.
pub fn device_id_from_topic(topic: &str) -> Option<&str> {
    let mut parts = topic.split('/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("trafficnode"), Some(id), Some("battery"), None) if !id.is_empty() => Some(id),
        _ => None,
    }
}

/// Erlaubt sind Buchstaben, Ziffern, Bindestrich, Unterstrich und Punkt.
pub fn is_valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_DEVICE_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn parse_timestamp(value: &Value) -> Result<i64, Rejection> {
    match value {
        Value::Number(n) => n.as_i64().ok_or(Rejection::InvalidTimestamp),
        Value::String(s) => DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.timestamp_millis())
            .map_err(|_| Rejection::InvalidTimestamp),
        _ => Err(Rejection::InvalidTimestamp),
    }
}

/// Prüft eine Nachricht und macht daraus einen Messwert.
///
/// `now_ms` ist der Empfangszeitpunkt in Unix-Millisekunden und wird von außen
/// übergeben, damit die Funktion rein bleibt.
pub fn parse_message(topic: &str, payload: &[u8], now_ms: i64, rules: &Rules) -> Result<Reading, Rejection> {
    let text = std::str::from_utf8(payload).map_err(|_| Rejection::NotUtf8)?;
    let json: Value = serde_json::from_str(text).map_err(|e| Rejection::InvalidJson(e.to_string()))?;
    if !json.is_object() {
        return Err(Rejection::NotAnObject);
    }
    let raw: RawMessage = serde_json::from_value(json).map_err(|e| Rejection::InvalidJson(e.to_string()))?;

    let topic_id = device_id_from_topic(topic);
    let device_id = match (raw.device_id.as_deref(), topic_id) {
        (Some(p), Some(t)) if p != t => {
            return Err(Rejection::DeviceMismatch {
                payload: p.to_owned(),
                topic: t.to_owned(),
            });
        }
        (Some(p), _) => p,
        (None, Some(t)) => t,
        (None, None) => return Err(Rejection::NoDeviceInTopic),
    };
    if !is_valid_device_id(device_id) {
        return Err(Rejection::InvalidDeviceId);
    }

    let charge_pct = match raw.charge_pct {
        None | Some(Value::Null) => return Err(Rejection::MissingCharge),
        Some(Value::Number(n)) => n.as_f64().ok_or(Rejection::ChargeNotANumber)?,
        Some(_) => return Err(Rejection::ChargeNotANumber),
    };
    if !charge_pct.is_finite() || !(0.0..=100.0).contains(&charge_pct) {
        return Err(Rejection::ChargeOutOfRange(charge_pct));
    }

    let timestamp_ms = match raw.timestamp {
        None | Some(Value::Null) => now_ms,
        Some(v) => parse_timestamp(&v)?,
    };
    if timestamp_ms > now_ms + rules.max_future_ms {
        return Err(Rejection::TimestampInFuture);
    }
    if timestamp_ms < now_ms - rules.max_age_ms {
        return Err(Rejection::TimestampTooOld);
    }

    Ok(Reading {
        device_id: device_id.to_owned(),
        timestamp_ms,
        charge_pct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1. November 2026, 05:46 Uhr UTC
    const NOW: i64 = 1_793_500_000_000;
    const TOPIC: &str = "trafficnode/TN-0001/battery";

    fn parse(topic: &str, payload: &str) -> Result<Reading, Rejection> {
        parse_message(topic, payload.as_bytes(), NOW, &Rules::default())
    }

    #[test]
    fn accepts_complete_message() {
        let r = parse(
            TOPIC,
            r#"{"device_id":"TN-0001","timestamp":1792999999000,"charge_pct":87.5}"#,
        )
        .unwrap();
        assert_eq!(r.device_id, "TN-0001");
        assert_eq!(r.timestamp_ms, 1_792_999_999_000);
        assert_eq!(r.charge_pct, 87.5);
    }

    #[test]
    fn accepts_rfc3339_timestamp() {
        let r = parse(TOPIC, r#"{"timestamp":"2026-10-31T12:00:00Z","charge_pct":50}"#).unwrap();
        assert_eq!(r.timestamp_ms, 1_793_448_000_000);
    }

    #[test]
    fn device_and_timestamp_are_optional() {
        let r = parse(TOPIC, r#"{"charge_pct":42}"#).unwrap();
        assert_eq!(r.device_id, "TN-0001");
        assert_eq!(r.timestamp_ms, NOW);
    }

    #[test]
    fn payload_device_works_without_topic_id() {
        let r = parse("irgendwas/anderes", r#"{"device_id":"TN-9","charge_pct":10}"#).unwrap();
        assert_eq!(r.device_id, "TN-9");
    }

    #[test]
    fn accepts_charge_aliases() {
        assert_eq!(parse(TOPIC, r#"{"charge":60}"#).unwrap().charge_pct, 60.0);
        assert_eq!(parse(TOPIC, r#"{"soc":61}"#).unwrap().charge_pct, 61.0);
    }

    #[test]
    fn accepts_boundary_charges() {
        assert!(parse(TOPIC, r#"{"charge_pct":0}"#).is_ok());
        assert!(parse(TOPIC, r#"{"charge_pct":100}"#).is_ok());
    }

    #[test]
    fn rejects_non_utf8() {
        let r = parse_message(TOPIC, &[0xff, 0xfe], NOW, &Rules::default());
        assert_eq!(r, Err(Rejection::NotUtf8));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(parse(TOPIC, "{kaputt"), Err(Rejection::InvalidJson(_))));
        assert!(matches!(parse(TOPIC, ""), Err(Rejection::InvalidJson(_))));
    }

    #[test]
    fn rejects_non_objects() {
        assert_eq!(parse(TOPIC, "[1,2]"), Err(Rejection::NotAnObject));
        assert_eq!(parse(TOPIC, "42"), Err(Rejection::NotAnObject));
    }

    #[test]
    fn rejects_missing_or_wrong_charge() {
        assert_eq!(parse(TOPIC, "{}"), Err(Rejection::MissingCharge));
        assert_eq!(parse(TOPIC, r#"{"charge_pct":null}"#), Err(Rejection::MissingCharge));
        assert_eq!(
            parse(TOPIC, r#"{"charge_pct":"viel"}"#),
            Err(Rejection::ChargeNotANumber)
        );
    }

    #[test]
    fn rejects_charge_out_of_range() {
        assert!(matches!(
            parse(TOPIC, r#"{"charge_pct":100.1}"#),
            Err(Rejection::ChargeOutOfRange(_))
        ));
        assert!(matches!(
            parse(TOPIC, r#"{"charge_pct":-1}"#),
            Err(Rejection::ChargeOutOfRange(_))
        ));
    }

    #[test]
    fn rejects_unknown_device() {
        assert_eq!(
            parse("foo/bar", r#"{"charge_pct":50}"#),
            Err(Rejection::NoDeviceInTopic)
        );
        assert_eq!(
            parse("trafficnode//battery", r#"{"charge_pct":50}"#),
            Err(Rejection::NoDeviceInTopic)
        );
    }

    #[test]
    fn rejects_invalid_device_id() {
        assert_eq!(
            parse("trafficnode/a b/battery", r#"{"charge_pct":50}"#),
            Err(Rejection::InvalidDeviceId)
        );
        let long = "x".repeat(65);
        assert_eq!(
            parse("ohne/topic", &format!(r#"{{"device_id":"{long}","charge_pct":50}}"#)),
            Err(Rejection::InvalidDeviceId)
        );
    }

    #[test]
    fn rejects_device_mismatch() {
        let r = parse(TOPIC, r#"{"device_id":"TN-0002","charge_pct":50}"#);
        assert!(matches!(r, Err(Rejection::DeviceMismatch { .. })));
    }

    #[test]
    fn rejects_bad_timestamps() {
        assert_eq!(
            parse(TOPIC, r#"{"timestamp":"gestern","charge_pct":50}"#),
            Err(Rejection::InvalidTimestamp)
        );
        assert_eq!(
            parse(TOPIC, r#"{"timestamp":true,"charge_pct":50}"#),
            Err(Rejection::InvalidTimestamp)
        );
        assert_eq!(
            parse(TOPIC, r#"{"timestamp":1.5,"charge_pct":50}"#),
            Err(Rejection::InvalidTimestamp)
        );
    }

    #[test]
    fn rejects_future_and_old_timestamps() {
        let future = NOW + 10 * 60 * 1000;
        assert_eq!(
            parse(TOPIC, &format!(r#"{{"timestamp":{future},"charge_pct":50}}"#)),
            Err(Rejection::TimestampInFuture)
        );
        let slightly_ahead = NOW + 60 * 1000;
        assert!(parse(TOPIC, &format!(r#"{{"timestamp":{slightly_ahead},"charge_pct":50}}"#)).is_ok());
        let old = NOW - 400 * 24 * 3600 * 1000;
        assert_eq!(
            parse(TOPIC, &format!(r#"{{"timestamp":{old},"charge_pct":50}}"#)),
            Err(Rejection::TimestampTooOld)
        );
    }

    #[test]
    fn topic_parsing() {
        assert_eq!(device_id_from_topic("trafficnode/TN-1/battery"), Some("TN-1"));
        assert_eq!(device_id_from_topic("trafficnode/TN-1/battery/extra"), None);
        assert_eq!(device_id_from_topic("trafficnode/TN-1"), None);
        assert_eq!(device_id_from_topic("other/TN-1/battery"), None);
    }

    #[test]
    fn rejection_text_is_readable() {
        assert_eq!(
            Rejection::ChargeOutOfRange(120.0).to_string(),
            "Ladestand außerhalb von 0 bis 100 Prozent: 120"
        );
    }
}
