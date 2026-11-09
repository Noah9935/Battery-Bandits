//! Auswertungen über die Flotte für die Zusatzfunktionen (rein, ohne Browser).

use shared::{Device, DeviceStatus, DomainEvent};

/// Grenzen der Ladebereiche der Verteilung in Prozent (obere Grenze, exklusiv).
pub const BUCKET_LABELS: [&str; 5] = ["unter 20 %", "20 bis 40 %", "40 bis 60 %", "60 bis 80 %", "ab 80 %"];

/// Anzahl Geräte je Ladebereich. Geräte ohne bekannten Ladezustand zählen nicht mit.
pub fn histogram(devices: &[Device]) -> [usize; 5] {
    let mut buckets = [0; 5];
    for soc in devices.iter().filter_map(|d| d.soc_percent) {
        let index = ((soc / 20.0).floor() as isize).clamp(0, 4) as usize;
        buckets[index] += 1;
    }
    buckets
}

#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub id: String,
    pub name: Option<String>,
    pub hours: f32,
    pub status: DeviceStatus,
}

/// Die `n` Geräte mit der kürzesten prognostizierten Restlaufzeit. Offline-Geräte haben keine belastbare Prognose.
pub fn due_next(devices: &[Device], n: usize) -> Vec<Due> {
    let mut list: Vec<Due> = devices
        .iter()
        .filter(|d| d.status != DeviceStatus::Offline)
        .filter_map(|d| {
            Some(Due {
                id: d.id.clone(),
                name: d.name.clone(),
                hours: d.forecast_hours_remaining?,
                status: d.status,
            })
        })
        .collect();
    list.sort_by(|a, b| a.hours.total_cmp(&b.hours).then_with(|| a.id.cmp(&b.id)));
    list.truncate(n);
    list
}

/// Pfad einer kleinen Trendlinie in einem Kasten der Größe `width` mal `height`. Die Höhe skaliert zwischen
/// dem kleinsten und größten Wert (mindestens 5 Prozentpunkte Spanne, damit flache Kurven flach bleiben).
/// Weniger als zwei Punkte ergeben einen leeren Pfad.
pub fn sparkline_path(points: &[f32], width: f64, height: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let min = points.iter().copied().fold(f32::INFINITY, f32::min);
    let max = points.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = f64::from((max - min).max(5.0));
    let mid = f64::from(min + max) / 2.0;
    let step = width / (points.len() - 1) as f64;
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let y = height / 2.0 - (f64::from(*p) - mid) / span * (height - 2.0);
            format!("{}{:.1} {:.1}", if i == 0 { "M" } else { " L" }, i as f64 * step, y)
        })
        .collect()
}

/// Kurze Prognose für Tabelle und Geräteseite: Solange der Ladestand über dem kritischen Wert liegt, die Zeit
/// bis dahin, danach die Zeit bis leer. `None`, wenn es keine belastbare Entladerate gibt.
pub fn forecast_short(device: &Device, critical_pct: f32) -> Option<String> {
    let rate = device.discharge_rate_pct_per_hour.filter(|r| *r > 0.0)?;
    let soc = device.soc_percent?;
    let hours = device.forecast_hours_remaining?;
    Some(if soc > critical_pct {
        format!(
            "{} bis kritisch",
            crate::format::duration_hours((soc - critical_pct) / rate)
        )
    } else {
        format!("{} bis leer", crate::format::duration_hours(hours))
    })
}

/// Strichlänge und Umfang eines Rings für `percent` Prozent bei dem Radius `radius`.
pub fn ring_dash(percent: f32, radius: f64) -> (f64, f64) {
    let circumference = 2.0 * std::f64::consts::PI * radius;
    (
        circumference * f64::from(percent.clamp(0.0, 100.0) / 100.0),
        circumference,
    )
}

fn status_text(status: DeviceStatus) -> &'static str {
    match status {
        DeviceStatus::Normal => "in Ordnung",
        DeviceStatus::Warning => "im Warnbereich",
        DeviceStatus::Critical => "kritisch",
        DeviceStatus::Offline => "offline",
    }
}

