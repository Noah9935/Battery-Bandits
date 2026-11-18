//! Zustandsschätzer für den Ladezustand (SoC) mit Fehlerangabe.
//!
//! Zwei Quellen sagen etwas über den Ladezustand, und beide sind für sich unzuverlässig:
//!
//! * Die Ladungsintegration (Vorlesung: `SOC = SOC(Start) − 100 · Q / Nennkapazität`) ist kurzfristig sehr genau,
//!   driftet aber: Ein Offset im Stromsensor, ein Verstärkungsfehler und eine von den 80 Ah abweichende wirkliche
//!   Kapazität summieren sich mit jeder Messung auf, und bei Datenlücken fehlt der Strom ganz.
//! * Die Ruhespannung liefert über die Spannungskurve der Batterie einen absoluten Wert ohne Drift, ist aber
//!   verrauscht, hängt von Last und Temperatur ab und die Kurve selbst ist nur auf wenige Prozent genau.
//!
//! Ein erweitertes Kalman-Filter führt beide zusammen. Der Zustand hat drei Größen:
//!
//! 1. der Ladezustand `s` in Prozent,
//! 2. ein Offset `b` des Stromsensors in Ampere,
//! 3. ein relativer Faktor `g`, der Verstärkungsfehler des Sensors und Abweichung der Kapazität zusammenfasst.
//!
//! Die entnommene Ladung ist `(I − b) · (1 + g) · Δt`. Weil `b` und `g` mitgeschätzt werden, lernt das Filter
//! die Drift der Integration anhand der Spannung selbst und wird mit der Zeit genauer. Zu jedem Wert gehört eine
//! Standardabweichung. Sie wächst bei Datenlücken und starker Last, schrumpft bei guten Messungen und enthält
//! einen festen Anteil für den Fehler der Spannungskurve, den kein Filter wegmitteln kann.
//!
//! Die Spannungskurve stammt aus einer vollständigen, gleichmäßigen Entladung eines echten Geräts der
//! Messdatenplattform (Gerät 101, 80 Ah, 285 mA, Ladezustand aus der Trapezregel der Vorlesung). Sie ist die
//! Standardkurve und kann später je Batterietyp ersetzt werden.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Spannungskurve der ruhenden Batterie: (Ladezustand in Prozent, Klemmenspannung in mV), aufsteigend.
///
/// Gemessen an einer vollständigen Entladung (Mittel je 5-Prozent-Stufe). Unterhalb von etwa 15 Prozent bricht die
/// Spannung stark ein: Dort ist die Batterie im Betrieb praktisch leer, obwohl rechnerisch noch Ladung übrig ist.
pub const OCV_CURVE: [(f64, f64); 22] = [
    (0.0, 8_400.0),
    (2.5, 8_874.0),
    (7.5, 9_890.0),
    (12.5, 10_756.0),
    (17.5, 11_179.0),
    (22.5, 11_371.0),
    (27.5, 11_500.0),
    (32.5, 11_605.0),
    (37.5, 11_697.0),
    (42.5, 11_783.0),
    (47.5, 11_865.0),
    (52.5, 11_952.0),
    (57.5, 12_046.0),
    (62.5, 12_139.0),
    (67.5, 12_221.0),
    (72.5, 12_297.0),
    (77.5, 12_373.0),
    (82.5, 12_447.0),
    (87.5, 12_531.0),
    (92.5, 12_617.0),
    (97.5, 12_692.0),
    (100.0, 12_720.0),
];

/// Temperatur, auf die sich die Spannungskurve bezieht.
pub const OCV_REFERENCE_TEMP_C: f64 = 20.0;
/// Änderung der Ruhespannung einer 12-V-Bleibatterie je Kelvin (etwa 0,2 mV je Zelle und Kelvin).
pub const OCV_TEMP_COEFF_MV_PER_K: f64 = 1.2;

/// Innenwiderstand, mit dem der Spannungsabfall unter Last herausgerechnet wird (Annahme, siehe Unsicherheit).
const R_INTERNAL_MOHM: f64 = 15.0;
/// Unsicherheit dieses Widerstands in Milliohm.
const SIGMA_R_MOHM: f64 = 20.0;
/// Rauschen der Spannungsmessung in mV. Gemessen an den echten Reihen (Gerät 101 und TN0001, Streuung aufeinander
/// folgender Werte): rund 3 mV. Angesetzt sind 5 mV, damit andere Geräte und Wandler Reserve haben.
const SIGMA_ADC_MV: f64 = 5.0;
/// Unsicherheit der Temperaturkorrektur in Kelvin (ohne Temperaturmessung deutlich größer).
const SIGMA_TEMP_K: f64 = 5.0;
const SIGMA_TEMP_UNKNOWN_K: f64 = 10.0;
/// Zufälliger Anteil des Kurvenfehlers je Messung in Prozentpunkten.
const SIGMA_CURVE_RANDOM: f64 = 1.0;
/// Fester Anteil des Kurvenfehlers (Batterietyp, Alterung) in Prozentpunkten. Er geht unverändert in die
/// angegebene Unsicherheit ein, weil mehr Messungen ihn nicht verkleinern: Ein Versatz der Kurve ist von einem
/// falschen Startwert nicht zu unterscheiden. Die Kurve stammt von einer einzigen gemessenen Batterie, andere
/// Typen und alte Batterien weichen um mehrere Prozentpunkte ab. Mit einer eigenen Kurve je Batterietyp
/// ließe sich dieser Wert verkleinern.
pub const SIGMA_SYSTEMATIC: f64 = 3.0;

