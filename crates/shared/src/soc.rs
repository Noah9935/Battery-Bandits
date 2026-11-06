//! Ladezustand (SoC) nach der Vorlesung "Einschub Batteriemessung".
//!
//! * Entnommene Ladung per Trapezregel: `ΔQ = (I1 + I2) / 2 · Δt`.
//! * `SOC(t) = SOC(Start) − 100 · Q(t) / Nennkapazität`.
//! * Der Startwert nach einem Batteriewechsel ist eine Annahme: lineare
//!   Schätzung aus der Ruhespannung zwischen Mindest- und Höchstspannung.

/// Schätzt den Ladezustand in Prozent linear aus der Spannung (0 bis 100).
///
/// Annahme (nicht aus der Vorlesung): Die Spannung einer ruhenden Batterie
/// verläuft zwischen `min_mv` (leer) und `max_mv` (voll) näherungsweise linear.
/// Ungültige Grenzen (`max_mv <= min_mv`) ergeben 0.
pub fn calculate_soc(voltage_mv: i32, min_mv: i32, max_mv: i32) -> f32 {
    if max_mv <= min_mv {
        return 0.0;
    }
    let span = f64::from(max_mv) - f64::from(min_mv);
    let pct = (f64::from(voltage_mv) - f64::from(min_mv)) / span * 100.0;
    pct.clamp(0.0, 100.0) as f32
}

/// Entnommene Ladung in Amperestunden zwischen zwei Messungen (Trapezregel).
///
/// Gerechnet wird mit dem Betrag des Stroms, weil das Vorzeichen je nach Quelle
/// unterschiedlich ist (live negativ, Vorlesungsdaten positiv). Negative
/// Zeitspannen zählen als 0.
pub fn discharged_ah(previous_ma: i32, current_ma: i32, seconds: f64) -> f64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0.0;
    }
    let mean_ma = (f64::from(previous_ma.unsigned_abs()) + f64::from(current_ma.unsigned_abs())) / 2.0;
    mean_ma / 1000.0 * seconds / 3600.0
}

/// Ladezustand nach einer insgesamt entnommenen Ladung (Prozent, 0 bis 100).
pub fn soc_after_discharge(start_pct: f32, discharged_ah: f64, capacity_ah: f64) -> f32 {
    if capacity_ah <= 0.0 {
        return 0.0;
    }
    let pct = f64::from(start_pct) - 100.0 * discharged_ah / capacity_ah;
    pct.clamp(0.0, 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i32 = 10_500;
    const MAX: i32 = 12_700;

    #[test]
    fn soc_is_zero_at_min_and_hundred_at_max() {
        assert_eq!(calculate_soc(MIN, MIN, MAX), 0.0);
        assert_eq!(calculate_soc(MAX, MIN, MAX), 100.0);
    }

    #[test]
    fn soc_is_linear_in_between() {
        assert!((calculate_soc(11_600, MIN, MAX) - 50.0).abs() < 1e-4);
        assert!((calculate_soc(12_296, MIN, MAX) - 81.636).abs() < 0.01);
    }

    #[test]
    fn soc_is_clamped() {
        assert_eq!(calculate_soc(9_000, MIN, MAX), 0.0);
        assert_eq!(calculate_soc(14_000, MIN, MAX), 100.0);
    }

    #[test]
    fn soc_with_invalid_bounds_is_zero() {
        assert_eq!(calculate_soc(12_000, MAX, MIN), 0.0);
        assert_eq!(calculate_soc(12_000, MIN, MIN), 0.0);
    }

    #[test]
    fn trapezoid_matches_lecture_formula() {
        // 1000 mA gleichbleibend über eine Stunde sind 1 Ah.
        assert!((discharged_ah(1000, 1000, 3600.0) - 1.0).abs() < 1e-9);
        // Rampe von 0 auf 2000 mA: Mittelwert 1000 mA.
        assert!((discharged_ah(0, 2000, 3600.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn discharge_ignores_current_sign() {
        assert_eq!(discharged_ah(-500, -500, 7200.0), discharged_ah(500, 500, 7200.0));
        assert!((discharged_ah(-500, -500, 7200.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn discharge_for_non_positive_time_is_zero() {
        assert_eq!(discharged_ah(1000, 1000, 0.0), 0.0);
        assert_eq!(discharged_ah(1000, 1000, -5.0), 0.0);
        assert_eq!(discharged_ah(1000, 1000, f64::NAN), 0.0);
    }

    #[test]
    fn lecture_example_65_08_ah_of_80_ah() {
        // Vorlesung: 65,08 Ah entnommen, das sind 81,35 % der Nennkapazität.
        let soc = soc_after_discharge(100.0, 65.08, 80.0);
        assert!((soc - 18.65).abs() < 0.01);
    }

    #[test]
    fn same_consumption_from_other_start_gives_other_soc() {
        let high = soc_after_discharge(100.0, 20.0, 80.0);
        let low = soc_after_discharge(60.0, 20.0, 80.0);
        assert!((high - 75.0).abs() < 1e-4);
        assert!((low - 35.0).abs() < 1e-4);
    }

    #[test]
    fn soc_after_discharge_is_clamped() {
        assert_eq!(soc_after_discharge(50.0, 200.0, 80.0), 0.0);
        assert_eq!(soc_after_discharge(100.0, -10.0, 80.0), 100.0);
        assert_eq!(soc_after_discharge(100.0, 1.0, 0.0), 0.0);
    }
}
