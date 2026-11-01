//! Ereignisse und Befehle, die die Dienste über NATS austauschen.
//!
//! Die Namen folgen dem Event Storming: Ereignisse beschreiben, was passiert
//! ist (Vergangenheit), Befehle sind Anfragen mit Antwort (Request/Reply).

use serde::{Deserialize, Serialize};

use crate::Status;

/// Subjects im JetStream-Stream `EVENTS`.
pub mod subject {
    pub const STREAM: &str = "EVENTS";
    /// Filter für den Stream
    pub const ALL_EVENTS: &str = "events.>";

    pub const READING_STORED: &str = "events.reading.stored";
    pub const DEVICE_CRITICAL: &str = "events.device.critical";
    pub const DEVICE_RECOVERED: &str = "events.device.recovered";
    pub const THRESHOLD_CHANGED: &str = "events.settings.threshold_changed";
    pub const INCIDENT_OPENED: &str = "events.incident.opened";
    pub const INCIDENT_DISMISSED: &str = "events.incident.dismissed";
    pub const INCIDENT_RESOLVED: &str = "events.incident.resolved";

    /// Befehle (Core NATS, Request/Reply)
    pub const CMD_SET_THRESHOLD: &str = "cmd.monitor.set_threshold";
    pub const CMD_DISMISS_INCIDENT: &str = "cmd.incidents.dismiss";
}

/// Erfassungs-Service: Eine Nachricht wurde geprüft und als Messwert gespeichert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingStored {
    pub device_id: String,
    pub timestamp_ms: i64,
    pub charge_pct: f64,
}

/// Überwachungs-Service: Ein Gerät ist unter den Schwellwert gefallen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceCritical {
    pub device_id: String,
    pub charge_pct: f64,
    pub threshold_pct: f64,
    pub at_ms: i64,
}

/// Überwachungs-Service: Ein kritisches Gerät ist wieder im grünen Bereich.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceRecovered {
    pub device_id: String,
    pub charge_pct: f64,
    pub at_ms: i64,
}

/// Überwachungs-Service: Der Schwellwert wurde geändert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdChanged {
    pub critical_pct: f64,
}

/// Meldungs-Service: Eine Meldung wurde eröffnet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncidentOpened {
    pub id: i64,
    pub device_id: String,
    pub opened_at_ms: i64,
    pub charge_pct: f64,
}

/// Meldungs-Service: Eine Meldung wurde abgewiesen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncidentDismissed {
    pub id: i64,
    pub device_id: String,
}

/// Meldungs-Service: Eine Meldung ist erledigt, weil das Gerät wieder normal ist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncidentResolved {
    pub id: i64,
    pub device_id: String,
}

/// Befehl an den Überwachungs-Service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetThreshold {
    pub critical_pct: f64,
}

/// Befehl an den Meldungs-Service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DismissIncident {
    pub id: i64,
}

/// Antwort auf einen Befehl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandReply {
    pub ok: bool,
    /// Fehlertext, wenn `ok` falsch ist
    pub error: Option<String>,
}

impl CommandReply {
    pub fn ok() -> Self {
        Self { ok: true, error: None }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
    }
}

/// Was die API per Server-Sent Events an den Browser weitergibt.
///
/// Der Browser lädt daraufhin die betroffenen Daten neu; die Nutzlast ist
/// absichtlich klein.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveEvent {
    Reading {
        device_id: String,
        timestamp_ms: i64,
        charge_pct: f64,
    },
    StatusChanged {
        device_id: String,
        status: Status,
    },
    IncidentChanged {
        id: i64,
        device_id: String,
    },
    SettingsChanged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_event_has_type_tag() {
        let json = serde_json::to_string(&LiveEvent::StatusChanged {
            device_id: "TN-0001".into(),
            status: Status::Critical,
        })
        .unwrap();
        assert!(json.contains("\"type\":\"status_changed\""));
        assert!(json.contains("\"status\":\"critical\""));
    }

    #[test]
    fn reply_helpers() {
        assert!(CommandReply::ok().ok);
        let e = CommandReply::error("kaputt");
        assert!(!e.ok);
        assert_eq!(e.error.as_deref(), Some("kaputt"));
    }

    #[test]
    fn events_and_commands_use_separate_subject_spaces() {
        for s in [
            subject::READING_STORED,
            subject::DEVICE_CRITICAL,
            subject::DEVICE_RECOVERED,
            subject::THRESHOLD_CHANGED,
            subject::INCIDENT_OPENED,
            subject::INCIDENT_DISMISSED,
            subject::INCIDENT_RESOLVED,
        ] {
            assert!(s.starts_with("events."));
        }
        assert!(!subject::CMD_SET_THRESHOLD.starts_with("events."));
        assert!(!subject::CMD_DISMISS_INCIDENT.starts_with("events."));
    }
}