/// Größte Unsicherheit eines Startwerts aus der Spannung. Wer gar nichts über den Ladezustand weiß, liegt mit einer
/// Gleichverteilung von 0 bis 100 Prozent bei etwa 29 Punkten Standardabweichung. Mehr hat keine Aussagekraft.
const SIGMA_START_MAX: f64 = 30.0;

/// Zuschlag zur Unsicherheit eines Startwerts aus der Spannung (Punkte). Die erste Messung kann in einem Übergang
/// liegen: Bei TN0001 war sie 6 Punkte höher als alle späteren (frisch geladene Batterie mit Oberflächenladung).
const SIGMA_START_EXTRA: f64 = 4.0;

/// Zeitkonstante der Selbstkontrolle (Anzahl Messungen, über die der Mittelwert der normierten Abweichungen läuft).
const FIT_MEMORY: f64 = 50.0;
/// Eine einzelne Abweichung zählt höchstens so stark (NIS), damit ein Ausreißer die Spanne nicht auf Dauer aufbläht.
const FIT_CAP: f64 = 16.0;

/// Unsicherheit eines angegebenen Startwerts (zum Beispiel beim Batteriewechsel) in Prozentpunkten.
pub const KNOWN_SOC_SIGMA: f64 = 3.0;
/// Anfängliche Unsicherheit des Stromoffsets (A) und des Faktors `g`.
const SIGMA_BIAS0_A: f64 = 0.015;
const SIGMA_GAIN0: f64 = 0.05;

/// Rauschen der Zustandsfortschreibung.
/// Relative Unsicherheit der Trapezregel zwischen zwei regulären Messungen.
const RHO_STEP: f64 = 0.02;
/// Relative Unsicherheit des angenommenen Stroms während einer Datenlücke.
const RHO_GAP: f64 = 0.5;
/// Unbekannte Selbstentladung in Prozentpunkten je Wurzel Stunde.
const SELF_DRAIN: f64 = 0.01;
/// Zufallsbewegung des Stromoffsets in A je Wurzel Stunde.
const BIAS_WALK: f64 = 0.002;
/// Zufallsbewegung des Faktors (Alterung) je Wurzel Stunde.
const GAIN_WALK: f64 = 0.0002;

/// Ab diesem Abstand zur letzten Messung gilt der Strom dazwischen als unbekannt.
pub const MAX_STEP_SECONDS: f64 = 1_800.0;
/// Zeitkonstante des gleitenden Mittels des Stroms in Stunden (Annahme für Lücken).
const CURRENT_MEMORY_HOURS: f64 = 6.0;

/// Eine Spannungsmessung weiter als so viele Standardabweichungen von der Erwartung wird nicht verwendet.
const GATE_SIGMAS: f64 = 4.0;
/// So viele Abweisungen in Folge zeigen, dass der Zustand falsch ist (zum Beispiel ungemeldeter Batteriewechsel).
const RESTART_AFTER: u32 = 12;

/// Grenzen der gelernten Größen.
const BIAS_LIMIT_A: f64 = 0.1;
const GAIN_MIN: f64 = -0.3;
const GAIN_MAX: f64 = 0.5;

/// Mit so vielen Standardabweichungen unter der Schätzung entscheidet der Status (Warnung, kritisch).
pub const STATUS_SIGMAS: f64 = 1.0;

/// z-Wert für den 95-Prozent-Bereich.
pub const Z95: f64 = 1.96;

/// Spannung der ruhenden Batterie bei einem Ladezustand (mV).
pub fn ocv_mv(soc_percent: f64) -> f64 {
    let soc = soc_percent.clamp(0.0, 100.0);
    let idx = OCV_CURVE
        .windows(2)
        .position(|w| soc <= w[1].0)
        .unwrap_or(OCV_CURVE.len() - 2);
    let (s0, v0) = OCV_CURVE[idx];
    let (s1, v1) = OCV_CURVE[idx + 1];
    v0 + (v1 - v0) * (soc - s0) / (s1 - s0)
}

/// Ladezustand zu einer Ruhespannung samt Steigung der Kurve (mV je Prozentpunkt) an dieser Stelle.
/// Außerhalb der Kurve gilt der Randwert.
pub fn soc_from_ocv(mv: f64) -> (f64, f64) {
    let (lo, hi) = (OCV_CURVE[0].1, OCV_CURVE[OCV_CURVE.len() - 1].1);
    let v = mv.clamp(lo, hi);
    let idx = OCV_CURVE
        .windows(2)
        .position(|w| v <= w[1].1)
        .unwrap_or(OCV_CURVE.len() - 2);
    let (s0, v0) = OCV_CURVE[idx];
    let (s1, v1) = OCV_CURVE[idx + 1];
    let slope = (v1 - v0) / (s1 - s0);
    (s0 + (v - v0) / slope, slope)
}

/// Eine Messung des Geräts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub at: DateTime<Utc>,
    pub voltage_mv: i32,
    /// Strom in mA, das Vorzeichen ist je nach Quelle verschieden und wird nicht beachtet.
    pub current_ma: i32,
    pub temperature_c: Option<f32>,
}

/// Geschätzter Ladezustand mit Standardabweichung (beides in Prozentpunkten).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub soc_percent: f32,
    pub sigma_percent: f32,
}

