//! Fortschreibung des Ladezustands je Gerät.
//!
//! Die Rechnung steckt in [`shared::estimator`]: ein Kalman-Filter führt die Ladungsintegration der Vorlesung
//! (Trapezregel) und die Ruhespannung zusammen und gibt zu jedem Wert eine Standardabweichung an. Dieses Modul
//! verbindet das Filter mit dem, was die Datenbank je Gerät hält: dem gespeicherten Filterzustand, der seit dem
//! Einbau entnommenen Ladung (für den Vergleich der Batterien) und dem Startwert des Zyklus.

use chrono::{DateTime, Duration, Utc};
use shared::estimator::{Estimate, EstimatorState, Measurement, Verdict};

/// Gespeicherter Stand eines Geräts vor der neuen Messung.
#[derive(Debug, Clone, Default)]
pub struct DeviceSocState {
    /// Zustand des Filters. `None` bei einem neuen Gerät und nach einem Batteriewechsel ohne bekannten Ladezustand:
    /// Dann beginnt das Filter mit dieser Messung aus der Spannung.
    pub estimator: Option<EstimatorState>,
    pub soc_start_percent: Option<f32>,
    pub discharged_ah: f64,
}

/// Neuer Stand nach der Messung.
#[derive(Debug, Clone)]
pub struct SocUpdate {
    pub estimate: Estimate,
    pub estimator: EstimatorState,
    pub soc_start_percent: f32,
    pub discharged_ah: f64,
    pub verdict: Verdict,
}

/// Zeitstempel einer Messung: der Wert aus der Nachricht, wenn er plausibel ist
/// (höchstens ein Tag alt, höchstens fünf Minuten in der Zukunft), sonst der Empfangszeitpunkt.
pub fn effective_time(payload_time: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    match payload_time {
        Some(t) if t >= now - Duration::days(1) && t <= now + Duration::minutes(5) => t,
        _ => now,
    }
}

