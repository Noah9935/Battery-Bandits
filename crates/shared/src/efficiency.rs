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

/// Mindestzahl Messwerte für eine Aussage.
pub const MIN_READINGS: i64 = 50;
/// Mindestlänge des Messzeitraums in Stunden.
pub const MIN_WINDOW_HOURS: f64 = 24.0;
/// So weit muss die Spannung im Messzeitraum mindestens gefallen sein (mV), sonst steckt der Abfall im Rauschen.
pub const MIN_TOTAL_DROP_MV: f64 = 150.0;
/// Über diesem Wert gilt das Ergebnis als unplausibel: Eine Batterie liefert nicht mehr als ihre Nennkapazität.
pub const MAX_PLAUSIBLE_SCORE_PCT: f64 = 120.0;

/// Warum es keine Aussage gibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    TooFewReadings,
    TooShort,
    NoLoad,
    TooLittleDrop,
    Implausible,
}

impl Reason {
    pub fn text(self) -> &'static str {
        match self {
            Reason::TooFewReadings => "Zu wenige Messwerte für eine belastbare Aussage.",
            Reason::TooShort => "Der Messzeitraum ist noch zu kurz, es braucht mindestens 24 Stunden.",
            Reason::NoLoad => "Das Gerät wird kaum belastet, der Strom reicht für keine Aussage.",
            Reason::TooLittleDrop => "Die Spannung ist noch zu wenig gefallen, um daraus die Kapazität abzuleiten.",
            Reason::Implausible => {
                "Die Messwerte passen nicht zur Nennkapazität (mehr als 120 Prozent). Eine Aussage wäre geraten."
            }
        }
    }
}

/// Worauf eine Auswertung beruht.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evidence {
    /// Zahl der Messwerte, `None` wenn sie nicht bekannt ist (abgeschlossene Zyklen).
    pub readings: Option<i64>,
    pub window_hours: f64,
}

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
    /// Warum es keine Effizienz gibt, falls keine berechnet wurde.
    #[serde(default)]
    pub reason: Option<Reason>,
}

/// Wie [`efficiency`], aber nur mit belastbarer Grundlage. Reichen Messwerte, Zeitraum, Last und Spannungsabfall nicht
/// aus oder ist das Ergebnis unplausibel, bleibt es bei "keine Aussage" mit Begründung. Strom und Spannungsabfall
/// bleiben als gemessene Werte sichtbar.
pub fn assess(
    avg_current_ma: Option<f64>,
    voltage_drop_mv_per_day: Option<f64>,
    evidence: Evidence,
    settings: &Settings,
) -> Efficiency {
    let core = efficiency(avg_current_ma, voltage_drop_mv_per_day, settings);
    let window = evidence.window_hours;
    let reason = if evidence.readings.is_some_and(|n| n < MIN_READINGS) {
        Some(Reason::TooFewReadings)
    } else if !window.is_finite() || window < MIN_WINDOW_HOURS {
        Some(Reason::TooShort)
    } else if core.avg_current_ma.is_none() {
        Some(Reason::NoLoad)
    } else if core
        .voltage_drop_mv_per_day
        .is_none_or(|d| d * window / 24.0 < MIN_TOTAL_DROP_MV)
    {
        Some(Reason::TooLittleDrop)
    } else if core.score_pct.is_some_and(|s| f64::from(s) > MAX_PLAUSIBLE_SCORE_PCT) {
        Some(Reason::Implausible)
    } else {
        None
    };
    match reason {
        Some(reason) => Efficiency {
            real_runtime_hours: None,
            score_pct: None,
            reason: Some(reason),
            ..core
        },
        None => core,
    }
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
        reason: None,
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
    assess(
        current,
        drop,
        Evidence {
            readings: None,
            window_hours: hours,
        },
        settings,
    )
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
    /// Innenwiderstand in Milliohm, geschätzt aus Spannung und Strom der Messwerte (siehe [`internal_resistance_mohm`]).
    #[serde(default)]
    pub internal_resistance_mohm: Option<f64>,
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
    /// Zusammenfassung je Batteriemodell für den Vergleich von Kosten und Nutzen.
    #[serde(default)]
    pub models: Vec<ModelSummary>,
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
    /// Preis der Batterie in Cent, falls beim Einbau angegeben.
    #[serde(default)]
    pub price_cents: Option<i32>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub resistance_mohm: Option<f64>,
    #[serde(flatten)]
    pub result: Efficiency,
}