impl Estimate {
    /// Halbe Breite des 95-Prozent-Bereichs in Prozentpunkten.
    pub fn margin_percent(self) -> f32 {
        margin_95(self.sigma_percent)
    }

    /// Untere Grenze des 95-Prozent-Bereichs.
    pub fn lower_percent(self) -> f32 {
        (self.soc_percent - self.margin_percent()).max(0.0)
    }

    /// Obere Grenze des 95-Prozent-Bereichs.
    pub fn upper_percent(self) -> f32 {
        (self.soc_percent + self.margin_percent()).min(100.0)
    }
}

/// Halbe Breite des 95-Prozent-Bereichs zu einer Standardabweichung.
pub fn margin_95(sigma_percent: f32) -> f32 {
    (Z95 as f32) * sigma_percent
}

/// Vorsichtiger Wert für Alarme: Schätzung minus Unsicherheit, nie unter 0.
pub fn conservative_percent(soc_percent: f32, sigma_percent: Option<f32>) -> f32 {
    (soc_percent - (STATUS_SIGMAS as f32) * sigma_percent.unwrap_or(0.0).max(0.0)).max(0.0)
}

/// Was mit einer Messung geschehen ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Erste Messung: Der Zustand wurde aus der Spannung aufgebaut.
    Started,
    /// Die Spannung wurde mit der Ladungsintegration zusammengeführt.
    Fused,
    /// Die Spannung passte nicht zur Erwartung und wurde nicht verwendet (Ausreißer).
    Rejected,
    /// Wiederholte Abweisungen: Der Zustand war falsch und wurde aus der Spannung neu aufgebaut.
    Restarted,
    /// Die Messung ist älter als der Zustand und ändert nichts.
    Late,
}

/// Ergebnis eines Schritts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub estimate: Estimate,
    /// In diesem Schritt entnommene Ladung in Ah (nur bei lückenlosen Messungen, sonst 0).
    pub drawn_ah: f64,
    pub verdict: Verdict,
    /// Quadrat der normierten Abweichung zwischen Spannung und Erwartung (Innovation zum Quadrat durch ihre Varianz).
    /// Passt das Rauschmodell, liegt der Mittelwert über viele Messungen bei etwa 1. Deutlich darüber ist das Filter
    /// zu sicher, deutlich darunter zu vorsichtig. Nur bei Messungen mit Abgleich gesetzt.
    pub nis: Option<f64>,
}

type Mat = [[f64; 3]; 3];

/// Zustand des Filters eines Geräts. Wird je Gerät in der Datenbank gespeichert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimatorState {
    /// Ladezustand in Prozent.
    pub soc: f64,
    /// Offset des Stromsensors in A.
    pub bias_a: f64,
    /// Relativer Faktor für Verstärkungs- und Kapazitätsfehler.
    pub gain: f64,
    /// Kovarianz von (soc, bias_a, gain), zeilenweise.
    pub cov: [f64; 9],
    /// Gleitendes Mittel des Stroms in A (Annahme für Datenlücken).
    pub mean_current_a: Option<f64>,
    /// Aufeinanderfolgende abgewiesene Spannungsmessungen.
    pub rejected: u32,
    /// Selbstkontrolle: gleitender Mittelwert der normierten Abweichungen zum Quadrat (NIS). Bei passendem Modell
    /// liegt er bei 1 oder darunter. Steigt er darüber, wird die angegebene Spanne entsprechend verbreitert.
    #[serde(default = "neutral_fit")]
    pub fit: f64,
    pub last_at: Option<DateTime<Utc>>,
    pub last_current_a: Option<f64>,
}

fn neutral_fit() -> f64 {
    1.0
}

fn to_mat(c: &[f64; 9]) -> Mat {
    [[c[0], c[1], c[2]], [c[3], c[4], c[5]], [c[6], c[7], c[8]]]
}

fn from_mat(m: &Mat) -> [f64; 9] {
    [
        m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2],
    ]
}

#[allow(clippy::needless_range_loop)]
fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            for k in 0..3 {
                r[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    r
}

#[allow(clippy::needless_range_loop)]
fn transpose(a: &Mat) -> Mat {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a[j][i];
        }
    }
    r
}

#[allow(clippy::needless_range_loop)]
fn symmetrize(m: &mut Mat) {
    for i in 0..3 {
        for j in (i + 1)..3 {
            let v = 0.5 * (m[i][j] + m[j][i]);
            m[i][j] = v;
            m[j][i] = v;
        }
    }
}

/// Unsicherheit der Spannungsmessung in Ladezustandspunkten und der aus der Spannung geschätzte Ladezustand.
struct VoltageView {
    soc: f64,
    /// Varianz dieses Werts in Prozentpunkten zum Quadrat.
    variance: f64,
}

