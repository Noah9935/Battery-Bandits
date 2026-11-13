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
        Stagnation => "stagnation",
        Drop => "drop",
        Fault => "fault",
    }
);

impl AlertKind {
    /// Kurztext für Listen und Protokoll.
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Critical => "Ladestand kritisch",
            AlertKind::Offline => "Keine Messwerte mehr",
            AlertKind::Stagnation => "Messwerte bleiben unverändert",
            AlertKind::Drop => "Spannung bricht plötzlich ein",
            AlertKind::Fault => "Ausgefallen trotz hohem Ladestand",
        }
    }

    /// Braucht die Meldung sofort jemanden vor Ort? Leere Batterie, Funkstille und Ausfall ja,
    /// Auffälligkeiten (Stillstand, Spannungseinbruch) sind Hinweise.
    pub fn is_urgent(self) -> bool {
        matches!(self, AlertKind::Critical | AlertKind::Offline | AlertKind::Fault)
    }

    /// Hängt die Meldung am Gerätestatus? Dann schließt sie sich, sobald das Gerät wieder in Ordnung ist.
    /// Auffälligkeiten (Stillstand, Spannungseinbruch) bleiben, bis jemand sie bearbeitet.
    pub fn follows_status(self) -> bool {
        matches!(self, AlertKind::Critical | AlertKind::Offline | AlertKind::Fault)
    }
}

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
    /// Seit wann das Gerät in seinem jetzigen Status ist (von der Überwachung bei jedem Wechsel gesetzt).
    #[serde(default)]
    pub status_changed_at: Option<DateTime<Utc>>,
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

/// Batteriewechsel mit einem Tipp bestätigen, auch ohne vorher geplanten Einsatz.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BatterySwap {
    pub device_id: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Benutzername der Person, die gewechselt hat. Ohne Angabe gilt, wer angemeldet ist.
    #[serde(default)]
    pub technician: Option<String>,
    /// Ladestand der neuen Batterie in Prozent. Ohne Angabe wird er aus der nächsten Spannung geschätzt.
    #[serde(default)]
    pub soc_percent: Option<f32>,
}

impl BatterySwap {
    /// Vorgabe der Schnellerfassung: eine volle Batterie.
    pub const FULL_SOC_PERCENT: f32 = 100.0;

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.device_id.trim().is_empty() {
            return Err("Das Gerät fehlt");
        }
        if let Some(soc) = self.soc_percent
            && !(soc.is_finite() && (0.0..=100.0).contains(&soc))
        {
            return Err("Der Ladestand muss zwischen 0 und 100 Prozent liegen");
        }
        if self.note.as_deref().is_some_and(|n| n.chars().count() > 500) {
            return Err("Die Notiz ist zu lang (höchstens 500 Zeichen)");
        }
        Ok(())
    }
}

/// Ein abgeschlossener Batteriezyklus (vom Einbau bis zum Wechsel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct BatteryCycle {
    pub id: i64,
    pub device_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub discharged_ah: f64,
    pub start_voltage_mv: Option<i32>,
    pub end_voltage_mv: Option<i32>,
    pub avg_temperature_c: Option<f32>,
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

/// Eintrag im Postfach einer Person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct InboxItem {
    pub id: i64,
    pub at: DateTime<Utc>,
    /// `alert`, `assigned` oder `comment`.
    pub kind: String,
    pub text: String,
    pub device_id: Option<String>,
    pub task_id: Option<i64>,
    pub read: bool,
}

/// Welche Einträge als gelesen gelten sollen: `None` bedeutet alle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkRead {
    #[serde(default)]
    pub ids: Option<Vec<i64>>,
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
    fn urgent_kinds_are_the_ones_that_follow_the_status() {
        for kind in AlertKind::ALL {
            assert_eq!(kind.is_urgent(), kind.follows_status(), "{kind:?}");
        }
    }

    #[test]
    fn a_battery_swap_needs_a_device_and_a_plausible_charge() {
        let ok = BatterySwap {
            device_id: "TN0001".into(),
            soc_percent: Some(BatterySwap::FULL_SOC_PERCENT),
            ..BatterySwap::default()
        };
        assert_eq!(ok.validate(), Ok(()));
        assert!(BatterySwap::default().validate().is_err());
        for soc in [-1.0, 100.5, f32::NAN] {
            let bad = BatterySwap {
                soc_percent: Some(soc),
                ..ok.clone()
            };
            assert!(bad.validate().is_err(), "{soc}");
        }
        let long = BatterySwap {
            note: Some("x".repeat(501)),
            ..ok.clone()
        };
        assert!(long.validate().is_err());
        // Ältere Aufrufer schicken nur die Kennung.
        let minimal: BatterySwap = serde_json::from_str(r#"{"device_id":"TN0001"}"#).unwrap();
        assert_eq!((minimal.soc_percent, minimal.note), (None, None));
    }

    #[test]
    fn summary_defaults_to_zero() {
        let s = DashboardSummary::default();
        assert_eq!((s.total, s.open_alerts, s.average_soc_percent), (0, 0, None));
    }
}
