//! Fachliche Modelle. Mit dem Feature `sqlx` direkt aus der Datenbank lesbar.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident, $pg:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        #[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
        #[cfg_attr(feature = "sqlx", sqlx(type_name = $pg, rename_all = "lowercase"))]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s { $($text => Ok($name::$variant),)+ other => Err(format!("unbekannter Wert: {other}")) }
            }
        }
    };
}

text_enum!(
    /// Zustand eines Geräts. `Offline` hat Vorrang vor allen Ladezuständen.
    DeviceStatus, "device_status", {
        Normal => "normal",
        Warning => "warning",
        Critical => "critical",
        Offline => "offline",
    }
);

text_enum!(
    /// Lebenszyklus einer Meldung.
    AlertStatus, "alert_status", {
        Open => "open",
        Dismissed => "dismissed",
        Resolved => "resolved",
    }
);

text_enum!(
    /// Anlass einer Meldung.
    AlertKind, "alert_kind", {
        Critical => "critical",
        Offline => "offline",
    }
);

text_enum!(
    /// Stand eines Wartungseinsatzes.
    MaintenanceStatus, "maintenance_status", {
        Planned => "planned",
        Completed => "completed",
    }
);

text_enum!(
    /// Art eines Einsatzes.
    TaskKind, "task_kind", {
        Battery => "battery",
        Inspection => "inspection",
        Repair => "repair",
    }
);

text_enum!(
    /// Dringlichkeit eines Einsatzes.
    TaskPriority, "task_priority", {
        Low => "low",
        Normal => "normal",
        High => "high",
    }
);

#[allow(clippy::derivable_impls)]
impl Default for TaskKind {
    fn default() -> Self {
        TaskKind::Battery
    }
}

#[allow(clippy::derivable_impls)]
impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            TaskKind::Battery => "Batteriewechsel",
            TaskKind::Inspection => "Kontrolle vor Ort",
            TaskKind::Repair => "Reparatur",
        }
    }
}

impl TaskPriority {
    pub fn label(self) -> &'static str {
        match self {
            TaskPriority::Low => "niedrig",
            TaskPriority::Normal => "normal",
            TaskPriority::High => "hoch",
        }
    }
}

text_enum!(
    /// Woher die Daten eines Geräts stammen: vom Dozenten (Messdatenplattform und Vorlesung) oder erfunden (Simulator).
    DataSource, "data_source", {
        Dozent => "dozent",
        Simulation => "simulation",
    }
);

impl DeviceStatus {
    /// Muss sich jemand kümmern?
    pub fn needs_attention(self) -> bool {
        matches!(self, DeviceStatus::Critical | DeviceStatus::Offline)
    }
}

/// Ein TrafficNode mit seinem letzten bekannten Zustand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Device {
    pub id: String,
    pub name: Option<String>,
    pub status: DeviceStatus,
    pub soc_percent: Option<f32>,
    pub voltage_mv: Option<i32>,
    pub current_ma: Option<i32>,
    pub temperature_c: Option<f32>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub altitude_m: Option<f64>,
    pub satellites: Option<i16>,
    pub last_seen_at: Option<DateTime<Utc>>,
    /// Ladezustand beim Einbau der aktuellen Batterie (Startwert der Integration).
    pub soc_start_percent: Option<f32>,
    /// Seit dem Einbau entnommene Ladung in Ah (Integral des Stroms).
    pub discharged_ah: f64,
    pub battery_installed_at: Option<DateTime<Utc>>,
    /// Herkunft der Daten (Dozent oder Simulation).
    pub data_source: DataSource,
    /// Entladerate der letzten 24 Stunden in Prozentpunkten pro Stunde (Prognose des Monitoring-Service).
    pub discharge_rate_pct_per_hour: Option<f32>,
    /// Prognostizierte Stunden bis 0 Prozent.
    pub forecast_hours_remaining: Option<f32>,
    pub created_at: DateTime<Utc>,
}

/// Ein gespeicherter Messwert (Hypertable `readings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Reading {
    pub time: DateTime<Utc>,
    pub device_id: String,
    pub voltage_mv: i32,
    pub current_ma: i32,
    pub temperature_c: Option<f32>,
    pub soc_percent: f32,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub altitude_m: Option<f64>,
    pub satellites: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Alert {
    pub id: i64,
    pub device_id: String,
    pub kind: AlertKind,
    pub status: AlertStatus,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct MaintenanceTask {
    pub id: i64,
    pub device_id: String,
    pub alert_id: Option<i64>,
    pub status: MaintenanceStatus,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub kind: TaskKind,
    pub priority: TaskPriority,
    /// Benutzername der zuständigen Person.
    pub assignee: Option<String>,
    pub assignee_name: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub created_by_name: Option<String>,
    pub completed_by_name: Option<String>,
    pub comment_count: i64,
}

/// Anfrage zum Planen eines Einsatzes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NewMaintenanceTask {
    pub device_id: String,
    #[serde(default)]
    pub alert_id: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub kind: TaskKind,
    #[serde(default)]
    pub priority: TaskPriority,
    /// Benutzername der zuständigen Person, leer für "noch niemand".
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub due_date: Option<NaiveDate>,
}

/// Zuständigkeit ändern: `None` nimmt sie weg.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignTask {
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct TaskComment {
    pub id: i64,
    pub task_id: i64,
    pub author_name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewTaskComment {
    pub body: String,
}

/// Eintrag im Aktivitätsprotokoll.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ActivityEntry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub actor_name: String,
    pub device_id: Option<String>,
    pub text: String,
}

/// Kennzahlen für die Übersicht.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total: i64,
    pub normal: i64,
    pub warning: i64,
    pub critical: i64,
    pub offline: i64,
    pub open_alerts: i64,
    pub average_soc_percent: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn status_text_roundtrip() {
        for status in DeviceStatus::ALL {
            assert_eq!(DeviceStatus::from_str(status.as_str()), Ok(*status));
        }
        assert!(DeviceStatus::from_str("kaputt").is_err());
    }

    #[test]
    fn enums_serialize_lowercase() {
        assert_eq!(serde_json::to_string(&DeviceStatus::Critical).unwrap(), "\"critical\"");
        assert_eq!(serde_json::to_string(&AlertStatus::Dismissed).unwrap(), "\"dismissed\"");
        assert_eq!(
            serde_json::from_str::<AlertKind>("\"offline\"").unwrap(),
            AlertKind::Offline
        );
        assert_eq!(
            serde_json::to_string(&MaintenanceStatus::Completed).unwrap(),
            "\"completed\""
        );
    }

    #[test]
    fn attention_only_for_critical_and_offline() {
        assert!(DeviceStatus::Critical.needs_attention());
        assert!(DeviceStatus::Offline.needs_attention());
        assert!(!DeviceStatus::Normal.needs_attention());
        assert!(!DeviceStatus::Warning.needs_attention());
    }

    #[test]
    fn summary_defaults_to_zero() {
        let s = DashboardSummary::default();
        assert_eq!((s.total, s.open_alerts, s.average_soc_percent), (0, 0, None));
    }
}
