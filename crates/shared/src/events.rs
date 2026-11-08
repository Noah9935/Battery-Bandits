//! Domänenereignisse zwischen den Diensten.
//!
//! Transport ist Postgres `NOTIFY`/`LISTEN` auf dem Kanal [`CHANNEL`]. Die
//! Nutzlast ist JSON mit dem Feld `type`. Die Event-Storming-Namen
//! `DeviceCritical`, `DeviceOffline` und `DeviceRecovered` sind Ausprägungen
//! von `DeviceStatusChanged` (siehe [`DomainEvent::storming_name`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{AlertKind, DeviceStatus};

/// Name des Postgres-Kanals.
pub const CHANNEL: &str = "domain_events";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DomainEvent {
    ReadingSaved {
        device_id: String,
        time: DateTime<Utc>,
        soc_percent: f32,
    },
    DeviceStatusChanged {
        device_id: String,
        old_status: DeviceStatus,
        new_status: DeviceStatus,
        soc_percent: Option<f32>,
    },
    AlertCreated {
        alert_id: i64,
        device_id: String,
        kind: AlertKind,
    },
    BatteryReplaced {
        device_id: String,
        at: DateTime<Utc>,
    },
    /// Die Überwachung hat eine Auffälligkeit bemerkt (Stillstand oder Spannungseinbruch).
    AnomalyDetected {
        device_id: String,
        kind: AlertKind,
    },
    /// Ein Einsatz wurde geplant, zugewiesen, kommentiert oder erledigt. Die Oberflächen laden dann neu.
    TaskChanged {
        task_id: i64,
        device_id: String,
    },
}

impl DomainEvent {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Ereignisse sind immer serialisierbar")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Das Gerät, auf das sich das Ereignis bezieht.
    pub fn device_id(&self) -> &str {
        match self {
            DomainEvent::ReadingSaved { device_id, .. }
            | DomainEvent::DeviceStatusChanged { device_id, .. }
            | DomainEvent::AlertCreated { device_id, .. }
            | DomainEvent::BatteryReplaced { device_id, .. }
            | DomainEvent::AnomalyDetected { device_id, .. }
            | DomainEvent::TaskChanged { device_id, .. } => device_id,
        }
    }

    /// Name aus dem Event Storming (`DeviceCritical`, `DeviceOffline`, `DeviceRecovered`).
    pub fn storming_name(&self) -> &'static str {
        match self {
            DomainEvent::ReadingSaved { .. } => "ReadingSaved",
            DomainEvent::DeviceStatusChanged {
                new_status, old_status, ..
            } => match new_status {
                DeviceStatus::Critical => "DeviceCritical",
                DeviceStatus::Offline => "DeviceOffline",
                DeviceStatus::Normal | DeviceStatus::Warning if old_status.needs_attention() => "DeviceRecovered",
                _ => "DeviceStatusChanged",
            },
            DomainEvent::AlertCreated { .. } => "AlertCreated",
            DomainEvent::BatteryReplaced { .. } => "BatteryReplaced",
            DomainEvent::AnomalyDetected { .. } => "AnomalyDetected",
            DomainEvent::TaskChanged { .. } => "TaskChanged",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(old: DeviceStatus, new: DeviceStatus) -> DomainEvent {
        DomainEvent::DeviceStatusChanged {
            device_id: "TN0001".into(),
            old_status: old,
            new_status: new,
            soc_percent: Some(15.0),
        }
    }

    #[test]
    fn json_roundtrip_for_every_event() {
        let at = DateTime::parse_from_rfc3339("2026-11-05T11:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let events = [
            DomainEvent::ReadingSaved {
                device_id: "TN0001".into(),
                time: at,
                soc_percent: 80.5,
            },
            changed(DeviceStatus::Normal, DeviceStatus::Critical),
            DomainEvent::AlertCreated {
                alert_id: 7,
                device_id: "TN0001".into(),
                kind: AlertKind::Offline,
            },
            DomainEvent::BatteryReplaced {
                device_id: "TN0001".into(),
                at,
            },
        ];
        for event in events {
            assert_eq!(DomainEvent::from_json(&event.to_json()).unwrap(), event);
        }
    }

    #[test]
    fn json_has_type_tag() {
        let value: serde_json::Value =
            serde_json::from_str(&changed(DeviceStatus::Normal, DeviceStatus::Offline).to_json()).unwrap();
        assert_eq!(value["type"], "DeviceStatusChanged");
        assert_eq!(value["new_status"], "offline");
    }

    #[test]
    fn unknown_or_broken_json_is_an_error() {
        assert!(DomainEvent::from_json(r#"{"type":"Unbekannt"}"#).is_err());
        assert!(DomainEvent::from_json("kein json").is_err());
    }

    #[test]
    fn exposes_device_id() {
        assert_eq!(
            changed(DeviceStatus::Normal, DeviceStatus::Warning).device_id(),
            "TN0001"
        );
    }

    #[test]
    fn maps_to_storming_names() {
        assert_eq!(
            changed(DeviceStatus::Normal, DeviceStatus::Critical).storming_name(),
            "DeviceCritical"
        );
        assert_eq!(
            changed(DeviceStatus::Warning, DeviceStatus::Offline).storming_name(),
            "DeviceOffline"
        );
        assert_eq!(
            changed(DeviceStatus::Critical, DeviceStatus::Normal).storming_name(),
            "DeviceRecovered"
        );
        assert_eq!(
            changed(DeviceStatus::Offline, DeviceStatus::Warning).storming_name(),
            "DeviceRecovered"
        );
        assert_eq!(
            changed(DeviceStatus::Normal, DeviceStatus::Warning).storming_name(),
            "DeviceStatusChanged"
        );
    }
}
