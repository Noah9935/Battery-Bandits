//! Effizienz einer Batterie: Wie viel leistet sie wirklich, verglichen mit ihrer Nennkapazität?
//!
//! Der Ladezustand im System entsteht durch Stromintegration gegen die Nennkapazität (80 Ah). Er sagt
//! deshalb nichts darüber, ob die Batterie diese Kapazität noch hat. Die Spannung ist die zweite,
//! unabhängige Messgröße: Fällt sie schneller, als die entnommene Ladung erwarten lässt, ist die Batterie
//! schwächer als auf dem Papier (Alter, Kälte, hohe Last).
//!
//! * Nennlaufzeit: Nennkapazität geteilt durch den mittleren Strom.
//! * Reale Laufzeit: nutzbare Spannungsspanne (voll bis leer) geteilt durch den Spannungsabfall pro Tag.
//! * Effizienz: reale Laufzeit im Verhältnis zur Nennlaufzeit, in Prozent.
//!
//! Annahme wie beim Startwert des Ladezustands: Die Spannung fällt zwischen voll und leer näherungsweise
//! linear. Die Werte sind deshalb Schätzungen und als solche in der Oberfläche benannt.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::DeviceStatus;
use crate::settings::Settings;

/// Unter diesem mittleren Strom (mA) gilt ein Gerät als nicht belastet: keine Aussage möglich.
pub const MIN_CURRENT_MA: f64 = 1.0;
/// Unter diesem Spannungsabfall (mV pro Tag) gilt die Batterie als nicht entladen: keine Aussage möglich.
pub const MIN_DROP_MV_PER_DAY: f64 = 1.0;
/// Obergrenze der Effizienz in Prozent. Werte darüber sind Messrauschen, keine Wunderbatterien.
pub const MAX_SCORE_PCT: f64 = 200.0;

/// Ergebnis der Auswertung. Jede Angabe fehlt, wenn die Daten sie nicht hergeben.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Efficiency {
    /// Mittlerer Strom in mA (Betrag).
    pub avg_current_ma: Option<f64>,
    /// Spannungsabfall in mV pro 24 Stunden (positiv bedeutet fallend).
    pub voltage_drop_mv_per_day: Option<f64>,
    /// Laufzeit einer vollen Batterie laut Nennkapazität, in Stunden.
    pub nominal_runtime_hours: Option<f64>,
    /// Laufzeit einer vollen Batterie laut Spannungsverlauf, in Stunden.
    pub real_runtime_hours: Option<f64>,
    /// Reale Laufzeit im Verhältnis zur Nennlaufzeit, in Prozent.
    pub score_pct: Option<f32>,
}

/// Rechnet aus mittlerem Strom und Spannungsabfall die Effizienz.
///
/// `avg_current_ma` darf ein Vorzeichen haben (je nach Quelle verschieden), gerechnet wird mit dem Betrag.
/// `voltage_drop_mv_per_day` ist positiv, wenn die Spannung fällt.
pub fn efficiency(
    avg_current_ma: Option<f64>,
    voltage_drop_mv_per_day: Option<f64>,
    settings: &Settings,
) -> Efficiency {
    let current = avg_current_ma
        .map(f64::abs)
        .filter(|c| c.is_finite() && *c >= MIN_CURRENT_MA);
    let drop = voltage_drop_mv_per_day.filter(|d| d.is_finite() && *d >= MIN_DROP_MV_PER_DAY);
    let span_mv = f64::from(settings.max_voltage_mv) - f64::from(settings.min_voltage_mv);
    let nominal = current.map(|c| f64::from(settings.battery_capacity_ah) * 1000.0 / c);
    let real = drop.filter(|_| span_mv > 0.0).map(|d| span_mv / d * 24.0);
    let score = match (nominal, real) {
        (Some(n), Some(r)) if n > 0.0 => Some((r / n * 100.0).clamp(0.0, MAX_SCORE_PCT) as f32),
        _ => None,
    };
    Efficiency {
        avg_current_ma: current,
        voltage_drop_mv_per_day: voltage_drop_mv_per_day.filter(|d| d.is_finite()),
        nominal_runtime_hours: nominal,
        real_runtime_hours: real,
        score_pct: score,
    }
}