fn view_voltage(m: &Measurement) -> VoltageView {
    let current_a = f64::from(m.current_ma).abs() / 1000.0;
    let (temp_correction_mv, sigma_temp) = match m.temperature_c {
        Some(t) => (
            OCV_TEMP_COEFF_MV_PER_K * (f64::from(t) - OCV_REFERENCE_TEMP_C),
            SIGMA_TEMP_K,
        ),
        None => (0.0, SIGMA_TEMP_UNKNOWN_K),
    };
    // Unter Last liegt die Klemmenspannung unter der Ruhespannung: Spannungsabfall am Innenwiderstand addieren.
    let ocv = f64::from(m.voltage_mv) + current_a * R_INTERNAL_MOHM - temp_correction_mv;
    let (soc, slope) = soc_from_ocv(ocv);
    let sigma_mv_sq =
        SIGMA_ADC_MV.powi(2) + (current_a * SIGMA_R_MOHM).powi(2) + (OCV_TEMP_COEFF_MV_PER_K * sigma_temp).powi(2);
    // Die Steigung der Kurve rechnet Millivolt in Prozentpunkte um. Flache Stellen sind weniger aussagekräftig.
    let slope = slope.max(4.0);
    VoltageView {
        soc,
        variance: sigma_mv_sq / slope.powi(2) + SIGMA_CURVE_RANDOM.powi(2),
    }
}

/// Varianz eines Startwerts aus der Spannung, nach oben begrenzt.
fn start_variance(view: &VoltageView) -> f64 {
    (view.variance + SIGMA_START_EXTRA.powi(2)).min(SIGMA_START_MAX.powi(2))
}

impl EstimatorState {
    fn fresh(soc: f64, soc_variance: f64) -> Self {
        Self {
            soc: soc.clamp(0.0, 100.0),
            bias_a: 0.0,
            gain: 0.0,
            cov: from_mat(&[
                [soc_variance, 0.0, 0.0],
                [0.0, SIGMA_BIAS0_A.powi(2), 0.0],
                [0.0, 0.0, SIGMA_GAIN0.powi(2)],
            ]),
            mean_current_a: None,
            rejected: 0,
            fit: neutral_fit(),
            last_at: None,
            last_current_a: None,
        }
    }

    /// Erster Zustand aus der Spannung einer ersten Messung (neues Gerät oder verlorener Zustand).
    pub fn start(m: &Measurement) -> (Self, Step) {
        let view = view_voltage(m);
        let mut state = Self::fresh(view.soc, start_variance(&view));
        state.remember(m);
        let step = Step {
            estimate: state.estimate(),
            drawn_ah: 0.0,
            verdict: Verdict::Started,
            nis: None,
        };
        (state, step)
    }

    /// Zustand mit angegebenem Ladezustand (zum Beispiel beim Batteriewechsel). Die erste Messung danach wird nur
    /// mit der Spannung abgeglichen, ohne Strom zu integrieren.
    pub fn known(soc_percent: f32, sigma_percent: f64) -> Self {
        Self::fresh(f64::from(soc_percent), sigma_percent.powi(2))
    }

    fn remember(&mut self, m: &Measurement) {
        let current_a = f64::from(m.current_ma).abs() / 1000.0;
        self.last_at = Some(m.at);
        self.last_current_a = Some(current_a);
        if self.mean_current_a.is_none() {
            self.mean_current_a = Some(current_a);
        }
    }

    pub fn estimate(&self) -> Estimate {
        let p = self.cov[0].max(0.0);
        Estimate {
            soc_percent: self.soc.clamp(0.0, 100.0) as f32,
            sigma_percent: ((p + SIGMA_SYSTEMATIC.powi(2)) * self.fit.max(1.0)).sqrt() as f32,
        }
    }

    /// Verarbeitet eine Messung: Ladungsintegration über die Zeit seit der letzten, dann Abgleich mit der Spannung.
    pub fn step(&mut self, m: &Measurement, capacity_ah: f64) -> Step {
        let capacity = capacity_ah.max(1.0);
        let current_a = f64::from(m.current_ma).abs() / 1000.0;
        let mut drawn_ah = 0.0;

        if let Some(last) = self.last_at {
            if m.at <= last {
                return Step {
                    estimate: self.estimate(),
                    drawn_ah: 0.0,
                    verdict: Verdict::Late,
                    nis: None,
                };
            }
            let seconds = (m.at - last).num_milliseconds() as f64 / 1000.0;
            drawn_ah = self.predict(seconds, current_a, capacity);
        }

        let view = view_voltage(m);
        let (verdict, nis) = self.update(&view);
        self.remember(m);
        Step {
            estimate: self.estimate(),
            drawn_ah,
            verdict,
            nis,
        }
    }