/// Kosten je Tag in Cent, wenn Preis und Laufzeit (mindestens 24 Stunden) bekannt sind.
pub fn cost_per_day_cents(price_cents: Option<i32>, hours: f64) -> Option<f64> {
    let price = price_cents?;
    (hours.is_finite() && hours >= MIN_WINDOW_HOURS).then(|| f64::from(price) / (hours / 24.0))
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

// ---------------------------------------------------------------- Innenwiderstand und Spannungsabfall

/// Mindestzahl Messwerte für eine Schätzung des Innenwiderstands.
pub const MIN_RESISTANCE_READINGS: i64 = 100;
/// So breit muss der Strom schwanken (Standardabweichung in mA), sonst lässt sich der Widerstand nicht aus Strom
/// und Spannung ablesen: Ohne unterschiedliche Lasten gibt es keine Steigung.
pub const MIN_CURRENT_SPREAD_MA: f64 = 50.0;
/// Über diesem Wert (mΩ) ist die Schätzung unplausibel, etwa weil Messwerte verrutschen.
pub const MAX_PLAUSIBLE_MOHM: f64 = 500.0;
/// Strom beim Senden, mit dem der Spannungsabfall als Beispiel gerechnet wird.
pub const REFERENCE_CURRENT_MA: f64 = 5_000.0;

/// Innenwiderstand in Milliohm nach dem Ohmschen Gesetz, so wie man ihn im Alltag misst: Unter Last fällt die
/// Klemmenspannung um `U = I * R`. Trägt man die Spannung über dem Strom auf, ist die Steigung der negative
/// Widerstand. `slope_v_per_ma` ist diese Steigung in mV je mA (das ist Ohm). Ohne genug Messwerte, ohne
/// schwankenden Strom oder mit unsinnigem Ergebnis gibt es keine Aussage.
pub fn internal_resistance_mohm(
    slope_v_per_ma: Option<f64>,
    current_spread_ma: Option<f64>,
    readings: i64,
) -> Option<f64> {
    let slope = slope_v_per_ma.filter(|s| s.is_finite() && *s < 0.0)?;
    let spread = current_spread_ma.filter(|s| s.is_finite() && *s >= MIN_CURRENT_SPREAD_MA)?;
    let _ = spread;
    if readings < MIN_RESISTANCE_READINGS {
        return None;
    }
    let mohm = -slope * 1000.0;
    (mohm > 0.0 && mohm <= MAX_PLAUSIBLE_MOHM).then_some(mohm)
}

/// Spannungsabfall in mV bei diesem Strom (`U = I * R`).
pub fn voltage_drop_mv(resistance_mohm: f64, current_ma: f64) -> f64 {
    current_ma.abs() * resistance_mohm / 1000.0
}

/// Einordnung des Innenwiderstands für eine 12-V-Batterie mit 80 Ah. Richtwert: Neu liegt sie bei etwa 5 bis 10 mΩ,
/// ab 15 mΩ lässt die Leistung unter Last spürbar nach, ab 30 mΩ ist sie verbraucht oder schlecht angeschlossen.
pub fn resistance_label(mohm: f64) -> &'static str {
    if mohm <= 15.0 {
        "gut"
    } else if mohm <= 30.0 {
        "erhöht"
    } else {
        "hoch"
    }
}

/// Anteil der Klemmenspannung, den der Spannungsabfall bei diesem Strom kostet, in Prozent.
pub fn drop_share_pct(drop_mv: f64, terminal_mv: f64) -> Option<f64> {
    (terminal_mv > 0.0 && drop_mv.is_finite()).then(|| drop_mv / terminal_mv * 100.0)
}

// ---------------------------------------------------------------- Batteriemodelle

/// Anzeigename für Batterien ohne Modellangabe.
pub const UNKNOWN_MODEL: &str = "Modell unbekannt";

/// Eine Batterie (laufend oder abgeschlossen) für die Zusammenfassung nach Modell.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInput {
    pub model: Option<String>,
    pub hours: f64,
    pub discharged_ah: f64,
    pub price_cents: Option<i32>,
    pub resistance_mohm: Option<f64>,
    pub score_pct: Option<f32>,
    /// Abgeschlossen: Die Laufzeit steht fest. Laufende Batterien zählen nur bei Zahl, Preis und Widerstand mit.
    pub finished: bool,
}

