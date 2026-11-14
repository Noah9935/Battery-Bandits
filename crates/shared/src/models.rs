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
    /// Seit wann das Gerät ausgemustert ist (None: es ist aktiv).
    #[serde(default)]
    pub retired_at: Option<DateTime<Utc>>,
    /// Warum es ausgemustert wurde (`done`, `replaced`, `discarded`, siehe [`RetireReason`]).
    #[serde(default)]
    pub retired_reason: Option<String>,
    #[serde(default)]
    pub retired_note: Option<String>,
}

/// Grund, ein Gerät auszutragen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetireReason {
    /// Alles erledigt, das Gerät wird nicht mehr gebraucht.
    Done,
    /// Durch ein anderes Gerät ersetzt.
    Replaced,
    /// Weggeworfen oder entsorgt.
    Discarded,
}

impl RetireReason {
    pub const ALL: [RetireReason; 3] = [RetireReason::Done, RetireReason::Replaced, RetireReason::Discarded];

    pub fn as_str(self) -> &'static str {
        match self {
            RetireReason::Done => "done",
            RetireReason::Replaced => "replaced",
            RetireReason::Discarded => "discarded",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            RetireReason::Done => "Alles erledigt",
            RetireReason::Replaced => "Ersetzt",
            RetireReason::Discarded => "Weggeworfen",
        }
    }
}

/// Anfrage zum Ausmustern eines Geräts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetireDevice {
    pub reason: RetireReason,
    #[serde(default)]
    pub note: Option<String>,
}

impl RetireDevice {
    pub const MAX_NOTE_CHARS: usize = 500;

    pub fn validate(&self) -> Result<(), &'static str> {
        match &self.note {
            Some(note) if note.chars().count() > Self::MAX_NOTE_CHARS => {
                Err("Die Notiz darf höchstens 500 Zeichen lang sein.")
            }
            _ => Ok(()),
        }
    }
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
    /// Ladestand der neuen Batterie in Prozent. Ohne Angabe schätzt das System ihn mit der ersten Messung aus der
    /// Spannung: Die Batterien werden mitgenommen, geladen und zurückgebracht, niemand muss den Stand kennen.
    #[serde(default)]
    pub soc_percent: Option<f32>,
    /// Preis der neuen Batterie in Cent (optional), für den Vergleich der Kosten je Batterie.
    #[serde(default)]
    pub price_cents: Option<i32>,
    /// Modell der neuen Batterie (optional). Gleiche Modelle werden in der Effizienz zusammengefasst.
    #[serde(default)]
    pub model: Option<String>,
}

impl BatterySwap {
    /// Längster erlaubter Modellname.
    pub const MAX_MODEL_CHARS: usize = 60;

    /// Höchster erlaubter Preis in Cent (10.000 Euro), gegen Tippfehler.
    pub const MAX_PRICE_CENTS: i32 = 1_000_000;

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.device_id.trim().is_empty() {
            return Err("Das Gerät fehlt");
        }
        if let Some(soc) = self.soc_percent
            && !(soc.is_finite() && (0.0..=100.0).contains(&soc))
        {
            return Err("Der Ladestand muss zwischen 0 und 100 Prozent liegen");
        }
        if self
            .model
            .as_deref()
            .is_some_and(|m| m.trim().chars().count() > Self::MAX_MODEL_CHARS)
        {
            return Err("Der Modellname ist zu lang (höchstens 60 Zeichen)");
        }
        if self
            .price_cents
            .is_some_and(|p| !(0..=Self::MAX_PRICE_CENTS).contains(&p))
        {
            return Err("Der Preis muss zwischen 0 und 10.000 Euro liegen");
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
    /// Preis der Batterie dieses Zyklus in Cent, falls beim Einbau angegeben.
    pub price_cents: Option<i32>,
    /// Modell der Batterie, falls beim Einbau angegeben.
    pub model: Option<String>,
    /// Innenwiderstand in Milliohm, geschätzt aus Spannung und Strom des Zyklus.
    pub resistance_mohm: Option<f32>,
}

/// Betrag in Euro mit Dezimalkomma: `89,50 €`.
pub fn euro_text(cents: i32) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    format!("{sign}{},{:02} €", cents / 100, cents % 100)
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
    /// Einsatz, auf den sich der Eintrag bezieht (bei Kommentaren zum Ausklappen).
    #[serde(default)]
    pub task_id: Option<i64>,
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
            soc_percent: Some(100.0),
            price_cents: Some(8_950),
            ..BatterySwap::default()
        };
        assert_eq!(ok.validate(), Ok(()));
        for price in [-1, BatterySwap::MAX_PRICE_CENTS + 1] {
            let bad = BatterySwap {
                price_cents: Some(price),
                ..ok.clone()
            };
            assert!(bad.validate().is_err(), "{price}");
        }
        assert_eq!(BatterySwap::default().validate(), Err("Das Gerät fehlt"));
        let long_model = BatterySwap {
            model: Some("x".repeat(61)),
            ..ok.clone()
        };
        assert!(long_model.validate().is_err());
        assert_eq!(euro_text(8_950), "89,50 €");
        assert_eq!(euro_text(5), "0,05 €");
        assert_eq!(euro_text(100_000), "1000,00 €");
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

    #[test]
    fn retire_reasons_round_trip_and_have_german_names() {
        for reason in RetireReason::ALL {
            assert_eq!(RetireReason::parse(reason.as_str()), Some(reason));
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, format!("\"{}\"", reason.as_str()));
            assert!(!reason.label().is_empty());
        }
        assert_eq!(RetireReason::parse("kaputt"), None);
        let long = RetireDevice {
            reason: RetireReason::Done,
            note: Some("x".repeat(501)),
        };
        assert!(long.validate().is_err());
        assert!(
            RetireDevice {
                reason: RetireReason::Done,
                note: None
            }
            .validate()
            .is_ok()
        );
    }
}