/// Effizienz eines abgeschlossenen Zyklus aus seinen Eckwerten: Dauer, entnommene Ladung, Spannung am Anfang
/// und am Ende. Der mittlere Strom ist hier exakt (Ladung durch Zeit), der Spannungsabfall die Differenz.
pub fn cycle_efficiency(
    hours: f64,
    discharged_ah: f64,
    start_voltage_mv: Option<i32>,
    end_voltage_mv: Option<i32>,
    settings: &Settings,
) -> Efficiency {
    if !hours.is_finite() || hours <= 0.0 {
        return Efficiency::default();
    }
    let current = Some(discharged_ah * 1000.0 / hours);
    let drop = match (start_voltage_mv, end_voltage_mv) {
        (Some(start), Some(end)) => Some((f64::from(start) - f64::from(end)) / hours * 24.0),
        _ => None,
    };
    efficiency(current, drop, settings)
}

/// Einordnung der Effizienz in Worte.
pub fn score_label(score_pct: f32) -> &'static str {
    if score_pct >= 90.0 {
        "wie neu"
    } else if score_pct >= 70.0 {
        "gut"
    } else if score_pct >= 50.0 {
        "nachlassend"
    } else {
        "schwach"
    }
}

/// Auswertung eines Geräts über den laufenden Zyklus (höchstens die letzten 30 Tage).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceEfficiency {
    pub device_id: String,
    pub name: Option<String>,
    pub status: DeviceStatus,
    /// Länge des ausgewerteten Zeitraums in Stunden.
    pub window_hours: f64,
    /// Zahl der Messwerte im Zeitraum.
    pub readings: i64,
    pub avg_temperature_c: Option<f32>,
    #[serde(flatten)]
    pub result: Efficiency,
}

/// Datenbestand: Was liegt in der Datenbank?
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DataStock {
    /// Gespeicherte Messwerte (bei sehr großen Beständen eine Schätzung der Datenbank).
    pub readings_total: i64,
    pub oldest_reading: Option<DateTime<Utc>>,
    pub newest_reading: Option<DateTime<Utc>>,
    /// Verworfene Nachrichten, die mit Grund protokolliert sind.
    pub invalid_messages: i64,
}

/// Antwort von `/api/efficiency`: die ganze Flotte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetEfficiency {
    pub devices: Vec<DeviceEfficiency>,
    pub stock: DataStock,
    pub capacity_ah: f32,
}

/// Messwerte eines Temperaturbereichs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemperatureBand {
    /// Untere Grenze in Grad Celsius, `None` für "darunter".
    pub from_c: Option<f32>,
    /// Obere Grenze in Grad Celsius, `None` für "darüber".
    pub to_c: Option<f32>,
    pub readings: i64,
    pub avg_current_ma: f64,
    pub avg_voltage_mv: f64,
}

/// Grenzen der Temperaturbereiche in Grad Celsius.
pub const BAND_EDGES_C: [f32; 4] = [0.0, 10.0, 20.0, 30.0];

/// Nummer des Temperaturbereichs (0 bis 4) für eine Temperatur.
pub fn band_index(temperature_c: f32) -> usize {
    BAND_EDGES_C.iter().take_while(|edge| temperature_c >= **edge).count()
}

/// Grenzen eines Temperaturbereichs nach seiner Nummer.
pub fn band_bounds(index: usize) -> (Option<f32>, Option<f32>) {
    let from = index.checked_sub(1).and_then(|i| BAND_EDGES_C.get(i).copied());
    let to = BAND_EDGES_C.get(index).copied();
    (from, to)
}

/// Beschriftung eines Temperaturbereichs.
pub fn band_label(from_c: Option<f32>, to_c: Option<f32>) -> String {
    match (from_c, to_c) {
        (None, Some(to)) => format!("unter {to:.0} °C"),
        (Some(from), Some(to)) => format!("{from:.0} bis {to:.0} °C"),
        (Some(from), None) => format!("ab {from:.0} °C"),
        (None, None) => "alle Temperaturen".to_owned(),
    }
}