/// Alle Batterien eines Modells zusammengenommen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSummary {
    pub model: String,
    /// Zahl der Batterien (laufende und abgeschlossene).
    pub batteries: i64,
    pub finished: i64,
    /// Mittlere Laufzeit abgeschlossener Batterien (mindestens ein Tag) in Stunden.
    pub avg_hours: Option<f64>,
    /// Mittlere entnommene Ladung je abgeschlossener Batterie in Ah.
    pub avg_discharged_ah: Option<f64>,
    /// Verbrauch: entnommene Ladung je Tag in Ah.
    pub ah_per_day: Option<f64>,
    pub avg_price_cents: Option<i32>,
    /// Kosten je Tag Laufzeit in Cent.
    pub cost_per_day_cents: Option<f64>,
    /// Kosten je entnommener Amperestunde in Cent.
    pub cost_per_ah_cents: Option<f64>,
    pub avg_resistance_mohm: Option<f64>,
    pub avg_score_pct: Option<f32>,
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / f64::from(n))
}

/// Fasst gleiche Modelle zusammen (ohne auf Groß- und Kleinschreibung oder Leerzeichen am Rand zu achten), damit sich
/// Kosten und Nutzen vergleichen lassen: wie lange sie halten, wie viel sie liefern, was ein Tag und eine
/// Amperestunde kosten. Das günstigste Modell je Tag steht vorn, Modelle ohne Aussage dahinter.
pub fn aggregate_models(inputs: &[ModelInput]) -> Vec<ModelSummary> {
    let mut groups: Vec<(String, String, Vec<&ModelInput>)> = Vec::new();
    for input in inputs {
        let name = input
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or(UNKNOWN_MODEL);
        let key = name.to_lowercase();
        match groups.iter_mut().find(|(k, ..)| *k == key) {
            Some((.., list)) => list.push(input),
            None => groups.push((key, name.to_owned(), vec![input])),
        }
    }
    let mut summaries: Vec<ModelSummary> = groups
        .into_iter()
        .map(|(_, model, list)| {
            let done: Vec<&&ModelInput> = list
                .iter()
                .filter(|i| i.finished && i.hours.is_finite() && i.hours >= MIN_WINDOW_HOURS)
                .collect();
            let avg_hours = mean(done.iter().map(|i| i.hours));
            let avg_ah = mean(done.iter().map(|i| i.discharged_ah));
            let ah_per_day = avg_hours.zip(avg_ah).map(|(h, ah)| ah / (h / 24.0));
            let avg_price = mean(list.iter().filter_map(|i| i.price_cents).map(f64::from));
            let cost_per_day = avg_price.zip(avg_hours).map(|(p, h)| p / (h / 24.0));
            let cost_per_ah = avg_price.zip(avg_ah).filter(|(_, ah)| *ah > 0.0).map(|(p, ah)| p / ah);
            ModelSummary {
                model,
                batteries: list.len() as i64,
                finished: list.iter().filter(|i| i.finished).count() as i64,
                avg_hours,
                avg_discharged_ah: avg_ah,
                ah_per_day,
                avg_price_cents: avg_price.map(|p| p.round() as i32),
                cost_per_day_cents: cost_per_day,
                cost_per_ah_cents: cost_per_ah,
                avg_resistance_mohm: mean(list.iter().filter_map(|i| i.resistance_mohm)),
                avg_score_pct: mean(list.iter().filter_map(|i| i.score_pct).map(f64::from)).map(|s| s as f32),
            }
        })
        .collect();
    summaries.sort_by(|a, b| {
        let key = |m: &ModelSummary| m.cost_per_day_cents.unwrap_or(f64::MAX);
        key(a)
            .total_cmp(&key(b))
            .then_with(|| (a.model == UNKNOWN_MODEL).cmp(&(b.model == UNKNOWN_MODEL)))
            .then_with(|| a.model.cmp(&b.model))
    });
    summaries
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
            price_cents: None,
            model: None,
            resistance_mohm: None,
            result: Efficiency {
                score_pct: score,
                ..Efficiency::default()
            },
        }
    }

    fn evidence(readings: i64, hours: f64) -> Evidence {
        Evidence {
            readings: Some(readings),
            window_hours: hours,
        }
    }

    #[test]
    fn a_solid_basis_gives_a_score() {
        let e = assess(Some(800.0), Some(528.0), evidence(500, 100.0), &settings());
        assert!((e.score_pct.unwrap() - 100.0).abs() < 1e-3);
        assert_eq!(e.reason, None);
    }

    #[test]
    fn too_few_readings_or_too_short_a_period_means_no_statement() {
        let few = assess(Some(800.0), Some(528.0), evidence(10, 100.0), &settings());
        assert_eq!(
            (few.score_pct, few.real_runtime_hours, few.reason),
            (None, None, Some(Reason::TooFewReadings))
        );
        assert_eq!(few.avg_current_ma, Some(800.0), "gemessene Werte bleiben sichtbar");
        assert!(few.nominal_runtime_hours.is_some());
        let short = assess(Some(800.0), Some(528.0), evidence(500, 6.0), &settings());
        assert_eq!((short.score_pct, short.reason), (None, Some(Reason::TooShort)));
        let nan = assess(Some(800.0), Some(528.0), evidence(500, f64::NAN), &settings());
        assert_eq!(nan.reason, Some(Reason::TooShort));
    }

    #[test]
    fn without_load_or_a_real_drop_there_is_no_statement() {
        let idle = assess(Some(0.2), Some(528.0), evidence(500, 100.0), &settings());
        assert_eq!((idle.score_pct, idle.reason), (None, Some(Reason::NoLoad)));
        // 121 mV pro Tag über zwei Tage sind 242 mV: reicht. Über 30 Stunden nur 151 mV: gerade so. Darunter nicht.
        let flat = assess(Some(800.0), Some(60.0), evidence(500, 48.0), &settings());
        assert_eq!((flat.score_pct, flat.reason), (None, Some(Reason::TooLittleDrop)));
        let rising = assess(Some(800.0), Some(-40.0), evidence(500, 100.0), &settings());
        assert_eq!(rising.reason, Some(Reason::TooLittleDrop));
        let unknown = assess(Some(800.0), None, evidence(500, 100.0), &settings());
        assert_eq!(unknown.reason, Some(Reason::TooLittleDrop));
    }

    #[test]
    fn a_result_above_the_rated_capacity_is_not_believed() {
        // 80 Ah bei 306 mA halten 10,9 Tage. 121 mV pro Tag ergäben 18 Tage: 167 Prozent, das glaubt niemand.
        let e = assess(Some(306.0), Some(121.0), evidence(126, 48.0), &settings());
        assert_eq!(
            (e.score_pct, e.real_runtime_hours, e.reason),
            (None, None, Some(Reason::Implausible))
        );
        assert!(e.nominal_runtime_hours.is_some());
        // Knapp über 100 Prozent bleibt erlaubt (Messtoleranz).
        let ok = assess(Some(800.0), Some(500.0), evidence(500, 100.0), &settings());
        assert!(ok.score_pct.is_some_and(|s| s > 100.0 && s <= 120.0), "{ok:?}");
    }

    #[test]
    fn every_reason_has_a_text() {
        for reason in [
            Reason::TooFewReadings,
            Reason::TooShort,
            Reason::NoLoad,
            Reason::TooLittleDrop,
            Reason::Implausible,
        ] {
            assert!(reason.text().ends_with('.'), "{reason:?}");
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
    fn the_cost_per_day_needs_a_price_and_a_long_enough_run() {
        assert_eq!(cost_per_day_cents(Some(9_600), 240.0), Some(960.0));
        assert_eq!(
            cost_per_day_cents(Some(9_600), 12.0),
            None,
            "unter einem Tag sagt es nichts"
        );
        assert_eq!(cost_per_day_cents(None, 240.0), None);
        assert_eq!(cost_per_day_cents(Some(100), f64::NAN), None);
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
            internal_resistance_mohm: Some(8.0),
            result: efficiency(Some(800.0), Some(528.0), &settings()),
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["avg_current_ma"], 800.0, "die Kennzahlen stehen flach im Objekt");
        assert_eq!(serde_json::from_value::<DeviceEfficiency>(json).unwrap(), detail);
    }

    #[test]
    fn the_resistance_comes_from_how_far_the_voltage_falls_per_current() {
        // 0,01 mV je mA sind 10 mΩ: Bei 5 A fallen 50 mV.
        let r = internal_resistance_mohm(Some(-0.01), Some(300.0), 500).unwrap();
        assert!((r - 10.0).abs() < 1e-9);
        assert!((voltage_drop_mv(r, 5_000.0) - 50.0).abs() < 1e-9);
        assert!(
            (voltage_drop_mv(r, -5_000.0) - 50.0).abs() < 1e-9,
            "das Vorzeichen des Stroms zählt nicht"
        );
        assert_eq!(resistance_label(8.0), "gut");
        assert_eq!(resistance_label(15.0), "gut");
        assert_eq!(resistance_label(22.0), "erhöht");
        assert_eq!(resistance_label(45.0), "hoch");
        assert!((drop_share_pct(50.0, 12_500.0).unwrap() - 0.4).abs() < 1e-9);
        assert_eq!(drop_share_pct(50.0, 0.0), None);
    }

    #[test]
    fn without_varying_load_or_enough_data_there_is_no_resistance() {
        assert_eq!(
            internal_resistance_mohm(Some(-0.01), Some(10.0), 500),
            None,
            "Strom schwankt kaum"
        );
        assert_eq!(
            internal_resistance_mohm(Some(-0.01), Some(300.0), 50),
            None,
            "zu wenige Messwerte"
        );
        assert_eq!(
            internal_resistance_mohm(Some(0.01), Some(300.0), 500),
            None,
            "Spannung steigt mit dem Strom"
        );
        assert_eq!(
            internal_resistance_mohm(Some(-2.0), Some(300.0), 500),
            None,
            "2000 mΩ glaubt niemand"
        );
        assert_eq!(internal_resistance_mohm(None, Some(300.0), 500), None);
        assert_eq!(internal_resistance_mohm(Some(-0.01), None, 500), None);
        assert_eq!(internal_resistance_mohm(Some(f64::NAN), Some(300.0), 500), None);
    }

    fn battery(model: Option<&str>, hours: f64, ah: f64, price: Option<i32>, finished: bool) -> ModelInput {
        ModelInput {
            model: model.map(str::to_owned),
            hours,
            discharged_ah: ah,
            price_cents: price,
            resistance_mohm: Some(10.0),
            score_pct: Some(90.0),
            finished,
        }
    }

    #[test]
    fn equal_models_are_combined_whatever_their_spelling() {
        let list = [
            battery(Some("Varta 80"), 240.0, 40.0, Some(10_000), true),
            battery(Some("  varta 80 "), 480.0, 60.0, Some(12_000), true),
            battery(Some("Exide 80"), 120.0, 20.0, Some(8_000), true),
        ];
        let summary = aggregate_models(&list);
        assert_eq!(summary.len(), 2);
        let varta = summary.iter().find(|m| m.model == "Varta 80").unwrap();
        assert_eq!((varta.batteries, varta.finished), (2, 2));
        assert_eq!(varta.avg_hours, Some(360.0));
        assert_eq!(varta.avg_discharged_ah, Some(50.0));
        assert_eq!(varta.avg_price_cents, Some(11_000));
        // 50 Ah in 15 Tagen: gut 3,3 Ah pro Tag, 110 Euro auf 15 Tage sind 7,33 Euro pro Tag, 2,20 Euro je Ah.
        assert!((varta.ah_per_day.unwrap() - 50.0 / 15.0).abs() < 1e-9);
        assert!((varta.cost_per_day_cents.unwrap() - 11_000.0 / 15.0).abs() < 1e-9);
        assert!((varta.cost_per_ah_cents.unwrap() - 220.0).abs() < 1e-9);
        assert_eq!(varta.avg_resistance_mohm, Some(10.0));
        assert_eq!(varta.avg_score_pct, Some(90.0));
    }

    #[test]
    fn the_cheapest_model_per_day_comes_first_and_unknown_ones_last() {
        let list = [
            battery(None, 240.0, 40.0, None, true),
            battery(Some("Teuer"), 240.0, 40.0, Some(30_000), true),
            battery(Some("Günstig"), 240.0, 40.0, Some(5_000), true),
            battery(Some("Ohne Preis"), 240.0, 40.0, None, true),
        ];
        let names: Vec<String> = aggregate_models(&list).into_iter().map(|m| m.model).collect();
        assert_eq!(
            names,
            ["Günstig", "Teuer", "Ohne Preis", "Modell unbekannt"].map(String::from)
        );
        assert!(aggregate_models(&[]).is_empty());
    }

    #[test]
    fn running_batteries_count_but_do_not_set_the_lifetime() {
        let list = [
            battery(Some("Varta"), 240.0, 40.0, Some(10_000), true),
            battery(Some("Varta"), 12.0, 2.0, Some(10_000), false),
            battery(Some("Varta"), 5.0, 1.0, None, true), // unter einem Tag: zu kurz für Laufzeit und Verbrauch
        ];
        let varta = &aggregate_models(&list)[0];
        assert_eq!((varta.batteries, varta.finished), (3, 2));
        assert_eq!(
            varta.avg_hours,
            Some(240.0),
            "nur abgeschlossene mit mindestens einem Tag"
        );
        assert_eq!(varta.avg_discharged_ah, Some(40.0));
        assert_eq!(varta.avg_price_cents, Some(10_000));
        let only_running = &aggregate_models(&[battery(Some("Neu"), 12.0, 2.0, Some(9_000), false)])[0];
        assert_eq!(
            (
                only_running.avg_hours,
                only_running.cost_per_day_cents,
                only_running.cost_per_ah_cents
            ),
            (None, None, None)
        );
        assert_eq!(only_running.avg_price_cents, Some(9_000));
    }
}
