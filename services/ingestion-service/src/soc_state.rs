//! Fortschreibung des Ladezustands je Gerät (reine Funktionen).
//!
//! Nach der Vorlesung wird die entnommene Ladung aus dem Strom integriert
//! (Trapezregel) und vom Startwert abgezogen. Den Startwert schätzen wir aus der
//! ersten gemessenen Spannung (Annahme, siehe `shared::soc::calculate_soc`).

use chrono::{DateTime, Duration, Utc};
use shared::Settings;
use shared::soc::{calculate_soc, discharged_ah, soc_after_discharge};

/// Gespeicherter Stand eines Geräts vor der neuen Messung.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceSocState {
    pub last_seen_at: Option<DateTime<Utc>>,
    pub current_ma: Option<i32>,
    pub soc_start_percent: Option<f32>,
    pub discharged_ah: f64,
}

/// Neuer Stand nach der Messung.
#[derive(Debug, Clone, PartialEq)]
pub struct SocUpdate {
    pub soc_percent: f32,
    pub soc_start_percent: f32,
    pub discharged_ah: f64,
    /// Wurde ein neuer Zyklus begonnen (erste Messung oder Batteriewechsel)?
    pub started_new_cycle: bool,
}

/// Zeitstempel einer Messung: der Wert aus der Nachricht, wenn er plausibel ist
/// (höchstens ein Tag alt, höchstens fünf Minuten in der Zukunft), sonst der Empfangszeitpunkt.
pub fn effective_time(payload_time: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    match payload_time {
        Some(t) if t >= now - Duration::days(1) && t <= now + Duration::minutes(5) => t,
        _ => now,
    }
}

