//! Prüfung des Schätzers an echten Messreihen (nicht Teil des normalen Testlaufs).
//!
//! Aufruf (im Dev-Container, Datei im Arbeitsordner):
//!
//! ```text
//! REPLAY_CSV=/work/real.csv cargo test -p shared --test replay_real -- --ignored --nocapture
//! ```
//!
//! Die CSV hat je Zeile `Gerät,Zeit (ISO 8601),Spannung in mV,Strom in mA,Temperatur in °C (oder leer)`, sortiert
//! nach Gerät und Zeit. Ohne Wahrheit zum Vergleich prüft das, was sich ohne sie prüfen lässt:
//!
//! * **NIS** (normierte Abweichung zum Quadrat) im Mittel nahe 1: Das Rauschmodell passt zu den Daten.
//! * **Anteil der Abweisungen** klein: Das Filter hält echte Messwerte nicht für Ausreißer.
//! * **Verlauf** des Ladezustands mit Spanne neben dem Wert, den die Spannung allein ergäbe.

use chrono::{DateTime, Utc};
use shared::estimator::{EstimatorState, Measurement, Verdict, soc_from_ocv};

struct Row {
    device: String,
    m: Measurement,
}

fn read_rows(path: &str) -> Vec<Row> {
    std::fs::read_to_string(path)
        .expect("REPLAY_CSV lesen")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split(',').map(|v| v.trim().trim_matches('"')).collect();
            Row {
                device: f[0].to_owned(),
                m: Measurement {
                    at: DateTime::parse_from_rfc3339(f[1]).expect("Zeit").with_timezone(&Utc),
                    voltage_mv: f[2].parse().expect("Spannung"),
                    current_ma: f[3].parse().expect("Strom"),
                    temperature_c: f.get(4).and_then(|t| t.parse().ok()),
                },
            }
        })
        .collect()
}

#[test]
#[ignore = "braucht echte Messreihen (REPLAY_CSV)"]
fn replay_real_series() {
    let Ok(path) = std::env::var("REPLAY_CSV") else {
        println!("REPLAY_CSV ist nicht gesetzt");
        return;
    };
    let rows = read_rows(&path);
    let mut devices: Vec<String> = rows.iter().map(|r| r.device.clone()).collect();
    devices.dedup();

    for device in devices {
        let series: Vec<&Row> = rows.iter().filter(|r| r.device == device).collect();
        let mut state: Option<EstimatorState> = None;
        let (mut nis_sum, mut nis_n, mut nis_over4) = (0.0, 0usize, 0usize);
        let (mut rejected, mut restarted) = (0usize, 0usize);
        let mut drawn = 0.0;
        println!("== {device}: {} Messungen", series.len());
        for (i, row) in series.iter().enumerate() {
            let step = match state.as_mut() {
                None => {
                    let (s, step) = EstimatorState::start(&row.m);
                    state = Some(s);
                    step
                }
                Some(s) => s.step(&row.m, 80.0),
            };
            drawn += step.drawn_ah;
            if let Some(nis) = step.nis {
                nis_sum += nis;
                nis_n += 1;
                if nis > 4.0 {
                    nis_over4 += 1;
                }
            }
            match step.verdict {
                Verdict::Rejected => rejected += 1,
                Verdict::Restarted => restarted += 1,
                _ => {}
            }
            if i % (series.len() / 12).max(1) == 0 || i + 1 == series.len() {
                let current_a = f64::from(row.m.current_ma).abs() / 1000.0;
                let ocv = f64::from(row.m.voltage_mv) + current_a * 15.0;
                let by_voltage = soc_from_ocv(ocv).0;
                println!(
                    "  {}  {:5} mV  Schätzung {:5.1} % ± {:4.1} (95 %: ± {:4.1})  nur Spannung {:5.1} %  entnommen {:5.1} Ah",
                    row.m.at.format("%d.%m. %H:%M"),
                    row.m.voltage_mv,
                    step.estimate.soc_percent,
                    step.estimate.sigma_percent,
                    step.estimate.margin_percent(),
                    by_voltage,
                    drawn
                );
            }
        }
        if nis_n > 0 {
            println!(
                "  NIS im Mittel {:.2} (Ziel etwa 1), davon über 4: {:.1} % (Ziel unter 5 %), abgewiesen {}, neu gestartet {}",
                nis_sum / nis_n as f64,
                100.0 * nis_over4 as f64 / nis_n as f64,
                rejected,
                restarted
            );
        }
    }
}