    /// Fortschreibung des Zustands über `seconds`. Gibt die entnommene Ladung zurück (nur bei lückenlosen Messungen).
    fn predict(&mut self, seconds: f64, current_a: f64, capacity: f64) -> f64 {
        let dt_h = seconds / 3600.0;
        let previous_a = self.last_current_a.unwrap_or(current_a);
        let trapezoid_a = 0.5 * (previous_a + current_a);
        let regular = seconds <= MAX_STEP_SECONDS;
        let mean_a = self.mean_current_a.unwrap_or(trapezoid_a);
        // Bei einer Lücke ist der Strom dazwischen unbekannt: Das gleitende Mittel ist die beste Annahme, und
        // seine Unsicherheit ist groß. Bei lückenlosen Messungen gilt die Trapezregel.
        let (used_a, rho) = if regular {
            (trapezoid_a, RHO_STEP)
        } else {
            (mean_a, RHO_GAP)
        };

        let per_ah = 100.0 * dt_h / capacity;
        self.soc -= per_ah * (used_a - self.bias_a) * (1.0 + self.gain);
        self.soc = self.soc.clamp(0.0, 100.0);

        let f: Mat = [
            [1.0, per_ah * (1.0 + self.gain), -per_ah * (used_a - self.bias_a)],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let p = to_mat(&self.cov);
        let mut next = mul(&mul(&f, &p), &transpose(&f));
        next[0][0] += (rho * used_a * per_ah).powi(2) + SELF_DRAIN.powi(2) * dt_h;
        next[1][1] += BIAS_WALK.powi(2) * dt_h;
        next[2][2] += GAIN_WALK.powi(2) * dt_h;
        symmetrize(&mut next);
        self.cov = from_mat(&next);

        let weight = 1.0 - (-dt_h / CURRENT_MEMORY_HOURS).exp();
        let old = self.mean_current_a.unwrap_or(current_a);
        self.mean_current_a = Some(old + weight * (current_a - old));

        if regular { trapezoid_a * dt_h } else { 0.0 }
    }

    /// Abgleich mit der aus der Spannung geschätzten Ladung.
    fn update(&mut self, view: &VoltageView) -> (Verdict, Option<f64>) {
        let innovation = view.soc - self.soc;
        let p = to_mat(&self.cov);
        let s = p[0][0] + view.variance;
        let nis = innovation * innovation / s;
        self.fit += (nis.min(FIT_CAP) - self.fit) / FIT_MEMORY;
        if innovation * innovation > GATE_SIGMAS.powi(2) * s {
            self.rejected += 1;
            if self.rejected >= RESTART_AFTER {
                let (last_at, last_current, mean) = (self.last_at, self.last_current_a, self.mean_current_a);
                // Die Selbstkontrolle bleibt: Wer immer wieder neu anfangen muss, passt schlecht zum Modell.
                let fit = self.fit;
                *self = Self::fresh(view.soc, start_variance(view));
                self.fit = fit;
                self.last_at = last_at;
                self.last_current_a = last_current;
                self.mean_current_a = mean;
                return (Verdict::Restarted, Some(nis));
            }
            return (Verdict::Rejected, Some(nis));
        }
        self.rejected = 0;

        let k = [p[0][0] / s, p[1][0] / s, p[2][0] / s];
        self.soc = (self.soc + k[0] * innovation).clamp(0.0, 100.0);
        self.bias_a = (self.bias_a + k[1] * innovation).clamp(-BIAS_LIMIT_A, BIAS_LIMIT_A);
        self.gain = (self.gain + k[2] * innovation).clamp(GAIN_MIN, GAIN_MAX);

        // Joseph-Form: bleibt auch bei Rundungsfehlern positiv und symmetrisch.
        let a: Mat = [[1.0 - k[0], 0.0, 0.0], [-k[1], 1.0, 0.0], [-k[2], 0.0, 1.0]];
        let mut next = mul(&mul(&a, &p), &transpose(&a));
        for i in 0..3 {
            for j in 0..3 {
                next[i][j] += k[i] * view.variance * k[j];
            }
        }
        symmetrize(&mut next);
        self.cov = from_mat(&next);
        (Verdict::Fused, Some(nis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-06T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn meas(minutes: i64, mv: i32, ma: i32) -> Measurement {
        Measurement {
            at: t0() + Duration::minutes(minutes),
            voltage_mv: mv,
            current_ma: ma,
            temperature_c: Some(20.0),
        }
    }

    #[test]
    fn the_curve_is_monotonic_and_round_trips() {
        for w in OCV_CURVE.windows(2) {
            assert!(w[1].0 > w[0].0 && w[1].1 > w[0].1, "{w:?}");
        }
        for soc in [0.0, 3.0, 10.0, 25.0, 50.0, 80.0, 99.0, 100.0] {
            let (back, _) = soc_from_ocv(ocv_mv(soc));
            assert!((back - soc).abs() < 1e-9, "{soc} -> {back}");
        }
    }

    #[test]
    fn the_curve_is_clamped_outside() {
        assert_eq!(soc_from_ocv(5_000.0).0, 0.0);
        assert_eq!(soc_from_ocv(14_000.0).0, 100.0);
        assert_eq!(ocv_mv(-5.0), OCV_CURVE[0].1);
        assert_eq!(ocv_mv(150.0), OCV_CURVE[OCV_CURVE.len() - 1].1);
    }

    #[test]
    fn the_curve_matches_the_measured_discharge() {
        // Stützwerte der echten Entladung: 12,7 V voll, rund 11,95 V bei der Hälfte, 11,37 V bei 22,5 Prozent.
        assert!((ocv_mv(52.5) - 11_952.0).abs() < 1.0);
        assert!((ocv_mv(22.5) - 11_371.0).abs() < 1.0);
        assert!(ocv_mv(100.0) > 12_700.0);
    }

    #[test]
    fn the_first_reading_starts_from_the_voltage_with_a_clear_error() {
        let (state, step) = EstimatorState::start(&meas(0, 11_952, 300));
        assert_eq!(step.verdict, Verdict::Started);
        // 11,952 V plus ein kleiner Spannungsabfall ist rund die Hälfte.
        assert!((step.estimate.soc_percent - 52.7).abs() < 1.0, "{:?}", step.estimate);
        // Die erste Messung ist unsicherer als spätere (Zuschlag für einen möglichen Übergang): rund 5 Punkte.
        assert!(
            step.estimate.sigma_percent > 4.0 && step.estimate.sigma_percent < 7.0,
            "{:?}",
            step.estimate
        );
        assert_eq!(state.last_at, Some(t0()));
    }

    #[test]
    fn a_start_under_heavy_load_is_never_less_certain_than_a_pure_guess() {
        // Bei 25 A sagt die Spannung wegen des unbekannten Innenwiderstands fast nichts (rund 30 Punkte Unsicherheit
        // allein daraus). Mehr als bei völligem Raten (Gleichverteilung, etwa 29 Punkte) darf der Wert nie sein.
        let (_, step) = EstimatorState::start(&meas(0, 12_200, 25_000));
        let limit = (SIGMA_START_MAX.powi(2) + SIGMA_SYSTEMATIC.powi(2)).sqrt() as f32;
        assert!(step.estimate.sigma_percent <= limit + 1e-3, "{:?}", step.estimate);
        assert!(
            step.estimate.sigma_percent > 20.0,
            "die Unsicherheit bleibt groß: {:?}",
            step.estimate
        );
    }

    #[test]
    fn a_known_start_value_is_checked_against_the_voltage_not_discarded() {
        // 100 Prozent angegeben, die Spannung (11,95 V) zeigt aber rund die Hälfte: Die Angabe wird nicht blind
        // geglaubt, sondern nach mehreren Messungen aus der Spannung neu aufgebaut.
        let mut state = EstimatorState::known(100.0, KNOWN_SOC_SIGMA);
        let mut verdicts = Vec::new();
        for i in 0..14 {
            let step = state.step(&meas(i * 15, 11_952, 300), 80.0);
            verdicts.push(step.verdict);
        }
        assert_eq!(verdicts[0], Verdict::Rejected);
        assert!(verdicts.contains(&Verdict::Restarted), "{verdicts:?}");
        assert!((state.estimate().soc_percent - 52.0).abs() < 4.0);
    }

    #[test]
    fn a_plausible_known_start_value_is_kept_and_sharpened() {
        let mut state = EstimatorState::known(52.0, KNOWN_SOC_SIGMA);
        let step = state.step(&meas(0, 11_952, 300), 80.0);
        assert_eq!(step.verdict, Verdict::Fused);
        assert!(step.estimate.sigma_percent < 3.6);
    }

    #[test]
    fn integration_follows_the_lecture_formula_between_regular_readings() {
        // 8 A über eine Stunde sind 8 Ah, das sind 10 Prozent von 80 Ah. Die Spannung passt zum Verlauf.
        let (mut state, _) = EstimatorState::start(&meas(0, ocv_mv(80.0).round() as i32, 8_000));
        let mut drawn = 0.0;
        for i in 1..=4 {
            let step = state.step(
                &meas(i * 15, ocv_mv(80.0 - 2.5 * i as f64).round() as i32 - 120, 8_000),
                80.0,
            );
            drawn += step.drawn_ah;
        }
        assert!((drawn - 8.0).abs() < 1e-6, "{drawn}");
    }

    #[test]
    fn a_data_gap_raises_the_uncertainty_instead_of_a_false_value() {
        let (mut state, first) = EstimatorState::start(&meas(0, 12_300, 500));
        let before = first.estimate;
        // Drei Tage Pause. Die letzte Messung hatte 500 mA: Die alte Rechnung hätte etwa 45 Prozentpunkte
        // abgezogen. Hier wird nicht integriert (0 Ah), und die Unsicherheit wächst stark.
        let mut gap = state.clone();
        gap.predict(3.0 * 24.0 * 3600.0, 0.5, 80.0);
        assert!(gap.cov[0].sqrt() > 10.0, "{}", gap.cov[0].sqrt());
        let step = state.step(&meas(3 * 24 * 60, 12_300, 500), 80.0);
        assert_eq!(step.drawn_ah, 0.0, "keine erfundene Entnahme über die Lücke");
        // Die Spannung holt den Wert zurück, die Unsicherheit schrumpft wieder.
        assert!(step.estimate.sigma_percent < gap.cov[0].sqrt() as f32);
        assert!(
            (step.estimate.soc_percent - before.soc_percent).abs() < 6.0,
            "{:?}",
            step.estimate
        );
    }

    #[test]
    fn an_outlier_voltage_is_rejected_and_the_state_survives() {
        let (mut state, _) = EstimatorState::start(&meas(0, 12_300, 300));
        for i in 1..6 {
            state.step(&meas(i * 15, 12_300, 300), 80.0);
        }
        let before = state.estimate();
        let step = state.step(&meas(6 * 15, 9_000, 300), 80.0);
        assert_eq!(step.verdict, Verdict::Rejected);
        assert!((step.estimate.soc_percent - before.soc_percent).abs() < 1.0);
        let step = state.step(&meas(7 * 15, 12_300, 300), 80.0);
        assert_eq!(step.verdict, Verdict::Fused);
        assert_eq!(state.rejected, 0);
    }

    #[test]
    fn a_late_reading_changes_nothing() {
        let (mut state, _) = EstimatorState::start(&meas(60, 12_300, 300));
        let before = state.clone();
        let step = state.step(&meas(30, 12_000, 300), 80.0);
        assert_eq!(step.verdict, Verdict::Late);
        assert_eq!(state, before);
    }

    #[test]
    fn the_state_survives_the_database_round_trip() {
        let (mut state, _) = EstimatorState::start(&meas(0, 12_300, 300));
        state.step(&meas(15, 12_290, 300), 80.0);
        let json = serde_json::to_string(&state).unwrap();
        let back: EstimatorState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }

    #[test]
    fn colder_batteries_read_a_lower_voltage_for_the_same_charge() {
        let warm = view_voltage(&Measurement {
            temperature_c: Some(30.0),
            ..meas(0, 12_000, 300)
        });
        let cold = view_voltage(&Measurement {
            temperature_c: Some(0.0),
            ..meas(0, 12_000, 300)
        });
        assert!(cold.soc > warm.soc, "bei Kälte zeigt dieselbe Spannung mehr Ladung");
    }

    #[test]
    fn conservative_value_subtracts_the_uncertainty() {
        assert_eq!(conservative_percent(30.0, Some(4.0)), 26.0);
        assert_eq!(conservative_percent(30.0, None), 30.0);
        assert_eq!(conservative_percent(2.0, Some(9.0)), 0.0);
    }

    #[test]
    fn the_interval_is_clamped_to_the_valid_range() {
        let e = Estimate {
            soc_percent: 97.0,
            sigma_percent: 3.0,
        };
        assert_eq!(e.upper_percent(), 100.0);
        assert!((e.lower_percent() - (97.0 - 5.88)).abs() < 1e-3);
    }

    /// Läuft `steps` Messungen mit 300 mA im Viertelstundentakt. `wobble_mv` schwankt von Messung zu Messung
    /// mit wechselndem Vorzeichen um die Kurve. Gibt den letzten Zustand und die letzte Schätzung zurück.
    fn run_series(steps: i64, wobble_mv: f64) -> (EstimatorState, Estimate) {
        let (mut soc, mut state, mut last) = (80.0, None::<EstimatorState>, None);
        for k in 0..steps {
            soc -= 100.0 * 0.3 * 0.25 / 80.0;
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            let mv = (ocv_mv(soc) - 0.3 * R_INTERNAL_MOHM + sign * wobble_mv).round() as i32;
            let m = meas(k * 15, mv, -300);
            let est = match state.as_mut() {
                None => {
                    let (s, step) = EstimatorState::start(&m);
                    state = Some(s);
                    step.estimate
                }
                Some(s) => s.step(&m, 80.0).estimate,
            };
            last = Some(est);
        }
        (state.unwrap(), last.unwrap())
    }

    #[test]
    fn a_model_that_fits_never_changes_the_stated_margin() {
        let (state, est) = run_series(200, 3.0);
        assert!(state.fit < 1.0, "Selbstkontrolle {:.2}", state.fit);
        let base = (state.cov[0] + SIGMA_SYSTEMATIC.powi(2)).sqrt() as f32;
        assert!((est.sigma_percent - base).abs() < 1e-4, "{est:?} gegen {base}");
    }

    #[test]
    fn a_model_that_keeps_missing_widens_the_margin_by_itself() {
        let (_, clean) = run_series(200, 3.0);
        // Schwankungen von 40 mV (rund 2,4 Punkte) sind weit mehr, als das Rauschmodell erwartet.
        let (state, wild) = run_series(200, 40.0);
        assert!(state.fit > 2.0, "Selbstkontrolle {:.2}", state.fit);
        assert!(
            wild.sigma_percent > clean.sigma_percent * 1.4,
            "verrauscht {:?} gegen sauber {:?}",
            wild,
            clean
        );
    }

    // ---------------------------------------------------------------------------------------------------------
    // Prüfung gegen eine simulierte Welt mit bekannter Wahrheit: Stimmt die angegebene Fehlerspanne?
    // ---------------------------------------------------------------------------------------------------------

    /// Kleiner deterministischer Zufall (xorshift), damit der Test immer gleich läuft.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }

        fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }

        fn normal(&mut self) -> f64 {
            let (u1, u2) = (self.next().max(1e-12), self.next());
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        }
    }

    struct Outcome {
        covered: usize,
        total: usize,
        squared_error: f64,
        baseline_squared_error: f64,
        margin_sum: f64,
    }

    /// Eine simulierte Batterie mit allen Fehlern, die das Filter nicht kennt. `severity` skaliert diese Fehler:
    /// 1 ist eine gewöhnliche Batterie, 2 eine alte, schlecht kalibrierte mit stark abweichender Kennlinie.
    fn simulate(seed: u64, with_gaps: bool, severity: f64) -> Outcome {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let nominal = 80.0;
        let true_capacity = nominal * (1.0 + rng.uniform(-0.06, 0.06) * severity);
        let sensor_gain = 1.0 + rng.uniform(-0.02, 0.02) * severity;
        let sensor_bias_a = rng.uniform(-0.010, 0.010) * severity;
        // Die Kennlinie der wirklichen Batterie weicht in Lage und Neigung von der Standardkurve ab.
        let curve_offset = rng.uniform(-2.5, 2.5) * severity;
        let curve_tilt = rng.uniform(-2.0, 2.0) * severity;
        let r_true_mohm = rng.uniform(5.0, 35.0) * severity;
        // Gemessen an den echten Reihen sind es rund 3 mV. Die simulierte Welt nimmt das Doppelte.
        let voltage_noise_mv = 6.0 * severity;
        let temp_coeff = rng.uniform(0.8, 1.6);
        let mut temperature = rng.uniform(5.0, 25.0);
        let mut soc_true = rng.uniform(60.0, 100.0);

        let mut at = t0();
        let mut state: Option<EstimatorState> = None;
        let mut baseline: Option<(f64, Option<f64>)> = None; // (Ladezustand, letzter Strom in A)
        let mut out = Outcome {
            covered: 0,
            total: 0,
            squared_error: 0.0,
            baseline_squared_error: 0.0,
            margin_sum: 0.0,
        };
        let mut step_index = 0;
        while soc_true > 4.0 && step_index < 1_400 {
            let hour = at.timestamp() as f64 / 3600.0;
            let load_a = 0.30 * (1.0 + 0.3 * (hour / 24.0 * std::f64::consts::TAU).sin()) * (1.0 + 0.05 * rng.normal());
            let dt_h = if with_gaps && rng.next() < 0.004 {
                rng.uniform(2.0, 40.0)
            } else {
                0.25
            };
            // Die Wahrheit entlädt mit der wirklichen Last, auch während einer Lücke.
            soc_true = (soc_true - 100.0 * load_a * dt_h / true_capacity).max(0.0);
            at += Duration::milliseconds((dt_h * 3_600_000.0) as i64);
            temperature = (temperature + 0.2 * rng.normal()).clamp(0.0, 30.0);
            step_index += 1;

            let shown = soc_true + curve_offset + curve_tilt * (soc_true - 50.0) / 50.0;
            let ocv = ocv_mv(shown) + temp_coeff * (temperature - OCV_REFERENCE_TEMP_C);
            let voltage = ocv - load_a * r_true_mohm + voltage_noise_mv * rng.normal();
            let current_measured = (load_a * sensor_gain + sensor_bias_a + 0.003 * rng.normal()).max(0.0);
            let m = Measurement {
                at,
                voltage_mv: voltage.round() as i32,
                current_ma: -((current_measured * 1000.0).round() as i32),
                temperature_c: Some(temperature as f32),
            };

            let est = match state.as_mut() {
                None => {
                    let (s, step) = EstimatorState::start(&m);
                    state = Some(s);
                    step.estimate
                }
                Some(s) => s.step(&m, nominal).estimate,
            };

            // Bisheriges Verfahren: linearer Start aus der Spannung, danach reine Integration (Lücken eingeschlossen).
            let linear = |v: f64| ((v - 10_500.0) / 2_200.0 * 100.0).clamp(0.0, 100.0);
            let base = match baseline {
                None => linear(voltage),
                Some((soc, prev)) => {
                    let prev_a = prev.unwrap_or(current_measured);
                    let ah = 0.5 * (prev_a + current_measured) * dt_h;
                    (soc - 100.0 * ah / nominal).max(0.0)
                }
            };
            baseline = Some((base, Some(current_measured)));

            if step_index > 4 {
                let error = f64::from(est.soc_percent) - soc_true;
                out.total += 1;
                if error.abs() <= f64::from(est.margin_percent()) {
                    out.covered += 1;
                }
                out.squared_error += error * error;
                out.baseline_squared_error += (base - soc_true).powi(2);
                out.margin_sum += f64::from(est.margin_percent());
            }
        }
        out
    }

    /// Zusammengefasste Ergebnisse vieler simulierter Batterien.
    struct Summary {
        coverage: f64,
        rmse: f64,
        baseline_rmse: f64,
        mean_margin: f64,
    }

    fn pooled(with_gaps: bool, severity: f64) -> Summary {
        let (mut covered, mut total, mut se, mut base, mut margin) = (0usize, 0usize, 0.0, 0.0, 0.0);
        for seed in 1..=80 {
            let o = simulate(seed, with_gaps, severity);
            covered += o.covered;
            total += o.total;
            se += o.squared_error;
            base += o.baseline_squared_error;
            margin += o.margin_sum;
        }
        let n = total as f64;
        Summary {
            coverage: covered as f64 / n,
            rmse: (se / n).sqrt(),
            baseline_rmse: (base / n).sqrt(),
            mean_margin: margin / n,
        }
    }

    #[test]
    fn the_stated_95_percent_interval_covers_at_least_95_percent_of_the_time() {
        for (gaps, severity) in [(false, 1.0), (true, 1.0), (false, 2.0), (true, 2.0)] {
            let s = pooled(gaps, severity);
            println!(
                "Lücken {gaps}, Schwere {severity}: Abdeckung {:.3}, Fehler {:.2}, Spanne {:.2}, altes Verfahren {:.2}",
                s.coverage, s.rmse, s.mean_margin, s.baseline_rmse
            );
            assert!(
                s.coverage >= 0.95,
                "Lücken {gaps}, Schwere {severity}: Abdeckung {:.3}, Fehler {:.2}, Spanne {:.2}",
                s.coverage,
                s.rmse,
                s.mean_margin
            );
        }
    }

    #[test]
    fn the_interval_is_useful_and_not_just_very_wide() {
        let s = pooled(true, 1.0);
        assert!(
            s.mean_margin < 7.0,
            "mittlere Spanne {:.2} Prozentpunkte",
            s.mean_margin
        );
        assert!(s.rmse < 2.5, "mittlerer Fehler {:.2} Prozentpunkte", s.rmse);
    }

    #[test]
    fn the_filter_beats_the_old_method_clearly() {
        for severity in [1.0, 2.0] {
            let s = pooled(true, severity);
            assert!(
                s.rmse < s.baseline_rmse * 0.5,
                "Schwere {severity}: neu {:.2} gegen alt {:.2}",
                s.rmse,
                s.baseline_rmse
            );
        }
    }
}