/// Vergleich zwischen hoher und üblicher Last. "Hoch" ist mehr als das Anderthalbfache des mittleren Stroms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadSplit {
    pub median_current_ma: f64,
    pub high_readings: i64,
    pub high_avg_voltage_mv: Option<f64>,
    pub normal_readings: i64,
    pub normal_avg_voltage_mv: Option<f64>,
}

/// Faktor, ab dem ein Strom als hohe Last zählt (bezogen auf den Median des Geräts).
pub const HIGH_LOAD_FACTOR: f64 = 1.5;

/// Ein Batteriezyklus im Vergleich. Der laufende Zyklus hat kein Ende.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleSummary {
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub hours: f64,
    pub discharged_ah: f64,
    pub avg_temperature_c: Option<f32>,
    #[serde(flatten)]
    pub result: Efficiency,
}

/// Antwort von `/api/devices/{id}/efficiency`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EfficiencyDetail {
    pub current: DeviceEfficiency,
    pub bands: Vec<TemperatureBand>,
    pub load: Option<LoadSplit>,
    /// Neueste zuerst. Der laufende Zyklus steht an erster Stelle.
    pub cycles: Vec<CycleSummary>,
}

/// Wie viel Spannung kostet Kälte? Differenz der mittleren Spannung zwischen dem wärmsten und dem kältesten
/// Bereich mit genügend Messwerten, in mV (positiv: bei Kälte liegt die Spannung tiefer).
pub fn cold_effect_mv(bands: &[TemperatureBand], min_readings: i64) -> Option<f64> {
    let usable: Vec<&TemperatureBand> = bands.iter().filter(|b| b.readings >= min_readings).collect();
    let (coldest, warmest) = (usable.first()?, usable.last()?);
    if std::ptr::eq(*coldest, *warmest) {
        return None;
    }
    Some(warmest.avg_voltage_mv - coldest.avg_voltage_mv)
}

/// Wie viel Spannung kostet hohe Last? Differenz der mittleren Spannung zwischen üblicher und hoher Last in mV
/// (positiv: unter hoher Last liegt die Spannung tiefer).
pub fn load_effect_mv(load: &LoadSplit, min_readings: i64) -> Option<f64> {
    if load.high_readings < min_readings || load.normal_readings < min_readings {
        return None;
    }
    Some(load.normal_avg_voltage_mv? - load.high_avg_voltage_mv?)
}

