//! Datentypen der REST-API, gemeinsam für den API-Server und die Web-App.

use serde::{Deserialize, Serialize};

use crate::Status;

/// Eine Zeile der Geräteübersicht (F03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSummary {
    pub id: String,
    pub charge_pct: Option<f64>,
    pub last_reading_ms: Option<i64>,
    pub status: Status,
    pub open_incident: bool,
}

/// Ein Punkt im Ladestandsverlauf (F04).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub t_ms: i64,
    pub charge_pct: f64,
}

/// Auflösung eines Verlaufs. Lange Zeiträume kommen aus Stundenwerten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    Raw,
    Hourly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub device_id: String,
    pub from_ms: i64,
    pub to_ms: i64,
    pub resolution: Resolution,
    pub points: Vec<HistoryPoint>,
}

/// Wählbare Zeiträume der Verlaufsansicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Range {
    Day,
    Week,
    Month,
    Year,
}

impl Range {
    pub const ALL: [Range; 4] = [Range::Day, Range::Week, Range::Month, Range::Year];

    pub fn as_str(self) -> &'static str {
        match self {
            Range::Day => "day",
            Range::Week => "week",
            Range::Month => "month",
            Range::Year => "year",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "day" => Some(Range::Day),
            "week" => Some(Range::Week),
            "month" => Some(Range::Month),
            "year" => Some(Range::Year),
            _ => None,
        }
    }

    pub fn duration_ms(self) -> i64 {
        const DAY: i64 = 24 * 3600 * 1000;
        match self {
            Range::Day => DAY,
            Range::Week => 7 * DAY,
            Range::Month => 30 * DAY,
            Range::Year => 365 * DAY,
        }
    }

    /// Rohwerte nur für kurze Zeiträume, sonst Stundenwerte (Continuous Aggregate).
    pub fn resolution(self) -> Resolution {
        match self {
            Range::Day | Range::Week => Resolution::Raw,
            Range::Month | Range::Year => Resolution::Hourly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncidentState {
    Open,
    Dismissed,
    Resolved,
}

impl IncidentState {
    pub fn as_str(self) -> &'static str {
        match self {
            IncidentState::Open => "open",
            IncidentState::Dismissed => "dismissed",
            IncidentState::Resolved => "resolved",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "open" => Some(IncidentState::Open),
            "dismissed" => Some(IncidentState::Dismissed),
            "resolved" => Some(IncidentState::Resolved),
            _ => None,
        }
    }
}

/// Eine Meldung zu einem kritischen Gerät.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Incident {
    pub id: i64,
    pub device_id: String,
    pub opened_at_ms: i64,
    pub closed_at_ms: Option<i64>,
    pub state: IncidentState,
    pub charge_pct: f64,
}

/// Konfiguration, die ohne Codeänderung anpassbar ist (NF07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub critical_threshold_pct: f64,
}

/// Eine verworfene Nachricht (F06).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RejectedMessage {
    pub id: i64,
    pub received_at_ms: i64,
    pub topic: String,
    pub payload: String,
    pub reason: String,
}

/// Zusammenfassung für die Kopfzeile der Übersicht.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub total: usize,
    pub ok: usize,
    pub critical: usize,
}

impl Counts {
    pub fn of(devices: &[DeviceSummary]) -> Self {
        let mut c = Counts {
            total: devices.len(),
            ..Default::default()
        };
        for d in devices {
            match d.status {
                Status::Ok => c.ok += 1,
                Status::Critical => c.critical += 1,
            }
        }
        c
    }
}

/// Fehlerantwort der API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_roundtrip() {
        for r in Range::ALL {
            assert_eq!(Range::parse(r.as_str()), Some(r));
        }
        assert_eq!(Range::parse("decade"), None);
    }

    #[test]
    fn long_ranges_use_hourly_values() {
        assert_eq!(Range::Day.resolution(), Resolution::Raw);
        assert_eq!(Range::Week.resolution(), Resolution::Raw);
        assert_eq!(Range::Month.resolution(), Resolution::Hourly);
        assert_eq!(Range::Year.resolution(), Resolution::Hourly);
    }

    #[test]
    fn counts_summarize_devices() {
        let d = |id: &str, status| DeviceSummary {
            id: id.into(),
            charge_pct: Some(50.0),
            last_reading_ms: Some(0),
            status,
            open_incident: false,
        };
        let c = Counts::of(&[d("a", Status::Ok), d("b", Status::Critical), d("c", Status::Ok)]);
        assert_eq!(
            c,
            Counts {
                total: 3,
                ok: 2,
                critical: 1
            }
        );
    }

    #[test]
    fn incident_state_roundtrip() {
        for s in [IncidentState::Open, IncidentState::Dismissed, IncidentState::Resolved] {
            assert_eq!(IncidentState::from_db(s.as_str()), Some(s));
        }
    }
}
