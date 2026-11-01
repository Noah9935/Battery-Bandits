//! Live-Updates (NF01): Ereignisse aus NATS gehen per Server-Sent Events an den Browser.

use futures_util::StreamExt;
use shared::Status;
use shared::events::{
    DeviceCritical, DeviceRecovered, IncidentDismissed, IncidentOpened, IncidentResolved, LiveEvent, ReadingStored,
    subject,
};

use crate::state::AppState;

/// Übersetzt ein Ereignis aus dem Bus in das, was der Browser braucht.
/// Unbekannte oder unlesbare Ereignisse ergeben `None`.
pub fn to_live_event(subject: &str, payload: &[u8]) -> Option<LiveEvent> {
    match subject {
        subject::READING_STORED => {
            let e: ReadingStored = serde_json::from_slice(payload).ok()?;
            Some(LiveEvent::Reading {
                device_id: e.device_id,
                timestamp_ms: e.timestamp_ms,
                charge_pct: e.charge_pct,
            })
        }
        subject::DEVICE_CRITICAL => {
            let e: DeviceCritical = serde_json::from_slice(payload).ok()?;
            Some(LiveEvent::StatusChanged {
                device_id: e.device_id,
                status: Status::Critical,
            })
        }
        subject::DEVICE_RECOVERED => {
            let e: DeviceRecovered = serde_json::from_slice(payload).ok()?;
            Some(LiveEvent::StatusChanged {
                device_id: e.device_id,
                status: Status::Ok,
            })
        }
        subject::INCIDENT_OPENED => {
            let e: IncidentOpened = serde_json::from_slice(payload).ok()?;
            Some(LiveEvent::IncidentChanged {
                id: e.id,
                device_id: e.device_id,
            })
        }
        subject::INCIDENT_DISMISSED => {
            let e: IncidentDismissed = serde_json::from_slice(payload).ok()?;
            Some(LiveEvent::IncidentChanged {
                id: e.id,
                device_id: e.device_id,
            })
        }
        subject::INCIDENT_RESOLVED => {
            let e: IncidentResolved = serde_json::from_slice(payload).ok()?;
            Some(LiveEvent::IncidentChanged {
                id: e.id,
                device_id: e.device_id,
            })
        }
        subject::THRESHOLD_CHANGED => Some(LiveEvent::SettingsChanged),
        _ => None,
    }
}

/// Hört dauerhaft auf den Bus und verteilt an alle Browser-Verbindungen.
pub async fn relay(state: AppState) -> anyhow::Result<()> {
    let mut sub = state.bus.subscribe_events().await?;
    while let Some(msg) = sub.next().await {
        if let Some(event) = to_live_event(msg.subject.as_str(), &msg.payload) {
            // Ohne offene Browser gibt es keine Empfänger. Das ist kein Fehler.
            let _ = state.live.send(event);
        }
    }
    anyhow::bail!("Ereignisabo wurde beendet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_becomes_live_reading() {
        let payload = br#"{"device_id":"TN-1","timestamp_ms":5,"charge_pct":40.0}"#;
        assert_eq!(
            to_live_event(subject::READING_STORED, payload),
            Some(LiveEvent::Reading {
                device_id: "TN-1".into(),
                timestamp_ms: 5,
                charge_pct: 40.0
            })
        );
    }

    #[test]
    fn critical_and_recovered_become_status_changes() {
        let critical = br#"{"device_id":"TN-1","charge_pct":10.0,"threshold_pct":20.0,"at_ms":1}"#;
        assert_eq!(
            to_live_event(subject::DEVICE_CRITICAL, critical),
            Some(LiveEvent::StatusChanged {
                device_id: "TN-1".into(),
                status: Status::Critical
            })
        );
        let recovered = br#"{"device_id":"TN-1","charge_pct":90.0,"at_ms":2}"#;
        assert_eq!(
            to_live_event(subject::DEVICE_RECOVERED, recovered),
            Some(LiveEvent::StatusChanged {
                device_id: "TN-1".into(),
                status: Status::Ok
            })
        );
    }

    #[test]
    fn incident_events_become_incident_changes() {
        let opened = br#"{"id":7,"device_id":"TN-1","opened_at_ms":1,"charge_pct":10.0}"#;
        assert_eq!(
            to_live_event(subject::INCIDENT_OPENED, opened),
            Some(LiveEvent::IncidentChanged {
                id: 7,
                device_id: "TN-1".into()
            })
        );
        let dismissed = br#"{"id":7,"device_id":"TN-1"}"#;
        assert!(to_live_event(subject::INCIDENT_DISMISSED, dismissed).is_some());
        assert!(to_live_event(subject::INCIDENT_RESOLVED, dismissed).is_some());
    }

    #[test]
    fn threshold_change_signals_settings() {
        assert_eq!(
            to_live_event(subject::THRESHOLD_CHANGED, b"{\"critical_pct\":30.0}"),
            Some(LiveEvent::SettingsChanged)
        );
    }

    #[test]
    fn garbage_and_unknown_subjects_are_ignored() {
        assert_eq!(to_live_event(subject::READING_STORED, b"kein json"), None);
        assert_eq!(to_live_event("events.something.else", b"{}"), None);
    }
}
