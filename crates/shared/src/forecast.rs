//! Einfache lineare Restlaufzeit-Prognose aus dem Verlauf des Ladezustands.
//!
//! Annahme: Die Entladung ist in den letzten 24 Stunden näherungsweise gleichmäßig.
//! Eine Ausgleichsgerade (kleinste Quadrate) über Zeit und Ladezustand liefert die
//! Entladerate in Prozentpunkten pro Stunde; daraus folgt die Restzeit bis leer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Ab dieser Rate (Prozentpunkte pro Stunde) gilt ein Gerät als entladend.
const MIN_DISCHARGE_RATE: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Forecast {
    /// Entladerate in Prozentpunkten pro Stunde (positiv).
    pub rate_pct_per_hour: f32,
    /// Stunden bis 0 Prozent.
    pub hours_to_empty: f32,
}

impl Forecast {
    /// Stunden bis zum Unterschreiten einer Schwelle (0, wenn schon darunter).
    pub fn hours_to_threshold(&self, soc_percent: f32, threshold_pct: f32) -> f32 {
        ((soc_percent - threshold_pct) / self.rate_pct_per_hour).max(0.0)
    }
}

/// Steigung der Ausgleichsgeraden (kleinste Quadrate) von y über x.
/// `None`, wenn weniger als zwei Punkte oder alle x gleich sind.
pub fn linear_slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if sxx <= f64::EPSILON {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    Some(sxy / sxx)
}

/// Prognose aus einer Steigung (Prozentpunkte pro Stunde, negativ bei Entladung).
/// `None`, wenn das Gerät nicht entladen wird.
pub fn forecast_from_slope(slope_pct_per_hour: f64, soc_percent: f32) -> Option<Forecast> {
    let rate = -slope_pct_per_hour;
    if !rate.is_finite() || rate < MIN_DISCHARGE_RATE {
        return None;
    }
    Some(Forecast {
        rate_pct_per_hour: rate as f32,
        hours_to_empty: (f64::from(soc_percent.max(0.0)) / rate) as f32,
    })
}

/// Prognose aus einem Verlauf (Zeitpunkt, Ladezustand). Der letzte Punkt gilt als aktueller Stand.
pub fn forecast_from_points(points: &[(DateTime<Utc>, f32)]) -> Option<Forecast> {
    let latest = points.iter().max_by_key(|p| p.0)?;
    let hours: Vec<(f64, f64)> = points
        .iter()
        .map(|(t, soc)| (t.timestamp_millis() as f64 / 3_600_000.0, f64::from(*soc)))
        .collect();
    forecast_from_slope(linear_slope(&hours)?, latest.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn series(start_soc: f32, rate_per_hour: f32, hours: i64) -> Vec<(DateTime<Utc>, f32)> {
        let t0 = DateTime::parse_from_rfc3339("2026-11-06T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        (0..=hours * 4)
            .map(|i| {
                let h = i as f32 / 4.0;
                (t0 + Duration::minutes(i * 15), start_soc - rate_per_hour * h)
            })
            .collect()
    }

    #[test]
    fn slope_of_a_straight_line() {
        let pts = [(0.0, 10.0), (1.0, 8.0), (2.0, 6.0)];
        assert!((linear_slope(&pts).unwrap() + 2.0).abs() < 1e-12);
    }

    #[test]
    fn slope_needs_two_distinct_x() {
        assert_eq!(linear_slope(&[]), None);
        assert_eq!(linear_slope(&[(1.0, 1.0)]), None);
        assert_eq!(linear_slope(&[(1.0, 1.0), (1.0, 5.0)]), None);
    }

    #[test]
    fn forecast_from_known_discharge() {
        let f = forecast_from_points(&series(80.0, 2.0, 12)).unwrap();
        assert!((f.rate_pct_per_hour - 2.0).abs() < 1e-3);
        // Nach 12 h sind 56 Prozent übrig, bei 2 Punkten pro Stunde noch 28 Stunden.
        assert!((f.hours_to_empty - 28.0).abs() < 0.01);
        assert!((f.hours_to_threshold(56.0, 20.0) - 18.0).abs() < 0.01);
    }

    #[test]
    fn no_forecast_when_not_discharging() {
        assert_eq!(forecast_from_points(&series(80.0, 0.0, 6)), None);
        assert_eq!(forecast_from_points(&series(50.0, -3.0, 6)), None, "Aufladen");
        assert_eq!(forecast_from_slope(f64::NAN, 50.0), None);
    }

    #[test]
    fn threshold_already_passed_gives_zero() {
        let f = Forecast {
            rate_pct_per_hour: 1.0,
            hours_to_empty: 15.0,
        };
        assert_eq!(f.hours_to_threshold(15.0, 20.0), 0.0);
    }

    #[test]
    fn unordered_points_use_the_latest_as_current() {
        let mut pts = series(80.0, 2.0, 6);
        pts.reverse();
        let f = forecast_from_points(&pts).unwrap();
        assert!((f.hours_to_empty - 34.0).abs() < 0.01);
    }
}
