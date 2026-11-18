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
    /// Vorsichtige Grenze: Mit 90 Prozent Wahrscheinlichkeit hält die Batterie mindestens so lange.
    #[serde(default)]
    pub hours_low: Option<f32>,
    /// Optimistische Grenze: Mit 90 Prozent Wahrscheinlichkeit ist die Batterie spätestens dann leer.
    #[serde(default)]
    pub hours_high: Option<f32>,
}

impl Forecast {
    /// Stunden bis zum Unterschreiten einer Schwelle (0, wenn schon darunter).
    pub fn hours_to_threshold(&self, soc_percent: f32, threshold_pct: f32) -> f32 {
        ((soc_percent - threshold_pct) / self.rate_pct_per_hour).max(0.0)
    }
}

/// z-Wert für die einseitigen 90-Prozent-Grenzen (zusammen eine 80-Prozent-Spanne).
pub const BAND_Z: f64 = 1.2816;
/// Die künftige Last kann von der der letzten 24 Stunden abweichen (Tageszeit, Wochentag, Wetter).
/// Als Anteil der Entladerate in die Unsicherheit eingerechnet.
pub const LOAD_VARIATION: f64 = 0.10;
/// Obergrenze der angegebenen Laufzeiten (ein Jahr): Bei kaum messbarer Entladung ist die Spanne sonst unendlich.
pub const HORIZON_CAP_HOURS: f64 = 24.0 * 365.0;

/// Restlaufzeit mit Spanne aus Ladezustand und Entladerate, jeweils mit Standardabweichung.
///
/// `margin_pct` ist der Abstand zum Ziel in Prozentpunkten (Ladezustand für "leer", Ladezustand minus Schwelle
/// für eine Schwelle). Die Laufzeit ist `margin / rate`. Die Streuung folgt aus der Fehlerfortpflanzung erster
/// Ordnung: `σ_T² = (σ_m / r)² + (m · σ_r / r²)²`. Gibt (Mitte, untere, obere Grenze) in Stunden zurück.
pub fn runtime_band(margin_pct: f64, margin_sigma: f64, rate: f64, rate_sigma: f64) -> (f64, f64, f64) {
    if margin_pct <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let hours = margin_pct / rate;
    let sigma = ((margin_sigma / rate).powi(2) + (margin_pct * rate_sigma / (rate * rate)).powi(2)).sqrt();
    (
        hours,
        (hours - BAND_Z * sigma).max(0.0),
        (hours + BAND_Z * sigma).min(HORIZON_CAP_HOURS),
    )
}

/// Prognose aus einer Steigung mit Standardfehler und einem Ladezustand mit Standardabweichung.
/// `None`, wenn das Gerät nicht entladen wird.
pub fn forecast_with_band(
    slope_pct_per_hour: f64,
    slope_std_error: f64,
    soc_percent: f32,
    soc_sigma: f32,
) -> Option<Forecast> {
    let base = forecast_from_slope(slope_pct_per_hour, soc_percent)?;
    let rate = f64::from(base.rate_pct_per_hour);
    let rate_sigma = (slope_std_error.max(0.0).powi(2) + (LOAD_VARIATION * rate).powi(2)).sqrt();
    let (_, low, high) = runtime_band(
        f64::from(soc_percent.max(0.0)),
        f64::from(soc_sigma.max(0.0)),
        rate,
        rate_sigma,
    );
    Some(Forecast {
        hours_low: Some(low as f32),
        hours_high: Some(high as f32),
        ..base
    })
}

/// Standardfehler der Steigung einer Ausgleichsgeraden aus den Summen, die die Datenbank liefert:
/// Bestimmtheitsmaß `r2`, Varianz von y und von x (jeweils Population) und Zahl der Punkte.
pub fn slope_std_error(r2: f64, var_y: f64, var_x: f64, n: i64) -> f64 {
    if n < 3 || var_x <= f64::EPSILON || !r2.is_finite() || !var_y.is_finite() {
        return f64::INFINITY;
    }
    (((1.0 - r2).max(0.0) * var_y) / (var_x * (n as f64 - 2.0))).sqrt()
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
        hours_low: None,
        hours_high: None,
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
            hours_low: None,
            hours_high: None,
        };
        assert_eq!(f.hours_to_threshold(15.0, 20.0), 0.0);
    }

    #[test]
    fn the_band_widens_with_the_uncertainty_and_never_goes_negative() {
        let (mid, low, high) = runtime_band(50.0, 3.0, 2.0, 0.2);
        assert!((mid - 25.0).abs() < 1e-9);
        assert!(low < mid && mid < high && low >= 0.0);
        let (_, low2, high2) = runtime_band(50.0, 9.0, 2.0, 0.2);
        assert!(low2 < low && high2 > high, "mehr Unsicherheit, breitere Spanne");
        let (_, low3, _) = runtime_band(5.0, 20.0, 2.0, 1.0);
        assert_eq!(low3, 0.0);
        assert_eq!(runtime_band(0.0, 3.0, 2.0, 0.2), (0.0, 0.0, 0.0));
    }

    #[test]
    fn the_band_holds_what_it_promises() {
        // Ladezustand und Rate zufällig aus ihren Unsicherheiten ziehen: Die wahre Laufzeit soll in etwa 80 Prozent
        // der Fälle in der Spanne liegen.
        let (soc, soc_sigma, rate, rate_sigma) = (60.0, 4.0, 1.5, 0.25);
        let (_, low, high) = runtime_band(soc, soc_sigma, rate, rate_sigma);
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut normal = || {
            let (u1, u2) = (next().max(1e-12), next());
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        };
        let trials = 20_000;
        let inside = (0..trials)
            .filter(|_| {
                let t = (soc + soc_sigma * normal()) / (rate + rate_sigma * normal());
                (low..=high).contains(&t)
            })
            .count();
        let share = inside as f64 / f64::from(trials);
        assert!((0.74..=0.88).contains(&share), "Anteil in der Spanne: {share:.3}");
    }

    #[test]
    fn a_forecast_with_band_keeps_the_plain_value_in_the_middle() {
        let f = forecast_with_band(-2.0, 0.05, 56.0, 3.0).unwrap();
        assert!((f.hours_to_empty - 28.0).abs() < 1e-3);
        assert!(f.hours_low.unwrap() < 28.0 && f.hours_high.unwrap() > 28.0);
        assert_eq!(forecast_with_band(0.5, 0.05, 56.0, 3.0), None, "lädt");
    }

    #[test]
    fn the_standard_error_of_a_clean_line_is_tiny_and_a_noisy_one_is_larger() {
        assert!(slope_std_error(1.0, 100.0, 50.0, 50) < 1e-9);
        let noisy = slope_std_error(0.5, 100.0, 50.0, 50);
        assert!(noisy > 0.01);
        assert!(slope_std_error(0.9, 100.0, 50.0, 2).is_infinite());
    }

    #[test]
    fn unordered_points_use_the_latest_as_current() {
        let mut pts = series(80.0, 2.0, 6);
        pts.reverse();
        let f = forecast_from_points(&pts).unwrap();
        assert!((f.hours_to_empty - 34.0).abs() < 0.01);
    }
}