/// Gesundheit im Vergleich: Effizienz des laufenden Zyklus im Verhältnis zum Mittel der früheren, in Prozent.
pub fn health_vs_previous(cycles: &[CycleSummary]) -> Option<f32> {
    let current = cycles.iter().find(|c| c.ended_at.is_none())?.result.score_pct?;
    let earlier: Vec<f32> = cycles
        .iter()
        .filter(|c| c.ended_at.is_some())
        .filter_map(|c| c.result.score_pct)
        .collect();
    if earlier.is_empty() {
        return None;
    }
    let mean = earlier.iter().sum::<f32>() / earlier.len() as f32;
    (mean > 0.0).then(|| current / mean * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings::default()
    }

    fn band(from: Option<f32>, to: Option<f32>, readings: i64, mv: f64) -> TemperatureBand {
        TemperatureBand {
            from_c: from,
            to_c: to,
            readings,
            avg_current_ma: 300.0,
            avg_voltage_mv: mv,
        }
    }

    fn cycle(ended: bool, score: Option<f32>) -> CycleSummary {
        let at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        CycleSummary {
            started_at: at,
            ended_at: ended.then_some(at),
            hours: 100.0,
            discharged_ah: 30.0,
            avg_temperature_c: None,
            result: Efficiency {
                score_pct: score,
                ..Efficiency::default()
            },
        }
    }

    #[test]
    fn a_battery_that_matches_its_rating_scores_one_hundred() {
        // 80 Ah bei 800 mA halten 100 Stunden. Fällt die Spannung in dieser Zeit um die ganze Spanne
        // (2200 mV), sind das 528 mV pro Tag.
        let e = efficiency(Some(800.0), Some(528.0), &settings());
        assert!((e.nominal_runtime_hours.unwrap() - 100.0).abs() < 1e-9);
        assert!((e.real_runtime_hours.unwrap() - 100.0).abs() < 1e-9);
        assert!((e.score_pct.unwrap() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn a_faster_voltage_drop_means_a_weaker_battery() {
        let weak = efficiency(Some(800.0), Some(1056.0), &settings());
        assert!((weak.score_pct.unwrap() - 50.0).abs() < 1e-3);
        assert!((weak.real_runtime_hours.unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn the_sign_of_the_current_does_not_matter() {
        let a = efficiency(Some(-800.0), Some(528.0), &settings());
        let b = efficiency(Some(800.0), Some(528.0), &settings());
        assert_eq!(a, b);
        assert_eq!(a.avg_current_ma, Some(800.0));
    }

    #[test]
    fn without_load_or_without_a_drop_there_is_no_score() {
        let idle = efficiency(Some(0.2), Some(528.0), &settings());
        assert_eq!(
            (idle.avg_current_ma, idle.nominal_runtime_hours, idle.score_pct),
            (None, None, None)
        );
        assert!(idle.real_runtime_hours.is_some());

        let charging = efficiency(Some(800.0), Some(-40.0), &settings());
        assert_eq!((charging.real_runtime_hours, charging.score_pct), (None, None));
        assert_eq!(
            charging.voltage_drop_mv_per_day,
            Some(-40.0),
            "der Messwert selbst bleibt sichtbar"
        );

        let nothing = efficiency(None, None, &settings());
        assert_eq!(nothing, Efficiency::default());
        let broken = efficiency(Some(f64::NAN), Some(f64::INFINITY), &settings());
        assert_eq!(broken.score_pct, None);
        assert_eq!(broken.voltage_drop_mv_per_day, None);
    }

    #[test]
    fn the_score_is_capped() {
        let noisy = efficiency(Some(800.0), Some(1.5), &settings());
        assert_eq!(noisy.score_pct, Some(MAX_SCORE_PCT as f32));
    }

    #[test]
    fn invalid_voltage_bounds_give_no_real_runtime() {
        let flat = Settings {
            min_voltage_mv: 12_000,
            max_voltage_mv: 12_000,
            ..settings()
        };
        let e = efficiency(Some(800.0), Some(500.0), &flat);
        assert_eq!((e.real_runtime_hours, e.score_pct), (None, None));
    }

    #[test]
    fn a_finished_cycle_is_rated_from_its_corner_values() {
        // 100 Stunden, 80 Ah entnommen, Spannung von 12,7 auf 10,5 V: genau nach Datenblatt.
        let e = cycle_efficiency(100.0, 80.0, Some(12_700), Some(10_500), &settings());
        assert!((e.avg_current_ma.unwrap() - 800.0).abs() < 1e-9);
        assert!((e.voltage_drop_mv_per_day.unwrap() - 528.0).abs() < 1e-9);
        assert!((e.score_pct.unwrap() - 100.0).abs() < 1e-3);

        // Dieselbe Spannung, aber nur 40 Ah entnommen: halbe Kapazität.
        let half = cycle_efficiency(50.0, 40.0, Some(12_700), Some(10_500), &settings());
        assert!((half.score_pct.unwrap() - 50.0).abs() < 1e-3);
    }

    #[test]
    fn a_cycle_without_duration_or_voltage_has_no_score() {
        assert_eq!(
            cycle_efficiency(0.0, 10.0, Some(12_700), Some(12_000), &settings()),
            Efficiency::default()
        );
        assert_eq!(
            cycle_efficiency(f64::NAN, 10.0, None, None, &settings()),
            Efficiency::default()
        );
        let no_voltage = cycle_efficiency(10.0, 8.0, None, Some(12_000), &settings());
        assert!(no_voltage.avg_current_ma.is_some());
        assert_eq!(no_voltage.score_pct, None);
    }

    #[test]
    fn scores_have_words() {
        assert_eq!(score_label(104.0), "wie neu");
        assert_eq!(score_label(90.0), "wie neu");
        assert_eq!(score_label(75.0), "gut");
        assert_eq!(score_label(55.0), "nachlassend");
        assert_eq!(score_label(20.0), "schwach");
    }

    #[test]
    fn temperatures_fall_into_five_bands() {
        assert_eq!(band_index(-5.0), 0);
        assert_eq!(band_index(0.0), 1);
        assert_eq!(band_index(9.9), 1);
        assert_eq!(band_index(10.0), 2);
        assert_eq!(band_index(25.0), 3);
        assert_eq!(band_index(30.0), 4);
        assert_eq!(band_bounds(0), (None, Some(0.0)));
        assert_eq!(band_bounds(2), (Some(10.0), Some(20.0)));
        assert_eq!(band_bounds(4), (Some(30.0), None));
        for t in [-20.0_f32, 0.0, 5.0, 15.0, 29.9, 45.0] {
            let (from, to) = band_bounds(band_index(t));
            assert!(from.is_none_or(|f| t >= f) && to.is_none_or(|u| t < u), "{t}");
        }
    }

    #[test]
    fn bands_have_readable_labels() {
        assert_eq!(band_label(None, Some(0.0)), "unter 0 °C");
        assert_eq!(band_label(Some(10.0), Some(20.0)), "10 bis 20 °C");
        assert_eq!(band_label(Some(30.0), None), "ab 30 °C");
        assert_eq!(band_label(None, None), "alle Temperaturen");
    }

    #[test]
    fn cold_costs_voltage() {
        let bands = [
            band(None, Some(0.0), 40, 11_900.0),
            band(Some(0.0), Some(10.0), 3, 5_000.0),
            band(Some(10.0), Some(20.0), 60, 12_150.0),
        ];
        // Der mittlere Bereich hat zu wenige Messwerte und zählt nicht.
        assert_eq!(cold_effect_mv(&bands, 10), Some(250.0));
        assert_eq!(
            cold_effect_mv(&bands[..1], 10),
            None,
            "ein Bereich allein ist kein Vergleich"
        );
        assert_eq!(cold_effect_mv(&[], 10), None);
        assert_eq!(cold_effect_mv(&bands, 100), None);
    }

    #[test]
    fn high_load_costs_voltage() {
        let load = LoadSplit {
            median_current_ma: 300.0,
            high_readings: 20,
            high_avg_voltage_mv: Some(11_800.0),
            normal_readings: 200,
            normal_avg_voltage_mv: Some(12_100.0),
        };
        assert_eq!(load_effect_mv(&load, 10), Some(300.0));
        assert_eq!(load_effect_mv(&load, 50), None, "zu wenige Messwerte unter hoher Last");
        let no_high = LoadSplit {
            high_avg_voltage_mv: None,
            ..load
        };
        assert_eq!(load_effect_mv(&no_high, 10), None);
    }

    #[test]
    fn health_compares_the_running_cycle_with_earlier_ones() {
        let cycles = [
            cycle(false, Some(72.0)),
            cycle(true, Some(90.0)),
            cycle(true, Some(90.0)),
        ];
        assert!((health_vs_previous(&cycles).unwrap() - 80.0).abs() < 1e-3);
        assert_eq!(
            health_vs_previous(&cycles[..1]),
            None,
            "ohne früheren Zyklus kein Vergleich"
        );
        assert_eq!(health_vs_previous(&[cycle(false, None), cycle(true, Some(90.0))]), None);
        assert_eq!(
            health_vs_previous(&[cycle(true, Some(90.0))]),
            None,
            "kein laufender Zyklus"
        );
        assert_eq!(health_vs_previous(&[cycle(false, Some(72.0)), cycle(true, None)]), None);
    }

    #[test]
    fn the_detail_roundtrips_through_json_with_flat_results() {
        let detail = DeviceEfficiency {
            device_id: "TN0001".into(),
            name: None,
            status: DeviceStatus::Normal,
            window_hours: 48.0,
            readings: 500,
            avg_temperature_c: Some(12.5),
            result: efficiency(Some(800.0), Some(528.0), &settings()),
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["avg_current_ma"], 800.0, "die Kennzahlen stehen flach im Objekt");
        assert_eq!(serde_json::from_value::<DeviceEfficiency>(json).unwrap(), detail);
    }
}