pub fn update_soc(
    prev: &DeviceSocState,
    voltage_mv: i32,
    current_ma: i32,
    at: DateTime<Utc>,
    settings: &Settings,
) -> SocUpdate {
    let capacity = f64::from(settings.battery_capacity_ah);
    let start_from_voltage = || {
        let start = calculate_soc(voltage_mv, settings.min_voltage_mv, settings.max_voltage_mv);
        SocUpdate {
            soc_percent: start,
            soc_start_percent: start,
            discharged_ah: 0.0,
            started_new_cycle: true,
        }
    };
    let Some(start) = prev.soc_start_percent else {
        return start_from_voltage();
    };
    let (Some(last), Some(prev_current)) = (prev.last_seen_at, prev.current_ma) else {
        // Der Startwert ist bekannt (beim Batteriewechsel angegeben), es gibt aber noch keine Messung in diesem
        // Zyklus: Die Integration beginnt mit dieser Messung, der angegebene Startwert bleibt.
        return SocUpdate {
            soc_percent: soc_after_discharge(start, prev.discharged_ah, capacity),
            soc_start_percent: start,
            discharged_ah: prev.discharged_ah,
            started_new_cycle: true,
        };
    };
    let seconds = (at - last).num_milliseconds() as f64 / 1000.0;
    let total = prev.discharged_ah + discharged_ah(prev_current, current_ma, seconds);
    SocUpdate {
        soc_percent: soc_after_discharge(start, total, capacity),
        soc_start_percent: start,
        discharged_ah: total,
        started_new_cycle: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn state(last: &str, current: i32, start: f32, ah: f64) -> DeviceSocState {
        DeviceSocState {
            last_seen_at: Some(t(last)),
            current_ma: Some(current),
            soc_start_percent: Some(start),
            discharged_ah: ah,
        }
    }

    #[test]
    fn first_reading_starts_cycle_from_voltage() {
        let u = update_soc(
            &DeviceSocState::default(),
            11_600,
            -300,
            t("2026-11-06T10:00:00Z"),
            &Settings::default(),
        );
        assert!(u.started_new_cycle);
        assert!((u.soc_percent - 50.0).abs() < 1e-3);
        assert_eq!(u.soc_start_percent, u.soc_percent);
        assert_eq!(u.discharged_ah, 0.0);
    }

    #[test]
    fn a_stated_start_value_survives_the_first_reading_of_the_new_cycle() {
        // Nach der Schnellerfassung: 100 Prozent angegeben, Strom noch unbekannt. Die Spannung der ersten
        // Messung (hier nur halb voll) darf den angegebenen Wert nicht überschreiben.
        let prev = DeviceSocState {
            last_seen_at: Some(t("2026-11-01T10:00:00Z")),
            current_ma: None,
            soc_start_percent: Some(100.0),
            discharged_ah: 0.0,
        };
        let first = update_soc(&prev, 11_600, -800, t("2026-11-06T10:00:00Z"), &Settings::default());
        assert!(first.started_new_cycle);
        assert_eq!(
            (first.soc_percent, first.soc_start_percent, first.discharged_ah),
            (100.0, 100.0, 0.0)
        );

        // Danach wird ganz normal integriert: 800 mA über eine Stunde sind 0,8 Ah, also ein Prozentpunkt.
        let next = DeviceSocState {
            last_seen_at: Some(t("2026-11-06T10:00:00Z")),
            current_ma: Some(-800),
            soc_start_percent: Some(first.soc_start_percent),
            discharged_ah: first.discharged_ah,
        };
        let second = update_soc(&next, 12_600, -800, t("2026-11-06T11:00:00Z"), &Settings::default());
        assert!(!second.started_new_cycle);
        assert!((second.soc_percent - 99.0).abs() < 1e-3);
    }

    #[test]
    fn later_reading_integrates_current_over_time() {
        let prev = state("2026-11-06T10:00:00Z", -8_000, 100.0, 0.0);
        // 8 A über eine Stunde sind 8 Ah, das sind 10 % von 80 Ah.
        let u = update_soc(&prev, 12_000, -8_000, t("2026-11-06T11:00:00Z"), &Settings::default());
        assert!(!u.started_new_cycle);
        assert!((u.discharged_ah - 8.0).abs() < 1e-9);
        assert!((u.soc_percent - 90.0).abs() < 1e-3);
    }

    #[test]
    fn accumulates_across_readings() {
        let settings = Settings::default();
        let mut current = state("2026-11-06T10:00:00Z", 4_000, 80.0, 0.0);
        for minute in [30, 60, 90, 120] {
            let at = t("2026-11-06T10:00:00Z") + Duration::minutes(minute);
            let u = update_soc(&current, 12_000, 4_000, at, &settings);
            current = DeviceSocState {
                last_seen_at: Some(at),
                current_ma: Some(4_000),
                soc_start_percent: Some(u.soc_start_percent),
                discharged_ah: u.discharged_ah,
            };
        }
        // 4 A über 2 Stunden: 8 Ah, 10 Prozentpunkte.
        assert!((current.discharged_ah - 8.0).abs() < 1e-9);
        let end = update_soc(&current, 12_000, 4_000, t("2026-11-06T12:00:00Z"), &settings);
        assert!((end.soc_percent - 70.0).abs() < 1e-3);
    }

    #[test]
    fn out_of_order_time_changes_nothing() {
        let prev = state("2026-11-06T10:00:00Z", -300, 60.0, 5.0);
        let u = update_soc(&prev, 12_000, -300, t("2026-11-06T09:00:00Z"), &Settings::default());
        assert_eq!(u.discharged_ah, 5.0);
        assert!((u.soc_percent - 53.75).abs() < 1e-3);
    }

    #[test]
    fn soc_never_drops_below_zero() {
        let prev = state("2026-11-06T10:00:00Z", 50_000, 5.0, 0.0);
        let u = update_soc(&prev, 10_000, 50_000, t("2026-11-06T12:00:00Z"), &Settings::default());
        assert_eq!(u.soc_percent, 0.0);
    }

    #[test]
    fn missing_history_fields_restart_the_cycle() {
        let prev = DeviceSocState {
            last_seen_at: Some(t("2026-11-06T10:00:00Z")),
            ..Default::default()
        };
        let u = update_soc(&prev, 12_000, -300, t("2026-11-06T11:00:00Z"), &Settings::default());
        assert!(u.started_new_cycle);
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