/// Satz für das Aktivitätsprotokoll. Einzelne Messwerte sind zu häufig und ergeben keinen Eintrag.
pub fn describe_event(event: &DomainEvent) -> Option<String> {
    match event {
        DomainEvent::ReadingSaved { .. } => None,
        DomainEvent::DeviceStatusChanged {
            device_id,
            old_status,
            new_status,
            ..
        } => Some(if old_status.needs_attention() && !new_status.needs_attention() {
            format!(
                "{device_id} ist wieder {} (war {}).",
                status_text(*new_status),
                status_text(*old_status)
            )
        } else {
            format!("{device_id} ist jetzt {}.", status_text(*new_status))
        }),
        DomainEvent::AlertCreated { device_id, kind, .. } => {
            Some(format!("Meldung für {device_id}: {}.", kind.label()))
        }
        DomainEvent::BatteryReplaced { device_id, .. } => Some(format!("Batteriewechsel bei {device_id} bestätigt.")),
        DomainEvent::AnomalyDetected { .. } | DomainEvent::TaskChanged { .. } => None,
    }
}

/// Wird das Gerät durch dieses Ereignis kritisch? (für die Browser-Benachrichtigung)
pub fn became_critical(event: &DomainEvent) -> Option<&str> {
    match event {
        DomainEvent::DeviceStatusChanged {
            device_id,
            new_status: DeviceStatus::Critical,
            ..
        } => Some(device_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use shared::{AlertKind, DataSource};

    fn device(id: &str, status: DeviceStatus, soc: Option<f32>, hours: Option<f32>) -> Device {
        Device {
            id: id.into(),
            name: None,
            status,
            soc_percent: soc,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: None,
            lon: None,
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: hours,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn histogram_counts_each_band_and_ignores_unknown() {
        let socs = [0.0, 19.9, 20.0, 45.0, 79.9, 80.0, 100.0];
        let devices: Vec<Device> = socs
            .iter()
            .enumerate()
            .map(|(i, s)| device(&format!("D{i}"), DeviceStatus::Normal, Some(*s), None))
            .collect();
        let mut all = devices;
        all.push(device("X", DeviceStatus::Normal, None, None));
        assert_eq!(histogram(&all), [2, 1, 1, 1, 2]);
        assert_eq!(histogram(&[]), [0; 5]);
    }

    #[test]
    fn due_next_sorts_by_remaining_time_and_skips_offline_and_unknown() {
        let devices = vec![
            device("A", DeviceStatus::Normal, Some(80.0), Some(50.0)),
            device("B", DeviceStatus::Critical, Some(10.0), Some(3.0)),
            device("C", DeviceStatus::Offline, Some(10.0), Some(1.0)),
            device("D", DeviceStatus::Warning, Some(30.0), None),
            device("E", DeviceStatus::Warning, Some(30.0), Some(12.0)),
        ];
        let due = due_next(&devices, 2);
        assert_eq!(due.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["B", "E"]);
        assert_eq!(due[0].hours, 3.0);
    }

    #[test]
    fn sparkline_stays_inside_the_box_and_needs_two_points() {
        assert_eq!(sparkline_path(&[50.0], 60.0, 16.0), "");
        let path = sparkline_path(&[90.0, 70.0, 50.0, 30.0], 60.0, 16.0);
        let numbers: Vec<f64> = path
            .replace(['M', 'L'], " ")
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(numbers.len(), 8);
        for pair in numbers.chunks(2) {
            assert!(
                (0.0..=60.0).contains(&pair[0]) && (0.0..=16.0).contains(&pair[1]),
                "{pair:?}"
            );
        }
        assert!(numbers[1] < numbers[7], "fallende Werte laufen nach unten");
    }

    #[test]
    fn flat_series_stays_flat() {
        let path = sparkline_path(&[50.0, 50.0, 50.0], 60.0, 16.0);
        let ys: Vec<&str> = path
            .replace(['M', 'L'], " ")
            .split_whitespace()
            .skip(1)
            .step_by(2)
            .map(|_| "")
            .collect();
        assert_eq!(ys.len(), 3);
        assert!(path.matches("8.0").count() >= 3, "{path}");
    }

    fn with_forecast(soc: Option<f32>, rate: Option<f32>, hours: Option<f32>) -> Device {
        Device {
            discharge_rate_pct_per_hour: rate,
            ..device("X", DeviceStatus::Normal, soc, hours)
        }
    }

    #[test]
    fn short_forecast_counts_to_critical_first_then_to_empty() {
        let above = with_forecast(Some(60.0), Some(2.0), Some(30.0));
        assert_eq!(
            forecast_short(&above, 20.0).unwrap(),
            format!("{} bis kritisch", crate::format::duration_hours(20.0))
        );
        let below = with_forecast(Some(15.0), Some(2.0), Some(7.5));
        assert_eq!(
            forecast_short(&below, 20.0).unwrap(),
            format!("{} bis leer", crate::format::duration_hours(7.5))
        );
    }

    #[test]
    fn short_forecast_needs_a_usable_rate() {
        assert_eq!(forecast_short(&with_forecast(Some(60.0), None, Some(30.0)), 20.0), None);
        assert_eq!(
            forecast_short(&with_forecast(Some(60.0), Some(0.0), Some(30.0)), 20.0),
            None
        );
        assert_eq!(
            forecast_short(&with_forecast(Some(60.0), Some(-1.0), Some(30.0)), 20.0),
            None
        );
        assert_eq!(forecast_short(&with_forecast(None, Some(2.0), Some(30.0)), 20.0), None);
        assert_eq!(forecast_short(&with_forecast(Some(60.0), Some(2.0), None), 20.0), None);
    }

    #[test]
    fn ring_dash_is_proportional_and_clamped() {
        let (dash, whole) = ring_dash(50.0, 10.0);
        assert!((dash - whole / 2.0).abs() < 1e-9);
        assert_eq!(ring_dash(150.0, 10.0).0, whole);
        assert_eq!(ring_dash(-5.0, 10.0).0, 0.0);
    }

    #[test]
    fn events_are_described_in_german() {
        let changed = |old, new| DomainEvent::DeviceStatusChanged {
            device_id: "SIM03".into(),
            old_status: old,
            new_status: new,
            soc_percent: None,
        };
        assert_eq!(
            describe_event(&changed(DeviceStatus::Normal, DeviceStatus::Critical)).unwrap(),
            "SIM03 ist jetzt kritisch."
        );
        assert_eq!(
            describe_event(&changed(DeviceStatus::Offline, DeviceStatus::Normal)).unwrap(),
            "SIM03 ist wieder in Ordnung (war offline)."
        );
        let alert = DomainEvent::AlertCreated {
            alert_id: 1,
            device_id: "A".into(),
            kind: AlertKind::Offline,
        };
        assert_eq!(describe_event(&alert).unwrap(), "Meldung für A: Keine Messwerte mehr.");
        let replaced = DomainEvent::BatteryReplaced {
            device_id: "A".into(),
            at: DateTime::from_timestamp(0, 0).unwrap(),
        };
        assert!(describe_event(&replaced).unwrap().contains("Batteriewechsel"));
        let reading = DomainEvent::ReadingSaved {
            device_id: "A".into(),
            time: DateTime::from_timestamp(0, 0).unwrap(),
            soc_percent: 1.0,
        };
        assert_eq!(describe_event(&reading), None);
    }

    #[test]
    fn detects_the_critical_transition() {
        let to = |new| DomainEvent::DeviceStatusChanged {
            device_id: "A".into(),
            old_status: DeviceStatus::Normal,
            new_status: new,
            soc_percent: None,
        };
        assert_eq!(became_critical(&to(DeviceStatus::Critical)), Some("A"));
        assert_eq!(became_critical(&to(DeviceStatus::Warning)), None);
    }
}