/// Verarbeitet eine Messung. `capacity_ah` ist die Nennkapazität aus den Einstellungen.
pub fn update_soc(prev: &DeviceSocState, measurement: &Measurement, capacity_ah: f64) -> SocUpdate {
    let (estimator, step) = match prev.estimator.clone() {
        Some(mut state) => {
            let step = state.step(measurement, capacity_ah);
            (state, step)
        }
        None => EstimatorState::start(measurement),
    };
    // Die entnommene Ladung zählt nur lückenlose Messungen. Über eine Lücke wird nichts erfunden, die Zahl ist dann
    // eine Untergrenze.
    let discharged_ah = prev.discharged_ah + step.drawn_ah;
    let soc_start_percent = prev.soc_start_percent.unwrap_or(step.estimate.soc_percent);
    SocUpdate {
        estimate: step.estimate,
        estimator,
        soc_start_percent,
        discharged_ah,
        verdict: step.verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::estimator::{KNOWN_SOC_SIGMA, ocv_mv};

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn meas(at: &str, mv: i32, ma: i32) -> Measurement {
        Measurement {
            at: t(at),
            voltage_mv: mv,
            current_ma: ma,
            temperature_c: Some(20.0),
        }
    }

    #[test]
    fn the_first_reading_starts_the_cycle_from_the_voltage_with_an_error() {
        let u = update_soc(
            &DeviceSocState::default(),
            &meas("2026-11-06T10:00:00Z", 11_952, -300),
            80.0,
        );
        assert_eq!(u.verdict, Verdict::Started);
        assert!((u.estimate.soc_percent - 52.7).abs() < 1.5, "{:?}", u.estimate);
        assert!(u.estimate.sigma_percent > 0.0);
        assert_eq!(u.soc_start_percent, u.estimate.soc_percent);
        assert_eq!(u.discharged_ah, 0.0);
    }

    #[test]
    fn later_readings_add_the_drawn_charge_by_the_trapezoid_rule() {
        let first = update_soc(
            &DeviceSocState::default(),
            &meas("2026-11-06T10:00:00Z", 12_300, -8_000),
            80.0,
        );
        let prev = DeviceSocState {
            estimator: Some(first.estimator),
            soc_start_percent: Some(first.soc_start_percent),
            discharged_ah: first.discharged_ah,
        };
        // 8 A über eine Viertelstunde sind 2 Ah.
        let next = update_soc(&prev, &meas("2026-11-06T10:15:00Z", 12_200, -8_000), 80.0);
        assert!((next.discharged_ah - 2.0).abs() < 1e-9);
        assert!(next.estimate.soc_percent < first.estimate.soc_percent + 1.0);
        assert_eq!(next.soc_start_percent, first.soc_start_percent);
    }

    #[test]
    fn a_data_gap_does_not_invent_drawn_charge() {
        let first = update_soc(
            &DeviceSocState::default(),
            &meas("2026-11-06T10:00:00Z", 12_300, -500),
            80.0,
        );
        let prev = DeviceSocState {
            estimator: Some(first.estimator),
            soc_start_percent: Some(first.soc_start_percent),
            discharged_ah: first.discharged_ah,
        };
        // Drei Tage später. Die alte Rechnung hätte 36 Ah entnommen und das Gerät als fast leer gemeldet.
        let next = update_soc(&prev, &meas("2026-11-09T10:00:00Z", 12_250, -500), 80.0);
        assert_eq!(next.discharged_ah, 0.0);
        assert!(next.estimate.soc_percent > 55.0, "{:?}", next.estimate);
    }

    #[test]
    fn a_stated_start_value_is_used_and_checked_against_the_voltage() {
        let prev = DeviceSocState {
            estimator: Some(EstimatorState::known(70.0, KNOWN_SOC_SIGMA)),
            soc_start_percent: Some(70.0),
            discharged_ah: 0.0,
        };
        let voltage = ocv_mv(70.0).round() as i32;
        let u = update_soc(&prev, &meas("2026-11-06T10:00:00Z", voltage, -300), 80.0);
        assert_eq!(u.verdict, Verdict::Fused);
        assert!((u.estimate.soc_percent - 70.0).abs() < 2.0);
        assert_eq!(u.soc_start_percent, 70.0);
    }

    #[test]
    fn a_stated_value_that_the_voltage_contradicts_is_replaced_after_repeated_readings() {
        let mut prev = DeviceSocState {
            estimator: Some(EstimatorState::known(100.0, KNOWN_SOC_SIGMA)),
            soc_start_percent: Some(100.0),
            discharged_ah: 0.0,
        };
        let mut verdicts = Vec::new();
        for i in 0..14 {
            let at = t("2026-11-06T10:00:00Z") + Duration::minutes(15 * i);
            let m = Measurement {
                at,
                voltage_mv: 11_952,
                current_ma: -300,
                temperature_c: Some(20.0),
            };
            let u = update_soc(&prev, &m, 80.0);
            verdicts.push(u.verdict);
            prev = DeviceSocState {
                estimator: Some(u.estimator),
                soc_start_percent: Some(u.soc_start_percent),
                discharged_ah: u.discharged_ah,
            };
        }
        assert_eq!(verdicts[0], Verdict::Rejected);
        assert!(verdicts.contains(&Verdict::Restarted), "{verdicts:?}");
    }

    #[test]
    fn out_of_order_readings_change_nothing() {
        let first = update_soc(
            &DeviceSocState::default(),
            &meas("2026-11-06T10:00:00Z", 12_300, -300),
            80.0,
        );
        let prev = DeviceSocState {
            estimator: Some(first.estimator.clone()),
            soc_start_percent: Some(first.soc_start_percent),
            discharged_ah: 5.0,
        };
        let late = update_soc(&prev, &meas("2026-11-06T09:00:00Z", 12_000, -300), 80.0);
        assert_eq!(late.verdict, Verdict::Late);
        assert_eq!(late.discharged_ah, 5.0);
        assert_eq!(late.estimator, first.estimator);
    }

    #[test]
    fn a_legacy_device_keeps_its_cycle_while_the_filter_starts() {
        // Gerät aus der Zeit vor dem Filter: Startwert und entnommene Ladung bleiben, nur der Filter beginnt.
        let prev = DeviceSocState {
            estimator: None,
            soc_start_percent: Some(90.0),
            discharged_ah: 12.0,
        };
        let u = update_soc(&prev, &meas("2026-11-06T10:00:00Z", 12_300, -300), 80.0);
        assert_eq!(u.verdict, Verdict::Started);
        assert_eq!((u.soc_start_percent, u.discharged_ah), (90.0, 12.0));
    }

    #[test]
    fn effective_time_prefers_plausible_payload_time() {
        let now = t("2026-11-06T12:00:00Z");
        assert_eq!(
            effective_time(Some(t("2026-11-06T11:59:00Z")), now),
            t("2026-11-06T11:59:00Z")
        );
        assert_eq!(effective_time(None, now), now);
        assert_eq!(
            effective_time(Some(t("2026-11-05T11:00:00Z")), now),
            now,
            "älter als ein Tag"
        );
        assert_eq!(
            effective_time(Some(t("2026-11-06T12:30:00Z")), now),
            now,
            "zu weit in der Zukunft"
        );
    }
}
