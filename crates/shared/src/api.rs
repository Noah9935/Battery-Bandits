//! Typen der REST-Schnittstelle zwischen Dashboard-Service und Frontend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::forecast::Forecast;

/// Zeitraum der Verlaufsansicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Range {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
}

impl Range {
    pub const ALL: [Range; 3] = [Range::Day, Range::Week, Range::Month];

    /// Wert für die Adresse (`?range=24h`).
    pub fn as_str(self) -> &'static str {
        match self {
            Range::Day => "24h",
            Range::Week => "7d",
            Range::Month => "30d",
        }
    }

    /// Anzeigename für die Oberfläche.
    pub fn label(self) -> &'static str {
        match self {
            Range::Day => "24 Std.",
            Range::Week => "7 Tage",
            Range::Month => "30 Tage",
        }
    }

    /// Länge des Zeitraums als Postgres-Intervall.
    pub fn window_sql(self) -> &'static str {
        match self {
            Range::Day => "24 hours",
            Range::Week => "7 days",
            Range::Month => "30 days",
        }
    }

    /// Breite eines Mittelungsfensters, damit jede Kurve höchstens rund 300 Punkte hat.
    pub fn bucket_sql(self) -> &'static str {
        match self {
            Range::Day => "5 minutes",
            Range::Week => "30 minutes",
            Range::Month => "2 hours",
        }
    }
}

impl std::str::FromStr for Range {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Range::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unbekannter Zeitraum: {s}"))
    }
}

/// Ein (gemittelter) Punkt der Verlaufskurve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct HistoryPoint {
    pub time: DateTime<Utc>,
    pub soc_percent: f32,
    pub voltage_mv: f32,
    pub current_ma: f32,
    pub temperature_c: Option<f32>,
}

/// Verlauf eines Geräts mit Prognose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub device_id: String,
    pub range: Range,
    pub points: Vec<HistoryPoint>,
    /// Restlaufzeit-Prognose aus den letzten 24 Stunden, falls das Gerät entladen wird.
    pub forecast: Option<Forecast>,
}

/// Verlauf des Ladezustands eines Geräts für die kleine Trendlinie in der Tabelle:
/// Stundenmittel der letzten 24 Stunden bis zur letzten Messung des Geräts, älteste zuerst.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trend {
    pub device_id: String,
    pub points: Vec<f32>,
}

/// Angemeldete Person (Antwort von `/api/auth/me` und `/api/auth/login`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Me {
    pub username: String,
    pub display_name: String,
}

/// Anmeldedaten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
}

/// Sortierschlüssel der Geräteliste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceSort {
    #[default]
    Id,
    SocPercent,
    LastSeenAt,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_roundtrips_through_text_and_json() {
        for range in Range::ALL {
            assert_eq!(range.as_str().parse::<Range>(), Ok(range));
            let json = serde_json::to_string(&range).unwrap();
            assert_eq!(json, format!("\"{}\"", range.as_str()));
            assert_eq!(serde_json::from_str::<Range>(&json).unwrap(), range);
        }
        assert!("1h".parse::<Range>().is_err());
    }

    #[test]
    fn longer_ranges_use_wider_buckets() {
        assert_eq!(Range::Day.bucket_sql(), "5 minutes");
        assert_eq!(Range::Month.window_sql(), "30 days");
    }

    #[test]
    fn sort_keys_use_snake_case() {
        assert_eq!(
            serde_json::to_string(&DeviceSort::SocPercent).unwrap(),
            "\"soc_percent\""
        );
        assert_eq!(
            serde_json::from_str::<DeviceSort>("\"last_seen_at\"").unwrap(),
            DeviceSort::LastSeenAt
        );
        assert_eq!(serde_json::from_str::<SortDir>("\"desc\"").unwrap(), SortDir::Desc);
    }
}
