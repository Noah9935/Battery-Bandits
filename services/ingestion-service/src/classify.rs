//! Prüfen einer Roh-Nachricht (F06): gültig oder verwerfen und protokollieren.

use shared::{TelemetryPayload, node_id_from_topic};

/// Eine gültige Messung mit Gerätekennung aus dem Topic.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidReading {
    pub node_id: String,
    pub payload: TelemetryPayload,
}

/// Eine verworfene Nachricht mit Grund.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidMessage {
    pub topic: String,
    pub payload: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Valid(ValidReading),
    Invalid(InvalidMessage),
}

/// Längste Nutzlast, die wir im Protokoll aufbewahren.
const MAX_LOGGED_PAYLOAD: usize = 2000;

pub fn classify(topic: &str, payload: &[u8]) -> Incoming {
    let invalid = |reason: String| {
        let text = String::from_utf8_lossy(payload);
        Incoming::Invalid(InvalidMessage {
            topic: topic.to_owned(),
            payload: text.chars().take(MAX_LOGGED_PAYLOAD).collect(),
            reason,
        })
    };
    let Some(node_id) = node_id_from_topic(topic) else {
        return invalid("Topic ohne Gerätekennung".to_owned());
    };
    let parsed: TelemetryPayload = match serde_json::from_slice(payload) {
        Ok(p) => p,
        Err(e) => return invalid(format!("Kein gültiges Nachrichtenformat: {e}")),
    };
    match parsed.validate() {
        Ok(()) => Incoming::Valid(ValidReading {
            node_id: node_id.to_owned(),
            payload: parsed,
        }),
        Err(e) => invalid(format!("Ungültige Werte: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPIC: &str = "traffic-nodes/TN0001/measurements";
    const SAMPLE: &str = r#"{"current_mA":-311.25,"voltage_mV":12296.25,"lat":null,"lon":null,
        "altitude_m":null,"satellites":null,"time":null,"temperature_C":16.44}"#;

    #[test]
    fn accepts_the_lecture_sample() {
        let Incoming::Valid(v) = classify(TOPIC, SAMPLE.as_bytes()) else {
            panic!("sollte gültig sein")
        };
        assert_eq!(v.node_id, "TN0001");
        assert_eq!(v.payload.voltage_mv, 12_296);
    }

    #[test]
    fn rejects_topic_without_node() {
        let Incoming::Invalid(m) = classify("traffic-nodes/measurements", SAMPLE.as_bytes()) else {
            panic!()
        };
        assert!(m.reason.contains("Gerätekennung"));
    }

    #[test]
    fn rejects_broken_json_and_keeps_payload_for_the_log() {
        let Incoming::Invalid(m) = classify(TOPIC, b"{kaputt") else {
            panic!()
        };
        assert_eq!(m.payload, "{kaputt");
        assert!(m.reason.contains("Nachrichtenformat"));
    }

    #[test]
    fn rejects_out_of_range_values() {
        let json = r#"{"current_mA":1,"voltage_mV":0,"temperature_C":20}"#;
        let Incoming::Invalid(m) = classify(TOPIC, json.as_bytes()) else {
            panic!()
        };
        assert!(m.reason.contains("Ungültige Werte"));
    }

    #[test]
    fn handles_non_utf8_payload() {
        let Incoming::Invalid(m) = classify(TOPIC, &[0xff, 0xfe, 0x00]) else {
            panic!()
        };
        assert!(!m.payload.is_empty());
    }

    #[test]
    fn truncates_huge_payloads_in_the_log() {
        let big = vec![b'x'; 10_000];
        let Incoming::Invalid(m) = classify(TOPIC, &big) else {
            panic!()
        };
        assert_eq!(m.payload.len(), MAX_LOGGED_PAYLOAD);
    }
}
